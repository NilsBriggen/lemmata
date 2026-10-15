#!/usr/bin/env bash
# Regenerates templates/<id>/preview.png (page 1 of every template), which the New
# project gallery shows. Run it after changing a template, then commit the PNGs.
#
#   scripts/template-previews.sh target/debug/lemmata
#
# Needs pdftoppm (poppler) and the TeX packages of every template. Templates that can't
# be built here keep their old preview. The image is capped at 420 px on its longer
# side, which keeps even a pale A0 poster far below the 60 KB limit checked at the end.
set -euo pipefail

cli=$(realpath "$1")
root=$(cd "$(dirname "$0")/.." && pwd)
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
failed=0

for dir in "$root"/templates/*/; do
  name=$(basename "$dir")
  [ -f "$dir/template.toml" ] && [ "$name" != example ] || continue
  cp -r "$dir" "$work/$name"
  if "$cli" build "$work/$name" --json >/dev/null 2>&1 && pdf=$(ls "$work/$name"/.lemmata/build/*.pdf 2>/dev/null | head -1) && [ -n "$pdf" ]; then
    pdftoppm -png -f 1 -l 1 -scale-to 420 -singlefile "$pdf" "$dir/preview"
    echo "ok   $name ($(stat -c %s "$dir/preview.png") bytes)"
  else
    echo "skip $name (does not build here)"
  fi
done

for png in "$root"/templates/*/preview.png; do
  [ "$(stat -c %s "$png")" -lt 61440 ] || { echo "TOO BIG $png"; failed=1; }
done
exit $failed
