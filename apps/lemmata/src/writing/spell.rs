//! The spell checker's app side (spell/service.ts, magic.ts): which language applies, and which
//! words were already checked, so each is asked about once. Checking itself runs in lemmata-spell
//! behind the shell; it is fast enough to ask from the UI thread for the lines on screen.

use lemmata_shell::Shell;
use regex::Regex;
use std::collections::HashMap;
use std::sync::{Arc, LazyLock};

/// Language codes and their names, bundled first, then downloaded on demand.
pub const LANGUAGES: &[(&str, &str)] = &[
    ("en", "English (US)"),
    ("en-GB", "English (UK)"),
    ("de", "Deutsch"),
    ("de-CH", "Deutsch (Schweiz)"),
    ("fr", "Français"),
    ("es", "Español"),
    ("it", "Italiano"),
    ("pt-PT", "Português (Portugal)"),
    ("pt-BR", "Português (Brasil)"),
    ("nl", "Nederlands"),
    ("pl", "Polski"),
    ("sv", "Svenska"),
    ("da", "Dansk"),
    ("nb", "Norsk bokmål"),
    ("cs", "Čeština"),
    ("ru", "Русский"),
    ("uk", "Українська"),
    ("tr", "Türkçe"),
    ("el", "Ελληνικά"),
];

pub fn label(code: &str) -> &str {
    LANGUAGES
        .iter()
        .find(|(c, _)| *c == code)
        .map_or(code, |(_, l)| l)
}

/// Guesses the language from babel or polyglossia in the preamble.
pub fn detect_language(source: &str) -> &'static str {
    static BABEL: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"\\usepackage\[([^\]]*)\]\{babel\}").unwrap());
    static POLY: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"\\set(?:default|main)language(?:\[[^\]]*\])?\{([^}]*)\}").unwrap()
    });
    let babel = BABEL
        .captures(source)
        .map(|c| c[1].to_owned())
        .unwrap_or_default();
    let poly = POLY
        .captures(source)
        .map(|c| c[1].to_owned())
        .unwrap_or_default();
    // babel's last option is the main language.
    let main = if poly.is_empty() {
        babel
            .split(',')
            .map(str::trim)
            .rfind(|s| !s.is_empty())
            .unwrap_or("")
            .to_lowercase()
    } else {
        poly.to_lowercase()
    };
    let has = |words: &[&str]| words.iter().any(|w| main.contains(w));
    if has(&["swiss"]) {
        return "de-CH";
    }
    if has(&["german", "deutsch"]) {
        return "de";
    }
    if has(&["french", "francais"]) {
        return "fr";
    }
    if has(&["british", "ukenglish"]) {
        return "en-GB";
    }
    const OTHER: &[(&[&str], &str)] = &[
        (&["spanish", "espanol"], "es"),
        (&["italian"], "it"),
        (&["brazil"], "pt-BR"),
        (&["portuguese"], "pt-PT"),
        (&["dutch"], "nl"),
        (&["polish"], "pl"),
        (&["swedish"], "sv"),
        (&["danish"], "da"),
        (&["norsk", "norwegian", "bokmal"], "nb"),
        (&["czech"], "cs"),
        (&["russian"], "ru"),
        (&["ukrainian"], "uk"),
        (&["turkish"], "tr"),
        (&["greek"], "el"),
    ];
    OTHER.iter().find(|(w, _)| has(w)).map_or("en", |(_, c)| c)
}

/// "de_DE" → "de", "en_GB" → "en-GB", "de-CH" → "de-CH"; None when unsupported.
pub fn normalize_language(code: &str) -> Option<String> {
    let code = code.trim().replacen('_', "-", 1);
    let mut parts = code.split('-');
    let l = parts.next().filter(|l| !l.is_empty())?.to_lowercase();
    let r = parts.next().map(str::to_uppercase);
    let known = |c: &str| LANGUAGES.iter().any(|(k, _)| *k == c);
    let exact = match &r {
        Some(r) => format!("{l}-{r}"),
        None => l.clone(),
    };
    if known(&exact) {
        return Some(exact);
    }
    // "de-DE", "en-US", "fr-CA" use their language's plain dictionary.
    let plain = match l.as_str() {
        "pt" => "pt-PT".to_owned(),
        "no" => "nb".to_owned(),
        _ => l,
    };
    known(&plain).then_some(plain)
}

