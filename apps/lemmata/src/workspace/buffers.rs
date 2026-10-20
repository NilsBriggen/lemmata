//! What is known about the open files' saved state (session.svelte.ts `Buffers`): unsaved edits,
//! autosave deadlines, conflicts with other programs and failed saves. The editors hold the text;
//! this keeps the rules that keep edits safe, so they can be tested without a UI.

use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};

/// FNV-1a over the UTF-8 bytes: the hash `write_text` compares with the disk.
pub fn content_hash(text: &str) -> u32 {
    lemmata_shell::files::content_hash(text.as_bytes())
}

#[derive(Debug, PartialEq)]
pub enum WriteResult {
    Saved,
    Conflict,
    Error(String),
}

/// What to do about a file that changed on disk.
#[derive(Debug, PartialEq)]
pub enum OutsideChange {
    Ignore,
    Conflict,
    Reload,
}

#[derive(Default)]
pub struct Buffers {
    /// What the disk holds, as far as the app knows: last read or written.
    pub last_saved: HashMap<String, String>,
    pub dirty: HashSet<String>,
    /// Changed by another program while they had unsaved edits here: the user decides.
    pub conflicts: Vec<String>,
    pub failed: Vec<String>,
    deadlines: HashMap<String, Instant>,
}

impl Buffers {
    /// Paths with edits not on disk yet, sorted.
    pub fn unsaved(&self) -> Vec<String> {
        let mut v: Vec<String> = self.dirty.iter().cloned().collect();
        v.sort();
        v
    }

    pub fn is_dirty(&self, path: &str) -> bool {
        self.dirty.contains(path)
    }

    /// Text read from disk.
    pub fn loaded(&mut self, path: &str, text: &str) {
        self.last_saved.insert(path.into(), text.into());
    }

    /// An edit: unsaved until the autosave delay passes (never while in conflict). 0 ms saves only on request.
    pub fn edit(&mut self, path: &str, now: Instant, autosave_ms: u64) {
        self.dirty.insert(path.into());
        self.schedule(path, now, autosave_ms);
    }

    pub fn schedule(&mut self, path: &str, now: Instant, autosave_ms: u64) {
        self.deadlines.remove(path);
        if autosave_ms > 0 && !self.conflicts.iter().any(|c| c == path) {
            self.deadlines
                .insert(path.into(), now + Duration::from_millis(autosave_ms));
        }
    }

    /// Files whose autosave delay has passed; their deadlines are cleared.
    pub fn due(&mut self, now: Instant) -> Vec<String> {
        let mut due: Vec<String> = self
            .deadlines
            .iter()
            .filter(|(_, t)| **t <= now)
            .map(|(p, _)| p.clone())
            .collect();
        due.sort();
        for p in &due {
            self.deadlines.remove(p);
        }
        due
    }

    /// The next deadline, so the UI wakes up for it.
    pub fn next_deadline(&self) -> Option<Instant> {
        self.deadlines.values().min().copied()
    }

    /// No pending autosave runs (e.g. before a restore writes the files).
    pub fn cancel_all(&mut self) {
        self.deadlines.clear();
    }

    /// What to write for `path`: the hash the disk must still have, or None when there is nothing
    /// to save (not dirty, or in conflict). `None` inside means "write whatever is there".
    pub fn begin_write(&mut self, path: &str) -> Option<Option<u32>> {
        self.deadlines.remove(path);
        if !self.dirty.contains(path) || self.conflicts.iter().any(|c| c == path) {
            return None;
        }
        Some(self.last_saved.get(path).map(|t| content_hash(t)))
    }

    /// The write of `written` finished. `current` is the editor's text now: typing that continued
    /// while the write was in flight stays unsaved.
    pub fn finish_write(
        &mut self,
        path: &str,
        written: &str,
        current: &str,
        result: Result<(), String>,
        now: Instant,
        autosave_ms: u64,
    ) -> WriteResult {
        match result {
            Err(e) if e.starts_with("conflict:") => {
                self.conflict(path);
                WriteResult::Conflict
            }
            Err(e) => {
                if !self.failed.iter().any(|f| f == path) {
                    self.failed.push(path.into());
                }
                WriteResult::Error(e)
            }
            Ok(()) => {
                self.last_saved.insert(path.into(), written.into());
                self.failed.retain(|f| f != path);
                if current == written {
                    self.dirty.remove(path);
                } else {
                    self.schedule(path, now, autosave_ms);
                }
                WriteResult::Saved
            }
        }
    }

