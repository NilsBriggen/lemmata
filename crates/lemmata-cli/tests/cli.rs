//! Runs the `lemmata` binary on the article fixture.

use lemmata_project::Project;
use serde_json::{Value, json};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn fixture() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/article")
}

fn lemmata(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_lemmata"))
        .args(args)
        .output()
        .expect("runs the lemmata binary")
}

fn stdout(o: &Output) -> String {
    String::from_utf8_lossy(&o.stdout).into_owned()
}

fn code(o: &Output) -> i32 {
    o.status.code().unwrap_or(-1)
}

fn json(o: &Output) -> Value {
    serde_json::from_slice(&o.stdout)
        .unwrap_or_else(|e| panic!("not JSON ({e}): {}", String::from_utf8_lossy(&o.stdout)))
}

fn tool_runs(name: &str) -> bool {
    Command::new(name).arg("--version").output().is_ok()
}

/// Builds need latexmk, pdflatex and biber (the fixture uses biblatex).
fn can_build() -> bool {
    let ready = lemmata_tex::detect(&[]).tool("latexmk").is_some()
        && tool_runs("pdflatex")
        && tool_runs("biber");
    if !ready {
        eprintln!("skipping: needs latexmk, pdflatex and biber");
    }
    ready
}

/// A writable copy of the fixture: builds write into the project folder.
fn article_copy() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    copy_dir(&fixture(), dir.path());
    dir
}

fn copy_dir(from: &Path, to: &Path) {
    fs::create_dir_all(to).unwrap();
    for e in fs::read_dir(from).unwrap() {
        let e = e.unwrap();
        let dest = to.join(e.file_name());
        if e.file_type().unwrap().is_dir() {
            copy_dir(&e.path(), &dest);
        } else {
            fs::copy(e.path(), dest).unwrap();
        }
    }
}

fn arg(dir: &Path) -> &str {
    dir.to_str().unwrap()
}

fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    haystack.windows(needle.len()).any(|w| w == needle)
}

#[test]
fn help_lists_every_command() {
    let o = lemmata(&["--help"]);
    assert!(o.status.success());
    let text = stdout(&o);
    for c in [
        "build",
        "watch",
        "lint",
        "wordcount",
        "bib",
        "export",
        "outline",
        "doctor",
    ] {
        assert!(text.contains(c), "--help doesn't mention {c}");
    }
}

#[test]
fn a_missing_folder_is_an_error() {
    let o = lemmata(&["wordcount", "/nonexistent/lemmata-test-folder"]);
    assert_eq!(code(&o), 1);
    assert!(String::from_utf8_lossy(&o.stderr).contains("opening"));
}

#[test]
fn wordcount_matches_the_app_count() {
    let dir = fixture();
    let o = lemmata(&["wordcount", arg(&dir), "--json"]);
    assert_eq!(code(&o), 0);
    let v = json(&o);
    let expected = lemmata_project::extras::counts(&Project::open(&dir).unwrap());
    assert_eq!(
        v["total_words"].as_u64(),
        Some(u64::from(expected.total_words))
    );
    let per_file: u64 = v["files"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| f["words"].as_u64().unwrap())
        .sum();
    assert_eq!(per_file, u64::from(expected.total_words));
    assert!(v["files"].to_string().contains("chapters/method.tex"));

    let plain = lemmata(&["wordcount", arg(&dir)]);
    assert!(stdout(&plain).contains("words in total"));
}

#[test]
fn outline_gives_headings_with_their_lines() {
    let dir = fixture();
    let o = lemmata(&["outline", arg(&dir), "--json"]);
    assert_eq!(code(&o), 0);
    let v = json(&o);
    let h = &v["headings"][0];
    assert_eq!(h["title"], "Method");
    assert_eq!(h["level"], 2);
    assert_eq!(h["file"], "chapters/method.tex");
    assert_eq!(h["line"], 1);

    let plain = stdout(&lemmata(&["outline", arg(&dir)]));
    assert!(plain.contains("chapters/method.tex:1:"), "{plain}");
    assert!(plain.contains("Method ("), "{plain}");
}

#[test]
fn bib_lists_entries_and_missing_keys() {
    let dir = fixture();
    let listed = lemmata(&["bib", arg(&dir)]);
    assert_eq!(code(&listed), 0);
    assert!(stdout(&listed).contains("schulman2017ppo"));

    let missing = lemmata(&["bib", arg(&dir), "--missing", "--json"]);
    assert_eq!(code(&missing), 1, "a cited key is in no .bib file");
    assert_eq!(json(&missing), json!(["nobody2020"]));
}

#[test]
fn bib_counts_a_hand_written_bibliography_as_defined() {
    let d = article_copy();
    let method = d.path().join("chapters/method.tex");
    let mut text = fs::read_to_string(&method).unwrap();
    text.push_str("\\begin{thebibliography}{9}\n\\bibitem[Nobody(2020)]{nobody2020} A.\n\\end{thebibliography}\n");
    fs::write(&method, text).unwrap();

    let o = lemmata(&["bib", arg(d.path()), "--missing", "--json"]);
    assert_eq!(code(&o), 0, "{}", stdout(&o));
    assert_eq!(json(&o), json!([]));
}

