import { useCallback, useEffect, useMemo, useState } from "react";
import { api } from "../lib/api";
import { useStore } from "../lib/store";
import {
  scopeKey,
  type NarratorPromptView,
  type RuleStatus,
} from "../lib/types";
import { Icon } from "../components/Icon";
import { Banner, Empty, Progress, ScoreRing } from "../components/ui";

type Tab = "final" | "tagged" | "prompt";

export function ScriptStudio() {
  const project = useStore((s) => s.project);
  const setProject = useStore((s) => s.setProject);
  const setView = useStore((s) => s.setView);
  const scope = useStore((s) => s.scope);
  const setScope = useStore((s) => s.setScope);
  const settings = useStore((s) => s.settings);
  const job = useStore((s) => s.job);
  const startJob = useStore((s) => s.startJob);
  const endJob = useStore((s) => s.endJob);
  const toast = useStore((s) => s.toast);
  const notifyError = useStore((s) => s.notifyError);

  const [tab, setTab] = useState<Tab>("final");
  const [editing, setEditing] = useState(false);
  const [editText, setEditText] = useState("");

  const bundle = project ? project.scripts[scopeKey(scope)] ?? null : null;
  const running = job.running && job.job === "script";

  // The editor works on the tagged copy, so a panel can be rewritten without
  // losing which panel it belongs to.
  useEffect(() => {
    setEditing(false);
    setEditText(bundle?.tagged_script ?? "");
  }, [bundle?.generated_at, bundle?.tagged_script]);

  if (!project) return <Empty icon="script" title="Open a project first" />;
  if (project.bible.readings.length === 0) {
    return (
      <Empty
        icon="script"
        title="Analyze the source first"
        action={
          <button className="btn primary" onClick={() => setView("analyze")}>
            Go to Analyze
          </button>
        }
      >
        The narration is written from the panel record, so the analysis has to
        run first.
      </Empty>
    );
  }

  async function generate() {
    if (!project) return;
    startJob("script", "Narrating");
    try {
      const updated = await api.generateScript({
        root: project.meta.root,
        scope,
      });
      setProject(updated);
      const fresh = updated.scripts[scopeKey(scope)];
      toast({
        tone: "success",
        title: "Narration ready",
        body: `${fresh?.final_word_count ?? 0} words across ${
          fresh?.narrated_panel_count ?? 0
        }/${fresh?.panel_count ?? 0} panels`,
      });
      setTab("final");
    } catch (err) {
      notifyError(err, "Could not write the narration");
    } finally {
      endJob();
    }
  }

  async function repair() {
    if (!project) return;
    startJob("script", "Fixing rule violations");
    try {
      setProject(await api.autoFixScript(project.meta.root, scope));
      toast({ tone: "success", title: "Violations fixed" });
    } catch (err) {
      notifyError(err, "Auto-fix failed");
    } finally {
      endJob();
    }
  }

  async function recheck() {
    if (!project) return;
    startJob("script", "Re-checking");
    try {
      setProject(
        await api.recheckCompliance(
          project.meta.root,
          scope,
          settings?.grounding_check ?? true
        )
      );
    } catch (err) {
      notifyError(err, "Check failed");
    } finally {
      endJob();
    }
  }

  async function saveEdit() {
    if (!project) return;
    try {
      setProject(
        await api.updateScriptText(project.meta.root, scope, editText)
      );
      setEditing(false);
      toast({ tone: "success", title: "Saved and re-checked" });
    } catch (err) {
      notifyError(err, "Could not save");
    }
  }

  async function exportAs(format: string) {
    if (!project) return;
    try {
      const path = await api.exportScript(project.meta.root, scope, format);
      toast({ tone: "success", title: "Exported", body: path });
    } catch (err) {
      notifyError(err, "Export failed");
    }
  }

  const covered = bundle
    ? bundle.panel_count > 0 &&
      bundle.narrated_panel_count === bundle.panel_count
    : false;

  return (
    <div className="page wide">
      <div className="page-head row wrap">
        <div style={{ flex: 1, minWidth: 240 }}>
          <h1>Narration</h1>
          <p>
            One prompt, one pass. Every panel in the source gets its own stretch
            of narration, in reading order, nothing skipped or merged.
          </p>
        </div>

        <select
          value={scope === "whole_project" ? "all" : String(scope.chapter)}
          onChange={(e) =>
            setScope(
              e.target.value === "all"
                ? "whole_project"
                : { chapter: Number(e.target.value) }
            )
          }
          style={{ maxWidth: 260 }}
          aria-label="Narration scope"
        >
          <option value="all">
            Whole source ({project.source.chapters.length} chapter
            {project.source.chapters.length === 1 ? "" : "s"})
          </option>
          {project.source.chapters.map((c) => (
            <option key={c.index} value={c.index}>
              {c.title ?? `Chapter ${c.index + 1}`}
            </option>
          ))}
        </select>

        <button
          className="btn primary"
          onClick={() => void generate()}
          disabled={running}
        >
          {running ? (
            <>
              <Icon name="refresh" size={15} className="spin" />
              Narrating
            </>
          ) : (
            <>
              <Icon name="wand" size={15} />
              {bundle ? "Regenerate" : "Narrate"}
            </>
          )}
        </button>
      </div>

      {running && (
        <div className="card mb">
          <div className="row mb">
            <Icon name="refresh" size={15} className="spin" />
            <strong style={{ flex: 1 }}>{job.message || "Working"}</strong>
          </div>
          <Progress current={job.current} total={job.total} indeterminate />
        </div>
      )}

      <div className="script-shell">
        <div>
          <div className="card">
            <div className="tabs" style={{ marginBottom: 12 }}>
              <button
                className={`tab${tab === "final" ? " active" : ""}`}
                onClick={() => setTab("final")}
              >
                Narration
                {bundle && ` (${bundle.final_word_count}w)`}
              </button>
              <button
                className={`tab${tab === "tagged" ? " active" : ""}`}
                onClick={() => setTab("tagged")}
              >
                Panel by panel
                {bundle && ` (${bundle.narrated_panel_count})`}
              </button>
              <button
                className={`tab${tab === "prompt" ? " active" : ""}`}
                onClick={() => setTab("prompt")}
              >
                Engine
              </button>
            </div>

            {tab === "final" &&
              (!bundle ? (
                <Empty
                  icon="wand"
                  title="No narration yet"
                  action={
                    <button
                      className="btn primary"
                      onClick={() => void generate()}
                      disabled={running}
                    >
                      <Icon name="wand" size={15} />
                      Narrate
                    </button>
                  }
                >
                  The engine walks the pages panel by panel and narrates each one
                  in order, at full length.
                </Empty>
              ) : (
                <>
                  <div className="row wrap mb">
                    <span className={`pill ${covered ? "good" : "amber"}`}>
                      {bundle.narrated_panel_count}/{bundle.panel_count} panels
                    </span>
                    <span className="pill">{bundle.final_word_count} words</span>
                    <span className="pill">
                      ~{Math.round((bundle.final_word_count / 150) * 60)}s read
                    </span>
                    <div className="spacer" />
                    <button
                      className="btn sm"
                      onClick={() => {
                        void navigator.clipboard.writeText(bundle.final_script);
                        toast({ tone: "success", title: "Copied to clipboard" });
                      }}
                    >
                      Copy
                    </button>
                  </div>
                  <div className="script-text">{bundle.final_script}</div>
                </>
              ))}

            {tab === "tagged" &&
              (!bundle ? (
                <Empty icon="script" title="No narration yet" />
              ) : editing ? (
                <>
                  <p className="card-sub">
                    Keep every <code>[[page:panel]]</code> tag on its own line.
                    They are what pins each stretch of narration to its panel.
                  </p>
                  <textarea
                    className="script-edit"
                    value={editText}
                    onChange={(e) => setEditText(e.target.value)}
                  />
                  <div className="row mt">
                    <button
                      className="btn primary"
                      onClick={() => void saveEdit()}
                    >
                      <Icon name="check" size={14} />
                      Save and re-check
                    </button>
                    <button
                      className="btn ghost"
                      onClick={() => {
                        setEditing(false);
                        setEditText(bundle.tagged_script);
                      }}
                    >
                      Cancel
                    </button>
                  </div>
                </>
              ) : (
                <>
                  <div className="row wrap mb">
                    <p className="card-sub" style={{ flex: 1, margin: 0 }}>
                      The same narration with its panel tags showing. This is the
                      copy the storyboard and the coverage check read.
                    </p>
                    <button className="btn sm" onClick={() => setEditing(true)}>
                      Edit
                    </button>
                  </div>
                  <div className="script-text">{bundle.tagged_script}</div>
                </>
              ))}

            {tab === "prompt" && <PromptView />}
          </div>
        </div>

        <div className="col">
          <div className="card">
            <div className="card-head">
              <h2>Coverage</h2>
            </div>
            {!bundle ? (
              <p className="field-hint">
                Nothing narrated yet for this scope.
              </p>
            ) : covered ? (
              <Banner title="Every panel narrated">
                All {bundle.panel_count} panels in this scope have their own
                narration, in reading order.
              </Banner>
            ) : (
              <div className="banner bad">
                <Icon name="alert" size={15} />
                <div style={{ minWidth: 0 }}>
                  <strong>
                    {bundle.missing_panels.length} panel
                    {bundle.missing_panels.length === 1 ? "" : "s"} skipped
                  </strong>
                  {bundle.missing_panels.slice(0, 6).map((p, i) => (
                    <div key={i} className="offender">
                      page {p.page + 1}, panel {p.panel + 1}
                    </div>
                  ))}
                  {bundle.missing_panels.length > 6 && (
                    <div className="faint small">
                      and {bundle.missing_panels.length - 6} more
                    </div>
                  )}
                </div>
              </div>
            )}
          </div>

          {bundle?.compliance && (
            <ComplianceCard
              onRepair={() => void repair()}
              onRecheck={() => void recheck()}
              busy={running}
            />
          )}

          {bundle && (
            <div className="card">
              <div className="card-head">
                <h2>Export</h2>
              </div>
              <div className="col" style={{ gap: 6 }}>
                <button className="btn sm" onClick={() => void exportAs("txt")}>
                  <Icon name="download" size={13} />
                  Narration (.txt)
                </button>
                <button className="btn sm" onClick={() => void exportAs("md")}>
                  <Icon name="download" size={13} />
                  With metadata (.md)
                </button>
                <button
                  className="btn sm"
                  onClick={() => void exportAs("tagged")}
                >
                  <Icon name="download" size={13} />
                  Panel by panel (.txt)
                </button>
                <button
                  className="btn sm"
                  onClick={() => void exportAs("storyboard")}
                >
                  <Icon name="download" size={13} />
                  Storyboard (.tsv)
                </button>
              </div>
              <button
                className="btn primary mt"
                style={{ width: "100%" }}
                onClick={() => setView("render")}
              >
                Narrate and render
                <Icon name="chevron" size={14} />
              </button>
            </div>
          )}
        </div>
      </div>
    </div>
  );
}