    pub fn conflict(&mut self, path: &str) {
        self.deadlines.remove(path);
        if !self.conflicts.iter().any(|c| c == path) {
            self.conflicts.push(path.into());
        }
    }

    /// The user picked a side. Mine: the next write goes over the other program's version.
    /// Disk: ours is dropped (the caller reloads the file).
    pub fn resolve(&mut self, path: &str, keep_mine: bool) {
        self.conflicts.retain(|c| c != path);
        if keep_mine {
            self.last_saved.remove(path);
        } else {
            self.reset(path);
        }
    }

    /// Forgets the saved state, so the file is read from disk the next time it's shown.
    pub fn reset(&mut self, path: &str) {
        self.deadlines.remove(path);
        self.last_saved.remove(path);
        self.dirty.remove(path);
    }

    /// The file was closed or removed: nothing of it is kept, and no pending save runs.
    pub fn forget(&mut self, path: &str) {
        self.reset(path);
        self.conflicts.retain(|c| c != path);
        self.failed.retain(|c| c != path);
    }

    /// Another program changed `path`; `disk` is its text now and `current` the editor's.
    pub fn outside_change(
        &mut self,
        path: &str,
        disk: &str,
        current: Option<&str>,
    ) -> OutsideChange {
        // Our own read or write echoing back, or exactly what the editor has, isn't a change.
        if self.last_saved.get(path).is_some_and(|t| t == disk) {
            return OutsideChange::Ignore;
        }
        if current == Some(disk) {
            self.last_saved.insert(path.into(), disk.into());
            return OutsideChange::Ignore;
        }
        if self.dirty.contains(path) {
            self.conflict(path);
            return OutsideChange::Conflict;
        }
        OutsideChange::Reload
    }

    /// A file or folder moved: unsaved marks and conflicts follow; the text is read again under the new name.
    pub fn rename(&mut self, from: &str, to: &str) {
        let inside = |p: &str| p == from || p.starts_with(&format!("{from}/"));
        let moved = |p: &str| {
            if p == from {
                to.to_owned()
            } else if inside(p) {
                format!("{to}{}", &p[from.len()..])
            } else {
                p.to_owned()
            }
        };
        self.last_saved.retain(|p, _| !inside(p));
        self.deadlines.retain(|p, _| !inside(p));
        self.dirty = self.dirty.iter().map(|p| moved(p)).collect();
        self.conflicts = self.conflicts.iter().map(|p| moved(p)).collect();
    }

