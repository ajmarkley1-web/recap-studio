import { useMemo, useState } from "react";
import { api, fileUrl } from "../lib/api";
import { useStore } from "../lib/store";
import { type Character } from "../lib/types";
import { Icon } from "../components/Icon";
import { Empty, Lightbox, Modal } from "../components/ui";

type Tab = "overview" | "cast" | "timeline" | "threads" | "chapters" | "pages";

const TABS: Array<{ id: Tab; label: string }> = [
  { id: "overview", label: "Overview" },
  { id: "cast", label: "Cast" },
  { id: "timeline", label: "Timeline" },
  { id: "threads", label: "Threads" },
  { id: "chapters", label: "Chapters" },
  { id: "pages", label: "Pages" },
];

export function BibleView() {
  const project = useStore((s) => s.project);
  const toast = useStore((s) => s.toast);
  const notifyError = useStore((s) => s.notifyError);
  const [tab, setTab] = useState<Tab>("overview");
  const [zoom, setZoom] = useState<string | null>(null);
  const [character, setCharacter] = useState<Character | null>(null);

  if (!project) return <Empty icon="bible" title="Open a project first" />;
  if (project.bible.readings.length === 0) {
    return (
      <Empty icon="bible" title="Nothing analyzed yet">
        Run the analyze engine and the story bible fills in.
      </Empty>
    );
  }

  const { bible } = project;

  async function exportBible() {
    if (!project) return;
    try {
      const path = await api.exportBible(project.meta.root);
      toast({ tone: "success", title: "Story bible exported", body: path });
    } catch (err) {
      notifyError(err, "Export failed");
    }
  }

  return (
    <div className="page wide">
      <div className="page-head row">
        <div style={{ flex: 1, minWidth: 0 }}>
          <h1>{bible.work.title || project.meta.name}</h1>
          <p>
            Everything the engine learned from the source. Every entry traces
            back to a page.
          </p>
        </div>
        <button className="btn" onClick={() => void exportBible()}>
          <Icon name="download" size={15} />
          Export JSON
        </button>
      </div>

      <div className="tabs">
        {TABS.map((t) => (
          <button
            key={t.id}
            className={`tab${tab === t.id ? " active" : ""}`}
            onClick={() => setTab(t.id)}
          >
            {t.label}
            {t.id === "cast" && ` (${bible.characters.length})`}
            {t.id === "timeline" && ` (${bible.events.length})`}
            {t.id === "threads" && ` (${bible.threads.length})`}
            {t.id === "chapters" && ` (${bible.chapters.length})`}
          </button>
        ))}
      </div>

      {tab === "overview" && <Overview />}
      {tab === "cast" && <Cast onOpen={setCharacter} />}
      {tab === "timeline" && <Timeline />}
      {tab === "threads" && <Threads />}
      {tab === "chapters" && <Chapters />}
      {tab === "pages" && <Pages onZoom={setZoom} />}

      <Lightbox src={zoom} onClose={() => setZoom(null)} />
      {character && (
        <CharacterModal character={character} onClose={() => setCharacter(null)} />
      )}
    </div>
  );
}

function Overview() {
  const project = useStore((s) => s.project)!;
  const { work } = project.bible;
  return (
    <>
      <div className="card">
        <div className="card-head">
          <h2>The series</h2>
          <div className="spacer" />
          {work.genres.map((g) => (
            <span key={g} className="pill">
              {g}
            </span>
          ))}
        </div>
        <dl className="kv">
          {work.premise && (
            <>
              <dt>Premise</dt>
              <dd>{work.premise}</dd>
            </>
          )}
          {work.setting && (
            <>
              <dt>Setting</dt>
              <dd>{work.setting}</dd>
            </>
          )}
          {work.power_system && (
            <>
              <dt>Power system</dt>
              <dd>{work.power_system}</dd>
            </>
          )}
          {work.tone && (
            <>
              <dt>Tone</dt>
              <dd>{work.tone}</dd>
            </>
          )}
        </dl>
      </div>

      {project.bible.relationships.length > 0 && (
        <div className="card">
          <div className="card-head">
            <h2>Relationships</h2>
          </div>
          <div className="col" style={{ gap: 8 }}>
            {project.bible.relationships.map((r, i) => (
              <div key={i} className="row" style={{ alignItems: "flex-start" }}>
                <span className="pill" style={{ minWidth: 0 }}>
                  {r.from}
                </span>
                <span
                  className="pill"
                  style={{
                    color:
                      r.sentiment > 25
                        ? "var(--good)"
                        : r.sentiment < -25
                        ? "var(--bad)"
                        : undefined,
                  }}
                >
                  {r.kind}
                </span>
                <span className="pill" style={{ minWidth: 0 }}>
                  {r.to}
                </span>
                <span className="small muted" style={{ flex: 1, minWidth: 0 }}>
                  {r.description}
                </span>
              </div>
            ))}
          </div>
        </div>
      )}

      {project.bible.foreshadowing.length > 0 && (
        <div className="card">
          <div className="card-head">
            <h2>Foreshadowing</h2>
            <span className="faint small">
              Setups and whether they have paid off yet
            </span>
          </div>
          <div className="col" style={{ gap: 10 }}>
            {project.bible.foreshadowing.map((f, i) => (
              <div key={i}>
                <div className="row">
                  <span className="pill amber">p{f.setup_page + 1}</span>
                  <strong className="small">{f.setup}</strong>
                </div>
                <div className="small muted" style={{ paddingLeft: 8 }}>
                  {f.payoff ? (
                    <>
                      Pays off on page {(f.payoff_page ?? 0) + 1}: {f.payoff}
                    </>
                  ) : (
                    <em>Not paid off in this source yet.</em>
                  )}
                </div>
              </div>
            ))}
          </div>
        </div>
      )}
    </>
  );
}

