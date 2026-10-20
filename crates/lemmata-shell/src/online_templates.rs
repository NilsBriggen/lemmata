//! Templates from the templates repository, beyond the few the app ships with.
//! `refresh` reads the repository's index.json and keeps a copy of every
//! template in the cache folder, laid out like the bundled ones (template.toml,
//! the files, preview.png), so listing and creating projects work the same and
//! templates seen once stay available offline. Every file is checked against
//! the SHA-256 in the index before it is kept.

use serde::Deserialize;
use std::collections::HashSet;
use std::path::{Component, Path, PathBuf};
use std::time::Duration;

/// Where index.json and templates/<id>/<file> are served from.
// shortcut: the repository's final home (its own organization) isn't decided; change it here when it moves.
const BASE: &str = "https://raw.githubusercontent.com/NilsBriggen/lemmata-templates/main/";
const MAX_FILE: u64 = 5 * 1024 * 1024;
const META: &str = "template.toml";
const PREVIEW: &str = "preview.png";

#[derive(Deserialize)]
struct Index {
    version: u32,
    templates: Vec<Entry>,
}

#[derive(Deserialize)]
struct Entry {
    id: String,
    name: String,
    description: String,
    category: String,
    #[serde(default)]
    tags: Vec<String>,
    #[serde(default)]
    order: u32,
    preview: File,
    files: Vec<File>,
}

#[derive(Deserialize)]
struct File {
    path: String,
    size: u64,
    sha256: String,
}

/// A template id becomes a folder name: lowercase letters, digits and dashes only.
fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 64
        && id
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

/// A file path from the index, as a relative path that stays inside the template folder.
fn safe_path(p: &str) -> Option<PathBuf> {
    if p.is_empty() || p.contains('\\') || p.contains(':') {
        return None;
    }
    let path = Path::new(p);
    path.components()
        .all(|c| matches!(c, Component::Normal(s) if !s.to_string_lossy().starts_with('.')))
        .then(|| path.to_path_buf())
}

fn parse(json: &[u8]) -> Result<Vec<Entry>, String> {
    let index: Index =
        serde_json::from_slice(json).map_err(|e| format!("The template list is damaged: {e}"))?;
    if index.version != 1 {
        return Err("The template list needs a newer Lemmata. Update the app to see it.".into());
    }
    for t in &index.templates {
        let bad = !valid_id(&t.id)
            || t.files.iter().any(|f| {
                safe_path(&f.path).is_none()
                    || f.path == META
                    || f.path == PREVIEW
                    || f.size > MAX_FILE
                    || !is_sha256(&f.sha256)
            })
            || t.preview.path != PREVIEW
            || t.preview.size > MAX_FILE
            || !is_sha256(&t.preview.sha256);
        if bad {
            return Err(format!(
                "The template list has an invalid entry ({}).",
                t.id
            ));
        }
    }
    Ok(index.templates)
}

fn is_sha256(s: &str) -> bool {
    s.len() == 64 && s.bytes().all(|b| b.is_ascii_hexdigit())
}

fn sha256(bytes: &[u8]) -> String {
    ring::digest::digest(&ring::digest::SHA256, bytes)
        .as_ref()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// Whether `path` already holds exactly the file the index describes.
fn have(path: &Path, f: &File) -> bool {
    std::fs::metadata(path).is_ok_and(|m| m.len() == f.size)
        && std::fs::read(path).is_ok_and(|b| sha256(&b) == f.sha256)
}

/// The files of `entries` the cache doesn't have yet: (template id, file).
fn missing<'a>(cache: &Path, entries: &'a [Entry]) -> Vec<(&'a str, &'a File)> {
    entries
        .iter()
        .flat_map(|t| {
            let dir = cache.join(&t.id);
            t.files
                .iter()
                .chain([&t.preview])
                .filter(move |f| !have(&dir.join(&f.path), f))
                .map(move |f| (t.id.as_str(), f))
        })
        .collect()
}

