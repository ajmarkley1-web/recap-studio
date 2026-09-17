//! Prompts for the analyze engine.
//!
//! These build the story bible. They are separate from the narrator prompt in
//! `narrator.rs`, which is the only thing that decides how the narration reads.
//! Each pass here asks for strict JSON so the result can be merged into the
//! bible instead of being re-read by a human.

use crate::model::SourceFormat;

pub fn reading_order_hint(format: SourceFormat) -> &'static str {
    match format {
        SourceFormat::Manga => {
            "This is Japanese-style manga. Panels read RIGHT TO LEFT, then top to bottom. \
             The rightmost panel in a row comes first."
        }
        SourceFormat::Manhwa => {
            "This is a Korean-style manhwa webtoon. It is a vertical scroll. Panels read \
             strictly TOP TO BOTTOM in one column."
        }
        SourceFormat::Comic => {
            "This is a Western-style comic. Panels read LEFT TO RIGHT, then top to bottom."
        }
    }
}

// ---------------------------------------------------------------------------
// Pass 1 - page perception
// ---------------------------------------------------------------------------

pub const PERCEPTION_SYSTEM: &str = r#"You are a comics analyst building a structured record of a story so a narrator can later retell it panel by panel without re-reading the source. You look at pages and report exactly what is on them.

Rules you never break:
- Report only what is visibly on the page. Never guess at plot you cannot see.
- Transcribe dialogue as literally as you can read it. If text is unreadable, leave it out rather than inventing it.
- Attribute every line of dialogue to a speaker when the panel makes it clear. Use "unknown" when it does not.
- Use a character's printed name when you can read one. Otherwise use a stable visual descriptor like "white-haired swordsman" and reuse that exact descriptor for the same person on later pages.
- Rate significance honestly. Most panels are 10-40. Reserve 80+ for revelations, deaths, betrayals, power-ups and cliffhangers.

You reply with a single JSON object and nothing else."#;

/// `{pages}` is replaced with the per-page manifest for the batch.
pub fn perception_user(order_hint: &str, manifest: &str, context: &str) -> String {
    let context_block = if context.trim().is_empty() {
        String::new()
    } else {
        format!(
            "\nWhat has happened in the story so far, for continuity and name spelling:\n{context}\n"
        )
    };
    format!(
        r#"{order_hint}
{context_block}
The images that follow are pages, in order. This batch contains:
{manifest}

For each page, produce this JSON shape:

{{
  "pages": [
    {{
      "page_index": <the global index given in the manifest>,
      "role": "story" | "chapter_start" | "cover_or_title" | "character_profile" | "recap_page" | "author_note" | "ad_or_filler",
      "chapter_number": <number printed on the page, or null>,
      "chapter_title": "<title printed on the page, or null>",
      "location": "<where the scene takes place>",
      "time_of_day": "<day, night, unclear, etc>",
      "is_flashback": true | false,
      "panels": [
        {{
          "index": <0-based panel number in reading order>,
          "description": "<what is drawn>",
          "characters": ["<names present>"],
          "action": "<what happens in this panel>",
          "dialogue": [{{ "speaker": "<name or unknown>", "text": "<verbatim>", "kind": "speech" | "thought" | "narration" | "shout" | "whisper" | "system" }}],
          "sfx": ["<sound effects drawn in the art>"],
          "emotion": "<dominant emotion>",
          "significance": <0-100>,
          "shot": "close_up" | "wide" | "splash" | "reaction" | "establishing"
        }}
      ],
      "characters_present": [
        {{ "name": "<name or stable descriptor>", "appearance": "<hair, clothes, distinguishing features>", "role_guess": "protagonist" | "antagonist" | "support" | "minor", "confidence": 0.0-1.0 }}
      ],
      "raw_text": "<every readable word on the page, in reading order>",
      "events": [
        {{ "summary": "<one sentence, plot-relevant>", "kind": "fight" | "revelation" | "betrayal" | "power_up" | "death" | "confession" | "travel" | "flashback" | "contract" | "training" | "comedy" | "cliffhanger" | "other", "actors": ["<names>"], "importance": <0-100>, "panel_refs": [<panel indices>] }}
      ],
      "notes": "<anything a narrator would want flagged: a symbol, a callback, a detail planted for later>"
    }}
  ]
}}

Return one entry per page in the batch, in the same order. JSON only."#
    )
}

