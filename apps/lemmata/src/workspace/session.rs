//! Where you left off, per project (session.ts): open tabs, the active file, its cursor line and
//! the PDF position. Stored like tauri-plugin-store did, in `sessions.json` keyed by project root,
//! so sessions from the Tauri app carry over.

use crate::store::Dirs;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Session {
    pub tabs: Vec<String>,
    pub active: Option<String>,
    pub line: usize,
    /// Folders collapsed in the file tree.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub collapsed: Option<Vec<String>>,
    /// Tabs that stay put at the left and can't be closed by accident.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pinned: Option<Vec<String>>,
    /// Folded sections per file, as the line numbers of their first lines.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub folds: Option<BTreeMap<String, Vec<usize>>>,
    /// `{ fit: "width" | "page" | number, scroll }`; kept as it was written.
    #[serde(default)]
    pub pdf: Value,
}

fn path(dirs: &Dirs) -> std::path::PathBuf {
    dirs.config.join("sessions.json")
}

fn read_all(dirs: &Dirs) -> serde_json::Map<String, Value> {
    let all = lemmata_env::fs::read(path(dirs))
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok());
    if all.is_none() {
        // The next save writes a new file: what is there stays as sessions.json.bak.
        crate::store::backup_unreadable(&path(dirs));
    }
    all.unwrap_or_default()
}

pub fn load(dirs: &Dirs, root: &str) -> Option<Session> {
    serde_json::from_value(read_all(dirs).get(root)?.clone()).ok()
}

/// Not remembering a session is harmless, so failures are ignored.
pub fn save(dirs: &Dirs, root: &str, session: &Session) {
    let mut all = read_all(dirs);
    all.insert(
        root.into(),
        serde_json::to_value(session).unwrap_or(Value::Null),
    );
    let p = path(dirs);
    if let Some(d) = p.parent() {
        let _ = std::fs::create_dir_all(d);
    }
    let _ = lemmata_env::fs::write_atomic(
        &p,
        &serde_json::to_vec_pretty(&Value::Object(all)).unwrap_or_default(),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_and_reads_the_svelte_format() {
        let dir = tempfile::tempdir().unwrap();
        let dirs = Dirs {
            config: dir.path().into(),
            data: dir.path().into(),
            cache: dir.path().into(),
        };
        std::fs::write(
            dir.path().join("sessions.json"),
            r#"{"/p": {"tabs": ["main.tex"], "active": "main.tex", "line": 7, "pdf": {"fit": "width", "scroll": 0}}}"#,
        )
        .unwrap();
        let s = load(&dirs, "/p").unwrap();
        assert_eq!(
            (s.tabs.as_slice(), s.active.as_deref(), s.line),
            (["main.tex".to_owned()].as_slice(), Some("main.tex"), 7)
        );
        let mut s2 = s.clone();
        s2.pinned = Some(vec!["main.tex".into()]);
        save(&dirs, "/q", &s2);
        assert_eq!(load(&dirs, "/q"), Some(s2));
        assert_eq!(load(&dirs, "/p"), Some(s), "other projects' sessions stay");
        assert_eq!(load(&dirs, "/none"), None);
    }

    #[test]
    fn an_unreadable_file_is_kept_aside_before_it_is_replaced() {
        let dir = tempfile::tempdir().unwrap();
        let dirs = Dirs {
            config: dir.path().into(),
            data: dir.path().into(),
            cache: dir.path().into(),
        };
        std::fs::write(dir.path().join("sessions.json"), r#"{"/a": {},}"#).unwrap();
        save(&dirs, "/b", &Session::default());
        assert_eq!(
            std::fs::read_to_string(dir.path().join("sessions.json.bak")).unwrap(),
            r#"{"/a": {},}"#
        );
    }
}
