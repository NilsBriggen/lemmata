#!/usr/bin/env python3
"""Writes the UI fonts to apps/lemmata/assets/fonts as plain TTF.

epaint can't read WOFF, so this unwraps the same @fontsource latin and latin-ext
subsets the Svelte bundle ships. Run from the repo root after `pnpm install`:
    python3 scripts/gen-fonts.py
"""

from pathlib import Path

from fontTools.ttLib import TTFont

SRC = Path("apps/desktop/node_modules/@fontsource")
OUT = Path("apps/lemmata/assets/fonts")
FONTS = [
    ("young-serif", 400),
    ("ibm-plex-sans", 400),
    ("ibm-plex-sans", 500),
    ("ibm-plex-sans", 600),
    ("ibm-plex-mono", 400),
    ("ibm-plex-mono", 500),
]

OUT.mkdir(parents=True, exist_ok=True)
for family, weight in FONTS:
    for subset in ("latin", "latin-ext"):
        font = TTFont(SRC / family / "files" / f"{family}-{subset}-{weight}-normal.woff")
        font.flavor = None
        font.save(OUT / f"{family}-{subset}-{weight}.ttf")
print(f"{len(FONTS) * 2} fonts in {OUT}")
