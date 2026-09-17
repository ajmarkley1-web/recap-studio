use super::{error_for_status, ChatRequest, ChatResponse, LlmClient, ModelInfo, Part, Role};
use anyhow::{anyhow, Result};
use serde_json::{json, Value};

fn supports_vision(id: &str) -> bool {
    let m = id.to_ascii_lowercase();
    if m.contains("embedding") || m.contains("aqa") || m.contains("imagen") || m.contains("tts") {
        return false;
    }
    // Every Gemini 1.5+ generative model is multimodal.
    m.contains("gemini")
}

fn rank_of(id: &str) -> i32 {
    let m = id.to_ascii_lowercase();
    let mut rank = 0;
    for (needle, score) in [
        ("gemini-3", 100),
        ("gemini-2.5", 90),
        ("gemini-2.0", 80),
        ("gemini-1.5", 60),
    ] {
        if m.contains(needle) {
            rank += score;
            break;
        }
    }
    if m.contains("pro") {
        rank += 12;
    } else if m.contains("flash") {
        rank += 8;
    }
    if m.contains("lite") || m.contains("8b") {
        rank -= 6;
    }
    if m.contains("exp") || m.contains("preview") {
        rank -= 4;
    }
    rank
}

/// Gemini model names come back as `models/gemini-2.5-pro`. The generateContent
/// path wants the same `models/...` form, but the picker shows the short id.
fn short_id(name: &str) -> &str {
    name.strip_prefix("models/").unwrap_or(name)
}

pub async fn list_models(client: &LlmClient) -> Result<Vec<ModelInfo>> {
    let mut out = Vec::new();
    let mut page_token: Option<String> = None;

    loop {
        let mut url = format!(
            "{}/models?key={}&pageSize=200",
            client.creds.base_url, client.creds.api_key
        );
        if let Some(token) = &page_token {
            url.push_str(&format!("&pageToken={token}"));
        }
        let resp = client.http().get(&url).send().await?;
        if !resp.status().is_success() {
            return Err(error_for_status("Gemini", resp).await);
        }
        let body: Value = resp.json().await?;

        if let Some(list) = body.get("models").and_then(|m| m.as_array()) {
            for item in list {
                let Some(name) = item.get("name").and_then(|n| n.as_str()) else {
                    continue;
                };
                // Only keep models that can actually answer a generateContent call.
                let supports_generate = item
                    .get("supportedGenerationMethods")
                    .and_then(|m| m.as_array())
                    .map(|arr| arr.iter().any(|v| v.as_str() == Some("generateContent")))
                    .unwrap_or(false);
                if !supports_generate {
                    continue;
                }
                let id = short_id(name).to_string();
                let label = item
                    .get("displayName")
                    .and_then(|d| d.as_str())
                    .unwrap_or(&id)
                    .to_string();
                out.push(ModelInfo {
                    vision: supports_vision(&id),
                    rank: rank_of(&id),
                    context_tokens: item.get("inputTokenLimit").and_then(|v| v.as_u64()),
                    family: "gemini".into(),
                    label,
                    id,
                });
            }
        }

        page_token = body
            .get("nextPageToken")
            .and_then(|t| t.as_str())
            .map(str::to_string);
        if page_token.is_none() {
            break;
        }
    }
    Ok(out)
}

pub async fn chat(client: &LlmClient, req: &ChatRequest) -> Result<ChatResponse> {
    let contents: Vec<Value> = req
        .messages
        .iter()
        .map(|msg| {
            let role = match msg.role {
                Role::User => "user",
                Role::Assistant => "model",
            };
            let parts: Vec<Value> = msg
                .parts
                .iter()
                .map(|p| match p {
                    Part::Text(t) => json!({ "text": t }),
                    Part::Image { mime, data_b64 } => json!({
                        "inline_data": { "mime_type": mime, "data": data_b64 }
                    }),
                })
                .collect();
            json!({ "role": role, "parts": parts })
        })
        .collect();

    let mut generation_config = json!({
        "maxOutputTokens": req.max_tokens,
        "temperature": req.temperature,
    });
    if req.json_mode {
        generation_config["responseMimeType"] = json!("application/json");
    }

    let mut body = json!({
        "contents": contents,
        "generationConfig": generation_config,
        // Recaps routinely describe fights and deaths. Without this, Gemini
        // blocks a large share of legitimate action chapters.
        "safetySettings": [
            { "category": "HARM_CATEGORY_HARASSMENT", "threshold": "BLOCK_ONLY_HIGH" },
            { "category": "HARM_CATEGORY_HATE_SPEECH", "threshold": "BLOCK_ONLY_HIGH" },
            { "category": "HARM_CATEGORY_SEXUALLY_EXPLICIT", "threshold": "BLOCK_ONLY_HIGH" },
            { "category": "HARM_CATEGORY_DANGEROUS_CONTENT", "threshold": "BLOCK_ONLY_HIGH" }
        ]
    });
    if let Some(system) = &req.system {
        body["system_instruction"] = json!({ "parts": [{ "text": system }] });
    }

    let url = format!(
        "{}/models/{}:generateContent?key={}",
        client.creds.base_url,
        short_id(&req.model),
        client.creds.api_key
    );
    let resp = client.http().post(&url).json(&body).send().await?;
    if !resp.status().is_success() {
        return Err(error_for_status("Gemini", resp).await);
    }
    let value: Value = resp.json().await?;

    if let Some(reason) = value.pointer("/promptFeedback/blockReason").and_then(|r| r.as_str()) {
        return Err(anyhow!(
            "Gemini blocked the request ({reason}). Try a different model or reduce the pages per batch."
        ));
    }

    let text = value
        .pointer("/candidates/0/content/parts")
        .and_then(|p| p.as_array())
        .map(|parts| {
            parts
                .iter()
                .filter_map(|p| p.get("text").and_then(|t| t.as_str()))
                .collect::<Vec<_>>()
                .join("")
        })
        .unwrap_or_default();

    if text.is_empty() {
        if let Some(finish) = value
            .pointer("/candidates/0/finishReason")
            .and_then(|r| r.as_str())
        {
            if finish != "STOP" {
                return Err(anyhow!(
                    "Gemini returned no text (finishReason: {finish}). \
                     SAFETY usually means the pages tripped a filter; MAX_TOKENS means the response limit was too low."
                ));
            }
        }
    }

    Ok(ChatResponse {
        text,
        input_tokens: value
            .pointer("/usageMetadata/promptTokenCount")
            .and_then(|v| v.as_u64())
            .unwrap_or(0),
        output_tokens: value
            .pointer("/usageMetadata/candidatesTokenCount")
            .and_then(|v| v.as_u64())
            .unwrap_or(0),
        model: req.model.clone(),
    })
}
