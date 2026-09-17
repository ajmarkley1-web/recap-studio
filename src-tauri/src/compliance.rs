//! Compliance checking against the narrator prompt.
//!
//! The prompt makes a handful of promises that can be checked mechanically, and
//! the most important one is coverage: every panel narrated, in order, none
//! skipped and none merged away as minor. That is checked against the panel list
//! rather than trusted. The rest are the prompt's stated bans - no visual or
//! viewer language, no quoted dialogue, third person, prose only - plus a depth
//! check, because a panel answered with four words has been skipped in all but
//! name.

use crate::model::{ComplianceReport, PanelRef, RuleResult, RuleStatus};
use crate::narrator::NarratedPanel;
use crate::util;
use once_cell::sync::Lazy;
use regex::Regex;

// ---------------------------------------------------------------------------
// Coverage
// ---------------------------------------------------------------------------

/// How the narration lines up with the panels it was supposed to cover.
#[derive(Debug, Clone, Default)]
pub struct Coverage {
    /// Panels in scope, in reading order.
    pub expected: Vec<PanelRef>,
    /// Panels the narration actually covers, in the order it covers them.
    pub narrated: Vec<PanelRef>,
    /// In scope but never narrated.
    pub missing: Vec<PanelRef>,
    /// Tagged by the narration but not in scope.
    pub unknown: Vec<PanelRef>,
    /// Panels narrated out of reading order.
    pub out_of_order: Vec<PanelRef>,
    /// Panels whose narration is too thin to be a narration: (panel, words).
    pub thin: Vec<(PanelRef, usize)>,
    /// Panels whose narration is broken across a blank line instead of running
    /// as one block.
    pub split: Vec<PanelRef>,
}

/// A panel narrated in fewer words than this has been summarized, not narrated.
const THIN_PANEL_WORDS: usize = 12;

impl Coverage {
    pub fn measure(expected: &[PanelRef], narrated: &[NarratedPanel]) -> Self {
        let narrated_refs: Vec<PanelRef> = narrated.iter().map(|n| n.panel).collect();

        let missing: Vec<PanelRef> = expected
            .iter()
            .copied()
            .filter(|p| !narrated_refs.contains(p))
            .collect();
        let unknown: Vec<PanelRef> = narrated_refs
            .iter()
            .copied()
            .filter(|p| !expected.contains(p))
            .collect();

        // Reading order is page, then panel within the page. Anything that goes
        // backwards means the narration jumped around.
        let mut out_of_order = Vec::new();
        let mut previous: Option<PanelRef> = None;
        for p in &narrated_refs {
            if let Some(prev) = previous {
                if (p.page, p.panel) <= (prev.page, prev.panel) {
                    out_of_order.push(*p);
                }
            }
            previous = Some(*p);
        }

        let thin: Vec<(PanelRef, usize)> = narrated
            .iter()
            .map(|n| (n.panel, util::word_count(&n.text)))
            .filter(|(_, words)| *words < THIN_PANEL_WORDS)
            .collect();

        let split: Vec<PanelRef> = narrated
            .iter()
            .filter(|n| n.text.lines().any(|l| l.trim().is_empty()))
            .map(|n| n.panel)
            .collect();

        Self {
            expected: expected.to_vec(),
            narrated: narrated_refs,
            missing,
            unknown,
            out_of_order,
            thin,
            split,
        }
    }

    pub fn is_complete(&self) -> bool {
        self.missing.is_empty() && self.out_of_order.is_empty()
    }
}

fn label_panels(panels: &[PanelRef], limit: usize) -> Vec<String> {
    panels
        .iter()
        .take(limit)
        .map(|p| format!("page {}, panel {}", p.page + 1, p.panel + 1))
        .collect()
}

// ---------------------------------------------------------------------------
// Checks
// ---------------------------------------------------------------------------

struct Check {
    id: &'static str,
    label: &'static str,
    source: &'static str,
    status: RuleStatus,
    detail: String,
    offenders: Vec<String>,
    /// How much a failure costs the score.
    weight: u32,
}

impl Check {
    fn pass(
        id: &'static str,
        label: &'static str,
        source: &'static str,
        detail: impl Into<String>,
        weight: u32,
    ) -> Self {
        Self {
            id,
            label,
            source,
            status: RuleStatus::Pass,
            detail: detail.into(),
            offenders: Vec::new(),
            weight,
        }
    }

