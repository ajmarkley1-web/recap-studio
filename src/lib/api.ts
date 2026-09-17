import { invoke } from "@tauri-apps/api/core";
import { convertFileSrc } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type {
  AppError,
  AppInfo,
  ComplianceReport,
  FfmpegStatus,
  LogLine,
  ModelInfo,
  ModelRole,
  Progress,
  Project,
  ProjectMeta,
  ProviderId,
  RenderResult,
  ScriptScope,
  Settings,
  SourceFormat,
  NarratorPromptView,
  VoiceInfo,
} from "./types";

/** Tauri rejects with our AppError shape; normalize anything else into it. */
export function toAppError(err: unknown): AppError {
  if (err && typeof err === "object" && "message" in err) {
    const e = err as Partial<AppError>;
    return {
      message: String(e.message ?? "Something went wrong"),
      kind: String(e.kind ?? "unknown"),
      hint: e.hint ?? null,
    };
  }
  return { message: String(err), kind: "unknown", hint: null };
}

async function call<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  try {
    return await invoke<T>(cmd, args);
  } catch (err) {
    throw toAppError(err);
  }
}

/** Turn an absolute file path into something an <img> or <video> can load. */
export function fileUrl(path: string | null | undefined): string {
  if (!path) return "";
  return convertFileSrc(path);
}

export const api = {
  // app
  appInfo: () => call<AppInfo>("app_info"),

  // settings
  getSettings: () => call<Settings>("get_settings"),
  saveSettings: (incoming: Settings) =>
    call<Settings>("save_settings", { incoming }),
  setProviderKey: (provider: ProviderId, apiKey: string) =>
    call<Settings>("set_provider_key", { provider, apiKey }),
  setProviderBaseUrl: (provider: ProviderId, baseUrl: string) =>
    call<Settings>("set_provider_base_url", { provider, baseUrl }),
  setTtsKey: (apiKey: string) => call<Settings>("set_tts_key", { apiKey }),
  setModelChoice: (role: ModelRole, provider: ProviderId, model: string) =>
    call<Settings>("set_model_choice", { role, provider, model }),
  refreshModels: (provider: ProviderId) =>
    call<ModelInfo[]>("refresh_models", { provider }),
  cachedModels: (provider: ProviderId) =>
    call<ModelInfo[]>("cached_models", { provider }),
  testProvider: (provider: ProviderId) =>
    call<string>("test_provider", { provider }),

  // projects
  listProjects: () => call<ProjectMeta[]>("list_projects"),
  createProject: (name: string, format: SourceFormat) =>
    call<Project>("create_project", { name, format }),
  openProject: (root: string) => call<Project>("open_project", { root }),
  deleteProject: (root: string) => call<void>("delete_project", { root }),
  updateProject: (
    root: string,
    patch: {
      name?: string;
      format?: SourceFormat;
    }
  ) => call<Project>("update_project", { root, patch }),

  // ingest
  stagePdfPage: (root: string, ordinal: number, bytes: number[]) =>
    call<void>("stage_pdf_page", { root, ordinal, bytes }),
  ingestPdf: (root: string, chapterLabel: string) =>
    call<Project>("ingest_pdf", { root, chapterLabel }),
  ingestPaths: (root: string, paths: string[]) =>
    call<Project>("ingest_paths", { root, paths }),

  // analyze
  analyzeProject: (root: string) => call<Project>("analyze_project", { root }),

  // scripts
  narratorPrompt: () => call<NarratorPromptView>("narrator_prompt"),
  generateScript: (request: { root: string; scope: ScriptScope }) =>
    call<Project>("generate_script", { request }),
  recheckCompliance: (root: string, scope: ScriptScope, grounding: boolean) =>
    call<Project>("recheck_compliance", { root, scope, grounding }),
  autoFixScript: (root: string, scope: ScriptScope) =>
    call<Project>("auto_fix_script", { root, scope }),
  updateScriptText: (root: string, scope: ScriptScope, text: string) =>
    call<Project>("update_script_text", { root, scope, text }),

  // narration
  listVoices: () => call<VoiceInfo[]>("list_voices"),
  listTtsModels: () => call<VoiceInfo[]>("list_tts_models"),
  previewVoice: (text: string) => call<string>("preview_voice", { text }),
  narrateScript: (root: string, scope: ScriptScope) =>
    call<Project>("narrate_script", { root, scope }),

  // video
  ffmpegStatus: () => call<FfmpegStatus>("ffmpeg_status"),
  renderVideo: (root: string, scope: ScriptScope) =>
    call<RenderResult>("render_video", { root, scope }),

  // export
  exportScript: (root: string, scope: ScriptScope, format: string) =>
    call<string>("export_script", { root, scope, format }),
  exportBible: (root: string) => call<string>("export_bible", { root }),
};

export type ComplianceLike = ComplianceReport;

export function onProgress(handler: (p: Progress) => void): Promise<UnlistenFn> {
  return listen<Progress>("recap://progress", (e) => handler(e.payload));
}

export function onLog(handler: (l: LogLine) => void): Promise<UnlistenFn> {
  return listen<LogLine>("recap://log", (e) => handler(e.payload));
}
