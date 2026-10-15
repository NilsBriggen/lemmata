// A file can choose its own spell-check language with a magic comment near
// the top, the way TeXstudio and TeXworks do:
//   % !TeX spellcheck = de_DE
//   % !TeX lang = fr
// Anything the spell checker has no dictionary for is ignored.

// Bundled languages, then the ones downloaded on demand (the app fetches a
// missing dictionary when it is chosen in Preferences).
const KNOWN = new Set(["en", "en-GB", "de", "de-CH", "fr", "es", "it", "pt-PT", "pt-BR", "nl", "pl", "sv", "da", "nb", "cs", "ru", "uk", "tr", "el"]);
// Other codes for the same dictionary: bare Portuguese is European, no is Bokmål.
const ALIASES: Record<string, string> = { pt: "pt-PT", no: "nb" };

/** "de_DE" → "de", "en_GB" → "en-GB", "de-CH" → "de-CH"; null when unsupported. */
export function normalizeLanguage(code: string): string | null {
  const [lang, region] = code.trim().replace("_", "-").split("-");
  if (!lang) return null;
  const l = lang.toLowerCase();
  const r = region?.toUpperCase();
  const exact = r ? `${l}-${r}` : l;
  if (KNOWN.has(exact)) return exact;
  // "de-DE", "en-US", "fr-CA" use the plain dictionary of their language.
  const plain = ALIASES[l] ?? l;
  return KNOWN.has(plain) ? plain : null;
}

/** The language a file asks for in its first lines, or null. */
export function magicLanguage(source: string): string | null {
  const head = source.split("\n", 30);
  for (const line of head) {
    const m = /^\s*%\s*!TeX\s+(?:spellcheck|lang(?:uage)?)\s*=\s*([A-Za-z]{2,3}(?:[_-][A-Za-z]{2,4})?)/i.exec(line);
    if (m) return normalizeLanguage(m[1]);
  }
  return null;
}