/// Writes each template's template.toml, and removes templates and files the index no longer lists.
fn tidy(cache: &Path, entries: &[Entry]) -> std::io::Result<()> {
    let ids: HashSet<&str> = entries.iter().map(|t| t.id.as_str()).collect();
    for e in std::fs::read_dir(cache)?.flatten() {
        let name = e.file_name().to_string_lossy().into_owned();
        if e.path().is_dir() && !ids.contains(name.as_str()) {
            std::fs::remove_dir_all(e.path())?;
        }
    }
    for t in entries {
        let dir = cache.join(&t.id);
        let keep: HashSet<PathBuf> = t
            .files
            .iter()
            .map(|f| PathBuf::from(&f.path))
            .chain([PathBuf::from(META), PathBuf::from(PREVIEW)])
            .collect();
        remove_unlisted(&dir, &dir, &keep)?;
        let meta = toml::to_string(&crate::projects::TemplateMeta {
            name: t.name.clone(),
            description: t.description.clone(),
            order: t.order,
            category: Some(t.category.clone()),
            tags: t.tags.clone(),
        })
        .map_err(std::io::Error::other)?;
        std::fs::write(dir.join(META), meta)?;
    }
    Ok(())
}

fn remove_unlisted(root: &Path, dir: &Path, keep: &HashSet<PathBuf>) -> std::io::Result<()> {
    for e in std::fs::read_dir(dir)?.flatten() {
        let path = e.path();
        if path.is_dir() {
            remove_unlisted(root, &path, keep)?;
            let _ = std::fs::remove_dir(&path); // only if now empty
        } else if !keep.contains(path.strip_prefix(root).unwrap_or(&path)) {
            std::fs::remove_file(&path)?;
        }
    }
    Ok(())
}

async fn fetch(client: &reqwest::Client, url: &str) -> Result<Vec<u8>, String> {
    let offline = |e: reqwest::Error| {
        format!("Couldn't reach the template collection ({e}). Check your internet connection.")
    };
    let res = client.get(url).send().await.map_err(offline)?;
    if !res.status().is_success() {
        return Err(format!(
            "The template collection answered {}.",
            res.status()
        ));
    }
    Ok(res.bytes().await.map_err(offline)?.to_vec())
}

