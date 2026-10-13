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

// A small in-memory file system, so the file tree can be driven in the preview.
interface Node { dir: boolean; modified: number; size: number }
const stamp = Math.floor(Date.now() / 1000);
const fsNodes = new Map<string, Node>(
  (
    [
      ["chapters", true, 0, 0],
      ["chapters/method.tex", false, stamp - 3600, 2210],
      ["chapters/old-draft.tex", false, stamp - 40 * 86400, 640],
      ["figures", true, 0, 0],
      ["figures/reward-curve.pdf", false, stamp - 86400, 48_300],
      ["figures/unused-plot.png", false, stamp - 20 * 86400, 120_400],
      ["lemmata.toml", false, stamp - 7 * 86400, 38],
      ["main.tex", false, stamp - 600, 1480],
      ["refs.bib", false, stamp - 5 * 86400, 910],
    ] as [string, boolean, number, number][]
  ).map(([path, dir, modified, size]) => [path, { dir, modified, size }]),
);
const parentDir = (p: string) => p.split("/").slice(0, -1).join("/");
const underPath = (p: string, root: string) => p === root || p.startsWith(root + "/");

const tree = (parent = ""): unknown[] =>
  [...fsNodes.entries()]
    .filter(([path]) => parentDir(path) === parent)
    .map(([path, n]) => ({
      name: path.split("/").pop(),
      path,
      is_dir: n.dir,
      size: n.dir ? 0 : (files[path]?.length ?? n.size),
      modified: n.modified,
      children: n.dir ? tree(path) : [],
    }))
    .sort((a: any, b: any) => Number(b.is_dir) - Number(a.is_dir) || a.name.localeCompare(b.name));

const touch = (path: string, dir = false) => {
  for (let d = parentDir(path); d; d = parentDir(d)) if (!fsNodes.has(d)) fsNodes.set(d, { dir: true, modified: 0, size: 0 });
  fsNodes.set(path, { dir, modified: Math.floor(Date.now() / 1000), size: 0 });
};
const subtree = (root: string) => [...fsNodes.keys()].filter((p) => underPath(p, root));
const relocate = (from: string, to: string) => {
  if (fsNodes.has(to)) throw `${to} already exists.`;
  for (const p of subtree(from)) {
    const dest = to + p.slice(from.length);
    touch(parentDir(dest) + "/_", true);
    fsNodes.delete(parentDir(dest) + "/_");
    fsNodes.set(dest, fsNodes.get(p)!);
    fsNodes.delete(p);
    if (p in files) {
      files[dest] = files[p];
      delete files[p];
    }
  }
};
const topOf = (paths: string[]) => paths.filter((p, i) => paths.indexOf(p) === i && !paths.some((o) => o !== p && p.startsWith(o + "/")));
const trashBin = new Map<string, { paths: string[]; nodes: [string, Node][]; text: Record<string, string> }>();

let config = { main: "main.tex", engine: "pdflatex", bib: "biber", shell_escape: false, outdir: ".lemmata/build", spellcheck: null, words: [], figures_dir: "figures" };

