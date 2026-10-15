//! Grammar checking through a LanguageTool server at the address the user set.

use crate::CmdResult;

#[tauri::command]
pub async fn grammar_check(
    url: String,
    language: String,
    text: String,
) -> CmdResult<Vec<lemmata_grammar::Match>> {
    lemmata_grammar::check(&url, &language, &text).await
}

#[tauri::command]
pub async fn grammar_status(url: String) -> CmdResult<()> {
    lemmata_grammar::status(&url).await
}
