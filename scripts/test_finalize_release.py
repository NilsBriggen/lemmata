"""Checks finalize-release.py against the file names scripts/package-app.sh produces."""

import json
import subprocess
import sys
import tempfile
from pathlib import Path

SCRIPT = Path(__file__).with_name("finalize-release.py")

PACKAGER_OUTPUT = {
    "linux_x86_64": [
        "lemmata-app_0.3.0_x86_64.AppImage",
        "lemmata-app_0.3.0_x86_64.AppImage.sig",
        "lemmata-app_0.3.0_amd64.deb",
        "lemmata-0.3.0-1.x86_64.rpm",
    ],
    "macos_universal": [
        "Lemmata_0.3.0_universal.dmg",
        "Lemmata.app.tar.gz",
        "Lemmata.app.tar.gz.sig",
    ],
    "windows_x86_64": [
        "Lemmata_0.3.0_x64-setup.exe",
        "Lemmata_0.3.0_x64-setup.exe.sig",
        "Lemmata_0.3.0_x64_en-US.msi",
    ],
}


def run(version: str, tag: str, cli_name: str | None = None) -> Path:
    tmp = Path(tempfile.mkdtemp())
    for label, files in PACKAGER_OUTPUT.items():
        for rel in files:
            p = tmp / "artifacts" / label / rel
            p.parent.mkdir(parents=True, exist_ok=True)
            p.write_text("sig-" + p.name if p.name.endswith(".sig") else "bin")
    # The command-line archives, as scripts/package-cli.sh names them.
    for label, ext in [("linux_x86_64", ".tar.gz"), ("windows_x86_64", ".zip")]:
        p = tmp / "artifacts" / f"cli_{label}" / (cli_name or f"lemmata-cli_{version}_{label}{ext}")
        p.parent.mkdir(parents=True, exist_ok=True)
        p.write_text("cli")
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
        "lemmata-cli_0.3.0_linux_x86_64.tar.gz",
        "lemmata-cli_0.3.0_windows_x86_64.zip",
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
    assert "lemmata-cli_0.3.0_windows_x86_64.zip" in sums


def test_a_misnamed_command_line_archive_stops_the_release() -> None:
    try:
        run("0.3.0", "v0.3.0", cli_name="lemmata-cli_0.2.9_linux_x86_64.tar.gz")
    except subprocess.CalledProcessError:
        return
    raise AssertionError("an archive for another version was published")


def test_prerelease_never_writes_the_updater_manifest() -> None:
    out = run("0.3.0-beta.1", "v0.3.0-beta.1")
    assert not (out / "latest.json").exists()
    assert not (out / "beta.json").exists()
    # The installers are still published.
    assert (out / "Lemmata_0.3.0-beta.1_linux_x86_64.AppImage").exists()


if __name__ == "__main__":
    test_stable_release()
    test_a_misnamed_command_line_archive_stops_the_release()
    test_prerelease_never_writes_the_updater_manifest()
    print("ok")
