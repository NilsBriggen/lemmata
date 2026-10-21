//! Maths symbols for the symbol palette and the maths completions (writing/symbols.ts):
//! category, the package they need, and the commands, written without the backslash.

const GROUPS: &[(&str, Option<&str>, &str)] = &[
    (
        "Greek",
        None,
        "alpha beta gamma delta epsilon varepsilon zeta eta theta vartheta iota kappa lambda mu nu xi pi varpi rho varrho sigma varsigma tau upsilon phi varphi chi psi omega Gamma Delta Theta Lambda Xi Pi Sigma Upsilon Phi Psi Omega",
    ),
    (
        "Relations",
        None,
        "leq geq neq approx equiv sim simeq cong propto ll gg prec succ preceq succeq subset supset subseteq supseteq in ni notin parallel perp mid models vdash dashv asymp doteq",
    ),
    (
        "Relations",
        Some("amssymb"),
        "leqslant geqslant lesssim gtrsim lessgtr nsubseteq nsupseteq subsetneq supsetneq nleq ngeq nparallel nmid triangleq thickapprox",
    ),
    (
        "Operators",
        None,
        "pm mp times div cdot ast star circ bullet oplus ominus otimes oslash odot cap cup vee wedge setminus sqcup sqcap uplus amalg diamond dagger ddagger",
    ),
    (
        "Big operators",
        None,
        "sum prod coprod int iint iiint oint bigcup bigcap bigvee bigwedge bigoplus bigotimes bigodot biguplus lim limsup liminf max min sup inf",
    ),
    (
        "Arrows",
        None,
        "leftarrow rightarrow leftrightarrow Leftarrow Rightarrow Leftrightarrow uparrow downarrow updownarrow Uparrow Downarrow longleftarrow longrightarrow longleftrightarrow Longleftarrow Longrightarrow Longleftrightarrow mapsto longmapsto hookrightarrow hookleftarrow nearrow searrow swarrow nwarrow rightharpoonup rightharpoondown leftharpoonup leftharpoondown rightleftharpoons leadsto",
    ),
    (
        "Arrows",
        Some("amssymb"),
        "rightsquigarrow twoheadrightarrow twoheadleftarrow rightrightarrows leftleftarrows circlearrowright circlearrowleft curvearrowright curvearrowleft nrightarrow nleftarrow nRightarrow",
    ),
    (
        "Sets and logic",
        None,
        "emptyset forall exists nexists neg lnot land lor top bot vdots ldots cdots ddots in notin subset cup cap setminus",
    ),
    (
        "Sets and logic",
        Some("amssymb"),
        "mathbb{R} mathbb{N} mathbb{Z} mathbb{Q} mathbb{C} mathbb{P} mathbb{E} mathbb{F} mathbb{H} mathbb{1} varnothing complement therefore because square blacksquare checkmark",
    ),
    (
        "Calculus",
        None,
        "partial nabla infty prime backprime Re Im aleph hbar ell wp angle triangle surd imath jmath",
    ),
    (
        "Functions",
        None,
        "sin cos tan cot sec csc arcsin arccos arctan sinh cosh tanh coth log ln lg exp det dim ker deg gcd hom arg Pr",
    ),
    (
        "Accents",
        None,
        "hat{x}|widehat{x}|check{x}|tilde{x}|widetilde{x}|bar{x}|overline{x}|underline{x}|vec{x}|dot{x}|ddot{x}|acute{x}|grave{x}|breve{x}|overrightarrow{x}|overbrace{x}|underbrace{x}",
    ),
    (
        "Delimiters",
        None,
        "langle rangle lceil rceil lfloor rfloor lvert rvert lVert rVert lbrace rbrace lbrack rbrack",
    ),
    (
        "Structures",
        None,
        "frac{a}{b}|dfrac{a}{b}|tfrac{a}{b}|binom{n}{k}|sqrt{x}|sqrt[n]{x}|sum_{i=1}^{n}|prod_{i=1}^{n}|int_{a}^{b}|lim_{x \\to 0}|left( x \\right)|left[ x \\right]|left\\{ x \\right\\}|mathrm{d}x|text{if}|operatorname{argmax}|stackrel{a}{=}|xrightarrow{f}|xleftarrow{f}",
    ),
    (
        "Scripts",
        None,
        "~x^{2}|~x_{i}|~x_{i}^{2}|~x^{\\prime}|~\\bar{x}_{i}",
    ),
    (
        "Fonts",
        None,
        "mathbf{x}|mathit{x}|mathrm{x}|mathsf{x}|mathtt{x}|mathcal{A}|mathfrak{A}|boldsymbol{x}|mathbb{A}",
    ),
    (
        "Spacing",
        None,
        "quad|qquad|,|;|:|!|hspace{1em}|phantom{x}|displaystyle|textstyle",
    ),
    (
        "Misc",
        None,
        "dots cdots ldots vdots ddots hbar degree dagger S P copyright pounds",
    ),
];

#[derive(Clone, Debug, PartialEq)]
pub struct Symbol {
    /// LaTeX source to insert, with the leading backslash.
    pub tex: String,
    pub group: &'static str,
    pub pkg: Option<&'static str>,
}

/// Each distinct command once per category, in category order.
pub fn symbols() -> &'static [Symbol] {
    static ALL: std::sync::OnceLock<Vec<Symbol>> = std::sync::OnceLock::new();
    ALL.get_or_init(|| {
        let mut seen = std::collections::HashSet::new();
        let mut out = Vec::new();
        for &(group, pkg, list) in GROUPS {
            let tokens: Vec<&str> = if list.contains('|') {
                list.split('|').collect()
            } else {
                list.split_whitespace().collect()
            };
            for t in tokens.into_iter().filter(|t| !t.is_empty()) {
                // A leading ~ marks a template that isn't a command.
                let tex = match t.strip_prefix('~') {
                    Some(rest) => rest.to_owned(),
                    None => format!("\\{t}"),
                };
                if seen.insert(format!("{group}:{tex}")) {
                    out.push(Symbol { tex, group, pkg });
                }
            }
        }
        out
    })
}

/// The categories, in order.
pub fn groups() -> Vec<&'static str> {
    let mut out: Vec<&'static str> = Vec::new();
    for s in symbols() {
        if !out.contains(&s.group) {
            out.push(s.group);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    // symbols.test.ts
    #[test]
    fn expands_commands_and_templates() {
        let all = symbols();
        assert!(all.iter().any(|s| s.tex == "\\alpha" && s.group == "Greek"));
        assert!(
            all.iter()
                .any(|s| s.tex == "\\mathbb{R}" && s.pkg == Some("amssymb"))
        );
        assert!(all.iter().any(|s| s.tex == "x^{2}" && s.group == "Scripts"));
        assert!(all.iter().any(|s| s.tex == "\\left( x \\right)"));
        // \in is listed twice in one category but appears once there.
        assert_eq!(
            all.iter()
                .filter(|s| s.tex == "\\in" && s.group == "Relations")
                .count(),
            1
        );
        assert_eq!(groups().first(), Some(&"Greek"));
    }
}
