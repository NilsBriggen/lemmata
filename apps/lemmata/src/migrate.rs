//! The Tauri app kept recent projects and library folders in its webview's localStorage, not in
//! files (settings and sessions it kept in files, which this app reads as they are). The first
//! start without recent.json and groups.json copies them over, once.
//!
//! WebKit (Linux, macOS) stores localStorage in SQLite, read through the system's libsqlite3:
//! WebKit itself needs it, so it is there wherever the Tauri app ran. WebView2 (Windows) uses
//! LevelDB.

use crate::store::{Dirs, Groups, Recents};
use std::path::{Path, PathBuf};

const RECENT: &str = "lemmata.recent";
const GROUPS: &str = "lemmata.groups";

/// Copies the Tauri app's lists when this app has none of its own yet.
pub fn run(dirs: &Dirs) {
    let (recent, groups) = (
        dirs.config.join("recent.json"),
        dirs.config.join("groups.json"),
    );
    if recent.exists() || groups.exists() {
        return;
    }
    let Some(found) = read(dirs) else { return };
    let value = |key| {
        found
            .iter()
            .find(|(k, _)| k == key)
            .and_then(|(_, v)| serde_json::from_str(v).ok())
    };
    let r = Recents::parse(value(RECENT));
    let g = Groups::parse(value(GROUPS));
    if !r.0.is_empty() {
        r.save(dirs);
    }
    if !g.groups.is_empty() {
        g.save(dirs);
    }
}

/// The `lemmata.*` keys of the newest localStorage that has them.
fn read(dirs: &Dirs) -> Option<Vec<(String, String)>> {
    #[cfg(unix)]
    {
        let mut roots = vec![dirs.data.clone()];
        // macOS keeps WKWebView data apart from the app's own folder.
        if cfg!(target_os = "macos")
            && let Some(home) = dirs::home_dir()
        {
            roots.push(home.join("Library/WebKit").join(crate::store::IDENTIFIER));
        }
        let mut files = Vec::new();
        for r in roots {
            find(&r, 6, &mut files);
        }
        files.sort_by_key(|f| std::cmp::Reverse(modified(f)));
        files
            .iter()
            .find_map(|f| sqlite::read(f).ok().filter(|kv| !kv.is_empty()))
    }
    #[cfg(windows)]
    {
        let _ = dirs;
        let dir = dirs::data_local_dir()?
            .join(crate::store::IDENTIFIER)
            .join("EBWebView/Default/Local Storage/leveldb");
        leveldb::read(&dir).ok().filter(|kv| !kv.is_empty())
    }
}

/// WebKit's localStorage databases under `dir`: `*.localstorage` (WebKitGTK) or
/// `localstorage.sqlite3` (newer WebKit, in salted folders per origin).
#[cfg(unix)]
fn find(dir: &Path, depth: usize, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for e in entries.flatten() {
        let p = e.path();
        let name = e.file_name().to_string_lossy().into_owned();
        if e.file_type().is_ok_and(|t| t.is_dir()) {
            if depth > 0 {
                find(&p, depth - 1, out);
            }
        } else if name.ends_with(".localstorage") || name == "localstorage.sqlite3" {
            out.push(p);
        }
    }
}

/// With its write-ahead log, where the newest writes can still be.
#[cfg(unix)]
fn modified(db: &Path) -> std::time::SystemTime {
    let m = |p: &Path| std::fs::metadata(p).and_then(|m| m.modified()).ok();
    let wal = PathBuf::from(format!("{}-wal", db.display()));
    m(db).max(m(&wal)).unwrap_or(std::time::UNIX_EPOCH)
}

/// localStorage values are UTF-16 in both browsers (WebKit and Chromium's usual form).
fn utf16(b: &[u8]) -> Option<String> {
    let (pairs, rest) = b.as_chunks::<2>();
    let units: Vec<u16> = pairs.iter().map(|&p| u16::from_le_bytes(p)).collect();
    rest.is_empty()
        .then(|| String::from_utf16(&units).ok())
        .flatten()
}

#[cfg(unix)]
mod sqlite {
    use std::ffi::{CStr, CString, c_char, c_int, c_void};
    use std::path::Path;

    type Db = *mut c_void;
    type Stmt = *mut c_void;

    /// `key`, `value` of the `lemmata.*` rows in WebKit's `ItemTable`.
    pub fn read(db: &Path) -> Result<Vec<(String, String)>, String> {
        // A copy, so the Tauri app's files are never touched; SQLite folds the log into it.
        let tmp = std::env::temp_dir().join(format!("lemmata-migrate-{}", std::process::id()));
        std::fs::create_dir_all(&tmp).map_err(|e| e.to_string())?;
        let copy = tmp.join("localstorage");
        let result = (|| {
            std::fs::copy(db, &copy).map_err(|e| e.to_string())?;
            let _ = std::fs::copy(
                format!("{}-wal", db.display()),
                tmp.join("localstorage-wal"),
            );
            query(&copy)
        })();
        let _ = std::fs::remove_dir_all(&tmp);
        result
    }