function Cast({ onOpen }: { onOpen: (c: Character) => void }) {
  const project = useStore((s) => s.project)!;
  const characters = project.bible.characters;
  if (characters.length === 0) {
    return <Empty icon="user" title="No characters identified" />;
  }
  return (
    <div className="grid three">
      {characters.map((c) => (
        <button key={c.id} className="char-card" onClick={() => onOpen(c)}>
          <div className="char-portrait">
            {c.portrait_path ? (
              <img src={fileUrl(c.portrait_path)} alt={c.name} loading="lazy" />
            ) : (
              <Icon name="user" size={26} />
            )}
          </div>
          <div className="char-body">
            <div className="row">
              <h3 style={{ flex: 1, minWidth: 0 }} className="truncate">
                {c.name}
              </h3>
              <span className="pill">{c.role}</span>
            </div>
            {c.aliases.length > 0 && (
              <div className="small faint truncate">
                also: {c.aliases.join(", ")}
              </div>
            )}
            <div className="muted">{c.personality || c.appearance}</div>
            <div className="spacer" />
            <div className="bar" title={`Prominence ${c.prominence}`}>
              <span style={{ width: `${c.prominence}%` }} />
            </div>
            <div className="small faint">
              {c.appearance_count} page{c.appearance_count === 1 ? "" : "s"}
            </div>
          </div>
        </button>
      ))}
    </div>
  );
}

function CharacterModal({
  character,
  onClose,
}: {
  character: Character;
  onClose: () => void;
}) {
  return (
    <Modal title={character.name} onClose={onClose} wide>
      <div className="row" style={{ alignItems: "flex-start", gap: 16 }}>
        {character.portrait_path && (
          <img
            src={fileUrl(character.portrait_path)}
            alt=""
            style={{
              width: 190,
              borderRadius: 10,
              border: "1px solid var(--border)",
            }}
          />
        )}
        <dl className="kv" style={{ flex: 1, minWidth: 0 }}>
          <dt>Role</dt>
          <dd>{character.role}</dd>
          {character.aliases.length > 0 && (
            <>
              <dt>Also called</dt>
              <dd>{character.aliases.join(", ")}</dd>
            </>
          )}
          {character.appearance && (
            <>
              <dt>Appearance</dt>
              <dd>{character.appearance}</dd>
            </>
          )}
          {character.personality && (
            <>
              <dt>Personality</dt>
              <dd>{character.personality}</dd>
            </>
          )}
          {character.abilities.length > 0 && (
            <>
              <dt>Abilities</dt>
              <dd>{character.abilities.join(", ")}</dd>
            </>
          )}
          {character.affiliations.length > 0 && (
            <>
              <dt>Affiliations</dt>
              <dd>{character.affiliations.join(", ")}</dd>
            </>
          )}
          {character.goals && (
            <>
              <dt>Wants</dt>
              <dd>{character.goals}</dd>
            </>
          )}
          {character.arc && (
            <>
              <dt>Arc</dt>
              <dd>{character.arc}</dd>
            </>
          )}
          <dt>First seen</dt>
          <dd>Page {character.first_seen_page + 1}</dd>
          <dt>Appears on</dt>
          <dd className="mono small">
            {character.page_refs.map((p) => p + 1).join(", ") || "-"}
          </dd>
        </dl>
      </div>
    </Modal>
  );
}

