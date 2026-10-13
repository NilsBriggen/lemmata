// Typed wrappers around the Tauri commands in src-tauri/src/lib.rs.
import { invoke } from "@tauri-apps/api/core";

export type Severity = "error" | "warning" | "info";
export type DiagnosticKind =
  | { type: "missing-file"; name: string }
  | { type: "undefined-citation"; key: string }
  | { type: "undefined-reference"; key: string }
  | { type: "undefined-control-sequence" }
  | { type: "bad-box" }
  | { type: "other" };

export interface Diagnostic {
  severity: Severity;
  kind: DiagnosticKind;
  file: string | null;
  line: number | null;
  message: string;
  hint: string | null;
  fix?: { explanation: string; command: string | null };
}

export type InstallRequest =
  | { what: "tex"; platform: string; full: boolean }
  | { what: "package"; file: string };

export interface Outcome {
  status: "success" | "failed" | "cancelled";
  duration_ms: number;
  pdf: string | null;
  diagnostics: Diagnostic[];
}

export interface Tool { name: string; path: string | null; required: boolean }
export type Distribution =
  | { name: "texlive"; year: number | null }
  | { name: "miktex"; version: string | null }
  | { name: "unknown" };

export interface Detection {
  bin_dir: string | null;
  distribution: Distribution;
  tools: Tool[];
  ready: boolean;
  outdated: boolean;
}

export interface Instructions {
  label: string;
  recommended: string;
  full: string;
  recommended_size: string;
  full_size: string;
  link: string | null;
  note: string | null;
}

export type Family = "arch" | "debian" | "fedora" | "suse" | "macos" | "windows" | "other";

export interface TexStatus {
  detection: Detection;
  os: { family: Family; name: string };
  instructions: Record<string, Instructions>;
}

export type Engine = "pdflatex" | "xelatex" | "lualatex";

export interface Config {
  main: string | null;
  engine: Engine;
  bib: "biber" | "bibtex";
  shell_escape: boolean;
  outdir: string;
}

export interface Entry { name: string; path: string; is_dir: boolean; children: Entry[] }

export interface ProjectInfo {
  root: string;
  name: string;
  main: string;
  main_candidates: string[];
  config: Config;
  tree: Entry[];
}

export interface Heading { level: number; title: string; file: string; line: number }

export interface PdfBox { page: number; x: number; y: number; h: number; v: number; width: number; height: number }
export interface SourceHit { path: string; line: number; inside_project: boolean }

export const api = {
  texStatus: (extraDir?: string) => invoke<TexStatus>("tex_status", { extraDir }),
  openProject: (path: string) => invoke<ProjectInfo>("open_project", { path }),
  projectInfo: () => invoke<ProjectInfo>("project_info_cmd"),
  setConfig: (config: Config) => invoke<ProjectInfo>("set_config", { config }),
  readText: (path: string) => invoke<string>("read_text", { path }),
  writeText: (path: string, content: string) => invoke<void>("write_text", { path, content }),
  build: () => invoke<Outcome>("build"),
  readPdf: async () => new Uint8Array(await invoke<ArrayBuffer>("read_pdf")),
  synctexForward: (path: string, line: number, column: number) =>
    invoke<PdfBox[]>("synctex_forward", { path, line, column }),
  synctexBackward: (page: number, x: number, y: number) =>
    invoke<SourceHit | null>("synctex_backward", { page, x, y }),
  outline: () => invoke<Heading[]>("outline"),
  openInstallTerminal: (request: InstallRequest) => invoke<void>("open_install_terminal", { request }),
};

export function errorMessage(e: unknown): string {
  return typeof e === "string" ? e : e instanceof Error ? e.message : String(e);
}
