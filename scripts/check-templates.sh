#!/usr/bin/env bash
# Builds every template (and the example document) from a clean copy with the
# lemmata CLI, so a template that no longer compiles can't ship. Outside CI a template
# whose TeX package isn't installed is skipped rather than failed.
#
#   scripts/check-templates.sh target/debug/lemmata
set -euo pipefail

cli=$(realpath "$1")
root=$(cd "$(dirname "$0")/.." && pwd)
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
failed=0

for dir in "$root"/templates/*/; do
  name=$(basename "$dir")
  cp -r "$dir" "$work/$name"
  rm -f "$work/$name/template.toml"
  if "$cli" build "$work/$name" --json >"$work/$name.json" 2>"$work/$name.err"; then
    echo "ok   $name"
  elif [ -z "${CI:-}" ] && grep -q '"type": "missing-file"' "$work/$name.json"; then
    # A publisher class (IEEEtran, acmart, ...) this machine doesn't have; CI installs them all.
    echo "skip $name (a TeX package is missing here)"
  else
    echo "FAIL $name"
    cat "$work/$name.err" "$work/$name.json" | head -40
    failed=1
  fi
done
exit $failed