    fn fail(mut self, detail: impl Into<String>, offenders: Vec<String>) -> Self {
        self.status = RuleStatus::Fail;
        self.detail = detail.into();
        self.offenders = offenders;
        self
    }

    fn warn(mut self, detail: impl Into<String>, offenders: Vec<String>) -> Self {
        self.status = RuleStatus::Warn;
        self.detail = detail.into();
        self.offenders = offenders;
        self
    }

    fn into_result(self) -> RuleResult {
        RuleResult {
            id: self.id.to_string(),
            label: self.label.to_string(),
            status: self.status,
            detail: self.detail,
            offenders: self.offenders,
            source: self.source.to_string(),
        }
    }
}

const PROMPT: &str = "The narrator prompt";
const DELIVERY: &str = "The narrator prompt - delivery";

/// "Do not describe visual elements like panels, angles, or framing."
static VISUAL_TERMS: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        r"(?i)\b(in (this|the (first|second|third|next|last|final)?\s?)panel|the panel|panels?\b|this frame|the frame|framing|splash page|the art|the artwork|the illustration|close[- ]up|wide shot|establishing shot|the shot|camera|zooms? (in|out)|pans? (to|across)|cuts? to|the angle|low angle|high angle|off[- ]panel|on the page|top of the page|bottom of the page|the gutter|speech bubble|thought bubble|word balloon)\b",
    )
    .unwrap()
});

/// "Avoid cinematic or viewer-based terms" and the 3rd-person requirement.
static VIEWER_TERMS: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        r"(?i)\b(we (see|watch|find|follow|get|cut|are shown|learn)|you (see|watch|can see|notice)|the (reader|viewer|audience)|our (hero|protagonist|view)|let'?s|the scene (shifts|changes|opens|cuts)|meanwhile,? back|as the scene|on screen|the screen)\b",
    )
    .unwrap()
});

/// "Dialogue should be embedded naturally into narration, not in quotes."
static QUOTED_DIALOGUE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r#"["“][^"”\n]{6,}["”]"#).unwrap());

/// Prose only: no headers, bullets, numbered lists or labels.
static MARKDOWN_LINE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?m)^\s*(#{1,6}\s|[-*+•]\s|\d+[.)]\s|>\s)").unwrap());
static BOLD_ITALIC: Lazy<Regex> = Lazy::new(|| Regex::new(r"\*\*|__|\*\w|_\w+_").unwrap());
static SCENE_LABEL: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?mi)^\s*(page\s*\d|panel\s*\d|scene\s*\d|int\.|ext\.|\[[^\]]{2,40}\]|[A-Z][A-Z \t]{3,30}:)")
        .unwrap()
});

/// "Do not open with a preamble or close with a sign-off."
static OUTRO: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?i)(thanks for watching|thank you for watching|see you (in the )?next|see you guys|don't forget to|dont forget to|like and subscribe|hit the like|smash that like|leave a comment|let me know in the comments|what do you think\?|that'?s it for (today|this|now)|stay tuned|catch you (in the )?next|until next time|hope you enjoyed|subscribe for more|in the next (video|episode)|to be continued|in summary|to summarize|in conclusion|here'?s (the|a) (narration|summary|retelling))").unwrap()
});

/// A tag that survived into the reading copy.
static LEFTOVER_TAG: Lazy<Regex> = Lazy::new(|| Regex::new(r"\[\[[^\]]*\]\]").unwrap());

/// "No em-dashes." An en-dash and a spaced double hyphen read the same aloud.
static EM_DASH: Lazy<Regex> = Lazy::new(|| Regex::new(r"[\u{2014}\u{2013}]|\s-{2,}\s").unwrap());

/// "No semicolons."
static SEMICOLON: Lazy<Regex> = Lazy::new(|| Regex::new(r";").unwrap());

/// A sentence longer than this is hard to say at a natural speaking pace.
const LONG_SENTENCE_WORDS: usize = 34;
/// Average sentence length above this stops sounding spoken.
const SPOKEN_AVG_WORDS: f32 = 24.0;

fn find_lines_with(text: &str, re: &Regex, limit: usize) -> Vec<String> {
    let mut out = Vec::new();
    for sentence in util::split_sentences(text) {
        if re.is_match(&sentence) {
            out.push(util::truncate(&sentence, 220));
            if out.len() >= limit {
                break;
            }
        }
    }
    if out.is_empty() {
        for m in re.find_iter(text).take(limit) {
            out.push(m.as_str().to_string());
        }
    }
    out
}

