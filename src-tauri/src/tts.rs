//! Narration. ElevenLabs or OpenAI, one clip per storyboard shot.
//!
//! Clips are generated per shot rather than as one long file so the video
//! renderer can hold each shot's images on screen for exactly as long as its
//! narration runs.

use crate::events::Emitter;
use crate::model::{Project, ScriptBundle};
use crate::project::Paths;
use crate::settings::{Settings, TtsProvider};
use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::path::{Path, PathBuf};
use std::time::Duration;

const ELEVEN_BASE: &str = "https://api.elevenlabs.io/v1";
const OPENAI_BASE: &str = "https://api.openai.com/v1";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VoiceInfo {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub preview_url: Option<String>,
}

fn http() -> Result<reqwest::Client> {
    Ok(reqwest::Client::builder()
        .timeout(Duration::from_secs(300))
        .user_agent("RecapStudio/1.0")
        .build()?)
}

/// Built-in OpenAI voices. OpenAI has no voice-listing endpoint.
const OPENAI_VOICES: &[(&str, &str)] = &[
    ("alloy", "Alloy - neutral, even"),
    ("ash", "Ash - warm, grounded"),
    ("ballad", "Ballad - expressive"),
    ("coral", "Coral - bright, upbeat"),
    ("echo", "Echo - crisp, male"),
    ("fable", "Fable - storyteller"),
    ("onyx", "Onyx - deep, male"),
    ("nova", "Nova - energetic, female"),
    ("sage", "Sage - calm"),
    ("shimmer", "Shimmer - soft, female"),
];

pub async fn list_voices(settings: &Settings) -> Result<Vec<VoiceInfo>> {
    match settings.tts.provider {
        TtsProvider::None => Ok(Vec::new()),
        TtsProvider::OpenAi => Ok(OPENAI_VOICES
            .iter()
            .map(|(id, desc)| VoiceInfo {
                id: id.to_string(),
                name: id.to_string(),
                description: desc.to_string(),
                preview_url: None,
            })
            .collect()),
        TtsProvider::ElevenLabs => {
            if settings.tts.api_key.trim().is_empty() {
                return Err(anyhow!("Add your ElevenLabs API key in Settings first."));
            }
            let resp = http()?
                .get(format!("{ELEVEN_BASE}/voices"))
                .header("xi-api-key", &settings.tts.api_key)
                .send()
                .await?;
            if !resp.status().is_success() {
                let status = resp.status();
                let body = resp.text().await.unwrap_or_default();
                return Err(anyhow!("ElevenLabs returned {status}: {}", crate::util::truncate(&body, 300)));
            }
            let value: serde_json::Value = resp.json().await?;
            let voices = value
                .get("voices")
                .and_then(|v| v.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|v| {
                            Some(VoiceInfo {
                                id: v.get("voice_id")?.as_str()?.to_string(),
                                name: v.get("name")?.as_str()?.to_string(),
                                description: v
                                    .get("labels")
                                    .and_then(|l| l.as_object())
                                    .map(|o| {
                                        o.iter()
                                            .filter_map(|(k, val)| {
                                                val.as_str().map(|s| format!("{k}: {s}"))
                                            })
                                            .collect::<Vec<_>>()
                                            .join(", ")
                                    })
                                    .unwrap_or_default(),
                                preview_url: v
                                    .get("preview_url")
                                    .and_then(|p| p.as_str())
                                    .map(str::to_string),
                            })
                        })
                        .collect()
                })
                .unwrap_or_default();
            Ok(voices)
        }
    }
}

pub async fn list_tts_models(settings: &Settings) -> Result<Vec<VoiceInfo>> {
    match settings.tts.provider {
        TtsProvider::None => Ok(Vec::new()),
        TtsProvider::OpenAi => Ok([
            ("gpt-4o-mini-tts", "Newest, most controllable"),
            ("tts-1-hd", "Higher fidelity"),
            ("tts-1", "Fastest"),
        ]
        .iter()
        .map(|(id, desc)| VoiceInfo {
            id: id.to_string(),
            name: id.to_string(),
            description: desc.to_string(),
            preview_url: None,
        })
        .collect()),
        TtsProvider::ElevenLabs => {
            if settings.tts.api_key.trim().is_empty() {
                return Err(anyhow!("Add your ElevenLabs API key in Settings first."));
            }
            let resp = http()?
                .get(format!("{ELEVEN_BASE}/models"))
                .header("xi-api-key", &settings.tts.api_key)
                .send()
                .await?;
            if !resp.status().is_success() {
                let status = resp.status();
                return Err(anyhow!("ElevenLabs returned {status} listing models"));
            }
            let value: serde_json::Value = resp.json().await?;
            Ok(value
                .as_array()
                .map(|arr| {
                    arr.iter()
                        .filter(|m| {
                            m.get("can_do_text_to_speech")
                                .and_then(|c| c.as_bool())
                                .unwrap_or(true)
                        })
                        .filter_map(|m| {
                            Some(VoiceInfo {
                                id: m.get("model_id")?.as_str()?.to_string(),
                                name: m
                                    .get("name")
                                    .and_then(|n| n.as_str())
                                    .unwrap_or_default()
                                    .to_string(),
                                description: m
                                    .get("description")
                                    .and_then(|d| d.as_str())
                                    .unwrap_or_default()
                                    .to_string(),
                                preview_url: None,
                            })
                        })
                        .collect()
                })
                .unwrap_or_default())
        }
    }
}

