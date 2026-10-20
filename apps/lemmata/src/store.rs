//! What the app remembers between runs: settings, recently opened projects and the library's
//! folders. Settings keep tauri-plugin-store's file and format (`settings.json` in the config folder,
//! `{"settings": {…}}`), so an existing install keeps its settings.

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// Tauri's bundle identifier; the config, data and cache folders are named after it.
pub const IDENTIFIER: &str = "dev.briggen.lemmata";

pub struct Dirs {
    pub config: PathBuf,
    pub data: PathBuf,
    pub cache: PathBuf,
}

impl Dirs {
    /// The folders Tauri used, so files from the Tauri app are found.
    pub fn system() -> Self {
        let base = |d: Option<PathBuf>| d.unwrap_or_else(std::env::temp_dir).join(IDENTIFIER);
        Self {
            config: base(dirs::config_dir()),
            data: base(dirs::data_dir()),
            cache: base(dirs::cache_dir()),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Snippet {
    pub trigger: String,
    /// Text to insert; `${}` or `${name}` marks a tab stop.
    pub body: String,
    pub description: String,
}

/// App-wide settings. Project-specific options live in lemmata.toml instead.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    /// "system", "light" or "dark".
    pub theme: String,
    pub font_size: f64,
    /// Delay after the last keystroke before saving; 0 saves only on Ctrl+S.
    pub autosave_ms: f64,
    /// Compile after every save. Off means only the Recompile button builds.
    pub auto_compile: bool,
    pub auto_close: bool,
    pub autocomplete: bool,
    pub env_sync: bool,
    pub folding: bool,
    pub word_wrap: bool,
    /// One indentation level: 2, 4 or 8 spaces, or "tab".
    pub indent: Value,
    /// "plex" (bundled look), "system" or a font family name.
    pub font_family: String,
    pub line_height: f64,
    pub check_for_updates: bool,
    /// A TeX bin folder chosen during onboarding.
    pub tex_dir: Option<String>,
    pub pdf_dark: bool,
    /// "split", "editor" or "pdf".
    pub layout: String,
    pub sidebar_open: bool,
    pub sidebar_width: f64,
    /// Width of the PDF pane in split layout.
    pub pdf_width: f64,
    /// Spell-check language for projects that don't choose one.
    pub spell_default: String,
    /// "list" or "grid".
    pub library_view: String,
    /// "name", "type", "modified" or "size".
    pub tree_sort: String,
    pub tree_folders_first: bool,
    /// "project" keeps deleted files in .lemmata/trash so Undo works; "system" uses the OS trash.
    pub trash_mode: String,
    pub snippets: Vec<Snippet>,
    pub first_run_done: bool,
    /// Projects (by folder) whose lemmata.toml may turn on shell escape: the user allowed it.
    pub shell_escape_projects: Vec<String>,
    /// "off", "local" or "custom".
    pub grammar: String,
    pub grammar_url: String,
    /// "single", "spread" or "spread-first".
    pub pdf_layout: String,
    /// The app version last started, for the "What's new" window after an update.
    pub last_seen_version: String,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            theme: "system".into(),
            font_size: 13.0,
            autosave_ms: 600.0,
            auto_compile: true,
            auto_close: true,
            autocomplete: true,
            env_sync: true,
            folding: true,
            word_wrap: true,
            indent: Value::from(4),
            font_family: "plex".into(),
            line_height: 1.65,
            check_for_updates: true,
            tex_dir: None,
            pdf_dark: false,
            layout: "split".into(),
            sidebar_open: true,
            sidebar_width: 240.0,
            pdf_width: 560.0,
            spell_default: "auto".into(),
            library_view: "list".into(),
            tree_sort: "name".into(),
            tree_folders_first: true,
            trash_mode: "project".into(),
            snippets: Vec::new(),
            first_run_done: false,
            shell_escape_projects: Vec::new(),
            grammar: "off".into(),
            grammar_url: "http://localhost:8081".into(),
            pdf_layout: "single".into(),
            last_seen_version: String::new(),
        }
    }
}

