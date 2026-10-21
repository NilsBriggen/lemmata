//! The logic behind the file tree (treeModel.ts): sorting, the rows on screen and which files open.

use crate::screens::library::natural_cmp;
use lemmata_project::Entry;
use std::collections::HashSet;

fn ext(name: &str) -> String {
    match name.rfind('.') {
        Some(i) if i > 0 => name[i + 1..].to_lowercase(),
        _ => String::new(),
    }
}

/// A sorted copy of the tree: "name", "type", "modified" or "size". Folders always sort by name;
/// `folders_first` keeps them above files.
pub fn sort_entries(entries: &[Entry], mode: &str, folders_first: bool) -> Vec<Entry> {
    let mut v: Vec<Entry> = entries
        .iter()
        .map(|e| {
            if e.is_dir {
                Entry {
                    children: sort_entries(&e.children, mode, folders_first),
                    ..e.clone()
                }
            } else {
                e.clone()
            }
        })
        .collect();
    v.sort_by(|a, b| {
        if folders_first && a.is_dir != b.is_dir {
            return if a.is_dir {
                std::cmp::Ordering::Less
            } else {
                std::cmp::Ordering::Greater
            };
        }
        let by_name = || natural_cmp(&a.name, &b.name);
        if a.is_dir && b.is_dir {
            return by_name();
        }
        match mode {
            "type" => ext(&a.name).cmp(&ext(&b.name)).then_with(by_name),
            "modified" => b.modified.cmp(&a.modified).then_with(by_name),
            "size" => b.size.cmp(&a.size).then_with(by_name),
            _ => by_name(),
        }
    });
    v
}

/// The rows on screen, top to bottom, with their depth, skipping collapsed folders' insides.
pub fn flatten<'a>(entries: &'a [Entry], collapsed: &HashSet<String>) -> Vec<(&'a Entry, usize)> {
    fn walk<'a>(
        list: &'a [Entry],
        collapsed: &HashSet<String>,
        depth: usize,
        out: &mut Vec<(&'a Entry, usize)>,
    ) {
        for e in list {
            out.push((e, depth));
            if e.is_dir && !collapsed.contains(&e.path) {
                walk(&e.children, collapsed, depth + 1, out);
            }
        }
    }
    let mut out = Vec::new();
    walk(entries, collapsed, 0, &mut out);
    out
}

pub fn base_name(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

/// Files the editor opens as text. Everything that opens anywhere uses this.
pub fn is_text_file(path: &str) -> bool {
    const EXT: &[&str] = &[
        "tex", "bib", "sty", "cls", "bst", "bbx", "cbx", "tikz", "txt", "md", "markdown", "toml",
        "csv", "tsv", "json", "yml", "yaml", "xml", "ini", "cfg", "conf", "py", "r", "m", "jl",
        "sh", "lua",
    ];
    const NAMES: &[&str] = &[
        ".latexmkrc",
        "latexmkrc",
        "makefile",
        "readme",
        "license",
        "changelog",
        ".gitignore",
        ".gitattributes",
        ".editorconfig",
    ];
    let name = base_name(path).to_lowercase();
    EXT.contains(&ext(&name).as_str()) || NAMES.contains(&name.as_str())
}

/// Images and PDFs open in a read-only viewer tab.
pub fn is_viewer_file(path: &str) -> bool {
    ["png", "jpg", "jpeg", "gif", "webp", "svg", "pdf"].contains(&ext(base_name(path)).as_str())
}

pub fn is_tex(path: &str) -> bool {
    ["tex", "sty", "cls"].contains(&ext(base_name(path)).as_str())
}

/// Files (not folders) under an entry, for the folder badge.
pub fn count_files(e: &Entry) -> usize {
    if e.is_dir {
        e.children.iter().map(count_files).sum()
    } else {
        1
    }
}

pub fn format_size(bytes: u64) -> String {
    if bytes < 1024 {
        format!("{bytes} B")
    } else if bytes < 1024 * 1024 {
        let k = bytes as f64 / 1024.0;
        if bytes < 10 * 1024 {
            format!("{k:.1} KB")
        } else {
            format!("{k:.0} KB")
        }
    } else {
        format!("{:.1} MB", bytes as f64 / 1024.0 / 1024.0)
    }
}

pub fn find<'a>(entries: &'a [Entry], path: &str) -> Option<&'a Entry> {
    for e in entries {
        if e.path == path {
            return Some(e);
        }
        if e.is_dir && path.starts_with(&format!("{}/", e.path)) {
            return find(&e.children, path);
        }
    }
    None
}

