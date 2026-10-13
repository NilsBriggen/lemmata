// Fake backend for the browser preview (`pnpm dev:mock`). Answers every
// command the app sends with plausible data, so the real UI code can be
// viewed and screenshotted without Tauri. Never part of a release build.
//
// Scenarios via the URL: ?mock=first-run | no-tex | errors | empty
import mainTex from "../../../../tests/fixtures/article/main.tex?raw";
import methodTex from "../../../../tests/fixtures/article/chapters/method.tex?raw";
import refsBib from "../../../../tests/fixtures/article/refs.bib?raw";
import samplePdf from "./sample.pdf?url";
import { mockEmit } from "./tauri-event";

const scenario = new URLSearchParams(location.search).get("mock") ?? "";
if (scenario === "first-run" || scenario === "no-tex") {
  // Behave like a fresh install.
  for (const k of Object.keys(localStorage)) if (k.startsWith("mock-store:") || k === "lemmata.recent") localStorage.removeItem(k);
}
if (scenario === "" && !localStorage.getItem("lemmata.recent")) {
  localStorage.setItem(
    "lemmata.recent",
    JSON.stringify([
      { path: "/home/you/Documents/papers/rl-thesis", lastOpened: Date.now() - 3600e3 },
      { path: "/home/you/Documents/uni/CA_Assignement_4", lastOpened: Date.now() - 86400e3 },
      { path: "/home/you/Documents/uni/NET_assignment_4", lastOpened: Date.now() - 3 * 86400e3 },
      { path: "/home/you/Documents/cv", lastOpened: Date.now() - 20 * 86400e3 },
    ]),
  );
  localStorage.setItem("mock-store:settings.json", JSON.stringify({ settings: { firstRunDone: true } }));
}

const files: Record<string, string> = {
  "main.tex": mainTex,
  "chapters/method.tex": methodTex,
  "refs.bib": refsBib,
  "lemmata.toml": 'main = "main.tex"\nengine = "pdflatex"\n',
};

const tools = (found: boolean) =>
  ["latexmk", "pdflatex", "synctex", "biber", "bibtex", "xelatex", "lualatex"].map((name, i) => ({
    name,
    path: found ? `/usr/bin/${name}` : null,
    required: i < 2,
  }));

const instructions = {
  arch: {
    label: "Arch / CachyOS / Manjaro",
    recommended: "sudo pacman -S --needed texlive-basic texlive-latex texlive-latexrecommended texlive-latexextra texlive-fontsrecommended texlive-bibtexextra texlive-binextra texlive-mathscience biber",
    full: "sudo pacman -S --needed texlive texlive-lang biber",
    recommended_size: "≈1.2 GB",
    full_size: "≈5.6 GB",
    link: null,
    note: null,
  },
  debian: { label: "Debian / Ubuntu / Mint", recommended: "sudo apt install texlive-latex-extra latexmk biber", full: "sudo apt install texlive-full", recommended_size: "≈1.1 GB", full_size: "≈7 GB", link: null, note: null },
  macos: { label: "macOS", recommended: "brew install --cask mactex-no-gui", full: "brew install --cask mactex", recommended_size: "≈5 GB", full_size: "≈6 GB", link: "https://www.tug.org/mactex/", note: null },
  windows: { label: "Windows", recommended: "winget install MiKTeX.MiKTeX", full: "winget install MiKTeX.MiKTeX", recommended_size: "≈200 MB", full_size: "≈200 MB", link: "https://tug.org/texlive/windows.html", note: "MiKTeX installs missing packages the first time a document needs them." },
  other: { label: "Other Linux", recommended: "Install TeX Live from https://tug.org/texlive/quickinstall.html", full: "Install TeX Live from https://tug.org/texlive/quickinstall.html", recommended_size: "", full_size: "", link: "https://tug.org/texlive/quickinstall.html", note: null },
};

let openRoot = "/home/you/Documents/papers/rl-thesis";

const tree = () => [
  { name: "chapters", path: "chapters", is_dir: true, children: [{ name: "method.tex", path: "chapters/method.tex", is_dir: false, children: [] }] },
  { name: "figures", path: "figures", is_dir: true, children: [{ name: "reward-curve.pdf", path: "figures/reward-curve.pdf", is_dir: false, children: [] }] },
  { name: "lemmata.toml", path: "lemmata.toml", is_dir: false, children: [] },
  { name: "main.tex", path: "main.tex", is_dir: false, children: [] },
  { name: "refs.bib", path: "refs.bib", is_dir: false, children: [] },
];

