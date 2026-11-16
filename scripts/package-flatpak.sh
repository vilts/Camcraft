#!/usr/bin/env bash
# Build a Flatpak bundle from the Linux release tarball.
set -euo pipefail

root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
cd "$root"
if [[ $(uname -s) != Linux || $(uname -m) != x86_64 ]]; then
    echo "package-flatpak: Linux x86_64 is required" >&2
    exit 1
fi
for tool in flatpak flatpak-builder tar; do
    command -v "$tool" >/dev/null || { echo "package-flatpak: missing $tool" >&2; exit 1; }
done
dist=${CAMCRAFT_DIST_DIR:-dist}
mkdir -p "$dist"
dist=$(cd "$dist" && pwd)
shopt -s nullglob
archives=("$dist"/camcraft-*-linux-x86_64.tar.gz)
if ((${#archives[@]} != 1)); then
    echo "package-flatpak: expected one Linux release tarball in $dist" >&2
    exit 1
fi
name=${archives[0]##*/}
version=${name#camcraft-}
version=${version%-linux-x86_64.tar.gz}
if [[ ! $version =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
    echo "package-flatpak: invalid version '$version'" >&2
    exit 1
fi
work=$(mktemp -d "$dist/.package-flatpak.XXXXXX")
trap 'rm -rf "$work"' EXIT
mkdir -p "$work/stage"
tar -xzf "${archives[0]}" -C "$work/stage" --strip-components=1
test -x "$work/stage/camcraft-ui"
cat > "$work/stage/io.github.NeoTech.Camcraft.desktop" <<'EOF'
[Desktop Entry]
Type=Application
Name=CamCraft
Comment=Computer-aided manufacturing
Exec=camcraft-ui
Icon=io.github.NeoTech.Camcraft
Terminal=false
Categories=Graphics;Engineering;
EOF
cp assets/logo/icons/camcraft-256.png "$work/stage/io.github.NeoTech.Camcraft.png"
cat > "$work/manifest.yml" <<'EOF'
id: io.github.NeoTech.Camcraft
runtime: org.freedesktop.Platform
runtime-version: '25.08'
sdk: org.freedesktop.Sdk
command: camcraft-ui
finish-args:
  - --share=ipc
  - --socket=wayland
  - --socket=fallback-x11
  - --device=dri
  - --filesystem=home
  - --filesystem=/media
  - --filesystem=/mnt
modules:
  - name: camcraft
    buildsystem: simple
    build-commands:
      - install -Dm755 camcraft-ui /app/bin/camcraft-ui
      - install -Dm755 camcraft-cli /app/bin/camcraft-cli
      - install -Dm755 post /app/bin/post
      - cp -a assets /app/bin/assets
      - install -Dm644 io.github.NeoTech.Camcraft.desktop /app/share/applications/io.github.NeoTech.Camcraft.desktop
      - install -Dm644 io.github.NeoTech.Camcraft.png /app/share/icons/hicolor/256x256/apps/io.github.NeoTech.Camcraft.png
    sources:
      - type: dir
        path: stage
EOF
flatpak-builder --user --repo="$work/repo" "$work/build" "$work/manifest.yml"
flatpak build-bundle "$work/repo" "$dist/camcraft-$version-linux-x86_64.flatpak" \
    io.github.NeoTech.Camcraft --runtime-repo=https://dl.flathub.org/repo/flathub.flatpakrepo
test -s "$dist/camcraft-$version-linux-x86_64.flatpak"
echo "Flatpak: $dist/camcraft-$version-linux-x86_64.flatpak"