    fn library() -> Result<libloading::Library, String> {
        let names: &[&str] = if cfg!(target_os = "macos") {
            &["/usr/lib/libsqlite3.dylib", "libsqlite3.dylib"]
        } else {
            &["libsqlite3.so.0", "libsqlite3.so"]
        };
        names
            .iter()
            .find_map(|n| unsafe { libloading::Library::new(n) }.ok())
            .ok_or_else(|| "no libsqlite3".into())
    }

    fn query(path: &Path) -> Result<Vec<(String, String)>, String> {
        let lib = library()?;
        // SAFETY: the signatures are SQLite's C API; every pointer handed back is checked
        // before use, and the statement and the connection are closed on every path.
        unsafe {
            let sym = |n: &[u8]| {
                lib.get::<*const c_void>(n)
                    .map(|s| *s)
                    .map_err(|e| e.to_string())
            };
            let open: extern "C" fn(*const c_char, *mut Db, c_int, *const c_char) -> c_int =
                std::mem::transmute(sym(b"sqlite3_open_v2\0")?);
            let prepare: extern "C" fn(
                Db,
                *const c_char,
                c_int,
                *mut Stmt,
                *mut *const c_char,
            ) -> c_int = std::mem::transmute(sym(b"sqlite3_prepare_v2\0")?);
            let step: extern "C" fn(Stmt) -> c_int = std::mem::transmute(sym(b"sqlite3_step\0")?);
            let text: extern "C" fn(Stmt, c_int) -> *const c_char =
                std::mem::transmute(sym(b"sqlite3_column_text\0")?);
            let blob: extern "C" fn(Stmt, c_int) -> *const u8 =
                std::mem::transmute(sym(b"sqlite3_column_blob\0")?);
            let bytes: extern "C" fn(Stmt, c_int) -> c_int =
                std::mem::transmute(sym(b"sqlite3_column_bytes\0")?);
            let finalize: extern "C" fn(Stmt) -> c_int =
                std::mem::transmute(sym(b"sqlite3_finalize\0")?);
            let close: extern "C" fn(Db) -> c_int =
                std::mem::transmute(sym(b"sqlite3_close_v2\0")?);

            let name =
                CString::new(path.to_string_lossy().as_bytes()).map_err(|e| e.to_string())?;
            let mut db: Db = std::ptr::null_mut();
            // SQLITE_OPEN_READWRITE: a database with a log can't be read otherwise. It's the copy.
            let rc = open(name.as_ptr(), &mut db, 0x2, std::ptr::null());
            if rc != 0 {
                close(db);
                return Err(format!("sqlite3_open_v2: {rc}"));
            }
            let sql = c"SELECT key, value FROM ItemTable WHERE key LIKE 'lemmata.%'";
            let mut stmt: Stmt = std::ptr::null_mut();
            if prepare(db, sql.as_ptr(), -1, &mut stmt, std::ptr::null_mut()) != 0 || stmt.is_null()
            {
                close(db);
                return Err("not a localStorage database".into());
            }
            let mut out = Vec::new();
            const ROW: c_int = 100;
            while step(stmt) == ROW {
                let k = text(stmt, 0);
                let (v, n) = (blob(stmt, 1), bytes(stmt, 1));
                if k.is_null() || v.is_null() || n < 0 {
                    continue;
                }
                let key = CStr::from_ptr(k).to_string_lossy().into_owned();
                let raw = std::slice::from_raw_parts(v, n as usize);
                // UTF-16 as WebKit writes it; plain UTF-8 from older versions.
                if let Some(value) = super::utf16(raw)
                    .filter(|s| s.starts_with(['[', '{']))
                    .or_else(|| String::from_utf8(raw.to_vec()).ok())
                {
                    out.push((key, value));
                }
            }
            finalize(stmt);
            close(db);
            Ok(out)
        }
    }
}

#[cfg(any(windows, test))]
mod leveldb {
    use rusty_leveldb::{DB, LdbIterator, Options};
    use std::path::Path;

