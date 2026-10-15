//! Spell checking with Hunspell dictionaries via spellbook.
//!
//! Dictionaries live in `<dir>/<lang>/index.{aff,dic}`: the bundled ones in
//! `data/dictionaries`, the others downloaded on demand (see `remote`).
//! `Speller::load` searches its directories in order. Swiss German uses the German dictionary and accepts
//! "ss" wherever standard German writes "ß".

use std::collections::HashSet;
use std::path::{Path, PathBuf};

mod remote;
pub use remote::{COMMIT, REMOTE, Remote, download, remote};

/// Dictionaries that ship with the app.
pub const LANGUAGES: &[&str] = &["en", "en-GB", "de", "de-CH", "fr"];

pub struct Speller {
    dict: spellbook::Dictionary,
    swiss: bool,
    personal: HashSet<String>,
}

#[derive(Debug)]
pub struct Error(pub String);

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// What a language picker needs to know about one dictionary.
#[derive(Debug, serde::Serialize)]
pub struct Available {
    pub code: String,
    pub label: String,
    pub installed: bool,
    pub bundled: bool,
    /// Download size; 0 for bundled languages.
    pub size_kb: u32,
}

/// Every supported language: the bundled ones, then the downloadable ones,
/// with whether each is usable now. `bundled`/`downloaded` are the two roots
/// `Speller::load` searches.
pub fn available(bundled: &Path, downloaded: &Path) -> Vec<Available> {
    let has = |root: &Path, code: &str| root.join(code).join("index.dic").exists();
    let builtin = LANGUAGES.iter().map(|&code| Available {
        code: code.into(),
        label: builtin_label(code).into(),
        installed: true,
        bundled: true,
        size_kb: 0,
    });
    let remote = REMOTE.iter().map(|r| {
        let bundled = has(bundled, r.code);
        Available {
            code: r.code.into(),
            label: r.label.into(),
            installed: bundled || has(downloaded, r.code),
            bundled,
            size_kb: r.size_kb,
        }
    });
    builtin.chain(remote).collect()
}

fn builtin_label(code: &str) -> &'static str {
    match code {
        "en" => "English (US)",
        "en-GB" => "English (UK)",
        "de" => "Deutsch",
        "de-CH" => "Deutsch (Schweiz)",
        _ => "Français",
    }
}

/// The first of `dirs` that has a dictionary named `name`.
fn find(dirs: &[&Path], name: &str) -> Option<PathBuf> {
    dirs.iter()
        .map(|d| d.join(name))
        .find(|f| f.join("index.dic").exists())
}

impl Speller {
    /// Loads `lang` from the first of `dirs` that has it (bundled folder
    /// first, then the downloads folder).
    pub fn load(dirs: &[&Path], lang: &str) -> Result<Speller, Error> {
        if !LANGUAGES.contains(&lang) && remote(lang).is_none() {
            return Err(Error(format!("No dictionary for {lang}.")));
        }
        let swiss = lang == "de-CH";
        let name = if swiss { "de" } else { lang };
        let folder = find(dirs, name).ok_or_else(|| {
            let label = remote(lang).map_or(lang, |r| r.label);
            Error(format!("The {label} dictionary isn't downloaded yet."))
        })?;
        // en-GB ships no index.aff of its own: it is byte-identical to en's.
        let affix_folder = if lang == "en-GB" {
            find(dirs, "en").unwrap_or_else(|| folder.clone())
        } else {
            folder.clone()
        };
        let read = |base: &Path, name: &str| {
            std::fs::read_to_string(base.join(name))
                .map_err(|e| Error(format!("Could not read the {lang} dictionary: {e}")))
        };
        let aff = read(&affix_folder, "index.aff")?;
        let dict = spellbook::Dictionary::new(&aff, &read(&folder, "index.dic")?)
            .map_err(|e| Error(format!("The {lang} dictionary is invalid: {e}")))?;
        Ok(Speller {
            dict,
            swiss,
            personal: HashSet::new(),
        })
    }

    pub fn add(&mut self, word: &str) {
        self.personal.insert(word.to_string());
    }

    pub fn check(&self, word: &str) -> bool {
        if self.personal.contains(word) || self.dict.check(word) {
            return true;
        }
        // Swiss spelling: try ß for each "ss" (Strasse → Straße, Grösse → Größe).
        self.swiss
            && word.contains("ss")
            && sharp_s_variants(word).iter().any(|v| self.dict.check(v))
    }

