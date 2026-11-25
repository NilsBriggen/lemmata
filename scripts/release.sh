#!/usr/bin/env bash
# Usage: scripts/release.sh prepare 0.3.0    (or 0.3.0-rc.1)
#        scripts/release.sh tag 0.3.0
# prepare bumps every version field and adds the new version's section to CHANGELOG.md, without
# committing, so the section can be edited first; a section written ahead as "## X.Y.Z (unreleased)"
# is kept and gets the date. tag checks the section isn't blank, commits and
# tags. Push with `git push --follow-tags` to start .github/workflows/release.yml.
set -euo pipefail

step=${1:-}
version=${2:-}
[[ $step == prepare || $step == tag ]] && [[ -n $version ]] || { echo "usage: scripts/release.sh prepare|tag <version>" >&2; exit 1; }
[[ $version =~ ^[0-9]+\.[0-9]+\.[0-9]+(-[0-9A-Za-z.]+)?$ ]] || { echo "not a semver version: $version" >&2; exit 1; }
cd "$(dirname "$0")/.."

if [[ $step == prepare ]]; then
  [[ -z $(git status --porcelain) ]] || { echo "commit or stash your changes first" >&2; exit 1; }
  command -v git-cliff >/dev/null || { echo "install git-cliff (pacman -S git-cliff or cargo install git-cliff)" >&2; exit 1; }
  python3 -I -c 'import pathlib, re, sys
p = pathlib.Path("Cargo.toml")
p.write_text(re.sub(r"(?m)^version = \".*\"", f"version = \"{sys.argv[1]}\"", p.read_text(), count=1))' "$version"
  cargo update --workspace --offline >/dev/null 2>&1 || cargo update --workspace >/dev/null
  if grep -q "^## $version (unreleased)" CHANGELOG.md; then
    # Written ahead of the release: it only gets today's date.
    python3 -I -c 'import datetime, pathlib, sys
p = pathlib.Path("CHANGELOG.md")
v = sys.argv[1]
today = datetime.date.today().isoformat()
p.write_text(p.read_text().replace(f"## {v} (unreleased)", f"## {v} ({today})", 1))' "$version"
  else
    # Only the new version's section is added on top, so sections edited by hand stay as they are.
    section=$(git cliff --unreleased --tag "v$version" --strip header)
    { head -n 3 CHANGELOG.md; echo; echo "$section"; tail -n +4 CHANGELOG.md; } > CHANGELOG.md.new && mv CHANGELOG.md.new CHANGELOG.md
  fi
  echo "Edit the $version section of CHANGELOG.md, then: scripts/release.sh tag $version"
  exit 0
fi

grep -q "^version = \"$version\"" Cargo.toml || { echo "Cargo.toml isn't at $version: run scripts/release.sh prepare $version first" >&2; exit 1; }
# The same section the release workflow publishes as the release notes.
notes=$(awk -v v="## $version " 'index($0, v) == 1 {on = 1; next} on && /^## / {exit} on' CHANGELOG.md)
grep -q '[^[:space:]]' <<<"$notes" || { echo "CHANGELOG.md has no section for $version, or it is blank" >&2; exit 1; }
others=$(git status --porcelain | grep -vE '^ ?M (Cargo\.toml|Cargo\.lock|CHANGELOG\.md)$' || true)
[[ -z $others ]] || { echo "only the version bump and the changelog may be uncommitted:"$'\n'"$others" >&2; exit 1; }
git add Cargo.toml Cargo.lock CHANGELOG.md
git commit -m "chore(release): v$version"
git tag -a "v$version" -m "Lemmata $version"
echo "Tagged v$version. Push with: git push --follow-tags"
