//! Creating projects: from a bundled template, or from an Overleaf zip.

use serde::{Deserialize, Serialize};
use std::fs;
use std::io;
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

pub fn list_templates(dir: &Path) -> Vec<Template> {
    let mut found: Vec<(u32, Template)> = fs::read_dir(dir)
        .into_iter()
        .flatten()
        .filter_map(|e| e.ok())
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

/// Extracts an Overleaf "Download as source (.zip)" into `parent/<name>`.
/// If everything in the zip sits in one top-level folder, that folder's
/// contents become the project.
pub fn import_zip(zip_path: &Path, parent: &Path, name: &str) -> Result<PathBuf, String> {
    let target = fresh_target(parent, name)?;
    let file = fs::File::open(zip_path)
        .map_err(|e| format!("Could not open {}: {e}", zip_path.display()))?;
    let mut archive =
        zip::ZipArchive::new(file).map_err(|e| format!("This isn't a valid zip file: {e}"))?;

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
    let strip = common_top_folder(&paths);

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
    Ok(target)
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
        let out = import_zip(&z, d.path(), "paper").unwrap();
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
        let out = import_zip(&z, d.path(), "thesis").unwrap();
        assert!(out.join("main.tex").is_file());
        assert!(out.join("ch/1.tex").is_file());
        assert!(!out.join("__MACOSX").exists());
    }

    #[test]
    fn refuses_zip_slip() {
        let d = tempfile::tempdir().unwrap();
        let z = d.path().join("evil.zip");
        make_zip(&z, &[("../evil.tex", "x")]);
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
}