    pub fn suggest(&self, word: &str) -> Vec<String> {
        let mut out = Vec::new();
        self.dict.suggest(word, &mut out);
        if self.swiss {
            for s in &mut out {
                *s = s.replace('ß', "ss");
            }
            let mut seen = HashSet::new();
            out.retain(|s| seen.insert(s.clone()) && s != word);
        }
        out.truncate(6);
        out
    }
}

/// Every way of turning one or more "ss" in `word` into "ß".
fn sharp_s_variants(word: &str) -> Vec<String> {
    // Overlapping, so "sss" (Grossstadt) yields both Großstadt and Grosßtadt.
    let positions: Vec<usize> = (0..word.len().saturating_sub(1))
        .filter(|&i| word.is_char_boundary(i) && word[i..].starts_with("ss"))
        .collect();
    let mut out = Vec::new();
    // Cap combinations to keep this cheap; real words have few.
    let n = positions.len().min(4);
    for mask in 1u32..(1 << n) {
        let mut s = String::with_capacity(word.len());
        let mut last = 0;
        for (bit, &p) in positions.iter().take(n).enumerate() {
            if mask & (1 << bit) != 0 && p >= last {
                s.push_str(&word[last..p]);
                s.push('ß');
                last = p + 2;
            }
        }
        s.push_str(&word[last..]);
        out.push(s);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn dir() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../data/dictionaries")
    }

    #[test]
    fn english_and_british() {
        let en = Speller::load(&[&dir()], "en").unwrap();
        assert!(en.check("methods") && !en.check("recieve"));
        assert_eq!(en.suggest("recieve")[0], "receive");
        // en-GB has no index.aff of its own, so this also checks the en fallback.
        let gb = Speller::load(&[&dir()], "en-GB").unwrap();
        assert!(gb.check("colour") && gb.check("methods") && !gb.check("recieve"));
    }

    #[test]
    fn german_compounds() {
        let de = Speller::load(&[&dir()], "de").unwrap();
        for w in ["Übersetzung", "Größe", "Forschungsergebnisse"] {
            assert!(de.check(w), "{w}");
        }
        assert!(!de.check("Ubersetzung"));
    }

    #[test]
    fn swiss_german_writes_ss() {
        let ch = Speller::load(&[&dir()], "de-CH").unwrap();
        for w in ["Grösse", "Strasse", "Grüsse", "Übersetzung"] {
            assert!(ch.check(w), "{w}");
        }
        assert!(!ch.check("Grösee"));
        assert!(ch.suggest("Strase").iter().all(|s| !s.contains('ß')));
    }

    #[test]
    fn french_and_personal_words() {
        let mut fr = Speller::load(&[&dir()], "fr").unwrap();
        assert!(fr.check("l'ordinateur") && !fr.check("ordinatuer"));
        assert!(!fr.check("Lemmata"));
        fr.add("Lemmata");
        assert!(fr.check("Lemmata"));
    }

    #[test]
    fn lookup_prefers_the_bundled_folder() {
        let (a, b) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
        let put = |root: &Path, word: &str| {
            let f = root.join("es");
            std::fs::create_dir_all(&f).unwrap();
            std::fs::write(f.join("index.aff"), "SET UTF-8\n").unwrap();
            std::fs::write(f.join("index.dic"), format!("1\n{word}\n")).unwrap();
        };
        let err = Speller::load(&[a.path(), b.path()], "es").err().unwrap();
        assert!(err.0.contains("isn't downloaded"), "{err}");
        put(b.path(), "segundo");
        assert!(
            Speller::load(&[a.path(), b.path()], "es")
                .unwrap()
                .check("segundo")
        );
        put(a.path(), "primero");
        let sp = Speller::load(&[a.path(), b.path()], "es").unwrap();
        assert!(sp.check("primero") && !sp.check("segundo"));
        let es = |root: &Path| {
            available(root, b.path())
                .into_iter()
                .find(|l| l.code == "es")
                .unwrap()
        };
        assert!(es(a.path()).bundled && es(a.path()).installed);
        let none = tempfile::tempdir().unwrap();
        assert!(
            available(none.path(), none.path())
                .iter()
                .filter(|l| !l.installed)
                .count()
                == REMOTE.len()
        );
        assert!(Speller::load(&[a.path()], "zz").is_err());
    }

    #[test]
    fn variants() {
        assert_eq!(sharp_s_variants("Strasse"), ["Straße"]);
        assert!(sharp_s_variants("Grossstadt").contains(&"Großstadt".to_string()));
    }
}
