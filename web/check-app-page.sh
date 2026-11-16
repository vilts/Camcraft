#!/usr/bin/env bash
# check-app-page.sh - checks a laid-out browser shell folder (#1103): the
# four files web/build-app-wasm.sh writes are there and not empty, and
# nothing the page or its scripts load is addressed from the site root.
# Provenance: our own check; URL resolution rules from the WHATWG URL and
# HTML import-map specifications (relative to the page / module URL).
#
# Why: the release workflow publishes the folder at <pages>/<repo>/app/
# (e.g. /Camcraft/app/, or a fork's own repo name). A "/x.js" there would
# load from the Pages host's root - another site, or a 404 - so every URL
# must be relative ("./x.js", or new URL("x", import.meta.url)).
#
# Usage: web/check-app-page.sh <folder>
#   locally:   web/check-app-page.sh target/web
#   workflow:  web/check-app-page.sh site/public/app  (the Pages artifact)
# Exit 0 when the folder passes; otherwise each problem is printed and the
# exit status is 1.
set -euo pipefail

dir="${1:-}"
[[ -n "$dir" ]] || { echo "usage: $0 <folder holding index.html>" >&2; exit 2; }
[[ -d "$dir" ]] || { echo "check-app-page: no folder $dir" >&2; exit 1; }

fail=0
problem() { echo "check-app-page: $*" >&2; fail=1; }

# --- 1. the files the page needs ------------------------------------------
for f in index.html wasi_shim.js camcraft-ui.js camcraft-ui_bg.wasm; do
    [[ -s "$dir/$f" ]] || problem "$dir/$f is missing or empty"
done
# A .wasm that is not a wasm module (an HTML error page, a git LFS pointer)
# would fail only in the browser: check the magic number "\0asm".
if [[ -s "$dir/camcraft-ui_bg.wasm" ]]; then
    magic="$(head -c 4 "$dir/camcraft-ui_bg.wasm" | od -An -tx1 | tr -d ' \n')"
    [[ "$magic" == "0061736d" ]] || problem "$dir/camcraft-ui_bg.wasm is not a wasm module (starts with $magic)"
fi

# --- 2. no root-absolute or protocol-relative URL -------------------------
# Every place a page or module names a URL to load: src= / href= /
# action= attributes, import-map values, static and dynamic imports,
# fetch(), new URL(), new Worker(). A value starting with "/" (root-
# absolute) or "//" (another host) fails. Absolute https:// URLs are not
# ours to load from and are reported too.
url='["'"'"'`](/|https?:)'
patterns=(
    "(src|href|action)[[:space:]]*=[[:space:]]*$url"
    "\"imports\"[^}]*:[[:space:]]*$url"
    "(import|from)[[:space:]]*\\(?[[:space:]]*$url"
    "fetch[[:space:]]*\\([[:space:]]*$url"
    "new[[:space:]]+(URL|Worker|SharedWorker)[[:space:]]*\\([[:space:]]*$url"
)
shopt -s nullglob
for file in "$dir"/*.html "$dir"/*.js; do
    for p in "${patterns[@]}"; do
        if hits="$(grep -nE "$p" "$file")"; then
            while IFS= read -r line; do
                problem "$file:${line:0:200} - loads from outside the page's own folder"
            done <<<"$hits"
        fi
    done
done

# --- 3. nothing that needs cross-origin isolation -------------------------
# GitHub Pages sends no COOP/COEP headers, so SharedArrayBuffer (and with
# it wasm threads) is unavailable there: a page that needs it cannot start.
for file in "$dir"/*.js; do
    if grep -q 'SharedArrayBuffer' "$file"; then
        problem "$file uses SharedArrayBuffer - GitHub Pages cannot serve the COOP/COEP headers it needs"
    fi
done

if [[ "$fail" -ne 0 ]]; then exit 1; fi
echo "check-app-page: $dir ok (4 files present; every URL relative to the page's folder; no SharedArrayBuffer)"
