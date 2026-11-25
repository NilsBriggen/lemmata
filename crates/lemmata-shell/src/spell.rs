//! Spell-check commands. The dictionary loads in milliseconds, so it's
//! loaded on demand when the language changes. Bundled dictionaries are
//! looked up first, then the ones downloaded into the app data folder.

use crate::{CmdResult, LockSafe, Shell};
use lemmata_spell::{Available, Speller};
use std::path::PathBuf;

fn bundled_dir(state: &Shell) -> CmdResult<PathBuf> {
    Ok(state.dirs.resources.join("dictionaries"))
}

fn downloaded_dir(state: &Shell) -> CmdResult<PathBuf> {
    Ok(state.dirs.data.join("dictionaries"))
}

#[derive(Default)]
pub(crate) struct SpellerState {
    active_lang: Option<String>,
    /// Small 2-entry LRU cache of loaded dictionaries so bilingual projects and personal-word
    /// updates never reload `.aff`/`.dic` from disk.
    cache: Vec<(String, Speller)>,
}

impl SpellerState {
    fn active(&self) -> Option<&Speller> {
        let lang = self.active_lang.as_deref()?;
        self.cache.iter().find(|(l, _)| l == lang).map(|(_, s)| s)
    }

    fn active_mut(&mut self) -> Option<&mut Speller> {
        let lang = self.active_lang.as_deref()?;
        self.cache
            .iter_mut()
            .find(|(l, _)| l == lang)
            .map(|(_, s)| s)
    }
}

impl Shell {
    /// Every language with whether it can be used now or needs a download.
    pub fn spell_available(&self) -> CmdResult<Vec<Available>> {
        Ok(lemmata_spell::available(
            &bundled_dir(self)?,
            &downloaded_dir(self)?,
        ))
    }

    /// Downloads a dictionary (checksum-verified) into the app data folder.
    pub async fn spell_download(&self, code: String) -> CmdResult<()> {
        lemmata_spell::download(&downloaded_dir(self)?, &code)
            .await
            .map_err(crate::err)
    }

    pub fn spell_set_language(&self, lang: String, words: Vec<String>) -> CmdResult<()> {
        if lang == "off" {
            self.speller.lock_safe().active_lang = None;
            return Ok(());
        }
        // Fast path: reuse an already-loaded dictionary for this language and update personal
        // words in place (zero disk I/O when adding a word or switching between two languages).
        {
            let mut slot = self.speller.lock_safe();
            if let Some(pos) = slot.cache.iter().position(|(l, _)| l == &lang) {
                let mut entry = slot.cache.remove(pos);
                entry.1.set_personal(words);
                slot.cache.insert(0, entry);
                slot.active_lang = Some(lang);
                return Ok(());
            }
        }
        let (bundled, downloaded) = (bundled_dir(self)?, downloaded_dir(self)?);
        // Loaded before the lock is taken: the editor checks words under it while it paints.
        let mut speller = Speller::load(&[&bundled, &downloaded], &lang).map_err(crate::err)?;
        speller.set_personal(words);
        let mut slot = self.speller.lock_safe();
        slot.cache.retain(|(l, _)| l != &lang);
        slot.cache.insert(0, (lang.clone(), speller));
        slot.cache.truncate(2);
        slot.active_lang = Some(lang);
        Ok(())
    }

    /// The words from `words` that are misspelled.
    pub fn spell_check(&self, words: Vec<String>) -> Vec<String> {
        let slot = self.speller.lock_safe();
        let Some(speller) = slot.active() else {
            return Vec::new();
        };
        words.into_iter().filter(|w| !speller.check(w)).collect()
    }

    /// `true` for each word in `words` that is correctly spelled.
    pub fn spell_check_flags(&self, words: &[String]) -> Vec<bool> {
        let slot = self.speller.lock_safe();
        let Some(speller) = slot.active() else {
            return vec![true; words.len()];
        };
        words.iter().map(|w| speller.check(w)).collect()
    }

    pub fn spell_suggest(&self, word: String) -> Vec<String> {
        let slot = self.speller.lock_safe();
        slot.active().map(|s| s.suggest(&word)).unwrap_or_default()
    }

    pub fn spell_add(&self, word: String) {
        if let Some(s) = self.speller.lock_safe().active_mut() {
            s.add(&word);
        }
    }
}