const errorDiagnostics = [
  { severity: "error", kind: { type: "undefined-control-sequence" }, file: "./chapters/method.tex", line: 4, message: "Undefined control sequence.", hint: "A command isn't defined. Check its spelling, or load the package that provides it.", fix: { explanation: "\\mathbb comes from the amssymb package.", command: null, package: "amssymb" } },
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
  clean_build: () => 4,
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
    { id: "homework", name: "Homework", description: "Exercise sheets: shaded problem boxes, solutions, and a header with your name and course." },
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
  history_restore_all: () => 3,
  history_compare: () => [{ status: "modified", path: "main.tex" }, { status: "added", path: "refs.bib" }],
  history_relabel: () => {},
  copy_path: (a) => {
    for (const p of subtree(a.from)) {
      const dest = a.to + p.slice(a.from.length);
      fsNodes.set(dest, { ...fsNodes.get(p)! });
      if (p in files) files[dest] = files[p];
    }
    if (!fsNodes.has(a.to)) throw `${a.from} doesn't exist.`;
  },
  create_path: (a) => {
    if (fsNodes.has(a.path)) throw `${a.path} already exists.`;
    touch(a.path, a.dir);
    if (!a.dir) files[a.path] = "";
  },
  rename_path: (a) => {
    relocate(a.from, a.to);
    return 0;
  },
  move_paths: (a) => {
    const plan = topOf(a.paths)
      .map((from: string) => ({ from, to: (a.folder ? a.folder + "/" : "") + from.split("/").pop() }))
      .filter((m: { from: string; to: string }) => m.from !== m.to);
    for (const m of plan) {
      if (fsNodes.has(m.to)) throw `${m.to} already exists.`;
      if (a.folder === m.from || a.folder.startsWith(m.from + "/")) throw `${m.from} can't be moved into itself.`;
    }
    for (const m of plan) relocate(m.from, m.to);
    return plan;
  },
  trash_paths: (a) => {
    const paths = topOf(a.paths);
    const id = String(Date.now());
    const nodes = paths.flatMap((p) => subtree(p)).map((p): [string, Node] => [p, fsNodes.get(p)!]);
    const text: Record<string, string> = {};
    for (const [p] of nodes) {
      if (p in files) text[p] = files[p];
      fsNodes.delete(p);
      delete files[p];
    }
    trashBin.set(id, { paths, nodes, text });
    return { id, paths };
  },
  restore_trash: (a) => {
    const batch = trashBin.get(a.id);
    if (!batch) throw "That deletion can't be undone anymore.";
    for (const p of batch.paths) if (fsNodes.has(p)) throw `${p} exists again, so it can't be restored.`;
    for (const [p, n] of batch.nodes) fsNodes.set(p, n);
    Object.assign(files, batch.text);
    trashBin.delete(a.id);
    return batch.paths;
  },
  trash_path: (a) => handlers.trash_paths({ paths: [a.path] }),
  purge_trash: () => 0,
  symbol_defs: () => [
    { kind: "label", role: "def", name: "sec:intro", file: "main.tex", line: 11, col: 7, text: "\\section{Introduction}\\label{sec:intro}" },
    { kind: "label", role: "def", name: "eq:policy", file: "chapters/method.tex", line: 5, col: 6, text: "\\label{eq:policy}" },
    { kind: "cite", role: "def", name: "schulman2017ppo", file: "refs.bib", line: 1, col: 9, text: "@article{schulman2017ppo," },
    { kind: "cite", role: "def", name: "mnih2015human", file: "refs.bib", line: 9, col: 9, text: "@article{mnih2015human," },
  ],
  symbol_uses: (a) => [
    { kind: a.kind, role: "def", name: a.name, file: "main.tex", line: 11, col: 7, text: `\\label{${a.name}}` },
    { kind: a.kind, role: "use", name: a.name, file: "chapters/method.tex", line: 7, col: 20, text: `as in \\ref{${a.name}}` },
  ],
  rename_symbol: () => ["main.tex", "chapters/method.tex"],
  unused_files: () => ["chapters/old-draft.tex", "figures/unused-plot.png"],
  read_binary: async () => (await fetch(samplePdf)).arrayBuffer(),
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
  search_project: (a) => {
    const q = String(a.options.query ?? "");
    if (!q) return [];
    const needle = a.options.case_sensitive ? q : q.toLowerCase();
    return Object.entries(files).flatMap(([path, text]) => {
      const matches = text.split("\n").flatMap((line, i) => {
        const hay = a.options.case_sensitive ? line : line.toLowerCase();
        const ranges: [number, number][] = [];
        for (let at = hay.indexOf(needle); at >= 0; at = hay.indexOf(needle, at + needle.length)) ranges.push([at, at + needle.length]);
        const replaced = a.replacement == null ? undefined : line.split(q).join(a.replacement);
        return ranges.length ? [{ line: i + 1, text: line, ranges, replaced }] : [];
      });
      return matches.length ? [{ path, matches }] : [];
    });
  },
  replace_in_files: (a) => {
    let n = 0;
    for (const t of a.targets) {
      files[t.path] = files[t.path].split("\n").map((l: string, i: number) => (t.lines.includes(i + 1) ? (n++, l.split(a.options.query).join(a.replacement)) : l)).join("\n");
    }
    return n;
  },
  update_check: () => null,
  has_package: () => true,
  find_references: (a) => {
    const stem = String(a.path).replace(/\.tex$/, "");
    return Object.entries(files).flatMap(([file, text]) =>
      text.split("\n").flatMap((line, i) => (line.includes(`{${stem}}`) || line.includes(`{${a.path}}`) ? [{ file, line: i + 1, text: line.trim() }] : [])),
    );
  },
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
