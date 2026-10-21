//! Creating projects: from a bundled template, or from an Overleaf zip.

use serde::{Deserialize, Serialize};
use std::fs;
use std::io::{self, Read, Seek};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize)]
pub struct Template {
    pub id: String,
    pub name: String,
    pub description: String,
    /// Saved by the user from one of their projects, so it can be deleted.
    pub user: bool,
    /// Gallery section; user templates are always "My templates".
    pub category: String,
    pub tags: Vec<String>,
    /// Page 1 as a `data:image/png;base64,` URL, when the template ships a preview.png.
    pub preview: Option<String>,
}

pub const USER_CATEGORY: &str = "My templates";

/// Ids of user templates start with this, so they can't clash with bundled ones.
pub const USER_PREFIX: &str = "user:";
/// Ids of templates from the template collection (see online_templates.rs).
pub const ONLINE_PREFIX: &str = "online:";

/// Where a template comes from: shipped with the app, the template collection, or saved by the user.
#[derive(Clone, Copy, PartialEq)]
enum Source {
    Bundled,
    Online,
    User,
}

#[derive(Deserialize, Serialize)]
pub(crate) struct TemplateMeta {
    pub name: String,
    pub description: String,
    #[serde(default)]
    pub order: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub category: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
}

/// Checks that `path` may be moved to the trash as a whole project: a folder
/// with a .tex file or lemmata.toml directly inside, and not the home folder
/// or a folder above it (a stray .tex file there must not take it all).
pub fn deletable_project(path: &Path, home: Option<&Path>) -> Result<PathBuf, String> {
    // The plain form, like the open project's root: with Windows' \\?\ it would never match it.
    let dir = lemmata_project::canonical(path)
        .map_err(|e| format!("Could not find {}: {e}", path.display()))?;
    let refuse = || {
        format!(
            "{} doesn't look like a project folder, so Lemmata won't delete it.",
            dir.display()
        )
    };
    let above_home = home
        .and_then(|h| lemmata_project::canonical(h).ok())
        .is_some_and(|h| h.starts_with(&dir));
    if !dir.is_dir() || dir.parent().is_none() || above_home {
        return Err(refuse());
    }
    let project = fs::read_dir(&dir)
        .map_err(|e| format!("Could not read {}: {e}", dir.display()))?
        .flatten()
        .any(|e| {
            let name = e.file_name().to_string_lossy().to_lowercase();
            name == "lemmata.toml" || name.ends_with(".tex")
        });
    if !project {
        return Err(refuse());
    }
    Ok(dir)
}

/// Metadata file inside each template folder; not copied into projects.
const META: &str = "template.toml";
/// Gallery thumbnail of a bundled template; not copied into projects either.
const PREVIEW: &str = "preview.png";
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

/// Bundled templates in their own order, then the collection's (kept in `online`), then the user's.
pub fn list_all(bundled: &Path, online: &Path, user: &Path) -> Vec<Template> {
    let mut all = list_templates(bundled, Source::Bundled);
    all.extend(list_templates(online, Source::Online));
    all.extend(list_templates(user, Source::User));
    all
}

fn list_templates(dir: &Path, source: Source) -> Vec<Template> {
    let user = source == Source::User;
    let mut found: Vec<(u32, Template)> = fs::read_dir(dir)
        .into_iter()
        .flatten()
        .filter_map(|e| e.ok())
        // The example document is offered on its own, not as a template.
        .filter(|e| e.file_name() != EXAMPLE)
        .filter_map(|e| {
            let text = fs::read_to_string(e.path().join(META)).ok()?;
            let meta: TemplateMeta = toml::from_str(&text).ok()?;
            let folder = e.file_name().to_string_lossy().into_owned();
            let id = match source {
                Source::User => format!("{USER_PREFIX}{folder}"),
                Source::Online => format!("{ONLINE_PREFIX}{folder}"),
                Source::Bundled => folder,
            };
            let category = match (user, meta.category) {
                (false, Some(c)) if !c.trim().is_empty() => c,
                (false, _) => "Other".to_string(),
                (true, _) => USER_CATEGORY.to_string(),
            };
            // Only bundled and collection templates come with a thumbnail: a user's own project may have a preview.png of its own.
            let preview = (!user)
                .then(|| fs::read(e.path().join(PREVIEW)).ok())
                .flatten()
                .map(|png| format!("data:image/png;base64,{}", base64(&png)));
            Some((
                meta.order,
                Template {
                    id,
                    name: meta.name,
                    description: meta.description,
                    user,
                    category,
                    tags: meta.tags,
                    preview,
                },
            ))
        })
        .collect();
    found.sort_by_key(|(order, t)| (*order, t.name.clone()));
    found.into_iter().map(|(_, t)| t).collect()
}

