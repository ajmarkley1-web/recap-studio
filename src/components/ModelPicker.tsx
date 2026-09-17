import { useEffect, useMemo, useState } from "react";
import { api } from "../lib/api";
import { useStore } from "../lib/store";
import type { ModelInfo, ModelRole, ProviderId } from "../lib/types";
import { Icon } from "./Icon";
import { Spinner } from "./ui";

interface Props {
  role: ModelRole;
  label: string;
  hint?: string;
  /** Shown as the option when nothing is selected, for the optional overrides. */
  inheritLabel?: string;
  /** Flag models that cannot see images. */
  requireVision?: boolean;
}

/**
 * Provider + model selector with a live refresh.
 *
 * The model list comes from the provider itself rather than a hardcoded table,
 * so new models show up the day they ship. Lists are cached in settings so the
 * picker is populated immediately on the next launch.
 */
export function ModelPicker({
  role,
  label,
  hint,
  inheritLabel,
  requireVision,
}: Props) {
  const settings = useStore((s) => s.settings);
  const info = useStore((s) => s.info);
  const setSettings = useStore((s) => s.setSettings);
  const toast = useStore((s) => s.toast);
  const notifyError = useStore((s) => s.notifyError);

  const current = useMemo(() => {
    if (!settings) return null;
    if (role === "primary") return settings.primary;
    if (role === "vision") return settings.vision_override;
    return settings.writer_override;
  }, [settings, role]);

  const fallbackProvider: ProviderId =
    current?.provider ?? settings?.primary.provider ?? "openai";
  const [provider, setProvider] = useState<ProviderId>(fallbackProvider);
  const [models, setModels] = useState<ModelInfo[]>([]);
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    setProvider(current?.provider ?? settings?.primary.provider ?? "openai");
  }, [current?.provider, settings?.primary.provider]);

  // Show whatever was cached the last time this provider was refreshed.
  useEffect(() => {
    let cancelled = false;
    api
      .cachedModels(provider)
      .then((list) => {
        if (!cancelled) setModels(list);
      })
      .catch(() => {
        if (!cancelled) setModels([]);
      });
    return () => {
      cancelled = true;
    };
  }, [provider, settings?.providers]);

  const providerHasKey =
    info?.providers.find((p) => p.id === provider)?.has_key ?? false;
  const needsKey =
    info?.providers.find((p) => p.id === provider)?.needs_api_key ?? true;

  async function refresh() {
    setBusy(true);
    try {
      const list = await api.refreshModels(provider);
      setModels(list);
      setSettings(await api.getSettings());
      toast({
        tone: "success",
        title: `${list.length} models loaded`,
        body: `From ${info?.providers.find((p) => p.id === provider)?.label ?? provider}`,
      });
    } catch (err) {
      notifyError(err, "Could not load models");
    } finally {
      setBusy(false);
    }
  }

  async function choose(model: string) {
    try {
      const next = await api.setModelChoice(role, provider, model);
      setSettings(next);
    } catch (err) {
      notifyError(err, "Could not save that choice");
    }
  }

  async function switchProvider(next: ProviderId) {
    setProvider(next);
    // Selecting a provider without a model would leave the role unusable, so
    // only commit once a model is picked. Clear the override in the meantime.
    if (role !== "primary" && current) {
      await choose("");
    }
  }

  const selectedModel = current?.model ?? "";
  const selectedInfo = models.find((m) => m.id === selectedModel);
  const visionProblem =
    requireVision && selectedInfo && !selectedInfo.vision ? true : false;

  return (
    <div className="field">
      <span className="field-label">{label}</span>
      <div className="row" style={{ alignItems: "stretch" }}>
        <select
          value={provider}
          onChange={(e) => void switchProvider(e.target.value as ProviderId)}
          style={{ maxWidth: 190 }}
          aria-label={`${label} provider`}
        >
          {info?.providers.map((p) => (
            <option key={p.id} value={p.id}>
              {p.label}
              {p.needs_api_key && !p.has_key ? " (no key)" : ""}
            </option>
          ))}
        </select>

        <select
          value={selectedModel}
          onChange={(e) => void choose(e.target.value)}
          aria-label={`${label} model`}
        >
          <option value="">
            {inheritLabel ??
              (models.length === 0
                ? "Refresh to load models"
                : "Choose a model")}
          </option>
          {models.map((m) => (
            <option key={m.id} value={m.id}>
              {m.label}
              {m.vision ? "  [vision]" : ""}
            </option>
          ))}
          {/* Keep a previously chosen model visible even if it is no longer listed. */}
          {selectedModel && !models.some((m) => m.id === selectedModel) && (
            <option value={selectedModel}>{selectedModel} (not in list)</option>
          )}
        </select>

        <button
          className="btn"
          onClick={() => void refresh()}
          disabled={busy || (needsKey && !providerHasKey)}
          title={
            needsKey && !providerHasKey
              ? "Add an API key for this provider first"
              : "Fetch the current model list from the provider"
          }
        >
          {busy ? <Spinner /> : <Icon name="refresh" size={14} />}
          Refresh
        </button>
      </div>

      {visionProblem && (
        <div className="field-hint" style={{ color: "var(--warn)" }}>
          {selectedInfo?.label} cannot read images. Page analysis needs a vision
          model.
        </div>
      )}
      {needsKey && !providerHasKey && (
        <div className="field-hint" style={{ color: "var(--warn)" }}>
          No API key saved for this provider yet. Add one above.
        </div>
      )}
      {hint && <div className="field-hint">{hint}</div>}
    </div>
  );
}