function PromptView() {
  const [engine, setEngine] = useState<NarratorPromptView | null>(null);
  const [prompt, setPrompt] = useState("");
  const [delivery, setDelivery] = useState("");
  const [saving, setSaving] = useState(false);
  const settings = useStore((s) => s.settings);
  const setSettings = useStore((s) => s.setSettings);
  const toast = useStore((s) => s.toast);
  const notifyError = useStore((s) => s.notifyError);

  const load = useCallback(() => {
    api
      .narratorPrompt()
      .then((e) => {
        setEngine(e);
        setPrompt(e.prompt);
        setDelivery(e.delivery);
      })
      .catch((err) => notifyError(err, "Could not load the engine"));
  }, [notifyError]);

  useEffect(load, [load]);

  const dirty =
    !!engine && (prompt !== engine.prompt || delivery !== engine.delivery);

  // An empty box means "use the shipped text", which is also how a reset is
  // stored, so the two paths cannot drift apart.
  async function persist(nextPrompt: string, nextDelivery: string) {
    if (!settings) return;
    setSaving(true);
    try {
      const isDefault = (v: string, d: string) => v.trim() === d.trim();
      const updated = await api.saveSettings({
        ...settings,
        narrator_prompt:
          engine && isDefault(nextPrompt, engine.default_prompt) ? "" : nextPrompt,
        delivery_contract:
          engine && isDefault(nextDelivery, engine.default_delivery)
            ? ""
            : nextDelivery,
      });
      setSettings(updated);
      load();
      toast({
        tone: "success",
        title: "Engine saved",
        body: "The next narration uses it. Existing scripts are unchanged.",
      });
    } catch (err) {
      notifyError(err, "Could not save the engine");
    } finally {
      setSaving(false);
    }
  }

  async function toggleRule(id: string, enabled: boolean) {
    if (!settings || !engine) return;
    const next = enabled
      ? engine.disabled_rules.filter((r) => r !== id)
      : [...engine.disabled_rules, id];
    try {
      const updated = await api.saveSettings({ ...settings, disabled_rules: next });
      setSettings(updated);
      load();
    } catch (err) {
      notifyError(err, "Could not change that rule");
    }
  }

  if (!engine) return <p className="card-sub">Loading the engine...</p>;

  return (
    <>
      <p className="card-sub">
        This is the whole engine, and all of it is yours to change. Edits apply
        to the next narration you generate.
      </p>

      {!engine.delivery_keeps_panel_tags && (
        <div className="banner bad mb">
          <Icon name="alert" size={15} />
          <div style={{ minWidth: 0 }}>
            <strong>The delivery rules no longer ask for panel tags</strong>
            <span className="small">
              Coverage checking and the storyboard are both built from{" "}
              <code>[[page:panel]]</code> tags. Without them the next narration
              reports every panel as skipped and the storyboard falls back to
              splitting on sentences.
            </span>
          </div>
        </div>
      )}

      <PromptBox
        title="The narrator prompt"
        subtitle="Sent as the system prompt on every pass. This is the voice."
        value={prompt}
        onChange={setPrompt}
        isCustom={engine.prompt_is_custom}
        onReset={() => {
          setPrompt(engine.default_prompt);
          void persist(engine.default_prompt, delivery);
        }}
        rows={14}
      />

      <PromptBox
        title="Delivery rules"
        subtitle="Sent with the panel record. This is what pins narration to panels and sets the output rules."
        value={delivery}
        onChange={setDelivery}
        isCustom={engine.delivery_is_custom}
        onReset={() => {
          setDelivery(engine.default_delivery);
          void persist(prompt, engine.default_delivery);
        }}
        rows={14}
      />

      <div className="row mt">
        <button
          className="btn primary"
          disabled={!dirty || saving}
          onClick={() => void persist(prompt, delivery)}
        >
          <Icon name="check" size={14} />
          {saving ? "Saving" : "Save engine"}
        </button>
        {dirty && (
          <button
            className="btn ghost"
            onClick={() => {
              setPrompt(engine.prompt);
              setDelivery(engine.delivery);
            }}
          >
            Discard changes
          </button>
        )}
      </div>

      <div className="card-head mt" style={{ marginBottom: 4 }}>
        <h2 style={{ fontSize: 15 }}>Mechanical checks</h2>
      </div>
      <p className="field-hint" style={{ marginTop: 0 }}>
        Switched off means left out of the report and out of the score, not
        shown as passing.
      </p>
      <div className="col" style={{ gap: 2 }}>
        {engine.rules.map(([id, label]) => {
          const enabled = !engine.disabled_rules.includes(id);
          return (
            <label key={id} className="check">
              <input
                type="checkbox"
                checked={enabled}
                onChange={(e) => void toggleRule(id, e.target.checked)}
              />
              <span className="check-body">
                <strong>{label}</strong>
              </span>
            </label>
          );
        })}
      </div>
    </>
  );
}

