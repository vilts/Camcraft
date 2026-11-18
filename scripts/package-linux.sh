#!/usr/bin/env bash
# Package Linux release binaries with the same adjacent assets as the Windows zip.
# Provenance: CamCraft's release.yml staging layout and the DEB/RPM package formats.
set -euo pipefail

root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
cd "$root"

if [[ $(uname -s) != Linux || $(uname -m) != x86_64 ]]; then
    echo "package-linux: this script packages Linux x86_64 binaries" >&2
    exit 1
fi
for tool in dpkg-deb rpmbuild tar; do
    if ! command -v "$tool" >/dev/null 2>&1; then
        echo "package-linux: missing $tool" >&2
        exit 1
    fi
done

bin_dir=${CAMCRAFT_BIN_DIR:-target/release}
dist=${CAMCRAFT_DIST_DIR:-dist}
for binary in camcraft-ui camcraft-cli post; do
    if [[ ! -x "$bin_dir/$binary" ]]; then
        echo "package-linux: missing executable $bin_dir/$binary" >&2
        exit 1
    fi
done
version=$("$bin_dir/post" --version | awk '{print $2}')
if [[ ! $version =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
    echo "package-linux: unexpected post version '$version'" >&2
    exit 1
fi

mkdir -p "$dist"
dist=$(cd "$dist" && pwd)
name="camcraft-$version-linux-x86_64"
work=$(mktemp -d "$dist/.package-linux.XXXXXX")
trap 'rm -rf "$work"' EXIT
stage="$work/$name"
pkg_root="$work/pkg-root"
rpm_dir="$work/rpmbuild"
mkdir -p "$stage/assets" "$pkg_root/usr/lib/camcraft" \
    "$pkg_root/usr/bin" "$pkg_root/usr/share/applications" \
    "$pkg_root/usr/share/pixmaps" "$pkg_root/DEBIAN" \
    "$rpm_dir/SPECS" "$rpm_dir/RPMS"

for binary in camcraft-ui camcraft-cli post; do
    install -m 755 "$bin_dir/$binary" "$stage/$binary"
done
for asset in posts machines ui scripts themes; do
    cp -a "assets/$asset" "$stage/assets/$asset"
done
cp -a examples "$stage/examples"
install -m 644 RELEASE_NOTES.md TESTING.md LICENSE NOTICE README.md "$stage/"
tar -C "$work" -czf "$dist/$name.tar.gz" "$name"

cp -a "$stage/." "$pkg_root/usr/lib/camcraft/"
for pair in camcraft:camcraft-ui camcraft-cli:camcraft-cli camcraft-post:post; do
    command_name=${pair%%:*}
    binary=${pair#*:}
    cat > "$pkg_root/usr/bin/$command_name" <<EOF
#!/bin/sh
exec /usr/lib/camcraft/$binary "\$@"
EOF
    chmod 755 "$pkg_root/usr/bin/$command_name"
done
install -m 644 assets/logo/icons/camcraft-256.png "$pkg_root/usr/share/pixmaps/camcraft.png"
for size in 16 32 48 64 128 256 512; do
    mkdir -p "$pkg_root/usr/share/icons/hicolor/${size}x${size}/apps"
    install -m 644 "assets/logo/icons/camcraft-$size.png" \
        "$pkg_root/usr/share/icons/hicolor/${size}x${size}/apps/camcraft.png"
done
cat > "$pkg_root/usr/share/applications/camcraft.desktop" <<'EOF'
[Desktop Entry]
Type=Application
Name=CamCraft
Comment=Computer-aided manufacturing
Exec=camcraft
Icon=camcraft
Terminal=false
Categories=Graphics;Engineering;
EOF

cat > "$pkg_root/DEBIAN/control" <<EOF
Package: camcraft
Version: $version-1
Section: graphics
Priority: optional
Architecture: amd64
Maintainer: CamCraft contributors <noreply@github.com>
Depends: libc6 (>= 2.35), libgcc-s1, libstdc++6, libxkbcommon0, libxkbcommon-x11-0, libx11-6 | libwayland-client0, libvulkan1 | libgl1
Description: CamCraft computer-aided manufacturing application
 Desktop CAM application, headless CLI, post processor, and supporting data.
EOF
dpkg-deb --build --root-owner-group "$pkg_root" "$dist/$name.deb"

cat > "$rpm_dir/SPECS/camcraft.spec" <<EOF
Name: camcraft
Version: $version
Release: 1%{?dist}
Summary: Computer-aided manufacturing application
License: Apache-2.0
BuildArch: x86_64
Requires: vulkan-loader
Requires: libxkbcommon-x11

%description
Desktop CAM application, headless CLI, post processor, and supporting data.

%prep

%build

%install
mkdir -p "%{buildroot}/usr"
cp -a "%{camcraft_package_root}/usr/." "%{buildroot}/usr/"

%files
/usr/bin/camcraft
/usr/bin/camcraft-cli
/usr/bin/camcraft-post
/usr/lib/camcraft
/usr/share/applications/camcraft.desktop
/usr/share/pixmaps/camcraft.png
/usr/share/icons/hicolor/*/apps/camcraft.png
EOF
rpmbuild -bb --define "_topdir $rpm_dir" \
    --define "camcraft_package_root $pkg_root" "$rpm_dir/SPECS/camcraft.spec"
shopt -s nullglob
rpms=("$rpm_dir"/RPMS/x86_64/camcraft-"$version"-1*.rpm)
if ((${#rpms[@]} != 1)); then
    echo "package-linux: expected one RPM, found ${#rpms[@]}" >&2
    exit 1
fi
cp "${rpms[0]}" "$dist/$name.rpm"

echo "Linux packages:"
printf '  %s\n' "$dist/$name.tar.gz" "$dist/$name.deb" "$dist/$name.rpm"