/// Brings the cache up to date with the template collection. Returns how many templates it has.
/// Files that fail their check are not kept; a template then stays as it was, or absent.
pub async fn refresh(cache: &Path) -> Result<usize, String> {
    static PROVIDER: std::sync::Once = std::sync::Once::new();
    PROVIDER.call_once(|| {
        let _ = rustls::crypto::ring::default_provider().install_default();
    });
    let client = reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(120))
        .user_agent(concat!("Lemmata/", env!("CARGO_PKG_VERSION")))
        .build()
        .map_err(|e| e.to_string())?;
    // Debug builds can point elsewhere (a local copy of the repository) to test changes before they're published.
    #[cfg(debug_assertions)]
    let base = std::env::var("LEMMATA_TEMPLATES_URL").unwrap_or_else(|_| BASE.to_string());
    #[cfg(not(debug_assertions))]
    let base = BASE.to_string();
    let entries = parse(&fetch(&client, &format!("{base}index.json")).await?)?;
    let save = |e: std::io::Error| format!("Couldn't save the templates: {e}");
    std::fs::create_dir_all(cache).map_err(save)?;
    // A few downloads at a time: one after another, a first visit takes half a minute.
    const PARALLEL: usize = 8;
    let mut todo = missing(cache, &entries)
        .into_iter()
        .map(|(id, f)| (id.to_string(), f.path.clone(), f.size, f.sha256.clone()))
        .collect::<Vec<_>>()
        .into_iter();
    let mut running = tokio::task::JoinSet::new();
    let start = |running: &mut tokio::task::JoinSet<Result<(), String>>,
                 (id, path, size, hash): (String, String, u64, String)| {
        let (client, url, target) = (
            client.clone(),
            format!("{base}templates/{id}/{path}"),
            cache.join(&id).join(&path),
        );
        running.spawn(async move {
            let bytes = fetch(&client, &url).await?;
            if bytes.len() as u64 != size || sha256(&bytes) != hash {
                return Err(format!(
                    "A file of the {id} template didn't match the template list, so it wasn't kept. Try again later."
                ));
            }
            let save = |e: std::io::Error| format!("Couldn't save the templates: {e}");
            if let Some(parent) = target.parent() {
                std::fs::create_dir_all(parent).map_err(save)?;
            }
            // The whole name: main.tex and main.bib must not share a temp file.
            let tmp = target.with_file_name(format!("{}.part", target.file_name().unwrap_or_default().to_string_lossy()));
            std::fs::write(&tmp, &bytes).map_err(save)?;
            std::fs::rename(&tmp, &target).map_err(save)
        });
    };
    for job in todo.by_ref().take(PARALLEL) {
        start(&mut running, job);
    }
    while let Some(done) = running.join_next().await {
        done.map_err(|e| e.to_string())??;
        if let Some(job) = todo.next() {
            start(&mut running, job);
        }
    }
    tidy(cache, &entries).map_err(save)?;
    Ok(entries.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn index(files: &str) -> String {
        format!(
            r#"{{"version":1,"templates":[{{"id":"ieee","name":"IEEE","description":"d","category":"Papers",
            "preview":{{"path":"preview.png","size":3,"sha256":"{p}"}},"files":[{files}]}}]}}"#,
            p = sha256(b"png")
        )
    }

    fn file(path: &str, body: &[u8]) -> String {
        format!(
            r#"{{"path":"{path}","size":{},"sha256":"{}"}}"#,
            body.len(),
            sha256(body)
        )
    }

    #[test]
    fn rejects_paths_that_leave_the_template_folder() {
        for bad in [
            "../x.tex",
            "/etc/passwd",
            "a/../../x",
            "C:x.tex",
            "a\\\\b.tex", // JSON for a\b.tex
            ".git/config",
            "template.toml",
        ] {
            assert!(parse(index(&file(bad, b"x")).as_bytes()).is_err(), "{bad}");
        }
        assert!(parse(index(&file("chapters/a.tex", b"x")).as_bytes()).is_ok());
        assert!(parse(br#"{"version":2,"templates":[]}"#).is_err());
    }

    #[test]
    fn fetches_only_what_changed_and_tidies_the_rest() {
        let d = tempfile::tempdir().unwrap();
        let cache = d.path();
        let entries = parse(
            index(&[file("main.tex", b"new"), file("refs.bib", b"bib")].join(",")).as_bytes(),
        )
        .unwrap();
        std::fs::create_dir_all(cache.join("ieee")).unwrap();
        std::fs::write(cache.join("ieee/main.tex"), "old").unwrap();
        std::fs::write(cache.join("ieee/refs.bib"), "bib").unwrap();
        std::fs::write(cache.join("ieee/stale.tex"), "x").unwrap();
        std::fs::create_dir_all(cache.join("gone")).unwrap();
        let paths: Vec<&str> = missing(cache, &entries)
            .iter()
            .map(|(_, f)| f.path.as_str())
            .collect();
        assert_eq!(paths, ["main.tex", "preview.png"]);
        tidy(cache, &entries).unwrap();
        assert!(!cache.join("gone").exists());
        assert!(!cache.join("ieee/stale.tex").exists());
        assert!(cache.join("ieee/refs.bib").exists());
        let meta = std::fs::read_to_string(cache.join("ieee/template.toml")).unwrap();
        assert!(
            meta.contains("name = \"IEEE\"") && meta.contains("category = \"Papers\""),
            "{meta}"
        );
    }
}