    /// Chromium's localStorage rows: `_<origin>\0\x01<key>` → a type byte and the text
    /// (0: UTF-16, 1: Latin-1). Only the `lemmata.*` keys, of any origin.
    #[cfg_attr(not(windows), allow(dead_code))]
    pub fn read(dir: &Path) -> Result<Vec<(String, String)>, String> {
        // A copy: LevelDB locks its folder, and the Tauri app's must stay as it is.
        let tmp = std::env::temp_dir().join(format!("lemmata-migrate-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).map_err(|e| e.to_string())?;
        let result = (|| {
            for e in std::fs::read_dir(dir).map_err(|e| e.to_string())?.flatten() {
                if e.file_name() != "LOCK" {
                    std::fs::copy(e.path(), tmp.join(e.file_name())).map_err(|e| e.to_string())?;
                }
            }
            rows(&tmp)
        })();
        let _ = std::fs::remove_dir_all(&tmp);
        result
    }

    pub(super) fn rows(dir: &Path) -> Result<Vec<(String, String)>, String> {
        let opt = Options {
            create_if_missing: false,
            ..Options::default()
        };
        let mut db = DB::open(dir, opt).map_err(|e| e.to_string())?;
        let mut it = db.new_iter().map_err(|e| e.to_string())?;
        let mut out = Vec::new();
        while let Some((k, v)) = it.next() {
            let Some(at) = k.windows(2).position(|w| w == b"\0\x01") else {
                continue;
            };
            let Ok(key) = std::str::from_utf8(&k[at + 2..]) else {
                continue;
            };
            if !k.starts_with(b"_") || !key.starts_with("lemmata.") {
                continue;
            }
            let value = match v.split_first() {
                Some((0, rest)) => super::utf16(rest),
                Some((1, rest)) => Some(rest.iter().map(|&b| b as char).collect()),
                _ => None,
            };
            if let Some(value) = value {
                out.push((key.to_owned(), value));
            }
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dirs(root: &Path) -> Dirs {
        Dirs {
            config: root.join("config"),
            data: root.join("data"),
            cache: root.join("cache"),
        }
    }

    #[cfg(unix)]
    #[test]
    fn the_tauri_apps_lists_are_copied_once() {
        let tmp = tempfile::tempdir().unwrap();
        let d = dirs(tmp.path());
        let ls = d.data.join("localstorage");
        std::fs::create_dir_all(&ls).unwrap();
        let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/tauri-localstorage/tauri_localhost_0.localstorage");
        std::fs::copy(fixture, ls.join("tauri_localhost_0.localstorage")).unwrap();
        run(&d);
        let r = Recents::load(&d);
        let paths: Vec<_> = r.0.iter().map(|e| e.path.as_str()).collect();
        assert_eq!(
            paths,
            [
                "/home/ada/thesis",
                "/home/ada/notes",
                "/home/ada/old-plain-path"
            ]
        );
        assert_eq!(r.0[0].last_opened, 1_760_000_000_000.0);
        let g = Groups::load(&d);
        assert_eq!(g.groups[0].name, "Course");
        assert_eq!(g.members["/home/ada/notes"], "g1");
        // Once: this app's own lists win from then on.
        Recents::default().save(&d);
        run(&d);
        assert!(Recents::load(&d).0.is_empty());
    }

    /// Reads this computer's Tauri data (a copy of it), by hand:
    /// `cargo test -p lemmata-app -- --ignored --nocapture this_computers`.
    #[test]
    #[ignore]
    fn this_computers_tauri_lists() {
        let found = read(&Dirs::system()).expect("the Tauri app's localStorage");
        for (k, v) in &found {
            println!("{k}: {} bytes", v.len());
        }
        let recent = found
            .iter()
            .find(|(k, _)| k == RECENT)
            .expect("recent projects");
        let r = Recents::parse(serde_json::from_str(&recent.1).ok());
        println!(
            "{} recent projects, first {:?}",
            r.0.len(),
            r.0.first().map(|e| &e.path)
        );
        assert!(!r.0.is_empty());
    }

    #[test]
    fn nothing_to_copy_writes_nothing() {
        let tmp = tempfile::tempdir().unwrap();
        let d = dirs(tmp.path());
        run(&d);
        assert!(!d.config.join("recent.json").exists());
    }

    #[test]
    fn chromium_rows_are_read_by_key_and_encoding() {
        let tmp = tempfile::tempdir().unwrap();
        let mut db =
            rusty_leveldb::DB::open(tmp.path(), rusty_leveldb::Options::default()).unwrap();
        let utf16 = |s: &str| {
            let mut v = vec![0u8];
            v.extend(s.encode_utf16().flat_map(u16::to_le_bytes));
            v
        };
        let mut latin1 = vec![1u8];
        latin1.extend(br#"{"groups":[],"members":{}}"#);
        db.put(
            b"_http://tauri.localhost\0\x01lemmata.recent",
            &utf16(r#"[{"path":"C:\\Users\\ada\\thèse","lastOpened":1}]"#),
        )
        .unwrap();
        db.put(b"_http://tauri.localhost\0\x01lemmata.groups", &latin1)
            .unwrap();
        db.put(b"_http://tauri.localhost\0\x01other", &utf16("x"))
            .unwrap();
        db.put(b"META:http://tauri.localhost", b"meta").unwrap();
        db.flush().unwrap();
        drop(db);
        let mut rows = leveldb::rows(tmp.path()).unwrap();
        rows.sort();
        assert_eq!(rows.len(), 2, "{rows:?}");
        assert_eq!(rows[1].0, RECENT);
        assert!(rows[1].1.contains("thèse"));
        assert_eq!(rows[0].1, r#"{"groups":[],"members":{}}"#);
    }
}