function Timeline() {
  const project = useStore((s) => s.project)!;
  const [chrono, setChrono] = useState(false);

  const events = useMemo(() => {
    const list = [...project.bible.events];
    list.sort((a, b) =>
      chrono
        ? a.chronological_order - b.chronological_order
        : a.presentation_order - b.presentation_order
    );
    return list;
  }, [project.bible.events, chrono]);

  const byId = useMemo(
    () => new Map(project.bible.events.map((e) => [e.id, e])),
    [project.bible.events]
  );

  if (events.length === 0) return <Empty icon="clock" title="No events recorded" />;

  return (
    <div className="card">
      <div className="card-head">
        <h2>Event graph</h2>
        <div className="spacer" />
        <button
          className={`btn sm${chrono ? "" : " primary"}`}
          onClick={() => setChrono(false)}
        >
          As drawn
        </button>
        <button
          className={`btn sm${chrono ? " primary" : ""}`}
          onClick={() => setChrono(true)}
        >
          Chronological
        </button>
      </div>
      <p className="card-sub">
        {chrono
          ? "Ordered by when things actually happened in story time, flashbacks slotted into place."
          : "Ordered by how the source presents them, page by page."}
      </p>

      <div className="timeline">
        {events.map((e) => (
          <div
            key={e.id}
            className={`tl-item${e.is_flashback ? " flashback" : ""}`}
          >
            <span className="tl-dot" />
            <div className="tl-head">
              <strong>{e.summary}</strong>
              <span className="pill">{e.kind}</span>
              {e.is_flashback && <span className="pill amber">flashback</span>}
              <span className="pill">
                p{e.page_start + 1}
                {e.page_end !== e.page_start ? `-${e.page_end + 1}` : ""}
              </span>
              <span
                className="pill"
                style={{
                  color:
                    e.importance >= 75
                      ? "var(--accent-2)"
                      : e.importance >= 45
                      ? "var(--text-dim)"
                      : "var(--text-faint)",
                }}
              >
                {e.importance}
              </span>
            </div>
            <div className="tl-body">
              {e.actors.length > 0 && (
                <span className="faint">{e.actors.join(", ")} &middot; </span>
              )}
              {e.stakes && <span>{e.stakes}</span>}
              {e.caused_by.length > 0 && (
                <div className="small faint mt-sm">
                  <Icon name="link" size={11} /> follows from:{" "}
                  {e.caused_by
                    .map((id) => byId.get(id)?.summary ?? "an earlier event")
                    .join("; ")}
                </div>
              )}
            </div>
          </div>
        ))}
      </div>
    </div>
  );
}

function Threads() {
  const project = useStore((s) => s.project)!;
  const threads = project.bible.threads;
  if (threads.length === 0)
    return <Empty icon="link" title="No open threads recorded" />;
  return (
    <div className="card">
      <div className="card-head">
        <h2>Open questions</h2>
        <span className="faint small">
          What the source has raised and not yet answered
        </span>
      </div>
      <div className="col">
        {threads.map((t) => (
          <div key={t.id} className="row" style={{ alignItems: "flex-start" }}>
            <span
              className={`pill ${
                t.status === "resolved" ? "good" : t.status === "open" ? "warn" : ""
              }`}
            >
              {t.status}
            </span>
            <div style={{ flex: 1, minWidth: 0 }}>
              <strong className="small">{t.question}</strong>
              <div className="small muted">
                Raised on page {t.opened_page + 1}
                {t.resolved_page != null &&
                  `, answered on page ${t.resolved_page + 1}`}
                {t.notes && ` · ${t.notes}`}
              </div>
            </div>
          </div>
        ))}
      </div>
    </div>
  );
}