/// Saved values of the wrong type (a damaged or hand-edited file) fall back to the defaults, key by key.
fn valid_settings(saved: &Map<String, Value>) -> Map<String, Value> {
    let Value::Object(defaults) = serde_json::to_value(Settings::default()).unwrap() else {
        unreachable!()
    };
    let kind = |v: &Value| std::mem::discriminant(v);
    saved
        .iter()
        .filter(|(k, v)| match defaults.get(*k) {
            None => false,
            // A number or "tab"; `valid_indent` checks it. (The Svelte app dropped "tab" here, so a
            // tab indent never survived a restart.)
            Some(_) if k.as_str() == "indent" => true,
            Some(Value::Null) => v.is_null() || v.is_string(),
            Some(d) => kind(d) == kind(v),
        })
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect()
}

impl Settings {
    pub fn load(dirs: &Dirs) -> Self {
        let saved = read_json(&dirs.config.join("settings.json"))
            .and_then(|v| v.get("settings").cloned())
            .and_then(|v| v.as_object().cloned())
            .unwrap_or_default();
        Self::from_saved(&saved)
    }

    fn from_saved(saved: &Map<String, Value>) -> Self {
        let mut valid = valid_settings(saved);
        // Before 1.0, "compile as I type" and "save as I type" were one switch.
        if !valid.contains_key("autoCompile") && valid.get("autosaveMs") == Some(&Value::from(0)) {
            valid.insert("autoCompile".into(), Value::Bool(false));
        }
        let Value::Object(mut all) = serde_json::to_value(Settings::default()).unwrap() else {
            unreachable!()
        };
        all.extend(valid);
        let mut settings: Settings = serde_json::from_value(Value::Object(all)).unwrap_or_default();
        settings.indent = valid_indent(&settings.indent);
        settings
    }

    pub fn save(&self, dirs: &Dirs) {
        write_json(
            &dirs.config.join("settings.json"),
            &serde_json::json!({ "settings": self }),
        );
    }

    /// The colour scheme this setting asks for, given the system's.
    pub fn dark(&self, system_dark: bool) -> bool {
        match self.theme.as_str() {
            "dark" => true,
            "light" => false,
            _ => system_dark,
        }
    }
}

/// Settings saved by hand or by older versions may hold anything.
fn valid_indent(v: &Value) -> Value {
    match v {
        Value::String(s) if s == "tab" => v.clone(),
        Value::Number(n) if matches!(n.as_f64(), Some(2.0 | 4.0 | 8.0)) => v.clone(),
        _ => Value::from(4),
    }
}

/// A recently opened project folder.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Recent {
    pub path: String,
    /// Milliseconds since the Unix epoch, as the Svelte app stored it.
    pub last_opened: f64,
}

const MAX_RECENT: usize = 200;

/// Recently opened project folders, newest first.
#[derive(Debug, Default, PartialEq)]
pub struct Recents(pub Vec<Recent>);

impl Recents {
    pub fn load(dirs: &Dirs) -> Self {
        Self::parse(read_json(&dirs.config.join("recent.json")))
    }

    /// Older versions stored plain paths; those still load. Anything damaged is skipped.
    fn parse(v: Option<Value>) -> Self {
        let Some(Value::Array(items)) = v else {
            return Self::default();
        };
        Self(
            items
                .into_iter()
                .filter_map(|e| match e {
                    Value::String(path) => Some(Recent {
                        path,
                        last_opened: 0.0,
                    }),
                    other => serde_json::from_value(other).ok(),
                })
                .collect(),
        )
    }

    pub fn save(&self, dirs: &Dirs) {
        write_json(
            &dirs.config.join("recent.json"),
            &serde_json::to_value(&self.0).unwrap(),
        );
    }

    pub fn remember(&mut self, path: &str, now: f64) {
        self.remember_all(&[path.to_owned()], now);
    }

    /// Adds several projects at once, keeping their order.
    pub fn remember_all(&mut self, paths: &[String], now: f64) {
        for p in paths.iter().rev() {
            self.0.retain(|e| &e.path != p);
            self.0.insert(
                0,
                Recent {
                    path: p.clone(),
                    last_opened: now,
                },
            );
        }
        self.0.truncate(MAX_RECENT);
    }

