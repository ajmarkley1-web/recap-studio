import { useEffect, useState } from "react";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { api, fileUrl } from "../lib/api";
import { useStore } from "../lib/store";
import type {
  FfmpegStatus,
  ProviderDescriptor,
  ProviderId,
  Settings,
  TtsProviderId,
  VoiceInfo,
} from "../lib/types";
import { Icon } from "../components/Icon";
import { ModelPicker } from "../components/ModelPicker";
import { Banner, Spinner } from "../components/ui";

export function SettingsView() {
  const settings = useStore((s) => s.settings);
  const info = useStore((s) => s.info);

  if (!settings || !info) return null;

  return (
    <div className="page">
      <div className="page-head">
        <h1>Settings</h1>
        <p>
          Keys are stored locally in this app's config folder and are never sent
          anywhere except to the provider you selected.
        </p>
      </div>

      <ProviderKeys />
      <ModelRoles />
      <EngineOptions />
      <NarrationSettings />
      <VideoSettings />
      <NarrationEngine />
      <About />
    </div>
  );
}

function ProviderKeys() {
  const info = useStore((s) => s.info)!;
  return (
    <div className="card">
      <div className="card-head">
        <h2>Providers</h2>
        <span className="faint small">
          Add a key for at least one. Ollama runs locally and needs none.
        </span>
      </div>
      <div className="col" style={{ gap: 4 }}>
        {info.providers.map((p) => (
          <ProviderRow key={p.id} provider={p} />
        ))}
      </div>
    </div>
  );
}

function ProviderRow({ provider }: { provider: ProviderDescriptor }) {
  const settings = useStore((s) => s.settings)!;
  const setSettings = useStore((s) => s.setSettings);
  const boot = useStore((s) => s.boot);
  const toast = useStore((s) => s.toast);
  const notifyError = useStore((s) => s.notifyError);

  const stored = settings.providers[provider.id];
  const [value, setValue] = useState("");
  const [baseUrl, setBaseUrl] = useState(
    stored?.base_url ?? provider.default_base_url
  );
  const [busy, setBusy] = useState<"save" | "test" | null>(null);
  const [expanded, setExpanded] = useState(false);

  useEffect(() => {
    setBaseUrl(
      settings.providers[provider.id]?.base_url ?? provider.default_base_url
    );
  }, [settings.providers, provider.id, provider.default_base_url]);

  async function saveKey() {
    setBusy("save");
    try {
      setSettings(await api.setProviderKey(provider.id, value));
      setValue("");
      await boot();
      toast({
        tone: "success",
        title: value.trim() ? "Key saved" : "Key cleared",
        body: provider.label,
      });
    } catch (err) {
      notifyError(err, "Could not save that key");
    } finally {
      setBusy(null);
    }
  }

  async function saveBaseUrl() {
    try {
      setSettings(await api.setProviderBaseUrl(provider.id, baseUrl));
      toast({ tone: "success", title: "Endpoint saved" });
    } catch (err) {
      notifyError(err, "Could not save the endpoint");
    }
  }

  async function test() {
    setBusy("test");
    try {
      const message = await api.testProvider(provider.id);
      toast({ tone: "success", title: "Connected", body: message });
    } catch (err) {
      notifyError(err, `${provider.label} did not respond`);
    } finally {
      setBusy(null);
    }
  }

  const refreshedAt = stored?.models_refreshed_at;

  return (
    <div
      style={{
        border: "1px solid var(--border)",
        borderRadius: "var(--radius)",
        padding: "10px 12px",
      }}
    >
      <div className="row wrap">
        <strong style={{ minWidth: 150 }}>{provider.label}</strong>
        {provider.has_key ? (
          <span className="pill good">
            <Icon name="check" size={10} strokeWidth={3} />
            key saved
          </span>
        ) : provider.needs_api_key ? (
          <span className="pill warn">no key</span>
        ) : (
          <span className="pill">no key needed</span>
        )}
        {stored?.cached_models.length ? (
          <span className="pill">{stored.cached_models.length} models</span>
        ) : null}
        <div className="spacer" />
        <button
          className="btn ghost sm"
          onClick={() => setExpanded((v) => !v)}
        >
          {expanded ? "Hide" : "Configure"}
        </button>
        <button
          className="btn sm"
          onClick={() => void test()}
          disabled={busy !== null || (provider.needs_api_key && !provider.has_key)}
        >
          {busy === "test" ? <Spinner size={13} /> : <Icon name="link" size={13} />}
          Test
        </button>
      </div>

      {expanded && (
        <div className="mt-sm">
          {provider.needs_api_key && (
            <div className="row" style={{ alignItems: "stretch" }}>
              <input
                type="password"
                value={value}
                placeholder={
                  provider.has_key
                    ? "Enter a new key to replace the saved one"
                    : "Paste your API key"
                }
                onChange={(e) => setValue(e.target.value)}
                onKeyDown={(e) => e.key === "Enter" && void saveKey()}
                autoComplete="off"
              />
              <button
                className="btn"
                onClick={() => void saveKey()}
                disabled={busy !== null}
              >
                {busy === "save" ? <Spinner size={13} /> : null}
                Save
              </button>
            </div>
          )}

          <div className="row mt-sm" style={{ alignItems: "stretch" }}>
            <input
              type="text"
              value={baseUrl}
              onChange={(e) => setBaseUrl(e.target.value)}
              onBlur={() => void saveBaseUrl()}
              placeholder={provider.default_base_url}
              aria-label={`${provider.label} endpoint`}
            />
          </div>
          <div className="field-hint">
            {provider.id === "ollama"
              ? "Point this at your Ollama server. The default is http://localhost:11434."
              : "Change this only if you route through a proxy or a compatible gateway."}
            {refreshedAt &&
              ` Models last refreshed ${new Date(refreshedAt).toLocaleString()}.`}
          </div>
        </div>
      )}
    </div>
  );
}

