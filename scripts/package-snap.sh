#!/usr/bin/env bash
# Wrap the Linux release tarball as a sideloadable desktop snap.
set -euo pipefail

root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
cd "$root"
if [[ $(uname -s) != Linux || $(uname -m) != x86_64 ]]; then
    echo "package-snap: Linux x86_64 is required" >&2
    exit 1
fi
command -v snapcraft >/dev/null || { echo "package-snap: missing snapcraft" >&2; exit 1; }
dist=${CAMCRAFT_DIST_DIR:-dist}
mkdir -p "$dist"
dist=$(cd "$dist" && pwd)
shopt -s nullglob
archives=("$dist"/camcraft-*-linux-x86_64.tar.gz)
if ((${#archives[@]} != 1)); then
    echo "package-snap: expected one Linux release tarball in $dist" >&2
    exit 1
fi
name=${archives[0]##*/}
version=${name#camcraft-}
version=${version%-linux-x86_64.tar.gz}
if [[ ! $version =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
    echo "package-snap: invalid version '$version'" >&2
    exit 1
fi
work=$(mktemp -d "$dist/.package-snap.XXXXXX")
trap 'rm -rf "$work"' EXIT
mkdir -p "$work/stage" "$work/snap/gui"
tar -xzf "${archives[0]}" -C "$work/stage" --strip-components=1
test -x "$work/stage/camcraft-ui"
cp assets/logo/icons/camcraft-256.png "$work/snap/gui/icon.png"
cat > "$work/snap/gui/camcraft.desktop" <<'EOF'
[Desktop Entry]
Type=Application
Name=CamCraft
Comment=Computer-aided manufacturing
Exec=camcraft
Icon=${SNAP}/meta/gui/icon.png
Terminal=false
Categories=Graphics;Engineering;
EOF
cat > "$work/snap/snapcraft.yaml" <<EOF
name: camcraft
version: '$version'
summary: Computer-aided manufacturing
description: |
  CamCraft is a desktop computer-aided manufacturing application with a
  command-line interface and post processor.
base: core22
grade: stable
confinement: strict
architectures:
  - build-on: amd64
parts:
  camcraft:
    plugin: dump
    source: stage
    stage-packages:
      - libxkbcommon0
      - libxkbcommon-x11-0
      - libwayland-client0
      - libwayland-cursor0
      - libwayland-egl1
      - libx11-6
      - libxcursor1
      - libxi6
      - libxrandr2
      - libxinerama1
      - libvulkan1
      - libgl1
      - libgcc-s1
      - libstdc++6
apps:
  camcraft:
    command: camcraft-ui
    desktop: meta/gui/camcraft.desktop
    plugs: [desktop, desktop-legacy, wayland, x11, opengl, home, network, gsettings]
  cli:
    command: camcraft-cli
    plugs: [home, network]
  post:
    command: post
    plugs: [home]
EOF
(
    cd "$work"
    snapcraft --destructive-mode --output "$dist/camcraft-$version-linux-x86_64.snap"
)
test -s "$dist/camcraft-$version-linux-x86_64.snap"
echo "Snap: $dist/camcraft-$version-linux-x86_64.snap"