/// The language a file asks for in its first lines (`% !TeX spellcheck = de_DE`).
pub fn magic_language(source: &str) -> Option<String> {
    static MAGIC: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?i)^\s*%\s*!TeX\s+(?:spellcheck|lang(?:uage)?)\s*=\s*([A-Za-z]{2,3}(?:[_-][A-Za-z]{2,4})?)").unwrap()
    });
    source
        .split('\n')
        .take(30)
        .find_map(|l| MAGIC.captures(l))
        .and_then(|c| normalize_language(&c[1]))
}

/// Acronyms, mixed-case names (LaTeX, iPhone) and single letters aren't checked.
pub fn checkable(w: &str) -> bool {
    w.chars().count() > 1 && w != w.to_uppercase() && !w.chars().skip(1).any(char::is_uppercase)
}

/// The checker's state as the editors see it.
#[derive(Default)]
pub struct Spell {
    /// The language loaded, or None when off (or not loaded yet).
    pub lang: Option<String>,
    /// Bumped whenever verdicts may have changed (another language, a word added).
    pub generation: u64,
    /// true = correct, for the current language.
    known: HashMap<String, bool>,
    pub shell: Option<Arc<Shell>>,
}

impl Spell {
    pub fn set_loaded(&mut self, lang: Option<String>) {
        self.lang = lang;
        self.known.clear();
        self.generation += 1;
    }

    /// Whether `word` is misspelled, asking the checker about words not seen before.
    pub fn bad(&mut self, word: &str) -> bool {
        if self.lang.is_none() || !checkable(word) {
            return false;
        }
        if let Some(ok) = self.known.get(word) {
            return !ok;
        }
        let Some(shell) = &self.shell else {
            return false;
        };
        let bad = !shell.spell_check(vec![word.to_owned()]).is_empty();
        if self.known.len() > 50_000 {
            self.known.clear();
        }
        self.known.insert(word.to_owned(), !bad);
        bad
    }

    /// Accepts a word (the app keeps it in lemmata.toml too).
    pub fn add(&mut self, word: &str) {
        self.known.insert(word.to_owned(), true);
        if let Some(s) = &self.shell {
            s.spell_add(word.to_owned());
        }
        self.generation += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // spell/service.test.ts, magic.test.ts
    #[test]
    fn detects_languages_from_babel_polyglossia_and_magic_comments() {
        assert_eq!(
            detect_language("\\usepackage[english,ngerman]{babel}"),
            "de"
        );
        assert_eq!(detect_language("\\usepackage[swissgerman]{babel}"), "de-CH");
        assert_eq!(detect_language("\\setdefaultlanguage{french}"), "fr");
        assert_eq!(detect_language("\\usepackage[british]{babel}"), "en-GB");
        assert_eq!(detect_language("\\usepackage[brazilian]{babel}"), "pt-BR");
        assert_eq!(detect_language("nothing"), "en");
        assert_eq!(normalize_language("de_DE").as_deref(), Some("de"));
        assert_eq!(normalize_language("en_GB").as_deref(), Some("en-GB"));
        assert_eq!(normalize_language("de-CH").as_deref(), Some("de-CH"));
        assert_eq!(normalize_language("pt").as_deref(), Some("pt-PT"));
        assert_eq!(normalize_language("xx"), None);
        assert_eq!(
            magic_language("% !TeX spellcheck = de_DE\n\\documentclass{article}").as_deref(),
            Some("de")
        );
        assert_eq!(magic_language("%!TEX lang=fr").as_deref(), Some("fr"));
        assert_eq!(magic_language("\\documentclass{article}"), None);
    }

    #[test]
    fn skips_acronyms_and_names() {
        assert!(checkable("word") && checkable("Word"));
        assert!(
            !checkable("a") && !checkable("NASA") && !checkable("LaTeX") && !checkable("iPhone")
        );
    }
}
