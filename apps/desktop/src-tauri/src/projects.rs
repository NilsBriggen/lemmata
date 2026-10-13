//! Creating projects: from a bundled template, or from an Overleaf zip.

use serde::{Deserialize, Serialize};
use std::fs;
use std::io::{self, Read, Seek};
use std::path::{Path, PathBuf};

#[derive(Debug, Serialize)]
pub struct Template {
    pub id: String,
    pub name: String,
    pub description: String,
}

#[derive(Deserialize)]
struct TemplateMeta {
    name: String,
    description: String,
    #[serde(default)]
    order: u32,
}

/// Metadata file inside each template folder; not copied into projects.
const META: &str = "template.toml";
pub const EXAMPLE: &str = "example";

/// Copies the example document into `dir` (the app's own data folder), or
/// leaves an existing copy alone so the user's experiments survive.
pub fn ensure_example(templates: &Path, dir: &Path) -> Result<PathBuf, String> {
    if dir.join("main.tex").is_file() {
        return Ok(dir.to_path_buf());
    }
    copy_dir(&templates.join(EXAMPLE), dir)
        .map_err(|e| format!("Could not create the example: {e}"))?;
    let _ = fs::remove_file(dir.join(META));
    Ok(dir.to_path_buf())
}

pub fn list_templates(dir: &Path) -> Vec<Template> {
    let mut found: Vec<(u32, Template)> = fs::read_dir(dir)
        .into_iter()
        .flatten()
        .filter_map(|e| e.ok())
        // The example document is offered on its own, not as a template.
        .filter(|e| e.file_name() != EXAMPLE)
        .filter_map(|e| {
            let text = fs::read_to_string(e.path().join(META)).ok()?;
            let meta: TemplateMeta = toml::from_str(&text).ok()?;
            let id = e.file_name().to_string_lossy().into_owned();
            Some((
                meta.order,
                Template {
                    id,
                    name: meta.name,
                    description: meta.description,
                },
            ))
        })
        .collect();
    found.sort_by_key(|(order, t)| (*order, t.name.clone()));
    found.into_iter().map(|(_, t)| t).collect()
}

/// A folder name the user typed: no separators, not empty, not "." or "..".
fn check_name(name: &str) -> Result<&str, String> {
    let name = name.trim();
    if name.is_empty() || name == "." || name == ".." || name.contains(['/', '\\']) {
        return Err("Choose a folder name without slashes.".into());
    }
    Ok(name)
}

fn fresh_target(parent: &Path, name: &str) -> Result<PathBuf, String> {
    let target = parent.join(check_name(name)?);
    if target.exists() {
        return Err(format!(
            "{} already exists. Choose another name.",
            target.display()
        ));
    }
    Ok(target)
}

pub fn create_from_template(
    templates: &Path,
    id: &str,
    parent: &Path,
    name: &str,
) -> Result<PathBuf, String> {
    let source = templates.join(check_name(id)?);
    if !source.join(META).is_file() {
        return Err(format!("There is no template called {id}."));
    }
    let target = fresh_target(parent, name)?;
    copy_dir(&source, &target).map_err(|e| format!("Could not create the project: {e}"))?;
    let _ = fs::remove_file(target.join(META));
    Ok(target)
}

fn copy_dir(from: &Path, to: &Path) -> io::Result<()> {
    fs::create_dir_all(to)?;
    for entry in fs::read_dir(from)? {
        let entry = entry?;
        let dest = to.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_dir(&entry.path(), &dest)?;
        } else {
            fs::copy(entry.path(), dest)?;
        }
    }
    Ok(())
}

#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct ZipInfo {
    /// Overleaf's "download all" format: a zip of project zips.
    pub bulk: bool,
    /// Project names: the inner zips (bulk) or the zip itself.
    pub projects: Vec<String>,
}

fn open_zip(zip_path: &Path) -> Result<zip::ZipArchive<fs::File>, String> {
    let file = fs::File::open(zip_path)
        .map_err(|e| format!("Could not open {}: {e}", zip_path.display()))?;
    zip::ZipArchive::new(file).map_err(|e| format!("This isn't a valid zip file: {e}"))
}

