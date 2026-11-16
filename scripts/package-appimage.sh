#!/usr/bin/env bash
# Bundle the Linux release binaries and adjacent data in one AppImage.
# Provenance: linuxdeploy's AppDir and output-plugin documentation.
set -euo pipefail

root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
cd "$root"
if [[ $(uname -s) != Linux || $(uname -m) != x86_64 ]]; then
    echo "package-appimage: Linux x86_64 is required" >&2
    exit 1
fi
linuxdeploy=${LINUXDEPLOY:-}
if [[ -z $linuxdeploy || ! -x $linuxdeploy ]]; then
    echo "package-appimage: set LINUXDEPLOY to an executable linuxdeploy AppImage" >&2
    exit 1
fi
if ! command -v convert >/dev/null 2>&1; then
    echo "package-appimage: ImageMagick convert is required for the 256x256 icon" >&2
    exit 1
fi
bin_dir=${CAMCRAFT_BIN_DIR:-target/release}
for binary in camcraft-ui camcraft-cli post; do
    if [[ ! -x $bin_dir/$binary ]]; then
        echo "package-appimage: missing executable $bin_dir/$binary" >&2
        exit 1
    fi
done
version=$("$bin_dir/post" --version | awk '{print $2}')
if [[ ! $version =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
    echo "package-appimage: unexpected post version '$version'" >&2
    exit 1
fi
dist=${CAMCRAFT_DIST_DIR:-dist}
mkdir -p "$dist"
dist=$(cd "$dist" && pwd)
work=$(mktemp -d "$dist/.package-appimage.XXXXXX")
trap 'rm -rf "$work"' EXIT
appdir="$work/CamCraft.AppDir"
mkdir -p "$appdir/usr/bin/assets" "$appdir/usr/share/applications" \
    "$appdir/usr/share/icons/hicolor/256x256/apps"
for binary in camcraft-ui camcraft-cli post; do
    install -m 755 "$bin_dir/$binary" "$appdir/usr/bin/$binary"
done
for asset in posts machines ui scripts themes; do
    cp -a "assets/$asset" "$appdir/usr/bin/assets/$asset"
done
cat > "$appdir/usr/share/applications/camcraft.desktop" <<'EOF'
[Desktop Entry]
Type=Application
Name=CamCraft
Comment=Computer-aided manufacturing
Exec=camcraft-ui
Icon=camcraft
Terminal=false
Categories=Graphics;Engineering;
EOF
install -m 644 assets/logo/icons/camcraft-256.png \
    "$appdir/usr/share/icons/hicolor/256x256/apps/camcraft.png"
ln -s usr/share/applications/camcraft.desktop "$appdir/camcraft.desktop"
ln -s usr/share/icons/hicolor/256x256/apps/camcraft.png "$appdir/camcraft.png"
cat > "$appdir/AppRun" <<'EOF'
#!/bin/sh
appdir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
exec "$appdir/usr/bin/camcraft-ui" "$@"
EOF
chmod 755 "$appdir/AppRun"
# linuxdeploy rewrites ELF headers even for binaries already in the AppDir.
# That overwrites .init in CamCraft and crashes before main on Fedora. Use
# the appimagetool bundled in our pinned linuxdeploy download directly.
before=$(sha256sum "$appdir/usr/bin/camcraft-ui" | awk '{print $1}')
(
    cd "$work"
    "$linuxdeploy" --appimage-extract >/dev/null
)
appimagetool="$work/squashfs-root/plugins/linuxdeploy-plugin-appimage/usr/bin/appimagetool"
if [[ ! -x $appimagetool ]]; then
    echo "package-appimage: pinned linuxdeploy has no bundled appimagetool" >&2
    exit 1
fi
ARCH=x86_64 "$appimagetool" "$appdir" \
    "$dist/camcraft-$version-linux-x86_64.AppImage" --no-appstream
after=$(sha256sum "$appdir/usr/bin/camcraft-ui" | awk '{print $1}')
if [[ $before != "$after" ]]; then
    echo "package-appimage: appimagetool modified camcraft-ui; refusing broken image" >&2
    exit 1
fi
test -s "$dist/camcraft-$version-linux-x86_64.AppImage"
echo "AppImage: $dist/camcraft-$version-linux-x86_64.AppImage"
