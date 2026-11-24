//! Citation picker and "Add reference" commands.

use crate::{CmdResult, Shell, project};
use lemmata_bib::Entry;
use serde::Serialize;

#[derive(Serialize)]
pub struct Bibliography {
    pub files: Vec<String>,
    pub entries: Vec<Entry>,
}

impl Shell {
    pub fn bib_entries(&self) -> CmdResult<Bibliography> {
        let p = project(self)?;
        let files = lemmata_bib::bib_files(&p);
        let entries = lemmata_bib::entries(&p.root, &files);
        Ok(Bibliography { files, entries })
    }
}

#[derive(Serialize)]
pub struct Added {
    pub key: String,
    pub file: String,
}

impl Shell {
    /// Looks up a DOI or arXiv ID on doi.org and appends it to `file` (a .bib
    /// file in the project; created if missing). Network access happens only
    /// here, when the user asks for it.
    pub async fn bib_add_reference(&self, id: String, file: String) -> CmdResult<Added> {
        let p = project(self)?;
        if !file.ends_with(".bib") || crate::files::safe_rel(&file).is_err() {
            return Err("Choose a .bib file inside the project.".into());
        }
        let doi = lemmata_bib::to_doi(&id).ok_or("That doesn't look like a DOI or an arXiv ID.")?;
        let bibtex = lemmata_bib::fetch_bibtex(&doi).await.map_err(crate::err)?;
        let existing: Vec<String> = lemmata_bib::entries(&p.root, &lemmata_bib::bib_files(&p))
            .into_iter()
            .map(|e| e.key)
            .collect();
        let (key, text) = lemmata_bib::prepare_entry(&bibtex, &existing).map_err(crate::err)?;
        let path = p.root.join(&file);
        writable(&p, &path)?;
        lemmata_bib::append(&path, &text).map_err(crate::err)?;
        Ok(Added { key, file })
    }

    /// Searches Crossref by title, author or keywords. The query leaves the
    /// computer only here, when the user clicks "Search online".
    pub async fn bib_search_online(
        &self,
        query: String,
    ) -> CmdResult<Vec<lemmata_bib::search::Hit>> {
        lemmata_bib::search::search_online(&query)
            .await
            .map_err(crate::err)
    }

    /// Every entry with its place in the file, duplicates, and which keys are
    /// cited or missing, for the bibliography manager.
    pub fn bib_library(&self) -> CmdResult<lemmata_bib::manage::Library> {
        let p = project(self)?;
        let files = lemmata_bib::bib_files(&p);
        Ok(lemmata_bib::manage::library(
            &p.root,
            &files,
            &lemmata_bib::cited_keys(&p),
        ))
    }
}

/// Reads `file` (a .bib file in the project), applies `edit` and writes it
/// back atomically. Returns the changed paths for the app to reload.
fn edit_bib(
    state: &Shell,
    file: &str,
    edit: impl FnOnce(&str) -> Result<String, lemmata_bib::Error>,
) -> CmdResult<Vec<String>> {
    let p = project(state)?;
    if !file.ends_with(".bib") || crate::files::safe_rel(file).is_err() {
        return Err("Choose a .bib file inside the project.".into());
    }
    let path = p.root.join(file);
    let text =
        edit(&lemmata_env::fs::read_to_string(&path).map_err(crate::err)?).map_err(crate::err)?;
    writable(&p, &path)?;
    lemmata_bib::manage::write_atomic(&path, &text).map_err(crate::err)?;
    Ok(vec![file.to_string()])
}

impl Shell {
    /// Deletes the entry `key` that starts on `line` of `file`; the rest of the
    /// file stays as it was.
    pub fn bib_remove_entry(
        &self,
        file: String,
        key: String,
        line: usize,
    ) -> CmdResult<Vec<String>> {
        edit_bib(self, &file, |src| {
            lemmata_bib::manage::remove_entry(src, &key, line)
        })
    }

