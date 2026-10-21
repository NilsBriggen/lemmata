//! Figures and tables to insert (writing/table.ts): tab-separated text from a spreadsheet as a
//! booktabs tabular, and the blocks the Figure and Table dialogs make.

/// LaTeX's special characters, escaped.
pub fn escape_latex(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\textbackslash{}"),
            '~' => out.push_str("\\textasciitilde{}"),
            '^' => out.push_str("\\textasciicircum{}"),
            '&' | '%' | '$' | '#' | '_' | '{' | '}' => {
                out.push('\\');
                out.push(c);
            }
            c => out.push(c),
        }
    }
    out
}

/// A number in a table cell: digits with separators, a sign, maybe a percent sign.
fn numeric(v: &str) -> bool {
    let v = v.strip_prefix(['-', '+']).unwrap_or(v);
    let v = v.strip_suffix('%').unwrap_or(v);
    v.ends_with(|c: char| c.is_ascii_digit())
        && v.chars()
            .all(|c| c.is_ascii_digit() || c.is_whitespace() || "'.,".contains(c))
}

/// None unless `text` looks like a table: 2+ rows, 2+ columns, all rows tab-separated.
pub fn tsv_to_tabular(text: &str) -> Option<String> {
    let text = text.replace("\r\n", "\n").replace('\r', "\n");
    let rows: Vec<&str> = text.trim_end_matches('\n').split('\n').collect();
    if rows.len() < 2 || !rows.iter().all(|r| r.contains('\t')) {
        return None;
    }
    let cells: Vec<Vec<&str>> = rows
        .iter()
        .map(|r| r.split('\t').map(str::trim).collect())
        .collect();
    let cols = cells.iter().map(Vec::len).max().unwrap_or(0);
    if cols < 2 {
        return None;
    }
    // Right-align columns that are numbers below the header.
    let align: String = (0..cols)
        .map(|c| {
            let num = cells[1..]
                .iter()
                .all(|r| r.get(c).is_none_or(|v| v.is_empty() || numeric(v)));
            if num { 'r' } else { 'l' }
        })
        .collect();
    let line = |r: &[&str]| {
        let row: Vec<String> = (0..cols)
            .map(|c| escape_latex(r.get(c).copied().unwrap_or("")))
            .collect();
        format!("    {} \\\\", row.join(" & "))
    };
    let mut out = vec![
        "\\begin{table}[htbp]".to_owned(),
        "  \\centering".into(),
        format!("  \\begin{{tabular}}{{{align}}}"),
        "    \\toprule".into(),
        line(&cells[0]),
        "    \\midrule".into(),
    ];
    out.extend(cells[1..].iter().map(|r| line(r)));
    out.extend(
        [
            "    \\bottomrule",
            "  \\end{tabular}",
            "  \\caption{}",
            "  \\label{tab:}",
            "\\end{table}",
        ]
        .map(String::from),
    );
    Some(out.join("\n"))
}

/// The file name without folders or extension.
fn stem(path: &str) -> &str {
    let name = path.rsplit('/').next().unwrap_or(path);
    name.rfind('.').map_or(name, |i| &name[..i])
}

/// A figure for a pasted or dropped image, as a template with the cursor in the caption.
pub fn figure_snippet(path: &str) -> String {
    format!(
        "\\begin{{figure}}[htbp]\n  \\centering\n  \\includegraphics[width=0.8\\linewidth]{{{path}}}\n  \\caption{{${{}}}}\n  \\label{{fig:{}}}\n\\end{{figure}}\n",
        stem(path)
    )
}

/// The slug used for a default label: letters, digits and dashes.
pub fn slug(s: &str) -> String {
    let mut out = String::new();
    for c in s.to_lowercase().chars() {
        if c.is_ascii_lowercase() || c.is_ascii_digit() {
            out.push(c);
        } else if !out.ends_with('-') {
            out.push('-');
        }
    }
    out.trim_matches('-').to_owned()
}

