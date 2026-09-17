//! The manga narrator engine.
//!
//! One prompt, transcribed verbatim below, is the whole voice of the app.
//! Everything the model is told about *how* to write lives in
//! `NARRATOR_PROMPT`; everything it is told about *what* to write lives in the
//! panel record that `narration_user` builds.
//!
//! The one thing wrapped around the prompt is the delivery contract: the model
//! marks each stretch of narration with the panel it belongs to, so nothing can
//! be skipped, reordered or quietly merged. The markers are stripped out of the
//! reading copy and kept for the storyboard.

use crate::model::PanelRef;
use once_cell::sync::Lazy;
use regex::Regex;

/// THE NARRATOR PROMPT. Verbatim. Do not paraphrase it.
pub const NARRATOR_PROMPT: &str = r#"You are a master manga narrator. Given sequential manga or manhwa pages, write a 3rd-person narrative that captures the full story in a natural prose style — like a light novel or dramatic short story.

Do not describe visual elements like panels, angles, or framing. Avoid cinematic or viewer-based terms (e.g., "camera zooms," "scene shifts," "we see," "in this panel"). Instead, describe the events as if you were telling a story to someone blindfolded.

Fully narrate all actions, reactions, inner thoughts, and emotions of characters. Include non-verbal cues like tension, hesitation, or atmosphere — not just spoken lines.

Dialogue should be embedded naturally into narration, not in quotes — paraphrased where appropriate.

Keep the narrative fluid, immersive, and sequential. Do not skip any part, even if it's minor. Use expressive language and full descriptions to maintain engagement.

Important: Treat each page like a continuous moment in the unfolding story. It's okay if the summary becomes long — quality and immersion are the goals."#;

/// The delivery contract wrapped around the prompt. It adds nothing to the
/// voice; it only pins the narration to the panel it came from, which is what
/// guarantees every panel is narrated once, in order.
pub const DELIVERY_CONTRACT: &str = r#"How to deliver it:

Every panel in the record below gets its own stretch of narration, in the exact order the record lists them. None is skipped. None is merged into another. None is summarized away as minor.

Before each stretch, write that panel's tag on its own line, exactly as the record prints it, in the form [[page:panel]]. Then write the narration for that panel underneath it. Move to the next tag only when that panel is fully narrated.

The tags are the only markup. Everything between them is flowing prose, with no headers, no bullets, no panel numbers spoken aloud in the text, and no labels of any kind. A reader who removes the tags should be left with one continuous story.

Each panel's narration is one unbroken paragraph. Never split a panel across a blank line.

It is written to be read aloud, so the punctuation stays simple. No em-dashes and no semicolons anywhere: where one of those would go, use a comma, or start a new sentence. Keep sentences short enough to say in one breath at a natural speaking pace. Break a long sentence in two rather than stacking clause on clause.

Narration for a panel is at least a couple of full sentences, and as many as the moment needs. A panel that only shows a character's face still has a reaction, a thought and a feeling to narrate. A panel with no dialogue still has a beat of tension, hesitation or atmosphere to carry.

Write it to be read aloud. Do not open with a preamble or close with a sign-off, a summary of what just happened, or any address to the audience. Begin at the first tag and end at the last one."#;

/// Marker the model writes before each panel's narration: `[[page:panel]]`.
static MARKER: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"\[\[\s*(\d+)\s*[:.\-]\s*(\d+)\s*\]\]").unwrap());

pub fn marker(page: usize, panel: usize) -> String {
    format!("[[{page}:{panel}]]")
}

/// One panel's worth of narration, already cut out of the model's reply.
#[derive(Debug, Clone, PartialEq)]
pub struct NarratedPanel {
    pub panel: PanelRef,
    pub text: String,
}

/// Split a tagged reply into per-panel narration, in the order the model wrote
/// it. Text before the first tag is dropped: it can only be a preamble, which
/// the contract forbids.
pub fn split_by_marker(text: &str) -> Vec<NarratedPanel> {
    let mut out: Vec<NarratedPanel> = Vec::new();
    let mut last: Option<(PanelRef, usize)> = None;

    for caps in MARKER.captures_iter(text) {
        let whole = caps.get(0).unwrap();
        if let Some((panel, start)) = last.take() {
            push_segment(&mut out, panel, &text[start..whole.start()]);
        }
        let page = caps[1].parse::<usize>().unwrap_or(0);
        let index = caps[2].parse::<usize>().unwrap_or(0);
        last = Some((PanelRef { page, panel: index }, whole.end()));
    }
    if let Some((panel, start)) = last {
        push_segment(&mut out, panel, &text[start..]);
    }
    out
}

