import { useEffect, useState } from "react";
import { openPath } from "@tauri-apps/plugin-opener";
import { api, fileUrl } from "../lib/api";
import { useStore } from "../lib/store";
import { scopeKey, type FfmpegStatus } from "../lib/types";
import { Icon } from "../components/Icon";
import { Banner, Empty, Progress, Stat } from "../components/ui";

export function RenderView() {
  const project = useStore((s) => s.project);
  const setProject = useStore((s) => s.setProject);
  const setView = useStore((s) => s.setView);
  const scope = useStore((s) => s.scope);
  const settings = useStore((s) => s.settings);
  const job = useStore((s) => s.job);
  const startJob = useStore((s) => s.startJob);
  const endJob = useStore((s) => s.endJob);
  const toast = useStore((s) => s.toast);
  const notifyError = useStore((s) => s.notifyError);

  const [ffmpeg, setFfmpeg] = useState<FfmpegStatus | null>(null);
  const [videoPath, setVideoPath] = useState<string | null>(null);

  useEffect(() => {
    api.ffmpegStatus().then(setFfmpeg).catch(() => setFfmpeg(null));
  }, [settings?.video.ffmpeg_path]);

  if (!project) return <Empty icon="render" title="Open a project first" />;

  const bundle = project.scripts[scopeKey(scope)] ?? null;
  if (!bundle) {
    return (
      <Empty
        icon="render"
        title="No script for this scope"
        action={
          <button className="btn primary" onClick={() => setView("script")}>
            Go to Script studio
          </button>
        }
      >
        Generate a script first, then narrate it and render the video.
      </Empty>
    );
  }

  const narrated = bundle.storyboard.filter((s) => s.audio_path).length;
  const allNarrated = narrated > 0 && narrated === bundle.storyboard.length;
  const ttsOff = (settings?.tts.provider ?? "none") === "none";
  const narrating = job.running && job.job === "narrate";
  const rendering = job.running && job.job === "render";

  async function narrate() {
    if (!project) return;
    startJob("narrate", "Starting narration");
    try {
      setProject(await api.narrateScript(project.meta.root, scope));
      toast({ tone: "success", title: "Narration complete" });
    } catch (err) {
      notifyError(err, "Narration failed");
    } finally {
      endJob();
    }
  }

  async function render() {
    if (!project) return;
    startJob("render", "Starting the render");
    try {
      const result = await api.renderVideo(project.meta.root, scope);
      setProject(result.project);
      setVideoPath(result.path);
      toast({
        tone: "success",
        title: "Video rendered",
        body: `${result.shots} shots, ${Math.round(result.duration_secs)}s`,
      });
    } catch (err) {
      notifyError(err, "Render failed");
    } finally {
      endJob();
    }
  }

  const estimatedSeconds = Math.round((bundle.final_word_count / 150) * 60);

  return (
    <div className="page">
      <div className="page-head">
        <h1>Narrate and render</h1>
        <p>
          Each panel gets its own narration clip, then the video holds that panel's
          panels on screen for exactly as long as the clip runs.
        </p>
      </div>

      <div className="card">
        <div className="card-head">
          <h2>Step 1 &middot; Narration</h2>
          <div className="spacer" />
          {allNarrated && (
            <span className="pill good">
              <Icon name="check" size={10} strokeWidth={3} />
              all {narrated} shots
            </span>
          )}
          <button
            className="btn primary"
            onClick={() => void narrate()}
            disabled={narrating || rendering || ttsOff}
          >
            {narrating ? (
              <>
                <Icon name="refresh" size={15} className="spin" />
                Narrating
              </>
            ) : (
              <>
                <Icon name="wave" size={15} />
                {narrated > 0 ? "Re-narrate" : "Narrate script"}
              </>
            )}
          </button>
        </div>

        {ttsOff ? (
          <Banner
            tone="warn"
            title="Narration is turned off"
            action={
              <button className="btn sm" onClick={() => setView("settings")}>
                Open Settings
              </button>
            }
          >
            Choose ElevenLabs or OpenAI under Narration in Settings, add a key,
            and pick a voice.
          </Banner>
        ) : (
          <div className="stat-row">
            <Stat value={bundle.storyboard.length} label="shots" />
            <Stat value={narrated} label="narrated" />
            <Stat value={bundle.final_word_count} label="words" />
            <Stat value={`~${estimatedSeconds}s`} label="estimated" />
          </div>
        )}

        {narrating && (
          <div className="mt">
            <div className="row mb">
              <span style={{ flex: 1 }}>{job.message}</span>
              <span className="mono faint small">
                {job.current}/{job.total}
              </span>
            </div>
            <Progress current={job.current} total={job.total} />
          </div>
        )}
      </div>

      <div className="card">
        <div className="card-head">
          <h2>Step 2 &middot; Video</h2>
          <div className="spacer" />
          <button
            className="btn primary"
            onClick={() => void render()}
            disabled={rendering || narrating || !ffmpeg?.available || narrated === 0}
          >
            {rendering ? (
              <>
                <Icon name="refresh" size={15} className="spin" />
                Rendering
              </>
            ) : (
              <>
                <Icon name="render" size={15} />
                Render recap.mp4
              </>
            )}
          </button>
        </div>

        {ffmpeg && !ffmpeg.available && (
          <Banner tone="warn" title="ffmpeg is needed for video">
            {ffmpeg.install_hint}
          </Banner>
        )}
        {ffmpeg?.available && (
          <p className="small faint mono truncate">{ffmpeg.version}</p>
        )}
        {narrated === 0 && ffmpeg?.available && (
          <Banner tone="warn" title="Narrate first">
            The renderer takes each shot's length from its narration clip, so
            narration has to run before the video.
          </Banner>
        )}

        {settings && (
          <div className="row wrap mt-sm">
            <span className="pill">
              {settings.video.width}x{settings.video.height}
            </span>
            <span className="pill">{settings.video.fps} fps</span>
            <span className="pill">crf {settings.video.crf}</span>
            {settings.video.ken_burns && <span className="pill">ken burns</span>}
            {settings.video.blurred_background && (
              <span className="pill">blurred fill</span>
            )}
            <button
              className="btn ghost sm"
              onClick={() => setView("settings")}
            >
              Change
            </button>
          </div>
        )}

        {rendering && (
          <div className="mt">
            <div className="row mb">
              <span style={{ flex: 1 }}>{job.message}</span>
              <span className="mono faint small">
                {job.total > 0 ? `${job.current}/${job.total}` : ""}
              </span>
            </div>
            <Progress current={job.current} total={job.total} />
          </div>
        )}

        {videoPath && !rendering && (
          <div className="mt">
            <video
              src={fileUrl(videoPath)}
              controls
              style={{
                width: "100%",
                borderRadius: "var(--radius)",
                border: "1px solid var(--border)",
                background: "#000",
              }}
            />
            <div className="row mt-sm">
              <span className="mono small faint truncate" style={{ flex: 1 }}>
                {videoPath}
              </span>
              <button
                className="btn sm"
                onClick={() => void openPath(videoPath).catch(() => undefined)}
              >
                <Icon name="external" size={13} />
                Open
              </button>
            </div>
          </div>
        )}
      </div>

      <div className="card">
        <div className="card-head">
          <h2>Project folder</h2>
        </div>
        <p className="small muted">
          Scripts, narration clips, panels and the finished video all live here.
        </p>
        <div className="row">
          <span className="mono small faint truncate" style={{ flex: 1 }}>
            {project.meta.root}
          </span>
          <button
            className="btn sm"
            onClick={() =>
              void openPath(project.meta.root).catch(() => undefined)
            }
          >
            <Icon name="folder" size={13} />
            Open folder
          </button>
        </div>
      </div>
    </div>
  );
}
