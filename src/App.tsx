import { useEffect } from "react";
import { api, onLog, onProgress } from "./lib/api";
import { stepStatus, useStore, type View } from "./lib/store";
import { Icon, type IconName } from "./components/Icon";
import { Toasts } from "./components/ui";
import { Library } from "./views/Library";
import { SourceView } from "./views/Source";
import { AnalyzeView } from "./views/Analyze";
import { BibleView } from "./views/Bible";
import { ScriptStudio } from "./views/ScriptStudio";
import { StoryboardView } from "./views/Storyboard";
import { RenderView } from "./views/Render";
import { SettingsView } from "./views/Settings";

interface Step {
  view: View;
  icon: IconName;
  label: string;
  /** Why the step is locked, when it is. */
  gate?: (s: ReturnType<typeof stepStatus>) => string | null;
}

const STEPS: Step[] = [
  { view: "source", icon: "source", label: "Source" },
  {
    view: "analyze",
    icon: "analyze",
    label: "Analyze",
    gate: (s) => (s.hasPages ? null : "Add pages first"),
  },
  {
    view: "bible",
    icon: "bible",
    label: "Story Bible",
    gate: (s) => (s.analyzed ? null : "Run Analyze first"),
  },
  {
    view: "script",
    icon: "script",
    label: "Script",
    gate: (s) => (s.analyzed ? null : "Run Analyze first"),
  },
  {
    view: "storyboard",
    icon: "storyboard",
    label: "Storyboard",
    gate: (s) => (s.hasScript ? null : "Generate a script first"),
  },
  {
    view: "render",
    icon: "render",
    label: "Narrate & Render",
    gate: (s) => (s.hasScript ? null : "Generate a script first"),
  },
];

export default function App() {
  const ready = useStore((s) => s.ready);
  const boot = useStore((s) => s.boot);
  const settings = useStore((s) => s.settings);
  const project = useStore((s) => s.project);
  const view = useStore((s) => s.view);
  const setView = useStore((s) => s.setView);
  const applyProgress = useStore((s) => s.applyProgress);
  const pushLog = useStore((s) => s.pushLog);
  const closeProject = useStore((s) => s.closeProject);

  useEffect(() => {
    void boot();
  }, [boot]);

  useEffect(() => {
    const unlisteners: Array<() => void> = [];
    void onProgress(applyProgress).then((fn) => unlisteners.push(fn));
    void onLog(pushLog).then((fn) => unlisteners.push(fn));
    return () => unlisteners.forEach((fn) => fn());
  }, [applyProgress, pushLog]);

  useEffect(() => {
    document.documentElement.dataset.theme = settings?.theme ?? "dark";
  }, [settings?.theme]);

  if (!ready) {
    return (
      <div className="empty" style={{ paddingTop: "22vh" }}>
        <div className="empty-icon">
          <Icon name="refresh" size={22} className="spin" />
        </div>
        <h2>Starting Recap Studio</h2>
      </div>
    );
  }

  const status = stepStatus(project);
  const doneFor: Record<string, boolean> = {
    source: status.hasPages,
    analyze: status.analyzed,
    bible: status.analyzed,
    script: status.hasScript,
    storyboard: status.hasScript,
    render: status.narrated,
  };

  return (
    <div className="app">
      <nav className="rail">
        <div className="rail-head">
          <div className="rail-logo">
            <Icon name="play" size={14} filled />
          </div>
          <div style={{ minWidth: 0 }}>
            <div className="rail-title">Recap Studio</div>
            <div className="rail-sub">Manhwa recap engine</div>
          </div>
        </div>

        <div className="rail-scroll">
          <button
            className={`nav-item${view === "library" ? " active" : ""}`}
            onClick={() => {
              closeProject();
              setView("library");
            }}
          >
            <Icon name="library" size={16} />
            <span className="nav-label">Projects</span>
          </button>

          {project && (
            <>
              <div className="rail-section">{project.meta.name}</div>
              {STEPS.map((step, i) => {
                const locked = step.gate?.(status) ?? null;
                return (
                  <button
                    key={step.view}
                    className={`nav-item${view === step.view ? " active" : ""}`}
                    disabled={!!locked}
                    title={locked ?? undefined}
                    onClick={() => setView(step.view)}
                  >
                    <span
                      className={`nav-step${doneFor[step.view] ? " done" : ""}`}
                    >
                      {doneFor[step.view] ? (
                        <Icon name="check" size={10} strokeWidth={3} />
                      ) : (
                        i + 1
                      )}
                    </span>
                    <span className="nav-label">{step.label}</span>
                  </button>
                );
              })}
            </>
          )}
        </div>

        <div className="rail-foot">
          <button
            className={`nav-item${view === "settings" ? " active" : ""}`}
            onClick={() => setView("settings")}
          >
            <Icon name="settings" size={16} />
            <span className="nav-label">Settings</span>
            {!settings?.primary.model && (
              <span className="pill warn" style={{ padding: "1px 6px" }}>
                setup
              </span>
            )}
          </button>
        </div>
      </nav>

      <main className="main">
        <TopBar />
        <div className="content">
          {view === "library" && <Library />}
          {view === "source" && <SourceView />}
          {view === "analyze" && <AnalyzeView />}
          {view === "bible" && <BibleView />}
          {view === "script" && <ScriptStudio />}
          {view === "storyboard" && <StoryboardView />}
          {view === "render" && <RenderView />}
          {view === "settings" && <SettingsView />}
        </div>
      </main>

      <Toasts />
    </div>
  );
}

const TITLES: Record<View, string> = {
  library: "Projects",
  source: "Source material",
  analyze: "Analyze engine",
  bible: "Story bible",
  script: "Script studio",
  storyboard: "Storyboard",
  render: "Narrate and render",
  settings: "Settings",
};

function TopBar() {
  const view = useStore((s) => s.view);
  const project = useStore((s) => s.project);
  const job = useStore((s) => s.job);
  const settings = useStore((s) => s.settings);
  const setSettings = useStore((s) => s.setSettings);

  async function toggleTheme() {
    if (!settings) return;
    const next = { ...settings, theme: settings.theme === "dark" ? "light" : "dark" };
    setSettings(next);
    await api.saveSettings(next).catch(() => undefined);
  }

  return (
    <header className="topbar">
      <div style={{ minWidth: 0 }}>
        <div className="topbar-title">{TITLES[view]}</div>
        {project && view !== "library" && view !== "settings" && (
          <div className="topbar-sub truncate">
            {project.meta.name} &middot; {project.source.pages.length} pages
            {project.source.chapters.length > 0 &&
              ` · ${project.source.chapters.length} chapter${
                project.source.chapters.length === 1 ? "" : "s"
              }`}
          </div>
        )}
      </div>
      <div className="spacer" />
      {job.running && (
        <span className="pill accent">
          <Icon name="refresh" size={11} className="spin" />
          {job.message || "Working"}
        </span>
      )}
      <button
        className="btn ghost sm"
        onClick={() => void toggleTheme()}
        title="Switch theme"
        aria-label="Switch theme"
      >
        <Icon name={settings?.theme === "light" ? "moon" : "sun"} size={15} />
      </button>
    </header>
  );
}
