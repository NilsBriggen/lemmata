import { invoke } from "@tauri-apps/api/core";

/** Makes a PDF of the changes since snapshot `id` (latexdiff) and returns its path. Move into api.ts when convenient. */
export const historyLatexdiff = (id: string) => invoke<string>("history_latexdiff", { id });