fn entry_names<R: Read + Seek>(archive: &mut zip::ZipArchive<R>) -> Vec<String> {
    let mut names = Vec::new();
    for i in 0..archive.len() {
        let Ok(e) = archive.by_index(i) else { continue };
        if e.is_dir() {
            continue;
        }
        if let Ok(n) = e.name()
            && !n.starts_with("__MACOSX/")
        {
            names.push(n.to_string());
        }
    }
    names
}

fn stem(name: &str) -> String {
    let base = name.rsplit('/').next().unwrap_or(name);
    base.strip_suffix(".zip")
        .or_else(|| base.strip_suffix(".ZIP"))
        .unwrap_or(base)
        .to_string()
}

/// What a zip contains, so the import dialog can show it first.
pub fn inspect_zip(zip_path: &Path) -> Result<ZipInfo, String> {
    let mut archive = open_zip(zip_path)?;
    let names = entry_names(&mut archive);
    let bulk = !names.is_empty() && names.iter().all(|n| n.to_lowercase().ends_with(".zip"));
    let projects = if bulk {
        names.iter().map(|n| stem(n)).collect()
    } else {
        vec![stem(
            &zip_path.file_name().unwrap_or_default().to_string_lossy(),
        )]
    };
    Ok(ZipInfo { bulk, projects })
}

/// Imports an Overleaf download. A single project ("Download → Source")
/// goes into `parent/<name>`. A bulk download (a zip of project zips)
/// puts each project into its own folder under `parent`, named after it.
pub fn import_zip(zip_path: &Path, parent: &Path, name: &str) -> Result<Vec<PathBuf>, String> {
    let info = inspect_zip(zip_path)?;
    let mut archive = open_zip(zip_path)?;
    if !info.bulk {
        let target = fresh_target(parent, name)?;
        extract_project(&mut archive, &target)?;
        return Ok(vec![target]);
    }
    let mut out = Vec::new();
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i).map_err(|e| e.to_string())?;
        let entry_name = entry.name().map(|n| n.to_string()).unwrap_or_default();
        if entry.is_dir()
            || entry_name.starts_with("__MACOSX/")
            || !entry_name.to_lowercase().ends_with(".zip")
        {
            continue;
        }
        let mut bytes = Vec::new();
        entry.read_to_end(&mut bytes).map_err(|e| e.to_string())?;
        let mut inner = zip::ZipArchive::new(io::Cursor::new(bytes))
            .map_err(|e| format!("{entry_name} isn't a valid zip file: {e}"))?;
        let target = available_target(parent, &stem(&entry_name))?;
        extract_project(&mut inner, &target).map_err(|e| format!("{}: {e}", stem(&entry_name)))?;
        out.push(target);
    }
    Ok(out)
}

/// `parent/name`, or `parent/name (2)` etc. if taken.
fn available_target(parent: &Path, name: &str) -> Result<PathBuf, String> {
    let name = check_name(name)?;
    let mut n = 1;
    loop {
        let candidate = if n == 1 {
            parent.join(name)
        } else {
            parent.join(format!("{name} ({n})"))
        };
        if !candidate.exists() {
            return Ok(candidate);
        }
        n += 1;
    }
}

