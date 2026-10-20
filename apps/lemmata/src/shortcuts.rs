//! Every keyboard shortcut in one table (lib/shortcuts.ts): the app registers its global ones from
//! here, the editor takes its keys from here, and the shortcut sheet lists them, so the three can't
//! disagree. Keys use CodeMirror's notation: "Mod-Shift-p", where Mod is Ctrl (⌘ on a Mac).

use eframe::egui::{self, Key, Modifiers};

pub struct Shortcut {
    pub id: &'static str,
    /// Empty for pointer actions in the "Mouse" group: shown, never matched.
    pub keys: &'static str,
    /// A second key for the same action, e.g. F1 next to Ctrl+Shift+/.
    pub alt: Option<&'static str>,
    pub label: &'static str,
    pub group: &'static str,
}

const fn s(
    id: &'static str,
    keys: &'static str,
    label: &'static str,
    group: &'static str,
) -> Shortcut {
    Shortcut {
        id,
        keys,
        alt: None,
        label,
        group,
    }
}

pub const SHORTCUTS: &[Shortcut] = &[
    s("recompile", "Mod-Enter", "Recompile", "Project"),
    s("save", "Mod-s", "Save all files", "Project"),
    s("snapshot", "Mod-Shift-s", "Save a snapshot", "Project"),
    s("palette", "Mod-Shift-p", "Command palette", "Navigation"),
    s("quick-open", "Mod-p", "Open a file by name", "Navigation"),
    s("search", "Mod-Shift-f", "Search in project", "Navigation"),
    s("focus", "F11", "Focus mode", "Navigation"),
    s("close-tab", "Mod-w", "Close tab", "Navigation"),
    s("next-tab", "Ctrl-Tab", "Next tab", "Navigation"),
    s("prev-tab", "Ctrl-Shift-Tab", "Previous tab", "Navigation"),
    s(
        "goto-heading",
        "Mod-Shift-o",
        "Go to a section",
        "Navigation",
    ),
    s(
        "reopen-tab",
        "Mod-Shift-t",
        "Reopen the last closed tab",
        "Navigation",
    ),
    Shortcut {
        id: "shortcuts",
        keys: "Mod-Shift-/",
        alt: Some("F1"),
        label: "Keyboard shortcuts",
        group: "Navigation",
    },
    s("cite", "Mod-Shift-c", "Insert citation", "Writing"),
    s("symbols", "Mod-Shift-m", "Insert symbol", "Writing"),
    s("bold", "Mod-b", "Bold", "Writing"),
    s("italic", "Mod-i", "Italic", "Writing"),
    s("comment", "Mod-/", "Comment or uncomment lines", "Writing"),
    s(
        "wrap-env",
        "Mod-Shift-e",
        "Wrap selection in an environment",
        "Writing",
    ),
    s(
        "heading-down",
        "Mod-]",
        "Make heading one level deeper",
        "Writing",
    ),
    s(
        "heading-up",
        "Mod-[",
        "Make heading one level higher",
        "Writing",
    ),
    s("next-problem", "F8", "Next problem", "Editing"),
    s("prev-problem", "Shift-F8", "Previous problem", "Editing"),
    s("find", "Mod-f", "Find in file", "Editing"),
    s("find-next", "Mod-g", "Find next", "Editing"),
    s("goto-line", "Mod-Alt-g", "Go to line", "Editing"),
    s("add-comment", "Mod-Alt-m", "Add a comment", "Editing"),
    s("move-line-up", "Alt-ArrowUp", "Move line up", "Editing"),
    s(
        "move-line-down",
        "Alt-ArrowDown",
        "Move line down",
        "Editing",
    ),
    s(
        "copy-line-down",
        "Shift-Alt-ArrowDown",
        "Duplicate line",
        "Editing",
    ),
    s("delete-line", "Mod-Shift-k", "Delete line", "Editing"),
    s("select-next", "Mod-d", "Select next occurrence", "Editing"),
    s("undo", "Mod-z", "Undo", "Editing"),
    s("redo", "Mod-Shift-z", "Redo", "Editing"),
    s("fold", "Ctrl-Shift-[", "Fold", "Editing"),
    s("unfold", "Ctrl-Shift-]", "Unfold", "Editing"),
    s("sync", "Mod-j", "Show this line in the PDF", "PDF"),
    s(
        "pdf-search",
        "Mod-f",
        "Search the PDF (when it has focus)",
        "PDF",
    ),
    s("definition", "F12", "Go to definition", "Editing"),
    s("references", "Shift-F12", "Find references", "Editing"),
    s("rename", "F2", "Rename symbol", "Editing"),
    s("tree-rename", "F2", "Rename the selected file", "Files"),
    s(
        "tree-delete",
        "Delete",
        "Delete the selected files",
        "Files",
    ),
    s("tree-cut", "Mod-x", "Cut files", "Files"),
    s("tree-copy", "Mod-c", "Copy files", "Files"),
    s("tree-paste", "Mod-v", "Paste files", "Files"),
    s("tree-duplicate", "Mod-d", "Duplicate files", "Files"),
    s(
        "tree-undo",
        "Mod-z",
        "Undo the last file operation",
        "Files",
    ),
    s("tree-select-all", "Mod-a", "Select all files", "Files"),
    s(
        "tree-new-file",
        "Mod-n",
        "New file in the selected folder",
        "Files",
    ),
    s("tree-new-folder", "Mod-Shift-n", "New folder", "Files"),
    s(
        "mouse-dblclick",
        "",
        "Double-click in the PDF: jump to the source",
        "Mouse",
    ),
    s(
        "mouse-ctrl-click",
        "",
        "Ctrl+click in the PDF: jump to the source",
        "Mouse",
    ),
    s(
        "mouse-right-file",
        "",
        "Right-click a file: file actions",
        "Mouse",
    ),
    s(
        "mouse-right-word",
        "",
        "Right-click a misspelled word: suggestions",
        "Mouse",
    ),
];

