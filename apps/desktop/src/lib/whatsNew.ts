// What a release lets people do, in plain words, for the short "What's new"
// window after an update: a handful per release, the ones people would miss.
// Only releases with something worth showing get an entry; the rest is in the changelog.

export interface Highlight {
  title: string;
  text: string;
}

export interface Release {
  version: string;
  highlights: Highlight[];
}

/** Newest first. */
export const RELEASES: Release[] = [
  {
    version: "2.0.0",
    highlights: [
      { title: "Git, built in", text: "Commit, pull and push from the Git panel, switch branches, and clone a project from GitHub, GitLab or Overleaf." },
      { title: "Review every change", text: "Compare your project with an earlier snapshot and keep or undo each change, or get a PDF with the changes marked." },
      { title: "Compile without installing TeX", text: "No TeX on this computer? Lemmata can download a small engine and compile right away." },
      { title: "A template for every occasion", text: "Journal and conference papers, a full thesis, a book, a poster, slides, an exam, a cover letter and more." },
      { title: "Spell check in many more languages", text: "Besides English, German and French, you can now check Spanish, Italian, Portuguese, Dutch, Polish, Swedish, Danish, Norwegian, Czech, Russian, Ukrainian, Turkish and Greek. Pick one in Preferences and it downloads once." },
    ],
  },
  {
    version: "1.5.0",
    highlights: [
      { title: "Find references online", text: "Search for papers by title and add them to your bibliography, then tidy it in the bibliography manager." },
      { title: "Hover to see what you refer to", text: "Point at a reference, citation or figure to see the equation, the paper or the image." },
      { title: "Comments and TODOs", text: "Leave comments on any part of the text and keep track of your TODOs in one list." },
      { title: "Grammar checking", text: "Turn on LanguageTool in Preferences to catch grammar mistakes as you write." },
      { title: "Clearer errors", text: "Many more TeX errors now come with a plain explanation of what went wrong and how to fix it." },
    ],
  },
];

/** Compares "1.5.2" and "2.0.0-rc.1" style versions; a pre-release comes before its release. */
export function compareVersions(a: string, b: string): number {
  const parse = (v: string) => {
    const [main, pre] = v.split("-", 2);
    return { nums: main.split(".").map((n) => parseInt(n, 10) || 0), pre: pre ?? null };
  };
  const [x, y] = [parse(a), parse(b)];
  for (let i = 0; i < 3; i++) {
    const d = (x.nums[i] ?? 0) - (y.nums[i] ?? 0);
    if (d) return Math.sign(d);
  }
  if (x.pre === y.pre) return 0;
  if (x.pre === null) return 1;
  if (y.pre === null) return -1;
  return x.pre.localeCompare(y.pre, undefined, { numeric: true });
}

/**
 * The releases to show after updating from `last` to `current`: newer than
 * `last`, up to `current`. A pre-release of 2.0.0 already shows 2.0.0's entry,
 * and its final release doesn't show it again.
 */
export function releasesSince(last: string, current: string): Release[] {
  const base = (v: string) => v.split("-")[0];
  return RELEASES.filter(
    (r) => compareVersions(r.version, base(last)) > 0 && compareVersions(r.version, base(current)) <= 0,
  );
}
