"""Checks finalize-release.py against the file names Tauri's bundler produces."""

import json
import subprocess
import sys
import tempfile
from pathlib import Path

SCRIPT = Path(__file__).with_name("finalize-release.py")

TAURI_OUTPUT = {
    "linux_x86_64": [
        "bundle/appimage/Lemmata_0.3.0_amd64.AppImage",
        "bundle/appimage/Lemmata_0.3.0_amd64.AppImage.sig",
        "bundle/deb/Lemmata_0.3.0_amd64.deb",
        "bundle/rpm/Lemmata-0.3.0-1.x86_64.rpm",
    ],
    "macos_universal": [
        "bundle/dmg/Lemmata_0.3.0_universal.dmg",
        "bundle/macos/Lemmata.app.tar.gz",
        "bundle/macos/Lemmata.app.tar.gz.sig",
    ],
    "windows_x86_64": [
        "bundle/nsis/Lemmata_0.3.0_x64-setup.exe",
        "bundle/nsis/Lemmata_0.3.0_x64-setup.exe.sig",
        "bundle/msi/Lemmata_0.3.0_x64_en-US.msi",
    ],
}


def run(version: str, tag: str) -> Path:
    tmp = Path(tempfile.mkdtemp())
    for label, files in TAURI_OUTPUT.items():
        for rel in files:
            p = tmp / "artifacts" / label / rel
            p.parent.mkdir(parents=True, exist_ok=True)
            p.write_text("sig-" + p.name if p.name.endswith(".sig") else "bin")
    subprocess.run(
        [sys.executable, SCRIPT, tmp / "artifacts", tmp / "out", version, tag, "NilsBriggen/lemmata"],
        check=True,
        capture_output=True,
    )
    return tmp / "out"


def test_stable_release() -> None:
    out = run("0.3.0", "v0.3.0")
    names = sorted(p.name for p in out.iterdir())
    for expected in [
        "Lemmata_0.3.0_linux_x86_64.AppImage",
        "Lemmata_0.3.0_linux_x86_64.deb",
        "Lemmata_0.3.0_linux_x86_64.rpm",
        "Lemmata_0.3.0_macos_universal.dmg",
        "Lemmata_0.3.0_macos_universal.app.tar.gz",
        "Lemmata_0.3.0_windows_x86_64-setup.exe",
        "Lemmata_0.3.0_windows_x86_64.msi",
        "SHA256SUMS.txt",
        "latest.json",
    ]:
        assert expected in names, f"{expected} missing from {names}"
    manifest = json.loads((out / "latest.json").read_text())
    assert set(manifest["platforms"]) == {"linux-x86_64", "darwin-aarch64", "darwin-x86_64", "windows-x86_64"}
    assert manifest["platforms"]["windows-x86_64"]["url"].endswith("/v0.3.0/Lemmata_0.3.0_windows_x86_64-setup.exe")
    assert manifest["platforms"]["windows-x86_64"]["signature"] == "sig-Lemmata_0.3.0_x64-setup.exe.sig"
    sums = (out / "SHA256SUMS.txt").read_text()
    assert "Lemmata_0.3.0_linux_x86_64.deb" in sums


def test_prerelease_never_writes_the_updater_manifest() -> None:
    out = run("0.3.0-beta.1", "v0.3.0-beta.1")
    assert not (out / "latest.json").exists()
    assert not (out / "beta.json").exists()
    # The installers are still published.
    assert (out / "Lemmata_0.3.0-beta.1_linux_x86_64.AppImage").exists()


if __name__ == "__main__":
    test_stable_release()
    test_prerelease_never_writes_the_updater_manifest()
    print("ok")