    /// The project closed.
    pub fn clear(&mut self) {
        *self = Self::default();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ms(t: Instant, n: u64) -> Instant {
        t + Duration::from_millis(n)
    }

    #[test]
    fn autosaves_after_the_delay_once_and_not_when_off() {
        let t = Instant::now();
        let mut b = Buffers::default();
        b.loaded("a.tex", "x");
        b.edit("a.tex", t, 600);
        b.edit("a.tex", ms(t, 300), 600); // typing restarts the delay
        assert!(b.due(ms(t, 899)).is_empty());
        assert_eq!(b.due(ms(t, 900)), vec!["a.tex"]);
        assert!(b.due(ms(t, 5000)).is_empty());
        let mut off = Buffers::default();
        off.edit("a.tex", t, 0);
        assert!(off.due(ms(t, 10_000)).is_empty());
    }

    #[test]
    fn saves_and_keeps_edits_typed_during_the_write_unsaved() {
        let t = Instant::now();
        let mut b = Buffers::default();
        b.loaded("a.tex", "one");
        b.edit("a.tex", t, 600);
        assert_eq!(b.begin_write("a.tex"), Some(Some(content_hash("one"))));
        // "three" was typed while "two" was being written.
        assert_eq!(
            b.finish_write("a.tex", "two", "three", Ok(()), t, 600),
            WriteResult::Saved
        );
        assert!(b.is_dirty("a.tex"));
        assert_eq!(b.due(ms(t, 600)), vec!["a.tex"]);
        assert_eq!(b.begin_write("a.tex"), Some(Some(content_hash("two"))));
        assert_eq!(
            b.finish_write("a.tex", "three", "three", Ok(()), t, 600),
            WriteResult::Saved
        );
        assert!(!b.is_dirty("a.tex"));
        assert_eq!(b.begin_write("a.tex"), None, "nothing left to save");
    }

    #[test]
    fn never_writes_over_another_programs_change_and_pauses_autosave() {
        let t = Instant::now();
        let mut b = Buffers::default();
        b.loaded("a.tex", "base");
        b.edit("a.tex", t, 600);
        b.begin_write("a.tex");
        assert_eq!(
            b.finish_write(
                "a.tex",
                "mine",
                "mine",
                Err("conflict: changed".into()),
                t,
                600
            ),
            WriteResult::Conflict
        );
        assert_eq!(b.conflicts, vec!["a.tex"]);
        b.edit("a.tex", t, 600);
        assert!(b.due(ms(t, 5000)).is_empty());
        assert_eq!(b.begin_write("a.tex"), None);
        // Keep mine: the next write doesn't check the disk.
        b.resolve("a.tex", true);
        assert_eq!(b.begin_write("a.tex"), Some(None));
        assert!(b.conflicts.is_empty());
    }

    #[test]
    fn drops_its_copy_when_the_user_takes_the_disk() {
        let mut b = Buffers::default();
        b.loaded("a.tex", "base");
        b.edit("a.tex", Instant::now(), 600);
        b.conflict("a.tex");
        b.resolve("a.tex", false);
        assert!(!b.last_saved.contains_key("a.tex"));
        assert!(!b.is_dirty("a.tex"));
        assert!(b.conflicts.is_empty());
    }

    #[test]
    fn tells_its_own_writes_from_outside_changes() {
        let t = Instant::now();
        let mut b = Buffers::default();
        b.loaded("a.tex", "one");
        assert_eq!(
            b.outside_change("a.tex", "one", Some("one")),
            OutsideChange::Ignore
        );
        b.edit("a.tex", t, 600);
        assert_eq!(
            b.outside_change("a.tex", "two", Some("two")),
            OutsideChange::Ignore
        );
        assert_eq!(b.last_saved["a.tex"], "two");
        assert_eq!(
            b.outside_change("a.tex", "other", Some("three")),
            OutsideChange::Conflict
        );
        assert_eq!(b.conflicts, vec!["a.tex"]);
        let mut clean = Buffers::default();
        clean.loaded("b.tex", "x");
        assert_eq!(
            clean.outside_change("b.tex", "y", Some("x")),
            OutsideChange::Reload
        );
    }

    #[test]
    fn keeps_a_failed_save_listed_until_it_succeeds() {
        let t = Instant::now();
        let mut b = Buffers::default();
        b.loaded("a.tex", "x");
        b.edit("a.tex", t, 600);
        assert_eq!(
            b.finish_write("a.tex", "y", "y", Err("disk full".into()), t, 600),
            WriteResult::Error("disk full".into())
        );
        assert!(b.is_dirty("a.tex"));
        assert_eq!(b.failed, vec!["a.tex"]);
        assert_eq!(
            b.finish_write("a.tex", "y", "y", Ok(()), t, 600),
            WriteResult::Saved
        );
        assert!(b.failed.is_empty());
    }

    #[test]
    fn renames_carry_marks_and_closed_files_are_forgotten() {
        let t = Instant::now();
        let mut b = Buffers::default();
        b.loaded("ch/a.tex", "a");
        b.edit("ch/a.tex", t, 600);
        b.conflict("ch/a.tex");
        b.rename("ch", "chapters");
        assert!(b.is_dirty("chapters/a.tex"));
        assert_eq!(b.conflicts, vec!["chapters/a.tex"]);
        assert!(!b.last_saved.contains_key("ch/a.tex"));
        b.loaded("main.tex", "m");
        b.edit("main.tex", t, 600);
        b.forget("main.tex");
        assert!(
            b.due(ms(t, 5000)).is_empty(),
            "a closed file's pending save never runs"
        );
        assert_eq!(b.unsaved(), vec!["chapters/a.tex"]);
        b.clear();
        assert!(b.unsaved().is_empty());
    }

    #[test]
    fn the_hash_matches_the_shell_and_the_svelte_app() {
        // contentHash("") in contentHash.ts is the FNV-1a offset basis.
        assert_eq!(content_hash(""), 0x811c9dc5);
        assert_eq!(content_hash("a"), 0xe40c292c);
    }
}
