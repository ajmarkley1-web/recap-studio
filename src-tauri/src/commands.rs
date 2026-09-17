//! Tauri command surface.
//!
//! Secrets never round-trip through the UI: `get_settings` returns masked keys
//! and keys are only ever written through the dedicated setter commands.

use crate::error::{AppError, AppResult};
use crate::events::Emitter;
use crate::model::*;
use crate::project::{self, Paths};
use crate::providers::{ModelInfo, ProviderKind};
use crate::settings::{self, ModelChoice, Settings};
use crate::{analyze, ingest, narrator, script, tts, util, video};
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Manager, State};

pub struct AppState {
    pub settings: Mutex<Settings>,
    pub config_dir: PathBuf,
    pub data_dir: PathBuf,
    /// Project roots with a job in flight, so two runs cannot fight over one file.
    pub busy: Mutex<HashSet<String>>,
}

impl AppState {
    pub fn projects_root(&self) -> PathBuf {
        let configured = self.settings.lock().projects_root.clone();
        configured
            .map(PathBuf::from)
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or_else(|| project::default_projects_root(&self.data_dir))
    }

    pub fn snapshot(&self) -> Settings {
        self.settings.lock().clone()
    }

    fn persist(&self) -> AppResult<()> {
        let settings = self.settings.lock().clone();
        settings::save(&self.config_dir, &settings).map_err(AppError::from)
    }
}

/// RAII guard so a panic or early return still frees the project.
struct BusyGuard<'a> {
    state: &'a AppState,
    key: String,
}

impl<'a> BusyGuard<'a> {
    fn acquire(state: &'a AppState, key: &str) -> AppResult<Self> {
        let mut busy = state.busy.lock();
        if busy.contains(key) {
            return Err(AppError::new(
                "busy",
                "This project already has a job running. Wait for it to finish.",
            ));
        }
        busy.insert(key.to_string());
        Ok(Self {
            state,
            key: key.to_string(),
        })
    }
}

impl Drop for BusyGuard<'_> {
    fn drop(&mut self) {
        self.state.busy.lock().remove(&self.key);
    }
}

fn load(root: &str) -> AppResult<Project> {
    project::load(Path::new(root)).map_err(AppError::from)
}

fn parse_provider(id: &str) -> AppResult<ProviderKind> {
    ProviderKind::from_id(id)
        .ok_or_else(|| AppError::new("bad_provider", format!("Unknown provider \"{id}\"")))
}

// ---------------------------------------------------------------------------
// App info
// ---------------------------------------------------------------------------

#[derive(Serialize)]
pub struct AppInfo {
    pub projects_root: String,
    pub config_dir: String,
    pub version: String,
    pub providers: Vec<ProviderDescriptor>,
}

#[derive(Serialize)]
pub struct ProviderDescriptor {
    pub id: String,
    pub label: String,
    pub needs_api_key: bool,
    pub default_base_url: String,
    pub has_key: bool,
}

#[tauri::command]
pub fn app_info(state: State<'_, AppState>) -> AppInfo {
    let settings = state.snapshot();
    AppInfo {
        projects_root: state.projects_root().to_string_lossy().to_string(),
        config_dir: state.config_dir.to_string_lossy().to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        providers: ProviderKind::all()
            .iter()
            .map(|kind| ProviderDescriptor {
                id: kind.id().to_string(),
                label: kind.label().to_string(),
                needs_api_key: kind.needs_api_key(),
                default_base_url: kind.default_base_url().to_string(),
                has_key: !settings.provider(*kind).api_key.trim().is_empty(),
            })
            .collect(),
    }
}

// ---------------------------------------------------------------------------
// Settings
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn get_settings(state: State<'_, AppState>) -> Settings {
    state.settings.lock().sanitized()
}

/// Save everything except secrets. Keys go through `set_provider_key`.
#[tauri::command]
pub fn save_settings(state: State<'_, AppState>, incoming: Settings) -> AppResult<Settings> {
    {
        let mut current = state.settings.lock();
        let mut next = incoming;
        // Carry the stored secrets and cached model lists forward untouched.
        for kind in ProviderKind::all() {
            let id = kind.id().to_string();
            let existing = current.provider(kind);
            let entry = next.providers.entry(id).or_default();
            entry.api_key = existing.api_key;
            entry.cached_models = existing.cached_models;
            entry.models_refreshed_at = existing.models_refreshed_at;
        }
        next.tts.api_key = current.tts.api_key.clone();
        *current = next;
    }
    state.persist()?;
    Ok(state.settings.lock().sanitized())
}

