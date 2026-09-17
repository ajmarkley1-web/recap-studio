# Recap Studio

A desktop app that reads manga and manhwa chapters panel by panel and narrates
every one of them, in order, as a single flowing third-person story ready to be
read aloud over the pages.

Built with Tauri 2 (Rust) and React. It is a remake of the existing legacy
project.

---

## What it does

**1. Ingest anything.** A PDF volume, a folder containing one folder per
chapter, or a pile of individual panel images. Chapter numbers are parsed out of
folder names (`Chapter 12`, `ch_007`, `Ep 5.5`), and PDFs get split into chapters
automatically during analysis wherever a printed chapter page appears in the art.

**2. Find the panels.** A recursive XY-cut segments every page into panels *in
reading order* — right-to-left for manga, left-to-right for comics, and strictly
top-to-bottom for vertical webtoon strips. This runs locally in Rust and costs
nothing.

**3. Analyze everything.** Eight passes turn raw pages into a story bible:

| Pass | What it produces |
| --- | --- |
| Perception | Every page and panel as structured data: characters present, actions, verbatim dialogue with speaker attribution, sound effects, shot framing, significance score |
| Chapters | Chapter boundaries corrected from chapter pages detected in the art |
| Cast | Observations merged into canonical characters — the same person recorded as "Jin Woo" on one page and "the white-haired hunter" on another becomes one entry with aliases |
| Portraits | A representative close-up panel chosen for each character |
| Event graph | Discrete events with causal links, stakes, importance, flashback detection, open threads, and foreshadowing paired to its payoff |
| Synthesis | A synopsis and ordered beat sheet per chapter, carrying continuity forward so chapter 40 knows what happened in chapter 3 |
| Classification | Genre, premise, setting, power system and overall tone |
| Save | Everything written to the project folder |

Nothing here filters panels out. Every panel the analyzer sees reaches the
narrator, whatever its significance score.

**4. Narrate it.** One prompt, one pass. The model is a master manga narrator: it
writes a third-person narrative in natural prose, like a light novel or a
dramatic short story. It never mentions panels, angles, framing or a camera, and
never addresses a viewer — the story is told as if to someone blindfolded.
Actions, reactions, inner thoughts and emotions are narrated in full, along with
the non-verbal beats: tension, hesitation, atmosphere. Dialogue is embedded into
the narration rather than quoted, and paraphrased where that reads better.
Nothing is skipped, however minor. Length is not a constraint; immersion is the
goal.

The only thing wrapped around that prompt is a delivery contract: each stretch of
narration is tagged with the panel it belongs to, as `[[page:panel]]`. That tag
is what makes "narrate every panel, in order, none merged" a thing the app can
check instead of hope for. The tags are stripped out of the reading copy and kept
for the storyboard.

Long chapters are narrated in segments of a few pages each, every segment running
at the full output-token allowance and carrying the story so far. That is what
stops an output limit from quietly forcing the model to merge panels to fit.

**5. Check it against the prompt.** Every narration is checked mechanically
before you read a word of it:

- every panel narrated, none skipped
- panels narrated in reading order
- no panel compressed down to a clause
- no panels, angles, framing or camera language
- third person, no viewer-based terms
- dialogue embedded, not quoted
- prose only — no headers, bullets, scene labels or stray tags
- no preamble, sign-off or closing summary
- full, immersive length rather than a summary

Plus a **grounding pass**: an AI check that flags any sentence asserting an event
the panel record does not support, so an expressive retelling never turns into an
invented one. Violations show with the exact offending text, and a one-click fix
re-prompts the writer with the specific failures — including the panels it never
narrated, with their record, so it writes them into place.

**6. Storyboard, narrate, render.** Every panel becomes one shot holding its own
narration. Narration audio is generated per shot, then the video holds each panel
on screen for exactly as long as its clip runs.

---

## Requirements

- **Windows, macOS or Linux** with a WebView runtime (WebView2 on Windows, which
  ships with Windows 11)
- **An API key** for at least one of OpenAI, Anthropic, Google Gemini, or a local
  **Ollama** server (which needs no key)
- **ffmpeg** — only for video rendering. Everything up to and including the
  narration works without it.
- To build from source: **Rust 1.77+**, **Node 18+**, and a C toolchain
  (MSVC Build Tools on Windows)

---

## Running it