    pub fn forget(&mut self, path: &str) {
        self.0.retain(|e| e.path != path);
    }
}

/// A folder on the start screen. It only organises the list; project folders on disk never move.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Group {
    pub id: String,
    pub name: String,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Groups {
    pub groups: Vec<Group>,
    /// Project path → the id of its group. A project in no group is absent.
    pub members: BTreeMap<String, String>,
}

pub const MAX_GROUP_NAME: usize = 40;

/// What the start screen shows: the top level (folders and the projects in none), or one folder.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub enum View {
    #[default]
    Root,
    Group(String),
}

impl Groups {
    pub fn load(dirs: &Dirs) -> Self {
        Self::parse(read_json(&dirs.config.join("groups.json")))
    }

    fn parse(v: Option<Value>) -> Self {
        let Some(raw) = v else { return Self::default() };
        let groups: Vec<Group> = match raw.get("groups") {
            Some(Value::Array(a)) => a
                .iter()
                .filter_map(|g| serde_json::from_value(g.clone()).ok())
                .collect(),
            _ => return Self::default(),
        };
        let members = raw
            .get("members")
            .and_then(Value::as_object)
            .map(|m| {
                m.iter()
                    .filter_map(|(path, id)| {
                        let id = id.as_str()?;
                        groups
                            .iter()
                            .any(|g| g.id == id)
                            .then(|| (path.clone(), id.to_owned()))
                    })
                    .collect()
            })
            .unwrap_or_default();
        Self { groups, members }
    }

    pub fn save(&self, dirs: &Dirs) {
        write_json(
            &dirs.config.join("groups.json"),
            &serde_json::to_value(self).unwrap(),
        );
    }

    /// Why `name` can't be used, or None when it can. `except` is the group being renamed.
    pub fn name_problem(&self, name: &str, except: Option<&str>) -> Option<String> {
        let n = name.trim();
        if n.is_empty() {
            return Some("Give the folder a name.".into());
        }
        if n.chars().count() > MAX_GROUP_NAME {
            return Some(format!("Use at most {MAX_GROUP_NAME} characters."));
        }
        if self
            .groups
            .iter()
            .any(|g| Some(g.id.as_str()) != except && g.name.to_lowercase() == n.to_lowercase())
        {
            return Some("A folder with that name exists.".into());
        }
        None
    }

    /// The new folder's id, or None when the name can't be used.
    pub fn create(&mut self, name: &str, id: Option<String>) -> Option<String> {
        if self.name_problem(name, None).is_some() {
            return None;
        }
        let id = id.unwrap_or_else(new_id);
        self.groups.push(Group {
            id: id.clone(),
            name: name.trim().into(),
        });
        Some(id)
    }

    pub fn rename(&mut self, id: &str, name: &str) -> bool {
        if self.name_problem(name, Some(id)).is_some() {
            return false;
        }
        if let Some(g) = self.groups.iter_mut().find(|g| g.id == id) {
            g.name = name.trim().into();
        }
        true
    }

    /// Removes the folder; its projects go back to "no folder".
    pub fn delete(&mut self, id: &str) {
        self.members.retain(|_, g| g != id);
        self.groups.retain(|g| g.id != id);
    }

    /// Puts back a folder that `delete` removed, with its projects. `before` is the state from
    /// before the delete. Changes made since win over the old membership.
    pub fn restore(&mut self, before: &Groups, id: &str) {
        let Some(pos) = before.groups.iter().position(|g| g.id == id) else {
            return;
        };
        if self.groups.iter().any(|g| g.id == id) {
            return;
        }
        self.groups
            .insert(pos.min(self.groups.len()), before.groups[pos].clone());
        for (path, g) in &before.members {
            if g == id {
                self.members
                    .entry(path.clone())
                    .or_insert_with(|| g.clone());
            }
        }
    }

    /// Puts a project in a folder, or in none with `None`. A folder that doesn't exist means none.
    pub fn move_project(&mut self, path: &str, id: Option<&str>) {
        match id {
            Some(id) if self.groups.iter().any(|g| g.id == id) => {
                self.members.insert(path.into(), id.into());
            }
            _ => {
                self.members.remove(path);
            }
        }
    }