#[tauri::command]
pub fn set_provider_key(
    state: State<'_, AppState>,
    provider: String,
    api_key: String,
) -> AppResult<Settings> {
    let kind = parse_provider(&provider)?;
    {
        let mut settings = state.settings.lock();
        let entry = settings.providers.entry(kind.id().to_string()).or_default();
        entry.api_key = api_key.trim().to_string();
        // A new key invalidates the cached list.
        entry.cached_models.clear();
        entry.models_refreshed_at = None;
    }
    state.persist()?;
    Ok(state.settings.lock().sanitized())
}

#[tauri::command]
pub fn set_provider_base_url(
    state: State<'_, AppState>,
    provider: String,
    base_url: String,
) -> AppResult<Settings> {
    let kind = parse_provider(&provider)?;
    {
        let mut settings = state.settings.lock();
        let entry = settings.providers.entry(kind.id().to_string()).or_default();
        let trimmed = base_url.trim();
        entry.base_url = Some(if trimmed.is_empty() {
            kind.default_base_url().to_string()
        } else {
            trimmed.trim_end_matches('/').to_string()
        });
    }
    state.persist()?;
    Ok(state.settings.lock().sanitized())
}

#[tauri::command]
pub fn set_tts_key(state: State<'_, AppState>, api_key: String) -> AppResult<Settings> {
    state.settings.lock().tts.api_key = api_key.trim().to_string();
    state.persist()?;
    Ok(state.settings.lock().sanitized())
}

/// Ask a provider for its current model list. This is what the refresh button
/// on the model picker calls.
#[tauri::command]
pub async fn refresh_models(
    state: State<'_, AppState>,
    provider: String,
) -> AppResult<Vec<ModelInfo>> {
    let kind = parse_provider(&provider)?;
    let settings = state.snapshot();
    let client = settings.client(kind).map_err(AppError::from)?;
    let models = client.list_models().await.map_err(|err| {
        AppError::new("provider", err.to_string()).with_hint(match kind {
            ProviderKind::Ollama => "Make sure Ollama is running (`ollama serve`) and the base URL is right.",
            _ => "Check the API key for this provider in Settings.",
        })
    })?;

    {
        let mut guard = state.settings.lock();
        let entry = guard.providers.entry(kind.id().to_string()).or_default();
        entry.cached_models = models.clone();
        entry.models_refreshed_at = Some(util::now_iso());
    }
    state.persist()?;
    Ok(models)
}

#[tauri::command]
pub fn cached_models(state: State<'_, AppState>, provider: String) -> AppResult<Vec<ModelInfo>> {
    let kind = parse_provider(&provider)?;
    Ok(state.snapshot().provider(kind).cached_models)
}

#[tauri::command]
pub async fn test_provider(state: State<'_, AppState>, provider: String) -> AppResult<String> {
    let kind = parse_provider(&provider)?;
    let settings = state.snapshot();
    let client = settings.client(kind).map_err(AppError::from)?;
    client
        .verify()
        .await
        .map_err(|err| AppError::new("provider", err.to_string()))
}

#[tauri::command]
pub fn set_model_choice(
    state: State<'_, AppState>,
    role: String,
    provider: String,
    model: String,
) -> AppResult<Settings> {
    let kind = parse_provider(&provider)?;
    let choice = ModelChoice {
        provider: kind,
        model: model.trim().to_string(),
    };
    {
        let mut settings = state.settings.lock();
        match role.as_str() {
            "primary" => settings.primary = choice,
            "vision" => {
                settings.vision_override = if choice.model.is_empty() {
                    None
                } else {
                    Some(choice)
                }
            }
            "writer" => {
                settings.writer_override = if choice.model.is_empty() {
                    None
                } else {
                    Some(choice)
                }
            }
            other => {
                return Err(AppError::new(
                    "bad_role",
                    format!("Unknown model role \"{other}\""),
                ))
            }
        }
    }
    state.persist()?;
    Ok(state.settings.lock().sanitized())
}

// ---------------------------------------------------------------------------
// Projects
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn list_projects(state: State<'_, AppState>) -> Vec<ProjectMeta> {
    let root = state.projects_root();
    let _ = std::fs::create_dir_all(&root);
    project::list(&root)
}

