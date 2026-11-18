#!/usr/bin/env bash
# Package native FreeBSD binaries with assets adjacent to the executables.
set -euo pipefail

root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
cd "$root"

if [[ $(uname -s) != FreeBSD || $(uname -m) != amd64 ]]; then
    echo "package-freebsd: this script packages FreeBSD amd64 binaries" >&2
    exit 1
fi

bin_dir=${CAMCRAFT_BIN_DIR:-target/release}
dist=${CAMCRAFT_DIST_DIR:-dist}
for binary in camcraft-ui camcraft-cli post; do
    if [[ ! -x "$bin_dir/$binary" ]]; then
        echo "package-freebsd: missing executable $bin_dir/$binary" >&2
        exit 1
    fi
done
version=$("$bin_dir/post" --version | awk '{print $2}')
if [[ ! $version =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
    echo "package-freebsd: unexpected post version '$version'" >&2
    exit 1
fi

mkdir -p "$dist"
dist=$(cd "$dist" && pwd)
name="camcraft-$version-freebsd-amd64"
work=$(mktemp -d "$dist/.package-freebsd.XXXXXX")
trap 'rm -rf "$work"' EXIT
stage="$work/$name"
mkdir -p "$stage/assets"

for binary in camcraft-ui camcraft-cli post; do
    install -m 755 "$bin_dir/$binary" "$stage/$binary"
done
for asset in posts machines ui scripts themes; do
    cp -R "assets/$asset" "$stage/assets/$asset"
done
cp -R examples "$stage/examples"
install -m 644 RELEASE_NOTES.md TESTING.md LICENSE NOTICE README.md "$stage/"
tar -C "$work" -czf "$dist/$name.tar.gz" "$name"
echo "FreeBSD package: $dist/$name.tar.gz"