let config = { main: "main.tex", engine: "pdflatex", bib: "biber", shell_escape: false, outdir: ".lemmata/build", spellcheck: null, words: [], figures_dir: "figures" };

const errorDiagnostics = [
  { severity: "error", kind: { type: "undefined-control-sequence" }, file: "./chapters/method.tex", line: 4, message: "Undefined control sequence.", hint: "A command isn't defined. Check its spelling, or load the package that provides it." },
  { severity: "error", kind: { type: "missing-file", name: "pgfplots.sty" }, file: "./main.tex", line: 5, message: "File `pgfplots.sty' not found.", hint: null, fix: { explanation: "pgfplots.sty is part of the TeX Live package \"pgfplots\", which your system ships in texlive-pictures.", command: "sudo pacman -S --needed texlive-pictures" } },
  { severity: "warning", kind: { type: "undefined-citation", key: "nobody2020" }, file: "./chapters/method.tex", line: 7, message: "Citation 'nobody2020' on page 1 undefined.", hint: "No entry with this key in your .bib files, or the bibliography hasn't been built yet." },
  { severity: "info", kind: { type: "bad-box" }, file: "./main.tex", line: 12, message: "Overfull \\hbox (12.3pt too wide)", hint: null },
];

const okDiagnostics = [
  { severity: "warning", kind: { type: "undefined-reference", key: "sec:missing" }, file: "./chapters/method.tex", line: 7, message: "Reference `sec:missing' on page 1 undefined.", hint: "No \\label with this name. A label added just now resolves on the next build." },
];

const now = Math.floor(Date.now() / 1000);
const snapshots = [
  { id: "c3", time: now - 600, label: "Automatic snapshot", changes: [{ status: "modified", path: "chapters/method.tex" }] },
  { id: "c2", time: now - 86400, label: "Sent to supervisor", changes: [{ status: "modified", path: "main.tex" }, { status: "added", path: "refs.bib" }] },
  { id: "c1", time: now - 3 * 86400, label: "History turned on", changes: [{ status: "added", path: "main.tex" }, { status: "added", path: "chapters/method.tex" }] },
];

const wait = (ms: number) => new Promise((r) => setTimeout(r, ms));