    /// Gives the entry `key` at `line` of `file` a new key. Citations in the
    /// .tex files are not touched: this is for fixing a key that two entries share.
    pub fn bib_rename_key(
        &self,
        file: String,
        key: String,
        line: usize,
        new_key: String,
    ) -> CmdResult<Vec<String>> {
        let p = project(self)?;
        let taken = lemmata_bib::entries(&p.root, &lemmata_bib::bib_files(&p))
            .into_iter()
            .any(|e| e.key.eq_ignore_ascii_case(&new_key));
        if taken {
            return Err("Another entry already uses that key.".into());
        }
        edit_bib(self, &file, |src| {
            lemmata_bib::manage::rename_entry(src, &key, line, &new_key)
        })
    }

    pub async fn zotero_status(&self) -> lemmata_bib::zotero::Status {
        lemmata_bib::zotero::status().await
    }

    /// Opens Zotero's own citation picker; returns the chosen keys.
    pub async fn zotero_pick(&self) -> CmdResult<Vec<String>> {
        lemmata_bib::zotero::pick().await.map_err(crate::err)
    }
}

#[derive(Serialize)]
pub struct ZoteroSync {
    /// Entries written to zotero.bib.
    pub written: usize,
    /// Cited keys neither in the project's other .bib files nor in Zotero.
    pub missing: Vec<String>,
    /// Whether the document already loads zotero.bib.
    pub linked: bool,
}

pub const ZOTERO_BIB: &str = "zotero.bib";

impl Shell {
    /// Writes zotero.bib with every key the document cites that isn't already
    /// in another .bib file of the project, exported from Zotero.
    pub async fn zotero_sync(&self) -> CmdResult<ZoteroSync> {
        let p = project(self)?;
        let files = lemmata_bib::bib_files(&p);
        let others: Vec<String> = files
            .iter()
            .filter(|f| f.as_str() != ZOTERO_BIB)
            .cloned()
            .collect();
        let known: std::collections::HashSet<String> = lemmata_bib::entries(&p.root, &others)
            .into_iter()
            .map(|e| e.key)
            .collect();
        let wanted: Vec<String> = lemmata_bib::cited_keys(&p)
            .into_iter()
            .filter(|k| !known.contains(k))
            .collect();
        let (text, missing) = if wanted.is_empty() {
            (String::new(), Vec::new())
        } else {
            lemmata_bib::zotero::export(&wanted)
                .await
                .map_err(crate::err)?
        };
        // Not one key came back: more likely a Zotero problem than a document citing nothing it
        // has. zotero.bib stays as it was.
        if !wanted.is_empty() && missing.len() == wanted.len() {
            return Err(format!(
                "Zotero knows none of the {} cited keys, so zotero.bib was left as it is. Check that Better BibTeX's citation keys match the document.",
                wanted.len()
            ));
        }
        writable(&p, &p.root.join(ZOTERO_BIB))?;
        let header = "% Exported from Zotero by Lemmata. Changes here are overwritten on the next sync;\n% edit the references in Zotero instead.\n\n";
        lemmata_bib::manage::write_atomic(
            &p.root.join(ZOTERO_BIB),
            format!("{header}{}", text.trim_start()),
        )
        .map_err(crate::err)?;
        Ok(ZoteroSync {
            written: wanted.len() - missing.len(),
            missing,
            linked: lemmata_bib::bib_files(&p).iter().any(|f| f == ZOTERO_BIB),
        })
    }

