//! Which LaTeX package provides a command or environment, for "Undefined
//! control sequence" and "Environment … undefined" errors. Only entries that
//! have a single, well-known home are listed.

/// Commands without the backslash.
const COMMANDS: &[(&str, &str)] = &[
    // graphics and colour
    ("includegraphics", "graphicx"),
    ("graphicspath", "graphicx"),
    ("rotatebox", "graphicx"),
    ("scalebox", "graphicx"),
    ("resizebox", "graphicx"),
    ("reflectbox", "graphicx"),
    ("textcolor", "xcolor"),
    ("colorbox", "xcolor"),
    ("definecolor", "xcolor"),
    ("fcolorbox", "xcolor"),
    ("rowcolor", "colortbl"),
    ("cellcolor", "colortbl"),
    ("columncolor", "colortbl"),
    ("tikz", "tikz"),
    ("node", "tikz"),
    ("draw", "tikz"),
    ("usetikzlibrary", "tikz"),
    ("addplot", "pgfplots"),
    ("pgfplotsset", "pgfplots"),
    // links
    ("url", "hyperref"),
    ("href", "hyperref"),
    ("autoref", "hyperref"),
    ("nameref", "hyperref"),
    ("hypersetup", "hyperref"),
    ("hyperref", "hyperref"),
    ("phantomsection", "hyperref"),
    ("texorpdfstring", "hyperref"),
    ("cref", "cleveref"),
    ("Cref", "cleveref"),
    ("crefrange", "cleveref"),
    ("labelcref", "cleveref"),
    // tables
    ("toprule", "booktabs"),
    ("midrule", "booktabs"),
    ("bottomrule", "booktabs"),
    ("cmidrule", "booktabs"),
    ("addlinespace", "booktabs"),
    ("multirow", "multirow"),
    ("tabularnewline", "array"),
    ("newcolumntype", "array"),
    ("makecell", "makecell"),
    ("thead", "makecell"),
    ("tabularx", "tabularx"),
    ("endhead", "longtable"),
    ("endfirsthead", "longtable"),
    // maths
    ("mathbb", "amssymb"),
    ("mathfrak", "amssymb"),
    ("checkmark", "amssymb"),
    ("leqslant", "amssymb"),
    ("geqslant", "amssymb"),
    ("varnothing", "amssymb"),
    ("square", "amssymb"),
    ("blacksquare", "amssymb"),
    ("implies", "amsmath"),
    ("iff", "amsmath"),
    ("text", "amsmath"),
    ("dfrac", "amsmath"),
    ("tfrac", "amsmath"),
    ("binom", "amsmath"),
    ("boxed", "amsmath"),
    ("eqref", "amsmath"),
    ("intertext", "amsmath"),
    ("operatorname", "amsmath"),
    ("DeclareMathOperator", "amsmath"),
    ("underset", "amsmath"),
    ("overset", "amsmath"),
    ("xrightarrow", "amsmath"),
    ("xleftarrow", "amsmath"),
    ("substack", "amsmath"),
    ("tag", "amsmath"),
    ("nonumber", "amsmath"),
    ("mathbbm", "bbm"),
    ("bm", "bm"),
    ("boldsymbol", "amsmath"),
    ("mathscr", "mathrsfs"),
    ("qedhere", "amsthm"),
    ("newtheorem", "amsthm"),
    ("theoremstyle", "amsthm"),
    ("SI", "siunitx"),
    ("si", "siunitx"),
    ("num", "siunitx"),
    ("qty", "siunitx"),
    ("unit", "siunitx"),
    ("ang", "siunitx"),
    ("SIrange", "siunitx"),
    ("sisetup", "siunitx"),
    ("ce", "mhchem"),
    // text and layout
    ("lipsum", "lipsum"),
    ("blindtext", "blindtext"),
    ("setstretch", "setspace"),
    ("doublespacing", "setspace"),
    ("onehalfspacing", "setspace"),
    ("singlespacing", "setspace"),
    ("geometry", "geometry"),
    ("newgeometry", "geometry"),
    ("fancyhf", "fancyhdr"),
    ("fancyhead", "fancyhdr"),
    ("fancyfoot", "fancyhdr"),
    ("setlist", "enumitem"),
    ("setitemize", "enumitem"),
    ("titleformat", "titlesec"),
    ("titlespacing", "titlesec"),
    ("captionof", "caption"),
    ("captionsetup", "caption"),
    ("subcaption", "subcaption"),
    ("subfloat", "subfig"),
    ("textls", "microtype"),
    ("csvreader", "csvsimple"),
    ("subfile", "subfiles"),
    ("includepdf", "pdfpages"),
    ("includesvg", "svg"),
    ("lstinline", "listings"),
    ("lstset", "listings"),
    ("lstinputlisting", "listings"),
    ("mintinline", "minted"),
    ("inputminted", "minted"),
    ("todo", "todonotes"),
    ("listoftodos", "todonotes"),
    ("xspace", "xspace"),
    ("ifthenelse", "ifthen"),
    ("newboolean", "ifthen"),
    ("setboolean", "ifthen"),
    ("ifstrequal", "etoolbox"),
    ("AtBeginEnvironment", "etoolbox"),
    ("robustify", "etoolbox"),
    ("nomenclature", "nomencl"),
    ("printnomenclature", "nomencl"),
    ("printglossary", "glossaries"),
    ("newacronym", "glossaries"),
    ("gls", "glossaries"),
    ("acrshort", "glossaries"),
    ("makeindex", "makeidx"),
    ("blockquote", "csquotes"),
    ("enquote", "csquotes"),
    ("textquote", "csquotes"),
    ("parencite", "biblatex"),
    ("textcite", "biblatex"),
    ("autocite", "biblatex"),
    ("footcite", "biblatex"),
    ("printbibliography", "biblatex"),
    ("addbibresource", "biblatex"),
    ("citep", "natbib"),
    ("citet", "natbib"),
    ("citeauthor", "natbib"),
    ("citeyear", "natbib"),
    ("pause", "beamer"),
    ("frametitle", "beamer"),
    ("only", "beamer"),
    ("alert", "beamer"),
    ("usetheme", "beamer"),
    ("usecolortheme", "beamer"),
];

