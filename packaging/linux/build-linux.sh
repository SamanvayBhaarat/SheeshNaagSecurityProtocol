#!/bin/bash
# Builds Sheshnag for Linux: a .deb package and a portable .tar.gz with install.sh.
# Run on Linux (CI uses ubuntu-22.04). Needs: cargo, dpkg-deb, rsvg-convert (librsvg2-bin).
#
# Build dependencies (Debian/Ubuntu):
#   sudo apt-get install -y build-essential pkg-config libxkbcommon-dev libwayland-dev \
#     libx11-dev libxcursor-dev libxrandr-dev libxi-dev libgl1-mesa-dev libegl1-mesa-dev \
#     librsvg2-bin
set -euo pipefail

ROOT=$(cd "$(dirname "$0")/../.." && pwd)
cd "$ROOT"
VERSION=$(grep -m1 '^version' Cargo.toml | cut -d'"' -f2)
BIN=sheshnag-ui
ARCH_DEB=$(dpkg --print-architecture)
DIST="$ROOT/dist"
WORK="$ROOT/target/linux-package"
rm -rf "$WORK"; mkdir -p "$WORK" "$DIST"

cargo build --release --locked

write_desktop() {
cat > "$1" <<DESK
[Desktop Entry]
Type=Application
Name=Sheshnag
Comment=Three-layer security watcher
Exec=sheshnag
Icon=sheshnag
Terminal=false
Categories=System;Security;
DESK
}

# ---- .deb ----
PKG="$WORK/deb"
mkdir -p "$PKG/DEBIAN" "$PKG/usr/bin" "$PKG/usr/share/applications" \
         "$PKG/usr/share/icons/hicolor/256x256/apps" "$PKG/usr/share/doc/sheshnag"
install -m 0755 "target/release/$BIN" "$PKG/usr/bin/sheshnag"
rsvg-convert -w 256 -h 256 packaging/assets/icon.svg -o "$PKG/usr/share/icons/hicolor/256x256/apps/sheshnag.png"
write_desktop "$PKG/usr/share/applications/sheshnag.desktop"
cp README.md "$PKG/usr/share/doc/sheshnag/README.md"
cat > "$PKG/DEBIAN/control" <<CTRL
Package: sheshnag
Version: $VERSION
Section: utils
Priority: optional
Architecture: $ARCH_DEB
Depends: libc6, libxkbcommon0, libgl1, libwayland-client0
Maintainer: Sheshnag <noreply@example.invalid>
Description: Three-layer security watcher dashboard
 Sakshi watches, Agni destroys a confirmed target, Kavach shields.
 Beta on Linux: systemd user units, autostart entries, cron and shell
 start-up files are watched. Nothing is destroyed without confirmation.
CTRL
dpkg-deb --build --root-owner-group "$PKG" "$DIST/sheshnag_${VERSION}_${ARCH_DEB}.deb"

# ---- portable tarball ----
TAR="$WORK/sheshnag-$VERSION-linux"
mkdir -p "$TAR"
install -m 0755 "target/release/$BIN" "$TAR/sheshnag"
cp packaging/assets/icon.png "$TAR/sheshnag.png"
write_desktop "$TAR/sheshnag.desktop"
cat > "$TAR/install.sh" <<'INST'
#!/bin/sh
# Per-user install, no root needed. Remove with: rm ~/.local/bin/sheshnag ~/.local/share/applications/sheshnag.desktop
set -e
HERE=$(cd "$(dirname "$0")" && pwd)
mkdir -p "$HOME/.local/bin" "$HOME/.local/share/applications" "$HOME/.local/share/icons/hicolor/512x512/apps"
install -m 0755 "$HERE/sheshnag" "$HOME/.local/bin/sheshnag"
cp "$HERE/sheshnag.png" "$HOME/.local/share/icons/hicolor/512x512/apps/sheshnag.png"
sed "s|^Exec=.*|Exec=$HOME/.local/bin/sheshnag|" "$HERE/sheshnag.desktop" > "$HOME/.local/share/applications/sheshnag.desktop"
echo "Installed. Launch Sheshnag from your applications menu or run: sheshnag"
INST
chmod +x "$TAR/install.sh"
tar -C "$WORK" -czf "$DIST/sheshnag-$VERSION-linux-$(uname -m).tar.gz" "sheshnag-$VERSION-linux"

(cd "$DIST" && sha256sum sheshnag_* sheshnag-*linux* > "SHA256SUMS-linux.txt")
echo "Done. Artifacts in $DIST"
