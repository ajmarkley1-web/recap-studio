use super::{error_for_status, ChatRequest, ChatResponse, LlmClient, ModelInfo, Part, Role};
use anyhow::{anyhow, Result};
use serde_json::{json, Value};

/// Known multimodal families available through Ollama.
const VISION_FAMILIES: &[&str] = &[
    "llava", "bakllava", "llama3.2-vision", "llama3.2vision", "minicpm-v", "moondream",
    "qwen2-vl", "qwen2.5vl", "qwen2.5-vl", "qwen3-vl", "gemma3", "mistral-small3",
    "granite3.2-vision", "llama4", "internvl",
];

fn supports_vision(id: &str, families: &[String]) -> bool {
    let m = id.to_ascii_lowercase();
    if VISION_FAMILIES.iter().any(|f| m.contains(f)) {
        return true;
    }
    // Ollama reports `families: ["llama", "clip"]` (or "mllama") for vision models.
    families
        .iter()
        .any(|f| matches!(f.to_ascii_lowercase().as_str(), "clip" | "mllama" | "vision"))
}

fn rank_of(id: &str, vision: bool) -> i32 {
    let mut rank = if vision { 50 } else { 10 };
    let m = id.to_ascii_lowercase();
    if m.contains("latest") {
        rank += 2;
    }
    // Bigger parameter counts usually read panels better.
    for (needle, bonus) in [("90b", 20), ("72b", 18), ("34b", 14), ("13b", 8), ("11b", 7), ("7b", 4)] {
        if m.contains(needle) {
            rank += bonus;
            break;
        }
    }
    rank
}

pub async fn list_models(client: &LlmClient) -> Result<Vec<ModelInfo>> {
    let url = format!("{}/api/tags", client.creds.base_url);
    let resp = match client.http().get(&url).send().await {
        Ok(r) => r,
        Err(err) => {
            return Err(anyhow!(
                "Could not reach Ollama at {}. Is it running? Start it with `ollama serve`. ({err})",
                client.creds.base_url
            ))
        }
    };
    if !resp.status().is_success() {
        return Err(error_for_status("Ollama", resp).await);
    }
    let body: Value = resp.json().await?;
    let mut out = Vec::new();
    if let Some(list) = body.get("models").and_then(|m| m.as_array()) {
        for item in list {
            let Some(name) = item
                .get("name")
                .or_else(|| item.get("model"))
                .and_then(|n| n.as_str())
            else {
                continue;
            };
            let families: Vec<String> = item
                .pointer("/details/families")
                .and_then(|f| f.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|v| v.as_str().map(str::to_string))
                        .collect()
                })
                .unwrap_or_default();
            let vision = supports_vision(name, &families);
            let size = item
                .pointer("/details/parameter_size")
                .and_then(|p| p.as_str())
                .unwrap_or("");
            let label = if size.is_empty() {
                name.to_string()
            } else {
                format!("{name} ({size})")
            };
            out.push(ModelInfo {
                id: name.to_string(),
                label,
                vision,
                context_tokens: None,
                family: "ollama".into(),
                rank: rank_of(name, vision),
            });
        }
    }
    Ok(out)
}

pub async fn chat(client: &LlmClient, req: &ChatRequest) -> Result<ChatResponse> {
    let mut messages: Vec<Value> = Vec::new();
    if let Some(system) = &req.system {
        messages.push(json!({ "role": "system", "content": system }));
    }
    for msg in &req.messages {
        let role = match msg.role {
            Role::User => "user",
            Role::Assistant => "assistant",
        };
        // Ollama attaches images to the message, not to individual content parts.
        let mut text = String::new();
        let mut images: Vec<String> = Vec::new();
        for part in &msg.parts {
            match part {
                Part::Text(t) => {
                    if !text.is_empty() {
                        text.push_str("\n\n");
                    }
                    text.push_str(t);
                }
                Part::Image { data_b64, .. } => images.push(data_b64.clone()),
            }
        }
        let mut entry = json!({ "role": role, "content": text });
        if !images.is_empty() {
            entry["images"] = json!(images);
        }
        messages.push(entry);
    }

    let mut body = json!({
        "model": req.model,
        "messages": messages,
        "stream": false,
        "options": {
            "temperature": req.temperature,
            "num_predict": req.max_tokens,
        }
    });
    if req.json_mode {
        body["format"] = json!("json");
    }

    let url = format!("{}/api/chat", client.creds.base_url);
    let resp = match client.http().post(&url).json(&body).send().await {
        Ok(r) => r,
        Err(err) => {
            return Err(anyhow!(
                "Could not reach Ollama at {}. Is it running? ({err})",
                client.creds.base_url
            ))
        }
    };
    if !resp.status().is_success() {
        return Err(error_for_status("Ollama", resp).await);
    }
    let value: Value = resp.json().await?;

    let text = value
        .pointer("/message/content")
        .and_then(|c| c.as_str())
        .unwrap_or_default()
        .to_string();

    Ok(ChatResponse {
        text,
        input_tokens: value
            .get("prompt_eval_count")
            .and_then(|v| v.as_u64())
            .unwrap_or(0),
        output_tokens: value.get("eval_count").and_then(|v| v.as_u64()).unwrap_or(0),
        model: req.model.clone(),
    })
}