function ModelRoles() {
  const settings = useStore((s) => s.settings)!;
  const [advanced, setAdvanced] = useState(
    !!settings.vision_override || !!settings.writer_override
  );

  return (
    <div className="card">
      <div className="card-head">
        <h2>Models</h2>
        <div className="spacer" />
        <button className="btn ghost sm" onClick={() => setAdvanced((v) => !v)}>
          {advanced ? "Simple" : "Use different models per job"}
        </button>
      </div>

      <ModelPicker
        role="primary"
        label="Main model"
        requireVision
        hint="Used for everything unless you override it below. It has to be a vision model, because the engine reads pages as images."
      />

      {advanced && (
        <>
          <ModelPicker
            role="vision"
            label="Page reading (optional override)"
            inheritLabel="Same as main model"
            requireVision
            hint="Reading every page is the expensive pass. A fast, cheap vision model here saves a lot without hurting the result much."
          />
          <ModelPicker
            role="writer"
            label="Script writing (optional override)"
            inheritLabel="Same as main model"
            hint="The narration itself. This is where a stronger model pays off most."
          />
        </>
      )}

      {!settings.primary.model && (
        <Banner tone="warn" title="Nothing selected yet">
          Add a provider key above, press Refresh next to the model dropdown,
          then pick a model.
        </Banner>
      )}
    </div>
  );
}

function EngineOptions() {
  const settings = useStore((s) => s.settings)!;
  const patch = usePatch();

  return (
    <div className="card">
      <div className="card-head">
        <h2>Analyze engine</h2>
      </div>

      <label className="field">
        <span className="field-label">
          Pages per AI call: {settings.pages_per_batch}
        </span>
        <input
          type="range"
          min={1}
          max={12}
          value={settings.pages_per_batch}
          onChange={(e) => patch({ pages_per_batch: Number(e.target.value) })}
        />
        <span className="field-hint">
          More pages per call is cheaper and faster. Fewer is more accurate on
          dense pages. Four is a good middle.
        </span>
      </label>

      <label className="field">
        <span className="field-label">
          Parallel calls: {settings.concurrency}
        </span>
        <input
          type="range"
          min={1}
          max={8}
          value={settings.concurrency}
          onChange={(e) => patch({ concurrency: Number(e.target.value) })}
        />
        <span className="field-hint">
          Lower this if the provider starts rate limiting you.
        </span>
      </label>

      <label className="field">
        <span className="field-label">
          Image size sent to the model: {settings.vision_max_dim}px
        </span>
        <input
          type="range"
          min={640}
          max={2048}
          step={64}
          value={settings.vision_max_dim}
          onChange={(e) => patch({ vision_max_dim: Number(e.target.value) })}
        />
        <span className="field-hint">
          Vision models bill by pixel area. Larger reads small speech bubbles
          better and costs more. Applies to the next import.
        </span>
      </label>

      <label className="check">
        <input
          type="checkbox"
          checked={settings.extract_panels}
          onChange={(e) => patch({ extract_panels: e.target.checked })}
        />
        <span className="check-body">
          <strong>Detect panels on import</strong>
          <span>
            Needed for the storyboard and the video. Runs locally, costs nothing.
          </span>
        </span>
      </label>

      <label className="check">
        <input
          type="checkbox"
          checked={settings.panel_level_vision}
          onChange={(e) => patch({ panel_level_vision: e.target.checked })}
        />
        <span className="check-body">
          <strong>Send individual panels instead of whole pages</strong>
          <span>
            Much more accurate on dense pages, and much more expensive, because
            each panel becomes its own image.
          </span>
        </span>
      </label>

      <label className="check">
        <input
          type="checkbox"
          checked={settings.grounding_check}
          onChange={(e) => patch({ grounding_check: e.target.checked })}
        />
        <span className="check-body">
          <strong>Run the grounding check on finished narrations</strong>
          <span>
            One extra call that flags any sentence the panel record does not
            support, so an expressive retelling never turns into an invented one.
          </span>
        </span>
      </label>
    </div>
  );
}