/// Extracts one Overleaf project into `target`. If everything sits in one
/// top-level folder, that folder's contents become the project. Fails (and
/// removes `target`) when the zip has no .tex file.
fn extract_project<R: Read + Seek>(
    archive: &mut zip::ZipArchive<R>,
    target: &Path,
) -> Result<(), String> {
    // Entry paths are validated by enclosed_name, which rejects absolute
    // paths and `..` so nothing is written outside the target folder.
    let mut paths = Vec::with_capacity(archive.len());
    for i in 0..archive.len() {
        let entry = archive.by_index(i).map_err(|e| e.to_string())?;
        let path = entry.enclosed_name().ok_or_else(|| {
            let shown = entry.name().map(|n| n.to_string()).unwrap_or_default();
            format!("The zip contains an unsafe path: {shown}")
        })?;
        paths.push(path);
    }
    if !paths
        .iter()
        .any(|p| p.extension().is_some_and(|e| e.eq_ignore_ascii_case("tex")))
    {
        return Err("This zip has no .tex file. In Overleaf, use Menu → Download → Source.".into());
    }
    let strip = common_top_folder(&paths);

    let result = (|| {
        for (i, path) in paths.iter().enumerate() {
            let mut entry = archive.by_index(i).map_err(|e| e.to_string())?;
            let rel = match &strip {
                Some(prefix) => path.strip_prefix(prefix).unwrap_or(path),
                None => path.as_path(),
            };
            if rel.as_os_str().is_empty() || rel.components().any(|c| c.as_os_str() == "__MACOSX") {
                continue;
            }
            let dest = target.join(rel);
            if entry.is_dir() {
                fs::create_dir_all(&dest).map_err(|e| e.to_string())?;
            } else {
                if let Some(parent) = dest.parent() {
                    fs::create_dir_all(parent).map_err(|e| e.to_string())?;
                }
                let mut out = fs::File::create(&dest).map_err(|e| e.to_string())?;
                io::copy(&mut entry, &mut out).map_err(|e| e.to_string())?;
            }
        }
        fs::create_dir_all(target).map_err(|e| e.to_string())
    })();
    if result.is_err() {
        let _ = fs::remove_dir_all(target);
    }
    result
}

