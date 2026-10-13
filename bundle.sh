#!/bin/sh
# Baut target/SnapIt.app (macOS). Ein App-Bundle ist nötig, damit macOS die
# Bildschirmaufnahme-Berechtigung dauerhaft der App zuordnet.
# Ist zusätzlich das Ziel x86_64-apple-darwin installiert, entsteht eine Universal-App.
set -e
cd "$(dirname "$0")"

VERSION=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)
# Anzeigeversion ohne „.0“ am Ende (0.81.0 → 0.81)
SHORT_VERSION=$(echo "$VERSION" | sed 's/\.0$//')
HOST=$(rustc -vV | sed -n 's/^host: //p')
TARGETS="$HOST"
for t in aarch64-apple-darwin x86_64-apple-darwin; do
  if [ "$t" != "$HOST" ] && rustup target list --installed 2>/dev/null | grep -qx "$t"; then
    TARGETS="$TARGETS $t"
  fi
done

BINS=""
for t in $TARGETS; do
  cargo build --release --target "$t"
  BINS="$BINS target/$t/release/snapit"
done

APP=target/SnapIt.app
rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
if [ "$(echo $BINS | wc -w)" -gt 1 ]; then
  lipo -create $BINS -output "$APP/Contents/MacOS/SnapIt"
  echo "Universal-Binary: $TARGETS"
else
  cp $BINS "$APP/Contents/MacOS/SnapIt"
fi

# App-Icon erzeugen
ICONSET=target/SnapIt.iconset
rm -rf "$ICONSET"
cargo run -q --release --example make_icon -- "$ICONSET"
iconutil -c icns "$ICONSET" -o "$APP/Contents/Resources/SnapIt.icns"

cat > "$APP/Contents/Info.plist" <<EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleName</key><string>SnapIt</string>
  <key>CFBundleDisplayName</key><string>SnapIt</string>
  <key>CFBundleIdentifier</key><string>ch.snapit.app</string>
  <key>CFBundleVersion</key><string>$VERSION</string>
  <key>CFBundleShortVersionString</key><string>$SHORT_VERSION</string>
  <key>CFBundleExecutable</key><string>SnapIt</string>
  <key>CFBundleIconFile</key><string>SnapIt</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>LSApplicationCategoryType</key><string>public.app-category.graphics-design</string>
  <key>LSMinimumSystemVersion</key><string>12.0</string>
  <key>NSHighResolutionCapable</key><true/>
  <key>NSScreenCaptureUsageDescription</key><string>SnapIt benötigt Zugriff, um Bildschirmfotos zu erstellen.</string>
</dict>
</plist>
EOF

# Mit fester Identität signieren, sonst verliert macOS die Berechtigung bei jedem Neubau.
IDENTITY=${SNAPIT_SIGN_IDENTITY:-$(security find-identity -v -p codesigning 2>/dev/null | awk -F'"' '/"/{print $2; exit}')}
if [ -n "$IDENTITY" ]; then
  codesign --force --deep --options runtime -s "$IDENTITY" "$APP"
  echo "Signiert mit: $IDENTITY"
else
  codesign --force --deep -s - "$APP"
  echo "Ad-hoc signiert (Berechtigung muss nach jedem Neubau neu erteilt werden)"
fi
echo "Fertig: $APP ($SHORT_VERSION)"
