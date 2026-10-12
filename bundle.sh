#!/bin/sh
# Baut SnapIt.app (macOS). Ein App-Bundle ist nötig, damit macOS die
# Bildschirmaufnahme-Berechtigung dauerhaft der App zuordnet.
set -e
cd "$(dirname "$0")"
cargo build --release
APP=target/SnapIt.app
rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS"
cp target/release/snapit "$APP/Contents/MacOS/SnapIt"
cat > "$APP/Contents/Info.plist" <<EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleName</key><string>SnapIt</string>
  <key>CFBundleDisplayName</key><string>SnapIt</string>
  <key>CFBundleIdentifier</key><string>ch.snapit.app</string>
  <key>CFBundleVersion</key><string>0.1.0</string>
  <key>CFBundleShortVersionString</key><string>0.1.0</string>
  <key>CFBundleExecutable</key><string>SnapIt</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>LSMinimumSystemVersion</key><string>12.0</string>
  <key>NSHighResolutionCapable</key><true/>
  <key>NSScreenCaptureUsageDescription</key><string>SnapIt benötigt Zugriff, um Bildschirmfotos zu erstellen.</string>
</dict>
</plist>
EOF
# Mit fester Identität signieren, sonst verliert macOS die Berechtigung bei jedem Neubau.
IDENTITY=${SNAPIT_SIGN_IDENTITY:-$(security find-identity -v -p codesigning 2>/dev/null | awk -F'"' '/"/{print $2; exit}')}
if [ -n "$IDENTITY" ]; then
  codesign --force --deep -s "$IDENTITY" "$APP"
  echo "Signiert mit: $IDENTITY"
else
  codesign --force --deep -s - "$APP"
  echo "Ad-hoc signiert (Berechtigung muss nach jedem Neubau neu erteilt werden)"
fi
echo "Fertig: $APP"
