//! Provider abstraction.
//!
//! Every AI call in the app goes through `LlmClient`. Four backends are wired up:
//! OpenAI, Anthropic, Google Gemini and Ollama. Each one can list its own models
//! live, which is what backs the refreshable model picker in Settings.

pub mod anthropic;
pub mod gemini;
pub mod ollama;
pub mod openai;

use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use std::time::Duration;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ProviderKind {
    /// Renamed so the wire value matches `id()`; the derived name would be
    /// "open_ai", which would not round-trip against the provider id strings.
    #[default]
    #[serde(rename = "openai")]
    OpenAi,
    Anthropic,
    Gemini,
    Ollama,
}

impl ProviderKind {
    pub fn id(self) -> &'static str {
        match self {
            ProviderKind::OpenAi => "openai",
            ProviderKind::Anthropic => "anthropic",
            ProviderKind::Gemini => "gemini",
            ProviderKind::Ollama => "ollama",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            ProviderKind::OpenAi => "OpenAI",
            ProviderKind::Anthropic => "Anthropic Claude",
            ProviderKind::Gemini => "Google Gemini",
            ProviderKind::Ollama => "Ollama (local)",
        }
    }

    /// Ollama runs locally and needs no key.
    pub fn needs_api_key(self) -> bool {
        !matches!(self, ProviderKind::Ollama)
    }

    pub fn default_base_url(self) -> &'static str {
        match self {
            ProviderKind::OpenAi => "https://api.openai.com/v1",
            ProviderKind::Anthropic => "https://api.anthropic.com/v1",
            ProviderKind::Gemini => "https://generativelanguage.googleapis.com/v1beta",
            ProviderKind::Ollama => "http://localhost:11434",
        }
    }

    pub fn all() -> [ProviderKind; 4] {
        [
            ProviderKind::OpenAi,
            ProviderKind::Anthropic,
            ProviderKind::Gemini,
            ProviderKind::Ollama,
        ]
    }

    pub fn from_id(id: &str) -> Option<Self> {
        match id {
            "openai" => Some(ProviderKind::OpenAi),
            "anthropic" => Some(ProviderKind::Anthropic),
            "gemini" => Some(ProviderKind::Gemini),
            "ollama" => Some(ProviderKind::Ollama),
            _ => None,
        }
    }
}

// ---------------------------------------------------------------------------
// Requests
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Part {
    Text(String),
    Image { mime: String, data_b64: String },
}

impl Part {
    pub fn text(s: impl Into<String>) -> Self {
        Part::Text(s.into())
    }

