#!/usr/bin/env bash
# Downloads the pinned texlab release for a Rust target triple into
# apps/desktop/src-tauri/binaries/texlab-<triple>[.exe], where Tauri's
# externalBin expects it. Checksums are pinned; bump them with the version.
#
#   scripts/fetch-texlab.sh x86_64-unknown-linux-gnu
#   scripts/fetch-texlab.sh universal-apple-darwin   (needs lipo, i.e. macOS)
set -euo pipefail

VERSION=v5.26.0
# A case statement, not an associative array: macOS ships bash 3.2.
sha256_for() {
  case $1 in
    x86_64-linux.tar.gz) echo 8697bd5e479d4584b14b7eed5c320c80ec4e1d91ebefbb6801e6bf38e9971300 ;;
    aarch64-linux.tar.gz) echo a85cdfcd22454b8d8550f4b0f0620c45ab51760f302fac7a12bc18a890f70f8c ;;
    x86_64-macos.tar.gz) echo 6091611f756b28e1a57612b130c196df4b0bb6e22dde5cf5d890578513397daf ;;
    aarch64-macos.tar.gz) echo af7972ffd230711ba04ada9b69cc32ce9111d9196ba69538062872faefdbee56 ;;
    x86_64-windows.zip) echo cb028d44c3d2b85d36a2ed52d41a0ff43a341b1f04c500c56c4524c4eb72b316 ;;
  esac
}

triple=${1:?usage: fetch-texlab.sh <rust-target-triple>}
root=$(cd "$(dirname "$0")/.." && pwd)
out="$root/apps/desktop/src-tauri/binaries"
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
mkdir -p "$out"

fetch() { # asset -> extracted binary path
  local asset=$1
  curl -sfL -o "$work/$asset" "https://github.com/latex-lsp/texlab/releases/download/$VERSION/texlab-$asset"
  local sum
  sum=$( (sha256sum "$work/$asset" 2>/dev/null || shasum -a 256 "$work/$asset") | cut -d' ' -f1)
  [[ $sum == "$(sha256_for "$asset")" ]] || { echo "checksum mismatch for $asset" >&2; exit 1; }
  mkdir -p "$work/${asset%%.*}"
  case $asset in
    *.zip) unzip -q -o "$work/$asset" -d "$work/${asset%%.*}" ;;
    *) tar xzf "$work/$asset" -C "$work/${asset%%.*}" ;;
  esac
  # Strip only now: the checksum above covers the downloaded archive, not the binary.
  case $asset in
    *linux*) strip "$work/${asset%%.*}/texlab" 2>/dev/null || true ;;
    *macos*) strip -x "$work/${asset%%.*}/texlab" 2>/dev/null || true ;;
  esac
  ls "$work/${asset%%.*}"/texlab*
}

case $triple in
  x86_64-unknown-linux-gnu) cp "$(fetch x86_64-linux.tar.gz)" "$out/texlab-$triple" ;;
  aarch64-unknown-linux-gnu) cp "$(fetch aarch64-linux.tar.gz)" "$out/texlab-$triple" ;;
  x86_64-apple-darwin) cp "$(fetch x86_64-macos.tar.gz)" "$out/texlab-$triple" ;;
  aarch64-apple-darwin) cp "$(fetch aarch64-macos.tar.gz)" "$out/texlab-$triple" ;;
  universal-apple-darwin)
    # Tauri builds each architecture separately and then merges them, so it
    # needs the per-architecture binaries as well as the universal one.
    x86=$(fetch x86_64-macos.tar.gz)
    arm=$(fetch aarch64-macos.tar.gz)
    cp "$x86" "$out/texlab-x86_64-apple-darwin"
    cp "$arm" "$out/texlab-aarch64-apple-darwin"
    lipo -create -output "$out/texlab-$triple" "$x86" "$arm" ;;
  x86_64-pc-windows-msvc) cp "$(fetch x86_64-windows.zip)" "$out/texlab-$triple.exe" ;;
  *) echo "no texlab build for $triple" >&2; exit 1 ;;
esac
chmod +x "$out"/texlab-* 2>/dev/null || true
echo "texlab $VERSION -> $out"
