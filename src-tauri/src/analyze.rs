//! The analyze engine.
//!
//! Eight passes turn raw pages into a story bible rich enough that the narrator
//! never has to guess. The per-panel record from pass 1 is what the narration is
//! written from, panel by panel, so nothing here filters panels out by
//! significance - every panel the analyzer sees reaches the narrator.
//!
//!   1. Perception  - every page and panel read into structured JSON
//!   2. Chapters    - chapter boundaries corrected from detected chapter pages
//!   3. Cast        - observations merged into canonical characters + relationships
//!   4. Portraits   - a representative panel picked for each character
//!   5. Graph       - events, causal links, open threads, foreshadowing
//!   6. Chapters    - per-chapter synopsis and beat sheet, carrying continuity
//!   7. Work meta   - genre, premise, setting, power system, tone
//!   8. Save
//!
//! Passes 1 and 6 are the expensive ones. Pass 1 runs concurrently; pass 6 is
//! sequential on purpose, because each chapter needs the continuity of the last.

use crate::events::Emitter;
use crate::model::*;
use crate::project::{self, Paths};
use crate::prompts;
use crate::providers::{ChatMessage, ChatRequest, LlmClient, Part};
use crate::settings::{ModelRole, Settings};
use crate::util;
use anyhow::{anyhow, Result};
use futures::stream::{self, StreamExt};
use serde::Deserialize;
use std::collections::{BTreeMap, HashMap};
use std::path::Path;
use std::sync::Arc;

// ---------------------------------------------------------------------------
// Shared call helpers
// ---------------------------------------------------------------------------

/// Ask a model for JSON, with one corrective retry when the reply will not parse.
async fn ask_json<T: serde::de::DeserializeOwned>(
    client: &LlmClient,
    model: &str,
    system: &str,
    messages: Vec<ChatMessage>,
    max_tokens: u32,
) -> Result<(T, u64, u64)> {
    let req = ChatRequest::new(model)
        .system(system)
        .max_tokens(max_tokens)
        .temperature(0.2)
        .json();
    let req = messages
        .iter()
        .cloned()
        .fold(req, |acc, m| acc.push(m));

    let first = client.chat(req).await?;
    match util::parse_json_lenient::<T>(&first.text) {
        Ok(value) => Ok((value, first.input_tokens, first.output_tokens)),
        Err(parse_err) => {
            tracing::warn!("JSON parse failed, asking the model to repair: {parse_err}");
            let mut repair_messages = messages;
            repair_messages.push(ChatMessage::assistant_text(util::truncate(
                &first.text,
                4000,
            )));
            repair_messages.push(ChatMessage::user_text(format!(
                "That reply could not be parsed as JSON ({parse_err}). \
                 Send the same content again as one valid JSON value. \
                 No markdown fences, no prose, no trailing commas."
            )));
            let retry_req = repair_messages
                .iter()
                .cloned()
                .fold(
                    ChatRequest::new(model)
                        .system(system)
                        .max_tokens(max_tokens)
                        .temperature(0.0)
                        .json(),
                    |acc, m| acc.push(m),
                );
            let second = client.chat(retry_req).await?;
            let value = util::parse_json_lenient::<T>(&second.text)?;
            Ok((
                value,
                first.input_tokens + second.input_tokens,
                first.output_tokens + second.output_tokens,
            ))
        }
    }
}

/// Ask a model for prose.
pub async fn ask_text(
    client: &LlmClient,
    model: &str,
    system: &str,
    messages: Vec<ChatMessage>,
    max_tokens: u32,
    temperature: f32,
) -> Result<(String, u64, u64)> {
    let req = messages.into_iter().fold(
        ChatRequest::new(model)
            .system(system)
            .max_tokens(max_tokens)
            .temperature(temperature),
        |acc, m| acc.push(m),
    );
    let resp = client.chat(req).await?;
    Ok((resp.text, resp.input_tokens, resp.output_tokens))
}

fn image_part(path: &Path) -> Result<Part> {
    let bytes = std::fs::read(path)
        .map_err(|e| anyhow!("could not read {} for analysis: {e}", path.display()))?;
    let mime = match path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase())
        .as_deref()
    {
        Some("png") => "image/png",
        Some("webp") => "image/webp",
        Some("gif") => "image/gif",
        _ => "image/jpeg",
    };
    Ok(Part::image(mime, util::b64(&bytes)))
}

// ---------------------------------------------------------------------------
// Pass 1 - perception
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct PerceptionBatch {
    #[serde(default)]
    pages: Vec<PageReading>,
}