/// Every check this module can run, in report order. The app shows these so a
/// user can switch individual ones off.
pub const RULE_IDS: &[(&str, &str)] = &[
    ("panel_coverage", "Every panel narrated"),
    ("panel_order", "Panels narrated in reading order"),
    ("panel_depth", "No panel compressed to a clause"),
    ("panel_tags", "Panel tags resolve"),
    ("no_visual_terms", "No panels, angles or framing described"),
    ("no_viewer_terms", "Third person, no viewer-based terms"),
    ("dialogue_embedded", "Dialogue embedded, not in quotes"),
    ("prose_only", "Prose only, no headers, bullets or labels"),
    ("no_sign_off", "No preamble, sign-off or closing summary"),
    ("no_em_dash", "No em-dashes"),
    ("no_semicolon", "No semicolons"),
    ("single_block", "Single flowing block"),
    ("spoken_cadence", "Sounds spoken, not written"),
    ("narration_length", "Full, immersive length"),
];

/// Run every mechanical check. `script` is the reading copy, with the panel tags
/// already stripped; `coverage` is measured from the tagged copy. Checks whose
/// id is in `disabled` are left out of the report and out of the score, so
/// switching one off cannot quietly cost points.
pub fn check(script: &str, coverage: &Coverage, disabled: &[String]) -> ComplianceReport {
    let text = script.trim();
    let mut checks: Vec<Check> = Vec::new();

    // --- Every panel narrated ----------------------------------------------
    let expected = coverage.expected.len();
    let narrated = coverage.narrated.len();
    let c = Check::pass(
        "panel_coverage",
        "Every panel narrated",
        DELIVERY,
        format!("All {expected} panel(s) narrated."),
        30,
    );
    checks.push(if expected == 0 {
        c.warn(
            "No panels in scope to check against. Re-run Analyze if that looks wrong.".to_string(),
            Vec::new(),
        )
    } else if coverage.missing.is_empty() {
        c
    } else {
        c.fail(
            format!(
                "{} of {expected} panel(s) were skipped. Narrated {narrated}.",
                coverage.missing.len()
            ),
            label_panels(&coverage.missing, 20),
        )
    });

    // --- In reading order ---------------------------------------------------
    let c = Check::pass(
        "panel_order",
        "Panels narrated in reading order",
        DELIVERY,
        "The narration follows the page, panel by panel.",
        16,
    );
    checks.push(if coverage.out_of_order.is_empty() {
        c
    } else {
        c.fail(
            format!(
                "{} panel(s) are narrated out of order.",
                coverage.out_of_order.len()
            ),
            label_panels(&coverage.out_of_order, 10),
        )
    });

    // --- Panels not merged away ---------------------------------------------
    let c = Check::pass(
        "panel_depth",
        "No panel compressed to a clause",
        DELIVERY,
        "Every panel gets a real stretch of narration.",
        12,
    );
    checks.push(if coverage.thin.is_empty() {
        c
    } else if coverage.thin.len() * 5 <= narrated.max(1) {
        c.warn(
            format!(
                "{} panel(s) get fewer than {THIN_PANEL_WORDS} words.",
                coverage.thin.len()
            ),
            coverage
                .thin
                .iter()
                .take(8)
                .map(|(p, w)| format!("page {}, panel {} - {w} word(s)", p.page + 1, p.panel + 1))
                .collect(),
        )
    } else {
        c.fail(
            format!(
                "{} of {narrated} narrated panel(s) get fewer than {THIN_PANEL_WORDS} words, which is a summary, not a narration.",
                coverage.thin.len()
            ),
            coverage
                .thin
                .iter()
                .take(12)
                .map(|(p, w)| format!("page {}, panel {} - {w} word(s)", p.page + 1, p.panel + 1))
                .collect(),
        )
    });

    // --- Tags belong to this scope ------------------------------------------
    if !coverage.unknown.is_empty() {
        checks.push(
            Check::pass("panel_tags", "Panel tags resolve", DELIVERY, String::new(), 6).fail(
                format!(
                    "{} tag(s) point at panels that are not in this scope.",
                    coverage.unknown.len()
                ),
                label_panels(&coverage.unknown, 10),
            ),
        );
    }

    // --- No visual, panel or framing language -------------------------------
    let visual_hits = find_lines_with(text, &VISUAL_TERMS, 6);
    let c = Check::pass(
        "no_visual_terms",
        "No panels, angles or framing described",
        PROMPT,
        "Nothing points at the art itself.",
        16,
    );
    checks.push(if visual_hits.is_empty() {
        c
    } else {
        c.fail(
            format!(
                "{} sentence(s) describe the drawing instead of the story.",
                visual_hits.len()
            ),
            visual_hits,
        )
    });

    // --- No cinematic or viewer-based terms ---------------------------------
    let viewer_hits = find_lines_with(text, &VIEWER_TERMS, 6);
    let c = Check::pass(
        "no_viewer_terms",
        "Third person, no viewer-based terms",
        PROMPT,
        "Told to someone blindfolded, not to someone watching.",
        16,
    );
    checks.push(if viewer_hits.is_empty() {
        c
    } else {
        c.fail(
            format!(
                "{} sentence(s) address or include a viewer.",
                viewer_hits.len()
            ),
            viewer_hits,
        )
    });

    // --- Dialogue embedded, not quoted --------------------------------------
    let quote_hits = find_lines_with(text, &QUOTED_DIALOGUE, 6);
    let c = Check::pass(
        "dialogue_embedded",
        "Dialogue embedded, not in quotes",
        PROMPT,
        "No quoted speech.",
        12,
    );
    checks.push(if quote_hits.is_empty() {
        c
    } else {
        c.fail(
            format!("{} quoted line(s) of dialogue.", quote_hits.len()),
            quote_hits,
        )
    });

    // --- Prose only ---------------------------------------------------------
    let mut format_hits: Vec<String> = Vec::new();
    format_hits.extend(find_lines_with(text, &MARKDOWN_LINE, 4));
    format_hits.extend(find_lines_with(text, &BOLD_ITALIC, 2));
    format_hits.extend(find_lines_with(text, &SCENE_LABEL, 4));
    format_hits.extend(find_lines_with(text, &LEFTOVER_TAG, 2));
    let c = Check::pass(
        "prose_only",
        "Prose only, no headers, bullets or labels",
        DELIVERY,
        "Continuous prose throughout.",
        10,
    );
    checks.push(if format_hits.is_empty() {
        c
    } else {
        c.fail(
            format!("{} formatting artifact(s) found.", format_hits.len()),
            format_hits,
        )
    });

    // --- No preamble or sign-off --------------------------------------------
    let outro_hits = find_lines_with(text, &OUTRO, 4);
    let c = Check::pass(
        "no_sign_off",
        "No preamble, sign-off or closing summary",
        DELIVERY,
        "Starts in the story and ends in the story.",
        10,
    );
    checks.push(if outro_hits.is_empty() {
        c
    } else {
        c.fail(
            "The narration steps outside the story to address the audience.".to_string(),
            outro_hits,
        )
    });

    // --- No em-dashes -------------------------------------------------------
    let dash_hits = find_lines_with(text, &EM_DASH, 6);
    let c = Check::pass("no_em_dash", "No em-dashes", DELIVERY, "No em-dashes found.", 10);
    checks.push(if dash_hits.is_empty() {
        c
    } else {
        c.fail(
            format!(
                "{} sentence(s) contain an em-dash, en-dash or double hyphen.",
                dash_hits.len()
            ),
            dash_hits,
        )
    });

    // --- No semicolons ------------------------------------------------------
    let semi_hits = find_lines_with(text, &SEMICOLON, 6);
    let c = Check::pass("no_semicolon", "No semicolons", DELIVERY, "No semicolons found.", 10);
    checks.push(if semi_hits.is_empty() {
        c
    } else {
        c.fail(
            format!("{} sentence(s) contain a semicolon.", semi_hits.len()),
            semi_hits,
        )
    });

    // --- Single flowing block -----------------------------------------------
    // The reading copy is one paragraph per panel by construction, so "one
    // block" is checked where it can still mean something: inside a panel.
    // With no tags at all there is nothing to divide by, so the whole script is
    // held to the original rule.
    let c = Check::pass(
        "single_block",
        "Single flowing block",
        DELIVERY,
        "Each panel runs as one block.",
        8,
    );
    checks.push(if !coverage.narrated.is_empty() {
        if coverage.split.is_empty() {
            c
        } else {
            c.fail(
                format!(
                    "{} panel(s) are broken across a blank line instead of running as one block.",
                    coverage.split.len()
                ),
                label_panels(&coverage.split, 10),
            )
        }
    } else {
        let paragraphs = text.split("\n\n").filter(|p| !p.trim().is_empty()).count();
        if paragraphs <= 1 {
            c
        } else {
            c.warn(
                format!("Untagged script is split into {paragraphs} paragraphs."),
                Vec::new(),
            )
        }
    });

    // --- Sounds spoken, not written -----------------------------------------
    let sentences = util::split_sentences(text);
    let long: Vec<String> = sentences
        .iter()
        .filter(|s| util::word_count(s) > LONG_SENTENCE_WORDS)
        .take(5)
        .map(|s| util::truncate(s, 240))
        .collect();
    let avg = if sentences.is_empty() {
        0.0
    } else {
        util::word_count(text) as f32 / sentences.len() as f32
    };
    let c = Check::pass(
        "spoken_cadence",
        "Sounds spoken, not written",
        DELIVERY,
        format!("Average sentence length {avg:.0} words."),
        10,
    );
    checks.push(if long.is_empty() && avg <= SPOKEN_AVG_WORDS {
        c
    } else if long.len() <= 2 && avg <= SPOKEN_AVG_WORDS + 4.0 {
        c.warn(
            format!("{} long sentence(s). Average {avg:.0} words.", long.len()),
            long,
        )
    } else {
        c.fail(
            format!(
                "{} sentence(s) are hard to say at a natural speaking pace. Average {avg:.0} words.",
                long.len()
            ),
            long,
        )
    });

    // --- Enough narration to be worth listening to ---------------------------
    let words = util::word_count(text);
    let per_panel = if narrated == 0 {
        0.0
    } else {
        words as f32 / narrated as f32
    };
    let c = Check::pass(
        "narration_length",
        "Full, immersive length",
        PROMPT,
        format!("{words} words, about {per_panel:.0} per panel."),
        8,
    );
    checks.push(if narrated == 0 || per_panel >= 25.0 {
        c
    } else if per_panel >= 18.0 {
        c.warn(
            format!("{words} words, about {per_panel:.0} per panel. The prompt asks for full descriptions, so this is thin."),
            Vec::new(),
        )
    } else {
        c.fail(
            format!("{words} words, about {per_panel:.0} per panel. That is a summary, not the immersive narration the prompt asks for."),
            Vec::new(),
        )
    });

    // --- Drop what the user switched off ------------------------------------
    checks.retain(|c| !disabled.iter().any(|d| d == c.id));

    // --- Score --------------------------------------------------------------
    let total_weight: u32 = checks.iter().map(|c| c.weight).sum();
    let lost: u32 = checks
        .iter()
        .map(|c| match c.status {
            RuleStatus::Pass => 0,
            RuleStatus::Warn => c.weight / 2,
            RuleStatus::Fail => c.weight,
        })
        .sum();
    let score = if total_weight == 0 {
        100
    } else {
        (((total_weight - lost) as f32 / total_weight as f32) * 100.0).round() as u8
    };

    ComplianceReport {
        score,
        rules: checks.into_iter().map(Check::into_result).collect(),
        grounding: None,
        checked_at: util::now_iso(),
    }
}

