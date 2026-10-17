#!/bin/bash
# Builds a universal (Apple Silicon + Intel) Sheshnag.app and a drag-to-Applications DMG.
#
#   packaging/macos/build-macos.sh
#
# Optional environment:
#   CODESIGN_IDENTITY  Developer ID Application identity (default: "-" = ad-hoc signature)
#   NOTARY_PROFILE     notarytool keychain profile; when set the DMG is notarized and stapled
set -euo pipefail

ROOT=$(cd "$(dirname "$0")/../.." && pwd)
cd "$ROOT"
VERSION=$(grep -m1 '^version' Cargo.toml | cut -d'"' -f2)
NAME=Sheshnag
BIN=sheshnag-ui
DIST="$ROOT/dist"
WORK="$ROOT/target/macos-package"
APP="$WORK/$NAME.app"
IDENTITY=${CODESIGN_IDENTITY:--}

rm -rf "$WORK"; mkdir -p "$WORK" "$DIST"

echo "==> Building both architectures"
rustup target add aarch64-apple-darwin x86_64-apple-darwin >/dev/null
cargo build --release --locked --target aarch64-apple-darwin
cargo build --release --locked --target x86_64-apple-darwin

echo "==> Creating universal binary"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
lipo -create \
  "target/aarch64-apple-darwin/release/$BIN" \
  "target/x86_64-apple-darwin/release/$BIN" \
  -output "$APP/Contents/MacOS/$BIN"

echo "==> Building icon"
ICONSET="$WORK/icon.iconset"; mkdir -p "$ICONSET"
for size in 16 32 128 256 512; do
  rsvg-convert -w $size -h $size packaging/assets/icon.svg -o "$ICONSET/icon_${size}x${size}.png"
  rsvg-convert -w $((size*2)) -h $((size*2)) packaging/assets/icon.svg -o "$ICONSET/icon_${size}x${size}@2x.png"
done
iconutil -c icns "$ICONSET" -o "$APP/Contents/Resources/$NAME.icns"

cat > "$APP/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
  <key>CFBundleName</key><string>$NAME</string>
  <key>CFBundleDisplayName</key><string>$NAME</string>
  <key>CFBundleIdentifier</key><string>in.palanhar.sheshnag.dashboard</string>
  <key>CFBundleExecutable</key><string>$BIN</string>
  <key>CFBundleIconFile</key><string>$NAME</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleShortVersionString</key><string>$VERSION</string>
  <key>CFBundleVersion</key><string>$VERSION</string>
  <key>LSMinimumSystemVersion</key><string>11.0</string>
  <key>LSApplicationCategoryType</key><string>public.app-category.utilities</string>
  <key>NSHighResolutionCapable</key><true/>
</dict></plist>
PLIST

echo "==> Signing ($IDENTITY)"
if [ "$IDENTITY" = "-" ]; then
  codesign --force --deep --sign - "$APP"
else
  codesign --force --deep --options runtime --timestamp --sign "$IDENTITY" "$APP"
fi
codesign --verify --deep --strict "$APP"

echo "==> Creating DMG"
STAGE="$WORK/dmg"; mkdir -p "$STAGE"
cp -R "$APP" "$STAGE/"
ln -s /Applications "$STAGE/Applications"
cat > "$STAGE/READ ME FIRST.txt" <<TXT
Sheshnag $VERSION

1. Drag Sheshnag into Applications.
2. Open it from Applications.

If macOS says the app cannot be verified, this build is not notarized yet.
Right-click Sheshnag, choose Open, then confirm. You only do this once.

Sheshnag only scans when you ask. It does not start at login.
TXT
DMG="$DIST/$NAME-$VERSION-macos-universal.dmg"
rm -f "$DMG"
hdiutil create -volname "$NAME $VERSION" -srcfolder "$STAGE" -ov -format UDZO "$DMG" >/dev/null
[ "$IDENTITY" = "-" ] || codesign --force --sign "$IDENTITY" --timestamp "$DMG"

if [ -n "${NOTARY_PROFILE:-}" ]; then
  echo "==> Notarizing"
  xcrun notarytool submit "$DMG" --keychain-profile "$NOTARY_PROFILE" --wait
  xcrun stapler staple "$DMG"
fi

(cd "$DIST" && shasum -a 256 "$(basename "$DMG")" > "$(basename "$DMG").sha256")
echo "==> Done: $DMG"
