#!/usr/bin/env bash
# Builds the app's installers for one platform into target/packager: cargo-packager for most
# formats, cargo-generate-rpm for the .rpm. With CARGO_PACKAGER_SIGN_PRIVATE_KEY (and _PASSWORD)
# set, it also signs what the updater downloads: the AppImage, Lemmata.app.tar.gz, the NSIS setup.
# Used by .github/workflows/release.yml; scripts/finalize-release.py renames the results.
#
#   scripts/package-app.sh <target> <formats>
#   scripts/package-app.sh x86_64-unknown-linux-gnu appimage,deb,rpm
#   scripts/package-app.sh universal-apple-darwin app,dmg      # both Mac architectures in one app
#   scripts/package-app.sh x86_64-pc-windows-msvc nsis,wix
set -euo pipefail

target=${1:?target}
formats=${2:?formats}
cd "$(dirname "$0")/.."
out=target/packager
rm -rf "$out"

bin="target/$target/release"
if [[ $target == universal-apple-darwin ]]; then
  cargo build --release -p lemmata-app --target aarch64-apple-darwin
  cargo build --release -p lemmata-app --target x86_64-apple-darwin
  mkdir -p "$bin"
  lipo -create -output "$bin/lemmata-app" \
    target/aarch64-apple-darwin/release/lemmata-app target/x86_64-apple-darwin/release/lemmata-app
  # Apple silicon only runs signed code; without a Developer ID the program is signed ad hoc.
  codesign --force --sign - "$bin/lemmata-app"
else
  cargo build --release -p lemmata-app --target "$target"
fi

packager_formats=$(echo "$formats" | tr ',' '\n' | grep -vx rpm | paste -sd, -)
# AppImage tools are AppImages themselves; runners have no FUSE to mount them.
APPIMAGE_EXTRACT_AND_RUN=1 cargo packager -p lemmata-app --release \
  --binaries-dir "$bin" --formats "$packager_formats"

if [[ ,$formats, == *,rpm,* ]]; then
  cargo generate-rpm -p lemmata-app --target "$target" -o "$out/"
fi

# The updater replaces the whole bundle on macOS, from a tarball of it.
if [[ ,$formats, == *,app,* ]]; then
  app=$(find "$out" -maxdepth 2 -name '*.app' -type d | head -n1)
  tar -czf "$out/Lemmata.app.tar.gz" -C "$(dirname "$app")" "$(basename "$app")"
fi

if [[ -n ${CARGO_PACKAGER_SIGN_PRIVATE_KEY:-} ]]; then
  find "$out" -maxdepth 2 -type f \( -name '*.AppImage' -o -name '*.app.tar.gz' -o -name '*-setup.exe' \) \
    -exec cargo packager signer sign {} \;
fi
ls -l "$out"
