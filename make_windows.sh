#!/bin/sh
# Baut Kadra für Windows (x64) per Cross-Compile auf macOS und erzeugt
#   target/Kadra-Setup-<version>.exe      Installer (NSIS, ohne Administratorrechte)
#   target/Kadra-<version>-windows-x64.zip portable Version
# Voraussetzungen: brew install mingw-w64 makensis && rustup target add x86_64-pc-windows-gnu
set -e
cd "$(dirname "$0")"

TARGET=x86_64-pc-windows-gnu
VERSION=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)
for tool in x86_64-w64-mingw32-gcc x86_64-w64-mingw32-windres makensis; do
  command -v "$tool" >/dev/null || { echo "Fehlt: $tool – brew install mingw-w64 makensis"; exit 1; }
done
rustup target list --installed | grep -q "$TARGET" || rustup target add "$TARGET"

[ -f assets/windows/kadra.ico ] || cargo run -q --example make_icon -- --ico assets/windows/kadra.ico

export CARGO_TARGET_X86_64_PC_WINDOWS_GNU_LINKER=x86_64-w64-mingw32-gcc
cargo build --release --target "$TARGET"

STAGE=target/windows-stage
EXE="$STAGE/kadra.exe"
rm -rf "$STAGE" && mkdir -p "$STAGE"
x86_64-w64-mingw32-strip -o "$EXE" "target/$TARGET/release/kadra.exe"

SETUP="target/Kadra-Setup-$VERSION.exe"
makensis -V2 -DVERSION="$VERSION" -DEXE="$(pwd)/$EXE" -DOUT="$(pwd)/$SETUP" installer/kadra.nsi

ZIP="target/Kadra-$VERSION-windows-x64.zip"
rm -f "$ZIP"
cp LICENSE "$STAGE/LICENSE.txt"
(cd "$STAGE" && zip -q -X "../$(basename "$ZIP")" kadra.exe LICENSE.txt)

ls -lh "$SETUP" "$ZIP"
