use super::{error_for_status, ChatRequest, ChatResponse, LlmClient, ModelInfo, Part, Role};
use anyhow::Result;
use serde_json::{json, Value};

const API_VERSION: &str = "2023-06-01";

/// Every current Claude model accepts images. Older Claude 2.x text models do not.
fn supports_vision(model: &str) -> bool {
    let m = model.to_ascii_lowercase();
    !(m.starts_with("claude-instant") || m.starts_with("claude-2"))
}

fn rank_of(model: &str) -> i32 {
    let m = model.to_ascii_lowercase();
    let mut rank = if m.contains("opus") {
        90
    } else if m.contains("sonnet") {
        80
    } else if m.contains("haiku") {
        60
    } else {
        20
    };
    // Prefer newer generations.
    for (needle, bonus) in [
        ("claude-5", 40),
        ("claude-4", 30),
        ("claude-3-7", 20),
        ("claude-3-5", 10),
    ] {
        if m.contains(needle) {
            rank += bonus;
            break;
        }
    }
    if m.contains("latest") {
        rank += 5;
    }
    rank
}

pub async fn list_models(client: &LlmClient) -> Result<Vec<ModelInfo>> {
    let url = format!("{}/models?limit=100", client.creds.base_url);
    let resp = client
        .http()
        .get(&url)
        .header("x-api-key", &client.creds.api_key)
        .header("anthropic-version", API_VERSION)
        .send()
        .await?;
    if !resp.status().is_success() {
        return Err(error_for_status("Anthropic", resp).await);
    }
    let body: Value = resp.json().await?;
    let mut out = Vec::new();
    if let Some(list) = body.get("data").and_then(|d| d.as_array()) {
        for item in list {
            let Some(id) = item.get("id").and_then(|i| i.as_str()) else {
                continue;
            };
            let label = item
                .get("display_name")
                .and_then(|d| d.as_str())
                .unwrap_or(id)
                .to_string();
            out.push(ModelInfo {
                id: id.to_string(),
                label,
                vision: supports_vision(id),
                context_tokens: None,
                family: "anthropic".into(),
                rank: rank_of(id),
            });
        }
    }
    Ok(out)
}

pub async fn chat(client: &LlmClient, req: &ChatRequest) -> Result<ChatResponse> {
    let mut messages: Vec<Value> = Vec::new();
    for msg in &req.messages {
        let role = match msg.role {
            Role::User => "user",
            Role::Assistant => "assistant",
        };
        let content: Vec<Value> = msg
            .parts
            .iter()
            .map(|p| match p {
                Part::Text(t) => json!({ "type": "text", "text": t }),
                Part::Image { mime, data_b64 } => json!({
                    "type": "image",
                    "source": {
                        "type": "base64",
                        "media_type": mime,
                        "data": data_b64
                    }
                }),
            })
            .collect();
        messages.push(json!({ "role": role, "content": content }));
    }

    // Anthropic requires the conversation to start with a user turn.
    if messages
        .first()
        .and_then(|m| m.get("role"))
        .and_then(|r| r.as_str())
        != Some("user")
    {
        messages.insert(
            0,
            json!({ "role": "user", "content": [{ "type": "text", "text": "Begin." }] }),
        );
    }

    let mut body = json!({
        "model": req.model,
        "max_tokens": req.max_tokens,
        "temperature": req.temperature,
        "messages": messages,
    });
    if let Some(system) = &req.system {
        let mut system = system.clone();
        if req.json_mode {
            // Claude has no JSON mode, so the contract goes in the system prompt.
            system.push_str(
                "\n\nRespond with a single valid JSON value and nothing else. \
                 No prose before it, no prose after it, no markdown code fences.",
            );
        }
        body["system"] = json!(system);
    } else if req.json_mode {
        body["system"] = json!(
            "Respond with a single valid JSON value and nothing else. \
             No prose before it, no prose after it, no markdown code fences."
        );
    }

    let url = format!("{}/messages", client.creds.base_url);
    let resp = client
        .http()
        .post(&url)
        .header("x-api-key", &client.creds.api_key)
        .header("anthropic-version", API_VERSION)
        .header("content-type", "application/json")
        .json(&body)
        .send()
        .await?;
    if !resp.status().is_success() {
        return Err(error_for_status("Anthropic", resp).await);
    }
    let value: Value = resp.json().await?;

    // content is an array of blocks; concatenate every text block.
    let text = value
        .get("content")
        .and_then(|c| c.as_array())
        .map(|blocks| {
            blocks
                .iter()
                .filter_map(|b| b.get("text").and_then(|t| t.as_str()))
                .collect::<Vec<_>>()
                .join("")
        })
        .unwrap_or_default();

    Ok(ChatResponse {
        text,
        input_tokens: value
            .pointer("/usage/input_tokens")
            .and_then(|v| v.as_u64())
            .unwrap_or(0),
        output_tokens: value
            .pointer("/usage/output_tokens")
            .and_then(|v| v.as_u64())
            .unwrap_or(0),
        model: req.model.clone(),
    })
}
