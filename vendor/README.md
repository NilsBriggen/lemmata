# Vendored crates

**hayro-interpret 0.8.0** (MIT or Apache-2.0, https://github.com/LaurenzV/hayro), used through
`[patch.crates-io]` in the workspace `Cargo.toml`. One change: `Type3Glyph::char_code()` and
`Type3Glyph::advance_width()`, so the PDF view can read text from TeX's bitmap (Type 3) fonts,
which have no `ToUnicode` map, and place it (`apps/lemmata/src/pdf/engine.rs`, `tex_bitmap_text`). Drop this copy once hayro has an equivalent.