    /// Adds `\addbibresource{zotero.bib}` (biblatex) or extends `\bibliography{}`.
    pub fn zotero_link(&self) -> CmdResult<()> {
        let p = project(self)?;
        let main = p.main_abs();
        let text = lemmata_env::fs::read_to_string(&main).map_err(crate::err)?;
        // The bibliography may be declared in a file the main one includes.
        let others = crate::files::text_files(&p)
            .into_iter()
            .filter(|f| f.ends_with(".tex"))
            .map(|f| p.root.join(f))
            .filter(|f| *f != main);
        let mut first_error = None;
        for (file, text) in std::iter::once((main.clone(), text))
            .chain(others.filter_map(|f| lemmata_env::fs::read_to_string(&f).ok().map(|t| (f, t))))
        {
            match link_zotero(&text) {
                Ok(new) => {
                    writable(&p, &file)?;
                    return lemmata_bib::manage::write_atomic(&file, new).map_err(crate::err);
                }
                Err(e) => {
                    first_error.get_or_insert(e);
                }
            }
        }
        Err(first_error.unwrap_or_default())
    }
}

/// A link out of the project is written through only in a trusted one.
fn writable(p: &lemmata_project::Project, path: &std::path::Path) -> CmdResult<()> {
    lemmata_env::fs::may_write(&p.root, p.trusted, path).map_err(|e| format!("{e}."))
}

/// The first `command` not in a `%` comment.
fn find_live(text: &str, command: &str) -> Option<usize> {
    text.match_indices(command).map(|(i, _)| i).find(|&i| {
        let line = &text[text[..i].rfind('\n').map_or(0, |n| n + 1)..i];
        !line
            .char_indices()
            .any(|(k, c)| c == '%' && !line[..k].ends_with('\\'))
    })
}

/// `text` loading zotero.bib too.
fn link_zotero(text: &str) -> CmdResult<String> {
    if let Some(pos) = find_live(text, "\\bibliography{") {
        let close = text[pos..].find('}').ok_or("Unclosed \\bibliography{.")? + pos;
        Ok(format!("{},zotero{}", &text[..close], &text[close..]))
    } else if let Some(pos) = find_live(text, "\\addbibresource") {
        let line_end = text[pos..].find('\n').map_or(text.len(), |e| pos + e);
        Ok(format!(
            "{}\n\\addbibresource{{{ZOTERO_BIB}}}{}",
            &text[..line_end],
            &text[line_end..]
        ))
    } else {
        Err("The document has no bibliography yet. Add \\addbibresource{zotero.bib} (biblatex) or \\bibliography{zotero} yourself.".into())
    }
}

#[cfg(test)]
mod link_tests {
    use super::link_zotero;

    #[test]
    fn links_the_live_bibliography_not_a_commented_one() {
        let t = "% \\bibliography{old}\n\\bibliography{refs}\n";
        assert_eq!(
            link_zotero(t).unwrap(),
            "% \\bibliography{old}\n\\bibliography{refs,zotero}\n"
        );
        let t = "\\addbibresource{refs.bib}\nText";
        assert_eq!(
            link_zotero(t).unwrap(),
            "\\addbibresource{refs.bib}\n\\addbibresource{zotero.bib}\nText"
        );
        assert!(link_zotero("% \\addbibresource{x.bib}").is_err());
    }
}

#[cfg(test)]
mod zotero_link_in_an_included_file_tests {
    #[test]
    fn the_bibliography_can_be_declared_in_an_included_file() {
        let d = tempfile::tempdir().unwrap();
        let root = d.path().join("p");
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(
            root.join("main.tex"),
            "\\documentclass{article}\\input{pre}\n",
        )
        .unwrap();
        std::fs::write(root.join("pre.tex"), "\\addbibresource{refs.bib}\n").unwrap();
        let dirs = crate::Dirs {
            resources: d.path().into(),
            data: d.path().into(),
            cache: d.path().into(),
            home: None,
        };
        let shell = crate::Shell::new(dirs, std::sync::Arc::new(|_| {}));
        shell
            .open_project(1, root.to_string_lossy().into_owned(), true)
            .unwrap();
        shell.zotero_link().unwrap();
        let pre = std::fs::read_to_string(root.join("pre.tex")).unwrap();
        assert!(pre.contains("\\addbibresource{zotero.bib}"), "{pre}");
    }
}
