//! Persisted app settings: provider credentials, model choices per role,
//! throughput knobs, narration and video options.

use crate::providers::{LlmClient, ProviderCreds, ProviderKind};
use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ProviderSettings {
    #[serde(default)]
    pub api_key: String,
    /// Override the endpoint. Mainly for Ollama, or an OpenAI-compatible proxy.
    #[serde(default)]
    pub base_url: Option<String>,
    /// Cached from the last successful refresh so the picker is populated on boot.
    #[serde(default)]
    pub cached_models: Vec<crate::providers::ModelInfo>,
    #[serde(default)]
    pub models_refreshed_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ModelChoice {
    pub provider: ProviderKind,
    pub model: String,
}

impl Default for ModelChoice {
    fn default() -> Self {
        Self {
            provider: ProviderKind::OpenAi,
            model: String::new(),
        }
    }
}

/// Which job a model is being asked to do. Splitting these lets a cheap fast
/// model read hundreds of pages while a stronger one does the writing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ModelRole {
    /// Page and panel perception. Must be a vision model.
    Vision,
    /// Story bible synthesis, event graph, grounding checks.
    Reasoning,
    /// The narration itself.
    Writer,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TtsProvider {
    None,
    #[serde(rename = "elevenlabs")]
    ElevenLabs,
    #[serde(rename = "openai")]
    OpenAi,
}

impl Default for TtsProvider {
    fn default() -> Self {
        TtsProvider::None
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TtsSettings {
    #[serde(default)]
    pub provider: TtsProvider,
    #[serde(default)]
    pub api_key: String,
    #[serde(default)]
    pub voice_id: String,
    #[serde(default)]
    pub model_id: String,
    #[serde(default = "default_stability")]
    pub stability: f32,
    #[serde(default = "default_similarity")]
    pub similarity_boost: f32,
    #[serde(default = "default_speed")]
    pub speed: f32,
}

fn default_stability() -> f32 {
    0.45
}
fn default_similarity() -> f32 {
    0.75
}
fn default_speed() -> f32 {
    1.0
}

impl Default for TtsSettings {
    fn default() -> Self {
        Self {
            provider: TtsProvider::None,
            api_key: String::new(),
            voice_id: String::new(),
            model_id: String::new(),
            stability: default_stability(),
            similarity_boost: default_similarity(),
            speed: default_speed(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VideoSettings {
    #[serde(default = "default_width")]
    pub width: u32,
    #[serde(default = "default_height")]
    pub height: u32,
    #[serde(default = "default_fps")]
    pub fps: u32,
    #[serde(default = "default_crf")]
    pub crf: u32,
    /// Slow zoom on each panel.
    #[serde(default = "default_true")]
    pub ken_burns: bool,
    /// Crossfade between shots, in seconds. 0 disables it.
    #[serde(default = "default_fade")]
    pub fade_secs: f32,
    /// Blurred fill behind letterboxed panels instead of flat black.
    #[serde(default = "default_true")]
    pub blurred_background: bool,
    /// Explicit ffmpeg path. Empty means "find it on PATH".
    #[serde(default)]
    pub ffmpeg_path: String,
}

fn default_width() -> u32 {
    1920
}
fn default_height() -> u32 {
    1080
}
fn default_fps() -> u32 {
    30
}
fn default_crf() -> u32 {
    20
}
fn default_true() -> bool {
    true
}
fn default_fade() -> f32 {
    0.35
}

impl Default for VideoSettings {
    fn default() -> Self {
        Self {
            width: default_width(),
            height: default_height(),
            fps: default_fps(),
            crf: default_crf(),
            ken_burns: true,
            fade_secs: default_fade(),
            blurred_background: true,
            ffmpeg_path: String::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Settings {
    #[serde(default)]
    pub providers: BTreeMap<String, ProviderSettings>,
    /// The model used for everything that has no override.
    #[serde(default)]
    pub primary: ModelChoice,
    #[serde(default)]
    pub vision_override: Option<ModelChoice>,
    #[serde(default)]
    pub writer_override: Option<ModelChoice>,

    /// Parallel AI calls during analysis.
    #[serde(default = "default_concurrency")]
    pub concurrency: usize,
    /// Pages sent to the vision model per call.
    #[serde(default = "default_batch")]
    pub pages_per_batch: usize,
    /// Longest edge, in pixels, of images sent to vision models.
    #[serde(default = "default_vision_dim")]
    pub vision_max_dim: u32,
    /// Run panel segmentation during ingest.
    #[serde(default = "default_true")]
    pub extract_panels: bool,
    /// Send individual panels rather than whole pages to the vision model.
    /// Much more accurate on dense pages, and more expensive.
    #[serde(default)]
    pub panel_level_vision: bool,
    /// Run the grounding pass that flags invented sentences.
    #[serde(default = "default_true")]
    pub grounding_check: bool,

    /// Pages narrated per writer call. The narration covers every panel on every
    /// page in the pass, so this is what keeps a long chapter inside the output
    /// limit without the model having to merge panels to fit.
    #[serde(default = "default_pages_per_narration")]
    pub pages_per_narration: usize,
    /// Output token ceiling for each narration call. The prompt asks for full,
    /// unhurried descriptions, so this runs as high as the writer model allows.
    #[serde(default = "default_max_output_tokens")]
    pub max_output_tokens: u32,

    /// The user's own narrator prompt. Empty means "use the shipped one", so a
    /// fresh install and a user who has reset their edit look identical.
    #[serde(default)]
    pub narrator_prompt: String,
    /// The user's own delivery contract. Empty means the shipped one.
    #[serde(default)]
    pub delivery_contract: String,
    /// Ids of mechanical checks the user has switched off. A disabled check is
    /// left out of the report entirely rather than shown as passing.
    #[serde(default)]
    pub disabled_rules: Vec<String>,

    #[serde(default)]
    pub tts: TtsSettings,
    #[serde(default)]
    pub video: VideoSettings,

    #[serde(default)]
    pub projects_root: Option<String>,
    #[serde(default = "default_theme")]
    pub theme: String,
    #[serde(default)]
    pub onboarded: bool,
}

fn default_concurrency() -> usize {
    3
}
fn default_batch() -> usize {
    4
}
fn default_vision_dim() -> u32 {
    1280
}
fn default_pages_per_narration() -> usize {
    4
}
fn default_max_output_tokens() -> u32 {
    16384
}
fn default_theme() -> String {
    "dark".into()
}

impl Default for Settings {
    fn default() -> Self {
        let mut providers = BTreeMap::new();
        for kind in ProviderKind::all() {
            providers.insert(
                kind.id().to_string(),
                ProviderSettings {
                    base_url: Some(kind.default_base_url().to_string()),
                    ..Default::default()
                },
            );
        }
        Self {
            providers,
            primary: ModelChoice::default(),
            vision_override: None,
            writer_override: None,
            concurrency: default_concurrency(),
            pages_per_batch: default_batch(),
            vision_max_dim: default_vision_dim(),
            extract_panels: true,
            panel_level_vision: false,
            grounding_check: true,
            pages_per_narration: default_pages_per_narration(),
            max_output_tokens: default_max_output_tokens(),
            narrator_prompt: String::new(),
            delivery_contract: String::new(),
            disabled_rules: Vec::new(),
            tts: TtsSettings::default(),
            video: VideoSettings::default(),
            projects_root: None,
            theme: default_theme(),
            onboarded: false,
        }
    }
}

impl Settings {
    pub fn provider(&self, kind: ProviderKind) -> ProviderSettings {
        self.providers
            .get(kind.id())
            .cloned()
            .unwrap_or_else(|| ProviderSettings {
                base_url: Some(kind.default_base_url().to_string()),
                ..Default::default()
            })
    }

    pub fn creds(&self, kind: ProviderKind) -> ProviderCreds {
        let p = self.provider(kind);
        ProviderCreds::new(kind, p.api_key, p.base_url)
    }

    pub fn client(&self, kind: ProviderKind) -> Result<LlmClient> {
        LlmClient::new(self.creds(kind))
    }

    pub fn choice_for(&self, role: ModelRole) -> ModelChoice {
        let override_choice = match role {
            ModelRole::Vision => self.vision_override.clone(),
            ModelRole::Writer => self.writer_override.clone(),
            ModelRole::Reasoning => None,
        };
        override_choice
            .filter(|c| !c.model.trim().is_empty())
            .unwrap_or_else(|| self.primary.clone())
    }

    /// Resolve a role to a ready-to-use client plus the model id.
    pub fn resolve(&self, role: ModelRole) -> Result<(LlmClient, String)> {
        let choice = self.choice_for(role);
        if choice.model.trim().is_empty() {
            return Err(anyhow!(
                "No model selected yet. Open Settings, add a provider key, refresh the model list and pick a model."
            ));
        }
        let client = self.client(choice.provider)?;
        Ok((client, choice.model))
    }

    /// The narrator prompt actually sent: the user's, or the shipped default
    /// when they have not written one. A prompt of pure whitespace counts as
    /// unset, so the engine can never be handed an empty system prompt.
    pub fn narrator_prompt(&self) -> &str {
        if self.narrator_prompt.trim().is_empty() {
            crate::narrator::NARRATOR_PROMPT
        } else {
            &self.narrator_prompt
        }
    }

    /// The delivery contract actually sent, same fallback.
    pub fn delivery_contract(&self) -> &str {
        if self.delivery_contract.trim().is_empty() {
            crate::narrator::DELIVERY_CONTRACT
        } else {
            &self.delivery_contract
        }
    }

    pub fn rule_is_enabled(&self, id: &str) -> bool {
        !self.disabled_rules.iter().any(|d| d == id)
    }

    /// True when at least one provider is usable.
    pub fn is_configured(&self) -> bool {
        !self.primary.model.trim().is_empty()
    }

    pub fn sanitized(&self) -> Settings {
        // Never ship raw keys back to the UI; it only needs to know they exist.
        let mut clone = self.clone();
        for p in clone.providers.values_mut() {
            if !p.api_key.is_empty() {
                p.api_key = mask(&p.api_key);
            }
        }
        if !clone.tts.api_key.is_empty() {
            clone.tts.api_key = mask(&clone.tts.api_key);
        }
        clone
    }
}

fn mask(key: &str) -> String {
    let len = key.chars().count();
    if len <= 8 {
        return "*".repeat(len);
    }
    let tail: String = key.chars().skip(len - 4).collect();
    format!("{}...{}", &key.chars().take(3).collect::<String>(), tail)
}

// ---------------------------------------------------------------------------
// Persistence
// ---------------------------------------------------------------------------

pub fn settings_path(config_dir: &PathBuf) -> PathBuf {
    config_dir.join("settings.json")
}

pub fn load(config_dir: &PathBuf) -> Settings {
    let path = settings_path(config_dir);
    match std::fs::read_to_string(&path) {
        Ok(raw) => serde_json::from_str(&raw).unwrap_or_else(|err| {
            tracing::warn!("settings.json unreadable ({err}); starting from defaults");
            Settings::default()
        }),
        Err(_) => Settings::default(),
    }
}

pub fn save(config_dir: &PathBuf, settings: &Settings) -> Result<()> {
    std::fs::create_dir_all(config_dir)?;
    let path = settings_path(config_dir);
    let json = serde_json::to_string_pretty(settings)?;
    // Write to a temp file then rename, so a crash mid-write cannot corrupt it.
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, json)?;
    std::fs::rename(&tmp, &path)?;
    Ok(())
}