pub fn keys_of(id: &str) -> &'static str {
    SHORTCUTS.iter().find(|s| s.id == id).map_or("", |s| s.keys)
}

const MAC: bool = cfg!(target_os = "macos");

/// "Mod-Shift-p" → "Ctrl+Shift+P" (or ⌘⇧P on a Mac).
pub fn display(keys: &str) -> String {
    let parts = split(keys);
    let (key, mods) = parts.split_last().unwrap_or((&"", &[]));
    let mods: Vec<&str> = mods
        .iter()
        .map(|m| match (*m, MAC) {
            ("Mod", true) => "⌘",
            ("Mod", false) | ("Ctrl", false) => "Ctrl",
            ("Shift", true) => "⇧",
            ("Alt", true) => "⌥",
            ("Ctrl", true) => "⌃",
            (m, _) => m,
        })
        .collect();
    let shown = if key.chars().count() == 1 {
        key.to_uppercase()
    } else {
        (*key).to_owned()
    };
    let mut all: Vec<String> = mods.into_iter().map(String::from).collect();
    all.push(shown);
    all.join(if MAC { "" } else { "+" })
}

/// Splits on `-` that separate parts, so "Mod--" (the minus key) still works.
fn split(spec: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut start = 0;
    let b = spec.as_bytes();
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'-' && i > start && i + 1 < b.len() {
            out.push(&spec[start..i]);
            start = i + 1;
        }
        i += 1;
    }
    out.push(&spec[start..]);
    out
}

fn key_of(name: &str) -> Option<Key> {
    Some(match name {
        "/" => Key::Slash,
        "[" => Key::OpenBracket,
        "]" => Key::CloseBracket,
        "\\" => Key::Backslash,
        "-" => Key::Minus,
        "=" => Key::Equals,
        "," => Key::Comma,
        "." => Key::Period,
        n if n.chars().count() == 1 => Key::from_name(&n.to_uppercase())?,
        n => Key::from_name(n)?,
    })
}

/// The key and modifiers a spec stands for.
pub fn parse(spec: &str) -> Option<(Modifiers, Key)> {
    let parts = split(spec);
    let (key, mods) = parts.split_last()?;
    let mut m = Modifiers::NONE;
    for p in mods {
        match *p {
            "Mod" => m.command = true,
            "Ctrl" => m.ctrl = true,
            "Shift" => m.shift = true,
            "Alt" => m.alt = true,
            _ => return None,
        }
    }
    if m.command {
        if MAC {
            m.mac_cmd = true;
        } else {
            m.ctrl = true;
        }
    }
    Some((m, key_of(key)?))
}

/// Does this key press match the spec? Mod-Shift-/ also matches with the key reported as "?".
pub fn matches_spec(spec: &str, mods: Modifiers, key: Key) -> bool {
    let Some((want, want_key)) = parse(spec) else {
        return false;
    };
    let ctrl = mods.ctrl || mods.mac_cmd || mods.command;
    let want_ctrl = want.ctrl || want.mac_cmd || want.command;
    ctrl == want_ctrl
        && mods.shift == want.shift
        && mods.alt == want.alt
        && (key == want_key
            || (want_key == Key::Slash && key == Key::Questionmark)
            // Shift-[ arrives as `{` on most layouts.
            || (want_key == Key::OpenBracket && key == Key::OpenCurlyBracket)
            || (want_key == Key::CloseBracket && key == Key::CloseCurlyBracket))
}

/// Takes the key press of shortcut `id` out of this frame's input, if there is one.
pub fn consume(ctx: &egui::Context, id: &str) -> bool {
    let Some(s) = SHORTCUTS.iter().find(|s| s.id == id) else {
        return false;
    };
    ctx.input_mut(|i| {
        let mut hit = false;
        i.events.retain(|e| match e {
            egui::Event::Key {
                key,
                pressed: true,
                modifiers,
                ..
            } if !hit
                && [Some(s.keys), s.alt]
                    .into_iter()
                    .flatten()
                    .any(|spec| matches_spec(spec, *modifiers, *key)) =>
            {
                hit = true;
                false
            }
            _ => true,
        });
        hit
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn specs_parse_and_match() {
        let ctrl_shift = Modifiers {
            ctrl: true,
            shift: true,
            command: !MAC,
            ..Modifiers::NONE
        };
        assert!(matches_spec("Mod-Shift-p", ctrl_shift, Key::P));
        assert!(!matches_spec("Mod-p", ctrl_shift, Key::P));
        assert!(matches_spec("Mod-Shift-/", ctrl_shift, Key::Questionmark));
        assert!(matches_spec("F8", Modifiers::NONE, Key::F8));
        assert!(matches_spec(
            "Mod-]",
            Modifiers {
                ctrl: true,
                command: !MAC,
                ..Modifiers::NONE
            },
            Key::CloseBracket
        ));
        assert!(matches_spec(
            "Shift-Alt-ArrowDown",
            Modifiers {
                shift: true,
                alt: true,
                ..Modifiers::NONE
            },
            Key::ArrowDown
        ));
        assert_eq!(split("Mod--"), vec!["Mod", "-"]);
    }

    #[test]
    fn display_reads_like_the_svelte_sheet() {
        if !MAC {
            assert_eq!(display("Mod-Shift-p"), "Ctrl+Shift+P");
            assert_eq!(display("F11"), "F11");
            assert_eq!(display("Alt-ArrowUp"), "Alt+ArrowUp");
        }
    }
}