function PromptBox({
  title,
  subtitle,
  value,
  onChange,
  isCustom,
  onReset,
  rows,
}: {
  title: string;
  subtitle: string;
  value: string;
  onChange: (v: string) => void;
  isCustom: boolean;
  onReset: () => void;
  rows: number;
}) {
  return (
    <div
      style={{
        border: "1px solid var(--border)",
        borderRadius: "var(--radius)",
        overflow: "hidden",
        marginBottom: 10,
      }}
    >
      <div
        className="row"
        style={{ background: "var(--surface-2)", padding: "10px 12px" }}
      >
        <span style={{ flex: 1, minWidth: 0 }}>
          <strong className="small">{title}</strong>
          <div className="small faint">{subtitle}</div>
        </span>
        {isCustom && <span className="pill amber">edited</span>}
        {isCustom && (
          <button className="btn sm ghost" onClick={onReset}>
            Reset to default
          </button>
        )}
      </div>
      <textarea
        value={value}
        rows={rows}
        onChange={(e) => onChange(e.target.value)}
        style={{
          width: "100%",
          border: 0,
          borderRadius: 0,
          resize: "vertical",
          fontSize: 12.5,
          lineHeight: 1.65,
          padding: "12px 14px",
        }}
      />
    </div>
  );
}

const STATUS_MARK: Record<RuleStatus, string> = {
  pass: "✓",
  warn: "!",
  fail: "✕",
};