fn common_top_folder(paths: &[PathBuf]) -> Option<PathBuf> {
    let firsts: Vec<_> = paths
        .iter()
        .filter(|p| !p.starts_with("__MACOSX"))
        .map(|p| p.components().next().map(|c| c.as_os_str().to_owned()))
        .collect();
    let first = firsts.first()?.clone()?;
    let all_same = firsts.iter().all(|f| f.as_ref() == Some(&first));
    // A lone file at the top isn't a folder to strip.
    let is_folder = paths
        .iter()
        .any(|p| p.starts_with(&first) && p.components().count() > 1);
    (all_same && is_folder).then(|| PathBuf::from(first))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn make_zip(path: &Path, files: &[(&str, &str)]) {
        let mut w = zip::ZipWriter::new(fs::File::create(path).unwrap());
        for (name, body) in files {
            w.start_file(*name, zip::write::SimpleFileOptions::default())
                .unwrap();
            w.write_all(body.as_bytes()).unwrap();
        }
        w.finish().unwrap();
    }

    #[test]
    fn imports_flat_overleaf_zip() {
        let d = tempfile::tempdir().unwrap();
        let z = d.path().join("paper.zip");
        make_zip(
            &z,
            &[
                ("main.tex", "\\documentclass{article}"),
                ("figures/a.png", "png"),
            ],
        );
        let out = import_zip(&z, d.path(), "paper").unwrap().remove(0);
        assert!(out.join("main.tex").is_file());
        assert!(out.join("figures/a.png").is_file());
    }

    #[test]
    fn strips_single_top_folder_and_macos_junk() {
        let d = tempfile::tempdir().unwrap();
        let z = d.path().join("x.zip");
        make_zip(
            &z,
            &[
                ("Thesis/main.tex", "x"),
                ("Thesis/ch/1.tex", "y"),
                ("__MACOSX/Thesis/._main.tex", "junk"),
            ],
        );
        let out = import_zip(&z, d.path(), "thesis").unwrap().remove(0);
        assert!(out.join("main.tex").is_file());
        assert!(out.join("ch/1.tex").is_file());
        assert!(!out.join("__MACOSX").exists());
    }

    #[test]
    fn refuses_zip_slip() {
        let d = tempfile::tempdir().unwrap();
        let z = d.path().join("evil.zip");
        make_zip(&z, &[("main.tex", "x"), ("../evil.tex", "x")]);
        assert!(import_zip(&z, d.path(), "p").is_err());
        assert!(!d.path().join("evil.tex").exists());
    }

    #[test]
    fn creates_from_template_without_metadata() {
        let d = tempfile::tempdir().unwrap();
        let t = d.path().join("templates/article");
        fs::create_dir_all(&t).unwrap();
        fs::write(t.join(META), "name = \"Article\"\ndescription = \"x\"\n").unwrap();
        fs::write(t.join("main.tex"), "\\documentclass{article}").unwrap();
        assert_eq!(
            list_templates(&d.path().join("templates"))[0].name,
            "Article"
        );

        let p = create_from_template(&d.path().join("templates"), "article", d.path(), "My Paper")
            .unwrap();
        assert!(p.join("main.tex").is_file());
        assert!(!p.join(META).exists());
        assert!(
            create_from_template(&d.path().join("templates"), "article", d.path(), "My Paper")
                .is_err()
        );
        assert!(create_from_template(&d.path().join("templates"), "../x", d.path(), "q").is_err());
    }

    fn zip_bytes(files: &[(&str, &str)]) -> Vec<u8> {
        let mut w = zip::ZipWriter::new(io::Cursor::new(Vec::new()));
        for (name, body) in files {
            w.start_file(*name, zip::write::SimpleFileOptions::default())
                .unwrap();
            w.write_all(body.as_bytes()).unwrap();
        }
        w.finish().unwrap().into_inner()
    }

    #[test]
    fn imports_overleaf_bulk_download() {
        let d = tempfile::tempdir().unwrap();
        let a = zip_bytes(&[("main.tex", "\\documentclass{article}")]);
        let b = zip_bytes(&[
            ("main.tex", "\\documentclass{article}"),
            ("images/plot.png", "png"),
        ]);
        let z = d.path().join("Overleaf Projects (2 items).zip");
        let mut w = zip::ZipWriter::new(fs::File::create(&z).unwrap());
        for (name, bytes) in [("Homework.zip", &a), ("CA_Assignement_4.zip", &b)] {
            w.start_file(name, zip::write::SimpleFileOptions::default())
                .unwrap();
            w.write_all(bytes).unwrap();
        }
        w.finish().unwrap();

        let info = inspect_zip(&z).unwrap();
        assert!(info.bulk);
        assert_eq!(info.projects, ["Homework", "CA_Assignement_4"]);

        // An existing folder with the same name gets a suffix, not overwritten.
        fs::create_dir(d.path().join("Homework")).unwrap();
        let out = import_zip(&z, d.path(), "ignored").unwrap();
        assert_eq!(
            out,
            [
                d.path().join("Homework (2)"),
                d.path().join("CA_Assignement_4")
            ]
        );
        assert!(out[1].join("images/plot.png").is_file());
        assert!(lemmata_project::Project::open(&out[0]).is_ok());
    }

    #[test]
    fn zip_without_tex_is_refused_and_cleaned_up() {
        let d = tempfile::tempdir().unwrap();
        let z = d.path().join("photos.zip");
        make_zip(&z, &[("a.png", "png")]);
        let err = import_zip(&z, d.path(), "photos").unwrap_err();
        assert!(err.contains("no .tex file"), "{err}");
        assert!(!d.path().join("photos").exists());
    }

    /// The user's real bulk download, when present on this machine.
    #[test]
    fn real_overleaf_bulk_download_if_present() {
        let Some(home) = std::env::var_os("HOME") else {
            return;
        };
        let real = PathBuf::from(home).join("Downloads/Overleaf Projects (6 items).zip");
        if !real.exists() {
            return;
        }
        let d = tempfile::tempdir().unwrap();
        let out = import_zip(&real, d.path(), "x").unwrap();
        assert_eq!(out.len(), 6);
        for p in &out {
            let project = lemmata_project::Project::open(p).unwrap();
            assert_eq!(project.main, PathBuf::from("main.tex"), "{}", p.display());
        }
        assert!(d.path().join("CA_Assignement_4/images").is_dir());
    }
}