function Chapters() {
  const project = useStore((s) => s.project)!;
  const setScope = useStore((s) => s.setScope);
  const setView = useStore((s) => s.setView);
  const chapters = project.bible.chapters;
  if (chapters.length === 0) return <Empty icon="script" title="No chapters summarized" />;

  return (
    <>
      {chapters.map((c) => (
        <div key={c.chapter_index} className="card">
          <div className="card-head">
            <h2>{c.title ?? `Chapter ${c.chapter_index + 1}`}</h2>
            {c.tone && <span className="pill">{c.tone}</span>}
            <div className="spacer" />
            <span className="faint small">
              pages {c.page_start + 1}-{c.page_end + 1}
            </span>
            <button
              className="btn sm"
              onClick={() => {
                setScope({ chapter: c.chapter_index });
                setView("script");
              }}
            >
              Write this chapter
              <Icon name="chevron" size={13} />
            </button>
          </div>
          <p className="muted">{c.synopsis}</p>

          {c.beats.length > 0 && (
            <>
              <h3 className="mt">Beat sheet</h3>
              <ol className="col mt-sm" style={{ paddingLeft: 18, gap: 6 }}>
                {c.beats.map((b) => (
                  <li key={b.id}>
                    <div className="small">{b.text}</div>
                    {b.quote && (
                      <div className="small" style={{ color: "var(--accent-2)" }}>
                        &ldquo;{b.quote}&rdquo;
                      </div>
                    )}
                  </li>
                ))}
              </ol>
            </>
          )}

          {c.cliffhanger && (
            <div className="banner warn mt">
              <Icon name="alert" size={15} />
              <div>
                <strong>Ends on</strong>
                <div className="small">{c.cliffhanger}</div>
              </div>
            </div>
          )}
        </div>
      ))}
    </>
  );
}

function Pages({ onZoom }: { onZoom: (src: string) => void }) {
  const project = useStore((s) => s.project)!;
  const [selected, setSelected] = useState<number | null>(null);

  const reading = useMemo(
    () =>
      selected == null
        ? null
        : project.bible.readings.find((r) => r.page_index === selected) ?? null,
    [selected, project.bible.readings]
  );
  const page = useMemo(
    () =>
      selected == null
        ? null
        : project.source.pages.find((p) => p.index === selected) ?? null,
    [selected, project.source.pages]
  );

  return (
    <>
      <div className="card">
        <div className="card-head">
          <h2>Page record</h2>
          <span className="faint small">
            Click a page to see exactly what the engine read from it
          </span>
        </div>
        <div className="page-grid">
          {project.source.pages.map((p) => {
            const r = project.bible.readings.find((x) => x.page_index === p.index);
            return (
              <figure
                key={p.id}
                className="page-thumb"
                onClick={() => setSelected(p.index)}
              >
                <img src={fileUrl(p.thumb_path)} alt={p.label} loading="lazy" />
                <figcaption>
                  <span>{p.index + 1}</span>
                  <span>{r?.role === "chapter_start" ? "CH" : ""}</span>
                </figcaption>
              </figure>
            );
          })}
        </div>
      </div>

      {selected != null && page && (
        <Modal
          title={`Page ${selected + 1} - ${page.label}`}
          onClose={() => setSelected(null)}
          wide
        >
          <div className="row" style={{ alignItems: "flex-start", gap: 16 }}>
            <img
              src={fileUrl(page.thumb_path)}
              alt=""
              onClick={() => onZoom(fileUrl(page.path))}
              style={{
                width: 260,
                borderRadius: 8,
                border: "1px solid var(--border)",
                cursor: "zoom-in",
              }}
            />
            <div style={{ flex: 1, minWidth: 0 }}>
              {!reading ? (
                <p className="muted">This page has not been analyzed.</p>
              ) : (
                <>
                  <div className="row wrap mb">
                    <span className="pill accent">{reading.role}</span>
                    {reading.is_flashback && (
                      <span className="pill amber">flashback</span>
                    )}
                    {reading.location && (
                      <span className="pill">{reading.location}</span>
                    )}
                    <span className="pill">{page.panels.length} panels</span>
                  </div>

                  {reading.panels.map((p) => (
                    <div key={p.index} className="mb">
                      <div className="row">
                        <span className="pill">#{p.index + 1}</span>
                        {p.shot && <span className="pill">{p.shot}</span>}
                        {p.emotion && <span className="pill">{p.emotion}</span>}
                        <span className="faint small">{p.significance}</span>
                      </div>
                      <div className="small">{p.action || p.description}</div>
                      {p.dialogue.map((d, i) => (
                        <div key={i} className="small muted">
                          <strong>{d.speaker}:</strong> &ldquo;{d.text}&rdquo;
                        </div>
                      ))}
                      {p.sfx.length > 0 && (
                        <div className="small faint mono">{p.sfx.join("  ")}</div>
                      )}
                    </div>
                  ))}

                  {reading.notes && (
                    <div className="banner info">
                      <Icon name="sparkle" size={14} />
                      <div className="small">{reading.notes}</div>
                    </div>
                  )}
                </>
              )}
            </div>
          </div>
        </Modal>
      )}
    </>
  );
}
