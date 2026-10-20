//! Hunks of a file against an earlier version, and putting one back (history/hunks.ts). A line
//! diff (Myers) over lines with their line breaks, so rejecting every hunk gives the old text back
//! exactly, missing final newline included.

#[derive(Clone, Debug, PartialEq)]
pub struct Hunk {
    /// Stable while the file is unchanged: the same change gets the same key, even twice.
    pub key: String,
    /// Byte offsets into the old and the current text; whole lines.
    pub old_from: usize,
    pub old_to: usize,
    pub new_from: usize,
    pub new_to: usize,
    pub old_text: String,
    pub new_text: String,
    /// 1-based line of the hunk in the current text.
    pub line: usize,
}

/// The text cut after each line break (the last piece may have none).
fn pieces(s: &str) -> Vec<&str> {
    s.split_inclusive('\n').collect()
}

/// Myers' shortest edit script: for each piece of `a` and `b`, whether it is kept.
fn diff(a: &[&str], b: &[&str]) -> (Vec<bool>, Vec<bool>) {
    let (n, m) = (a.len() as isize, b.len() as isize);
    let max = (n + m) as usize;
    let off = max as isize;
    let mut v = vec![0isize; 2 * max + 2];
    let mut trace: Vec<Vec<isize>> = Vec::new();
    'outer: for d in 0..=max as isize {
        trace.push(v.clone());
        let mut k = -d;
        while k <= d {
            let i = (k + off) as usize;
            let mut x = if k == -d || (k != d && v[i - 1] < v[i + 1]) {
                v[i + 1]
            } else {
                v[i - 1] + 1
            };
            let mut y = x - k;
            while x < n && y < m && a[x as usize] == b[y as usize] {
                x += 1;
                y += 1;
            }
            v[i] = x;
            if x >= n && y >= m {
                break 'outer;
            }
            k += 2;
        }
    }
    // Walk back through the trace, marking the diagonals as kept.
    let (mut keep_a, mut keep_b) = (vec![false; a.len()], vec![false; b.len()]);
    let (mut x, mut y) = (n, m);
    for d in (0..trace.len() as isize).rev() {
        let v = &trace[d as usize];
        let k = x - y;
        let i = (k + off) as usize;
        let prev_k = if k == -d || (k != d && v[i - 1] < v[i + 1]) {
            k + 1
        } else {
            k - 1
        };
        let prev_x = if d == 0 {
            0
        } else {
            v[(prev_k + off) as usize]
        };
        let prev_y = prev_x - prev_k;
        while x > prev_x && y > prev_y {
            x -= 1;
            y -= 1;
            keep_a[x as usize] = true;
            keep_b[y as usize] = true;
        }
        if d == 0 {
            break;
        }
        x = prev_x;
        y = prev_y;
    }
    (keep_a, keep_b)
}

/// Differences between a snapshot's text and the current text, in document order.
pub fn compute(old: &str, new: &str) -> Vec<Hunk> {
    if old == new {
        return Vec::new();
    }
    let (a, b) = (pieces(old), pieces(new));
    let (ka, kb) = diff(&a, &b);
    let starts = |p: &[&str]| {
        let mut s = vec![0usize];
        for x in p {
            s.push(s.last().unwrap() + x.len());
        }
        s
    };
    let (sa, sb) = (starts(&a), starts(&b));
    let mut out = Vec::new();
    let mut seen: std::collections::HashMap<String, usize> = Default::default();
    let (mut i, mut j) = (0, 0);
    while i < a.len() || j < b.len() {
        if i < a.len() && j < b.len() && ka[i] && kb[j] {
            i += 1;
            j += 1;
            continue;
        }
        let (i0, j0) = (i, j);
        while i < a.len() && !ka[i] {
            i += 1;
        }
        while j < b.len() && !kb[j] {
            j += 1;
        }
        let (o, n) = (old[sa[i0]..sa[i]].to_owned(), new[sb[j0]..sb[j]].to_owned());
        let base = format!("{o}\u{0}{n}");
        let nth = seen.entry(base.clone()).or_default();
        let key = format!("{nth}\u{0}{base}");
        *nth += 1;
        out.push(Hunk {
            key,
            old_from: sa[i0],
            old_to: sa[i],
            new_from: sb[j0],
            new_to: sb[j],
            line: new[..sb[j0]].matches('\n').count() + 1,
            old_text: o,
            new_text: n,
        });
    }
    out
}