#[tauri::command]
pub fn create_project(
    state: State<'_, AppState>,
    name: String,
    format: SourceFormat,
) -> AppResult<Project> {
    let root = state.projects_root();
    std::fs::create_dir_all(&root)?;
    project::create(&root, &name, format).map_err(AppError::from)
}

#[tauri::command]
pub fn open_project(root: String) -> AppResult<Project> {
    load(&root)
}

#[tauri::command]
pub fn delete_project(root: String) -> AppResult<()> {
    project::delete(Path::new(&root)).map_err(AppError::from)
}

#[derive(Deserialize)]
pub struct ProjectPatch {
    pub name: Option<String>,
    pub format: Option<SourceFormat>,
}

#[tauri::command]
pub fn update_project(root: String, patch: ProjectPatch) -> AppResult<Project> {
    let mut project = load(&root)?;
    if let Some(name) = patch.name {
        if !name.trim().is_empty() {
            project.meta.name = name.trim().to_string();
        }
    }
    if let Some(format) = patch.format {
        project.meta.format = format;
        project.source.format = format;
    }
    project::touch(&mut project);
    project::save(&project).map_err(AppError::from)?;
    Ok(project)
}

// ---------------------------------------------------------------------------
// Ingest
// ---------------------------------------------------------------------------

/// Receive one rasterized PDF page from the frontend renderer.
#[tauri::command]
pub fn stage_pdf_page(root: String, ordinal: usize, bytes: Vec<u8>) -> AppResult<()> {
    ingest::stage_pdf_page(Path::new(&root), ordinal, &bytes)
        .map(|_| ())
        .map_err(AppError::from)
}

#[tauri::command]
pub async fn ingest_pdf(
    app: AppHandle,
    state: State<'_, AppState>,
    root: String,
    chapter_label: String,
) -> AppResult<Project> {
    let mut project = load(&root)?;
    let _guard = BusyGuard::acquire(&state, &root)?;
    let emitter = Emitter::new(app, "ingest", &project.meta.id);
    let settings = state.snapshot();

    let items = ingest::pdf_items(Path::new(&root), &chapter_label);
    if items.is_empty() {
        let err = AppError::new("empty", "No PDF pages were received.")
            .with_hint("The PDF may be encrypted or have no renderable pages.");
        emitter.failed(err.message.clone());
        return Err(err);
    }

    let strategy = format!("{} pages rendered from a PDF", items.len());
    let result = ingest::build_source(
        Path::new(&root),
        items,
        project.meta.format,
        strategy,
        settings.vision_max_dim,
        settings.extract_panels,
        &emitter,
    );
    match result {
        Ok(source) => {
            project.source = source;
            project::touch(&mut project);
            project::save(&project).map_err(AppError::from)?;
            emitter.done(format!("Imported {} pages", project.source.pages.len()));
            Ok(project)
        }
        Err(err) => {
            emitter.failed(err.to_string());
            Err(AppError::from(err))
        }
    }
}

#[tauri::command]
pub async fn ingest_paths(
    app: AppHandle,
    state: State<'_, AppState>,
    root: String,
    paths: Vec<String>,
) -> AppResult<Project> {
    let mut project = load(&root)?;
    let _guard = BusyGuard::acquire(&state, &root)?;
    let emitter = Emitter::new(app, "ingest", &project.meta.id);
    let settings = state.snapshot();

    let inputs: Vec<PathBuf> = paths.iter().map(PathBuf::from).collect();
    let (items, strategy) = match ingest::collect(&inputs) {
        Ok(v) => v,
        Err(err) => {
            emitter.failed(err.to_string());
            return Err(AppError::from(err));
        }
    };
    emitter.info(strategy.clone());

    let result = ingest::build_source(
        Path::new(&root),
        items,
        project.meta.format,
        strategy,
        settings.vision_max_dim,
        settings.extract_panels,
        &emitter,
    );
    match result {
        Ok(source) => {
            project.source = source;
            project::touch(&mut project);
            project::save(&project).map_err(AppError::from)?;
            emitter.done(format!(
                "Imported {} pages across {} chapter(s)",
                project.source.pages.len(),
                project.source.chapters.len()
            ));
            Ok(project)
        }
        Err(err) => {
            emitter.failed(err.to_string());
            Err(AppError::from(err))
        }
    }
}

