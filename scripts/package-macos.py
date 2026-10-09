#!/usr/bin/env python3
"""Build and ad-hoc sign the macOS app for the current host architecture."""
import json
import os
from pathlib import Path
import plistlib
import shutil
import subprocess
import sys
import tempfile


def find_cargo():
    candidate = os.environ.get("CARGO") or shutil.which("cargo")
    if candidate:
        return candidate
    fallback = Path.home() / ".cargo/bin/cargo"
    if fallback.is_file():
        return str(fallback)
    raise SystemExit("Cargo not found. Install Rust and add cargo to PATH.")


def ignore_vanished_metadata(func, path, exc_info):
    # On external volumes, removing a file can also remove its AppleDouble twin.
    if isinstance(exc_info[1], FileNotFoundError) and Path(path).name.startswith("._"):
        return
    raise exc_info[1]


def main():
    if sys.platform != "darwin":
        raise SystemExit("This packaging script requires macOS.")
    root = Path(__file__).resolve().parent.parent
    cargo = find_cargo()
    config = json.loads((root / "tauri.conf.json").read_text())
    metadata = json.loads(subprocess.check_output(
        [cargo, "metadata", "--no-deps", "--format-version", "1", "--offline", "--locked"],
        cwd=root, text=True,
    ))
    package = next(p for p in metadata["packages"]
                   if Path(p["manifest_path"]).resolve() == root / "Cargo.toml")
    version = package["version"]
    subprocess.run([cargo, "build", "--release", "--locked"], cwd=root, check=True)
    app = root / "dist" / f'{config["productName"]}.app'
    if app.exists():
        shutil.rmtree(app, onerror=ignore_vanished_metadata)
    macos = app / "Contents/MacOS"
    resources = app / "Contents/Resources"
    macos.mkdir(parents=True)
    resources.mkdir(parents=True)
    binary = Path(metadata["target_directory"]) / "release/claude-guard"
    shutil.copy2(binary, macos / "claude-guard")
    with tempfile.TemporaryDirectory(prefix="cg-icon-") as temp:
        iconset = Path(temp) / "AppIcon.iconset"
        iconset.mkdir()
        for size in [16, 32, 128, 256, 512]:
            for scale in [1, 2]:
                filename = f"icon_{size}x{size}" + ("@2x" if scale == 2 else "") + ".png"
                subprocess.run([
                    "/usr/bin/sips", "-z", str(size * scale), str(size * scale),
                    str(root / "assets/icon.png"), "--out", str(iconset / filename),
                ], check=True, stdout=subprocess.DEVNULL)
        subprocess.run([
            "/usr/bin/iconutil", "-c", "icns", str(iconset),
            "-o", str(resources / "AppIcon.icns"),
        ], check=True)
    subprocess.run([
        "/usr/bin/xcrun", "swiftc", "-swift-version", "5", "-target",
        subprocess.check_output(["/usr/bin/uname", "-m"], text=True).strip() + "-apple-macos12.0",
        "-o", str(macos / "claudeguard-network"),
        str(root / "network-extension/Policy.swift"), str(root / "network-extension/Controller.swift"),
        "-framework", "NetworkExtension", "-framework", "SystemExtensions",
    ], check=True)
    shutil.copytree(root / "browser-extension", resources / "BrowserProtection",
                    ignore=shutil.ignore_patterns("._*"))
    info = dict(
        CFBundleName=config["productName"], CFBundleDisplayName=config["productName"],
        CFBundleIdentifier=config["identifier"], CFBundleExecutable="claude-guard",
        CFBundlePackageType="APPL", CFBundleShortVersionString=version,
        CFBundleVersion=version, CFBundleIconFile="AppIcon", LSMinimumSystemVersion="12.0",
        NSHighResolutionCapable=True, CGNetworkProvisioned=False,
    )
    (app / "Contents/Info.plist").write_bytes(plistlib.dumps(info))
    subprocess.run(["/usr/bin/codesign", "--force", "--sign", "-", str(macos / "claudeguard-network")], check=True)
    # Generated app contents only: AppleDouble sidecars are not app resources.
    for sidecar in app.rglob("._*"):
        try:
            if sidecar.is_file() and sidecar.read_bytes()[:4] == b"\x00\x05\x16\x07":
                sidecar.unlink()
        except FileNotFoundError:
            pass
    subprocess.run(["/usr/bin/codesign", "--force", "--sign", "-", str(app)], check=True)
    subprocess.run(["/usr/bin/codesign", "--verify", "--strict", str(app)], check=True)
    print(app)


if __name__ == "__main__":
    main()
