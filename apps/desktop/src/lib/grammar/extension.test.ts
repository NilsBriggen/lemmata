import { describe, expect, it, vi } from "vitest";

const invoke = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({ invoke: (...a: unknown[]) => invoke(...a) }));
import { grammar } from "./service";
import { buildMarks } from "./extension";
import { paragraphs } from "./text";

describe("grammar marks", () => {
  it("underlines the finding in the source, skipping removed commands", async () => {
    const src = "Intro.\n\nThey \\emph{is} here and we has \\cite{k} gone.";
    // "has" in the paragraph text "They is here and we has gone."
    const text = paragraphs(src)[1].text;
    invoke.mockResolvedValue([{ offset: text.indexOf("we has"), length: 6, message: "m", shortMessage: "", replacements: ["we have"], ruleId: "R", category: "GRAMMAR" }]);
    grammar.configure("local", "", "en");
    await grammar.check([text]);
    const marks = buildMarks(src, paragraphs(src));
    const out: [string, boolean][] = [];
    marks.between(0, src.length, (from, to, d) => void out.push([src.slice(from, to), d.spec.found.editable]));
    expect(out).toEqual([["we has", true]]);
  });
});
