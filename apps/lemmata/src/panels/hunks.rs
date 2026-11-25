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

/// Patience-style anchor matching for large diffs whose edit distance exceeds `MAX_D`:
/// matches lines that appear once in `a` and once in `b` via their longest increasing
/// subsequence, then diffs each gap between anchors. When no lines match (e.g. CRLF -> LF),
/// every line stays unmarked so the caller still emits one hunk cleanly.
fn diff_anchored(a: &[&str], b: &[&str]) -> (Vec<bool>, Vec<bool>) {
    let mut count_a: std::collections::HashMap<&str, (usize, usize)> = Default::default();
    for (i, &line) in a.iter().enumerate() {
        let e = count_a.entry(line).or_insert((0, i));
        e.0 += 1;
    }
    let mut count_b: std::collections::HashMap<&str, (usize, usize)> = Default::default();
    for (j, &line) in b.iter().enumerate() {
        let e = count_b.entry(line).or_insert((0, j));
        e.0 += 1;
    }
    let mut pairs: Vec<(usize, usize)> = Vec::new();
    for (i, &line) in a.iter().enumerate() {
        if count_a.get(line).is_some_and(|(c, _)| *c == 1)
            && let Some(&(1, j)) = count_b.get(line)
        {
            pairs.push((i, j));
        }
    }
    if pairs.is_empty() {
        return (vec![false; a.len()], vec![false; b.len()]);
    }
    // Longest increasing subsequence on `j` (since `i` is already increasing).
    let mut tails: Vec<usize> = Vec::new();
    let mut prev: Vec<Option<usize>> = vec![None; pairs.len()];
    for (idx, &(_, j)) in pairs.iter().enumerate() {
        let pos = tails.partition_point(|&t| pairs[t].1 < j);
        if pos > 0 {
            prev[idx] = Some(tails[pos - 1]);
        }
        if pos == tails.len() {
            tails.push(idx);
        } else {
            tails[pos] = idx;
        }
    }
    let mut anchors: Vec<(usize, usize)> = Vec::with_capacity(tails.len());
    let mut cur = tails.last().copied();
    while let Some(idx) = cur {
        anchors.push(pairs[idx]);
        cur = prev[idx];
    }
    anchors.reverse();

    let (mut keep_a, mut keep_b) = (vec![false; a.len()], vec![false; b.len()]);
    let (mut ai, mut bi) = (0usize, 0usize);
    for (ax, bx) in anchors
        .into_iter()
        .chain(std::iter::once((a.len(), b.len())))
    {
        if ax < a.len() && bx < b.len() {
            keep_a[ax] = true;
            keep_b[bx] = true;
        }
        if ai < ax && bi < bx {
            let (ka, kb) = diff_myers(&a[ai..ax], &b[bi..bx], false);
            for (k, v) in ka.into_iter().enumerate() {
                keep_a[ai + k] = v;
            }
            for (k, v) in kb.into_iter().enumerate() {
                keep_b[bi + k] = v;
            }
        }
        ai = ax + 1;
        bi = bx + 1;
    }
    (keep_a, keep_b)
}

/// Myers' shortest edit script: for each piece of `a` and `b`, whether it is kept.
fn diff(a: &[&str], b: &[&str]) -> (Vec<bool>, Vec<bool>) {
    // Strip common prefix and suffix first so large unchanged margins never count toward MAX_D.
    let pre = a.iter().zip(b).take_while(|(x, y)| x == y).count();
    let rem_a = a.len() - pre;
    let rem_b = b.len() - pre;
    let suf = a[pre..]
        .iter()
        .rev()
        .zip(b[pre..].iter().rev())
        .take(rem_a.min(rem_b))
        .take_while(|(x, y)| x == y)
        .count();
    let (mut keep_a, mut keep_b) = (vec![false; a.len()], vec![false; b.len()]);
    for i in 0..pre {
        keep_a[i] = true;
        keep_b[i] = true;
    }
    for i in 0..suf {
        keep_a[a.len() - 1 - i] = true;
        keep_b[b.len() - 1 - i] = true;
    }
    let mid_a = &a[pre..a.len() - suf];
    let mid_b = &b[pre..b.len() - suf];
    if !mid_a.is_empty() && !mid_b.is_empty() {
        let (ka, kb) = diff_myers(mid_a, mid_b, true);
        for (i, v) in ka.into_iter().enumerate() {
            keep_a[pre + i] = v;
        }
        for (i, v) in kb.into_iter().enumerate() {
            keep_b[pre + i] = v;
        }
    }
    (keep_a, keep_b)
}

fn diff_myers(a: &[&str], b: &[&str], allow_anchor: bool) -> (Vec<bool>, Vec<bool>) {
    let (n, m) = (a.len() as isize, b.len() as isize);
    let max = (n + m) as usize;
    let off = max as isize;
    let mut v = vec![0isize; 2 * max + 2];
    // Per step only the diagonals -d..=d, which are all that step reads.
    let mut trace: Vec<Vec<isize>> = Vec::new();
    let mut found = false;
    const MAX_D: isize = 2000;
    'outer: for d in 0..=(max as isize).min(MAX_D) {
        trace.push(v[(off - d) as usize..=(off + d) as usize].to_vec());
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
                found = true;
                break 'outer;
            }
            k += 2;
        }
    }
    if !found {
        return if allow_anchor {
            diff_anchored(a, b)
        } else {
            (vec![false; a.len()], vec![false; b.len()])
        };
    }
    // Walk back through the trace, marking the diagonals as kept.
    let (mut keep_a, mut keep_b) = (vec![false; a.len()], vec![false; b.len()]);
    let (mut x, mut y) = (n, m);
    for d in (0..trace.len() as isize).rev() {
        let v = &trace[d as usize];
        let k = x - y;
        let i = (k + d) as usize;
        let prev_k = if k == -d || (k != d && v[i - 1] < v[i + 1]) {
            k + 1
        } else {
            k - 1
        };
        let prev_x = if d == 0 { 0 } else { v[(prev_k + d) as usize] };
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
    fn a_file_with_every_line_changed_is_one_hunk_not_a_gigabyte() {
        let old: String = (0..3000).map(|i| format!("a{i}\r\n")).collect();
        let new = old.replace("\r\n", "\n");
        let h = compute(&old, &new);
        assert_eq!(h.len(), 1);
        assert_eq!(
            (h[0].old_text.as_str(), h[0].new_text.as_str()),
            (old.as_str(), new.as_str())
        );
        assert_eq!(reject(&new, &h), old);
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