function NarrationSettings() {
  const settings = useStore((s) => s.settings)!;
  const setSettings = useStore((s) => s.setSettings);
  const patch = usePatch();
  const toast = useStore((s) => s.toast);
  const notifyError = useStore((s) => s.notifyError);

  const [key, setKey] = useState("");
  const [voices, setVoices] = useState<VoiceInfo[]>([]);
  const [models, setModels] = useState<VoiceInfo[]>([]);
  const [busy, setBusy] = useState(false);

  const provider = settings.tts.provider;

  async function loadVoices() {
    setBusy(true);
    try {
      const [v, m] = await Promise.all([api.listVoices(), api.listTtsModels()]);
      setVoices(v);
      setModels(m);
      toast({ tone: "success", title: `${v.length} voices loaded` });
    } catch (err) {
      notifyError(err, "Could not load voices");
    } finally {
      setBusy(false);
    }
  }

  async function saveKey() {
    try {
      setSettings(await api.setTtsKey(key));
      setKey("");
      toast({ tone: "success", title: "Narration key saved" });
    } catch (err) {
      notifyError(err, "Could not save that key");
    }
  }

  async function preview() {
    setBusy(true);
    try {
      const path = await api.previewVoice("");
      // Cache-bust so a second preview with a different voice actually replays.
      const audio = new Audio(`${fileUrl(path)}?t=${Date.now()}`);
      await audio.play();
    } catch (err) {
      notifyError(err, "Preview failed");
    } finally {
      setBusy(false);
    }
  }

  return (
    <div className="card">
      <div className="card-head">
        <h2>Narration</h2>
      </div>

      <div className="field">
        <span className="field-label">Provider</span>
        <div className="row wrap">
          {(
            [
              ["none", "Off"],
              ["elevenlabs", "ElevenLabs"],
              ["openai", "OpenAI"],
            ] as Array<[TtsProviderId, string]>
          ).map(([id, label]) => (
            <button
              key={id}
              className={`btn${provider === id ? " primary" : ""}`}
              onClick={() =>
                patch({ tts: { ...settings.tts, provider: id } })
              }
            >
              {label}
            </button>
          ))}
        </div>
      </div>

      {provider !== "none" && (
        <>
          <div className="field">
            <span className="field-label">
              {provider === "elevenlabs" ? "ElevenLabs API key" : "OpenAI API key"}
            </span>
            <div className="row" style={{ alignItems: "stretch" }}>
              <input
                type="password"
                value={key}
                placeholder={
                  settings.tts.api_key
                    ? "A key is saved. Enter a new one to replace it."
                    : provider === "openai"
                    ? "Leave empty to reuse your OpenAI provider key"
                    : "Paste your key"
                }
                onChange={(e) => setKey(e.target.value)}
                autoComplete="off"
              />
              <button className="btn" onClick={() => void saveKey()}>
                Save
              </button>
              <button className="btn" onClick={() => void loadVoices()} disabled={busy}>
                {busy ? <Spinner size={13} /> : <Icon name="refresh" size={13} />}
                Load voices
              </button>
            </div>
          </div>

          <div className="grid two">
            <label className="field">
              <span className="field-label">Voice</span>
              <select
                value={settings.tts.voice_id}
                onChange={(e) =>
                  patch({ tts: { ...settings.tts, voice_id: e.target.value } })
                }
              >
                <option value="">
                  {voices.length ? "Choose a voice" : "Load voices first"}
                </option>
                {voices.map((v) => (
                  <option key={v.id} value={v.id}>
                    {v.name}
                    {v.description ? ` - ${v.description}` : ""}
                  </option>
                ))}
                {settings.tts.voice_id &&
                  !voices.some((v) => v.id === settings.tts.voice_id) && (
                    <option value={settings.tts.voice_id}>
                      {settings.tts.voice_id}
                    </option>
                  )}
              </select>
            </label>

            <label className="field">
              <span className="field-label">Model</span>
              <select
                value={settings.tts.model_id}
                onChange={(e) =>
                  patch({ tts: { ...settings.tts, model_id: e.target.value } })
                }
              >
                <option value="">Provider default</option>
                {models.map((m) => (
                  <option key={m.id} value={m.id}>
                    {m.name || m.id}
                  </option>
                ))}
                {settings.tts.model_id &&
                  !models.some((m) => m.id === settings.tts.model_id) && (
                    <option value={settings.tts.model_id}>
                      {settings.tts.model_id}
                    </option>
                  )}
              </select>
            </label>
          </div>

          <label className="field">
            <span className="field-label">
              Speed: {settings.tts.speed.toFixed(2)}x
            </span>
            <input
              type="range"
              min={0.7}
              max={1.3}
              step={0.05}
              value={settings.tts.speed}
              onChange={(e) =>
                patch({
                  tts: { ...settings.tts, speed: Number(e.target.value) },
                })
              }
            />
          </label>

          {provider === "elevenlabs" && (
            <div className="grid two">
              <label className="field">
                <span className="field-label">
                  Stability: {settings.tts.stability.toFixed(2)}
                </span>
                <input
                  type="range"
                  min={0}
                  max={1}
                  step={0.05}
                  value={settings.tts.stability}
                  onChange={(e) =>
                    patch({
                      tts: {
                        ...settings.tts,
                        stability: Number(e.target.value),
                      },
                    })
                  }
                />
                <span className="field-hint">
                  Lower is more expressive, higher is more consistent.
                </span>
              </label>
              <label className="field">
                <span className="field-label">
                  Similarity: {settings.tts.similarity_boost.toFixed(2)}
                </span>
                <input
                  type="range"
                  min={0}
                  max={1}
                  step={0.05}
                  value={settings.tts.similarity_boost}
                  onChange={(e) =>
                    patch({
                      tts: {
                        ...settings.tts,
                        similarity_boost: Number(e.target.value),
                      },
                    })
                  }
                />
              </label>
            </div>
          )}

          <button
            className="btn"
            onClick={() => void preview()}
            disabled={busy || !settings.tts.voice_id}
          >
            <Icon name="play" size={13} filled />
            Preview this voice
          </button>
        </>
      )}
    </div>
  );
}