// ---------------------------------------------------------------------------
// Analyze
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn analyze_project(
    app: AppHandle,
    state: State<'_, AppState>,
    root: String,
) -> AppResult<Project> {
    let mut project = load(&root)?;
    let _guard = BusyGuard::acquire(&state, &root)?;
    let emitter = Emitter::new(app, "analyze", &project.meta.id);
    let settings = state.snapshot();

    if !settings.is_configured() {
        let err = AppError::new(
            "unconfigured",
            "No model is selected yet.",
        )
        .with_hint("Open Settings, add a provider key, hit Refresh and pick a model.");
        emitter.failed(err.message.clone());
        return Err(err);
    }

    match analyze::run(&mut project, &settings, &emitter).await {
        Ok(()) => Ok(project),
        Err(err) => {
            // Keep whatever the passes did manage to produce.
            project::touch(&mut project);
            let _ = project::save(&project);
            emitter.failed(err.to_string());
            Err(AppError::from(err))
        }
    }
}

// ---------------------------------------------------------------------------
// Scripts
// ---------------------------------------------------------------------------

/// The prompt, exactly as the model receives it, for the Prompt tab in the UI.
#[derive(serde::Serialize)]
pub struct NarratorPromptView {
    pub prompt: String,
    pub delivery: String,
}

#[tauri::command]
pub fn narrator_prompt() -> NarratorPromptView {
    NarratorPromptView {
        prompt: narrator::NARRATOR_PROMPT.to_string(),
        delivery: narrator::DELIVERY_CONTRACT.to_string(),
    }
}

#[derive(Deserialize)]
pub struct ScriptRequest {
    pub root: String,
    pub scope: ScriptScope,
}

#[tauri::command]
pub async fn generate_script(
    app: AppHandle,
    state: State<'_, AppState>,
    request: ScriptRequest,
) -> AppResult<Project> {
    let mut project = load(&request.root)?;
    let _guard = BusyGuard::acquire(&state, &request.root)?;
    let emitter = Emitter::new(app, "script", &project.meta.id);
    let settings = state.snapshot();

    match script::generate(&mut project, &settings, request.scope, &emitter).await {
        Ok(_) => Ok(project),
        Err(err) => {
            emitter.failed(err.to_string());
            Err(AppError::from(err))
        }
    }
}

#[tauri::command]
pub async fn recheck_compliance(
    app: AppHandle,
    state: State<'_, AppState>,
    root: String,
    scope: ScriptScope,
    grounding: bool,
) -> AppResult<Project> {
    let mut project = load(&root)?;
    let _guard = BusyGuard::acquire(&state, &root)?;
    let emitter = Emitter::new(app, "script", &project.meta.id);
    let settings = state.snapshot();
    let key = script::scope_key(&scope);

    let bundle = project
        .scripts
        .get(&key)
        .cloned()
        .ok_or_else(|| AppError::new("no_script", "There is no script for this scope yet."))?;

    match script::check_compliance(&project, &settings, &scope, &bundle, grounding, &emitter).await {
        Ok(report) => {
            if let Some(entry) = project.scripts.get_mut(&key) {
                entry.compliance = Some(report);
            }
            project::touch(&mut project);
            project::save(&project).map_err(AppError::from)?;
            emitter.done("Compliance check complete");
            Ok(project)
        }
        Err(err) => {
            emitter.failed(err.to_string());
            Err(AppError::from(err))
        }
    }
}

#[tauri::command]
pub async fn auto_fix_script(
    app: AppHandle,
    state: State<'_, AppState>,
    root: String,
    scope: ScriptScope,
) -> AppResult<Project> {
    let mut project = load(&root)?;
    let _guard = BusyGuard::acquire(&state, &root)?;
    let emitter = Emitter::new(app, "script", &project.meta.id);
    let settings = state.snapshot();
    let key = script::scope_key(&scope);

    let mut bundle = project
        .scripts
        .get(&key)
        .cloned()
        .ok_or_else(|| AppError::new("no_script", "There is no script for this scope yet."))?;

    match script::auto_fix(&project, &settings, &scope, &mut bundle, &emitter).await {
        Ok(()) => {
            project.scripts.insert(key, bundle);
            project::touch(&mut project);
            project::save(&project).map_err(AppError::from)?;
            emitter.done("Script repaired");
            Ok(project)
        }
        Err(err) => {
            emitter.failed(err.to_string());
            Err(AppError::from(err))
        }
    }
}