    pub fn forget(&mut self, path: &str) {
        self.members.remove(path);
    }

    pub fn count_in(&self, id: &str) -> usize {
        self.members.values().filter(|g| *g == id).count()
    }

    pub fn in_view(&self, path: &str, view: &View) -> bool {
        match view {
            View::Root => !self.members.contains_key(path),
            View::Group(id) => self.members.get(path) == Some(id),
        }
    }
}

fn new_id() -> String {
    use std::hash::{BuildHasher, Hasher};
    let mut h = std::collections::hash_map::RandomState::new().build_hasher();
    h.write_u128(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos(),
    );
    format!("{:08x}", h.finish() as u32)
}

fn read_json(path: &Path) -> Option<Value> {
    serde_json::from_slice(&std::fs::read(path).ok()?).ok()
}

/// Written whole to a temporary file and renamed, so a crash never leaves half a file. Failing to
/// remember is harmless, so errors are ignored.
fn write_json(path: &Path, v: &Value) {
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let tmp = path.with_extension("json.tmp");
    if std::fs::write(&tmp, serde_json::to_vec_pretty(v).unwrap()).is_ok() {
        let _ = std::fs::rename(&tmp, path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn keeps_settings_of_the_right_type_and_drops_the_rest() {
        let saved = json!({ "fontSize": 15, "theme": 3, "snippets": "x", "texDir": null, "layout": "pdf", "unknown": 1, "shellEscapeProjects": ["/p"] });
        let got = valid_settings(saved.as_object().unwrap());
        assert_eq!(
            Value::Object(got),
            json!({ "fontSize": 15, "texDir": null, "layout": "pdf", "shellEscapeProjects": ["/p"] })
        );
        let got = valid_settings(json!({ "texDir": "/opt/tex" }).as_object().unwrap());
        assert_eq!(Value::Object(got), json!({ "texDir": "/opt/tex" }));
        assert!(Settings::default().shell_escape_projects.is_empty());
    }

    #[test]
    fn old_saving_switch_turns_auto_compile_off_and_bad_indent_falls_back() {
        let s = Settings::from_saved(json!({ "autosaveMs": 0, "indent": 3 }).as_object().unwrap());
        assert!(!s.auto_compile);
        assert_eq!(s.indent, json!(4));
        let s = Settings::from_saved(
            json!({ "indent": "tab", "fontSize": 15.5 })
                .as_object()
                .unwrap(),
        );
        assert_eq!((s.indent, s.font_size), (json!("tab"), 15.5));
    }

    #[test]
    fn settings_round_trip_in_the_plugin_store_format() {
        let dir = tempfile::tempdir().unwrap();
        let dirs = Dirs {
            config: dir.path().into(),
            data: dir.path().into(),
            cache: dir.path().into(),
        };
        let s = Settings {
            layout: "pdf".into(),
            ..Settings::default()
        };
        s.save(&dirs);
        let raw = read_json(&dir.path().join("settings.json")).unwrap();
        assert_eq!(raw["settings"]["layout"], "pdf");
        assert_eq!(Settings::load(&dirs), s);
    }

    #[test]
    fn recents_newest_first_without_duplicates() {
        let mut r = Recents::default();
        r.remember("/a", 1.0);
        r.remember("/b", 2.0);
        r.remember("/a", 3.0);
        let paths: Vec<_> = r.0.iter().map(|e| e.path.as_str()).collect();
        assert_eq!(paths, ["/a", "/b"]);
        assert_eq!(
            r.0[0],
            Recent {
                path: "/a".into(),
                last_opened: 3.0
            }
        );
    }

    #[test]
    fn recents_read_old_plain_paths_and_survive_corruption() {
        assert_eq!(
            Recents::parse(Some(json!(["/old"]))).0,
            vec![Recent {
                path: "/old".into(),
                last_opened: 0.0
            }]
        );
        assert_eq!(Recents::parse(None), Recents::default());
        assert_eq!(Recents::parse(Some(json!({"x": 1}))), Recents::default());
    }

    #[test]
    fn recents_add_several_keeping_order_and_forget() {
        let mut r = Recents::default();
        r.remember("/x", 1.0);
        r.remember_all(&["/p1".into(), "/p2".into()], 2.0);
        let paths = |r: &Recents| r.0.iter().map(|e| e.path.clone()).collect::<Vec<_>>();
        assert_eq!(paths(&r), ["/p1", "/p2", "/x"]);
        r.forget("/p1");
        assert_eq!(paths(&r), ["/p2", "/x"]);
    }

    #[test]
    fn folders_have_unique_trimmed_names() {
        let mut g = Groups::default();
        assert_eq!(g.create("  Thesis ", Some("t".into())), Some("t".into()));
        assert_eq!(
            g.groups,
            vec![Group {
                id: "t".into(),
                name: "Thesis".into()
            }]
        );
        assert_eq!(g.create("thesis", Some("x".into())), None);
        assert!(g.name_problem("", None).unwrap().contains("name"));
        assert!(
            g.name_problem(&"x".repeat(41), None)
                .unwrap()
                .contains("at most")
        );
        assert_eq!(g.name_problem("Thesis", Some("t")), None);
    }

    #[test]
    fn folders_rename_move_and_count() {
        let mut s = Groups::default();
        s.create("A", Some("a".into()));
        s.create("B", Some("b".into()));
        s.move_project("/p/one", Some("a"));
        s.move_project("/p/two", Some("a"));
        assert_eq!(s.count_in("a"), 2);
        s.move_project("/p/one", Some("b"));
        assert_eq!(s.count_in("a"), 1);
        assert!(!s.clone().rename("a", "B"));
        let mut r = s.clone();
        assert!(r.rename("a", "Courses"));
        assert_eq!(r.groups[0].name, "Courses");
        let mut m = s.clone();
        m.move_project("/p/two", None);
        assert_eq!(m.members, BTreeMap::from([("/p/one".into(), "b".into())]));
        // A folder that does not exist leaves the project unfiled.
        s.move_project("/p/two", Some("zzz"));
        assert!(!s.members.contains_key("/p/two"));
    }

    #[test]
    fn deleting_a_folder_unfiles_and_restoring_puts_it_back() {
        let mut s = Groups::default();
        s.create("A", Some("a".into()));
        s.create("B", Some("b".into()));
        s.move_project("/p/one", Some("a"));
        s.move_project("/p/two", Some("b"));
        let before = s.clone();
        s.delete("a");
        assert_eq!(s.groups.len(), 1);
        assert!(!s.members.contains_key("/p/one"));
        let mut restored = s.clone();
        restored.restore(&before, "a");
        assert_eq!(restored, before);
        // A folder already back, or one that never existed, changes nothing.
        let mut again = before.clone();
        again.restore(&before, "a");
        assert_eq!(again, before);
        let mut none = s.clone();
        none.restore(&before, "zzz");
        assert_eq!(none, s);
        // Changes made since the delete win over the old membership.
        let mut later = s.clone();
        later.move_project("/p/one", Some("b"));
        later.restore(&before, "a");
        assert_eq!(later.members.get("/p/one").map(String::as_str), Some("b"));
    }

    #[test]
    fn folders_filter_by_view_and_ignore_damaged_storage() {
        let mut s = Groups::default();
        s.create("A", Some("a".into()));
        s.move_project("/p/one", Some("a"));
        let in_view = |v: &View| {
            ["/p/one", "/p/two"]
                .into_iter()
                .filter(|p| s.in_view(p, v))
                .collect::<Vec<_>>()
        };
        assert_eq!(in_view(&View::Group("a".into())), ["/p/one"]);
        assert_eq!(in_view(&View::Root), ["/p/two"]);
        assert_eq!(Groups::parse(Some(serde_json::to_value(&s).unwrap())), s);
        assert_eq!(Groups::parse(Some(json!("x"))), Groups::default());
        let damaged = json!({ "groups": [{ "id": "a", "name": "A" }], "members": { "/p": "gone", "/q": "a" } });
        assert_eq!(
            Groups::parse(Some(damaged)).members,
            BTreeMap::from([("/q".into(), "a".into())])
        );
    }
}
