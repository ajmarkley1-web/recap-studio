// PDF rasterization runs in the webview with pdf.js.
//
// Doing it here rather than in Rust keeps the app free of a native PDF
// dependency, which is the usual source of "works on my machine" bugs in
// desktop builds. Pages are rendered one at a time and handed straight to the
// backend, so a 300-page volume never sits in memory all at once.

import * as pdfjs from "pdfjs-dist";
import workerUrl from "pdfjs-dist/build/pdf.worker.min.mjs?url";

pdfjs.GlobalWorkerOptions.workerSrc = workerUrl;

export interface RasterizeOptions {
  /** Target width in pixels for the long edge of each page. */
  targetWidth?: number;
  onProgress?: (current: number, total: number) => void;
  /** Called with the PNG bytes for each page, in order. */
  onPage: (ordinal: number, bytes: Uint8Array) => Promise<void>;
  signal?: AbortSignal;
}

export class PdfError extends Error {
  constructor(message: string, readonly hint?: string) {
    super(message);
    this.name = "PdfError";
  }
}

async function canvasToPng(canvas: HTMLCanvasElement): Promise<Uint8Array> {
  const blob = await new Promise<Blob | null>((resolve) =>
    canvas.toBlob((b) => resolve(b), "image/png")
  );
  if (!blob) throw new PdfError("The browser could not encode a rendered page.");
  return new Uint8Array(await blob.arrayBuffer());
}

export async function rasterize(
  file: File | ArrayBuffer,
  opts: RasterizeOptions
): Promise<number> {
  const data =
    file instanceof ArrayBuffer ? file : await file.arrayBuffer();

  let doc: pdfjs.PDFDocumentProxy;
  try {
    doc = await pdfjs.getDocument({ data: new Uint8Array(data) }).promise;
  } catch (err) {
    const message = String((err as Error)?.message ?? err);
    if (/password/i.test(message)) {
      throw new PdfError(
        "That PDF is password protected.",
        "Remove the password and try again, or export the pages as images."
      );
    }
    throw new PdfError(`That file could not be opened as a PDF. ${message}`);
  }

  const total = doc.numPages;
  if (total === 0) {
    throw new PdfError("That PDF has no pages.");
  }

  const targetWidth = opts.targetWidth ?? 1600;
  const canvas = document.createElement("canvas");
  const ctx = canvas.getContext("2d", { willReadFrequently: false });
  if (!ctx) throw new PdfError("This system could not create a drawing canvas.");

  try {
    for (let i = 1; i <= total; i++) {
      if (opts.signal?.aborted) return i - 1;

      const page = await doc.getPage(i);
      const base = page.getViewport({ scale: 1 });
      // Cap the scale so a poster-sized page cannot blow out memory.
      const scale = Math.min(Math.max(targetWidth / base.width, 0.5), 4);
      const viewport = page.getViewport({ scale });

      canvas.width = Math.floor(viewport.width);
      canvas.height = Math.floor(viewport.height);
      // Scanned pages sometimes have transparent backgrounds; force white.
      ctx.fillStyle = "#ffffff";
      ctx.fillRect(0, 0, canvas.width, canvas.height);

      await page.render({ canvasContext: ctx, viewport }).promise;
      const bytes = await canvasToPng(canvas);
      await opts.onPage(i - 1, bytes);
      page.cleanup();

      opts.onProgress?.(i, total);
    }
  } finally {
    await doc.destroy();
    canvas.width = 0;
    canvas.height = 0;
  }

  return total;
}
