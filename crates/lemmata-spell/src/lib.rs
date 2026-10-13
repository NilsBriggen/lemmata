//! Spell checking with Hunspell dictionaries via spellbook.
//!
//! Dictionaries live in `<dir>/<lang>/index.{aff,dic}` (see
//! `data/dictionaries`). Swiss German uses the German dictionary and accepts
//! "ss" wherever standard German writes "ß".

use std::collections::HashSet;
use std::path::Path;

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

impl Speller {
    pub fn load(dir: &Path, lang: &str) -> Result<Speller, Error> {
        if !LANGUAGES.contains(&lang) {
            return Err(Error(format!("No dictionary for {lang}.")));
        }
        let swiss = lang == "de-CH";
        let folder = dir.join(if swiss { "de" } else { lang });
        let read = |name: &str| {
            std::fs::read_to_string(folder.join(name))
                .map_err(|e| Error(format!("Could not read the {lang} dictionary: {e}")))
        };
        let dict = spellbook::Dictionary::new(&read("index.aff")?, &read("index.dic")?)
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
        let en = Speller::load(&dir(), "en").unwrap();
        assert!(en.check("methods") && !en.check("recieve"));
        assert_eq!(en.suggest("recieve")[0], "receive");
        let gb = Speller::load(&dir(), "en-GB").unwrap();
        assert!(gb.check("colour"));
    }

    #[test]
    fn german_compounds() {
        let de = Speller::load(&dir(), "de").unwrap();
        for w in ["Übersetzung", "Größe", "Forschungsergebnisse"] {
            assert!(de.check(w), "{w}");
        }
        assert!(!de.check("Ubersetzung"));
    }

    #[test]
    fn swiss_german_writes_ss() {
        let ch = Speller::load(&dir(), "de-CH").unwrap();
        for w in ["Grösse", "Strasse", "Grüsse", "Übersetzung"] {
            assert!(ch.check(w), "{w}");
        }
        assert!(!ch.check("Grösee"));
        assert!(ch.suggest("Strase").iter().all(|s| !s.contains('ß')));
    }

    #[test]
    fn french_and_personal_words() {
        let mut fr = Speller::load(&dir(), "fr").unwrap();
        assert!(fr.check("l'ordinateur") && !fr.check("ordinatuer"));
        assert!(!fr.check("Lemmata"));
        fr.add("Lemmata");
        assert!(fr.check("Lemmata"));
    }

    #[test]
    fn variants() {
        assert_eq!(sharp_s_variants("Strasse"), ["Straße"]);
        assert!(sharp_s_variants("Grossstadt").contains(&"Großstadt".to_string()));
    }
}