/// Save a hand-edited script, then re-run the mechanical checks on it.
#[tauri::command]
pub fn update_script_text(
    root: String,
    scope: ScriptScope,
    text: String,
) -> AppResult<Project> {
    let mut project = load(&root)?;
    let key = script::scope_key(&scope);
    if !project.scripts.contains_key(&key) {
        return Err(AppError::new("no_script", "There is no script for this scope yet."));
    }

    // A hand edit is taken as the new tagged copy: the editor shows the tags, so
    // a panel can be rewritten without losing which panel it belongs to.
    let mut updated = script::assemble(&project, &scope, text);
    let expected = script::panels_in_scope(&project, &scope);
    let narrated = narrator::split_by_marker(&updated.tagged_script);
    let coverage = crate::compliance::Coverage::measure(&expected, &narrated);
    // A manual edit invalidates the old grounding result rather than keeping a stale one.
    let mut report = crate::compliance::check(&updated.final_script, &coverage);
    report.grounding = None;
    updated.compliance = Some(report);

    project.scripts.insert(key, updated);
    project::touch(&mut project);
    project::save(&project).map_err(AppError::from)?;
    Ok(project)
}

// ---------------------------------------------------------------------------
// Narration
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn list_voices(state: State<'_, AppState>) -> AppResult<Vec<tts::VoiceInfo>> {
    let settings = state.snapshot();
    tts::list_voices(&settings)
        .await
        .map_err(|e| AppError::new("tts", e.to_string()))
}

#[tauri::command]
pub async fn list_tts_models(state: State<'_, AppState>) -> AppResult<Vec<tts::VoiceInfo>> {
    let settings = state.snapshot();
    tts::list_tts_models(&settings)
        .await
        .map_err(|e| AppError::new("tts", e.to_string()))
}

#[tauri::command]
pub async fn preview_voice(state: State<'_, AppState>, text: String) -> AppResult<String> {
    let settings = state.snapshot();
    let dest = state.config_dir.join("voice-preview.mp3");
    let sample = if text.trim().is_empty() {
        "So this guy wakes up in a dungeon with no memory of how he got there, and the system immediately hands him a quest he never asked for."
    } else {
        text.trim()
    };
    tts::preview(&settings, sample, &dest)
        .await
        .map(|p| p.to_string_lossy().to_string())
        .map_err(|e| AppError::new("tts", e.to_string()))
}

#[tauri::command]
pub async fn narrate_script(
    app: AppHandle,
    state: State<'_, AppState>,
    root: String,
    scope: ScriptScope,
) -> AppResult<Project> {
    let mut project = load(&root)?;
    let _guard = BusyGuard::acquire(&state, &root)?;
    let emitter = Emitter::new(app, "narrate", &project.meta.id);
    let settings = state.snapshot();
    let key = script::scope_key(&scope);

    let mut bundle = project
        .scripts
        .get(&key)
        .cloned()
        .ok_or_else(|| AppError::new("no_script", "Generate a script before narrating."))?;

    match tts::narrate(&project, &mut bundle, &settings, &emitter).await {
        Ok(()) => {
            project.scripts.insert(key, bundle);
            project::touch(&mut project);
            project::save(&project).map_err(AppError::from)?;
            Ok(project)
        }
        Err(err) => Err(AppError::new("tts", err.to_string())),
    }
}

// ---------------------------------------------------------------------------
// Video
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn ffmpeg_status(state: State<'_, AppState>) -> video::FfmpegStatus {
    video::locate(&state.snapshot().video)
}

#[derive(Serialize)]
pub struct RenderResult {
    pub project: Project,
    pub path: String,
    pub duration_secs: f32,
    pub shots: usize,
}

