// Dialogs answer with plausible paths so flows can be clicked through.
export async function open(opts: { directory?: boolean; filters?: { extensions: string[] }[] } = {}) {
  if (opts.filters?.some((f) => f.extensions.includes("zip"))) return "/home/you/Downloads/Overleaf Projects (6 items).zip";
  return opts.directory ? "/home/you/Documents/papers" : "/home/you/Documents/papers/figure.png";
}
export async function save(opts: { defaultPath?: string } = {}) {
  return `/home/you/Documents/${opts.defaultPath ?? "export"}`;
}