/// Environments.
const ENVIRONMENTS: &[(&str, &str)] = &[
    ("align", "amsmath"),
    ("align*", "amsmath"),
    ("alignat", "amsmath"),
    ("gather", "amsmath"),
    ("gather*", "amsmath"),
    ("multline", "amsmath"),
    ("multline*", "amsmath"),
    ("flalign", "amsmath"),
    ("split", "amsmath"),
    ("cases", "amsmath"),
    ("subequations", "amsmath"),
    ("pmatrix", "amsmath"),
    ("bmatrix", "amsmath"),
    ("vmatrix", "amsmath"),
    ("Bmatrix", "amsmath"),
    ("Vmatrix", "amsmath"),
    ("smallmatrix", "amsmath"),
    ("proof", "amsthm"),
    ("tikzpicture", "tikz"),
    ("axis", "pgfplots"),
    ("tabularx", "tabularx"),
    ("longtable", "longtable"),
    ("tabulary", "tabulary"),
    ("multicols", "multicol"),
    ("wrapfigure", "wrapfig"),
    ("subfigure", "subcaption"),
    ("subtable", "subcaption"),
    ("listing", "listings"),
    ("lstlisting", "listings"),
    ("minted", "minted"),
    ("algorithm", "algorithm"),
    ("algorithmic", "algorithmic"),
    ("algorithm2e", "algorithm2e"),
    ("enumerate*", "enumitem"),
    ("itemize*", "enumitem"),
    ("landscape", "pdflscape"),
    ("sidewaysfigure", "rotating"),
    ("sidewaystable", "rotating"),
    ("spacing", "setspace"),
    ("otherlanguage", "babel"),
    ("comment", "comment"),
    ("filecontents", "filecontents"),
    ("mdframed", "mdframed"),
    ("tcolorbox", "tcolorbox"),
    ("exampleblock", "beamer"),
    ("alertblock", "beamer"),
    ("columns", "beamer"),
];

/// The package that defines `\name` (pass it without the backslash).
pub fn for_command(name: &str) -> Option<&'static str> {
    COMMANDS.iter().find(|(c, _)| *c == name).map(|(_, p)| *p)
}

/// The package that defines the environment `name`.
pub fn for_environment(name: &str) -> Option<&'static str> {
    ENVIRONMENTS.iter().find(|(e, _)| *e == name).map(|(_, p)| *p)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_common_ones() {
        assert_eq!(for_command("mathbb"), Some("amssymb"));
        assert_eq!(for_command("toprule"), Some("booktabs"));
        assert_eq!(for_environment("align"), Some("amsmath"));
        assert_eq!(for_command("nonexistent"), None);
    }

    #[test]
    fn no_duplicate_keys() {
        let mut seen = std::collections::HashSet::new();
        for (c, _) in COMMANDS {
            assert!(seen.insert(*c), "{c} listed twice");
        }
        let mut seen = std::collections::HashSet::new();
        for (e, _) in ENVIRONMENTS {
            assert!(seen.insert(*e), "{e} listed twice");
        }
    }
}