#[tauri::command]
pub async fn render_video(
    app: AppHandle,
    state: State<'_, AppState>,
    root: String,
    scope: ScriptScope,
) -> AppResult<RenderResult> {
    let mut project = load(&root)?;
    let _guard = BusyGuard::acquire(&state, &root)?;
    let emitter = Emitter::new(app, "render", &project.meta.id);
    let settings = state.snapshot();
    let key = script::scope_key(&scope);

    let mut bundle = project
        .scripts
        .get(&key)
        .cloned()
        .ok_or_else(|| AppError::new("no_script", "Generate and narrate a script first."))?;

    // ffmpeg work is blocking, so it goes on the blocking pool.
    let project_for_render = project.clone();
    let settings_for_render = settings.clone();
    let emitter_for_render = emitter.clone();
    let mut bundle_for_render = bundle.clone();
    let outcome = tokio::task::spawn_blocking(move || {
        video::render(
            &project_for_render,
            &mut bundle_for_render,
            &settings_for_render,
            &emitter_for_render,
        )
        .map(|o| (o, bundle_for_render))
    })
    .await
    .map_err(|e| AppError::new("render", format!("Render task crashed: {e}")))?;

    match outcome {
        Ok((result, rendered_bundle)) => {
            bundle = rendered_bundle;
            project.scripts.insert(key, bundle);
            project::touch(&mut project);
            project::save(&project).map_err(AppError::from)?;
            Ok(RenderResult {
                path: result.path.to_string_lossy().to_string(),
                duration_secs: result.duration_secs,
                shots: result.shots,
                project,
            })
        }
        Err(err) => {
            emitter.failed(err.to_string());
            Err(AppError::new("render", err.to_string())
                .with_hint("Check that ffmpeg is installed and that the script has been narrated."))
        }
    }
}

// ---------------------------------------------------------------------------
// Export
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn export_script(root: String, scope: ScriptScope, format: String) -> AppResult<String> {
    let project = load(&root)?;
    let key = script::scope_key(&scope);
    let bundle = project
        .scripts
        .get(&key)
        .ok_or_else(|| AppError::new("no_script", "There is no script for this scope yet."))?;

    let paths = Paths::new(&project.meta.root);
    std::fs::create_dir_all(paths.out())?;
    let stem = format!("{}-{}", util::slugify(&project.meta.name), key);

    let (filename, contents) = match format.as_str() {
        "txt" => (format!("{stem}.txt"), bundle.final_script.clone()),
        "tagged" => (
            format!("{stem}-tagged.txt"),
            bundle.tagged_script.clone(),
        ),
        "md" => {
            let mut out = format!("# {} - narration\n\n", project.meta.name);
            out.push_str(&format!(
                "**Panels:** {} of {} narrated  \n**Words:** {}  \n**Read time:** about {} min\n\n",
                bundle.narrated_panel_count,
                bundle.panel_count,
                bundle.final_word_count,
                (bundle.final_word_count as f32 / 150.0).ceil() as usize
            ));
            if let Some(report) = &bundle.compliance {
                out.push_str(&format!("**Compliance:** {}/100\n\n", report.score));
            }
            out.push_str("---\n\n");
            out.push_str(&bundle.final_script);
            (format!("{stem}.md"), out)
        }
        "storyboard" => {
            let mut out = String::from("shot\tpanels\ttext\n");
            for shot in &bundle.storyboard {
                out.push_str(&format!(
                    "{}\t{}\t{}\n",
                    shot.index + 1,
                    shot.image_paths.join(" | "),
                    shot.text.replace('\t', " ")
                ));
            }
            (format!("{stem}-storyboard.tsv"), out)
        }
        other => {
            return Err(AppError::new(
                "bad_format",
                format!("Unknown export format \"{other}\""),
            ))
        }
    };

    let dest = paths.out().join(filename);
    std::fs::write(&dest, contents)?;
    Ok(dest.to_string_lossy().to_string())
}

#[tauri::command]
pub fn export_bible(root: String) -> AppResult<String> {
    let project = load(&root)?;
    let paths = Paths::new(&project.meta.root);
    std::fs::create_dir_all(paths.out())?;
    let dest = paths
        .out()
        .join(format!("{}-story-bible.json", util::slugify(&project.meta.name)));
    std::fs::write(&dest, serde_json::to_string_pretty(&project.bible)?)?;
    Ok(dest.to_string_lossy().to_string())
}

// ---------------------------------------------------------------------------
// Setup
// ---------------------------------------------------------------------------

pub fn build_state(app: &AppHandle) -> AppState {
    let config_dir = app
        .path()
        .app_config_dir()
        .unwrap_or_else(|_| std::env::temp_dir().join("recap-studio"));
    let data_dir = app
        .path()
        .app_data_dir()
        .unwrap_or_else(|_| std::env::temp_dir().join("recap-studio-data"));
    let _ = std::fs::create_dir_all(&config_dir);
    let _ = std::fs::create_dir_all(&data_dir);

    let settings = settings::load(&config_dir);
    AppState {
        settings: Mutex::new(settings),
        config_dir,
        data_dir,
        busy: Mutex::new(HashSet::new()),
    }
}