function VideoSettings() {
  const settings = useStore((s) => s.settings)!;
  const patch = usePatch();
  const [ffmpeg, setFfmpeg] = useState<FfmpegStatus | null>(null);

  useEffect(() => {
    api.ffmpegStatus().then(setFfmpeg).catch(() => setFfmpeg(null));
  }, [settings.video.ffmpeg_path]);

  const v = settings.video;

  async function pickFfmpeg() {
    const picked = await openDialog({ directory: true, multiple: false });
    if (typeof picked === "string") {
      patch({ video: { ...v, ffmpeg_path: picked } });
    }
  }

  return (
    <div className="card">
      <div className="card-head">
        <h2>Video</h2>
        <div className="spacer" />
        {ffmpeg?.available ? (
          <span className="pill good">
            <Icon name="check" size={10} strokeWidth={3} />
            ffmpeg found
          </span>
        ) : (
          <span className="pill warn">ffmpeg missing</span>
        )}
      </div>

      {ffmpeg && !ffmpeg.available && (
        <Banner
          tone="warn"
          title="ffmpeg is needed to render video"
          action={
            <button className="btn sm" onClick={() => void pickFfmpeg()}>
              Locate it
            </button>
          }
        >
          {ffmpeg.install_hint}
        </Banner>
      )}
      {ffmpeg?.available && (
        <p className="small faint mono truncate">{ffmpeg.ffmpeg_path}</p>
      )}

      <div className="grid two mt">
        <label className="field">
          <span className="field-label">Resolution</span>
          <select
            value={`${v.width}x${v.height}`}
            onChange={(e) => {
              const [width, height] = e.target.value.split("x").map(Number);
              patch({ video: { ...v, width, height } });
            }}
          >
            <option value="1920x1080">1920 x 1080 (1080p)</option>
            <option value="2560x1440">2560 x 1440 (1440p)</option>
            <option value="3840x2160">3840 x 2160 (4K)</option>
            <option value="1280x720">1280 x 720 (720p)</option>
            <option value="1080x1920">1080 x 1920 (vertical, shorts)</option>
          </select>
        </label>

        <label className="field">
          <span className="field-label">Frame rate</span>
          <select
            value={v.fps}
            onChange={(e) => patch({ video: { ...v, fps: Number(e.target.value) } })}
          >
            <option value={24}>24 fps</option>
            <option value={30}>30 fps</option>
            <option value={60}>60 fps</option>
          </select>
        </label>
      </div>

      <label className="field">
        <span className="field-label">
          Quality: crf {v.crf} {v.crf <= 18 ? "(near lossless)" : v.crf >= 26 ? "(small file)" : "(balanced)"}
        </span>
        <input
          type="range"
          min={14}
          max={30}
          value={v.crf}
          onChange={(e) => patch({ video: { ...v, crf: Number(e.target.value) } })}
        />
        <span className="field-hint">Lower is better quality and a bigger file.</span>
      </label>

      <label className="check">
        <input
          type="checkbox"
          checked={v.ken_burns}
          onChange={(e) => patch({ video: { ...v, ken_burns: e.target.checked } })}
        />
        <span className="check-body">
          <strong>Slow zoom on each shot</strong>
          <span>Keeps still panels from feeling static.</span>
        </span>
      </label>

      <label className="check">
        <input
          type="checkbox"
          checked={v.blurred_background}
          onChange={(e) =>
            patch({ video: { ...v, blurred_background: e.target.checked } })
          }
        />
        <span className="check-body">
          <strong>Blurred fill behind panels</strong>
          <span>
            Tall webtoon panels sit on a blurred copy of themselves instead of
            black bars.
          </span>
        </span>
      </label>

      <label className="field">
        <span className="field-label">
          Fade between shots: {v.fade_secs.toFixed(2)}s
        </span>
        <input
          type="range"
          min={0}
          max={1}
          step={0.05}
          value={v.fade_secs}
          onChange={(e) =>
            patch({ video: { ...v, fade_secs: Number(e.target.value) } })
          }
        />
      </label>
    </div>
  );
}

