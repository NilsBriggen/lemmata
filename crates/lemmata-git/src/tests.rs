use super::*;
use std::sync::Once;

/// No global identity or other user config leaks into the tests.
fn isolate() {
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        // Safe here: every test calls this before it starts a thread of its own.
        unsafe {
            std::env::set_var(
                "GIT_CONFIG_GLOBAL",
                if cfg!(windows) { "NUL" } else { "/dev/null" },
            );
            std::env::set_var("GIT_CONFIG_NOSYSTEM", "1");
            for v in [
                "GIT_AUTHOR_NAME",
                "GIT_AUTHOR_EMAIL",
                "GIT_COMMITTER_NAME",
                "GIT_COMMITTER_EMAIL",
                "EMAIL",
            ] {
                std::env::remove_var(v);
            }
        }
    });
}

fn git(dir: &Path, args: &[&str]) -> String {
    let o = exec(
        &find_git().expect("git"),
        Some(dir),
        args,
        Duration::from_secs(30),
    )
    .unwrap();
    assert!(o.ok, "git {args:?}: {}", o.stderr);
    o.stdout
}

fn identify(dir: &Path) {
    git(dir, &["config", "user.name", "Test"]);
    git(dir, &["config", "user.email", "test@example.com"]);
}

fn repo() -> tempfile::TempDir {
    isolate();
    let d = tempfile::tempdir().unwrap();
    git(d.path(), &["init", "-q", "-b", "main"]);
    git(d.path(), &["config", "user.useConfigOnly", "true"]);
    identify(d.path());
    d
}

fn write(dir: &Path, name: &str, text: &str) {
    std::fs::write(dir.join(name), text).unwrap();
}

fn commit_file(dir: &Path, name: &str, text: &str) {
    write(dir, name, text);
    stage(dir, &[name.to_string()]).unwrap();
    commit(dir, &format!("add {name}")).unwrap();
}

fn status(dir: &Path) -> Status {
    match overview(dir) {
        Overview::Repo(s) => *s,
        o => panic!("not a repo: {o:?}"),
    }
}

/// A bare remote plus a working repository pushed to it.
fn with_remote() -> (tempfile::TempDir, tempfile::TempDir) {
    let remote = tempfile::tempdir().unwrap();
    git(remote.path(), &["init", "-q", "--bare", "-b", "main"]);
    let a = repo();
    commit_file(a.path(), "a.tex", "one\n");
    git(
        a.path(),
        &["remote", "add", "origin", &remote.path().to_string_lossy()],
    );
    push(a.path()).unwrap();
    (remote, a)
}

fn clone_of(remote: &Path) -> (tempfile::TempDir, PathBuf) {
    let parent = tempfile::tempdir().unwrap();
    let b = clone(&remote.to_string_lossy(), parent.path(), "b").unwrap();
    identify(&b);
    (parent, b)
}

#[test]
fn parses_porcelain_v2() {
    let z = "# branch.oid abc\0# branch.head main\0# branch.upstream origin/main\0# branch.ab +2 -1\0\
1 M. N... 100644 100644 100644 a b staged.tex\0\
1 .M N... 100644 100644 100644 a b my file.tex\0\
1 MM N... 100644 100644 100644 a b both.tex\0\
2 R. N... 100644 100644 100644 a b R100 new.tex\0old.tex\0\
u UU N... 1 2 3 4 a b c merge.tex\0\
? loose.tex\0";
    let s = parse_status(z);
    assert_eq!(
        (
            s.branch.as_deref(),
            s.upstream.as_deref(),
            s.ahead,
            s.behind
        ),
        (Some("main"), Some("origin/main"), 2, 1)
    );
    assert!(s.has_commits);
    let names = |v: &[FileChange]| {
        v.iter()
            .map(|c| format!("{}{}", c.status, c.path))
            .collect::<Vec<_>>()
    };
    assert_eq!(names(&s.staged), ["Mstaged.tex", "Mboth.tex", "Rnew.tex"]);
    assert_eq!(s.staged[2].from.as_deref(), Some("old.tex"));
    assert_eq!(names(&s.unstaged), ["Mmy file.tex", "Mboth.tex"]);
    assert_eq!(names(&s.untracked), ["?loose.tex"]);
    assert_eq!(names(&s.conflicted), ["Umerge.tex"]);
}

#[test]
fn detached_and_unborn() {
    let s = parse_status("# branch.oid (initial)\0# branch.head main\0");
    assert!(!s.has_commits);
    assert_eq!(
        parse_status("# branch.oid x\0# branch.head (detached)\0").branch,
        None
    );
}