```bash
npm install
npm run app
```

To produce an installer:

```bash
npm run app:build
```

Bundles land in `src-tauri/target/release/bundle/`.

---

## First run

1. Open **Settings**, expand a provider, paste your API key, press **Save**.
2. Press **Refresh** next to the model dropdown. The list comes from the
   provider's own API, so new models appear the day they ship.
3. Pick a model. It must support vision — the engine reads pages as images.
   Models that can see images are tagged `[vision]`.
4. Go to **Projects → New project**, name it, pick the format.
5. Drop in your source, press **Analyze**, then **Narrate**.

### Using different models for different jobs

Under Settings → Models, "Use different models per job" splits the work:

- **Page reading** is the expensive pass — every page is an image. A fast, cheap
  vision model here saves a lot.
- **Narration** is where a stronger model pays off most.

### Ollama

Ollama needs no key. Start it with `ollama serve`, pull a vision model
(`ollama pull llama3.2-vision` or `ollama pull qwen2.5vl`), then hit Refresh.
The picker tags which local models can actually see images.

---

## How a project is stored

Each project is a self-contained folder. Zip it, move it to another machine,
keep working.

```
<projects folder>/<name>/
  project.json     the whole model: source, story bible, narrations, token usage
  pages/           full-resolution pages
  vision/          downscaled copies sent to vision models
  panels/          cropped panels
  portraits/       character portraits
  audio/           narration clips, one per shot
  out/             exported narrations, storyboard, recap.mp4
```

API keys live in the app's config folder (shown at the bottom of Settings), not
in the project, so projects are safe to share.

---

## Cost control

Page reading dominates the bill. The knobs that matter, in Settings → Analyze
engine:

- **Pages per AI call** — more pages per call is cheaper. Four is a good middle.
- **Image size** — vision models bill by pixel area. 1280px reads most speech
  bubbles; drop it for cheap passes, raise it for dense art.
- **Send individual panels instead of whole pages** — much more accurate on
  dense pages, and much more expensive. Off by default.

Under Settings → Narration engine:

- **Pages per narration call** — fewer pages means more room per panel and less
  chance the model starts compressing to fit.
- **Output tokens per call** — run this as high as your writer model allows. A
  low ceiling is the one thing that forces the narration to cut a panel short.

Running token totals are shown on the Analyze screen after a run.

---

## Architecture notes

**PDF rasterization runs in the webview** with pdf.js, not in Rust. This keeps
the app free of a native PDF dependency, which is the usual source of
"works on my machine" desktop build failures. Pages are rendered one at a time
and handed straight to the backend, so a 300-page volume never sits in memory
all at once.

**Panel detection uses XY-cut**, not contour finding. It is deterministic, needs
no model weights (the legacy pipeline shipped a text-detector checkpoint it did
not actually use on the default path), and produces panels already in reading
order because the recursion mirrors how a page is read.

**Video is assembled per shot, then stream-copied together.** Each shot becomes
its own segment with its own narration, which guarantees audio sync; joining
them is lossless and fast.

**The prompt in [`src-tauri/src/narrator.rs`](src-tauri/src/narrator.rs) is
verbatim.** Do not paraphrase it. It is the entire voice of the app, and the
Prompt tab in the narration screen shows exactly what gets sent.

---

## Layout

```
src/                 React frontend
  lib/               API bindings, store, types, PDF rasterizer
  components/        shared UI
  views/             one file per screen
src-tauri/src/
  model.rs           the data model everything revolves around
  narrator.rs        the narrator prompt, verbatim, and the panel tags
  prompts.rs         the analyze engine's own prompts
  analyze.rs         the eight-pass pipeline
  script.rs          narration, coverage, storyboard, auto-fix
  compliance.rs      mechanical rule checking
  panels.rs          XY-cut panel segmentation
  providers/         OpenAI, Anthropic, Gemini, Ollama
  ingest.rs          folders, images, staged PDF pages
  tts.rs             ElevenLabs and OpenAI narration
  video.rs           ffmpeg orchestration
```

## Tests

```bash
cd src-tauri && cargo test
```

Covers panel segmentation, the compliance rules, panel-tag parsing and coverage,
chapter number parsing, and lenient JSON recovery from model output.

## License

MIT. See [LICENSE](LICENSE). A remake of an earlier MIT-licensed project,
whose copyright notice is retained there.