/// The current text with these hunks put back as they were in the snapshot.
pub fn reject(new: &str, hunks: &[Hunk]) -> String {
    let mut out = new.to_owned();
    let mut hs: Vec<&Hunk> = hunks.iter().collect();
    // From the end, so earlier offsets stay valid.
    hs.sort_by_key(|h| std::cmp::Reverse(h.new_from));
    for h in hs {
        out.replace_range(h.new_from..h.new_to, &h.old_text);
    }
    out
}

/// Files that are plain text; binaries can't be reviewed.
pub fn is_text_path(path: &str) -> bool {
    const EXT: &[&str] = &[
        "tex",
        "bib",
        "sty",
        "cls",
        "bst",
        "bbx",
        "cbx",
        "dbx",
        "def",
        "cfg",
        "clo",
        "ltx",
        "dtx",
        "ins",
        "ist",
        "md",
        "txt",
        "csv",
        "tsv",
        "toml",
        "json",
        "yaml",
        "yml",
        "tikz",
        "latexmkrc",
    ];
    let name = path.rsplit('/').next().unwrap_or(path);
    match name.rfind('.') {
        Some(i) => EXT.contains(&name[i + 1..].to_lowercase().as_str()),
        None => true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const OLD: &str = "one\ntwo\nthree\nfour\nfive\nsix\nseven\n";

    // hunks.test.ts
    #[test]
    fn splits_separate_changes() {
        assert!(compute(OLD, OLD).is_empty());
        let now = "one\nTWO\nthree\nfour\nfive\nsix\nseven\neight\n";
        let h = compute(OLD, now);
        let got: Vec<(&str, &str, usize)> = h
            .iter()
            .map(|h| (h.old_text.as_str(), h.new_text.as_str(), h.line))
            .collect();
        assert_eq!(got, [("two\n", "TWO\n", 2), ("", "eight\n", 8)]);
        assert_eq!(
            reject(now, &h[..1]),
            "one\ntwo\nthree\nfour\nfive\nsix\nseven\neight\n"
        );
        let now = "ONE\ntwo\nthree\nfour\nfive\nsix\nSEVEN\n";
        let h = compute(OLD, now);
        assert_eq!(h.len(), 2);
        let after = reject(now, &h[1..]);
        assert_eq!(
            compute(OLD, &after)
                .iter()
                .map(|h| h.new_text.as_str())
                .collect::<Vec<_>>(),
            ["ONE\n"]
        );
    }

    #[test]
    fn rejecting_every_hunk_gives_the_snapshot_back() {
        let cases = [
            (OLD, "ONE\ntwo\nfour\nfive\nSIX\nseven\nnew tail"),
            ("a\nb", "a\nb\nc"),
            ("a\nb\n", "b\n"),
            ("", "all new\n"),
            ("all gone\n", ""),
            ("x\ny\nx\ny\n", "x\nz\nx\ny\nx\ny\n"),
            ("no newline at end", "no newline at end\nnow there is more"),
        ];
        for (a, b) in cases {
            assert_eq!(reject(b, &compute(a, b)), a, "{a:?} → {b:?}");
        }
        let mut seed: u64 = 7;
        let mut rnd = |n: u64| {
            seed = (seed * 1103515245 + 12345) & 0x7fff_ffff;
            seed % n
        };
        let words = ["a", "b", "c", "", "d d", "e"];
        for _ in 0..300 {
            let mut doc = || {
                (0..rnd(12))
                    .map(|_| words[rnd(6) as usize])
                    .collect::<Vec<_>>()
                    .join("\n")
            };
            let (a, b) = (doc(), doc());
            let h = compute(&a, &b);
            assert_eq!(reject(&b, &h), a);
            assert!(compute(&a, &reject(&b, &h[h.len().min(1)..])).len() <= 1);
        }
    }

    #[test]
    fn keys_and_text_files() {
        let (a, b) = ("x\ny\nx\ny\nx\n", "X\ny\nx\ny\nX\n");
        let h = compute(a, b);
        let keys: std::collections::HashSet<&String> = h.iter().map(|h| &h.key).collect();
        assert_eq!(keys.len(), h.len());
        assert!(
            is_text_path("chapters/a.tex") && is_text_path("refs.bib") && is_text_path("Makefile")
        );
        assert!(!is_text_path("fig/plot.png") && !is_text_path("main.pdf"));
    }
}