fn push_segment(out: &mut Vec<NarratedPanel>, panel: PanelRef, body: &str) {
    let body = body.trim();
    if body.is_empty() {
        return;
    }
    // A repeated tag means the model split one panel over two stretches. Join
    // them rather than treating the panel as narrated twice.
    if let Some(existing) = out.iter_mut().find(|n| n.panel == panel) {
        existing.text.push(' ');
        existing.text.push_str(body);
        return;
    }
    out.push(NarratedPanel {
        panel,
        text: body.to_string(),
    });
}

/// The reading copy: the same prose with the tags taken out.
pub fn strip_markers(text: &str) -> String {
    let without = MARKER.replace_all(text, "");
    let mut out = String::with_capacity(without.len());
    let mut blank_run = 0usize;
    for line in without.lines() {
        // Trimmed both ends: a tag sitting on the same line as its narration
        // would otherwise leave the line starting with a stray space.
        let line = line.trim();
        if line.is_empty() {
            blank_run += 1;
            if blank_run > 1 {
                continue;
            }
            out.push('\n');
        } else {
            blank_run = 0;
            out.push_str(line);
            out.push('\n');
        }
    }
    out.trim().to_string()
}

/// Rebuild a tagged script from per-panel narration, for round-tripping an
/// edited script back through the storyboard.
pub fn join_with_markers(panels: &[NarratedPanel]) -> String {
    panels
        .iter()
        .map(|n| format!("{}\n{}", marker(n.panel.page, n.panel.panel), n.text))
        .collect::<Vec<_>>()
        .join("\n\n")
}

/// The user turn: the delivery contract, the continuity carried in, and the
/// panel-by-panel record of the pages being narrated.
pub fn narration_user(
    reading_hint: &str,
    work: &str,
    cast: &str,
    story_so_far: &str,
    panel_record: &str,
    tag_count: usize,
) -> String {
    let context = if story_so_far.trim().is_empty() {
        "This is where the story starts. Nothing has happened before it.".to_string()
    } else {
        format!("The story up to this point, for continuity and for names:\n{story_so_far}")
    };
    let series = if work.trim().is_empty() {
        String::new()
    } else {
        format!("\nThe series:\n{work}\n")
    };
    let who = if cast.trim().is_empty() {
        String::new()
    } else {
        format!("\nWho is in it, with the spellings to use:\n{cast}\n")
    };

    format!(
        r#"{reading_hint}
{series}{who}
{context}

{DELIVERY_CONTRACT}

Here is the record of what is on the pages, panel by panel, already in reading order. Every line beginning with a tag is one panel. Dialogue is transcribed as it was read off the page, so work from it but do not quote it back.

{panel_record}

That is {tag_count} panel(s). Narrate all {tag_count}, in that order, starting at the first tag."#
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn markers_split_into_panels_in_order() {
        let reply = "[[0:0]]\nHe wakes in the dark.\n\n[[0:1]]\nSomething moves behind him.";
        let parts = split_by_marker(reply);
        assert_eq!(parts.len(), 2);
        assert_eq!(parts[0].panel, PanelRef { page: 0, panel: 0 });
        assert_eq!(parts[0].text, "He wakes in the dark.");
        assert_eq!(parts[1].panel, PanelRef { page: 0, panel: 1 });
    }

    #[test]
    fn a_preamble_before_the_first_tag_is_dropped() {
        let parts = split_by_marker("Here is the narration:\n\n[[3:2]]\nShe does not move.");
        assert_eq!(parts.len(), 1);
        assert_eq!(parts[0].text, "She does not move.");
    }

    #[test]
    fn a_repeated_tag_is_joined_rather_than_duplicated() {
        let parts = split_by_marker("[[1:0]]\nFirst half.\n\n[[1:0]]\nSecond half.");
        assert_eq!(parts.len(), 1);
        assert_eq!(parts[0].text, "First half. Second half.");
    }

    #[test]
    fn stripping_markers_leaves_clean_prose() {
        let clean = strip_markers("[[0:0]]\nHe wakes.\n\n[[0:1]]\nHe runs.");
        assert!(!clean.contains("[["));
        assert_eq!(clean, "He wakes.\n\nHe runs.");
    }

    #[test]
    fn an_inline_tag_does_not_leave_a_leading_space() {
        assert_eq!(strip_markers("[[0:0]] He wakes."), "He wakes.");
    }

    #[test]
    fn loose_tag_punctuation_still_parses() {
        let parts = split_by_marker("[[ 12 . 4 ]] The door gives way.");
        assert_eq!(parts.len(), 1);
        assert_eq!(parts[0].panel, PanelRef { page: 12, panel: 4 });
    }

    #[test]
    fn the_prompt_asks_for_length_not_compression() {
        let lowered = NARRATOR_PROMPT.to_ascii_lowercase();
        for banned in ["compress", "a third", "module", "niche", "word count"] {
            assert!(!lowered.contains(banned), "prompt still mentions {banned}");
        }
        assert!(lowered.contains("do not skip any part"));
        assert!(lowered.contains("3rd-person"));
    }
}
