//! Grammar checking through a LanguageTool server at the address the user set.

use crate::{CmdResult, Shell};

impl Shell {
    pub async fn grammar_check(
        &self,
        url: String,
        language: String,
        text: String,
    ) -> CmdResult<Vec<lemmata_grammar::Match>> {
        lemmata_grammar::check(&url, &language, &text).await
    }

    pub async fn grammar_status(&self, url: String) -> CmdResult<()> {
        lemmata_grammar::status(&url).await
    }
}
