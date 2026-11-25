#!/usr/bin/env bash
# Builds every project in tests/fixtures with the lemmata CLI and compares
# the diagnostics with tests/fixtures/<name>/expected.json. Each is built twice:
# trusted (through latexmk) and restricted (the engine run directly, the CLI's
# default), which compares with expected-restricted.json where the two differ.
#
#   scripts/check-fixtures.sh target/debug/lemmata           compare
#   scripts/check-fixtures.sh target/debug/lemmata --update  rewrite expected.json
set -euo pipefail

cli=$(realpath "$1")
update=${2:-}
root=$(cd "$(dirname "$0")/.." && pwd)
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
failed=0

for mode in trusted restricted; do
for dir in "$root"/tests/fixtures/*/; do
  name=$(basename "$dir")
  # Only projects have an expected.json (logs/ and prose/ hold other test data).
  [[ -f $dir/expected.json ]] || continue
  trust=--trust
  expected="$dir/expected.json"
  if [[ $mode == restricted ]]; then
    trust=
    [[ -f $dir/expected-restricted.json ]] && expected="$dir/expected-restricted.json"
  fi
  # Build in a temporary copy so tests/fixtures stays completely clean even if interrupted.
  copy="$work/$mode-$name"
  rm -rf "$copy"
  cp -R "$dir" "$copy"
  rm -rf "$copy/.lemmata"
  # Compile errors are expected for some fixtures; only the diagnostics matter.
  # Keep what the project itself causes: every error, and warnings located
  # in project files. Package chatter, system paths and bad boxes (which
  # depend on fonts and hyphenation) differ between TeX Live versions.
  actual=$("$cli" $trust build "$copy" --json 2>/dev/null \
    | python3 -I -c 'import json,sys
d=json.load(sys.stdin)
keep=[]
for x in d:
    if x["file"] and not x["file"].startswith("./"): x["file"]=None
    if x["severity"]!="error" and (x["file"] is None or x["line"] is None): continue
    if x["kind"]["type"]=="bad-box": continue
    keep.append(x)
print(json.dumps(keep, indent=2, sort_keys=True))' || true)
  rm -rf "$copy"
  if [[ $update == --update ]]; then
    # Restricted output gets a file of its own only where it differs.
    if [[ $mode == trusted ]] || ! diff -q "$dir/expected.json" <(printf '%s\n' "$actual") >/dev/null; then
      [[ $mode == restricted ]] && expected="$dir/expected-restricted.json"
      printf '%s\n' "$actual" > "$expected"
      echo "updated $name ($mode)"
    else
      # Restricted now equals trusted: an older file of its own would fail the comparison for good.
      rm -f "$dir/expected-restricted.json"
    fi
  elif ! diff -u "$expected" <(printf '%s\n' "$actual"); then
    echo "FAIL $name ($mode)"
    failed=1
  else
    echo "ok   $name ($mode)"
  fi
done
done
exit $failed