/// Render failing rules as instructions for the repair pass.
pub fn violations_text(report: &ComplianceReport) -> String {
    let mut lines = Vec::new();
    for rule in &report.rules {
        if rule.status == RuleStatus::Pass {
            continue;
        }
        let marker = if rule.status == RuleStatus::Fail {
            "MUST FIX"
        } else {
            "SHOULD FIX"
        };
        lines.push(format!("{marker}: {} - {}", rule.label, rule.detail));
        for offender in rule.offenders.iter().take(8) {
            lines.push(format!("    offending text: {offender}"));
        }
    }
    if let Some(grounding) = &report.grounding {
        for claim in grounding.unsupported.iter().take(10) {
            lines.push(format!(
                "MUST FIX: Invented plot - \"{}\" ({})",
                util::truncate(&claim.sentence, 200),
                claim.reason
            ));
        }
    }
    lines.join("\n")
}

pub fn has_failures(report: &ComplianceReport) -> bool {
    report.rules.iter().any(|r| r.status == RuleStatus::Fail)
        || report
            .grounding
            .as_ref()
            .map(|g| !g.unsupported.is_empty())
            .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn panel(page: usize, index: usize) -> PanelRef {
        PanelRef { page, panel: index }
    }

    fn narrated(refs: &[(usize, usize)], words: usize) -> Vec<NarratedPanel> {
        refs.iter()
            .map(|(page, index)| NarratedPanel {
                panel: panel(*page, *index),
                text: "word ".repeat(words).trim().to_string(),
            })
            .collect()
    }

    fn status_of(report: &ComplianceReport, id: &str) -> RuleStatus {
        report
            .rules
            .iter()
            .find(|r| r.id == id)
            .map(|r| r.status)
            .unwrap_or(RuleStatus::Pass)
    }

    #[test]
    fn a_skipped_panel_fails_coverage() {
        let expected = vec![panel(0, 0), panel(0, 1), panel(0, 2)];
        let coverage = Coverage::measure(&expected, &narrated(&[(0, 0), (0, 2)], 30));
        assert_eq!(coverage.missing, vec![panel(0, 1)]);
        let report = check("word ".repeat(60).as_str(), &coverage, &[]);
        assert_eq!(status_of(&report, "panel_coverage"), RuleStatus::Fail);
    }

    #[test]
    fn full_coverage_in_order_passes() {
        let expected = vec![panel(0, 0), panel(0, 1), panel(1, 0)];
        let coverage = Coverage::measure(&expected, &narrated(&[(0, 0), (0, 1), (1, 0)], 30));
        assert!(coverage.is_complete());
        let report = check(&"word ".repeat(90), &coverage, &[]);
        assert_eq!(status_of(&report, "panel_coverage"), RuleStatus::Pass);
        assert_eq!(status_of(&report, "panel_order"), RuleStatus::Pass);
    }

    #[test]
    fn panels_narrated_backwards_fail_the_order_check() {
        let expected = vec![panel(0, 0), panel(0, 1)];
        let coverage = Coverage::measure(&expected, &narrated(&[(0, 1), (0, 0)], 30));
        let report = check(&"word ".repeat(60), &coverage, &[]);
        assert_eq!(status_of(&report, "panel_order"), RuleStatus::Fail);
    }

    #[test]
    fn a_panel_answered_in_four_words_is_a_merged_panel() {
        let expected = vec![panel(0, 0), panel(0, 1)];
        let coverage = Coverage::measure(&expected, &narrated(&[(0, 0), (0, 1)], 4));
        assert_eq!(coverage.thin.len(), 2);
        let report = check(&"word ".repeat(8), &coverage, &[]);
        assert_eq!(status_of(&report, "panel_depth"), RuleStatus::Fail);
    }

    #[test]
    fn panel_and_camera_language_fails() {
        let report = check(
            "In this panel he turns around. The camera zooms in on the blade.",
            &Coverage::default(),
            &[],
        );
        assert_eq!(status_of(&report, "no_visual_terms"), RuleStatus::Fail);
    }

    #[test]
    fn viewer_language_fails() {
        let report = check(
            "We see him step through the door. The reader already knows what waits inside.",
            &Coverage::default(),
            &[],
        );
        assert_eq!(status_of(&report, "no_viewer_terms"), RuleStatus::Fail);
    }

    #[test]
    fn quoted_dialogue_fails() {
        let report = check(
            "He looked up at her and said \"I am never going back there again\" before leaving.",
            &Coverage::default(),
            &[],
        );
        assert_eq!(status_of(&report, "dialogue_embedded"), RuleStatus::Fail);
    }

    #[test]
    fn embedded_dialogue_passes() {
        let report = check(
            "He looked up at her and told her, flatly, that he was never going back there again, and then he left without waiting for an answer.",
            &Coverage::default(),
            &[],
        );
        assert_eq!(status_of(&report, "dialogue_embedded"), RuleStatus::Pass);
    }

    #[test]
    fn expressive_prose_is_not_punished() {
        // Atmospheric, emotional, unhurried prose is exactly what the prompt asks
        // for. The punctuation rules constrain how it is written, not how rich it
        // is allowed to be, so none of these checks should fire on it.
        let script = "The wind came off the rooftop in a long, cold pull, and he let it \
                      move through him without flinching. The ache in his shoulder had \
                      gone quiet, which frightened him more than the pain had. Somewhere \
                      below, a door closed. Slowly, deliberately, he began to count.";
        let report = check(script, &Coverage::default(), &[]);
        let failures: Vec<&str> = report
            .rules
            .iter()
            .filter(|r| r.status == RuleStatus::Fail)
            .map(|r| r.id.as_str())
            .collect();
        assert!(failures.is_empty(), "unexpected failures: {failures:?}");
    }

    #[test]
    fn a_sign_off_fails() {
        let report = check(
            "He walks away from the ruin. Thanks for watching, see you next time.",
            &Coverage::default(),
            &[],
        );
        assert_eq!(status_of(&report, "no_sign_off"), RuleStatus::Fail);
    }

    #[test]
    fn em_dashes_and_semicolons_fail() {
        let report = check(
            "He runs on into the dark — fast; and he does not once look back.",
            &Coverage::default(),
            &[],
        );
        assert_eq!(status_of(&report, "no_em_dash"), RuleStatus::Fail);
        assert_eq!(status_of(&report, "no_semicolon"), RuleStatus::Fail);
    }

    #[test]
    fn an_en_dash_and_a_double_hyphen_count_as_em_dashes() {
        for sample in ["He waits – then moves.", "He waits -- then moves."] {
            let report = check(sample, &Coverage::default(), &[]);
            assert_eq!(
                status_of(&report, "no_em_dash"),
                RuleStatus::Fail,
                "missed: {sample}"
            );
        }
    }

    #[test]
    fn a_panel_broken_across_a_blank_line_fails_single_block() {
        let expected = vec![panel(0, 0)];
        let narrated = vec![NarratedPanel {
            panel: panel(0, 0),
            text: "He steps to the ledge and looks down at the street below.\n\nThen he \
                   turns back toward the door he came through."
                .into(),
        }];
        let coverage = Coverage::measure(&expected, &narrated);
        assert_eq!(coverage.split, vec![panel(0, 0)]);
        let report = check("He steps to the ledge.", &coverage, &[]);
        assert_eq!(status_of(&report, "single_block"), RuleStatus::Fail);
    }

    #[test]
    fn one_paragraph_per_panel_is_not_a_single_block_violation() {
        // This is what strip_markers always produces, so it must not fire.
        let expected = vec![panel(0, 0), panel(0, 1)];
        let coverage = Coverage::measure(&expected, &narrated(&[(0, 0), (0, 1)], 30));
        assert!(coverage.split.is_empty());
        let report = check("He wakes in the dark.\n\nHe runs for the door.", &coverage, &[]);
        assert_eq!(status_of(&report, "single_block"), RuleStatus::Pass);
    }

    #[test]
    fn a_sentence_too_long_to_say_aloud_fails() {
        let long = format!("He {} ran.", "slowly and ".repeat(20));
        let report = check(&long, &Coverage::default(), &[]);
        assert_eq!(status_of(&report, "spoken_cadence"), RuleStatus::Fail);
    }

    #[test]
    fn short_spoken_sentences_pass() {
        let report = check(
            "He wakes in the dark. The room is not his. He does not move for a long moment.",
            &Coverage::default(),
            &[],
        );
        assert_eq!(status_of(&report, "spoken_cadence"), RuleStatus::Pass);
    }

    #[test]
    fn a_disabled_rule_leaves_the_report_entirely() {
        let script = "He runs on into the dark — fast; and he does not look back.";
        let on = check(script, &Coverage::default(), &[]);
        assert_eq!(status_of(&on, "no_em_dash"), RuleStatus::Fail);

        let off = check(
            script,
            &Coverage::default(),
            &["no_em_dash".to_string(), "no_semicolon".to_string()],
        );
        assert!(!off.rules.iter().any(|r| r.id == "no_em_dash"));
        assert!(!off.rules.iter().any(|r| r.id == "no_semicolon"));
        // And the score must not be dragged down by a rule that was switched off.
        assert!(off.score > on.score, "{} vs {}", off.score, on.score);
    }

    #[test]
    fn every_advertised_rule_id_can_actually_fire() {
        // RULE_IDS drives the settings UI, so a typo there would show the user a
        // toggle that controls nothing.
        let report = check("He walks.", &Coverage::default(), &[]);
        for (id, _) in RULE_IDS {
            if *id == "panel_tags" {
                continue; // only added when a tag points outside the scope
            }
            assert!(
                report.rules.iter().any(|r| r.id == *id),
                "RULE_IDS advertises \"{id}\" but check() never emits it"
            );
        }
    }

    #[test]
    fn a_leftover_tag_is_caught() {
        let report = check("[[0:1]] He turns the corner.", &Coverage::default(), &[]);
        assert_eq!(status_of(&report, "prose_only"), RuleStatus::Fail);
    }
}
