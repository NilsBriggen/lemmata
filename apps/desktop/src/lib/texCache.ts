// The last TeX detection that found a working installation. Detection runs
// pdflatex to identify it, which takes seconds on some Windows setups, so the
// app starts from this and checks again in the background.
import type { TexStatus } from "./api";

const KEY = "lemmata.tex";

export function saveTexCache(status: TexStatus, storage: Pick<Storage, "setItem"> = localStorage) {
  if (!status.detection.ready) return;
  try {
    storage.setItem(KEY, JSON.stringify(status));
  } catch {
    // Storage full or unavailable: the next start just waits for detection.
  }
}

/** A cached result, or null when there is none, it is damaged, or it did not find TeX. */
export function loadTexCache(storage: Pick<Storage, "getItem"> = localStorage): TexStatus | null {
  try {
    const status = JSON.parse(storage.getItem(KEY) ?? "null");
    return status?.detection?.ready === true && Array.isArray(status.detection.tools) ? (status as TexStatus) : null;
  } catch {
    return null;
  }
}
