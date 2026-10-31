#!/usr/bin/env bash
# Packages a built `target/release/orbmc` for Linux:
#   orbmc-<version>-linux-x86_64.deb       Debian, Ubuntu, Mint
#   orbmc-<version>-linux-x86_64.rpm       Fedora, openSUSE
#   orbmc-<version>-linux-x86_64.AppImage  any distribution, no install
#   orbmc-<version>-linux-x86_64.tar.gz    the bare binary with its icon
# Usage (from the repository root): packaging/linux/package.sh <version> <out dir>
# Needs cargo-deb and cargo-generate-rpm; downloads appimagetool.
set -euo pipefail

version="$1"
out="$(mkdir -p "$2" && cd "$2" && pwd)"
base="orbmc-$version-linux-x86_64"
binary="target/release/orbmc"
desktop="packaging/linux/orbmc.desktop"
icon="assets/logo/orbmc.png"
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT

test -x "$binary" || { echo "build $binary first" >&2; exit 1; }

# Package metadata (files, dependencies) is in app/Cargo.toml.
cargo deb -p orbmc --no-build -o "$out/$base.deb"
cargo generate-rpm -p app -o "$out/$base.rpm"

tarball="$work/$base"
mkdir -p "$tarball"
cp "$binary" "$desktop" "$icon" LICENSE "$tarball/"
tar -C "$work" -czf "$out/$base.tar.gz" "$base"

appdir="$work/OrbMC.AppDir"
mkdir -p "$appdir/usr/bin" "$appdir/usr/share/applications" "$appdir/usr/share/icons/hicolor/256x256/apps"
cp "$binary" "$appdir/usr/bin/orbmc"
cp "$desktop" "$appdir/usr/share/applications/orbmc.desktop"
cp "$icon" "$appdir/usr/share/icons/hicolor/256x256/apps/orbmc.png"
cp "$desktop" "$icon" "$appdir/"
ln -s usr/bin/orbmc "$appdir/AppRun"
curl -fsSL -o "$work/appimagetool" \
  https://github.com/AppImage/appimagetool/releases/download/continuous/appimagetool-x86_64.AppImage
chmod +x "$work/appimagetool"
# Runners have no FUSE: the tool unpacks itself instead of mounting.
APPIMAGE_EXTRACT_AND_RUN=1 ARCH=x86_64 "$work/appimagetool" "$appdir" "$out/$base.AppImage"

ls -l "$out"