    pub fn image(mime: impl Into<String>, data_b64: impl Into<String>) -> Self {
        Part::Image {
            mime: mime.into(),
            data_b64: data_b64.into(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    User,
    Assistant,
}

#[derive(Debug, Clone)]
pub struct ChatMessage {
    pub role: Role,
    pub parts: Vec<Part>,
}

impl ChatMessage {
    pub fn user(parts: Vec<Part>) -> Self {
        Self {
            role: Role::User,
            parts,
        }
    }

    pub fn user_text(text: impl Into<String>) -> Self {
        Self {
            role: Role::User,
            parts: vec![Part::text(text)],
        }
    }

    pub fn assistant_text(text: impl Into<String>) -> Self {
        Self {
            role: Role::Assistant,
            parts: vec![Part::text(text)],
        }
    }
}

#[derive(Debug, Clone)]
pub struct ChatRequest {
    pub model: String,
    pub system: Option<String>,
    pub messages: Vec<ChatMessage>,
    pub max_tokens: u32,
    pub temperature: f32,
    /// Ask the provider for strict JSON where it supports it.
    pub json_mode: bool,
}

impl ChatRequest {
    pub fn new(model: impl Into<String>) -> Self {
        Self {
            model: model.into(),
            system: None,
            messages: Vec::new(),
            max_tokens: 4096,
            temperature: 0.4,
            json_mode: false,
        }
    }

    pub fn system(mut self, s: impl Into<String>) -> Self {
        self.system = Some(s.into());
        self
    }

    pub fn push(mut self, m: ChatMessage) -> Self {
        self.messages.push(m);
        self
    }

    pub fn max_tokens(mut self, n: u32) -> Self {
        self.max_tokens = n;
        self
    }

    pub fn temperature(mut self, t: f32) -> Self {
        self.temperature = t;
        self
    }

    pub fn json(mut self) -> Self {
        self.json_mode = true;
        self
    }

    pub fn image_count(&self) -> usize {
        self.messages
            .iter()
            .flat_map(|m| m.parts.iter())
            .filter(|p| matches!(p, Part::Image { .. }))
            .count()
    }
}

#[derive(Debug, Clone, Default)]
pub struct ChatResponse {
    pub text: String,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub model: String,
}

// ---------------------------------------------------------------------------
// Model listing
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelInfo {
    pub id: String,
    pub label: String,
    /// Whether this model can accept images. Drives the "vision" badge and the
    /// warning shown if a text-only model is picked for page analysis.
    pub vision: bool,
    #[serde(default)]
    pub context_tokens: Option<u64>,
    #[serde(default)]
    pub family: String,
    /// Higher sorts first in the picker.
    #[serde(default)]
    pub rank: i32,
}

// ---------------------------------------------------------------------------
// Client
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct ProviderCreds {
    pub kind: ProviderKind,
    pub api_key: String,
    pub base_url: String,
}

impl ProviderCreds {
    pub fn new(kind: ProviderKind, api_key: impl Into<String>, base_url: Option<String>) -> Self {
        let base = base_url
            .map(|b| b.trim().trim_end_matches('/').to_string())
            .filter(|b| !b.is_empty())
            .unwrap_or_else(|| kind.default_base_url().to_string());
        Self {
            kind,
            api_key: api_key.into().trim().to_string(),
            base_url: base,
        }
    }
}

pub struct LlmClient {
    pub creds: ProviderCreds,
    http: reqwest::Client,
}

impl LlmClient {
    pub fn new(creds: ProviderCreds) -> Result<Self> {
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(600))
            .connect_timeout(Duration::from_secs(20))
            .user_agent("RecapStudio/1.0")
            .build()?;
        Ok(Self { creds, http })
    }

    pub fn http(&self) -> &reqwest::Client {
        &self.http
    }

    fn ensure_key(&self) -> Result<()> {
        if self.creds.kind.needs_api_key() && self.creds.api_key.is_empty() {
            return Err(anyhow!(
                "No API key set for {}. Add one in Settings.",
                self.creds.kind.label()
            ));
        }
        Ok(())
    }

    pub async fn list_models(&self) -> Result<Vec<ModelInfo>> {
        self.ensure_key()?;
        let mut models = match self.creds.kind {
            ProviderKind::OpenAi => openai::list_models(self).await?,
            ProviderKind::Anthropic => anthropic::list_models(self).await?,
            ProviderKind::Gemini => gemini::list_models(self).await?,
            ProviderKind::Ollama => ollama::list_models(self).await?,
        };
        models.sort_by(|a, b| b.rank.cmp(&a.rank).then_with(|| a.id.cmp(&b.id)));
        Ok(models)
    }

    pub async fn chat(&self, req: ChatRequest) -> Result<ChatResponse> {
        self.ensure_key()?;
        if req.model.trim().is_empty() {
            return Err(anyhow!(
                "No model selected for {}. Pick one in Settings.",
                self.creds.kind.label()
            ));
        }
        // Transient failures are common with vision payloads; retry with backoff.
        let mut attempt = 0u32;
        loop {
            attempt += 1;
            let result = match self.creds.kind {
                ProviderKind::OpenAi => openai::chat(self, &req).await,
                ProviderKind::Anthropic => anthropic::chat(self, &req).await,
                ProviderKind::Gemini => gemini::chat(self, &req).await,
                ProviderKind::Ollama => ollama::chat(self, &req).await,
            };
            match result {
                Ok(resp) => return Ok(resp),
                Err(err) => {
                    let msg = err.to_string();
                    let retryable = is_retryable(&msg);
                    if !retryable || attempt >= 4 {
                        return Err(err);
                    }
                    let wait = Duration::from_millis(1200u64 * 2u64.pow(attempt - 1));
                    tracing::warn!(
                        "{} call failed (attempt {attempt}), retrying in {:?}: {msg}",
                        self.creds.kind.id(),
                        wait
                    );
                    tokio::time::sleep(wait).await;
                }
            }
        }
    }

    /// Quick credential check used by the "Test connection" button.
    pub async fn verify(&self) -> Result<String> {
        let models = self.list_models().await?;
        Ok(format!(
            "{} reachable - {} model{} available",
            self.creds.kind.label(),
            models.len(),
            if models.len() == 1 { "" } else { "s" }
        ))
    }
}

fn is_retryable(msg: &str) -> bool {
    let lower = msg.to_ascii_lowercase();
    ["429", "500", "502", "503", "504", "overloaded", "rate limit", "timed out", "timeout", "connection reset", "sending request"]
        .iter()
        .any(|needle| lower.contains(needle))
}

/// Shared helper: turn a non-2xx response into a useful error message.
pub(crate) async fn error_for_status(
    provider: &str,
    resp: reqwest::Response,
) -> anyhow::Error {
    let status = resp.status();
    let body = resp.text().await.unwrap_or_default();
    // Most providers nest the useful text under { "error": { "message": ... } }.
    let detail = serde_json::from_str::<serde_json::Value>(&body)
        .ok()
        .and_then(|v| {
            v.pointer("/error/message")
                .or_else(|| v.pointer("/error"))
                .or_else(|| v.pointer("/message"))
                .map(|m| m.as_str().map(str::to_string).unwrap_or_else(|| m.to_string()))
        })
        .unwrap_or_else(|| crate::util::truncate(&body, 400));

    let hint = match status.as_u16() {
        401 | 403 => " (check the API key in Settings)",
        404 => " (the selected model may not exist for this key - refresh the model list)",
        429 => " (rate limited - the app will retry, or slow down concurrency in Settings)",
        _ => "",
    };
    anyhow!("{provider} returned {status}{hint}: {detail}")
}
