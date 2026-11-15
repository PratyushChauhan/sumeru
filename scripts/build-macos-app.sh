#!/usr/bin/env bash
# Inputs: [out-dir] (default dist-macos). Needs Rust targets aarch64/x86_64-apple-darwin + Swift 5.10+.
# Outputs: <out>/Sumeru.app (universal SwiftUI shell + headless Rust engine, ad-hoc signed)
#          and <out>/sumeru_<version>_universal.dmg, plus <out>/sumeru (universal engine) for CLI assets.
set -euo pipefail
cd "$(dirname "$0")/.."
OUT="${1:-dist-macos}"
VERSION=$(jq -r .version src-tauri/tauri.conf.json)
APP="$OUT/Sumeru.app"
rm -rf "$APP" && mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Helpers" "$APP/Contents/Resources/marketplace" "$APP/Contents/Library/LaunchAgents"

# Headless Rust engine (no Tauri/WebKit), both arches.
for t in aarch64-apple-darwin x86_64-apple-darwin; do
  cargo build --release --no-default-features --manifest-path src-tauri/Cargo.toml --target "$t"
done
lipo -create -output "$OUT/sumeru" src-tauri/target/{aarch64,x86_64}-apple-darwin/release/sumeru
strip -x "$OUT/sumeru"
cp "$OUT/sumeru" "$APP/Contents/Helpers/sumeru"

# SwiftUI shell, both arches.
for a in arm64 x86_64; do
  swift build -c release --package-path macos --product Sumeru --triple "$a-apple-macosx14.0"
done
lipo -create -output "$APP/Contents/MacOS/Sumeru" macos/.build/{arm64,x86_64}-apple-macosx/release/Sumeru

sed "s/__VERSION__/$VERSION/g" macos/Resources/Info.plist > "$APP/Contents/Info.plist"
cp macos/Resources/com.sumeru.app.login.plist "$APP/Contents/Library/LaunchAgents/"
cp src-tauri/icons/icon.icns "$APP/Contents/Resources/icon.icns"
cp src/lib/marketplace.json "$APP/Contents/Resources/"
cp src/assets/marketplace/* "$APP/Contents/Resources/marketplace/"

# Ad-hoc sign helper first, then the bundle (seals resources; Developer ID would replace "-").
codesign --force --sign - --options runtime "$APP/Contents/Helpers/sumeru"
codesign --force --sign - --options runtime "$APP"
codesign --verify --deep --strict "$APP"

DMG="$OUT/sumeru_${VERSION}_universal.dmg"
STAGE=$(mktemp -d)
cp -R "$APP" "$STAGE/" && ln -s /Applications "$STAGE/Applications"
rm -f "$DMG"
hdiutil create -volname Sumeru -srcfolder "$STAGE" -ov -format UDZO "$DMG" >/dev/null
rm -rf "$STAGE"
echo "$DMG"