type Args = Record<string, any>;
const handlers: Record<string, (a: Args) => unknown> = {
  tex_status: () => ({
    detection: {
      bin_dir: scenario === "no-tex" ? null : "/usr/bin",
      distribution: scenario === "no-tex" ? { name: "unknown" } : { name: "texlive", year: 2026 },
      tools: tools(scenario !== "no-tex"),
      ready: scenario !== "no-tex",
      outdated: false,
    },
    os: { family: "arch", name: "CachyOS Linux" },
    instructions,
  }),
  open_project: (a) => {
    openRoot = a.path;
    return projectInfo();
  },
  project_info_cmd: () => projectInfo(),
  set_config: (a) => {
    config = { ...config, ...a.config };
    return projectInfo();
  },
  read_text: (a) => files[a.path] ?? "",
  write_text: (a) => {
    files[a.path] = a.content;
  },
  build: async () => {
    await wait(700);
    return { status: scenario === "errors" ? "failed" : "success", duration_ms: 812, pdf: "/x/main.pdf", diagnostics: scenario === "errors" ? errorDiagnostics : okDiagnostics };
  },
  read_pdf: async () => (await fetch(samplePdf)).arrayBuffer(),
  read_log: () => "This is pdfTeX, Version 3.141592653-2.6-1.40.29 (TeX Live 2026/Arch Linux)\n entering extended mode\n(./main.tex\nLaTeX2e <2025-06-01>\n...\nOutput written on main.pdf (2 pages, 155357 bytes).",
  synctex_forward: () => [{ page: 1, x: 100, y: 300, h: 133, v: 295, width: 343, height: 9 }],
  synctex_backward: () => ({ path: "chapters/method.tex", line: 3, inside_project: true }),
  outline: () => ({
    headings: [
      { level: 2, title: "Introduction", file: "main.tex", line: 11, words: 142 },
      { level: 2, title: "Method", file: "chapters/method.tex", line: 1, words: 486 },
      { level: 3, title: "Policy gradient", file: "chapters/method.tex", line: 3, words: 231 },
      { level: 2, title: "Results", file: "chapters/method.tex", line: 9, words: 318 },
    ],
    total_words: 1177,
  }),
  labels: () => [
    { label: "sec:intro", file: "main.tex", line: 11, context: "\\section{Introduction}" },
    { label: "eq:policy", file: "chapters/method.tex", line: 5, context: "J(\\theta) = \\mathbb{E}[R]" },
    { label: "fig:curves", file: "chapters/method.tex", line: 14, context: "\\caption{Learning curves}" },
  ],
  word_count: () => ({
    total_words: 1177,
    files: [{ file: "main.tex", words: 142 }, { file: "chapters/method.tex", words: 1035 }],
    headings: 4, inline_math: 18, display_math: 3, floats: 2, citations: 7,
  }),
  package_names: () => ["amsmath", "amssymb", "amsthm", "biblatex", "booktabs", "cleveref", "graphicx", "hyperref", "listings", "siunitx", "tikz", "xcolor"],
  lsp_start: () => {
    throw "texlab isn't available in the preview.";
  },
  lsp_send: () => {},
  list_templates: () => [
    { id: "article", name: "Article", description: "A paper with sections, maths, figures and a biblatex bibliography." },
    { id: "thesis", name: "Thesis", description: "A report with a title page, chapters in separate files and a bibliography." },
    { id: "slides", name: "Slides", description: "A Beamer presentation with the clean metropolis theme." },
    { id: "cv", name: "CV", description: "A clean one-page curriculum vitae." },
    { id: "letter", name: "Letter", description: "A formal letter with the KOMA-Script scrlttr2 class." },
  ],
  create_project: (a) => `${a.parent}/${a.name}`,
  create_example: () => "/home/you/.local/share/dev.briggen.lemmata/example",
  inspect_zip: () => ({ bulk: true, projects: ["CA_Assignement_5", "INF_SEC_Assignement_3", "NET_assignment_4", "Homework", "CA_Assignement_4", "DB_Assignement_3"] }),
  import_overleaf_zip: (a) => ["CA_Assignement_5", "INF_SEC_Assignement_3", "NET_assignment_4", "Homework", "CA_Assignement_4", "DB_Assignement_3"].map((n) => `${a.parent}/${n}`),
  load_thumbnail: async () => {
    throw "No thumbnail yet.";
  },
  save_thumbnail: () => {},
  history_status: () => ({ state: "on" }),
  history_list: () => snapshots,
  history_file: (a) => (a.id === "c1" ? methodTex.replace("maximises", "maximizes") : files[a.path] ?? ""),
  history_snapshot: () => "c4",
  history_enable: () => {},
  history_restore: () => {},
  spell_set_language: () => {},
  spell_check: (a) => (a.words as string[]).filter((w) => ["recieve", "teh", "Overleaf"].includes(w)),
  spell_suggest: () => ["receive", "relieve"],
  spell_add: () => {},
  bib_entries: () => ({
    files: ["refs.bib"],
    entries: [
      { key: "schulman2017ppo", kind: "article", title: "Proximal Policy Optimization Algorithms", authors: "Schulman et al.", year: "2017", file: "refs.bib" },
      { key: "mnih2015human", kind: "article", title: "Human-level control through deep reinforcement learning", authors: "Mnih et al.", year: "2015", file: "refs.bib" },
      { key: "sutton2018reinforcement", kind: "book", title: "Reinforcement Learning: An Introduction", authors: "Sutton and Barto", year: "2018", file: "refs.bib" },
    ],
  }),
  bib_add_reference: () => ({ key: "vaswani2017attention", file: "refs.bib" }),
  zotero_status: () => "unavailable",
  search_project: () => [],
  update_check: () => null,
  has_package: () => true,
  find_references: () => [],
};

function projectInfo() {
  return {
    root: openRoot,
    name: openRoot.split("/").pop(),
    main: config.main,
    main_candidates: ["main.tex"],
    config,
    tree: tree(),
  };
}

export async function invoke<T>(cmd: string, args: Args = {}): Promise<T> {
  const handler = handlers[cmd];
  if (!handler) {
    console.info("[mock] unhandled command", cmd, args);
    return null as T;
  }
  if (cmd === "build") mockEmit("build-started", null);
  return (await handler(args)) as T;
}

export class Channel {}
