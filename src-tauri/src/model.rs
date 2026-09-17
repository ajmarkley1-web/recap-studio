//! Core data model for Recap Studio.
//!
//! The whole app revolves around a `Project`, which owns a `Source` (the pages that
//! were ingested), a `StoryBible` (everything the analyze engine learned) and a
//! `ScriptBundle` (the panel-by-panel narration written from both).

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

// ---------------------------------------------------------------------------
// Format / reading direction
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum SourceFormat {
    /// Vertical-scroll webtoon strips. Panels stack top to bottom.
    #[default]
    Manhwa,
    /// Printed manga pages. Panels read right-to-left, top-to-bottom.
    Manga,
    /// Western comic pages. Panels read left-to-right, top-to-bottom.
    Comic,
}

impl SourceFormat {
    pub fn reading_order(self) -> ReadingOrder {
        match self {
            SourceFormat::Manhwa => ReadingOrder::TopToBottom,
            SourceFormat::Manga => ReadingOrder::RightToLeft,
            SourceFormat::Comic => ReadingOrder::LeftToRight,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReadingOrder {
    RightToLeft,
    LeftToRight,
    TopToBottom,
}

// ---------------------------------------------------------------------------
// Source material
// ---------------------------------------------------------------------------

/// A single ingested image: one PDF page, one webtoon strip, or one panel file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PageAsset {
    pub id: String,
    /// Global index across the whole project, in reading order.
    pub index: usize,
    /// Index of the chapter this page belongs to (into `Source::chapters`).
    pub chapter_index: usize,
    /// Position of this page inside its chapter.
    pub page_in_chapter: usize,
    /// Absolute path of the full-resolution image on disk.
    pub path: String,
    /// Absolute path of the downscaled copy used for vision calls.
    pub thumb_path: String,
    pub width: u32,
    pub height: u32,
    /// Original file name or `page-0012.png` for PDF-derived pages.
    pub label: String,
    /// Panels detected on this page, in reading order.
    #[serde(default)]
    pub panels: Vec<PanelAsset>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PanelAsset {
    pub id: String,
    /// Index within the owning page, already sorted into reading order.
    pub index: usize,
    pub path: String,
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
    /// Fraction of the page area this panel covers (0.0 - 1.0).
    pub area_ratio: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChapterRef {
    pub index: usize,
    /// Chapter number as printed, when we could work it out.
    pub number: Option<f32>,
    pub title: Option<String>,
    /// Where the chapter came from: a folder name, a PDF, or a detected break.
    pub origin: String,
    pub first_page: usize,
    pub last_page: usize,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Source {
    pub format: SourceFormat,
    pub pages: Vec<PageAsset>,
    pub chapters: Vec<ChapterRef>,
    /// How chapters were determined, for display in the UI.
    #[serde(default)]
    pub chapter_strategy: String,
    #[serde(default)]
    pub total_panels: usize,
}

// ---------------------------------------------------------------------------
// Pass 1 - raw perception of a single page
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum PageRole {
    #[default]
    Story,
    ChapterStart,
    CoverOrTitle,
    CharacterProfile,
    RecapPage,
    AuthorNote,
    AdOrFiller,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DialogueLine {
    #[serde(default)]
    pub speaker: String,
    #[serde(default)]
    pub text: String,
    /// `speech`, `thought`, `narration`, `shout`, `whisper`, `system`.
    #[serde(default)]
    pub kind: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PanelReading {
    #[serde(default)]
    pub index: usize,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub characters: Vec<String>,
    #[serde(default)]
    pub action: String,
    #[serde(default)]
    pub dialogue: Vec<DialogueLine>,
    #[serde(default)]
    pub sfx: Vec<String>,
    #[serde(default)]
    pub emotion: String,
    /// 0-100. How much this panel matters to the plot.
    #[serde(default)]
    pub significance: u8,
    /// Shot framing: `close_up`, `wide`, `splash`, `reaction`, `establishing`.
    #[serde(default)]
    pub shot: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PageReading {
    pub page_index: usize,
    #[serde(default)]
    pub role: PageRole,
    #[serde(default)]
    pub chapter_number: Option<f32>,
    #[serde(default)]
    pub chapter_title: Option<String>,
    #[serde(default)]
    pub location: String,
    #[serde(default)]
    pub time_of_day: String,
    #[serde(default)]
    pub is_flashback: bool,
    #[serde(default)]
    pub panels: Vec<PanelReading>,
    #[serde(default)]
    pub characters_present: Vec<CharacterSighting>,
    #[serde(default)]
    pub raw_text: String,
    /// Beats extracted straight from this page, merged later into the event graph.
    #[serde(default)]
    pub events: Vec<RawEvent>,
    #[serde(default)]
    pub notes: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CharacterSighting {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub appearance: String,
    #[serde(default)]
    pub role_guess: String,
    #[serde(default)]
    pub confidence: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RawEvent {
    #[serde(default)]
    pub summary: String,
    /// `fight`, `revelation`, `betrayal`, `power_up`, `death`, `confession`,
    /// `travel`, `flashback`, `contract`, `training`, `comedy`, `cliffhanger`, ...
    #[serde(default)]
    pub kind: String,
    #[serde(default)]
    pub actors: Vec<String>,
    #[serde(default)]
    pub importance: u8,
    #[serde(default)]
    pub panel_refs: Vec<usize>,
}

// ---------------------------------------------------------------------------
// Pass 2/3 - the story bible
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Character {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub aliases: Vec<String>,
    /// `protagonist`, `deuteragonist`, `antagonist`, `support`, `minor`.
    #[serde(default)]
    pub role: String,
    #[serde(default)]
    pub appearance: String,
    #[serde(default)]
    pub personality: String,
    #[serde(default)]
    pub abilities: Vec<String>,
    #[serde(default)]
    pub affiliations: Vec<String>,
    #[serde(default)]
    pub goals: String,
    #[serde(default)]
    pub arc: String,
    #[serde(default)]
    pub first_seen_page: usize,
    #[serde(default)]
    pub appearance_count: usize,
    /// Pages where this character is on screen, for the evidence view.
    #[serde(default)]
    pub page_refs: Vec<usize>,
    /// 0-100 importance, drives ordering in the UI and name usage in the script.
    #[serde(default)]
    pub prominence: u8,
    /// Path to a cropped panel used as the character's portrait in the UI.
    #[serde(default)]
    pub portrait_path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Relationship {
    pub from: String,
    pub to: String,
    /// `ally`, `rival`, `enemy`, `family`, `romantic`, `mentor`, `subordinate`, `unknown`.
    #[serde(default)]
    pub kind: String,
    #[serde(default)]
    pub description: String,
    /// -100 (hostile) to 100 (devoted).
    #[serde(default)]
    pub sentiment: i8,
    #[serde(default)]
    pub evidence_pages: Vec<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct NamedEntity {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub page_refs: Vec<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct StoryEvent {
    pub id: String,
    pub summary: String,
    #[serde(default)]
    pub kind: String,
    #[serde(default)]
    pub actors: Vec<String>,
    #[serde(default)]
    pub chapter_index: usize,
    #[serde(default)]
    pub page_start: usize,
    #[serde(default)]
    pub page_end: usize,
    #[serde(default)]
    pub panel_refs: Vec<PanelRef>,
    #[serde(default)]
    pub importance: u8,
    #[serde(default)]
    pub is_flashback: bool,
    /// Ids of events this one directly follows from.
    #[serde(default)]
    pub caused_by: Vec<String>,
    #[serde(default)]
    pub stakes: String,
    /// Presentation order (as drawn) vs chronological order (as it happened).
    #[serde(default)]
    pub presentation_order: usize,
    #[serde(default)]
    pub chronological_order: usize,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct PanelRef {
    pub page: usize,
    pub panel: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct StoryThread {
    pub id: String,
    pub question: String,
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub opened_page: usize,
    #[serde(default)]
    pub resolved_page: Option<usize>,
    #[serde(default)]
    pub notes: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ForeshadowPair {
    pub setup: String,
    pub setup_page: usize,
    #[serde(default)]
    pub payoff: Option<String>,
    #[serde(default)]
    pub payoff_page: Option<usize>,
    #[serde(default)]
    pub confidence: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ChapterSummary {
    pub chapter_index: usize,
    #[serde(default)]
    pub number: Option<f32>,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub synopsis: String,
    #[serde(default)]
    pub beats: Vec<Beat>,
    #[serde(default)]
    pub cliffhanger: String,
    #[serde(default)]
    pub tone: String,
    #[serde(default)]
    pub page_start: usize,
    #[serde(default)]
    pub page_end: usize,
}

/// A single plot beat. The draft script is built from these in order, so each
/// one carries the panel evidence that justifies it.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Beat {
    pub id: String,
    pub text: String,
    #[serde(default)]
    pub importance: u8,
    #[serde(default)]
    pub panel_refs: Vec<PanelRef>,
    #[serde(default)]
    pub quote: Option<String>,
    #[serde(default)]
    pub characters: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct WorkMeta {
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub genres: Vec<String>,
    #[serde(default)]
    pub premise: String,
    #[serde(default)]
    pub setting: String,
    #[serde(default)]
    pub power_system: String,
    #[serde(default)]
    pub tone: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct StoryBible {
    #[serde(default)]
    pub work: WorkMeta,
    #[serde(default)]
    pub characters: Vec<Character>,
    #[serde(default)]
    pub relationships: Vec<Relationship>,
    #[serde(default)]
    pub locations: Vec<NamedEntity>,
    #[serde(default)]
    pub factions: Vec<NamedEntity>,
    #[serde(default)]
    pub items: Vec<NamedEntity>,
    #[serde(default)]
    pub glossary: Vec<NamedEntity>,
    #[serde(default)]
    pub events: Vec<StoryEvent>,
    #[serde(default)]
    pub threads: Vec<StoryThread>,
    #[serde(default)]
    pub foreshadowing: Vec<ForeshadowPair>,
    #[serde(default)]
    pub chapters: Vec<ChapterSummary>,
    /// Raw per-page perception, kept so the UI can show evidence for anything.
    #[serde(default)]
    pub readings: Vec<PageReading>,
    /// Rolling continuity summary carried into later chapters.
    #[serde(default)]
    pub continuity: String,
}

impl StoryBible {
    pub fn character_by_name(&self, name: &str) -> Option<&Character> {
        let needle = name.trim().to_ascii_lowercase();
        self.characters.iter().find(|c| {
            c.name.to_ascii_lowercase() == needle
                || c.aliases.iter().any(|a| a.to_ascii_lowercase() == needle)
        })
    }

    pub fn protagonist(&self) -> Option<&Character> {
        self.characters
            .iter()
            .find(|c| c.role.eq_ignore_ascii_case("protagonist"))
            .or_else(|| self.characters.iter().max_by_key(|c| c.prominence))
    }
}

// ---------------------------------------------------------------------------
// Scripts
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ScriptBundle {
    /// The narration exactly as the model wrote it, panel tags still in place.
    /// This is the copy the storyboard and the coverage check are built from.
    #[serde(default)]
    pub tagged_script: String,
    /// The reading copy: the same prose with the tags stripped out.
    #[serde(default)]
    pub final_script: String,
    #[serde(default)]
    pub final_word_count: usize,
    /// Panels inside this scope, and how many of them the narration covers.
    /// They are equal when nothing was skipped, which is the whole point.
    #[serde(default)]
    pub panel_count: usize,
    #[serde(default)]
    pub narrated_panel_count: usize,
    /// Panels the narration never reached, as `page:panel`.
    #[serde(default)]
    pub missing_panels: Vec<PanelRef>,
    #[serde(default)]
    pub compliance: Option<ComplianceReport>,
    /// One shot per panel, in reading order, used by the video renderer.
    #[serde(default)]
    pub storyboard: Vec<StoryboardShot>,
    /// Scope this script covers.
    #[serde(default)]
    pub scope: ScriptScope,
    #[serde(default)]
    pub generated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ScriptScope {
    #[default]
    WholeProject,
    Chapter(usize),
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct StoryboardShot {
    pub index: usize,
    pub text: String,
    /// The panel this shot narrates. A shot is one panel, so this holds one
    /// entry; it stays a list only so a hand-edited storyboard can group panels.
    #[serde(default)]
    pub panels: Vec<PanelRef>,
    /// Resolved image paths, so the UI does not have to walk the source tree.
    #[serde(default)]
    pub image_paths: Vec<String>,
    #[serde(default)]
    pub audio_path: Option<String>,
    #[serde(default)]
    pub duration_secs: Option<f32>,
}

// ---------------------------------------------------------------------------
// Compliance
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuleStatus {
    Pass,
    Warn,
    Fail,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuleResult {
    pub id: String,
    pub label: String,
    pub status: RuleStatus,
    pub detail: String,
    /// Exact offending snippets so the editor can highlight them.
    #[serde(default)]
    pub offenders: Vec<String>,
    /// Which part of the narrator prompt this rule comes from.
    pub source: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ComplianceReport {
    pub score: u8,
    pub rules: Vec<RuleResult>,
    #[serde(default)]
    pub grounding: Option<GroundingReport>,
    #[serde(default)]
    pub checked_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct GroundingReport {
    /// Sentences the grounding pass could not trace back to the source.
    pub unsupported: Vec<UnsupportedClaim>,
    pub checked_sentences: usize,
    #[serde(default)]
    pub summary: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct UnsupportedClaim {
    pub sentence: String,
    pub reason: String,
    #[serde(default)]
    pub severity: String,
}

// ---------------------------------------------------------------------------
// Project
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectMeta {
    pub id: String,
    pub name: String,
    pub created_at: String,
    pub updated_at: String,
    pub root: String,
    #[serde(default)]
    pub format: SourceFormat,
    #[serde(default)]
    pub page_count: usize,
    #[serde(default)]
    pub chapter_count: usize,
    #[serde(default)]
    pub analyzed: bool,
    #[serde(default)]
    pub has_script: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Project {
    pub meta: ProjectMeta,
    #[serde(default)]
    pub source: Source,
    #[serde(default)]
    pub bible: StoryBible,
    #[serde(default)]
    pub scripts: BTreeMap<String, ScriptBundle>,
    /// Token + cost accounting across every pass.
    #[serde(default)]
    pub usage: UsageLedger,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct UsageLedger {
    #[serde(default)]
    pub entries: Vec<UsageEntry>,
}

impl UsageLedger {
    pub fn add(&mut self, stage: &str, model: &str, input: u64, output: u64) {
        self.entries.push(UsageEntry {
            stage: stage.to_string(),
            model: model.to_string(),
            input_tokens: input,
            output_tokens: output,
            at: crate::util::now_iso(),
        });
    }

    pub fn total_tokens(&self) -> u64 {
        self.entries
            .iter()
            .map(|e| e.input_tokens + e.output_tokens)
            .sum()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UsageEntry {
    pub stage: String,
    pub model: String,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub at: String,
}