#[test]
fn bib_unused_lists_entries_nothing_cites() {
    let d = article_copy();
    let mut bib = fs::read_to_string(d.path().join("refs.bib")).unwrap();
    bib.push_str("\n@book{unusedbook2001,\n  title = {Never Cited},\n  year = {2001}\n}\n");
    fs::write(d.path().join("refs.bib"), bib).unwrap();

    let o = lemmata(&["bib", arg(d.path()), "--unused", "--json"]);
    assert_eq!(code(&o), 0);
    let v = json(&o);
    let list = v.as_array().unwrap();
    assert_eq!(list.len(), 1);
    assert_eq!(list[0]["key"], "unusedbook2001");
    assert_eq!(list[0]["cited"], false);
}

#[test]
fn bib_unused_and_missing_conflict() {
    let dir = fixture();
    let o = lemmata(&["bib", arg(&dir), "--unused", "--missing"]);
    assert_eq!(code(&o), 2);
}

#[test]
fn export_zips_sources_but_not_build_output() {
    let d = article_copy();
    fs::create_dir_all(d.path().join(".lemmata/build")).unwrap();
    fs::write(d.path().join(".lemmata/build/main.pdf"), b"%PDF-1.5").unwrap();
    let out = tempfile::tempdir().unwrap();
    let zip = out.path().join("project.zip");

    let o = lemmata(&["export", arg(d.path()), "--zip", zip.to_str().unwrap()]);
    assert_eq!(code(&o), 0, "{}", String::from_utf8_lossy(&o.stderr));
    let bytes = fs::read(&zip).unwrap();
    assert!(bytes.starts_with(b"PK\x03\x04"));
    assert!(contains(&bytes, b"chapters/method.tex"));
    assert!(contains(&bytes, b"refs.bib"));
    assert!(!contains(&bytes, b".lemmata"), "build output was exported");
    assert!(!contains(&bytes, b"%PDF-1.5"));

    // Check the archive with unzip when it is installed.
    if tool_runs("unzip") {
        let t = Command::new("unzip").arg("-tq").arg(&zip).output().unwrap();
        assert!(t.status.success(), "unzip -t rejected the zip");
        let main = Command::new("unzip")
            .arg("-p")
            .arg(&zip)
            .arg("main.tex")
            .output()
            .unwrap();
        assert_eq!(
            main.stdout,
            fs::read(fixture().join("main.tex")).unwrap(),
            "main.tex differs after the round trip"
        );
    } else {
        eprintln!("unzip not installed: archive check skipped");
    }
}

#[test]
fn lint_reports_the_missing_citation_and_label() {
    if !can_build() {
        return;
    }
    let d = article_copy();
    let o = lemmata(&["lint", arg(d.path())]);
    let text = stdout(&o);
    assert_eq!(code(&o), 1, "{text}");
    assert!(
        text.contains("chapters/method.tex:7: error: citation `nobody2020` is in no .bib file"),
        "{text}"
    );
    assert!(
        text.contains("chapters/method.tex:7: error: reference to label `sec:missing`"),
        "{text}"
    );
    // The build's own "undefined reference" warning for the label is replaced by the located error.
    assert_eq!(text.matches("sec:missing").count(), 1, "{text}");
    assert!(text.contains("2 error(s)"), "{text}");
}

#[test]
fn lint_json_has_located_diagnostics() {
    if !can_build() {
        return;
    }
    let d = article_copy();
    let o = lemmata(&["lint", arg(d.path()), "--json"]);
    assert_eq!(code(&o), 1);
    let v = json(&o);
    let errors: Vec<&Value> = v
        .as_array()
        .unwrap()
        .iter()
        .filter(|x| x["severity"] == "error")
        .collect();
    assert_eq!(errors.len(), 2, "{v}");
    assert!(
        errors
            .iter()
            .any(|x| x["kind"]["type"] == "undefined-citation"
                && x["kind"]["key"] == "nobody2020"
                && x["line"] == 7)
    );
    assert!(
        errors.iter().any(
            |x| x["kind"]["type"] == "undefined-reference" && x["kind"]["key"] == "sec:missing"
        )
    );
}

#[test]
fn lint_passes_once_the_references_resolve() {
    if !can_build() {
        return;
    }
    let d = article_copy();
    let path = d.path().join("chapters/method.tex");
    let fixed = fs::read_to_string(&path)
        .unwrap()
        .replace("sec:missing", "sec:method")
        .replace("nobody2020", "schulman2017ppo");
    fs::write(&path, fixed).unwrap();

    let o = lemmata(&["lint", arg(d.path()), "--json"]);
    assert_eq!(code(&o), 0, "{}", stdout(&o));
    let v = json(&o);
    assert!(
        v.as_array()
            .unwrap()
            .iter()
            .all(|x| x["severity"] != "error"),
        "{v}"
    );
}