// ---------------------------------------------------------------------------
// Pass 2 - cast resolution
// ---------------------------------------------------------------------------

pub const CAST_SYSTEM: &str = r#"You merge raw per-page character observations into a clean cast list.

The same person is often recorded under several labels: a printed name on one page, a visual descriptor on another, a title or nickname on a third. Your job is to collapse those into one entry per real person, keeping the most likely true name as the canonical name and everything else as aliases.

Be conservative. Two similar descriptions are only the same person when the evidence supports it. When in doubt, keep them separate.

You reply with a single JSON object and nothing else."#;

pub fn cast_user(observations: &str) -> String {
    format!(
        r#"Here are the character observations collected across the whole source, grouped by the label they were recorded under. Each line shows the label, how many pages it appeared on, the pages, and the descriptions seen.

{observations}

Produce:

{{
  "characters": [
    {{
      "name": "<canonical name, the printed one if it exists>",
      "aliases": ["<every other label that refers to this same person>"],
      "role": "protagonist" | "deuteragonist" | "antagonist" | "support" | "minor",
      "appearance": "<consolidated physical description>",
      "personality": "<what the pages show of how they behave>",
      "abilities": ["<powers, skills, weapons, ranks>"],
      "affiliations": ["<guilds, families, schools, companies>"],
      "goals": "<what they appear to want>",
      "arc": "<how they change across the pages seen, or 'no visible change yet'>",
      "prominence": <0-100, how central they are to the story>
    }}
  ],
  "relationships": [
    {{
      "from": "<canonical name>",
      "to": "<canonical name>",
      "kind": "ally" | "rival" | "enemy" | "family" | "romantic" | "mentor" | "subordinate" | "unknown",
      "description": "<one sentence on what is between them>",
      "sentiment": <-100 hostile to 100 devoted>,
      "evidence_pages": [<page indices that show this>]
    }}
  ]
}}

Every alias you list must be one of the labels given above, spelled identically. Exactly one character should have role "protagonist" unless the story genuinely has none. JSON only."#
    )
}

// ---------------------------------------------------------------------------
// Pass 3 - event graph, threads, foreshadowing
// ---------------------------------------------------------------------------

pub const GRAPH_SYSTEM: &str = r#"You turn a list of observed story beats into a causal event graph.

You care about three things: what happened, what caused it, and what it set up. You merge duplicates, drop noise, and order events both as they were shown on the page and as they happened in story time, which differ whenever there is a flashback.

You reply with a single JSON object and nothing else."#;

pub fn graph_user(beats: &str, cast: &str) -> String {
    format!(
        r#"Cast, for consistent naming:
{cast}

Observed beats, in page order. Each line is: [page] (importance) kind - summary.
{beats}

Produce:

{{
  "events": [
    {{
      "id": "e1",
      "summary": "<one clear sentence>",
      "kind": "<fight, revelation, betrayal, power_up, death, confession, travel, flashback, contract, training, comedy, cliffhanger, other>",
      "actors": ["<canonical names>"],
      "page_start": <page index>,
      "page_end": <page index>,
      "importance": <0-100>,
      "is_flashback": true | false,
      "caused_by": ["<ids of events that directly led to this one>"],
      "stakes": "<what is at risk, or empty if nothing is>",
      "chronological_order": <1-based position in story time, not page order>
    }}
  ],
  "threads": [
    {{
      "question": "<an open question the story has raised>",
      "status": "open" | "resolved" | "partially_resolved",
      "opened_page": <page index>,
      "resolved_page": <page index or null>,
      "notes": "<why it matters>"
    }}
  ],
  "foreshadowing": [
    {{
      "setup": "<the detail that was planted>",
      "setup_page": <page index>,
      "payoff": "<what it paid off into, or null if it has not yet>",
      "payoff_page": <page index or null>,
      "confidence": 0.0-1.0
    }}
  ]
}}

Merge beats that describe the same moment into a single event. Keep events that a retelling would have to mention; drop pure scenery. JSON only."#
    )
}

// ---------------------------------------------------------------------------
// Pass 4 - chapter synthesis
// ---------------------------------------------------------------------------

pub const CHAPTER_SYSTEM: &str = r#"You write the structural summary of a single chapter: what happens, in order, with the beats a narrator would need.

