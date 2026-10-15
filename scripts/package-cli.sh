#!/usr/bin/env bash
# Builds the command-line tool for one platform and packs it for a release:
# dist/lemmata-cli_<version>_<label>.tar.gz (.zip on Windows) with the binary and
# the license. Used by .github/workflows/release.yml, and by CI so it is tested
# on every push, not first at a release.
#
#   scripts/package-cli.sh <version> <label> <target>
#   scripts/package-cli.sh 2.0.0 linux_x86_64 x86_64-unknown-linux-gnu
#   scripts/package-cli.sh 2.0.0 macos_universal universal    # both Mac architectures in one binary
set -euo pipefail

version=${1:?version}
label=${2:?label}
target=${3:?target}
cd "$(dirname "$0")/.."

name="lemmata-cli_${version}_${label}"
out="dist/$name"
rm -rf "$out" "dist/$name.tar.gz" "dist/$name.zip"
mkdir -p "$out"

exe=lemmata
[[ $target == *windows* ]] && exe=lemmata.exe

if [[ $target == universal ]]; then
  cargo build --release -p lemmata-cli --target aarch64-apple-darwin
  cargo build --release -p lemmata-cli --target x86_64-apple-darwin
  lipo -create -output "$out/$exe" \
    target/aarch64-apple-darwin/release/$exe target/x86_64-apple-darwin/release/$exe
  # Apple silicon only runs signed code; the combined file gets an ad hoc signature.
  codesign --force --sign - "$out/$exe"
else
  cargo build --release -p lemmata-cli --target "$target"
  cp "target/$target/release/$exe" "$out/"
fi
cp LICENSE "$out/"

# The packed binary must run: a broken build fails here, not on a user's machine.
"$out/$exe" --help >/dev/null

cd dist
if [[ $target == *windows* ]]; then
  7z a -bso0 "$name.zip" "$name"
else
  tar czf "$name.tar.gz" "$name"
fi
rm -rf "$name"
ls -l "$name".*
