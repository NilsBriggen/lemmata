// "Report a problem": a GitHub issue prefilled with what helps diagnose it,
// shown to the user first. File paths are removed (they often contain names).
import type { Diagnostic, TexStatus } from "./api";

export const ISSUES = "https://github.com/NilsBriggen/lemmata/issues/new";

/** Replaces absolute paths (Unix, Windows, ~) with <path>. */
export function scrubPaths(text: string): string {
  return text
    .replace(/(?:[A-Za-z]:\\|\\\\)[^\s'"`]+/g, "<path>")
    .replace(/(?:~|\/(?:home|Users|tmp|var|usr|opt|mnt|media|Volumes))\/[^\s'"`]+/g, "<path>");
}

export function systemSummary(version: string, tex: TexStatus | null): string {
  const d = tex?.detection.distribution;
  const dist = !d || d.name === "unknown" ? "unknown TeX" : d.name === "texlive" ? `TeX Live ${d.year ?? "?"}` : `MiKTeX ${d.version ?? "?"}`;
  const missing = tex?.detection.tools.filter((t) => !t.path).map((t) => t.name) ?? [];
  return [
    `Lemmata ${version}`,
    `System: ${tex?.os.name ?? navigator.platform}`,
    `TeX: ${dist}${missing.length ? ` (missing: ${missing.join(", ")})` : ""}`,
  ].join("\n");
}

export function errorSummary(diagnostics: Diagnostic[]): string {
  const errors = diagnostics.filter((d) => d.severity === "error").slice(0, 5);
  return scrubPaths(errors.map((d) => `${d.file ? d.file.replace(/^\.\//, "") + (d.line ? `:${d.line}` : "") + ": " : ""}${d.message}`).join("\n"));
}

export function issueUrl(system: string, log: string): string {
  const q = new URLSearchParams({ template: "bug.yml", system, log });
  return `${ISSUES}?${q}`;
}
