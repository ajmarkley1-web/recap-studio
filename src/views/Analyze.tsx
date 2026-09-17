import { useEffect, useRef } from "react";
import { api } from "../lib/api";
import { useStore } from "../lib/store";
import { Icon } from "../components/Icon";
import { Banner, Empty, Progress, Stat } from "../components/ui";

const STAGES: Array<{ id: string; label: string; detail: string }> = [
  {
    id: "perception",
    label: "Read every page",
    detail:
      "Each page and panel becomes structured data: who is on it, what they do, what they say, how much it matters.",
  },
  {
    id: "chapters",
    label: "Confirm chapter breaks",
    detail:
      "Chapter pages printed in the art are used to re-cut chapters when the source arrived as one blob.",
  },
  {
    id: "cast",
    label: "Resolve the cast",
    detail:
      "The same person shows up under a name on one page and a description on another. Those get merged into one character.",
  },
  {
    id: "portraits",
    label: "Pick portraits",
    detail: "A representative panel is chosen for each character.",
  },
  {
    id: "graph",
    label: "Build the event graph",
    detail:
      "Events, what caused what, open questions, and foreshadowing paired with its payoff.",
  },
  {
    id: "synthesis",
    label: "Summarize each chapter",
    detail:
      "A synopsis and an ordered beat sheet per chapter, carrying continuity forward.",
  },
  {
    id: "classify",
    label: "Classify the series",
    detail: "Genre, premise, setting, power system and overall tone.",
  },
];

export function AnalyzeView() {
  const project = useStore((s) => s.project);
  const setProject = useStore((s) => s.setProject);
  const setView = useStore((s) => s.setView);
  const settings = useStore((s) => s.settings);
  const job = useStore((s) => s.job);
  const logs = useStore((s) => s.logs);
  const startJob = useStore((s) => s.startJob);
  const endJob = useStore((s) => s.endJob);
  const clearLogs = useStore((s) => s.clearLogs);
  const toast = useStore((s) => s.toast);
  const notifyError = useStore((s) => s.notifyError);

  const logRef = useRef<HTMLDivElement>(null);
  useEffect(() => {
    logRef.current?.scrollTo({ top: logRef.current.scrollHeight });
  }, [logs.length]);

  if (!project) return <Empty icon="analyze" title="Open a project first" />;

  const analyzed = project.bible.readings.length > 0;
  const running = job.running && job.job === "analyze";
  const activeIndex = STAGES.findIndex((s) => s.id === job.stage);
  const configured = !!settings?.primary.model;

  async function run() {
    if (!project) return;
    clearLogs();
    startJob("analyze", "Starting the analyze engine");
    try {
      const updated = await api.analyzeProject(project.meta.root);
      setProject(updated);
      toast({
        tone: "success",
        title: "Analysis complete",
        body: `${updated.bible.characters.length} characters, ${updated.bible.events.length} events`,
      });
    } catch (err) {
      notifyError(err, "Analysis failed");
    } finally {
      endJob();
    }
  }

  const estimatedCalls =
    Math.ceil(
      project.source.pages.length / Math.max(1, settings?.pages_per_batch ?? 4)
    ) +
    3 +
    project.source.chapters.length;

  return (
    <div className="page">
      <div className="page-head">
        <h1>Analyze engine</h1>
        <p>
          Eight passes turn the raw pages into a story bible. The narration
          engine writes from that bible, which is what keeps it from inventing
          things that were never on the page.
        </p>
      </div>

      {!configured && (
        <div className="mb">
          <Banner
            tone="warn"
            title="No model selected"
            action={
              <button className="btn sm" onClick={() => setView("settings")}>
                Open Settings
              </button>
            }
          >
            Pick a vision-capable model before running the analysis.
          </Banner>
        </div>
      )}

      <div className="card">
        <div className="card-head">
          <h2>{analyzed ? "Re-run the analysis" : "Run the analysis"}</h2>
          <div className="spacer" />
          {analyzed && (
            <button className="btn" onClick={() => setView("bible")}>
              View story bible
              <Icon name="chevron" size={14} />
            </button>
          )}
          <button
            className="btn primary"
            onClick={() => void run()}
            disabled={running || !configured || project.source.pages.length === 0}
          >
            {running ? (
              <>
                <Icon name="refresh" size={15} className="spin" />
                Analyzing
              </>
            ) : (
              <>
                <Icon name="sparkle" size={15} />
                {analyzed ? "Re-analyze" : "Analyze"}
              </>
            )}
          </button>
        </div>

        <div className="stat-row mb">
          <Stat value={project.source.pages.length} label="pages" />
          <Stat value={project.source.total_panels} label="panels" />
          <Stat value={project.source.chapters.length} label="chapters" />
          <Stat value={`~${estimatedCalls}`} label="AI calls" />
        </div>

        {running && (
          <>
            <div className="row mb">
              <strong style={{ flex: 1 }}>{job.message || "Working"}</strong>
              {job.total > 0 && (
                <span className="faint small mono">
                  {job.current}/{job.total}
                </span>
              )}
            </div>
            <Progress current={job.current} total={job.total} />
          </>
        )}

        <div className="stage-list mt">
          {STAGES.map((stage, i) => {
            const isActive = running && stage.id === job.stage;
            const isDone = analyzed
              ? !running
              : running && activeIndex > i && activeIndex !== -1;
            return (
              <div
                key={stage.id}
                className={`stage${isActive ? " active" : isDone ? " done" : ""}`}
              >
                <span className="stage-dot" />
                <div style={{ minWidth: 0 }}>
                  <div>{stage.label}</div>
                  {(isActive || !running) && (
                    <div className="faint small">{stage.detail}</div>
                  )}
                </div>
              </div>
            );
          })}
        </div>
      </div>

      {logs.length > 0 && (
        <div className="card">
          <div className="card-head">
            <h2>Activity</h2>
            <div className="spacer" />
            <button className="btn ghost sm" onClick={clearLogs}>
              Clear
            </button>
          </div>
          <div className="log" ref={logRef}>
            {logs.map((line, i) => (
              <div key={i} className={`log-line ${line.level}`}>
                <span className="log-time">
                  {new Date(line.at).toLocaleTimeString()}
                </span>
                <span>{line.text}</span>
              </div>
            ))}
          </div>
        </div>
      )}

      {analyzed && !running && (
        <div className="card">
          <div className="card-head">
            <h2>What the engine found</h2>
            <div className="spacer" />
            <span className="pill">
              {project.usage.entries
                .reduce((n, e) => n + e.input_tokens + e.output_tokens, 0)
                .toLocaleString()}{" "}
              tokens used
            </span>
          </div>
          <div className="stat-row">
            <Stat value={project.bible.characters.length} label="characters" />
            <Stat value={project.bible.events.length} label="events" />
            <Stat value={project.bible.relationships.length} label="relationships" />
            <Stat value={project.bible.threads.length} label="open threads" />
            <Stat value={project.bible.foreshadowing.length} label="setups" />
          </div>
          {project.bible.work.premise && (
            <p className="mt muted">{project.bible.work.premise}</p>
          )}
          <div className="row mt">
            <button className="btn primary" onClick={() => setView("bible")}>
              Explore the story bible
              <Icon name="chevron" size={14} />
            </button>
            <button className="btn" onClick={() => setView("script")}>
              Skip to the script
            </button>
          </div>
        </div>
      )}
    </div>
  );
}
