//! Narration.
//!
//! One pass, one prompt. The story bible supplies the panel-by-panel record of
//! what is on the pages, `narrator::NARRATOR_PROMPT` supplies the voice, and the
//! model writes one stretch of narration per panel, tagged with that panel, in
//! reading order.
//!
//! Long scopes are cut into segments of a few pages each, because output token
//! limits, not the prompt, are what would otherwise force panels to be merged or
//! dropped. Each segment is a fresh call at the full output allowance, carrying
//! the story so far, and the segments are joined end to end. Coverage is then
//! checked against the panel list, so a skipped panel is a visible failure with
//! a one-click fix rather than something nobody notices.

use crate::analyze::ask_text;
use crate::compliance::{self, Coverage};
use crate::events::Emitter;
use crate::model::*;
use crate::narrator;
use crate::project;
use crate::prompts;
use crate::providers::ChatMessage;
use crate::settings::{ModelRole, Settings};
use crate::util;
use anyhow::{anyhow, Result};
use serde::Deserialize;

pub fn scope_key(scope: &ScriptScope) -> String {
    match scope {
        ScriptScope::WholeProject => "all".to_string(),
        ScriptScope::Chapter(i) => format!("ch{i}"),
    }
}

pub fn scope_label(project: &Project, scope: &ScriptScope) -> String {
    match scope {
        ScriptScope::WholeProject => {
            if project.source.chapters.len() <= 1 {
                "this chapter".to_string()
            } else {
                format!("all {} chapters", project.source.chapters.len())
            }
        }
        ScriptScope::Chapter(i) => project
            .source
            .chapters
            .get(*i)
            .and_then(|c| c.title.clone())
            .unwrap_or_else(|| format!("Chapter {}", i + 1)),
    }
}

fn page_range(project: &Project, scope: &ScriptScope) -> (usize, usize) {
    match scope {
        ScriptScope::WholeProject => (0, project.source.pages.len().saturating_sub(1)),
        ScriptScope::Chapter(i) => project
            .source
            .chapters
            .get(*i)
            .map(|c| (c.first_page, c.last_page))
            .unwrap_or((0, project.source.pages.len().saturating_sub(1))),
    }
}

// ---------------------------------------------------------------------------
// Context blobs
// ---------------------------------------------------------------------------

fn work_blob(project: &Project) -> String {
    let w = &project.bible.work;
    let mut out = format!(
        "Title: {}\n",
        if w.title.is_empty() {
            &project.meta.name
        } else {
            &w.title
        }
    );
    if !w.genres.is_empty() {
        out.push_str(&format!("Genres: {}\n", w.genres.join(", ")));
    }
    if !w.premise.is_empty() {
        out.push_str(&format!("Premise: {}\n", w.premise));
    }
    if !w.setting.is_empty() {
        out.push_str(&format!("Setting: {}\n", w.setting));
    }
    if !w.power_system.is_empty() {
        out.push_str(&format!("Power system: {}\n", w.power_system));
    }
    if !w.tone.is_empty() {
        out.push_str(&format!("Tone: {}\n", w.tone));
    }
    out
}

