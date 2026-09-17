// Mirrors the Rust model in src-tauri/src/model.rs and settings.rs.
// Keep the two in sync: the serde representation is the contract.

export type SourceFormat = "manhwa" | "manga" | "comic";
export type ProviderId = "openai" | "anthropic" | "gemini" | "ollama";
export type ModelRole = "primary" | "vision" | "writer";
export type RuleStatus = "pass" | "warn" | "fail";
export type TtsProviderId = "none" | "elevenlabs" | "openai";

export type PageRole =
  | "story"
  | "chapter_start"
  | "cover_or_title"
  | "character_profile"
  | "recap_page"
  | "author_note"
  | "ad_or_filler";

/** Rust: `ScriptScope`. Externally tagged enum. */
export type ScriptScope = "whole_project" | { chapter: number };

export const wholeProject: ScriptScope = "whole_project";
export const chapterScope = (i: number): ScriptScope => ({ chapter: i });
export const scopeKey = (s: ScriptScope): string =>
  s === "whole_project" ? "all" : `ch${s.chapter}`;
export const isChapterScope = (
  s: ScriptScope
): s is { chapter: number } => typeof s === "object";

export interface PanelAsset {
  id: string;
  index: number;
  path: string;
  x: number;
  y: number;
  width: number;
  height: number;
  area_ratio: number;
}

export interface PageAsset {
  id: string;
  index: number;
  chapter_index: number;
  page_in_chapter: number;
  path: string;
  thumb_path: string;
  width: number;
  height: number;
  label: string;
  panels: PanelAsset[];
}

export interface ChapterRef {
  index: number;
  number: number | null;
  title: string | null;
  origin: string;
  first_page: number;
  last_page: number;
}

export interface Source {
  format: SourceFormat;
  pages: PageAsset[];
  chapters: ChapterRef[];
  chapter_strategy: string;
  total_panels: number;
}

export interface DialogueLine {
  speaker: string;
  text: string;
  kind: string;
}

export interface PanelReading {
  index: number;
  description: string;
  characters: string[];
  action: string;
  dialogue: DialogueLine[];
  sfx: string[];
  emotion: string;
  significance: number;
  shot: string;
}

export interface CharacterSighting {
  name: string;
  appearance: string;
  role_guess: string;
  confidence: number;
}

export interface RawEvent {
  summary: string;
  kind: string;
  actors: string[];
  importance: number;
  panel_refs: number[];
}

export interface PageReading {
  page_index: number;
  role: PageRole;
  chapter_number: number | null;
  chapter_title: string | null;
  location: string;
  time_of_day: string;
  is_flashback: boolean;
  panels: PanelReading[];
  characters_present: CharacterSighting[];
  raw_text: string;
  events: RawEvent[];
  notes: string;
}

export interface Character {
  id: string;
  name: string;
  aliases: string[];
  role: string;
  appearance: string;
  personality: string;
  abilities: string[];
  affiliations: string[];
  goals: string;
  arc: string;
  first_seen_page: number;
  appearance_count: number;
  page_refs: number[];
  prominence: number;
  portrait_path: string | null;
}

export interface Relationship {
  from: string;
  to: string;
  kind: string;
  description: string;
  sentiment: number;
  evidence_pages: number[];
}

export interface NamedEntity {
  id: string;
  name: string;
  description: string;
  page_refs: number[];
}

export interface PanelRef {
  page: number;
  panel: number;
}

export interface StoryEvent {
  id: string;
  summary: string;
  kind: string;
  actors: string[];
  chapter_index: number;
  page_start: number;
  page_end: number;
  panel_refs: PanelRef[];
  importance: number;
  is_flashback: boolean;
  caused_by: string[];
  stakes: string;
  presentation_order: number;
  chronological_order: number;
}

export interface StoryThread {
  id: string;
  question: string;
  status: string;
  opened_page: number;
  resolved_page: number | null;
  notes: string;
}

export interface ForeshadowPair {
  setup: string;
  setup_page: number;
  payoff: string | null;
  payoff_page: number | null;
  confidence: number;
}

export interface Beat {
  id: string;
  text: string;
  importance: number;
  panel_refs: PanelRef[];
  quote: string | null;
  characters: string[];
}

export interface ChapterSummary {
  chapter_index: number;
  number: number | null;
  title: string | null;
  synopsis: string;
  beats: Beat[];
  cliffhanger: string;
  tone: string;
  page_start: number;
  page_end: number;
}

export interface WorkMeta {
  title: string;
  genres: string[];
  premise: string;
  setting: string;
  power_system: string;
  tone: string;
}

export interface StoryBible {
  work: WorkMeta;
  characters: Character[];
  relationships: Relationship[];
  locations: NamedEntity[];
  factions: NamedEntity[];
  items: NamedEntity[];
  glossary: NamedEntity[];
  events: StoryEvent[];
  threads: StoryThread[];
  foreshadowing: ForeshadowPair[];
  chapters: ChapterSummary[];
  readings: PageReading[];
  continuity: string;
}

export interface RuleResult {
  id: string;
  label: string;
  status: RuleStatus;
  detail: string;
  offenders: string[];
  source: string;
}

