#!/bin/sh
# Baut target/Kadra.app (macOS). Ein App-Bundle ist nötig, damit macOS die
# Bildschirmaufnahme-Berechtigung dauerhaft der App zuordnet.
# Ist zusätzlich das Ziel x86_64-apple-darwin installiert, entsteht eine Universal-App.
set -e
cd "$(dirname "$0")"

VERSION=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)
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
  BINS="$BINS target/$t/release/kadra"
done

APP=target/Kadra.app
rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
if [ "$(echo $BINS | wc -w)" -gt 1 ]; then
  lipo -create $BINS -output "$APP/Contents/MacOS/Kadra"
  echo "Universal-Binary: $TARGETS"
else
  cp $BINS "$APP/Contents/MacOS/Kadra"
fi

# App-Icon erzeugen
ICONSET=target/Kadra.iconset
rm -rf "$ICONSET"
cargo run -q --release --example make_icon -- "$ICONSET"
iconutil -c icns "$ICONSET" -o "$APP/Contents/Resources/Kadra.icns"

cat > "$APP/Contents/Info.plist" <<EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleName</key><string>Kadra</string>
  <key>CFBundleDisplayName</key><string>Kadra</string>
  <key>CFBundleIdentifier</key><string>ch.kadra.app</string>
  <key>CFBundleVersion</key><string>$VERSION</string>
  <key>CFBundleShortVersionString</key><string>$VERSION</string>
  <key>CFBundleExecutable</key><string>Kadra</string>
  <key>CFBundleIconFile</key><string>Kadra</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>LSApplicationCategoryType</key><string>public.app-category.graphics-design</string>
  <key>LSMinimumSystemVersion</key><string>12.0</string>
  <key>NSHighResolutionCapable</key><true/>
  <key>NSScreenCaptureUsageDescription</key><string>Kadra benötigt Zugriff, um Bildschirmfotos zu erstellen.</string>
</dict>
</plist>
EOF

# Mit fester Identität signieren, sonst verliert macOS die Berechtigung bei jedem Neubau.
IDENTITY=${KADRA_SIGN_IDENTITY:-$(security find-identity -v -p codesigning 2>/dev/null | awk -F'"' '/"/{print $2; exit}')}
if [ -n "$IDENTITY" ]; then
  codesign --force --deep --options runtime -s "$IDENTITY" "$APP"
  echo "Signiert mit: $IDENTITY"
else
  codesign --force --deep -s - "$APP"
  echo "Ad-hoc signiert (Berechtigung muss nach jedem Neubau neu erteilt werden)"
fi
echo "Fertig: $APP ($VERSION)"
