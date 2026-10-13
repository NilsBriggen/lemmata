// Maths symbols for the symbol palette: [category, package needed, commands].
// Commands are written without the backslash and expanded by `SYMBOLS`.

const GROUPS: [string, string | null, string][] = [
  ["Greek", null, "alpha beta gamma delta epsilon varepsilon zeta eta theta vartheta iota kappa lambda mu nu xi pi varpi rho varrho sigma varsigma tau upsilon phi varphi chi psi omega Gamma Delta Theta Lambda Xi Pi Sigma Upsilon Phi Psi Omega"],
  ["Relations", null, "leq geq neq approx equiv sim simeq cong propto ll gg prec succ preceq succeq subset supset subseteq supseteq in ni notin parallel perp mid models vdash dashv asymp doteq"],
  ["Relations", "amssymb", "leqslant geqslant lesssim gtrsim lessgtr nsubseteq nsupseteq subsetneq supsetneq nleq ngeq nparallel nmid triangleq thickapprox"],
  ["Operators", null, "pm mp times div cdot ast star circ bullet oplus ominus otimes oslash odot cap cup vee wedge setminus sqcup sqcap uplus amalg diamond dagger ddagger"],
  ["Big operators", null, "sum prod coprod int iint iiint oint bigcup bigcap bigvee bigwedge bigoplus bigotimes bigodot biguplus lim limsup liminf max min sup inf"],
  ["Arrows", null, "leftarrow rightarrow leftrightarrow Leftarrow Rightarrow Leftrightarrow uparrow downarrow updownarrow Uparrow Downarrow longleftarrow longrightarrow longleftrightarrow Longleftarrow Longrightarrow Longleftrightarrow mapsto longmapsto hookrightarrow hookleftarrow nearrow searrow swarrow nwarrow rightharpoonup rightharpoondown leftharpoonup leftharpoondown rightleftharpoons leadsto"],
  ["Arrows", "amssymb", "rightsquigarrow twoheadrightarrow twoheadleftarrow rightrightarrows leftleftarrows circlearrowright circlearrowleft curvearrowright curvearrowleft nrightarrow nleftarrow nRightarrow"],
  ["Sets and logic", null, "emptyset forall exists nexists neg lnot land lor top bot vdots ldots cdots ddots in notin subset cup cap setminus"],
  ["Sets and logic", "amssymb", "mathbb{R} mathbb{N} mathbb{Z} mathbb{Q} mathbb{C} mathbb{P} mathbb{E} mathbb{F} mathbb{H} mathbb{1} varnothing complement therefore because square blacksquare checkmark"],
  ["Calculus", null, "partial nabla infty prime backprime Re Im aleph hbar ell wp angle triangle surd imath jmath"],
  ["Functions", null, "sin cos tan cot sec csc arcsin arccos arctan sinh cosh tanh coth log ln lg exp det dim ker deg gcd hom arg Pr"],
  ["Accents", null, "hat{x}|widehat{x}|check{x}|tilde{x}|widetilde{x}|bar{x}|overline{x}|underline{x}|vec{x}|dot{x}|ddot{x}|acute{x}|grave{x}|breve{x}|overrightarrow{x}|overbrace{x}|underbrace{x}"],
  ["Delimiters", null, "langle rangle lceil rceil lfloor rfloor lvert rvert lVert rVert lbrace rbrace lbrack rbrack"],
  ["Structures", null, "frac{a}{b}|dfrac{a}{b}|tfrac{a}{b}|binom{n}{k}|sqrt{x}|sqrt[n]{x}|sum_{i=1}^{n}|prod_{i=1}^{n}|int_{a}^{b}|lim_{x \\to 0}|left( x \\right)|left[ x \\right]|left\\{ x \\right\\}|mathrm{d}x|text{if}|operatorname{argmax}|stackrel{a}{=}|xrightarrow{f}|xleftarrow{f}"],
  ["Scripts", null, "~x^{2}|~x_{i}|~x_{i}^{2}|~x^{\\prime}|~\\bar{x}_{i}"],
  ["Fonts", null, "mathbf{x}|mathit{x}|mathrm{x}|mathsf{x}|mathtt{x}|mathcal{A}|mathfrak{A}|boldsymbol{x}|mathbb{A}"],
  ["Spacing", null, "quad|qquad|,|;|:|!|hspace{1em}|phantom{x}|displaystyle|textstyle"],
  ["Misc", null, "dots cdots ldots vdots ddots hbar degree dagger S P copyright pounds"],
];

export interface Symbol {
  /** LaTeX source to insert, with the leading backslash. */
  tex: string;
  group: string;
  pkg: string | null;
}

/** A leading ~ marks a template that isn't a command. */
const expand = (token: string) => (token.startsWith("~") ? token.slice(1) : `\\${token}`);

/** Each distinct command once, in category order. */
export const SYMBOLS: Symbol[] = (() => {
  const seen = new Set<string>();
  const out: Symbol[] = [];
  for (const [group, pkg, list] of GROUPS) {
    // Split on spaces that aren't inside braces/brackets or part of "left( x right)".
    const tokens = splitTokens(list);
    for (const t of tokens) {
      const tex = expand(t);
      const key = `${group}:${tex}`;
      if (seen.has(key)) continue;
      seen.add(key);
      out.push({ tex, group, pkg });
    }
  }
  return out;
})();

function splitTokens(list: string): string[] {
  return (list.includes("|") ? list.split("|") : list.split(/\s+/)).filter(Boolean);
}

export const GROUP_NAMES = [...new Set(SYMBOLS.map((s) => s.group))];
