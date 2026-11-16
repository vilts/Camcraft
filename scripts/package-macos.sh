#!/usr/bin/env bash
# Assemble a universal macOS app bundle, portable ZIP, and disk image.
# Provenance: Apple's documented macOS bundle layout and CamCraft's release assets.
set -euo pipefail

root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
cd "$root"
if [[ $(uname -s) != Darwin ]]; then
    echo "package-macos: macOS is required" >&2
    exit 1
fi
for tool in cargo python3 lipo codesign plutil ditto hdiutil sips iconutil; do
    if ! command -v "$tool" >/dev/null 2>&1; then
        echo "package-macos: missing $tool" >&2
        exit 1
    fi
done

version=$(cargo metadata --locked --no-deps --format-version 1 |
    python3 -c 'import json,sys; print(next(p["version"] for p in json.load(sys.stdin)["packages"] if p["name"] == "camcraft-ui"))')
if [[ ! $version =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
    echo "package-macos: unexpected CamCraft version '$version'" >&2
    exit 1
fi
dist=${CAMCRAFT_DIST_DIR:-dist}
mkdir -p "$dist"
dist=$(cd "$dist" && pwd)
work=$(mktemp -d "$dist/.package-macos.XXXXXX")
trap 'rm -rf "$work"' EXIT
app="$work/CamCraft.app"
macos="$app/Contents/MacOS"
resources="$app/Contents/Resources"
mkdir -p "$macos" "$resources/assets"

for binary in camcraft-ui camcraft-cli post; do
    arm="target/aarch64-apple-darwin/release/$binary"
    intel="target/x86_64-apple-darwin/release/$binary"
    if [[ ! -x $arm || ! -x $intel ]]; then
        echo "package-macos: missing release binary $arm or $intel" >&2
        exit 1
    fi
    lipo -create "$arm" "$intel" -output "$macos/$binary"
    lipo "$macos/$binary" -verify_arch arm64 x86_64
    chmod 755 "$macos/$binary"
done
for asset in posts machines ui scripts themes; do
    cp -R "assets/$asset" "$resources/assets/$asset"
done
install -m 644 RELEASE_NOTES.md TESTING.md LICENSE NOTICE README.md "$resources/"
iconset="$work/CamCraft.iconset"
mkdir -p "$iconset"
# The logotype's generated sizes (cargo xtask logo, #1459); only 1024 px (512@2x) is scaled up from the 512 px one.
logo_icon() {
    if [ -f "assets/logo/icons/camcraft-$1.png" ]; then
        echo "assets/logo/icons/camcraft-$1.png"
    else
        echo "assets/logo/icons/camcraft-512.png"
    fi
}
for size in 16 32 128 256 512; do
    sips -s format png -z "$size" "$size" "$(logo_icon "$size")" \
        --out "$iconset/icon_${size}x${size}.png" >/dev/null
done
for size in 16 32 128 256 512; do
    double=$((size * 2))
    sips -s format png -z "$double" "$double" "$(logo_icon "$double")" \
        --out "$iconset/icon_${size}x${size}@2x.png" >/dev/null
done
iconutil -c icns "$iconset" -o "$resources/CamCraft.icns"
cat > "$app/Contents/Info.plist" <<EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleExecutable</key><string>camcraft-ui</string>
  <key>CFBundleIdentifier</key><string>org.camcraft.app</string>
  <key>CFBundleName</key><string>CamCraft</string>
  <key>CFBundleDisplayName</key><string>CamCraft</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleIconFile</key><string>CamCraft</string>
  <key>CFBundleShortVersionString</key><string>$version</string>
  <key>CFBundleVersion</key><string>$version</string>
  <key>LSApplicationCategoryType</key><string>public.app-category.productivity</string>
  <key>NSHighResolutionCapable</key><true/>
</dict>
</plist>
EOF
plutil -lint "$app/Contents/Info.plist"

# An ad-hoc signature makes both Mach-O slices valid on Apple Silicon.
# Distribution signing and notarization can replace this when credentials exist.
for binary in camcraft-cli post; do
    codesign --force --sign - --timestamp=none "$macos/$binary"
done
codesign --force --sign - --timestamp=none "$app"
codesign --verify --deep --strict "$app"

name="camcraft-$version-macos-universal"
ditto -c -k --sequesterRsrc --keepParent "$app" "$dist/$name.zip"
mkdir -p "$work/dmg"
cp -R "$app" "$work/dmg/CamCraft.app"
ln -s /Applications "$work/dmg/Applications"
hdiutil create -volname CamCraft -srcfolder "$work/dmg" \
    -format UDZO -ov "$dist/$name.dmg"
echo "macOS packages:"
printf '  %s\n' "$dist/$name.zip" "$dist/$name.dmg"
