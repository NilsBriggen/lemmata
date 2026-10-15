// Draws one page, or the left or right half of it, into a canvas that fits a box.
import type { PDFDocumentProxy } from "pdfjs-dist";
import { fitScale, regionSize, type Region } from "./layout";

let lib: Promise<typeof import("pdfjs-dist")> | undefined;

/**
 * pdf.js and its worker, fetched on first use so they stay out of the startup bundle.
 * The legacy build: the modern one needs the newest engines (Uint8Array.toHex), which
 * older WebView2 and macOS WebKit versions lack.
 */
export function loadPdfjs() {
  return (lib ??= Promise.all([
    import("pdfjs-dist/legacy/build/pdf.mjs") as Promise<typeof import("pdfjs-dist")>,
    import("pdfjs-dist/legacy/build/pdf.worker.min.mjs?url"),
  ]).then(([pdfjs, worker]) => {
    pdfjs.GlobalWorkerOptions.workerSrc = worker.default;
    return pdfjs;
  }));
}

/** Returns false when a newer request replaced this one, so the caller can ignore it. */
export async function renderRegion(
  doc: PDFDocumentProxy,
  pageNumber: number,
  canvas: HTMLCanvasElement,
  boxW: number,
  boxH: number,
  region: Region,
  current: () => boolean = () => true,
): Promise<boolean> {
  const page = await doc.getPage(pageNumber);
  const base = page.getViewport({ scale: 1 });
  const part = regionSize(base.width, base.height, region);
  const ratio = window.devicePixelRatio || 1;
  const scale = fitScale(part, boxW, boxH);
  const viewport = page.getViewport({ scale: scale * ratio });
  if (!current()) return false;
  const whole = document.createElement("canvas");
  whole.width = Math.max(1, Math.floor(viewport.width));
  whole.height = Math.max(1, Math.floor(viewport.height));
  await page.render({ canvas: whole, canvasContext: whole.getContext("2d")!, viewport }).promise;
  if (!current()) return false;
  const sx = Math.floor(part.x * scale * ratio);
  const sw = Math.max(1, Math.min(whole.width - sx, Math.floor(part.w * scale * ratio)));
  canvas.width = sw;
  canvas.height = whole.height;
  canvas.style.width = `${sw / ratio}px`;
  canvas.style.height = `${whole.height / ratio}px`;
  canvas.getContext("2d")!.drawImage(whole, sx, 0, sw, whole.height, 0, 0, sw, whole.height);
  return true;
}