export interface UnsupportedClaim {
  sentence: string;
  reason: string;
  severity: string;
}

export interface GroundingReport {
  unsupported: UnsupportedClaim[];
  checked_sentences: number;
  summary: string;
}

export interface ComplianceReport {
  score: number;
  rules: RuleResult[];
  grounding: GroundingReport | null;
  checked_at: string;
}

export interface StoryboardShot {
  index: number;
  text: string;
  panels: PanelRef[];
  image_paths: string[];
  audio_path: string | null;
  duration_secs: number | null;
}

export interface ScriptBundle {
  /** The narration with its panel tags still in, one tag per panel. */
  tagged_script: string;
  /** The reading copy, tags stripped. */
  final_script: string;
  final_word_count: number;
  panel_count: number;
  narrated_panel_count: number;
  missing_panels: PanelRef[];
  compliance: ComplianceReport | null;
  storyboard: StoryboardShot[];
  scope: ScriptScope;
  generated_at: string;
}

export interface ProjectMeta {
  id: string;
  name: string;
  created_at: string;
  updated_at: string;
  root: string;
  format: SourceFormat;
  page_count: number;
  chapter_count: number;
  analyzed: boolean;
  has_script: boolean;
}

export interface UsageEntry {
  stage: string;
  model: string;
  input_tokens: number;
  output_tokens: number;
  at: string;
}

export interface Project {
  meta: ProjectMeta;
  source: Source;
  bible: StoryBible;
  scripts: Record<string, ScriptBundle>;
  usage: { entries: UsageEntry[] };
}

// --- settings --------------------------------------------------------------

export interface ModelInfo {
  id: string;
  label: string;
  vision: boolean;
  context_tokens: number | null;
  family: string;
  rank: number;
}

export interface ProviderSettings {
  api_key: string;
  base_url: string | null;
  cached_models: ModelInfo[];
  models_refreshed_at: string | null;
}

export interface ModelChoice {
  provider: ProviderId;
  model: string;
}

export interface TtsSettings {
  provider: TtsProviderId;
  api_key: string;
  voice_id: string;
  model_id: string;
  stability: number;
  similarity_boost: number;
  speed: number;
}

export interface VideoSettings {
  width: number;
  height: number;
  fps: number;
  crf: number;
  ken_burns: boolean;
  fade_secs: number;
  blurred_background: boolean;
  ffmpeg_path: string;
}

export interface Settings {
  providers: Record<string, ProviderSettings>;
  primary: ModelChoice;
  vision_override: ModelChoice | null;
  writer_override: ModelChoice | null;
  concurrency: number;
  pages_per_batch: number;
  vision_max_dim: number;
  extract_panels: boolean;
  panel_level_vision: boolean;
  grounding_check: boolean;
  pages_per_narration: number;
  max_output_tokens: number;
  /** The user's own narrator prompt. Empty means the shipped default. */
  narrator_prompt: string;
  /** The user's own delivery rules. Empty means the shipped default. */
  delivery_contract: string;
  /** Ids of mechanical checks switched off. */
  disabled_rules: string[];
  tts: TtsSettings;
  video: VideoSettings;
  projects_root: string | null;
  theme: string;
  onboarded: boolean;
}

export interface ProviderDescriptor {
  id: ProviderId;
  label: string;
  needs_api_key: boolean;
  default_base_url: string;
  has_key: boolean;
}

export interface AppInfo {
  projects_root: string;
  config_dir: string;
  version: string;
  providers: ProviderDescriptor[];
}

export interface VoiceInfo {
  id: string;
  name: string;
  description: string;
  preview_url: string | null;
}

export interface FfmpegStatus {
  available: boolean;
  ffmpeg_path: string | null;
  ffprobe_path: string | null;
  version: string | null;
  install_hint: string;
}

/** The engine as the UI shows it: what gets sent, the shipped defaults to
 *  revert to, and the checks that can be switched on and off. */
export interface NarratorPromptView {
  prompt: string;
  delivery: string;
  default_prompt: string;
  default_delivery: string;
  prompt_is_custom: boolean;
  delivery_is_custom: boolean;
  /** False when a custom delivery text has dropped the [[page:panel]] tags. */
  delivery_keeps_panel_tags: boolean;
  /** [id, label] for every mechanical check. */
  rules: [string, string][];
  disabled_rules: string[];
}

export interface RenderResult {
  project: Project;
  path: string;
  duration_secs: number;
  shots: number;
}

// --- events ----------------------------------------------------------------

export interface Progress {
  job: string;
  stage: string;
  current: number;
  total: number;
  message: string;
  done: boolean;
  error: string | null;
  project_id: string;
}

export interface LogLine {
  level: string;
  text: string;
  at: string;
  project_id: string;
}

export interface AppError {
  message: string;
  kind: string;
  hint: string | null;
}

// --- display helpers -------------------------------------------------------

export const FORMAT_LABELS: Record<SourceFormat, string> = {
  manhwa: "Manhwa / webtoon (vertical scroll)",
  manga: "Manga (right to left pages)",
  comic: "Comic (left to right pages)",
};
