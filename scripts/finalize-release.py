#!/usr/bin/env python3
"""Collect the per-platform build artifacts into release assets.

Usage: finalize-release.py <artifacts-dir> <out-dir> <version> <tag> <repo>

<artifacts-dir> holds one folder per build job, named after the platform
label (linux_x86_64, linux_aarch64, macos_universal, windows_x86_64), each
containing that job's Tauri bundle folder. This script:

  * copies every installer to Lemmata_<version>_<label><ext>
  * writes latest.json for the Tauri updater when .sig files are present
  * writes SHA256SUMS.txt

The names are a contract with the website (docs/website-handoff.md) and the
package-manager templates in packaging/. Change them in all three places.
"""

import hashlib
import json
import shutil
import sys
from datetime import datetime, timezone
from pathlib import Path

# Suffix patterns from Tauri's bundler → the extension we publish.
KINDS = [
    (".AppImage", ".AppImage"),
    (".deb", ".deb"),
    (".rpm", ".rpm"),
    (".dmg", ".dmg"),
    (".app.tar.gz", ".app.tar.gz"),
    ("-setup.exe", "-setup.exe"),
    (".msi", ".msi"),
]

# Which published file each updater platform downloads.
UPDATER = {
    "linux_x86_64": [("linux-x86_64", ".AppImage")],
    "linux_aarch64": [("linux-aarch64", ".AppImage")],
    "macos_universal": [("darwin-aarch64", ".app.tar.gz"), ("darwin-x86_64", ".app.tar.gz")],
    "windows_x86_64": [("windows-x86_64", "-setup.exe")],
}


def kind_of(name: str):
    for suffix, ext in KINDS:
        if name.endswith(suffix):
            return ext
    return None


def main() -> None:
    artifacts, out, version, tag, repo = sys.argv[1:6]
    artifacts, out = Path(artifacts), Path(out)
    out.mkdir(parents=True, exist_ok=True)
    signatures: dict[tuple[str, str], str] = {}
    published: list[Path] = []

    for job in sorted(p for p in artifacts.iterdir() if p.is_dir()):
        label = job.name
        for f in sorted(job.rglob("*")):
            if not f.is_file():
                continue
            is_sig = f.name.endswith(".sig")
            ext = kind_of(f.name[:-4] if is_sig else f.name)
            if ext is None:
                continue
            target = out / f"Lemmata_{version}_{label}{ext}"
            if is_sig:
                signatures[(label, ext)] = f.read_text().strip()
                shutil.copy(f, target.with_name(target.name + ".sig"))
            else:
                if target.exists():
                    sys.exit(f"two {ext} files for {label}: {target.name}")
                shutil.copy(f, target)
                published.append(target)
            print(f"{f.relative_to(artifacts)} -> {target.name}{'.sig' if is_sig else ''}")

    if not published:
        sys.exit("no installers found")

    if signatures:
        base = f"https://github.com/{repo}/releases/download/{tag}"
        platforms = {}
        for label, entries in UPDATER.items():
            for platform, ext in entries:
                sig = signatures.get((label, ext))
                if sig:
                    platforms[platform] = {"signature": sig, "url": f"{base}/Lemmata_{version}_{label}{ext}"}
        manifest = {
            "version": version,
            "notes": f"https://github.com/{repo}/releases/tag/{tag}",
            "pub_date": datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ"),
            "platforms": platforms,
        }
        name = "beta.json" if "-" in version else "latest.json"
        (out / name).write_text(json.dumps(manifest, indent=2) + "\n")
        print(f"wrote {name} for {', '.join(platforms)}")
    else:
        print("no .sig files: skipping the updater manifest (TAURI_SIGNING_PRIVATE_KEY not set?)")

    lines = []
    for f in sorted(out.iterdir()):
        if f.name == "SHA256SUMS.txt":
            continue
        digest = hashlib.sha256(f.read_bytes()).hexdigest()
        lines.append(f"{digest}  {f.name}")
    (out / "SHA256SUMS.txt").write_text("\n".join(lines) + "\n")


if __name__ == "__main__":
    main()