fn cast_blob(project: &Project) -> String {
    project
        .bible
        .characters
        .iter()
        .filter(|c| c.prominence >= 15 || c.role.eq_ignore_ascii_case("protagonist"))
        .take(24)
        .map(|c| {
            let mut line = format!("- {} ({})", c.name, c.role);
            if !c.aliases.is_empty() {
                line.push_str(&format!(" also called {}", c.aliases.join(", ")));
            }
            if !c.appearance.is_empty() {
                line.push_str(&format!(" | looks: {}", c.appearance));
            }
            if !c.abilities.is_empty() {
                line.push_str(&format!(" | abilities: {}", c.abilities.join(", ")));
            }
            if !c.goals.is_empty() {
                line.push_str(&format!(" | wants: {}", c.goals));
            }
            line
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Chapters that end before this page, so a later segment knows the backstory.
fn chapters_before(project: &Project, page: usize) -> String {
    project
        .bible
        .chapters
        .iter()
        .filter(|c| c.page_end < page)
        .map(|c| {
            format!(
                "{}: {}",
                c.title.clone().unwrap_or_else(|| "Chapter".into()),
                c.synopsis
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// What the narrator needs to carry forward into the next segment: the chapter
/// synopses that came before it, plus the tail of what has already been written
/// so the prose picks up mid-breath instead of restarting.
fn story_so_far(project: &Project, first_page: usize, written: &str) -> String {
    let mut out = chapters_before(project, first_page);
    let tail = narrator::strip_markers(written);
    if !tail.trim().is_empty() {
        let tail = util::truncate_tail(&tail, 2_400);
        if !out.is_empty() {
            out.push_str("\n\n");
        }
        out.push_str("The narration so far ends like this. Continue straight on from it:\n");
        out.push_str(&tail);
    }
    out
}

// ---------------------------------------------------------------------------
// The panel record
// ---------------------------------------------------------------------------

/// Pages a narration covers: everything in scope that the analyzer read as part
/// of the story. Only advertising and author notes are left out, because they
/// are not the story; every story page goes in, however minor.
fn narratable_pages<'a>(project: &'a Project, scope: &ScriptScope) -> Vec<&'a PageReading> {
    let (from, to) = page_range(project, scope);
    project
        .bible
        .readings
        .iter()
        .filter(|r| r.page_index >= from && r.page_index <= to)
        .filter(|r| !matches!(r.role, PageRole::AdOrFiller | PageRole::AuthorNote))
        .collect()
}

/// How many panels a page contributes. A page the analyzer returned with no
/// panels still counts as one, so a splash page cannot fall through the gap.
fn panel_count_for(reading: &PageReading) -> usize {
    reading.panels.len().max(1)
}

/// Every panel in scope, in reading order. This is the list the narration is
/// checked against.
pub fn panels_in_scope(project: &Project, scope: &ScriptScope) -> Vec<PanelRef> {
    narratable_pages(project, scope)
        .iter()
        .flat_map(|r| {
            (0..panel_count_for(r)).map(move |i| PanelRef {
                page: r.page_index,
                panel: i,
            })
        })
        .collect()
}

fn describe_panel(page: usize, index: usize, panel: Option<&PanelReading>) -> String {
    let tag = narrator::marker(page, index);
    let Some(panel) = panel else {
        return format!("{tag} the page carries this moment whole, with no separate panels drawn on it\n");
    };

    let mut line = tag;
    if !panel.characters.is_empty() {
        line.push_str(&format!(" who: {}", panel.characters.join(", ")));
    }
    if !panel.description.trim().is_empty() {
        line.push_str(&format!(" | drawn: {}", panel.description.trim()));
    }
    if !panel.action.trim().is_empty() {
        line.push_str(&format!(" | happens: {}", panel.action.trim()));
    }
    if !panel.emotion.trim().is_empty() {
        line.push_str(&format!(" | feeling: {}", panel.emotion.trim()));
    }
    if !panel.sfx.is_empty() {
        line.push_str(&format!(" | sound: {}", panel.sfx.join(", ")));
    }
    line.push('\n');

    for spoken in &panel.dialogue {
        if spoken.text.trim().is_empty() {
            continue;
        }
        let speaker = if spoken.speaker.trim().is_empty() {
            "unknown"
        } else {
            spoken.speaker.trim()
        };
        let kind = if spoken.kind.trim().is_empty() {
            "speech"
        } else {
            spoken.kind.trim()
        };
        line.push_str(&format!(
            "      {speaker} ({kind}): {}\n",
            spoken.text.trim()
        ));
    }
    line
}

/// The panel-by-panel record for one segment, and the panels it lists.
fn panel_record(pages: &[&PageReading]) -> (String, Vec<PanelRef>) {
    let mut out = String::new();
    let mut refs = Vec::new();

    for reading in pages {
        let mut header = format!("--- page {}", reading.page_index);
        if !reading.location.trim().is_empty() {
            header.push_str(&format!(" | {}", reading.location.trim()));
        }
        if !reading.time_of_day.trim().is_empty() {
            header.push_str(&format!(" | {}", reading.time_of_day.trim()));
        }
        if reading.is_flashback {
            header.push_str(" | this page is a flashback");
        }
        header.push_str(" ---\n");
        out.push_str(&header);

        for index in 0..panel_count_for(reading) {
            out.push_str(&describe_panel(
                reading.page_index,
                index,
                reading.panels.get(index),
            ));
            refs.push(PanelRef {
                page: reading.page_index,
                panel: index,
            });
        }
        if !reading.notes.trim().is_empty() {
            out.push_str(&format!("  worth knowing: {}\n", reading.notes.trim()));
        }
        out.push('\n');
    }
    (out, refs)
}

// ---------------------------------------------------------------------------
// Generation
// ---------------------------------------------------------------------------

/// Narrate one segment of pages and return the tagged prose.
async fn narrate_segment(
    project: &Project,
    settings: &Settings,
    pages: &[&PageReading],
    written_so_far: &str,
) -> Result<(String, u64, u64)> {
    let (client, model) = settings.resolve(ModelRole::Writer)?;
    let (record, refs) = panel_record(pages);
    let first_page = pages.first().map(|p| p.page_index).unwrap_or(0);

    let user = narrator::narration_user(
        prompts::reading_order_hint(project.source.format),
        settings.delivery_contract(),
        &work_blob(project),
        &cast_blob(project),
        &util::truncate(&story_so_far(project, first_page, written_so_far), 8_000),
        &util::truncate(&record, 90_000),
        refs.len(),
    );

    ask_text(
        &client,
        &model,
        settings.narrator_prompt(),
        vec![ChatMessage::user_text(user)],
        settings.max_output_tokens,
        0.75,
    )
    .await
}

pub async fn generate(
    project: &mut Project,
    settings: &Settings,
    scope: ScriptScope,
    emitter: &Emitter,
) -> Result<ScriptBundle> {
    if project.bible.readings.is_empty() {
        return Err(anyhow!(
            "Run Analyze first. The narration is written from the panel record, not from raw pages."
        ));
    }

    let label = scope_label(project, &scope);
    let writer_model = settings.choice_for(ModelRole::Writer).model;

    if !narrator::mentions_panel_tags(settings.delivery_contract()) {
        emitter.warn(
            "The delivery rules no longer ask for [[page:panel]] tags. Panel coverage \
             and the storyboard both depend on them, so both will come back empty.",
        );
    }

    // The page records are borrowed out of the project, so the token ledger is
    // written once the narration is done rather than during it.
    let (tagged, spend) = {
        let pages = narratable_pages(project, &scope);
        if pages.is_empty() {
            return Err(anyhow!(
                "There are no story pages in {label}. Check the analysis before narrating."
            ));
        }
        let panels = panels_in_scope(project, &scope).len();

        let per_call = settings.pages_per_narration.clamp(1, 24);
        let segments: Vec<&[&PageReading]> = pages.chunks(per_call).collect();
        let total = segments.len();
        emitter.info(format!(
            "Narrating {panels} panel(s) across {} page(s) of {label}, in {total} pass(es)",
            pages.len()
        ));

        let mut tagged = String::new();
        let mut spend: Vec<(u64, u64)> = Vec::new();

        for (i, segment) in segments.iter().copied().enumerate() {
            let first = segment.first().map(|p| p.page_index).unwrap_or(0);
            let last = segment.last().map(|p| p.page_index).unwrap_or(first);
            emitter.progress(
                "narrate",
                i,
                total,
                format!("Narrating pages {}-{}", first + 1, last + 1),
            );

            let (text, input, output) =
                narrate_segment(project, settings, segment, &tagged).await?;
            spend.push((input, output));

            let cleaned = text.trim();
            if cleaned.is_empty() {
                return Err(anyhow!(
                    "The writer model returned nothing for pages {}-{}. Try a different model or fewer pages per pass.",
                    first + 1,
                    last + 1
                ));
            }
            if !tagged.is_empty() {
                tagged.push_str("\n\n");
            }
            tagged.push_str(cleaned);
        }
        emitter.progress("narrate", total, total, "Narration written");
        (tagged, spend)
    };

    for (input, output) in spend {
        project.usage.add("narration", &writer_model, input, output);
    }


    let mut bundle = assemble(project, &scope, tagged);
    emitter.info(format!(
        "{} words covering {}/{} panels",
        bundle.final_word_count, bundle.narrated_panel_count, bundle.panel_count
    ));
    if !bundle.missing_panels.is_empty() {
        emitter.warn(format!(
            "{} panel(s) were not narrated. Use Fix to fill them in.",
            bundle.missing_panels.len()
        ));
    }

    let report = check_compliance(
        project,
        settings,
        &scope,
        &bundle,
        settings.grounding_check,
        emitter,
    )
    .await?;
    emitter.info(format!("Compliance score: {}/100", report.score));
    bundle.compliance = Some(report);

    project.scripts.insert(scope_key(&scope), bundle.clone());
    project::touch(project);
    project::save(project)?;
    emitter.done("Narration ready");
    Ok(bundle)
}

/// Turn a tagged reply into a finished bundle: reading copy, coverage, storyboard.
pub fn assemble(project: &Project, scope: &ScriptScope, tagged: String) -> ScriptBundle {
    let expected = panels_in_scope(project, scope);
    let narrated = narrator::split_by_marker(&tagged);
    let coverage = Coverage::measure(&expected, &narrated);
    let final_script = narrator::strip_markers(&tagged);

    ScriptBundle {
        final_word_count: util::word_count(&final_script),
        panel_count: expected.len(),
        // Panels in scope that were reached, so this reads N of N when nothing
        // was skipped even if the model also tagged something out of scope.
        narrated_panel_count: expected.len() - coverage.missing.len(),
        missing_panels: coverage.missing.clone(),
        storyboard: build_storyboard(project, scope, &tagged),
        final_script,
        tagged_script: tagged,
        compliance: None,
        scope: scope.clone(),
        generated_at: util::now_iso(),
    }
}

// ---------------------------------------------------------------------------
// Storyboard: one shot per panel, in order
// ---------------------------------------------------------------------------

fn panel_image(project: &Project, r: PanelRef) -> Option<String> {
    let page = project.source.pages.iter().find(|p| p.index == r.page)?;
    Some(
        page.panels
            .get(r.panel)
            .map(|p| p.path.clone())
            .unwrap_or_else(|| page.path.clone()),
    )
}

/// The storyboard is the tagged script read straight through: each panel becomes
/// one shot holding its own narration, in the order the panels are read. Nothing
/// is merged, so the video holds every panel on screen for its own narration.
pub fn build_storyboard(
    project: &Project,
    scope: &ScriptScope,
    tagged_script: &str,
) -> Vec<StoryboardShot> {
    let narrated = narrator::split_by_marker(tagged_script);
    if !narrated.is_empty() {
        return narrated
            .iter()
            .enumerate()
            .map(|(i, n)| StoryboardShot {
                index: i,
                text: n.text.clone(),
                panels: vec![n.panel],
                image_paths: panel_image(project, n.panel).into_iter().collect(),
                audio_path: None,
                duration_secs: None,
            })
            .collect();
    }
    // No tags at all: the model ignored the contract, or the script was pasted
    // in by hand. Spread the sentences evenly over the panels so the renderer
    // still has something to hold on screen.
    fallback_storyboard(project, scope, tagged_script)
}

fn fallback_storyboard(
    project: &Project,
    scope: &ScriptScope,
    script: &str,
) -> Vec<StoryboardShot> {
    let panels = panels_in_scope(project, scope);
    let sentences = util::split_sentences(script);
    if panels.is_empty() || sentences.is_empty() {
        return Vec::new();
    }
    sentences
        .iter()
        .enumerate()
        .map(|(i, text)| {
            let panel = panels[(i * panels.len()) / sentences.len()];
            StoryboardShot {
                index: i,
                text: text.clone(),
                panels: vec![panel],
                image_paths: panel_image(project, panel).into_iter().collect(),
                audio_path: None,
                duration_secs: None,
            }
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Compliance + grounding
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct GroundingResult {
    #[serde(default)]
    unsupported: Vec<UnsupportedClaim>,
    #[serde(default)]
    summary: String,
}

/// The source record for the whole scope, used by the grounding pass.
fn record_blob(project: &Project, scope: &ScriptScope) -> String {
    panel_record(&narratable_pages(project, scope)).0
}

pub async fn check_compliance(
    project: &Project,
    settings: &Settings,
    scope: &ScriptScope,
    bundle: &ScriptBundle,
    run_grounding: bool,
    emitter: &Emitter,
) -> Result<ComplianceReport> {
    emitter.stage("compliance", "Checking the narration against the prompt");
    let expected = panels_in_scope(project, scope);
    let narrated = narrator::split_by_marker(&bundle.tagged_script);
    let coverage = Coverage::measure(&expected, &narrated);
    let mut report = compliance::check(&bundle.final_script, &coverage, &settings.disabled_rules);

    if run_grounding && !bundle.final_script.trim().is_empty() {
        emitter.stage("grounding", "Checking every sentence against the source");
        match settings.resolve(ModelRole::Reasoning) {
            Ok((client, model)) => {
                let record = util::truncate(&record_blob(project, scope), 80_000);
                let sentences = util::split_sentences(&bundle.final_script).len();
                let result = ask_text(
                    &client,
                    &model,
                    prompts::GROUNDING_SYSTEM,
                    vec![ChatMessage::user_text(prompts::grounding_user(
                        &record,
                        &util::truncate(&bundle.final_script, 80_000),
                    ))],
                    4096,
                    0.0,
                )
                .await;
                match result {
                    Ok((text, _, _)) => match util::parse_json_lenient::<GroundingResult>(&text) {
                        Ok(parsed) => {
                            if !parsed.unsupported.is_empty() {
                                emitter.warn(format!(
                                    "Grounding found {} sentence(s) not supported by the source",
                                    parsed.unsupported.len()
                                ));
                            }
                            report.grounding = Some(GroundingReport {
                                unsupported: parsed.unsupported,
                                checked_sentences: sentences,
                                summary: parsed.summary,
                            });
                        }
                        Err(err) => emitter.warn(format!("Grounding reply unreadable: {err}")),
                    },
                    Err(err) => emitter.warn(format!("Grounding check failed: {err}")),
                }
            }
            Err(err) => emitter.warn(format!("Grounding skipped: {err}")),
        }
    }

    if let Some(grounding) = &report.grounding {
        let penalty = (grounding.unsupported.len() as u32 * 8).min(35) as u8;
        report.score = report.score.saturating_sub(penalty);
    }

    Ok(report)
}

// ---------------------------------------------------------------------------
// Auto-fix
// ---------------------------------------------------------------------------

/// The panels the repair pass has to write from scratch, with their record.
fn missing_panel_brief(project: &Project, scope: &ScriptScope, missing: &[PanelRef]) -> String {
    if missing.is_empty() {
        return String::new();
    }
    let pages = narratable_pages(project, scope);
    let mut out = String::from(
        "\nThese panels have no narration at all. Write each one into its proper place in the order, tag and all:\n",
    );
    for r in missing.iter().take(60) {
        let reading = pages.iter().find(|p| p.page_index == r.page);
        let panel = reading.and_then(|p| p.panels.get(r.panel));
        out.push_str("  ");
        out.push_str(&describe_panel(r.page, r.panel, panel));
    }
    out
}

/// Re-prompt the writer with the specific violations and re-check.
pub async fn auto_fix(
    project: &Project,
    settings: &Settings,
    scope: &ScriptScope,
    bundle: &mut ScriptBundle,
    emitter: &Emitter,
) -> Result<()> {
    let Some(report) = bundle.compliance.clone() else {
        return Err(anyhow!("Run the compliance check first."));
    };
    let mut violations = compliance::violations_text(&report);
    violations.push_str(&missing_panel_brief(project, scope, &bundle.missing_panels));
    if violations.trim().is_empty() {
        emitter.info("Nothing to fix, the narration already passes every rule.");
        return Ok(());
    }

    let (client, model) = settings.resolve(ModelRole::Writer)?;
    emitter.stage("repair", "Fixing rule violations");

    // The repair pass needs the narrator prompt in context, not just the errors,
    // or it drifts back into summarizing.
    let messages = vec![
        ChatMessage::user_text(format!(
            "{}\n\n{}",
            settings.narrator_prompt(),
            settings.delivery_contract()
        )),
        ChatMessage::assistant_text(
            "Understood. Every panel keeps its tag and its own narration, in order.",
        ),
        ChatMessage::user_text(prompts::repair_user(
            &util::truncate(&bundle.tagged_script, 120_000),
            &violations,
        )),
    ];

    let (text, input, output) = ask_text(
        &client,
        &model,
        prompts::REPAIR_SYSTEM,
        messages,
        settings.max_output_tokens,
        0.4,
    )
    .await?;
    // The repair runs against a read-only project, so the tokens it costs are
    // reported in the log rather than added to the ledger.
    emitter.info(format!("Repair used {input} in / {output} out tokens"));

    let fixed = text.trim();
    if fixed.is_empty() {
        return Err(anyhow!(
            "The repair pass returned nothing. The narration was left unchanged."
        ));
    }

    let mut repaired = assemble(project, scope, fixed.to_string());
    let new_report = check_compliance(
        project,
        settings,
        scope,
        &repaired,
        settings.grounding_check,
        emitter,
    )
    .await?;
    emitter.info(format!(
        "Repair complete. Compliance score {} -> {}",
        report.score, new_report.score
    ));
    repaired.compliance = Some(new_report);
    *bundle = repaired;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn page_asset(index: usize, panels: usize) -> PageAsset {
        PageAsset {
            id: format!("page{index}"),
            index,
            chapter_index: 0,
            page_in_chapter: index,
            path: format!("/pages/{index}.png"),
            thumb_path: format!("/vision/{index}.png"),
            width: 800,
            height: 1200,
            label: format!("page-{index}.png"),
            panels: (0..panels)
                .map(|i| PanelAsset {
                    id: format!("panel{index}-{i}"),
                    index: i,
                    path: format!("/panels/{index}-{i}.png"),
                    x: 0,
                    y: (i as u32) * 300,
                    width: 800,
                    height: 300,
                    area_ratio: 0.25,
                })
                .collect(),
        }
    }

    fn reading(index: usize, panels: usize, role: PageRole) -> PageReading {
        PageReading {
            page_index: index,
            role,
            panels: (0..panels)
                .map(|i| PanelReading {
                    index: i,
                    description: format!("panel {i} of page {index}"),
                    action: "something happens".into(),
                    ..Default::default()
                })
                .collect(),
            ..Default::default()
        }
    }

    /// Two story pages of two panels, one splash page the analyzer returned with
    /// no panels at all, and one advert that is not part of the story.
    fn project() -> Project {
        let mut project = Project {
            meta: ProjectMeta {
                id: "p".into(),
                name: "Test".into(),
                created_at: String::new(),
                updated_at: String::new(),
                root: "/tmp/test".into(),
                format: SourceFormat::Manhwa,
                page_count: 4,
                chapter_count: 1,
                analyzed: true,
                has_script: false,
            },
            source: Source::default(),
            bible: StoryBible::default(),
            scripts: Default::default(),
            usage: Default::default(),
        };
        project.source.pages = vec![
            page_asset(0, 2),
            page_asset(1, 2),
            page_asset(2, 0),
            page_asset(3, 1),
        ];
        project.source.chapters = vec![ChapterRef {
            index: 0,
            number: Some(1.0),
            title: Some("Chapter 1".into()),
            origin: "test".into(),
            first_page: 0,
            last_page: 3,
        }];
        project.bible.readings = vec![
            reading(0, 2, PageRole::Story),
            reading(1, 2, PageRole::Story),
            reading(2, 0, PageRole::Story),
            reading(3, 1, PageRole::AdOrFiller),
        ];
        project
    }

    fn narration_for(panels: &[PanelRef]) -> String {
        panels
            .iter()
            .map(|p| {
                format!(
                    "{}\nHe stands very still, and for a long moment nothing in him moves at all.",
                    narrator::marker(p.page, p.panel)
                )
            })
            .collect::<Vec<_>>()
            .join("\n\n")
    }

    #[test]
    fn a_page_with_no_panels_still_counts_as_one() {
        let panels = panels_in_scope(&project(), &ScriptScope::WholeProject);
        assert_eq!(
            panels,
            vec![
                PanelRef { page: 0, panel: 0 },
                PanelRef { page: 0, panel: 1 },
                PanelRef { page: 1, panel: 0 },
                PanelRef { page: 1, panel: 1 },
                PanelRef { page: 2, panel: 0 },
            ]
        );
    }

    #[test]
    fn advertising_pages_are_not_narrated() {
        let panels = panels_in_scope(&project(), &ScriptScope::WholeProject);
        assert!(!panels.iter().any(|p| p.page == 3));
    }

    #[test]
    fn the_record_tags_every_panel_it_lists() {
        let project = project();
        let pages = narratable_pages(&project, &ScriptScope::WholeProject);
        let (record, refs) = panel_record(&pages);
        assert_eq!(refs.len(), 5);
        for r in &refs {
            assert!(
                record.contains(&narrator::marker(r.page, r.panel)),
                "record is missing {r:?}"
            );
        }
    }

    #[test]
    fn a_complete_narration_covers_every_panel_and_storyboards_one_shot_each() {
        let project = project();
        let scope = ScriptScope::WholeProject;
        let panels = panels_in_scope(&project, &scope);

        let bundle = assemble(&project, &scope, narration_for(&panels));

        assert_eq!(bundle.panel_count, 5);
        assert_eq!(bundle.narrated_panel_count, 5);
        assert!(bundle.missing_panels.is_empty());
        assert!(!bundle.final_script.contains("[["));

        // One shot per panel, in reading order, each pointing at its own image.
        assert_eq!(bundle.storyboard.len(), 5);
        let shot_panels: Vec<PanelRef> = bundle
            .storyboard
            .iter()
            .flat_map(|s| s.panels.clone())
            .collect();
        assert_eq!(shot_panels, panels);
        assert_eq!(bundle.storyboard[0].image_paths, vec!["/panels/0-0.png"]);
        // The page with no detected panels falls back to the whole page image.
        assert_eq!(bundle.storyboard[4].image_paths, vec!["/pages/2.png"]);
    }

    #[test]
    fn a_skipped_panel_is_reported_rather_than_lost() {
        let project = project();
        let scope = ScriptScope::WholeProject;
        let mut panels = panels_in_scope(&project, &scope);
        let dropped = panels.remove(2);

        let bundle = assemble(&project, &scope, narration_for(&panels));

        assert_eq!(bundle.panel_count, 5);
        assert_eq!(bundle.narrated_panel_count, 4);
        assert_eq!(bundle.missing_panels, vec![dropped]);
    }

    #[test]
    fn a_chapter_scope_only_covers_its_own_pages() {
        let mut project = project();
        project.source.chapters = vec![
            ChapterRef {
                index: 0,
                number: Some(1.0),
                title: Some("Chapter 1".into()),
                origin: "test".into(),
                first_page: 0,
                last_page: 0,
            },
            ChapterRef {
                index: 1,
                number: Some(2.0),
                title: Some("Chapter 2".into()),
                origin: "test".into(),
                first_page: 1,
                last_page: 3,
            },
        ];
        let panels = panels_in_scope(&project, &ScriptScope::Chapter(0));
        assert_eq!(
            panels,
            vec![
                PanelRef { page: 0, panel: 0 },
                PanelRef { page: 0, panel: 1 }
            ]
        );
    }

    #[test]
    fn an_untagged_script_still_produces_a_storyboard() {
        let project = project();
        let scope = ScriptScope::WholeProject;
        let bundle = assemble(
            &project,
            &scope,
            "He wakes. He runs. He does not look back.".into(),
        );
        assert_eq!(bundle.narrated_panel_count, 0);
        assert_eq!(bundle.missing_panels.len(), 5);
        assert_eq!(bundle.storyboard.len(), 3);
    }
}
