use super::{error_for_status, ChatRequest, ChatResponse, LlmClient, ModelInfo, Part, Role};
use anyhow::Result;
use serde_json::{json, Value};

/// Models in the o-series and GPT-5 family use `max_completion_tokens` and reject
/// a custom `temperature`. Everything else takes the classic parameters.
fn is_reasoning_model(model: &str) -> bool {
    let m = model.to_ascii_lowercase();
    m.starts_with("o1") || m.starts_with("o3") || m.starts_with("o4") || m.starts_with("gpt-5")
}

fn supports_vision(model: &str) -> bool {
    let m = model.to_ascii_lowercase();
    if m.contains("embedding")
        || m.contains("whisper")
        || m.contains("tts")
        || m.contains("dall-e")
        || m.contains("moderation")
        || m.contains("audio")
        || m.contains("realtime")
        || m.contains("davinci")
        || m.contains("babbage")
    {
        return false;
    }
    m.starts_with("gpt-4o")
        || m.starts_with("gpt-4.1")
        || m.starts_with("gpt-4-turbo")
        || m.starts_with("gpt-5")
        || m.starts_with("chatgpt-4o")
        || m.starts_with("o1")
        || m.starts_with("o3")
        || m.starts_with("o4")
}

fn is_chat_model(model: &str) -> bool {
    let m = model.to_ascii_lowercase();
    if m.contains("embedding")
        || m.contains("whisper")
        || m.contains("tts")
        || m.contains("dall-e")
        || m.contains("moderation")
        || m.contains("babbage")
        || m.contains("davinci")
        || m.contains("realtime")
        || m.contains("transcribe")
        || m.contains("image")
        || m.contains("codex")
    {
        return false;
    }
    m.starts_with("gpt-") || m.starts_with("o1") || m.starts_with("o3") || m.starts_with("o4") || m.starts_with("chatgpt")
}

fn rank_of(model: &str) -> i32 {
    let m = model.to_ascii_lowercase();
    let mut rank = 0;
    if m.starts_with("gpt-5") {
        rank += 100;
    } else if m.starts_with("o3") || m.starts_with("o4") {
        rank += 90;
    } else if m.starts_with("gpt-4.1") {
        rank += 80;
    } else if m.starts_with("gpt-4o") {
        rank += 70;
    } else if m.starts_with("o1") {
        rank += 60;
    } else if m.starts_with("gpt-4") {
        rank += 40;
    } else {
        rank += 10;
    }
    if m.contains("mini") || m.contains("nano") {
        rank -= 5;
    }
    if m.chars().filter(|c| c.is_ascii_digit()).count() > 6 {
        // dated snapshots like gpt-4o-2024-08-06 sort below the floating alias
        rank -= 3;
    }
    if supports_vision(&m) {
        rank += 15;
    }
    rank
}

pub async fn list_models(client: &LlmClient) -> Result<Vec<ModelInfo>> {
    let url = format!("{}/models", client.creds.base_url);
    let resp = client
        .http()
        .get(&url)
        .bearer_auth(&client.creds.api_key)
        .send()
        .await?;
    if !resp.status().is_success() {
        return Err(error_for_status("OpenAI", resp).await);
    }
    let body: Value = resp.json().await?;
    let mut out = Vec::new();
    if let Some(list) = body.get("data").and_then(|d| d.as_array()) {
        for item in list {
            let Some(id) = item.get("id").and_then(|i| i.as_str()) else {
                continue;
            };
            if !is_chat_model(id) {
                continue;
            }
            out.push(ModelInfo {
                id: id.to_string(),
                label: id.to_string(),
                vision: supports_vision(id),
                context_tokens: None,
                family: "openai".into(),
                rank: rank_of(id),
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
        // A message that is only text can use the plain string form.
        if msg.parts.len() == 1 {
            if let Part::Text(t) = &msg.parts[0] {
                messages.push(json!({ "role": role, "content": t }));
                continue;
            }
        }
        let content: Vec<Value> = msg
            .parts
            .iter()
            .map(|p| match p {
                Part::Text(t) => json!({ "type": "text", "text": t }),
                Part::Image { mime, data_b64 } => json!({
                    "type": "image_url",
                    "image_url": {
                        "url": format!("data:{mime};base64,{data_b64}"),
                        "detail": "high"
                    }
                }),
            })
            .collect();
        messages.push(json!({ "role": role, "content": content }));
    }

    let mut body = json!({
        "model": req.model,
        "messages": messages,
    });

    if is_reasoning_model(&req.model) {
        body["max_completion_tokens"] = json!(req.max_tokens);
    } else {
        body["max_tokens"] = json!(req.max_tokens);
        body["temperature"] = json!(req.temperature);
    }
    if req.json_mode {
        body["response_format"] = json!({ "type": "json_object" });
    }

    let url = format!("{}/chat/completions", client.creds.base_url);
    let resp = client
        .http()
        .post(&url)
        .bearer_auth(&client.creds.api_key)
        .json(&body)
        .send()
        .await?;
    if !resp.status().is_success() {
        return Err(error_for_status("OpenAI", resp).await);
    }
    let value: Value = resp.json().await?;
    let text = value
        .pointer("/choices/0/message/content")
        .and_then(|c| c.as_str())
        .unwrap_or_default()
        .to_string();

    Ok(ChatResponse {
        text,
        input_tokens: value
            .pointer("/usage/prompt_tokens")
            .and_then(|v| v.as_u64())
            .unwrap_or(0),
        output_tokens: value
            .pointer("/usage/completion_tokens")
            .and_then(|v| v.as_u64())
            .unwrap_or(0),
        model: req.model.clone(),
    })
}