#[test]
fn passwords_in_urls_are_hidden() {
    let t = tidy("fatal: unable to access 'https://git:secret@git.overleaf.com/abc/': 403\n");
    assert!(
        !t.contains("secret") && t.contains("https://***@git.overleaf.com/abc/"),
        "{t}"
    );
}

#[test]
fn plain_folder_is_not_a_repo() {
    isolate();
    let d = tempfile::tempdir().unwrap();
    assert_eq!(overview(d.path()), Overview::NotARepo);
}

#[test]
fn init_makes_an_initial_commit() {
    let d = repo(); // already `git init`ed with an identity; init() is idempotent
    write(d.path(), "main.tex", "hi\n");
    init(d.path()).unwrap();
    let s = status(d.path());
    assert!(s.has_commits && !s.dirty() && s.untracked.is_empty());
    assert_eq!(log(d.path()).unwrap()[0].subject, "Initial commit");
    assert_eq!(
        std::fs::read_to_string(d.path().join(".gitignore")).unwrap(),
        ".lemmata/\n"
    );
}

#[test]
fn unset_identity_explains_the_fix() {
    isolate();
    let d = tempfile::tempdir().unwrap();
    git(d.path(), &["init", "-q"]);
    git(d.path(), &["config", "user.useConfigOnly", "true"]);
    write(d.path(), "a.tex", "x");
    stage_all(d.path()).unwrap();
    let e = commit(d.path(), "msg").unwrap_err().to_string();
    assert!(
        e.contains("git config --global user.name") && e.contains("git config --global user.email"),
        "{e}"
    );
    // init() refuses the same way, and leaves no repository behind.
    let fresh = tempfile::tempdir().unwrap();
    let e = init(fresh.path()).unwrap_err().to_string();
    assert!(e.contains("user.email"), "{e}");
    assert!(!fresh.path().join(".git").exists());
}

#[test]
fn stage_unstage_commit() {
    let d = repo();
    write(d.path(), "a.tex", "a");
    write(d.path(), "b.tex", "b");
    assert_eq!(status(d.path()).untracked.len(), 2);
    assert!(
        commit(d.path(), "nothing")
            .unwrap_err()
            .to_string()
            .contains("Nothing is staged")
    );
    stage(d.path(), &["a.tex".into()]).unwrap();
    assert_eq!(status(d.path()).staged[0].status, "A");
    unstage(d.path(), &["a.tex".into()]).unwrap(); // before the first commit
    assert!(status(d.path()).staged.is_empty());
    stage_all(d.path()).unwrap();
    assert!(
        commit(d.path(), "  ")
            .unwrap_err()
            .to_string()
            .contains("message")
    );
    commit(d.path(), "first").unwrap();
    write(d.path(), "a.tex", "changed");
    stage(d.path(), &["a.tex".into()]).unwrap();
    unstage(d.path(), &["a.tex".into()]).unwrap();
    let s = status(d.path());
    assert!(s.staged.is_empty() && s.unstaged[0].path == "a.tex");
    assert_eq!(
        std::fs::read_to_string(d.path().join("a.tex")).unwrap(),
        "changed"
    );
    let l = log(d.path()).unwrap();
    assert_eq!(
        (l[0].subject.as_str(), l[0].author.as_str()),
        ("first", "Test")
    );
    assert!(!l[0].date.is_empty());
}

#[test]
fn branches_switch_and_refusal() {
    let d = repo();
    assert!(log(d.path()).unwrap().is_empty());
    commit_file(d.path(), "a.tex", "one");
    create_branch(d.path(), "draft").unwrap();
    assert_eq!(status(d.path()).branch.as_deref(), Some("draft"));
    assert!(create_branch(d.path(), "draft").is_err());
    assert!(create_branch(d.path(), "bad name").is_err());
    assert!(create_branch(d.path(), "--force").is_err());
    assert_eq!(branches(d.path()).unwrap(), ["draft", "main"]);
    write(d.path(), "a.tex", "edited");
    let e = switch_branch(d.path(), "main").unwrap_err().to_string();
    assert!(e.contains("uncommitted changes"), "{e}");
    assert_eq!(status(d.path()).branch.as_deref(), Some("draft"));
    // Untracked files don't block a switch.
    stage_all(d.path()).unwrap();
    commit(d.path(), "edit").unwrap();
    write(d.path(), "loose.tex", "x");
    switch_branch(d.path(), "main").unwrap();
    assert_eq!(status(d.path()).branch.as_deref(), Some("main"));
    assert!(switch_branch(d.path(), "nope").is_err());
}