A beat is one plot movement. Not a panel, not a sentence of description. If a fight takes four pages and ends with one character losing an arm, that is one or two beats, not twelve.

You reply with a single JSON object and nothing else."#;

pub fn chapter_user(chapter_label: &str, detail: &str, prior: &str) -> String {
    let prior_block = if prior.trim().is_empty() {
        "This is the first chapter in the source.".to_string()
    } else {
        format!("What happened before this chapter:\n{prior}")
    };
    format!(
        r#"{prior_block}

Now analyze {chapter_label}. Here is the page-by-page record:

{detail}

Produce:

{{
  "synopsis": "<a full paragraph covering everything that happens in this chapter, in order>",
  "beats": [
    {{
      "text": "<one plot movement, written plainly>",
      "importance": <0-100>,
      "characters": ["<who is involved>"],
      "quote": "<a short verbatim line of dialogue from this beat worth carrying into the narration, or null>",
      "page_refs": [<page indices this beat covers>]
    }}
  ],
  "cliffhanger": "<how the chapter ends and what it makes the reader want to know, or empty>",
  "tone": "<the emotional register of this chapter>"
}}

Beats must be in reading order and must cover the whole chapter. Only use quotes that appear verbatim in the record. JSON only."#
    )
}

// ---------------------------------------------------------------------------
// Pass 5 - work-level metadata
// ---------------------------------------------------------------------------

pub const WORK_SYSTEM: &str = r#"You classify a comic series from its analyzed content.

You reply with a single JSON object and nothing else."#;

pub fn work_user(cast: &str, chapters: &str, project_name: &str) -> String {
    format!(
        r#"Project name as entered by the user: "{project_name}"

Cast:
{cast}

Chapter summaries:
{chapters}

Produce:

{{
  "title": "<the series title if it appeared in the pages, otherwise the project name>",
  "genres": ["<two to five genre tags>"],
  "premise": "<two sentences describing what this story is about>",
  "setting": "<where and when it takes place>",
  "power_system": "<the rules of power, ranks, magic, levels or systems, or empty if none>",
  "tone": "<the overall register>"
}}

JSON only."#
    )
}

// ---------------------------------------------------------------------------
// Grounding check
// ---------------------------------------------------------------------------

pub const GROUNDING_SYSTEM: &str = r#"You are a fact checker for narration scripts. You are given the source record and a finished narration. You find sentences in the narration that state something the source record does not support.

You are checking for invented plot, not for style. This narration is meant to be expressive and immersive, so descriptive language, named emotions, inner thoughts drawn from a character's expression, atmosphere and paraphrased dialogue are all allowed and are not violations. A sentence is only unsupported when it asserts an event, action, relationship or revelation that is not in the record.

You reply with a single JSON object and nothing else."#;

pub fn grounding_user(record: &str, script: &str) -> String {
    format!(
        r#"Source record:
{record}

---

Finished script:
{script}

---

Produce:

{{
  "unsupported": [
    {{
      "sentence": "<the exact sentence from the script>",
      "reason": "<what it asserts that the record does not support>",
      "severity": "high" | "medium" | "low"
    }}
  ],
  "summary": "<one sentence on the overall faithfulness of the script>"
}}

Quote sentences exactly as they appear in the script. If everything traces back to the record, return an empty "unsupported" array. JSON only."#
    )
}

// ---------------------------------------------------------------------------
// Repair pass - fixing compliance violations
// ---------------------------------------------------------------------------

pub const REPAIR_SYSTEM: &str = r#"You fix a narration script that broke specific rules, changing as little as possible.

You keep every panel's narration. You keep the voice. You only touch what the violation list names. You never drop a moment to make a fix easier, and when a panel is named as missing you write the narration it never got.

Every panel tag in the form [[page:panel]] stays exactly where it is, on its own line, spelled exactly as it appears. Tags are never removed, renumbered or reordered, and a missing one is added back in its proper place. Everything between the tags is flowing prose.

Output the corrected script only. No commentary about what you changed."#;

pub fn repair_user(script: &str, violations: &str) -> String {
    format!(
        r#"Here is the script, tags and all:

{script}

---

These rules were broken:

{violations}

---

Rewrite the script with those violations fixed and nothing else changed. Keep every tag, in order, on its own line. Output the corrected script only."#
    )
}
