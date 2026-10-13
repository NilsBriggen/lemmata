// User snippets: type a short trigger (like "fig") and the completion list
// offers its text, with ${} marking tab stops. They live in the settings.
import { snippet, type Completion, type CompletionContext, type CompletionResult, type CompletionSource } from "@codemirror/autocomplete";

export interface Snippet {
  trigger: string;
  /** Text to insert; `${}` or `${name}` marks a tab stop. */
  body: string;
  description: string;
}

/** Triggers are letters, digits and dashes, starting with a letter. */
export const validTrigger = (t: string) => /^[A-Za-z][A-Za-z0-9-]{1,23}$/.test(t);

/** Snippets whose trigger starts with what was typed (at least two letters), best match first. */
export function matchSnippets(snippets: Snippet[], typed: string): Snippet[] {
  if (typed.length < 2) return [];
  const t = typed.toLowerCase();
  return snippets
    .filter((s) => validTrigger(s.trigger) && s.body && s.trigger.toLowerCase().startsWith(t))
    .sort((a, b) => a.trigger.length - b.trigger.length || a.trigger.localeCompare(b.trigger));
}

export function snippetCompletions(get: () => Snippet[]): CompletionSource {
  return (ctx: CompletionContext): CompletionResult | null => {
    const word = ctx.matchBefore(/[A-Za-z][A-Za-z0-9-]*/);
    if (!word) return null;
    // A command name or a braced argument is for the other sources.
    const before = ctx.state.sliceDoc(Math.max(0, word.from - 1), word.from);
    if (before === "\\" || before === "{") return null;
    const hits = matchSnippets(get(), word.text);
    if (!hits.length) return null;
    const options: Completion[] = hits.map((s) => ({
      label: s.trigger,
      detail: s.description || "snippet",
      info: s.body.replace(/\$\{[^}]*\}/g, "…"),
      type: "text",
      boost: 99,
      apply: snippet(s.body),
    }));
    return { from: word.from, options, validFor: /^[A-Za-z0-9-]*$/ };
  };
}
