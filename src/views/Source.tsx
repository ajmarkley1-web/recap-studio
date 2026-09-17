import { useCallback, useEffect, useRef, useState } from "react";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { readFile } from "@tauri-apps/plugin-fs";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { api, fileUrl } from "../lib/api";
import { PdfError, rasterize } from "../lib/pdf";
import { useStore } from "../lib/store";
import { FORMAT_LABELS, type SourceFormat } from "../lib/types";
import { Icon } from "../components/Icon";
import { Banner, Empty, Lightbox, Progress } from "../components/ui";

const IMAGE_EXTS = ["png", "jpg", "jpeg", "webp", "bmp", "gif", "tif", "tiff", "avif"];

function extOf(path: string) {
  return path.split(".").pop()?.toLowerCase() ?? "";
}

export function SourceView() {
  const project = useStore((s) => s.project);
  const setProject = useStore((s) => s.setProject);
  const setView = useStore((s) => s.setView);
  const toast = useStore((s) => s.toast);
  const notifyError = useStore((s) => s.notifyError);
  const job = useStore((s) => s.job);

  const [over, setOver] = useState(false);
  const [pdfProgress, setPdfProgress] = useState<{ current: number; total: number } | null>(null);
  const [zoom, setZoom] = useState<string | null>(null);
  const [chapterLabel, setChapterLabel] = useState("Chapter 1");
  const busyRef = useRef(false);

  const handlePaths = useCallback(
    async (paths: string[]) => {
      if (!project || busyRef.current || paths.length === 0) return;
      busyRef.current = true;
      try {
        const pdfs = paths.filter((p) => extOf(p) === "pdf");
        const rest = paths.filter((p) => extOf(p) !== "pdf");

        if (pdfs.length > 0) {
          if (pdfs.length > 1) {
            toast({
              tone: "info",
              title: "One PDF at a time",
              body: `Importing ${pdfs[0].split(/[\\/]/).pop()}. Make another project for the rest.`,
            });
          }
          await importPdf(pdfs[0]);
        } else if (rest.length > 0) {
          const updated = await api.ingestPaths(project.meta.root, rest);
          setProject(updated);
          toast({
            tone: "success",
            title: `Imported ${updated.source.pages.length} pages`,
            body: updated.source.chapter_strategy,
          });
        }
      } catch (err) {
        if (err instanceof PdfError) {
          notifyError({ message: err.message, kind: "pdf", hint: err.hint ?? null }, "PDF problem");
        } else {
          notifyError(err, "Import failed");
        }
      } finally {
        busyRef.current = false;
        setPdfProgress(null);
      }
    },
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [project, chapterLabel]
  );

  async function importPdf(path: string) {
    if (!project) return;
    setPdfProgress({ current: 0, total: 0 });
    const bytes = await readFile(path);
    // readFile gives a Uint8Array view; pdf.js wants a standalone buffer.
    const buffer = bytes.buffer.slice(
      bytes.byteOffset,
      bytes.byteOffset + bytes.byteLength
    ) as ArrayBuffer;

    await rasterize(buffer, {
      targetWidth: 1600,
      onProgress: (current, total) => setPdfProgress({ current, total }),
      onPage: async (ordinal, png) => {
        await api.stagePdfPage(project.meta.root, ordinal, Array.from(png));
      },
    });

    const label = chapterLabel.trim() || path.split(/[\\/]/).pop() || "Chapter 1";
    const updated = await api.ingestPdf(project.meta.root, label);
    setProject(updated);
    toast({
      tone: "success",
      title: `Imported ${updated.source.pages.length} pages`,
      body: "Chapter breaks inside the PDF are detected during Analyze.",
    });
  }

  // Tauri intercepts native file drops, so we listen for its event rather than
  // the HTML drop event. This also gives us real paths instead of File objects.
  useEffect(() => {
    let unlisten: (() => void) | undefined;
    void getCurrentWebview()
      .onDragDropEvent((event) => {
        if (event.payload.type === "over") setOver(true);
        else if (event.payload.type === "leave") setOver(false);
        else if (event.payload.type === "drop") {
          setOver(false);
          void handlePaths(event.payload.paths);
        }
      })
      .then((fn) => {
        unlisten = fn;
      });
    return () => unlisten?.();
  }, [handlePaths]);

  async function pickPdf() {
    const picked = await openDialog({
      multiple: false,
      filters: [{ name: "PDF", extensions: ["pdf"] }],
    });
    if (typeof picked === "string") await handlePaths([picked]);
  }

  async function pickFolder() {
    const picked = await openDialog({ directory: true, multiple: false });
    if (typeof picked === "string") await handlePaths([picked]);
  }

  async function pickImages() {
    const picked = await openDialog({
      multiple: true,
      filters: [{ name: "Images", extensions: IMAGE_EXTS }],
    });
    if (Array.isArray(picked) && picked.length > 0) await handlePaths(picked);
  }

  async function changeFormat(format: SourceFormat) {
    if (!project) return;
    try {
      setProject(await api.updateProject(project.meta.root, { format }));
      toast({
        tone: "info",
        title: "Reading order updated",
        body:
          project.source.pages.length > 0
            ? "Re-import the source to re-cut panels with the new order."
            : undefined,
      });
    } catch (err) {
      notifyError(err, "Could not change the format");
    }
  }

  if (!project) {
    return <Empty icon="folder" title="Open a project first" />;
  }

  const hasPages = project.source.pages.length > 0;
  const importing = job.running && job.job === "ingest";
  const rasterizing = pdfProgress !== null;

  return (
    <div className="page wide">
      <div className="page-head">
        <h1>Source material</h1>
        <p>
          Drop in a PDF, a folder of chapter folders, or individual panel images.
          Pages are copied into the project so the originals are never touched.
        </p>
      </div>

      {(rasterizing || importing) && (
        <div className="card mb">
          <div className="row mb">
            <Icon name="refresh" size={15} className="spin" />
            <strong style={{ flex: 1 }}>
              {rasterizing && pdfProgress.total > 0
                ? `Rendering PDF page ${pdfProgress.current} of ${pdfProgress.total}`
                : rasterizing
                ? "Opening the PDF"
                : job.message || "Importing"}
            </strong>
            <span className="faint small">
              {importing && job.total > 0 ? `${job.current}/${job.total}` : ""}
            </span>
          </div>
          <Progress
            current={rasterizing ? pdfProgress.current : job.current}
            total={rasterizing ? pdfProgress.total : job.total}
          />
        </div>
      )}

      {!hasPages && (
        <>
          <div
            className={`dropzone${over ? " over" : ""}`}
            onClick={() => void pickPdf()}
          >
            <div className="empty-icon" style={{ margin: "0 auto 12px" }}>
              <Icon name="source" size={24} />
            </div>
            <h3>Drop your source here</h3>
            <p>
              A PDF volume, a folder containing one folder per chapter, or a pile
              of panel images
            </p>
          </div>

          <div className="row mt" style={{ justifyContent: "center" }}>
            <button className="btn" onClick={() => void pickPdf()} disabled={rasterizing}>
              <Icon name="file" size={15} />
              Choose a PDF
            </button>
            <button className="btn" onClick={() => void pickFolder()} disabled={rasterizing}>
              <Icon name="folder" size={15} />
              Choose a folder
            </button>
            <button className="btn" onClick={() => void pickImages()} disabled={rasterizing}>
              <Icon name="storyboard" size={15} />
              Choose images
            </button>
          </div>

          <div className="card mt">
            <h2>How chapters get detected</h2>
            <dl className="kv mt-sm">
              <dt>Folder of folders</dt>
              <dd>
                Each subfolder becomes a chapter, ordered by any number in its
                name. <span className="mono faint">Chapter 12</span>,{" "}
                <span className="mono faint">ch_007</span> and{" "}
                <span className="mono faint">Ep 5.5</span> all work.
              </dd>
              <dt>One flat folder</dt>
              <dd>Treated as a single chapter, pages sorted naturally.</dd>
              <dt>A PDF</dt>
              <dd>
                Imported as one chapter, then split automatically during Analyze
                wherever a printed chapter page is found in the art.
              </dd>
            </dl>
          </div>
        </>
      )}

      {hasPages && (
        <>
          <div className="card">
            <div className="card-head">
              <h2>Imported</h2>
              <span className="pill accent">
                {project.source.pages.length} pages
              </span>
              <span className="pill">{project.source.total_panels} panels</span>
              <div className="spacer" />
              <button className="btn sm" onClick={() => void pickFolder()}>
                <Icon name="refresh" size={14} />
                Replace source
              </button>
              <button className="btn primary sm" onClick={() => setView("analyze")}>
                Continue to Analyze
                <Icon name="chevron" size={14} />
              </button>
            </div>
            <p className="small muted">{project.source.chapter_strategy}</p>

            <div className="row wrap mt-sm">
              {project.source.chapters.map((c) => (
                <span key={c.index} className="pill">
                  {c.title ?? `Chapter ${c.index + 1}`}
                  <span className="faint">
                    &nbsp;p{c.first_page + 1}-{c.last_page + 1}
                  </span>
                </span>
              ))}
            </div>
          </div>

          <div className="card">
            <div className="card-head">
              <h2>Reading order</h2>
            </div>
            <div className="row wrap">
              {(Object.keys(FORMAT_LABELS) as SourceFormat[]).map((f) => (
                <button
                  key={f}
                  className={`btn${project.meta.format === f ? " primary" : ""}`}
                  onClick={() => void changeFormat(f)}
                >
                  {FORMAT_LABELS[f].split(" (")[0]}
                </button>
              ))}
            </div>
            <p className="field-hint">
              {FORMAT_LABELS[project.meta.format]}. Very tall images are always
              treated as vertical strips regardless of this setting.
            </p>
          </div>

          <div className="card">
            <div className="card-head">
              <h2>Pages</h2>
              <span className="faint small">Click a page to enlarge it</span>
            </div>
            <div className="page-grid">
              {project.source.pages.map((page) => (
                <figure
                  key={page.id}
                  className="page-thumb"
                  onClick={() => setZoom(fileUrl(page.path))}
                >
                  <img src={fileUrl(page.thumb_path)} alt={page.label} loading="lazy" />
                  <figcaption>
                    <span>{page.index + 1}</span>
                    <span>
                      {page.panels.length > 0 ? `${page.panels.length}p` : ""}
                    </span>
                  </figcaption>
                </figure>
              ))}
            </div>
          </div>
        </>
      )}

      {!hasPages && (
        <div className="mt">
          <Banner title="Naming the first chapter">
            PDFs come in as one unit. Give it a label so the story bible reads
            well later.
            <input
              className="mt-sm"
              type="text"
              value={chapterLabel}
              onChange={(e) => setChapterLabel(e.target.value)}
              placeholder="Chapter 1"
              style={{ maxWidth: 280 }}
            />
          </Banner>
        </div>
      )}

      <Lightbox src={zoom} onClose={() => setZoom(null)} />
    </div>
  );
}
