#!/usr/bin/env bash
# Usage: scripts/release.sh 0.3.0        (or 0.3.0-beta.1)
# Bumps every version field, adds the new version to CHANGELOG.md, commits and tags.
# Push with `git push --follow-tags` to start .github/workflows/release.yml.
set -euo pipefail

version=${1:?usage: scripts/release.sh <version>}
[[ $version =~ ^[0-9]+\.[0-9]+\.[0-9]+(-[0-9A-Za-z.]+)?$ ]] || { echo "not a semver version: $version" >&2; exit 1; }
cd "$(dirname "$0")/.."

[[ -z $(git status --porcelain) ]] || { echo "commit or stash your changes first" >&2; exit 1; }
command -v git-cliff >/dev/null || { echo "install git-cliff (pacman -S git-cliff or cargo install git-cliff)" >&2; exit 1; }

sed -i.bak -E "0,/^version = \".*\"/s//version = \"$version\"/" Cargo.toml && rm Cargo.toml.bak
sed -i.bak -E "s/\"version\": \"[^\"]*\"/\"version\": \"$version\"/" apps/desktop/package.json && rm apps/desktop/package.json.bak
cargo update --workspace --offline >/dev/null 2>&1 || cargo update --workspace >/dev/null

# Only the new version's section is added on top, so sections edited by hand stay as they are.
section=$(git cliff --unreleased --tag "v$version" --strip header)
{ head -n 3 CHANGELOG.md; echo "$section"; echo; tail -n +4 CHANGELOG.md; } > CHANGELOG.md.new && mv CHANGELOG.md.new CHANGELOG.md
git add Cargo.toml Cargo.lock apps/desktop/package.json CHANGELOG.md
git commit -m "chore(release): v$version"
git tag -a "v$version" -m "Lemmata $version"
echo "Tagged v$version. Push with: git push --follow-tags"
