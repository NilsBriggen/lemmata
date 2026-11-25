#!/usr/bin/env bash
# Builds the app's installers for one platform into target/packager: cargo-packager for most
# formats, cargo-generate-rpm for the .rpm. With CARGO_PACKAGER_SIGN_PRIVATE_KEY (and _PASSWORD)
# set, it also signs what the updater downloads: the AppImage, Lemmata.app.tar.gz, the NSIS setup.
# Used by .github/workflows/release.yml; scripts/finalize-release.py renames the results.
#
#   scripts/package-app.sh <target> <formats>
#   scripts/package-app.sh x86_64-unknown-linux-gnu appimage,deb,rpm
#   scripts/package-app.sh universal-apple-darwin app,dmg      # both Mac architectures in one app
#   PREBUILT=1 scripts/package-app.sh universal-apple-darwin app,dmg   # with target/<arch>/release built
#   scripts/package-app.sh x86_64-pc-windows-msvc nsis,wix
set -euo pipefail

target=${1:?target}
formats=${2:?formats}
cd "$(dirname "$0")/.."
out=target/packager
rm -rf "$out"

bin="target/$target/release"
if [[ $target == universal-apple-darwin ]]; then
  # PREBUILT: release.yml built each architecture on a runner of its own.
  if [[ -z ${PREBUILT:-} ]]; then
    cargo build --locked --release -p lemmata-app --target aarch64-apple-darwin
    cargo build --locked --release -p lemmata-app --target x86_64-apple-darwin
  fi
  mkdir -p "$bin"
  lipo -create -output "$bin/lemmata-app" \
    target/aarch64-apple-darwin/release/lemmata-app target/x86_64-apple-darwin/release/lemmata-app
  # Apple silicon only runs signed code; without a Developer ID the program is signed ad hoc.
  codesign --force --sign - "$bin/lemmata-app"
else
  cargo build --locked --release -p lemmata-app --target "$target"
fi

# cargo-packager takes the program from target/release, whatever was built for.
exe=lemmata-app
[[ $target == *windows* ]] && exe=lemmata-app.exe
mkdir -p target/release
[[ $bin == target/release ]] || cp "$bin/$exe" target/release/

packager_formats=$(echo "$formats" | tr ',' '\n' | grep -vx rpm | paste -sd, -)
# AppImage tools are AppImages themselves; runners have no FUSE to mount them.
APPIMAGE_EXTRACT_AND_RUN=1 cargo packager -p lemmata-app --release --formats "$packager_formats"

# cargo-packager writes the .deb with gzip; xz makes it about 28% smaller, and dpkg and the
# AUR's bsdtar read both.
for deb in "$out"/*.deb; do
  [[ -e $deb ]] || continue
  tmp=$(mktemp -d)
  dpkg-deb -R "$deb" "$tmp/pkg"
  dpkg-deb -Zxz -z9 --root-owner-group -b "$tmp/pkg" "$deb" >/dev/null
  rm -rf "$tmp"
done

if [[ ,$formats, == *,rpm,* ]]; then
  # RPM has no "-" in versions; a pre-release sorts before its release with "~".
  version=$(grep -m1 '^version' Cargo.toml | cut -d'"' -f2 | tr '-' '~')
  cargo generate-rpm -p apps/lemmata --target "$target" --payload-compress zstd -o "$out/" --set-metadata "version = \"$version\""
fi

# The updater replaces the whole bundle on macOS, from a tarball of it. A Mac only needs its own
# architecture, so the universal build (kept for the dmg) is split into one tarball per
# architecture, each half the size.
if [[ ,$formats, == *,app,* ]]; then
  app=$(find "$out" -maxdepth 2 -name '*.app' -type d | head -n1)
  if [[ $target == universal-apple-darwin ]]; then
    for arch in aarch64 x86_64; do
      tmp=$(mktemp -d)
      cp -R "$app" "$tmp/"
      thin="$tmp/$(basename "$app")/Contents/MacOS/lemmata-app"
      cp "target/$arch-apple-darwin/release/lemmata-app" "$thin"
      codesign --force --sign - "$thin"
      GZIP=-9 tar -czf "$out/Lemmata.$arch.app.tar.gz" -C "$tmp" "$(basename "$app")"
      if [[ ,$formats, == *,dmg,* ]]; then
        ln -s /Applications "$tmp/Applications"
        hdiutil create -fs HFS+ -format UDZO -volname Lemmata -srcfolder "$tmp" \
          -ov "$out/Lemmata.$arch.dmg" >/dev/null
      fi
      rm -rf "$tmp"
    done
  else
    GZIP=-9 tar -czf "$out/Lemmata.app.tar.gz" -C "$(dirname "$app")" "$(basename "$app")"
  fi
fi

# create-dmg writes zlib (UDZO); LZMA (ULMO, macOS 10.15+, the minimum already) is smaller.
for dmg in "$out"/*.dmg; do
  [[ -e $dmg ]] || continue
  hdiutil convert "$dmg" -format ULMO -o "$dmg.ulmo.dmg" >/dev/null
  mv "$dmg.ulmo.dmg" "$dmg"
done

# Every installer carries the templates and the dictionaries (resources in apps/lemmata/Cargo.toml):
# without them New project is empty and spell checking is off.
need="$(ls templates) dictionaries"
carries() {
  local listing
  listing=$(tr '\\' '/' <<<"$2")
  for n in $need; do
    grep -Eq "(^|[[:space:]/])$n/" <<<"$listing" || { echo "$1 lacks $n/" >&2; exit 1; }
  done
}
for deb in "$out"/*.deb; do
  [[ -e $deb ]] || continue
  carries "$deb" "$(dpkg-deb -c "$deb")"
done
for rpm in "$out"/*.rpm; do
  [[ -e $rpm ]] || continue
  if command -v rpm >/dev/null; then carries "$rpm" "$(rpm -qlp "$rpm")"; else echo "::warning::no rpm: $rpm not inspected"; fi
done
for appimage in "$out"/*.AppImage; do
  [[ -e $appimage ]] || continue
  tmp=$(mktemp -d)
  if (cd "$tmp" && chmod +x "$appimage" && "$appimage" --appimage-extract >/dev/null 2>&1); then
    carries "$appimage" "$(find "$tmp/squashfs-root" -type d | sed 's|$|/|')"
  else
    echo "::warning::could not extract $appimage for inspection"
  fi
  rm -rf "$tmp"
done
for app in "$out"/*.app "$out"/*/*.app; do
  [[ -d $app ]] || continue
  carries "$app" "$(find "$app" -type d | sed 's|$|/|')"
done
for exe in "$out"/*-setup.exe "$out"/*.msi; do
  [[ -e $exe ]] || continue
  if command -v 7z >/dev/null; then carries "$exe" "$(7z l "$exe")"; else echo "::warning::no 7z: $exe not inspected"; fi
done

if [[ -n ${CARGO_PACKAGER_SIGN_PRIVATE_KEY:-} ]]; then
  # A loop, not `find -exec ... \;`, which exits 0 whatever the command did: a failed signing
  # must fail the build, not ship unsigned files.
  while IFS= read -r f; do
    cargo packager signer sign "$f"
  done < <(find "$out" -maxdepth 2 -type f \( -name '*.AppImage' -o -name '*.app.tar.gz' -o -name '*-setup.exe' \))
fi
ls -l "$out"
