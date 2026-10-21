//! Prose words in LaTeX source for spell and grammar checking: lemmata-project's scanner (the
//! one word counts use), with positions in chars as the editor counts them.

pub use lemmata_project::prose::State;

#[derive(Clone, Debug, PartialEq)]
pub struct Word {
    /// Chars from the start of the line (plus the offset given).
    pub from: usize,
    pub to: usize,
    pub text: String,
}

/// The prose words of one line, carrying `state` across lines (maths, verbatim).
pub fn line_words(line: &str, state: &mut State, offset: usize) -> Vec<Word> {
    // Byte offsets to char offsets, walked once.
    let spans = lemmata_project::prose::words(line, state);
    let mut out = Vec::with_capacity(spans.len());
    let (mut byte, mut ch) = (0usize, 0usize);
    let mut chars = line.char_indices().peekable();
    let mut to_char = |b: usize| {
        while byte < b {
            match chars.next() {
                Some((_, c)) => {
                    byte += c.len_utf8();
                    ch += 1;
                }
                None => break,
            }
        }
        ch
    };
    for (a, b) in spans {
        let from = to_char(a);
        let to = to_char(b);
        out.push(Word {
            from: offset + from,
            to: offset + to,
            text: line[a..b].to_owned(),
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn texts(line: &str, s: &mut State) -> Vec<String> {
        line_words(line, s, 0).into_iter().map(|w| w.text).collect()
    }

    // spell/prose.test.ts
    #[test]
    fn skips_commands_maths_keys_and_comments() {
        let mut s = State::default();
        assert_eq!(
            texts(
                "We use \\emph{robust} methods~\\cite[p.~3]{knuth84} with $x^2$ \\% only, see \\cref{sec:a}. % not this",
                &mut s
            ),
            ["We", "use", "robust", "methods", "with", "only", "see"]
        );
        assert_eq!(
            texts("\\section{Über die Größe} don't stop-words", &mut s),
            ["Über", "die", "Größe", "don't", "stop-words"]
        );
        assert_eq!(
            texts(
                "\\includegraphics[width=\\linewidth]{figures/plot} Caption",
                &mut s
            ),
            ["Caption"]
        );
    }

    #[test]
    fn follows_maths_and_verbatim_across_lines_and_reports_char_positions() {
        let mut s = State::default();
        assert_eq!(texts("Before \\begin{equation}", &mut s), ["Before"]);
        assert!(texts("  E = mc^2 \\text{energy}", &mut s).is_empty());
        assert_eq!(texts("\\end{equation} after", &mut s), ["after"]);
        let w = line_words("ä \\textbf{bold}", &mut State::default(), 100);
        assert_eq!(
            w[0],
            Word {
                from: 100,
                to: 101,
                text: "ä".into()
            }
        );
        assert_eq!((w[1].from, w[1].to), (110, 114));
    }
}