/// Standard base64; the thumbnails are the only thing that needs it.
fn base64(bytes: &[u8]) -> String {
    const A: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for c in bytes.chunks(3) {
        let n = (c[0] as u32) << 16
            | (*c.get(1).unwrap_or(&0) as u32) << 8
            | *c.get(2).unwrap_or(&0) as u32;
        for i in 0..4 {
            if i <= c.len() {
                out.push(A[(n >> (18 - 6 * i) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
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

/// The folder a template id names: the user's (`user:`), the collection's (`online:`) or bundled.
fn template_dir(bundled: &Path, online: &Path, user: &Path, id: &str) -> Result<PathBuf, String> {
    Ok(if let Some(folder) = id.strip_prefix(USER_PREFIX) {
        user.join(check_name(folder)?)
    } else if let Some(folder) = id.strip_prefix(ONLINE_PREFIX) {
        online.join(check_name(folder)?)
    } else {
        bundled.join(check_name(id)?)
    })
}

pub fn create_from_template(
    bundled: &Path,
    online: &Path,
    user: &Path,
    id: &str,
    parent: &Path,
    name: &str,
) -> Result<PathBuf, String> {
    let source = template_dir(bundled, online, user, id)?;
    if !source.join(META).is_file() {
        return Err(format!("There is no template called {id}."));
    }
    let target = fresh_target(parent, name)?;
    copy_dir(&source, &target).map_err(|e| format!("Could not create the project: {e}"))?;
    let _ = fs::remove_file(target.join(META));
    if !id.starts_with(USER_PREFIX) {
        let _ = fs::remove_file(target.join(PREVIEW));
    }
    Ok(target)
}

/// Sets a new project's engine to Tectonic, keeping the rest of its lemmata.toml.
pub fn use_tectonic(project: &Path) -> Result<(), String> {
    let mut p = lemmata_project::Project::open(project).map_err(|e| e.to_string())?;
    p.config.engine = lemmata_project::Engine::Tectonic;
    p.save_config().map_err(|e| e.to_string())
}

/// A folder name for a template called `name`: letters, digits and dashes.
fn slug(name: &str) -> String {
    let s: String = name
        .trim()
        .chars()
        .map(|c| {
            if c.is_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect();
    let s = s
        .split('-')
        .filter(|p| !p.is_empty())
        .collect::<Vec<_>>()
        .join("-");
    if s.is_empty() { "template".into() } else { s }
}

/// Saves a copy of a project (without hidden files or build output) as a
/// user template. Returns the new template's id.
pub fn save_as_template(
    user: &Path,
    project_root: &Path,
    outdir: &str,
    name: &str,
    description: &str,
) -> Result<String, String> {
    let name = name.trim();
    if name.is_empty() {
        return Err("Give the template a name.".into());
    }
    let mut folder = slug(name);
    let mut n = 1;
    while user.join(&folder).exists() {
        n += 1;
        folder = format!("{}-{n}", slug(name));
    }
    let dest = user.join(&folder);
    let result = (|| -> io::Result<()> {
        fn walk(entries: &[lemmata_project::Entry], root: &Path, dest: &Path) -> io::Result<()> {
            for e in entries {
                let target = dest.join(&e.path);
                if e.is_dir {
                    fs::create_dir_all(&target)?;
                    walk(&e.children, root, dest)?;
                } else {
                    if let Some(parent) = target.parent() {
                        fs::create_dir_all(parent)?;
                    }
                    fs::copy(root.join(&e.path), target)?;
                }
            }
            Ok(())
        }
        fs::create_dir_all(&dest)?;
        walk(
            &lemmata_project::tree(project_root, outdir),
            project_root,
            &dest,
        )?;
        let meta = TemplateMeta {
            name: name.to_string(),
            description: description.trim().to_string(),
            order: 1000,
            category: None,
            tags: vec![],
        };
        fs::write(
            dest.join(META),
            toml::to_string_pretty(&meta).map_err(io::Error::other)?,
        )
    })();
    if let Err(e) = result {
        let _ = fs::remove_dir_all(&dest);
        return Err(format!("Could not save the template: {e}"));
    }
    Ok(format!("{USER_PREFIX}{folder}"))
}

/// Deletes a user template. Bundled ones can't be deleted.
pub fn delete_user_template(user: &Path, id: &str) -> Result<(), String> {
    let folder = id
        .strip_prefix(USER_PREFIX)
        .ok_or("Only your own templates can be deleted.")?;
    let dir = user.join(check_name(folder)?);
    if !dir.join(META).is_file() {
        return Err("That template no longer exists.".into());
    }
    fs::remove_dir_all(dir).map_err(|e| format!("Could not delete the template: {e}"))
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
            // A repository's or Lemmata's own settings can name programs to run (git's
            // core.fsmonitor, say): those never come in from a zip.
            if rel.as_os_str().is_empty()
                || rel.components().any(|c| {
                    matches!(
                        c.as_os_str().to_str(),
                        Some("__MACOSX" | ".git" | ".lemmata")
                    )
                })
            {
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

    #[test]
    fn only_project_folders_can_be_deleted() {
        let home = tempfile::tempdir().unwrap();
        let project = home.path().join("thesis");
        fs::create_dir_all(project.join("chapters")).unwrap();
        fs::write(project.join("main.tex"), "").unwrap();
        assert!(deletable_project(&project, Some(home.path())).is_ok());
        // A folder with .tex files only further down is not a project.
        let docs = home.path().join("Documents");
        fs::create_dir_all(docs.join("x")).unwrap();
        fs::write(docs.join("x/a.tex"), "").unwrap();
        assert!(deletable_project(&docs, Some(home.path())).is_err());
        // Never the home folder or a folder above it, even with a .tex inside.
        fs::write(home.path().join("stray.tex"), "").unwrap();
        assert!(deletable_project(home.path(), Some(home.path())).is_err());
        assert!(deletable_project(&project, Some(&project.join("chapters"))).is_err());
        assert!(deletable_project(&home.path().join("missing"), None).is_err());
    }

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
    fn leaves_out_repository_and_lemmata_settings() {
        let d = tempfile::tempdir().unwrap();
        let z = d.path().join("x.zip");
        make_zip(
            &z,
            &[
                ("main.tex", "x"),
                (".git/config", "[core]\n\tfsmonitor = sh -c evil"),
                (".lemmata/history/config", "x"),
            ],
        );
        let out = import_zip(&z, d.path(), "x").unwrap().remove(0);
        assert!(out.join("main.tex").is_file());
        assert!(!out.join(".git").exists() && !out.join(".lemmata").exists());
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
        let none = d.path().join("no-online-templates");
        let t = d.path().join("templates/article");
        fs::create_dir_all(&t).unwrap();
        fs::write(
            t.join(META),
            "name = \"Article\"\ndescription = \"x\"\ncategory = \"Papers\"\ntags = [\"a\", \"b\"]\n",
        )
        .unwrap();
        fs::write(t.join(PREVIEW), b"ab").unwrap();
        fs::write(t.join("main.tex"), "\\documentclass{article}").unwrap();
        let (bundled, user) = (d.path().join("templates"), d.path().join("user"));
        let listed = &list_all(&bundled, &none, &user)[0];
        assert_eq!(listed.name, "Article");
        assert_eq!((listed.category.as_str(), listed.tags.len()), ("Papers", 2));
        assert_eq!(
            listed.preview.as_deref(),
            Some("data:image/png;base64,YWI=")
        );

        let p =
            create_from_template(&bundled, &none, &user, "article", d.path(), "My Paper").unwrap();
        assert!(p.join("main.tex").is_file());
        assert!(!p.join(META).exists());
        assert!(!p.join(PREVIEW).exists());
        assert!(
            create_from_template(&bundled, &none, &user, "article", d.path(), "My Paper").is_err()
        );
        assert!(create_from_template(&bundled, &none, &user, "../x", d.path(), "q").is_err());
        assert!(create_from_template(&bundled, &none, &user, "user:../x", d.path(), "q").is_err());
        assert!(
            create_from_template(&bundled, &none, &user, "online:../x", d.path(), "q").is_err()
        );
    }

    #[test]
    fn tectonic_replaces_only_the_engine() {
        let d = tempfile::tempdir().unwrap();
        fs::write(d.path().join("main.tex"), "\\documentclass{article}").unwrap();
        use_tectonic(d.path()).unwrap();
        let text = fs::read_to_string(d.path().join("lemmata.toml")).unwrap();
        assert!(text.contains("engine = \"tectonic\""), "{text}");

        // A template's own settings and comments stay.
        fs::write(
            d.path().join("lemmata.toml"),
            "# my notes\nmain = \"main.tex\"\nengine = \"xelatex\" # for the font\nstop_on_error = true\n",
        )
        .unwrap();
        use_tectonic(d.path()).unwrap();
        let text = fs::read_to_string(d.path().join("lemmata.toml")).unwrap();
        assert!(
            text.starts_with("# my notes\nmain = \"main.tex\"\n"),
            "{text}"
        );
        assert!(
            text.contains("engine = \"tectonic\" # for the font"),
            "{text}"
        );
        assert!(text.contains("stop_on_error = true"), "{text}");
        assert!(!text.contains("xelatex"), "{text}");
    }

    #[test]
    fn encodes_base64() {
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"a"), "YQ==");
        assert_eq!(base64(b"ab"), "YWI=");
        assert_eq!(base64(b"abc"), "YWJj");
        assert_eq!(
            base64(b"Many hands make light work."),
            "TWFueSBoYW5kcyBtYWtlIGxpZ2h0IHdvcmsu"
        );
    }

    #[test]
    fn saves_a_project_as_a_template_and_uses_it() {
        let d = tempfile::tempdir().unwrap();
        let none = d.path().join("no-online-templates");
        let (bundled, user, proj) = (
            d.path().join("bundled"),
            d.path().join("user"),
            d.path().join("thesis"),
        );
        fs::create_dir_all(proj.join("chapters")).unwrap();
        fs::create_dir_all(proj.join(".lemmata/build")).unwrap();
        fs::write(proj.join("main.tex"), "\\documentclass{book}").unwrap();
        fs::write(proj.join("chapters/one.tex"), "x").unwrap();
        fs::write(proj.join(".lemmata/build/main.pdf"), "pdf").unwrap();

        let id = save_as_template(&user, &proj, ".lemmata/build", "My Thesis!", "Mine").unwrap();
        assert_eq!(id, "user:my-thesis");
        // A second save with the same name doesn't overwrite.
        assert_eq!(
            save_as_template(&user, &proj, ".lemmata/build", "My Thesis!", "").unwrap(),
            "user:my-thesis-2"
        );
        assert!(save_as_template(&user, &proj, ".lemmata/build", "  ", "").is_err());

        let listed = list_all(&bundled, &none, &user);
        assert_eq!(listed.len(), 2);
        assert!(listed.iter().all(|t| t.user));
        assert!(listed.iter().any(|t| t.description == "Mine"));

        let made =
            create_from_template(&bundled, &none, &user, &id, d.path(), "New thesis").unwrap();
        assert!(made.join("chapters/one.tex").is_file());
        assert!(!made.join(".lemmata").exists() && !made.join(META).exists());

        delete_user_template(&user, &id).unwrap();
        assert_eq!(list_all(&bundled, &none, &user).len(), 1);
        assert!(delete_user_template(&user, "article").is_err());
        assert!(delete_user_template(&user, "user:../../x").is_err());
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

    #[test]
    fn templates_from_the_collection_are_listed_and_used_like_bundled_ones() {
        let d = tempfile::tempdir().unwrap();
        let online = d.path().join("online");
        fs::create_dir_all(online.join("ieee")).unwrap();
        fs::write(
            online.join("ieee/template.toml"),
            "name = \"IEEE\"\ndescription = \"d\"\ncategory = \"Papers\"\n",
        )
        .unwrap();
        fs::write(online.join("ieee/main.tex"), "x").unwrap();
        fs::write(online.join("ieee/preview.png"), "png").unwrap();
        let none = d.path().join("none");
        let listed = list_all(&none, &online, &none);
        assert_eq!(listed.len(), 1);
        assert_eq!(
            (
                listed[0].id.as_str(),
                listed[0].category.as_str(),
                listed[0].user
            ),
            ("online:ieee", "Papers", false)
        );
        assert!(listed[0].preview.is_some());
        let made =
            create_from_template(&none, &online, &none, "online:ieee", d.path(), "Paper").unwrap();
        assert!(made.join("main.tex").is_file());
        assert!(!made.join("template.toml").exists() && !made.join("preview.png").exists());
    }
}
