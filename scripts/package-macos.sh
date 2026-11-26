#!/usr/bin/env bash
# Assemble a universal macOS app bundle, portable ZIP, and disk image.
# Provenance: Apple's macOS bundle, distribution signing and notarization documentation.
set -euo pipefail
# Keep credentials out of the trace if the script was invoked with bash -x.
set +x

root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
cd "$root"
if [[ $(uname -s) != Darwin ]]; then
    echo "package-macos: macOS is required" >&2
    exit 1
fi

signing_settings=(
    CAMCRAFT_MACOS_CERT_P12_BASE64
    CAMCRAFT_MACOS_CERT_PASSWORD
    CAMCRAFT_MACOS_NOTARY_KEY_BASE64
    CAMCRAFT_MACOS_NOTARY_KEY_ID
    CAMCRAFT_MACOS_NOTARY_ISSUER_ID
)
sign_release=false
missing=()
for setting in "${signing_settings[@]}"; do
    if [[ -n ${!setting:-} ]]; then
        sign_release=true
    else
        missing+=("$setting")
    fi
done
if $sign_release; then
    if (( ${#missing[@]} > 0 )); then
        echo "package-macos: incomplete signing configuration. Set ${missing[*]}" >&2
        exit 1
    fi
    # Keep the values in this shell and out of child process environments.
    export -n "${signing_settings[@]}"
fi

for tool in cargo python3 lipo codesign plutil ditto hdiutil sips iconutil; do
    if ! command -v "$tool" >/dev/null 2>&1; then
        echo "package-macos: missing $tool" >&2
        exit 1
    fi
done
if $sign_release; then
    for tool in base64 openssl security xcrun; do
        if ! command -v "$tool" >/dev/null 2>&1; then
            echo "package-macos: missing $tool for release signing" >&2
            exit 1
        fi
    done
    xcrun --find notarytool >/dev/null
    xcrun --find stapler >/dev/null
fi

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
signing_dir=
keychain=
cleanup() {
    local status=$?
    if [[ -n $keychain ]] && ! security delete-keychain "$keychain"; then
        echo "package-macos: could not delete the temporary signing keychain" >&2
        status=1
    fi
    if [[ -n $signing_dir ]]; then
        rm -rf "$signing_dir" || status=1
    fi
    rm -rf "$work" || status=1
    exit "$status"
}
trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM
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
    lipo "$macos/$binary" -verify_arch arm64
    lipo "$macos/$binary" -verify_arch x86_64
    chmod 755 "$macos/$binary"
done
for asset in posts machines ui scripts themes; do
    cp -R "assets/$asset" "$resources/assets/$asset"
done
cp -R examples "$resources/examples"
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

prepare_signing() {
    local certificate keychain_path keychain_password identity
    signing_dir=$(mktemp -d "${RUNNER_TEMP:-${TMPDIR:-/tmp}}/camcraft-sign.XXXXXX")
    certificate="$signing_dir/DeveloperID.p12"
    notary_key="$signing_dir/AuthKey.p8"
    if ! printf '%s' "$CAMCRAFT_MACOS_CERT_P12_BASE64" | base64 -D > "$certificate"; then
        echo "package-macos: CAMCRAFT_MACOS_CERT_P12_BASE64 is not valid base64" >&2
        exit 1
    fi
    if ! printf '%s' "$CAMCRAFT_MACOS_NOTARY_KEY_BASE64" | base64 -D > "$notary_key"; then
        echo "package-macos: CAMCRAFT_MACOS_NOTARY_KEY_BASE64 is not valid base64" >&2
        exit 1
    fi
    keychain_path="$signing_dir/camcraft.keychain-db"
    keychain_password=$(openssl rand -hex 32)
    security create-keychain -p "$keychain_password" "$keychain_path"
    keychain=$keychain_path
    security set-keychain-settings -lut 21600 "$keychain"
    security unlock-keychain -p "$keychain_password" "$keychain"
    security import "$certificate" -P "$CAMCRAFT_MACOS_CERT_PASSWORD" \
        -f pkcs12 -k "$keychain" -T /usr/bin/codesign
    rm "$certificate"
    unset CAMCRAFT_MACOS_CERT_P12_BASE64 CAMCRAFT_MACOS_CERT_PASSWORD CAMCRAFT_MACOS_NOTARY_KEY_BASE64
    security set-key-partition-list -S apple-tool:,apple: -s -k "$keychain_password" "$keychain" >/dev/null
    identity=$(security find-identity -v -p codesigning "$keychain" |
        awk '/"Developer ID Application: / { print $2 }')
    if [[ ! $identity =~ ^[[:xdigit:]]{40}$ ]]; then
        echo "package-macos: the PKCS#12 file must contain exactly one valid Developer ID Application identity" >&2
        exit 1
    fi
    signing=(--sign "$identity" --keychain "$keychain" --timestamp)
}

notarize_and_staple() {
    local archive=$1 target=$2 result submit_status=0
    result=$(xcrun notarytool submit "$archive" --wait --output-format json \
        --key "$notary_key" --key-id "$CAMCRAFT_MACOS_NOTARY_KEY_ID" \
        --issuer "$CAMCRAFT_MACOS_NOTARY_ISSUER_ID") || submit_status=$?
    if ! printf '%s' "$result" | python3 -c '
import json, sys
try:
    result = json.load(sys.stdin)
    submission_id = result.get("id")
    status = result.get("status")
    print(f"Apple notary submission {submission_id!r}: {status!r}")
except (ValueError, AttributeError):
    sys.exit(1)
sys.exit(0 if status == "Accepted" else 1)
' || (( submit_status != 0 )); then
        echo "package-macos: notarization did not finish successfully for ${target##*/}" >&2
        exit 1
    fi
    xcrun stapler staple "$target"
    xcrun stapler validate "$target"
}

# An ad hoc signature makes both Mach-O slices valid on Apple Silicon.
app_signing=(--sign - --timestamp=none)
if $sign_release; then
    prepare_signing
    app_signing=("${signing[@]}" --options runtime)
else
    echo "macOS release: unsigned for distribution (ad hoc app, no notarization)"
fi
# Sign subsidiary executables before sealing the bundle and its main executable.
for binary in camcraft-cli post; do
    codesign --force "${app_signing[@]}" "$macos/$binary"
done
codesign --force "${app_signing[@]}" "$app"
codesign --verify --deep --strict "$app"

name="camcraft-$version-macos-universal"
if $sign_release; then
    # A ZIP can be submitted, but the ticket attaches to its enclosed app.
    ditto -c -k --sequesterRsrc --keepParent "$app" "$work/CamCraft-notary.zip"
    notarize_and_staple "$work/CamCraft-notary.zip" "$app"
fi
ditto -c -k --sequesterRsrc --keepParent "$app" "$work/$name.zip"
mkdir -p "$work/dmg"
ditto "$app" "$work/dmg/CamCraft.app"
ln -s /Applications "$work/dmg/Applications"
hdiutil create -volname CamCraft -srcfolder "$work/dmg" \
    -format UDZO -ov "$work/$name.dmg"
if $sign_release; then
    codesign "${signing[@]}" --identifier org.camcraft.dmg "$work/$name.dmg"
    codesign --verify --strict "$work/$name.dmg"
    notarize_and_staple "$work/$name.dmg" "$work/$name.dmg"
    echo "macOS release: Developer ID signed and notarized"
fi
mv -f "$work/$name.zip" "$work/$name.dmg" "$dist/"
echo "macOS packages:"
printf '  %s\n' "$dist/$name.zip" "$dist/$name.dmg"