pub struct FigureOptions<'a> {
    pub path: &'a str,
    pub caption: &'a str,
    pub label: &'a str,
    /// Percent of the text width.
    pub width: u32,
    pub placement: &'a str,
}

/// The Figure dialog's block, as it is (no fields).
pub fn figure_block(o: &FigureOptions) -> String {
    let w = if o.width >= 100 {
        "\\linewidth".to_owned()
    } else {
        let f = format!("{:.2}", o.width as f64 / 100.0);
        format!("{}\\linewidth", f.strip_suffix('0').unwrap_or(&f))
    };
    let mut lines = vec![
        format!(
            "\\begin{{figure}}{}",
            if o.placement.is_empty() {
                String::new()
            } else {
                format!("[{}]", o.placement)
            }
        ),
        "  \\centering".into(),
        format!("  \\includegraphics[width={w}]{{{}}}", o.path),
    ];
    let caption = o.caption.trim();
    if !caption.is_empty() {
        lines.push(format!("  \\caption{{{caption}}}"));
    }
    let label = match o.label.trim() {
        "" if !caption.is_empty() => slug(caption).chars().take(30).collect(),
        "" => slug(stem(o.path)),
        l => l.to_owned(),
    };
    if !label.is_empty() {
        lines.push(format!(
            "  \\label{{fig:{}}}",
            label.strip_prefix("fig:").unwrap_or(&label)
        ));
    }
    lines.push("\\end{figure}".into());
    lines.join("\n")
}

pub struct TableOptions<'a> {
    pub rows: usize,
    pub cols: usize,
    pub header: bool,
    pub booktabs: bool,
    pub caption: &'a str,
    pub label: &'a str,
}

