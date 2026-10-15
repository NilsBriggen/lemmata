import { describe, expect, it } from "vitest";
import { magicLanguage, normalizeLanguage } from "./magic";

describe("normalizeLanguage", () => {
  it("maps locale codes onto the bundled dictionaries", () => {
    expect(normalizeLanguage("de_DE")).toBe("de");
    expect(normalizeLanguage("de-CH")).toBe("de-CH");
    expect(normalizeLanguage("en_GB")).toBe("en-GB");
    expect(normalizeLanguage("en_US")).toBe("en");
    expect(normalizeLanguage("fr")).toBe("fr");
  });
  it("accepts the downloadable languages", () => {
    expect(normalizeLanguage("es_ES")).toBe("es");
    expect(normalizeLanguage("es-MX")).toBe("es");
    expect(normalizeLanguage("pt_BR")).toBe("pt-BR");
    expect(normalizeLanguage("pt-pt")).toBe("pt-PT");
    expect(normalizeLanguage("pt")).toBe("pt-PT");
    expect(normalizeLanguage("nb_NO")).toBe("nb");
    expect(normalizeLanguage("no")).toBe("nb");
    expect(normalizeLanguage("el_GR")).toBe("el");
    expect(normalizeLanguage("uk_UA")).toBe("uk");
  });
  it("returns null for languages without a dictionary", () => {
    expect(normalizeLanguage("ja")).toBeNull();
    expect(normalizeLanguage("nn")).toBeNull();
    expect(normalizeLanguage("")).toBeNull();
  });
});

describe("magicLanguage", () => {
  it("reads spellcheck and lang comments", () => {
    expect(magicLanguage("% !TeX spellcheck = de_DE\n\\documentclass{article}")).toBe("de");
    expect(magicLanguage("\\documentclass{article}\n%!TeX lang=fr")).toBe("fr");
    expect(magicLanguage("% !tex language = en_GB")).toBe("en-GB");
    expect(magicLanguage("% !TeX spellcheck = es_ES")).toBe("es");
  });
  it("ignores unsupported languages and comments past the first lines", () => {
    expect(magicLanguage("% !TeX spellcheck = ja_JP")).toBeNull();
    expect(magicLanguage("\n".repeat(40) + "% !TeX spellcheck = de")).toBeNull();
    expect(magicLanguage("plain text")).toBeNull();
  });
});