function NarrationEngine() {
  const settings = useStore((s) => s.settings)!;
  const patch = usePatch();

  return (
    <div className="card">
      <div className="card-head">
        <h2>Narration engine</h2>
      </div>
      <p className="card-sub">
        The narration covers every panel on every page it is given, so these two
        knobs decide how much work each call has to fit into one reply.
      </p>

      <label className="field">
        <span className="field-label">
          Pages per narration call: {settings.pages_per_narration}
        </span>
        <input
          type="range"
          min={1}
          max={16}
          value={settings.pages_per_narration}
          onChange={(e) =>
            patch({ pages_per_narration: Number(e.target.value) })
          }
        />
        <span className="field-hint">
          Fewer pages per call means more room per panel and less chance the
          model starts compressing to fit. Four is a good middle. Raise it only
          if your writer model has a very large output limit.
        </span>
      </label>

      <label className="field">
        <span className="field-label">
          Output tokens per call: {settings.max_output_tokens}
        </span>
        <input
          type="range"
          min={2048}
          max={65536}
          step={1024}
          value={settings.max_output_tokens}
          onChange={(e) => patch({ max_output_tokens: Number(e.target.value) })}
        />
        <span className="field-hint">
          Run this as high as your writer model allows. The prompt asks for full,
          unhurried descriptions, and a low ceiling is the one thing that forces
          it to cut a panel short.
        </span>
      </label>
    </div>
  );
}

function About() {
  const info = useStore((s) => s.info)!;
  return (
    <div className="card">
      <div className="card-head">
        <h2>About</h2>
      </div>
      <dl className="kv">
        <dt>Version</dt>
        <dd>{info.version}</dd>
        <dt>Projects folder</dt>
        <dd className="mono small">{info.projects_root}</dd>
        <dt>Config folder</dt>
        <dd className="mono small">{info.config_dir}</dd>
      </dl>
    </div>
  );
}

/** Patch settings and persist, keeping the UI responsive. */
function usePatch() {
  const settings = useStore((s) => s.settings);
  const setSettings = useStore((s) => s.setSettings);
  const notifyError = useStore((s) => s.notifyError);

  return (partial: Partial<Settings>) => {
    if (!settings) return;
    const next = { ...settings, ...partial };
    setSettings(next);
    void api.saveSettings(next).catch((err) => {
      notifyError(err, "Could not save settings");
      void api.getSettings().then(setSettings);
    });
  };
}

export type { ProviderId };
