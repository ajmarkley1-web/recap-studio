import { useEffect, useState } from "react";
import { api } from "../lib/api";
import { useStore } from "../lib/store";
import {
  FORMAT_LABELS,
  type ProjectMeta,
  type SourceFormat,
} from "../lib/types";
import { Icon } from "../components/Icon";
import { Banner, Empty, Modal } from "../components/ui";

export function Library() {
  const projects = useStore((s) => s.projects);
  const refresh = useStore((s) => s.refreshProjects);
  const open = useStore((s) => s.openProject);
  const settings = useStore((s) => s.settings);
  const setView = useStore((s) => s.setView);
  const [creating, setCreating] = useState(false);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  const needsSetup = !settings?.primary.model;

  return (
    <div className="page">
      <div className="page-head row">
        <div style={{ flex: 1 }}>
          <h1>Projects</h1>
          <p>
            A project holds one source, its story bible, and every script you
            generate from it.
          </p>
        </div>
        <button className="btn primary" onClick={() => setCreating(true)}>
          <Icon name="plus" size={15} />
          New project
        </button>
      </div>

      {needsSetup && (
        <div className="mb">
          <Banner
            tone="warn"
            title="Pick a model before you start"
            action={
              <button className="btn sm" onClick={() => setView("settings")}>
                Open Settings
              </button>
            }
          >
            Add an API key for OpenAI, Claude, Gemini or Ollama, refresh the
            model list, and choose a model. Everything else is ready.
          </Banner>
        </div>
      )}

      {projects.length === 0 ? (
        <Empty
          icon="library"
          title="No projects yet"
          action={
            <button className="btn primary" onClick={() => setCreating(true)}>
              <Icon name="plus" size={15} />
              Create your first project
            </button>
          }
        >
          Start one, drop in a PDF or a folder of chapters, and the analyze
          engine takes it from there.
        </Empty>
      ) : (
        <div className="grid three">
          {projects.map((p) => (
            <ProjectCard key={p.id} project={p} onOpen={() => void open(p.root)} />
          ))}
        </div>
      )}

      {creating && <NewProjectModal onClose={() => setCreating(false)} />}
    </div>
  );
}

function ProjectCard({
  project,
  onOpen,
}: {
  project: ProjectMeta;
  onOpen: () => void;
}) {
  const refresh = useStore((s) => s.refreshProjects);
  const toast = useStore((s) => s.toast);
  const notifyError = useStore((s) => s.notifyError);
  const [confirming, setConfirming] = useState(false);

  async function remove() {
    try {
      await api.deleteProject(project.root);
      toast({ tone: "success", title: "Project deleted" });
      await refresh();
    } catch (err) {
      notifyError(err, "Could not delete that project");
    } finally {
      setConfirming(false);
    }
  }

  return (
    <>
      <div className="project-card" onClick={onOpen} role="button" tabIndex={0}
        onKeyDown={(e) => e.key === "Enter" && onOpen()}>
        <div className="row">
          <h3 style={{ flex: 1, minWidth: 0 }} className="truncate">
            {project.name}
          </h3>
          <button
            className="btn ghost sm"
            title="Delete project"
            onClick={(e) => {
              e.stopPropagation();
              setConfirming(true);
            }}
          >
            <Icon name="trash" size={14} />
          </button>
        </div>

        <div className="project-meta">
          <span className="pill">{FORMAT_LABELS[project.format].split(" (")[0]}</span>
          {project.page_count > 0 && (
            <span className="pill">{project.page_count} pages</span>
          )}
          {project.chapter_count > 1 && (
            <span className="pill">{project.chapter_count} chapters</span>
          )}
          {project.analyzed && (
            <span className="pill good">
              <Icon name="check" size={10} strokeWidth={3} />
              analyzed
            </span>
          )}
          {project.has_script && <span className="pill amber">script ready</span>}
        </div>

        <div className="small faint">
          Updated {new Date(project.updated_at).toLocaleString()}
        </div>
      </div>

      {confirming && (
        <Modal title="Delete this project?" onClose={() => setConfirming(false)}>
          <p className="muted">
            This permanently removes <strong>{project.name}</strong> and every
            page, panel, script and render inside it.
          </p>
          <p className="mono faint small">{project.root}</p>
          <div className="row mt" style={{ justifyContent: "flex-end" }}>
            <button className="btn ghost" onClick={() => setConfirming(false)}>
              Cancel
            </button>
            <button className="btn danger" onClick={() => void remove()}>
              <Icon name="trash" size={14} />
              Delete permanently
            </button>
          </div>
        </Modal>
      )}
    </>
  );
}

function NewProjectModal({ onClose }: { onClose: () => void }) {
  const setProject = useStore((s) => s.setProject);
  const setView = useStore((s) => s.setView);
  const refresh = useStore((s) => s.refreshProjects);
  const notifyError = useStore((s) => s.notifyError);

  const [name, setName] = useState("");
  const [format, setFormat] = useState<SourceFormat>("manhwa");
  const [busy, setBusy] = useState(false);

  async function create() {
    if (!name.trim()) return;
    setBusy(true);
    try {
      const project = await api.createProject(name.trim(), format);
      setProject(project);
      await refresh();
      setView("source");
      onClose();
    } catch (err) {
      notifyError(err, "Could not create the project");
    } finally {
      setBusy(false);
    }
  }

  return (
    <Modal title="New project" onClose={onClose}>
      <label className="field">
        <span className="field-label">Series name</span>
        <input
          type="text"
          value={name}
          autoFocus
          placeholder="Solo Leveling, chapter 12"
          onChange={(e) => setName(e.target.value)}
          onKeyDown={(e) => e.key === "Enter" && void create()}
        />
        <span className="field-hint">
          Use whatever helps you find it later. It also seeds the series title if
          the pages do not print one.
        </span>
      </label>

      <div className="field">
        <span className="field-label">Format</span>
        <div className="col" style={{ gap: 6 }}>
          {(Object.keys(FORMAT_LABELS) as SourceFormat[]).map((f) => (
            <label key={f} className="check">
              <input
                type="radio"
                name="format"
                checked={format === f}
                onChange={() => setFormat(f)}
              />
              <span className="check-body">
                <strong>{FORMAT_LABELS[f].split(" (")[0]}</strong>
                <span>
                  {f === "manhwa" &&
                    "Vertical scroll strips. Panels are read straight down."}
                  {f === "manga" &&
                    "Printed pages. Panels are read right to left, then down."}
                  {f === "comic" &&
                    "Printed pages. Panels are read left to right, then down."}
                </span>
              </span>
            </label>
          ))}
        </div>
        <span className="field-hint">
          This sets panel reading order. You can change it later.
        </span>
      </div>

      <div className="row" style={{ justifyContent: "flex-end" }}>
        <button className="btn ghost" onClick={onClose}>
          Cancel
        </button>
        <button
          className="btn primary"
          onClick={() => void create()}
          disabled={busy || !name.trim()}
        >
          Create and add source
          <Icon name="chevron" size={14} />
        </button>
      </div>
    </Modal>
  );
}