async fn synthesize(settings: &Settings, text: &str) -> Result<Vec<u8>> {
    let tts = &settings.tts;
    match tts.provider {
        TtsProvider::None => Err(anyhow!("No narration provider selected in Settings.")),
        TtsProvider::ElevenLabs => {
            if tts.voice_id.trim().is_empty() {
                return Err(anyhow!("Pick an ElevenLabs voice in Settings first."));
            }
            let model = if tts.model_id.trim().is_empty() {
                "eleven_multilingual_v2"
            } else {
                tts.model_id.trim()
            };
            let body = json!({
                "text": text,
                "model_id": model,
                "voice_settings": {
                    "stability": tts.stability,
                    "similarity_boost": tts.similarity_boost,
                    "speed": tts.speed,
                }
            });
            let resp = http()?
                .post(format!(
                    "{ELEVEN_BASE}/text-to-speech/{}?output_format=mp3_44100_128",
                    tts.voice_id.trim()
                ))
                .header("xi-api-key", &tts.api_key)
                .header("accept", "audio/mpeg")
                .json(&body)
                .send()
                .await?;
            if !resp.status().is_success() {
                let status = resp.status();
                let detail = resp.text().await.unwrap_or_default();
                return Err(anyhow!(
                    "ElevenLabs returned {status}: {}",
                    crate::util::truncate(&detail, 300)
                ));
            }
            Ok(resp.bytes().await?.to_vec())
        }
        TtsProvider::OpenAi => {
            let key = if tts.api_key.trim().is_empty() {
                settings
                    .provider(crate::providers::ProviderKind::OpenAi)
                    .api_key
            } else {
                tts.api_key.clone()
            };
            if key.trim().is_empty() {
                return Err(anyhow!("Add an OpenAI API key in Settings first."));
            }
            let model = if tts.model_id.trim().is_empty() {
                "gpt-4o-mini-tts"
            } else {
                tts.model_id.trim()
            };
            let voice = if tts.voice_id.trim().is_empty() {
                "onyx"
            } else {
                tts.voice_id.trim()
            };
            let body = json!({
                "model": model,
                "voice": voice,
                "input": text,
                "response_format": "mp3",
                "speed": tts.speed.clamp(0.25, 4.0),
            });
            let resp = http()?
                .post(format!("{OPENAI_BASE}/audio/speech"))
                .bearer_auth(key)
                .json(&body)
                .send()
                .await?;
            if !resp.status().is_success() {
                let status = resp.status();
                let detail = resp.text().await.unwrap_or_default();
                return Err(anyhow!(
                    "OpenAI TTS returned {status}: {}",
                    crate::util::truncate(&detail, 300)
                ));
            }
            Ok(resp.bytes().await?.to_vec())
        }
    }
}

/// Render one short sample so the user can audition a voice before committing.
pub async fn preview(settings: &Settings, text: &str, dest: &Path) -> Result<PathBuf> {
    let audio = synthesize(settings, text).await?;
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(dest, &audio)?;
    Ok(dest.to_path_buf())
}

/// Narrate every shot in the bundle, writing mp3s into the project's audio dir.
pub async fn narrate(
    project: &Project,
    bundle: &mut ScriptBundle,
    settings: &Settings,
    emitter: &Emitter,
) -> Result<()> {
    if matches!(settings.tts.provider, TtsProvider::None) {
        return Err(anyhow!(
            "Narration is turned off. Choose ElevenLabs or OpenAI under Narration in Settings."
        ));
    }
    if bundle.storyboard.is_empty() {
        return Err(anyhow!("This script has no storyboard shots to narrate."));
    }

    let paths = Paths::new(&project.meta.root);
    std::fs::create_dir_all(paths.audio())?;
    let total = bundle.storyboard.len();
    emitter.stage("narrate", format!("Narrating {total} shots"));

    // Sequential on purpose: TTS providers rate limit aggressively, and a
    // half-narrated script is worse than a slow one.
    for i in 0..total {
        let text = bundle.storyboard[i].text.clone();
        if text.trim().is_empty() {
            continue;
        }
        let dest = paths.audio().join(format!("shot-{i:04}.mp3"));
        match synthesize(settings, &text).await {
            Ok(audio) => {
                std::fs::write(&dest, &audio)?;
                bundle.storyboard[i].audio_path = Some(dest.to_string_lossy().to_string());
                bundle.storyboard[i].duration_secs = None;
            }
            Err(err) => {
                emitter.failed(format!("Narration failed on shot {}: {err}", i + 1));
                return Err(err);
            }
        }
        emitter.progress("narrate", i + 1, total, format!("Narrated shot {}/{total}", i + 1));
    }

    emitter.done(format!("Narration complete: {total} clips"));
    Ok(())
}