#[test]
fn ahead_behind_push_pull_ff_only() {
    let (remote, a) = with_remote();
    let s = status(a.path());
    assert_eq!(
        (s.upstream.as_deref(), s.ahead, s.behind),
        (Some("origin/main"), 0, 0)
    );
    commit_file(a.path(), "two.tex", "2");
    assert_eq!(status(a.path()).ahead, 1);
    push(a.path()).unwrap(); // upstream already set: plain push
    assert_eq!(status(a.path()).ahead, 0);

    let (_keep, b) = clone_of(remote.path());
    commit_file(&b, "three.tex", "3");
    push(&b).unwrap();
    fetch(a.path()).unwrap();
    assert_eq!(status(a.path()).behind, 1);
    pull(a.path()).unwrap();
    let s = status(a.path());
    assert_eq!((s.ahead, s.behind), (0, 0));
    assert!(a.path().join("three.tex").exists());

    // Both sides move: pull explains instead of merging.
    commit_file(a.path(), "mine.tex", "m");
    commit_file(&b, "theirs.tex", "t");
    push(&b).unwrap();
    fetch(a.path()).unwrap();
    let s = status(a.path());
    assert_eq!((s.ahead, s.behind), (1, 1));
    let e = pull(a.path()).unwrap_err().to_string();
    assert!(
        e.starts_with(
            "Your branch and the remote have both changed. Pull with rebase or merge in a terminal."
        ),
        "{e}"
    );
    let e = push(a.path()).unwrap_err().to_string();
    assert!(e.contains("Pull first"), "{e}");
}

#[test]
fn first_push_sets_upstream_and_needs_a_remote() {
    let d = repo();
    commit_file(d.path(), "a.tex", "x");
    assert!(
        push(d.path())
            .unwrap_err()
            .to_string()
            .contains("no remote")
    );
    let remote = tempfile::tempdir().unwrap();
    git(remote.path(), &["init", "-q", "--bare", "-b", "main"]);
    git(
        d.path(),
        &["remote", "add", "origin", &remote.path().to_string_lossy()],
    );
    assert_eq!(status(d.path()).upstream, None);
    push(d.path()).unwrap();
    assert_eq!(status(d.path()).upstream.as_deref(), Some("origin/main"));
    // A branch that was never pushed gets its own upstream too.
    create_branch(d.path(), "draft").unwrap();
    push(d.path()).unwrap();
    assert_eq!(status(d.path()).upstream.as_deref(), Some("origin/draft"));
}

#[test]
fn clone_checks_its_input() {
    isolate();
    let parent = tempfile::tempdir().unwrap();
    for bad in ["", "-u", "ext::sh -c x"] {
        assert!(clone(bad, parent.path(), "").is_err());
    }
    assert!(clone("x", &parent.path().join("missing"), "").is_err());
    std::fs::create_dir(parent.path().join("taken")).unwrap();
    assert!(
        clone("x", parent.path(), "taken")
            .unwrap_err()
            .to_string()
            .contains("already exists")
    );
    assert!(clone("x", parent.path(), "a/b").is_err());
    // A failed clone leaves nothing behind.
    assert!(
        clone(
            &parent.path().join("no-such-repo").to_string_lossy(),
            parent.path(),
            "c"
        )
        .is_err()
    );
    assert!(!parent.path().join("c").exists());
    assert_eq!(repo_name("https://git.overleaf.com/64f0c1/"), "64f0c1");
    assert_eq!(repo_name("git@github.com:you/thesis.git"), "thesis");
    assert_eq!(repo_name(""), "repository");
}

#[test]
fn auth_failures_get_a_hint() {
    let e = net_error(
        "fatal: could not read Username for 'https://github.com': terminal prompts disabled",
    )
    .to_string();
    assert!(
        e.contains("terminal") && e.contains("credential manager"),
        "{e}"
    );
    assert!(
        !net_error("fatal: repository not found")
            .to_string()
            .contains("credential manager")
    );
}

#[cfg(unix)]
#[test]
fn slow_commands_are_stopped() {
    isolate();
    let start = Instant::now();
    let e = exec(
        &find_git().unwrap(),
        None,
        &["-c", "alias.zz=!sleep 20", "zz"],
        Duration::from_millis(300),
    );
    assert!(e.err().unwrap().to_string().contains("took longer"));
    assert!(start.elapsed() < Duration::from_secs(5));
}
