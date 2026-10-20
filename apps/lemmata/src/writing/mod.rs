//! Writing aids: maths context and preview, symbols.

pub mod grammar;
pub mod images;
pub mod math;
pub mod prose;
pub mod render;
pub mod spell;
pub mod symbols;
pub mod table;

/// The language checks the editors share: spelling and grammar.
#[derive(Default)]
pub struct Checks {
    pub spell: spell::Spell,
    pub grammar: grammar::Grammar,
}
