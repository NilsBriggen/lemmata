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
        let mut slot = self.speller.lock_safe();
        if lang == "off" {
            *slot = None;
            return Ok(());
        }
        let (bundled, downloaded) = (bundled_dir(self)?, downloaded_dir(self)?);
        let mut speller = Speller::load(&[&bundled, &downloaded], &lang).map_err(crate::err)?;
        for w in &words {
            speller.add(w);
        }
        *slot = Some(speller);
        Ok(())
    }

    /// The words from `words` that are misspelled.
    pub fn spell_check(&self, words: Vec<String>) -> Vec<String> {
        let slot = self.speller.lock_safe();
        let Some(speller) = slot.as_ref() else {
            return Vec::new();
        };
        words.into_iter().filter(|w| !speller.check(w)).collect()
    }

    pub fn spell_suggest(&self, word: String) -> Vec<String> {
        let slot = self.speller.lock_safe();
        slot.as_ref().map(|s| s.suggest(&word)).unwrap_or_default()
    }

    pub fn spell_add(&self, word: String) {
        if let Some(s) = self.speller.lock_safe().as_mut() {
            s.add(&word);
        }
    }
}
