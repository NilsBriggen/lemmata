// Beamer's "show notes on second screen=right" makes every PDF page twice as
// wide: the slide on the left half and its \note text on the right half. These
// helpers tell such a PDF from a plain slide PDF and say which part to draw.

export type Region = "full" | "left" | "right";

/** Slides are 4:3 (1.33) or 16:9 (1.78); with notes beside them the page is about twice that. */
export function hasNotesPage(width: number, height: number): boolean {
  return height > 0 && width / height > 2.2;
}

/** Where the slide and the notes are on a page, as the part of the page width to draw. */
export function regions(width: number, height: number): { slide: Region; notes: Region | null } {
  return hasNotesPage(width, height) ? { slide: "left", notes: "right" } : { slide: "full", notes: null };
}

/** The size of a region of a page, in the page's own units. */
export function regionSize(width: number, height: number, region: Region): { w: number; h: number; x: number } {
  if (region === "full") return { w: width, h: height, x: 0 };
  return { w: width / 2, h: height, x: region === "right" ? width / 2 : 0 };
}

/** Scale that fits a region into a box without cutting it off. */
export function fitScale(region: { w: number; h: number }, boxW: number, boxH: number): number {
  if (region.w <= 0 || region.h <= 0) return 1;
  return Math.min(boxW / region.w, boxH / region.h);
}

/** 83 → "1:23", 3725 → "1:02:05" */
export function formatElapsed(seconds: number): string {
  const s = Math.max(0, Math.floor(seconds));
  const h = Math.floor(s / 3600);
  const m = Math.floor((s % 3600) / 60);
  const ss = String(s % 60).padStart(2, "0");
  return h ? `${h}:${String(m).padStart(2, "0")}:${ss}` : `${m}:${ss}`;
}

/** Keeps a slide number inside the document. */
export const clampPage = (page: number, total: number) => Math.min(Math.max(1, Math.round(page) || 1), Math.max(1, total));