/// A table to insert. The cells are filled with placeholders ("Header 1", "Cell 2.3", "Caption")
/// so the result is visible right away; with `fields`, each placeholder is a tab stop.
pub fn table_block(o: &TableOptions, fields: bool) -> String {
    // Text that looks like a snippet field would be misread as one.
    let use_fields = fields
        && !format!("{}{}", o.caption, o.label).contains("${")
        && !format!("{}{}", o.caption, o.label).contains("#{");
    let ph = |t: String| if use_fields { format!("${{{t}}}") } else { t };
    let cell = |r: usize, c: usize| {
        if o.header && r == 0 {
            ph(format!("Header {}", c + 1))
        } else {
            ph(format!(
                "Cell {}.{}",
                if o.header { r } else { r + 1 },
                c + 1
            ))
        }
    };
    let row = |r: usize| {
        format!(
            "    {} \\\\",
            (0..o.cols)
                .map(|c| cell(r, c))
                .collect::<Vec<_>>()
                .join(" & ")
        )
    };
    let (top, mid, bottom) = if o.booktabs {
        ("\\toprule", "\\midrule", "\\bottomrule")
    } else {
        ("\\hline", "\\hline", "\\hline")
    };
    let caption = o.caption.trim();
    let label = match o.label.trim() {
        "" if !caption.is_empty() => slug(caption).chars().take(30).collect(),
        l => l.to_owned(),
    };
    let spec = if o.booktabs {
        "l".repeat(o.cols)
    } else {
        format!("|{}", "l|".repeat(o.cols))
    };
    let mut lines = vec![
        "\\begin{table}[htbp]".to_owned(),
        "  \\centering".into(),
        format!("  \\begin{{tabular}}{{{spec}}}"),
        format!("    {top}"),
    ];
    for r in 0..o.rows {
        lines.push(row(r));
        if r == 0 && o.header && o.rows > 1 {
            lines.push(format!("    {mid}"));
        }
    }
    // Caption and label go below the table, as is standard for floats.
    let caption = if caption.is_empty() {
        ph("Caption".into())
    } else {
        caption.to_owned()
    };
    let label = if label.is_empty() {
        ph("label".into())
    } else {
        label.strip_prefix("tab:").unwrap_or(&label).to_owned()
    };
    lines.extend([
        format!("    {bottom}"),
        "  \\end{tabular}".into(),
        format!("  \\caption{{{caption}}}"),
        format!("  \\label{{tab:{label}}}"),
        "\\end{table}".into(),
    ]);
    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    // table.test.ts
    #[test]
    fn pastes_a_spreadsheet_as_a_table() {
        let t =
            tsv_to_tabular("Method\tScore\tNotes\nBaseline\t0.71\told\nOurs\t0.84\t5% better\n")
                .unwrap();
        assert!(t.contains("\\begin{tabular}{lrl}"));
        assert!(t.contains("    Method & Score & Notes \\\\"));
        assert!(t.contains("    Ours & 0.84 & 5\\% better \\\\"));
        assert!(t.find("\\midrule").unwrap() < t.find("Baseline").unwrap());
        assert!(tsv_to_tabular("just one line\twith a tab").is_none());
        assert!(tsv_to_tabular("two\nplain lines").is_none());
        assert_eq!(
            escape_latex("a_b & {c} ~ 100$ #1 \\"),
            "a\\_b \\& \\{c\\} \\textasciitilde{} 100\\$ \\#1 \\textbackslash{}"
        );
        let f = figure_snippet("figures/my-plot.png");
        assert!(f.contains("\\includegraphics[width=0.8\\linewidth]{figures/my-plot.png}"));
        assert!(f.contains("\\label{fig:my-plot}"));
        assert!(f.contains("\\caption{${}}"));
    }

    #[test]
    fn builds_figures_and_tables() {
        let f = figure_block(&FigureOptions {
            path: "figures/a b.png",
            caption: "Learning curves",
            label: "",
            width: 80,
            placement: "htbp",
        });
        assert_eq!(
            f,
            "\\begin{figure}[htbp]\n  \\centering\n  \\includegraphics[width=0.8\\linewidth]{figures/a b.png}\n  \\caption{Learning curves}\n  \\label{fig:learning-curves}\n\\end{figure}"
        );
        let f = figure_block(&FigureOptions {
            path: "x.png",
            caption: "",
            label: "fig:mine",
            width: 100,
            placement: "",
        });
        assert!(
            f.contains("\\begin{figure}\n")
                && f.contains("width=\\linewidth")
                && f.contains("\\label{fig:mine}")
                && !f.contains("\\caption")
        );
        assert!(
            figure_block(&FigureOptions {
                path: "x.png",
                caption: "",
                label: "",
                width: 30,
                placement: ""
            })
            .contains("width=0.3\\linewidth")
        );

        let o = |rows, cols, header, booktabs, caption| TableOptions {
            rows,
            cols,
            header,
            booktabs,
            caption,
            label: "",
        };
        let t = table_block(&o(3, 2, true, true, "Results"), false);
        assert!(t.contains("\\begin{tabular}{ll}") && t.contains("Header 1 & Header 2 \\\\"));
        assert!(
            t.contains("Cell 1.1 & Cell 1.2 \\\\")
                && t.contains("Cell 2.1 & Cell 2.2 \\\\")
                && t.contains("\\midrule")
        );
        assert!(t.contains(
            "\\end{tabular}\n  \\caption{Results}\n  \\label{tab:results}\n\\end{table}"
        ));
        let t = table_block(&o(2, 2, false, true, ""), false);
        assert!(t.contains("Cell 1.1 & Cell 1.2 \\\\") && !t.contains("Header"));
        assert!(t.contains("\\caption{Caption}") && t.contains("\\label{tab:label}"));
        let t = table_block(&o(2, 2, true, true, ""), true);
        assert!(
            t.contains("${Header 1} & ${Header 2} \\\\")
                && t.contains("${Cell 1.1} & ${Cell 1.2} \\\\")
                && t.contains("\\caption{${Caption}}")
        );
        assert!(!table_block(&o(1, 1, false, true, "Cost ${x}"), true).contains("${Cell"));
        let t = table_block(&o(1, 3, false, false, ""), false);
        assert!(
            t.contains("{|l|l|l|}") && t.contains("\\hline") && t.contains("\\caption{Caption}")
        );
        assert_eq!(slug("Hello, World!"), "hello-world");
    }
}
