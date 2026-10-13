#!/bin/sh
# Erzeugt target/SnapIt-<version>.dmg: SnapIt.app + Verknüpfung „Programme“ zum Hineinziehen.
set -e
cd "$(dirname "$0")"

./bundle.sh
VERSION=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1 | sed 's/\.0$//')
DMG="target/SnapIt-$VERSION.dmg"
STAGE=target/dmg-stage
RW=target/SnapIt-rw.dmg

rm -rf "$STAGE" "$RW" "$DMG"
# Alte, noch eingebundene SnapIt-Volumes lösen (sonst wird „SnapIt 1“ eingehängt)
for v in /Volumes/SnapIt*; do
  [ -d "$v" ] && hdiutil detach -quiet "$v" 2>/dev/null || true
done
mkdir -p "$STAGE"
cp -R target/SnapIt.app "$STAGE/"
ln -s /Applications "$STAGE/Programme"

# Beschreibbares Abbild anlegen, Finder-Fenster einrichten, dann komprimieren.
hdiutil create -quiet -volname "SnapIt" -srcfolder "$STAGE" -fs HFS+ -format UDRW -ov "$RW"
MOUNT=$(hdiutil attach -readwrite -noverify -noautoopen "$RW" | awk -F'\t' '/\/Volumes\//{print $NF; exit}')
VOLNAME=$(basename "$MOUNT")

# Fensterlayout (Icon-Ansicht, App links, Programme rechts). Schlägt ohne
# Automation-Berechtigung für den Finder fehl – das DMG funktioniert trotzdem.
osascript <<EOF >/dev/null 2>&1 || echo "Hinweis: Finder-Layout übersprungen (keine Automation-Berechtigung)."
tell application "Finder"
  tell disk "$VOLNAME"
    open
    set current view of container window to icon view
    set toolbar visible of container window to false
    set statusbar visible of container window to false
    set the bounds of container window to {200, 150, 760, 480}
    set opts to the icon view options of container window
    set arrangement of opts to not arranged
    set icon size of opts to 112
    set position of item "SnapIt.app" of container window to {150, 150}
    set position of item "Programme" of container window to {410, 150}
    update without registering applications
    delay 1
    close
  end tell
end tell
EOF

# Volume-Icon erst nach dem Finder-Schritt setzen (der Finder entfernt es sonst wieder)
cp target/SnapIt.app/Contents/Resources/SnapIt.icns "$MOUNT/.VolumeIcon.icns"
SetFile -a C "$MOUNT" 2>/dev/null || true

sync
hdiutil detach -quiet "$MOUNT" || hdiutil detach -force -quiet "$MOUNT"
hdiutil convert -quiet "$RW" -format UDZO -imagekey zlib-level=9 -o "$DMG"
rm -rf "$STAGE" "$RW"

IDENTITY=${SNAPIT_SIGN_IDENTITY:-$(security find-identity -v -p codesigning 2>/dev/null | awk -F'"' '/"/{print $2; exit}')}
[ -n "$IDENTITY" ] && codesign --force -s "$IDENTITY" "$DMG"

echo "DMG: $DMG ($(du -h "$DMG" | cut -f1))"
