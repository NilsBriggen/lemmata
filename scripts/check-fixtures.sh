#!/usr/bin/env bash
# Builds every project in tests/fixtures with the lemmata CLI and compares
# the diagnostics with tests/fixtures/<name>/expected.json.
#
#   scripts/check-fixtures.sh target/debug/lemmata           compare
#   scripts/check-fixtures.sh target/debug/lemmata --update  rewrite expected.json
set -euo pipefail

cli=$(realpath "$1")
update=${2:-}
root=$(cd "$(dirname "$0")/.." && pwd)
failed=0

for dir in "$root"/tests/fixtures/*/; do
  name=$(basename "$dir")
  # Only projects have an expected.json (logs/ and prose/ hold other test data).
  [[ -f $dir/expected.json ]] || continue
  # Compile errors are expected for some fixtures; only the diagnostics matter.
  # Keep what the project itself causes: every error, and warnings located
  # in project files. Package chatter, system paths and bad boxes (which
  # depend on fonts and hyphenation) differ between TeX Live versions.
  actual=$("$cli" build "$dir" --json 2>/dev/null \
    | python3 -I -c 'import json,sys
d=json.load(sys.stdin)
keep=[]
for x in d:
    if x["file"] and not x["file"].startswith("./"): x["file"]=None
    if x["severity"]!="error" and (x["file"] is None or x["line"] is None): continue
    if x["kind"]["type"]=="bad-box": continue
    keep.append(x)
print(json.dumps(keep, indent=2, sort_keys=True))' || true)
  rm -rf "$dir/.lemmata"
  expected="$dir/expected.json"
  if [[ $update == --update ]]; then
    printf '%s\n' "$actual" > "$expected"
    echo "updated $name"
  elif ! diff -u "$expected" <(printf '%s\n' "$actual"); then
    echo "FAIL $name"
    failed=1
  else
    echo "ok   $name"
  fi
done
exit $failed
