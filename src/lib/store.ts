import { create } from "zustand";
import { api, toAppError } from "./api";
import {
  scopeKey,
  wholeProject,
  type AppInfo,
  type LogLine,
  type Progress,
  type Project,
  type ProjectMeta,
  type ScriptBundle,
  type ScriptScope,
  type Settings,
} from "./types";

export type View =
  | "library"
  | "source"
  | "analyze"
  | "bible"
  | "script"
  | "storyboard"
  | "render"
  | "settings";

export interface Toast {
  id: number;
  tone: "info" | "success" | "error";
  title: string;
  body?: string;
  hint?: string;
}

/** Live state of whichever long job is running. */
export interface JobState {
  job: string;
  stage: string;
  current: number;
  total: number;
  message: string;
  running: boolean;
  error: string | null;
}

const idleJob: JobState = {
  job: "",
  stage: "",
  current: 0,
  total: 0,
  message: "",
  running: false,
  error: null,
};

interface Store {
  ready: boolean;
  info: AppInfo | null;
  settings: Settings | null;
  projects: ProjectMeta[];
  project: Project | null;
  view: View;
  scope: ScriptScope;
  job: JobState;
  logs: LogLine[];
  toasts: Toast[];

  boot: () => Promise<void>;
  setView: (view: View) => void;
  setScope: (scope: ScriptScope) => void;
  setSettings: (settings: Settings) => void;
  refreshSettings: () => Promise<void>;
  refreshProjects: () => Promise<void>;
  openProject: (root: string) => Promise<void>;
  setProject: (project: Project) => void;
  closeProject: () => void;

  applyProgress: (p: Progress) => void;
  pushLog: (l: LogLine) => void;
  clearLogs: () => void;
  startJob: (job: string, message: string) => void;
  endJob: () => void;

  toast: (t: Omit<Toast, "id">) => void;
  dismissToast: (id: number) => void;
  notifyError: (err: unknown, fallbackTitle?: string) => void;

  currentScript: () => ScriptBundle | null;
}

let toastSeq = 0;

export const useStore = create<Store>((set, get) => ({
  ready: false,
  info: null,
  settings: null,
  projects: [],
  project: null,
  view: "library",
  scope: wholeProject,
  job: idleJob,
  logs: [],
  toasts: [],

  boot: async () => {
    try {
      const [info, settings, projects] = await Promise.all([
        api.appInfo(),
        api.getSettings(),
        api.listProjects(),
      ]);
      set({ info, settings, projects, ready: true });
    } catch (err) {
      get().notifyError(err, "Could not start up");
      set({ ready: true });
    }
  },

  setView: (view) => set({ view }),
  setScope: (scope) => set({ scope }),
  setSettings: (settings) => set({ settings }),

  refreshSettings: async () => {
    try {
      set({ settings: await api.getSettings() });
    } catch (err) {
      get().notifyError(err, "Could not load settings");
    }
  },

  refreshProjects: async () => {
    try {
      set({ projects: await api.listProjects() });
    } catch (err) {
      get().notifyError(err, "Could not list projects");
    }
  },

  openProject: async (root) => {
    try {
      const project = await api.openProject(root);
      set({
        project,
        scope: wholeProject,
        logs: [],
        view: project.source.pages.length === 0 ? "source" : nextView(project),
      });
    } catch (err) {
      get().notifyError(err, "Could not open that project");
    }
  },

  setProject: (project) => set({ project }),
  closeProject: () => set({ project: null, view: "library", logs: [] }),

  applyProgress: (p) => {
    set({
      job: {
        job: p.job,
        stage: p.stage,
        current: p.current,
        total: p.total,
        message: p.message,
        running: !p.done,
        error: p.error,
      },
    });
    if (p.error) {
      get().toast({ tone: "error", title: "Job failed", body: p.error });
    }
  },

  pushLog: (l) =>
    set((s) => ({ logs: [...s.logs.slice(-299), l] })),
  clearLogs: () => set({ logs: [] }),

  startJob: (job, message) =>
    set({
      job: { job, stage: "starting", current: 0, total: 0, message, running: true, error: null },
    }),
  endJob: () => set({ job: idleJob }),

  toast: (t) => {
    const id = ++toastSeq;
    set((s) => ({ toasts: [...s.toasts, { ...t, id }] }));
    const ttl = t.tone === "error" ? 9000 : 4500;
    window.setTimeout(() => get().dismissToast(id), ttl);
  },

  dismissToast: (id) =>
    set((s) => ({ toasts: s.toasts.filter((t) => t.id !== id) })),

  notifyError: (err, fallbackTitle = "Something went wrong") => {
    const e = toAppError(err);
    get().toast({
      tone: "error",
      title: fallbackTitle,
      body: e.message,
      hint: e.hint ?? undefined,
    });
  },

  currentScript: () => {
    const { project, scope } = get();
    if (!project) return null;
    return project.scripts[scopeKey(scope)] ?? null;
  },
}));

/** Send the user to the furthest step their project has actually reached. */
function nextView(project: Project): View {
  if (project.source.pages.length === 0) return "source";
  if (project.bible.readings.length === 0) return "analyze";
  if (Object.keys(project.scripts).length === 0) return "script";
  return "script";
}

/** Step completion used by the sidebar and the wizard rail. */
export function stepStatus(project: Project | null) {
  const hasPages = !!project && project.source.pages.length > 0;
  const analyzed = !!project && project.bible.readings.length > 0;
  const scripts = project ? Object.values(project.scripts) : [];
  const hasScript = scripts.some((s) => s.final_script.trim().length > 0);
  const narrated = scripts.some((s) =>
    s.storyboard.some((shot) => !!shot.audio_path)
  );
  return { hasPages, analyzed, hasScript, narrated };
}