function ComplianceCard({
  onRepair,
  onRecheck,
  busy,
}: {
  onRepair: () => void;
  onRecheck: () => void;
  busy: boolean;
}) {
  const bundle = useStore((s) => s.currentScript())!;
  const report = bundle.compliance!;
  const [showPassing, setShowPassing] = useState(false);

  const failing = useMemo(
    () => report.rules.filter((r) => r.status !== "pass"),
    [report.rules]
  );
  const unsupported = report.grounding?.unsupported ?? [];
  const canRepair = failing.length > 0 || unsupported.length > 0;
  const shown = showPassing ? report.rules : failing;

  return (
    <div className="card">
      <div className="card-head">
        <h2 style={{ flex: 1 }}>Rule compliance</h2>
        <ScoreRing score={report.score} size={56} />
      </div>

      {!canRepair ? (
        <Banner title="Every rule passes">
          Nothing in this narration breaks the prompt.
        </Banner>
      ) : (
        <div className="row" style={{ gap: 6 }}>
          <button
            className="btn primary sm"
            onClick={onRepair}
            disabled={busy}
            style={{ flex: 1 }}
          >
            <Icon name="wand" size={13} />
            Fix {failing.length + unsupported.length} issue
            {failing.length + unsupported.length === 1 ? "" : "s"}
          </button>
          <button className="btn sm" onClick={onRecheck} disabled={busy}>
            <Icon name="refresh" size={13} />
          </button>
        </div>
      )}

      {unsupported.length > 0 && (
        <div className="banner bad mt">
          <Icon name="alert" size={15} />
          <div style={{ minWidth: 0 }}>
            <strong>
              {unsupported.length} sentence
              {unsupported.length === 1 ? "" : "s"} not supported by the source
            </strong>
            {unsupported.slice(0, 3).map((claim, i) => (
              <div key={i} className="offender">
                {claim.sentence}
                <div className="faint">{claim.reason}</div>
              </div>
            ))}
          </div>
        </div>
      )}

      <div className="mt">
        {shown.map((rule) => (
          <div key={rule.id} className="rule">
            <span className={`rule-icon ${rule.status}`}>
              {STATUS_MARK[rule.status]}
            </span>
            <div className="rule-body">
              <strong>{rule.label}</strong>
              <p>{rule.detail}</p>
              {rule.offenders.slice(0, 3).map((o, i) => (
                <div key={i} className="offender">
                  {o}
                </div>
              ))}
            </div>
          </div>
        ))}
      </div>

      <button
        className="btn ghost sm mt-sm"
        style={{ width: "100%" }}
        onClick={() => setShowPassing((v) => !v)}
      >
        {showPassing
          ? "Hide passing rules"
          : `Show all ${report.rules.length} rules`}
      </button>
    </div>
  );
}
