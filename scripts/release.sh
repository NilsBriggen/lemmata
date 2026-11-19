#!/usr/bin/env bash
# Usage: scripts/release.sh prepare 0.3.0    (or 0.3.0-rc.1)
#        scripts/release.sh tag 0.3.0
# prepare bumps every version field and adds the new version's section to CHANGELOG.md, without
# committing, so the section can be edited first. tag checks the section isn't blank, commits and
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
  sed -i.bak -E "0,/^version = \".*\"/s//version = \"$version\"/" Cargo.toml && rm Cargo.toml.bak
  cargo update --workspace --offline >/dev/null 2>&1 || cargo update --workspace >/dev/null
  # Only the new version's section is added on top, so sections edited by hand stay as they are.
  section=$(git cliff --unreleased --tag "v$version" --strip header)
  { head -n 3 CHANGELOG.md; echo; echo "$section"; tail -n +4 CHANGELOG.md; } > CHANGELOG.md.new && mv CHANGELOG.md.new CHANGELOG.md
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