fn batch_manifest(pages: &[&PageAsset], source: &Source) -> String {
    pages
        .iter()
        .map(|p| {
            let chapter = source
                .chapters
                .get(p.chapter_index)
                .and_then(|c| c.title.clone())
                .unwrap_or_else(|| format!("Chapter {}", p.chapter_index + 1));
            format!(
                "- page_index {} | {} | page {} of that chapter | file {} | {} panel(s) detected",
                p.index,
                chapter,
                p.page_in_chapter + 1,
                p.label,
                p.panels.len()
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

async fn perceive_batch(
    client: &LlmClient,
    model: &str,
    source: &Source,
    batch: Vec<PageAsset>,
    context: String,
    panel_level: bool,
) -> Result<(Vec<PageReading>, u64, u64)> {
    let refs: Vec<&PageAsset> = batch.iter().collect();
    let manifest = batch_manifest(&refs, source);
    let order_hint = prompts::reading_order_hint(source.format);
    let user_text = prompts::perception_user(order_hint, &manifest, &context);

    let mut parts = vec![Part::text(user_text)];
    for page in &batch {
        parts.push(Part::text(format!("--- page_index {} ---", page.index)));
        if panel_level && !page.panels.is_empty() {
            // Panel-level vision: send each crop so dense pages are read accurately.
            for panel in &page.panels {
                parts.push(Part::text(format!("panel {}", panel.index)));
                parts.push(image_part(Path::new(&panel.path))?);
            }
        } else {
            parts.push(image_part(Path::new(&page.thumb_path))?);
        }
    }

    let (parsed, input, output) = ask_json::<PerceptionBatch>(
        client,
        model,
        prompts::PERCEPTION_SYSTEM,
        vec![ChatMessage::user(parts)],
        8192,
    )
    .await?;

    // The model sometimes returns fewer entries than pages, or drifts on the
    // index. Re-anchor by position when the count matches.
    let mut readings = parsed.pages;
    if readings.len() == batch.len() {
        for (reading, page) in readings.iter_mut().zip(batch.iter()) {
            reading.page_index = page.index;
        }
    } else {
        let valid: Vec<usize> = batch.iter().map(|p| p.index).collect();
        readings.retain(|r| valid.contains(&r.page_index));
    }
    Ok((readings, input, output))
}

fn condense_for_context(readings: &[PageReading], limit: usize) -> String {
    let mut lines = Vec::new();
    for reading in readings {
        for event in &reading.events {
            if event.importance >= 40 {
                lines.push(format!("- {}", event.summary));
            }
        }
    }
    if lines.len() > limit {
        lines.drain(..lines.len() - limit);
    }
    lines.join("\n")
}

// ---------------------------------------------------------------------------
// Pass 2 - chapter boundary correction
// ---------------------------------------------------------------------------

/// When the source arrived as a single blob (a PDF, or one flat folder) but the
/// pages contain printed chapter starts, re-cut the chapters on those pages.
fn recut_chapters(source: &mut Source, readings: &[PageReading], emitter: &Emitter) {
    if source.chapters.len() > 1 {
        return; // folder structure already told us the truth
    }
    let mut starts: Vec<usize> = readings
        .iter()
        .filter(|r| r.role == PageRole::ChapterStart)
        .map(|r| r.page_index)
        .collect();
    starts.sort_unstable();
    starts.dedup();

    if starts.len() < 2 {
        return;
    }

    if starts.first() != Some(&0) {
        // Everything before the first detected chapter page is front matter, but
        // it still has to live somewhere, so fold it into chapter one.
        starts.insert(0, 0);
    }

    let by_page: HashMap<usize, &PageReading> =
        readings.iter().map(|r| (r.page_index, r)).collect();

    let mut chapters = Vec::new();
    for (i, start) in starts.iter().enumerate() {
        let end = starts
            .get(i + 1)
            .map(|next| next - 1)
            .unwrap_or_else(|| source.pages.len().saturating_sub(1));
        let reading = by_page.get(start);
        let number = reading.and_then(|r| r.chapter_number).or(Some(i as f32 + 1.0));
        let title = reading
            .and_then(|r| r.chapter_title.clone())
            .filter(|t| !t.trim().is_empty())
            .unwrap_or_else(|| match number {
                Some(n) if n.fract() == 0.0 => format!("Chapter {}", n as i64),
                Some(n) => format!("Chapter {n}"),
                None => format!("Chapter {}", i + 1),
            });
        chapters.push(ChapterRef {
            index: i,
            number,
            title: Some(title),
            origin: "Detected from chapter pages".into(),
            first_page: *start,
            last_page: end,
        });
    }

    for page in source.pages.iter_mut() {
        if let Some(chapter) = chapters
            .iter()
            .find(|c| page.index >= c.first_page && page.index <= c.last_page)
        {
            page.chapter_index = chapter.index;
            page.page_in_chapter = page.index - chapter.first_page;
        }
    }

    emitter.info(format!(
        "Split into {} chapters using chapter pages found in the art",
        chapters.len()
    ));
    source.chapter_strategy = format!("{} chapters detected from chapter start pages", chapters.len());
    source.chapters = chapters;
}

// ---------------------------------------------------------------------------
// Pass 3 - cast resolution
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct CastResult {
    #[serde(default)]
    characters: Vec<CastCharacter>,
    #[serde(default)]
    relationships: Vec<Relationship>,
}

#[derive(Debug, Deserialize)]
struct CastCharacter {
    #[serde(default)]
    name: String,
    #[serde(default)]
    aliases: Vec<String>,
    #[serde(default)]
    role: String,
    #[serde(default)]
    appearance: String,
    #[serde(default)]
    personality: String,
    #[serde(default)]
    abilities: Vec<String>,
    #[serde(default)]
    affiliations: Vec<String>,
    #[serde(default)]
    goals: String,
    #[serde(default)]
    arc: String,
    #[serde(default)]
    prominence: u8,
}

struct Observation {
    pages: Vec<usize>,
    descriptions: Vec<String>,
    roles: Vec<String>,
}

fn gather_observations(readings: &[PageReading]) -> BTreeMap<String, Observation> {
    let mut map: BTreeMap<String, Observation> = BTreeMap::new();
    for reading in readings {
        for sighting in &reading.characters_present {
            let label = sighting.name.trim();
            if label.is_empty() || label.eq_ignore_ascii_case("unknown") {
                continue;
            }
            let entry = map.entry(label.to_string()).or_insert_with(|| Observation {
                pages: Vec::new(),
                descriptions: Vec::new(),
                roles: Vec::new(),
            });
            entry.pages.push(reading.page_index);
            if !sighting.appearance.trim().is_empty()
                && !entry.descriptions.contains(&sighting.appearance)
            {
                entry.descriptions.push(sighting.appearance.clone());
            }
            if !sighting.role_guess.trim().is_empty() {
                entry.roles.push(sighting.role_guess.clone());
            }
        }
        // Speakers count as sightings too; a character can talk off-panel.
        for panel in &reading.panels {
            for line in &panel.dialogue {
                let speaker = line.speaker.trim();
                if speaker.is_empty() || speaker.eq_ignore_ascii_case("unknown") {
                    continue;
                }
                let entry = map.entry(speaker.to_string()).or_insert_with(|| Observation {
                    pages: Vec::new(),
                    descriptions: Vec::new(),
                    roles: Vec::new(),
                });
                entry.pages.push(reading.page_index);
            }
        }
    }
    for obs in map.values_mut() {
        obs.pages.sort_unstable();
        obs.pages.dedup();
    }
    map
}

fn observations_blob(map: &BTreeMap<String, Observation>) -> String {
    let mut entries: Vec<(&String, &Observation)> = map.iter().collect();
    entries.sort_by(|a, b| b.1.pages.len().cmp(&a.1.pages.len()));
    entries
        .iter()
        .take(90)
        .map(|(label, obs)| {
            let pages: Vec<String> = obs.pages.iter().take(14).map(|p| p.to_string()).collect();
            let roles = {
                let mut counts: BTreeMap<&str, usize> = BTreeMap::new();
                for r in &obs.roles {
                    *counts.entry(r.as_str()).or_insert(0) += 1;
                }
                counts
                    .into_iter()
                    .map(|(r, c)| format!("{r} x{c}"))
                    .collect::<Vec<_>>()
                    .join(", ")
            };
            format!(
                "- \"{label}\" | {} page(s) | pages: {} | role guesses: {} | seen as: {}",
                obs.pages.len(),
                pages.join(", "),
                if roles.is_empty() { "none".into() } else { roles },
                if obs.descriptions.is_empty() {
                    "no description recorded".to_string()
                } else {
                    util::truncate(&obs.descriptions.join(" / "), 320)
                }
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn build_characters(
    result: CastResult,
    observations: &BTreeMap<String, Observation>,
) -> (Vec<Character>, Vec<Relationship>) {
    let mut characters = Vec::new();
    for raw in result.characters {
        if raw.name.trim().is_empty() {
            continue;
        }
        // Union the pages of the canonical name and every alias.
        let mut page_refs: Vec<usize> = Vec::new();
        for label in std::iter::once(&raw.name).chain(raw.aliases.iter()) {
            if let Some(obs) = observations.get(label.trim()) {
                page_refs.extend(obs.pages.iter().copied());
            }
        }
        page_refs.sort_unstable();
        page_refs.dedup();

        let first_seen = page_refs.first().copied().unwrap_or(0);
        characters.push(Character {
            id: util::new_id("char"),
            name: raw.name.trim().to_string(),
            aliases: raw
                .aliases
                .into_iter()
                .map(|a| a.trim().to_string())
                .filter(|a| !a.is_empty())
                .collect(),
            role: raw.role,
            appearance: raw.appearance,
            personality: raw.personality,
            abilities: raw.abilities,
            affiliations: raw.affiliations,
            goals: raw.goals,
            arc: raw.arc,
            first_seen_page: first_seen,
            appearance_count: page_refs.len(),
            prominence: if raw.prominence == 0 {
                ((page_refs.len() as f32 / observations.len().max(1) as f32) * 100.0).min(100.0)
                    as u8
            } else {
                raw.prominence
            },
            page_refs,
            portrait_path: None,
        });
    }
    characters.sort_by(|a, b| b.prominence.cmp(&a.prominence));
    (characters, result.relationships)
}

// ---------------------------------------------------------------------------
// Pass 4 - portraits
// ---------------------------------------------------------------------------

/// Copy the best available panel of each character into `portraits/`.
fn assign_portraits(project: &mut Project) {
    let paths = Paths::new(&project.meta.root);
    let _ = std::fs::create_dir_all(paths.portraits());

    let readings: HashMap<usize, &PageReading> = project
        .bible
        .readings
        .iter()
        .map(|r| (r.page_index, r))
        .collect();
    let pages: HashMap<usize, &PageAsset> =
        project.source.pages.iter().map(|p| (p.index, p)).collect();

    let mut assignments: Vec<(usize, String)> = Vec::new();
    for (char_idx, character) in project.bible.characters.iter().enumerate() {
        let mut names: Vec<String> = vec![character.name.to_ascii_lowercase()];
        names.extend(character.aliases.iter().map(|a| a.to_ascii_lowercase()));

        let mut best: Option<(i32, String)> = None;
        for page_index in &character.page_refs {
            let (Some(reading), Some(page)) = (readings.get(page_index), pages.get(page_index))
            else {
                continue;
            };
            for panel in &reading.panels {
                let mentions = panel
                    .characters
                    .iter()
                    .any(|c| names.contains(&c.trim().to_ascii_lowercase()));
                if !mentions {
                    continue;
                }
                let Some(asset) = page.panels.get(panel.index) else {
                    continue;
                };
                // Prefer a close-up where this character is alone.
                let mut score = panel.significance as i32;
                if matches!(panel.shot.as_str(), "close_up" | "reaction") {
                    score += 40;
                }
                if panel.characters.len() == 1 {
                    score += 30;
                }
                if asset.area_ratio < 0.02 {
                    score -= 25; // tiny crops make poor portraits
                }
                if best.as_ref().map(|(s, _)| score > *s).unwrap_or(true) {
                    best = Some((score, asset.path.clone()));
                }
            }
        }

        if let Some((_, src)) = best {
            let dest = paths
                .portraits()
                .join(format!("{}.png", util::slugify(&character.name)));
            if std::fs::copy(&src, &dest).is_ok() {
                assignments.push((char_idx, dest.to_string_lossy().to_string()));
            }
        }
    }

    for (idx, path) in assignments {
        if let Some(character) = project.bible.characters.get_mut(idx) {
            character.portrait_path = Some(path);
        }
    }
}

// ---------------------------------------------------------------------------
// Pass 5 - event graph
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct GraphResult {
    #[serde(default)]
    events: Vec<RawGraphEvent>,
    #[serde(default)]
    threads: Vec<RawThread>,
    #[serde(default)]
    foreshadowing: Vec<ForeshadowPair>,
}

#[derive(Debug, Deserialize)]
struct RawGraphEvent {
    #[serde(default)]
    id: String,
    #[serde(default)]
    summary: String,
    #[serde(default)]
    kind: String,
    #[serde(default)]
    actors: Vec<String>,
    #[serde(default)]
    page_start: usize,
    #[serde(default)]
    page_end: usize,
    #[serde(default)]
    importance: u8,
    #[serde(default)]
    is_flashback: bool,
    #[serde(default)]
    caused_by: Vec<String>,
    #[serde(default)]
    stakes: String,
    #[serde(default)]
    chronological_order: usize,
}

#[derive(Debug, Deserialize)]
struct RawThread {
    #[serde(default)]
    question: String,
    #[serde(default)]
    status: String,
    #[serde(default)]
    opened_page: usize,
    #[serde(default)]
    resolved_page: Option<usize>,
    #[serde(default)]
    notes: String,
}

fn beats_blob(readings: &[PageReading]) -> String {
    let mut lines = Vec::new();
    for reading in readings {
        for event in &reading.events {
            if event.summary.trim().is_empty() {
                continue;
            }
            lines.push(format!(
                "[{}] ({}) {} - {}",
                reading.page_index,
                event.importance,
                if event.kind.is_empty() { "other" } else { &event.kind },
                event.summary
            ));
        }
    }
    lines.join("\n")
}

fn cast_blob(bible: &StoryBible) -> String {
    bible
        .characters
        .iter()
        .take(40)
        .map(|c| {
            let aliases = if c.aliases.is_empty() {
                String::new()
            } else {
                format!(" (also: {})", c.aliases.join(", "))
            };
            format!("- {}{} - {}", c.name, aliases, c.role)
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Everything the writing passes need to know about a page range, in prose.
pub fn detail_blob(project: &Project, from_page: usize, to_page: usize) -> String {
    let mut out = String::new();
    for reading in project
        .bible
        .readings
        .iter()
        .filter(|r| r.page_index >= from_page && r.page_index <= to_page)
    {
        if matches!(reading.role, PageRole::AdOrFiller | PageRole::AuthorNote) {
            continue;
        }
        out.push_str(&format!("\n[page {}]", reading.page_index));
        if reading.is_flashback {
            out.push_str(" (flashback)");
        }
        if !reading.location.trim().is_empty() {
            out.push_str(&format!(" location: {}", reading.location));
        }
        out.push('\n');
        for panel in &reading.panels {
            if !panel.action.trim().is_empty() {
                out.push_str(&format!("  - {}\n", panel.action));
            } else if !panel.description.trim().is_empty() {
                out.push_str(&format!("  - {}\n", panel.description));
            }
            for line in &panel.dialogue {
                if line.text.trim().is_empty() {
                    continue;
                }
                let speaker = if line.speaker.trim().is_empty() {
                    "unknown"
                } else {
                    line.speaker.trim()
                };
                out.push_str(&format!("      {speaker}: \"{}\"\n", line.text.trim()));
            }
        }
        if !reading.notes.trim().is_empty() {
            out.push_str(&format!("  note: {}\n", reading.notes));
        }
    }
    out
}

// ---------------------------------------------------------------------------
// Pass 6 - chapter synthesis
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct ChapterResult {
    #[serde(default)]
    synopsis: String,
    #[serde(default)]
    beats: Vec<RawBeat>,
    #[serde(default)]
    cliffhanger: String,
    #[serde(default)]
    tone: String,
}

#[derive(Debug, Deserialize)]
struct RawBeat {
    #[serde(default)]
    text: String,
    #[serde(default)]
    importance: u8,
    #[serde(default)]
    characters: Vec<String>,
    #[serde(default)]
    quote: Option<String>,
    #[serde(default)]
    page_refs: Vec<usize>,
}

/// Map a beat's page references onto the most significant panels on those pages,
/// so the storyboard and video have something to show for every line.
fn panels_for_beat(project: &Project, page_refs: &[usize]) -> Vec<PanelRef> {
    let readings: HashMap<usize, &PageReading> = project
        .bible
        .readings
        .iter()
        .map(|r| (r.page_index, r))
        .collect();
    let mut refs = Vec::new();
    for page in page_refs {
        let Some(reading) = readings.get(page) else {
            continue;
        };
        let page_asset = project.source.pages.iter().find(|p| p.index == *page);
        let panel_count = page_asset.map(|p| p.panels.len()).unwrap_or(0);

        let mut ranked: Vec<&PanelReading> = reading.panels.iter().collect();
        ranked.sort_by(|a, b| b.significance.cmp(&a.significance));
        for panel in ranked.into_iter().take(2) {
            if panel_count == 0 || panel.index < panel_count {
                refs.push(PanelRef {
                    page: *page,
                    panel: panel.index,
                });
            }
        }
        if refs.iter().all(|r| r.page != *page) {
            // No usable panel record; fall back to the whole page.
            refs.push(PanelRef {
                page: *page,
                panel: usize::MAX,
            });
        }
    }
    refs
}

// ---------------------------------------------------------------------------
// Pass 7 - work metadata
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct WorkResult {
    #[serde(default)]
    title: String,
    #[serde(default)]
    genres: Vec<String>,
    #[serde(default)]
    premise: String,
    #[serde(default)]
    setting: String,
    #[serde(default)]
    power_system: String,
    #[serde(default)]
    tone: String,
}

// ---------------------------------------------------------------------------
// Orchestration
// ---------------------------------------------------------------------------

pub async fn run(
    project: &mut Project,
    settings: &Settings,
    emitter: &Emitter,
) -> Result<()> {
    if project.source.pages.is_empty() {
        return Err(anyhow!("This project has no pages yet. Add a source first."));
    }

    let (vision_client, vision_model) = settings.resolve(ModelRole::Vision)?;
    let (reason_client, reason_model) = settings.resolve(ModelRole::Reasoning)?;
    let vision_client = Arc::new(vision_client);

    // ---- Pass 1: perception -------------------------------------------------
    let batch_size = settings.pages_per_batch.clamp(1, 12);
    let batches: Vec<Vec<PageAsset>> = project
        .source
        .pages
        .chunks(batch_size)
        .map(|c| c.to_vec())
        .collect();
    let total_batches = batches.len();
    emitter.stage(
        "perception",
        format!(
            "Reading {} pages in {} batch(es) with {}",
            project.source.pages.len(),
            total_batches,
            vision_model
        ),
    );

    let mut readings: Vec<PageReading> = Vec::new();
    let mut first_context = String::new();

    // The first batch runs alone so later batches inherit its name spellings.
    let mut remaining = batches;
    if !remaining.is_empty() {
        let first = remaining.remove(0);
        let (batch_readings, input, output) = perceive_batch(
            &vision_client,
            &vision_model,
            &project.source,
            first,
            String::new(),
            settings.panel_level_vision,
        )
        .await?;
        project.usage.add("perception", &vision_model, input, output);
        first_context = condense_for_context(&batch_readings, 12);
        readings.extend(batch_readings);
        emitter.progress(
            "perception",
            1,
            total_batches,
            format!("Read batch 1 of {total_batches}"),
        );
    }

    if !remaining.is_empty() {
        let concurrency = settings.concurrency.clamp(1, 8);
        let source = Arc::new(project.source.clone());
        let context = Arc::new(first_context.clone());
        let model = vision_model.clone();
        let panel_level = settings.panel_level_vision;

        let results: Vec<Result<(Vec<PageReading>, u64, u64)>> = stream::iter(
            remaining.into_iter().enumerate(),
        )
        .map(|(i, batch)| {
            let client = Arc::clone(&vision_client);
            let source = Arc::clone(&source);
            let context = Arc::clone(&context);
            let model = model.clone();
            async move {
                let out = perceive_batch(
                    &client,
                    &model,
                    &source,
                    batch,
                    context.as_str().to_string(),
                    panel_level,
                )
                .await;
                (i, out)
            }
        })
        .buffer_unordered(concurrency)
        .enumerate()
        .map(|(done, (_i, out))| {
            emitter.progress(
                "perception",
                done + 2,
                total_batches,
                format!("Read batch {} of {}", done + 2, total_batches),
            );
            out
        })
        .collect()
        .await;

        let mut failures = 0;
        for result in results {
            match result {
                Ok((batch_readings, input, output)) => {
                    project.usage.add("perception", &vision_model, input, output);
                    readings.extend(batch_readings);
                }
                Err(err) => {
                    failures += 1;
                    emitter.warn(format!("A page batch failed and was skipped: {err}"));
                }
            }
        }
        if failures > 0 && readings.is_empty() {
            return Err(anyhow!(
                "Every page batch failed. Check the provider key and selected model in Settings."
            ));
        }
    }

    readings.sort_by_key(|r| r.page_index);
    readings.dedup_by_key(|r| r.page_index);
    project.bible.readings = readings;
    emitter.info(format!(
        "Perception complete: {} pages read, {} panels described",
        project.bible.readings.len(),
        project
            .bible
            .readings
            .iter()
            .map(|r| r.panels.len())
            .sum::<usize>()
    ));

    // ---- Pass 2: chapter boundaries ----------------------------------------
    emitter.stage("chapters", "Confirming chapter boundaries");
    let readings_snapshot = project.bible.readings.clone();
    recut_chapters(&mut project.source, &readings_snapshot, emitter);

    // ---- Pass 3: cast -------------------------------------------------------
    emitter.stage("cast", "Resolving the cast");
    let observations = gather_observations(&project.bible.readings);
    if observations.is_empty() {
        emitter.warn("No characters were identified on any page.");
    } else {
        let blob = observations_blob(&observations);
        match ask_json::<CastResult>(
            &reason_client,
            &reason_model,
            prompts::CAST_SYSTEM,
            vec![ChatMessage::user_text(prompts::cast_user(&blob))],
            8192,
        )
        .await
        {
            Ok((result, input, output)) => {
                project.usage.add("cast", &reason_model, input, output);
                let (characters, relationships) = build_characters(result, &observations);
                emitter.info(format!(
                    "Cast resolved: {} characters, {} relationships",
                    characters.len(),
                    relationships.len()
                ));
                project.bible.characters = characters;
                project.bible.relationships = relationships;
            }
            Err(err) => emitter.warn(format!("Cast resolution failed: {err}")),
        }
    }

    // ---- Pass 4: portraits --------------------------------------------------
    emitter.stage("portraits", "Picking character portraits");
    assign_portraits(project);

    // ---- Pass 5: event graph ------------------------------------------------
    emitter.stage("graph", "Building the event graph");
    let beats = beats_blob(&project.bible.readings);
    if beats.trim().is_empty() {
        emitter.warn("No story beats were recorded, skipping the event graph.");
    } else {
        let cast = cast_blob(&project.bible);
        match ask_json::<GraphResult>(
            &reason_client,
            &reason_model,
            prompts::GRAPH_SYSTEM,
            vec![ChatMessage::user_text(prompts::graph_user(
                &util::truncate(&beats, 60_000),
                &cast,
            ))],
            8192,
        )
        .await
        {
            Ok((result, input, output)) => {
                project.usage.add("graph", &reason_model, input, output);
                let chapter_of = |page: usize| -> usize {
                    project
                        .source
                        .chapters
                        .iter()
                        .find(|c| page >= c.first_page && page <= c.last_page)
                        .map(|c| c.index)
                        .unwrap_or(0)
                };
                let mut id_map: HashMap<String, String> = HashMap::new();
                let mut events: Vec<StoryEvent> = Vec::new();
                for (i, raw) in result.events.iter().enumerate() {
                    let id = util::new_id("evt");
                    if !raw.id.is_empty() {
                        id_map.insert(raw.id.clone(), id.clone());
                    }
                    events.push(StoryEvent {
                        id,
                        summary: raw.summary.clone(),
                        kind: raw.kind.clone(),
                        actors: raw.actors.clone(),
                        chapter_index: chapter_of(raw.page_start),
                        page_start: raw.page_start,
                        page_end: raw.page_end.max(raw.page_start),
                        panel_refs: Vec::new(),
                        importance: raw.importance,
                        is_flashback: raw.is_flashback,
                        caused_by: Vec::new(),
                        stakes: raw.stakes.clone(),
                        presentation_order: i + 1,
                        chronological_order: if raw.chronological_order == 0 {
                            i + 1
                        } else {
                            raw.chronological_order
                        },
                    });
                }
                // Rewrite causal links from the model's temporary ids to ours.
                for (event, raw) in events.iter_mut().zip(result.events.iter()) {
                    event.caused_by = raw
                        .caused_by
                        .iter()
                        .filter_map(|old| id_map.get(old).cloned())
                        .collect();
                }
                emitter.info(format!(
                    "Event graph: {} events, {} open threads, {} foreshadowing pairs",
                    events.len(),
                    result.threads.len(),
                    result.foreshadowing.len()
                ));
                project.bible.events = events;
                project.bible.threads = result
                    .threads
                    .into_iter()
                    .map(|t| StoryThread {
                        id: util::new_id("thr"),
                        question: t.question,
                        status: t.status,
                        opened_page: t.opened_page,
                        resolved_page: t.resolved_page,
                        notes: t.notes,
                    })
                    .collect();
                project.bible.foreshadowing = result.foreshadowing;
            }
            Err(err) => emitter.warn(format!("Event graph failed: {err}")),
        }
    }

    // ---- Pass 6: chapter synthesis -----------------------------------------
    let chapters = project.source.chapters.clone();
    let total_chapters = chapters.len();
    emitter.stage(
        "synthesis",
        format!("Summarizing {total_chapters} chapter(s)"),
    );
    let mut summaries: Vec<ChapterSummary> = Vec::new();
    let mut continuity = String::new();

    for (i, chapter) in chapters.iter().enumerate() {
        let label = chapter
            .title
            .clone()
            .unwrap_or_else(|| format!("Chapter {}", i + 1));
        emitter.progress(
            "synthesis",
            i,
            total_chapters,
            format!("Summarizing {label}"),
        );

        let detail = detail_blob(project, chapter.first_page, chapter.last_page);
        if detail.trim().is_empty() {
            continue;
        }
        let result = ask_json::<ChapterResult>(
            &reason_client,
            &reason_model,
            prompts::CHAPTER_SYSTEM,
            vec![ChatMessage::user_text(prompts::chapter_user(
                &label,
                &util::truncate(&detail, 90_000),
                &util::truncate(&continuity, 6_000),
            ))],
            8192,
        )
        .await;

        match result {
            Ok((chapter_result, input, output)) => {
                project.usage.add("synthesis", &reason_model, input, output);
                let beats: Vec<Beat> = chapter_result
                    .beats
                    .iter()
                    .map(|raw| Beat {
                        id: util::new_id("beat"),
                        text: raw.text.clone(),
                        importance: raw.importance,
                        panel_refs: panels_for_beat(project, &raw.page_refs),
                        quote: raw
                            .quote
                            .clone()
                            .filter(|q| !q.trim().is_empty() && q.trim() != "null"),
                        characters: raw.characters.clone(),
                    })
                    .collect();

                if !continuity.is_empty() {
                    continuity.push_str("\n\n");
                }
                continuity.push_str(&format!("{label}: {}", chapter_result.synopsis));

                summaries.push(ChapterSummary {
                    chapter_index: chapter.index,
                    number: chapter.number,
                    title: Some(label.clone()),
                    synopsis: chapter_result.synopsis,
                    beats,
                    cliffhanger: chapter_result.cliffhanger,
                    tone: chapter_result.tone,
                    page_start: chapter.first_page,
                    page_end: chapter.last_page,
                });
            }
            Err(err) => emitter.warn(format!("Could not summarize {label}: {err}")),
        }
    }
    emitter.progress(
        "synthesis",
        total_chapters,
        total_chapters,
        "Chapter summaries complete",
    );
    project.bible.chapters = summaries;
    project.bible.continuity = continuity;

    // ---- Pass 7: work metadata ---------------------------------------------
    emitter.stage("classify", "Classifying the series");
    let chapters_blob = project
        .bible
        .chapters
        .iter()
        .map(|c| {
            format!(
                "- {}: {}",
                c.title.clone().unwrap_or_else(|| "Chapter".into()),
                util::truncate(&c.synopsis, 900)
            )
        })
        .collect::<Vec<_>>()
        .join("\n");

    match ask_json::<WorkResult>(
        &reason_client,
        &reason_model,
        prompts::WORK_SYSTEM,
        vec![ChatMessage::user_text(prompts::work_user(
            &cast_blob(&project.bible),
            &util::truncate(&chapters_blob, 40_000),
            &project.meta.name,
        ))],
        2048,
    )
    .await
    {
        Ok((work, input, output)) => {
            project.usage.add("classify", &reason_model, input, output);
            project.bible.work = WorkMeta {
                title: if work.title.trim().is_empty() {
                    project.meta.name.clone()
                } else {
                    work.title
                },
                genres: work.genres,
                premise: work.premise,
                setting: work.setting,
                power_system: work.power_system,
                tone: work.tone,
            };
            emitter.info(format!(
                "Classified as {}",
                if project.bible.work.genres.is_empty() {
                    project.bible.work.tone.clone()
                } else {
                    project.bible.work.genres.join(", ")
                }
            ));
        }
        Err(err) => emitter.warn(format!("Series classification failed: {err}")),
    }

    // ---- Pass 8: save -------------------------------------------------------
    project::touch(project);
    project::save(project)?;
    emitter.done(format!(
        "Analysis complete. {} pages, {} characters, {} events, {} chapters. {} tokens used.",
        project.bible.readings.len(),
        project.bible.characters.len(),
        project.bible.events.len(),
        project.bible.chapters.len(),
        project.usage.total_tokens()
    ));
    Ok(())
}
