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

/// Keys the user chose instead of the default (Preferences → the shortcut sheet), by id. An
/// empty spec turns a shortcut off.
static CHOSEN: std::sync::RwLock<Vec<(&'static str, &'static str)>> =
    std::sync::RwLock::new(Vec::new());

/// Shortcuts whose keys come from the system (copy, cut, paste) or from arrow combinations the
/// editor handles together: shown, not changeable.
const FIXED: &[&str] = &[
    "move-line-up",
    "move-line-down",
    "copy-line-down",
    "tree-cut",
    "tree-copy",
    "tree-paste",
];

pub fn editable(s: &Shortcut) -> bool {
    !s.keys.is_empty() && !FIXED.contains(&s.id)
}

/// Takes the user's keys from the settings. Unknown ids, specs that don't parse and fixed
/// shortcuts are left out.
pub fn choose(chosen: &std::collections::BTreeMap<String, String>) {
    let list = SHORTCUTS
        .iter()
        .filter(|s| editable(s))
        .filter_map(|s| {
            let spec = chosen.get(s.id)?;
            (spec.is_empty() || parse(spec).is_some())
                .then(|| (s.id, &*Box::leak(spec.clone().into_boxed_str())))
        })
        .collect();
    if let Ok(mut c) = CHOSEN.write() {
        *c = list;
    }
}

fn chosen(id: &str) -> Option<&'static str> {
    CHOSEN
        .read()
        .ok()?
        .iter()
        .find(|(i, _)| *i == id)
        .map(|(_, k)| *k)
}

/// The keys in effect: the user's, else the default and its second key.
pub fn specs(s: &Shortcut) -> Vec<&'static str> {
    match chosen(s.id) {
        Some("") => Vec::new(),
        Some(k) => vec![k],
        None => [Some(s.keys), s.alt].into_iter().flatten().collect(),
    }
}

/// The main key in effect for `id` ("" when there is none).
pub fn keys_of(id: &str) -> &'static str {
    SHORTCUTS
        .iter()
        .find(|s| s.id == id)
        .and_then(|s| specs(s).first().copied())
        .unwrap_or("")
}

/// A key press as a spec, for recording a shortcut: "Mod-Shift-p". None for a letter, digit or
/// sign without Ctrl, ⌘ or Alt, which would stop that key from typing.
pub fn spec_of(mods: Modifiers, key: Key) -> Option<String> {
    let typed =
        key.name().chars().count() == 1 || matches!(key, Key::Space | Key::Enter | Key::Tab);
    if typed && !(mods.command || mods.mac_cmd || mods.ctrl || mods.alt) {
        return None;
    }
    let mut parts = Vec::new();
    if mods.command || mods.mac_cmd || (!MAC && mods.ctrl) {
        parts.push("Mod");
    }
    if MAC && mods.ctrl {
        parts.push("Ctrl");
    }
    if mods.alt {
        parts.push("Alt");
    }
    if mods.shift {
        parts.push("Shift");
    }
    let name = key.name();
    let name = if name.chars().count() == 1 {
        name.to_lowercase()
    } else {
        name.to_owned()
    };
    parts.push(&name);
    let spec = parts.join("-");
    parse(&spec).is_some().then_some(spec)
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

/// "Ctrl+Shift+/ / F1": both keys of an entry, as the sheet shows them.
pub fn key_label(s: &Shortcut) -> String {
    specs(s)
        .into_iter()
        .filter(|k| !k.is_empty())
        .map(display)
        .collect::<Vec<_>>()
        .join(" / ")
}

/// Does a search-box query match a shortcut's label or its key text?
pub fn matches_query(s: &Shortcut, query: &str) -> bool {
    let q = query.trim().to_lowercase();
    if q.is_empty() {
        return true;
    }
    let keys = format!("{} {}", specs(s).join(" "), key_label(s)).to_lowercase();
    s.label.to_lowercase().contains(&q) || keys.contains(&q)
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
                && specs(s)
                    .into_iter()
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

    // shortcuts.test.ts: shortcut search
    #[test]
    fn recorded_keys_read_back_and_replace_the_default() {
        let press = |m: Modifiers, k: Key| spec_of(m, k);
        let shift = Modifiers::SHIFT;
        assert_eq!(
            press(Modifiers::COMMAND | shift, Key::P).as_deref(),
            Some("Mod-Shift-p")
        );
        assert_eq!(press(Modifiers::NONE, Key::F2).as_deref(), Some("F2"));
        assert_eq!(
            press(Modifiers::COMMAND, Key::Slash).map(|s| parse(&s).is_some()),
            Some(true)
        );
        assert_eq!(
            press(Modifiers::NONE, Key::X),
            None,
            "a bare letter would stop typing"
        );
        assert_eq!(press(shift, Key::X), None);

        // A shortcut no other test presses; set back at the end.
        let mut chosen = std::collections::BTreeMap::new();
        chosen.insert("goto-heading".to_string(), "Mod-Alt-h".to_string());
        chosen.insert("tree-copy".to_string(), "Mod-Alt-c".to_string());
        chosen.insert("nonsense".to_string(), "Mod-Alt-n".to_string());
        choose(&chosen);
        assert_eq!(keys_of("goto-heading"), "Mod-Alt-h");
        assert_eq!(
            keys_of("tree-copy"),
            "Mod-c",
            "fixed shortcuts keep their keys"
        );
        let ctx = egui::Context::default();
        let key = |k: Key, m: Modifiers| egui::Event::Key {
            key: k,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: m,
        };
        let frame = |events: Vec<egui::Event>, id: &str| {
            let mut hit = false;
            let mut out = ctx.run_ui(
                egui::RawInput {
                    events,
                    ..Default::default()
                },
                |ui| hit = consume(ui.ctx(), id),
            );
            out.textures_delta.clear();
            hit
        };
        let alt_cmd = Modifiers::COMMAND | Modifiers::ALT;
        assert!(frame(vec![key(Key::H, alt_cmd)], "goto-heading"));
        assert!(!frame(
            vec![key(Key::O, Modifiers::COMMAND | shift)],
            "goto-heading"
        ));
        chosen.insert("goto-heading".to_string(), String::new());
        choose(&chosen);
        assert_eq!(keys_of("goto-heading"), "", "turned off");
        choose(&Default::default());
        assert_eq!(keys_of("goto-heading"), "Mod-Shift-o");
    }

    #[test]
    fn the_sheet_searches_labels_and_keys() {
        let find = |q: &str| {
            SHORTCUTS
                .iter()
                .filter(|s| matches_query(s, q))
                .map(|s| s.id)
                .collect::<Vec<_>>()
        };
        assert_eq!(find("").len(), SHORTCUTS.len());
        assert!(find("KEYBOARD").contains(&"shortcuts"));
        assert!(find("f1").contains(&"shortcuts"));
        assert!(find("shift-f8").contains(&"prev-problem"));
        assert!(find("no such thing at all").is_empty());
        let s = SHORTCUTS.iter().find(|x| x.id == "shortcuts").unwrap();
        assert!(key_label(s).ends_with(" / F1"));
    }

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
