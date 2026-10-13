#!/usr/bin/env bash
# Usage: scripts/release.sh 0.3.0        (or 0.3.0-beta.1)
# Bumps every version field, regenerates CHANGELOG.md, commits and tags.
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

git cliff --tag "v$version" -o CHANGELOG.md
git add Cargo.toml Cargo.lock apps/desktop/package.json CHANGELOG.md
git commit -m "chore(release): v$version"
git tag -a "v$version" -m "Lemmata $version"
echo "Tagged v$version. Push with: git push --follow-tags"