/// The icon kind of a file, by extension (Tree.svelte `kind`).
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Kind {
    Tex,
    Bib,
    Image,
    Code,
    Config,
    File,
}

pub fn kind(name: &str) -> Kind {
    match ext(name).as_str() {
        "tex" => Kind::Tex,
        "bib" => Kind::Bib,
        "png" | "jpg" | "jpeg" | "pdf" | "eps" | "svg" | "gif" => Kind::Image,
        "sty" | "cls" | "bst" | "bbx" | "cbx" | "tikz" => Kind::Code,
        "toml" => Kind::Config,
        _ => Kind::File,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn f(path: &str, size: u64, modified: u64) -> Entry {
        Entry {
            name: base_name(path).into(),
            path: path.into(),
            is_dir: false,
            size,
            modified,
            children: vec![],
        }
    }
    fn d(path: &str, children: Vec<Entry>) -> Entry {
        Entry {
            name: base_name(path).into(),
            path: path.into(),
            is_dir: true,
            size: 0,
            modified: 0,
            children,
        }
    }

    #[test]
    fn sorts_by_mode_with_folders_first() {
        let tree = vec![
            f("b.tex", 10, 3),
            d("figs", vec![f("figs/z.png", 1, 1), f("figs/a.png", 1, 2)]),
            f("a10.tex", 30, 1),
            f("a2.bib", 20, 2),
        ];
        let names = |v: &[Entry]| v.iter().map(|e| e.name.clone()).collect::<Vec<_>>();
        assert_eq!(
            names(&sort_entries(&tree, "name", true)),
            ["figs", "a2.bib", "a10.tex", "b.tex"]
        );
        assert_eq!(
            names(&sort_entries(&tree, "type", true)),
            ["figs", "a2.bib", "a10.tex", "b.tex"]
        );
        assert_eq!(
            names(&sort_entries(&tree, "size", false)),
            ["a10.tex", "a2.bib", "b.tex", "figs"]
        );
        assert_eq!(
            names(&sort_entries(&tree, "modified", true)),
            ["figs", "b.tex", "a2.bib", "a10.tex"]
        );
        assert_eq!(
            names(&sort_entries(&tree, "name", true)[0].children),
            ["a.png", "z.png"]
        );
    }

    #[test]
    fn flattens_skipping_collapsed_folders() {
        let tree = vec![d("ch", vec![f("ch/a.tex", 1, 1)]), f("main.tex", 1, 1)];
        let rows = |c: &HashSet<String>| {
            flatten(&tree, c)
                .into_iter()
                .map(|(e, depth)| (e.path.clone(), depth))
                .collect::<Vec<_>>()
        };
        assert_eq!(
            rows(&HashSet::new()),
            vec![
                ("ch".into(), 0),
                ("ch/a.tex".into(), 1),
                ("main.tex".into(), 0)
            ]
        );
        assert_eq!(
            rows(&HashSet::from(["ch".to_owned()])),
            vec![("ch".into(), 0), ("main.tex".into(), 0)]
        );
        assert_eq!(count_files(&tree[0]), 1);
        assert_eq!(
            find(&tree, "ch/a.tex").map(|e| e.name.as_str()),
            Some("a.tex")
        );
    }

    #[test]
    fn knows_what_opens() {
        assert!(
            is_text_file("chapters/intro.tex")
                && is_text_file("Makefile")
                && is_text_file(".latexmkrc")
        );
        assert!(
            !is_text_file("fig.png") && is_viewer_file("fig.PNG") && !is_viewer_file("main.tex")
        );
        assert_eq!(format_size(900), "900 B");
        assert_eq!(format_size(2048), "2.0 KB");
        assert_eq!(format_size(48_300), "47 KB");
        assert_eq!(format_size(3 * 1024 * 1024), "3.0 MB");
    }
}
