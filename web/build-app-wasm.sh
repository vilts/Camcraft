#!/usr/bin/env bash
# build-app-wasm.sh - builds camcraft-app (the domain layer, flecs included)
# for wasm32-unknown-unknown and links camcraft-cli.wasm (#31, M13.3 rung 1),
# then builds and links camcraft-ui.wasm for the same target (rung 2), then
# runs wasm-bindgen over it and lays out the page a browser loads (rung 3).
# Provenance: our own build script; the toolchain facts come from the public
# wasi-sdk release notes and the cc / cargo `--config` documentation.
#
# What it works around, all outside our sources (native builds untouched):
#   1. flecs' C needs libc headers and libc itself; wasm32-unknown-unknown has
#      neither. The pinned wasi-sysroot gives both: its headers compile
#      flecs.c, its libc.a satisfies malloc/printf/pthread/clock at link.
#      `-D__COSMOCC__` puts flecs on its generic-POSIX path (no execinfo, no
#      MSVC *_s string calls) and `-D__wasi__` lets the wasi headers load.
#   2. flecs_ecs_sys 0.2.1 / flecs_ecs 0.2.2 do not compile for wasm32 as
#      shipped (upstream: "wasm32-unknown-unknown support in progress"):
#      bindings.rs imports libc::FILE (absent on this target) and declares
#      "C-unwind" where flecs_ecs's wasm cfg expects "C"; meta_fn_types.rs
#      gates the WorldRef/Entity import off on wasm while still using them.
#      The three edits are applied to COPIES under the tools dir and wired
#      in with `cargo --config patch.crates-io...` for this build only.
# The result links with a handful of wasi_snapshot_preview1 imports
# (clock_time_get, fd_*, poll_oneoff, proc_exit); web/wasi_shim.js answers
# them in the page, wired in by the import map in web/index.html.
#   3. wasm-bindgen's CLI must be the exact version of the wasm-bindgen
#      crate in Cargo.lock. A matching one on PATH is used; otherwise that
#      version is installed from crates.io under the tools dir (never into
#      ~/.cargo), after saying so. CAMCRAFT_WASM_NO_INSTALL=1 turns that
#      install into a refusal that names the version to provide.
# Cargo.lock: the flecs patch above makes cargo rewrite the workspace lock. The
# script saves the tracked lock to <target>/wasm-tools/Cargo.lock.keep first
# and restores it in an EXIT trap (success, failure, Ctrl-C), so a run leaves
# `git status` as it found it. Only a SIGKILL skips the trap.
# Output: <target>/web/ holds index.html, wasi_shim.js, camcraft-ui.js and
# camcraft-ui_bg.wasm - serve that folder over http to run the shell.
#
# Usage: web/build-app-wasm.sh [--release]
# Needs: clang with the wasm32 target, curl, tar, sha256sum (or shasum), rustup target
# wasm32-unknown-unknown.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
PROFILE_ARGS=()
if [[ "${1:-}" == "--release" ]]; then PROFILE_ARGS=(--release); fi

WASI_SDK=34
WASI_SYSROOT="wasi-sysroot-${WASI_SDK}.0"
WASI_SHA256="9d813544eeebe38b7b8f2244ed591de46b6db812c6dd1a257ff9f0d2a905a2be"
FLECS_VER="0.2.2"
FLECS_SYS_VER="0.2.1"

# Windows (Git Bash) clang and cargo want C:/ paths; elsewhere a no-op.
native_path() { if command -v cygpath >/dev/null 2>&1; then cygpath -m "$1"; else printf '%s' "$1"; fi; }
die() { echo "build-app-wasm: $*" >&2; exit 1; }
# Portable across GNU (Linux, Git Bash) and BSD (macOS) tools: BSD sed reads
# the next argument as the `-i` suffix, so `sed -i` is never used. The edit
# goes through a temp file and a move instead.
# Returns 1 (after naming the file) when sed or the move fails; each caller
# stops the script on that with `|| die`. A plain `&&` list would not do:
# bash ignores set -e inside it, so a failing sed would pass silently.
sed_in_place() {
    if ! sed "$1" "$2" > "$2.tmp"; then
        echo "build-app-wasm: sed failed on $2" >&2
        rm -f "$2.tmp"
        return 1
    fi
    if ! mv "$2.tmp" "$2"; then
        echo "build-app-wasm: could not replace $2" >&2
        rm -f "$2.tmp"
        return 1
    fi
}
# sha256sum is GNU; stock macOS ships shasum instead.
sha256_of() {
    if command -v sha256sum >/dev/null 2>&1; then sha256sum "$1" | cut -d' ' -f1
    else shasum -a 256 "$1" | cut -d' ' -f1; fi
}

TOOLS="$ROOT/target/wasm-tools"
mkdir -p "$TOOLS"

# The --config patch makes cargo rewrite the flecs entries of the workspace
# Cargo.lock (path source, no checksum). The tracked lock is saved here before
# any cargo call and put back by this EXIT trap on success, on failure and on
# Ctrl-C / TERM (bash runs EXIT traps on signal exits), so the checkout is left
# as it was found. Only SIGKILL escapes it; after a killed run, restore with
# `git checkout -- Cargo.lock`.
LOCK_KEEP="$TOOLS/Cargo.lock.keep"
cp "$ROOT/Cargo.lock" "$LOCK_KEEP"
restore_lock() {
    cp "$LOCK_KEEP" "$ROOT/Cargo.lock" \
        || echo "build-app-wasm: could not restore Cargo.lock from $LOCK_KEEP" >&2
}
trap restore_lock EXIT

# --- 1. the pinned wasi sysroot ------------------------------------------
SYSROOT="$TOOLS/$WASI_SYSROOT"
if [[ ! -d "$SYSROOT/include/wasm32-wasip1" ]]; then
    tarball="$TOOLS/$WASI_SYSROOT.tar.gz"
    curl -fsSL -o "$tarball" \
        "https://github.com/WebAssembly/wasi-sdk/releases/download/wasi-sdk-${WASI_SDK}/${WASI_SYSROOT}.tar.gz" \
        || die "cannot download $WASI_SYSROOT"
    got="$(sha256_of "$tarball")"
    [[ "$got" == "$WASI_SHA256" ]] || die "$WASI_SYSROOT sha256 $got, expected $WASI_SHA256"
    tar xzf "$tarball" -C "$TOOLS" || die "cannot unpack $tarball"
fi

# --- 2. patched copies of the two flecs crates ----------------------------
grep -q "^version = \"$FLECS_VER\"" <(grep -A1 '^name = "flecs_ecs"$' "$ROOT/Cargo.lock") \
    || die "Cargo.lock no longer pins flecs_ecs $FLECS_VER - re-check the patch points"
grep -q "^version = \"$FLECS_SYS_VER\"" <(grep -A1 '^name = "flecs_ecs_sys"$' "$ROOT/Cargo.lock") \
    || die "Cargo.lock no longer pins flecs_ecs_sys $FLECS_SYS_VER - re-check the patch points"
(cd "$ROOT" && cargo fetch --locked) || die "cargo fetch failed"
REG="${CARGO_HOME:-$HOME/.cargo}/registry/src"
src_of() { find "$REG" -maxdepth 2 -type d -name "$1" | head -1; }
FLECS_SRC="$(src_of "flecs_ecs-$FLECS_VER")"
FLECS_SYS_SRC="$(src_of "flecs_ecs_sys-$FLECS_SYS_VER")"
[[ -n "$FLECS_SRC" && -n "$FLECS_SYS_SRC" ]] || die "flecs sources not found under $REG"

PATCHED="$TOOLS/patched"
rm -rf "$PATCHED"
mkdir -p "$PATCHED"
cp -r "$FLECS_SRC" "$PATCHED/flecs_ecs"
cp -r "$FLECS_SYS_SRC" "$PATCHED/flecs_ecs_sys"

B="$PATCHED/flecs_ecs_sys/src/bindings.rs"
sed_in_place 's/^use libc::FILE;$/pub type FILE = core::ffi::c_void;/' "$B" || die "cannot patch $B (libc::FILE)"
sed_in_place 's/"C-unwind"/"C"/g' "$B" || die "cannot patch $B (C-unwind)"
grep -q '^pub type FILE = core::ffi::c_void;$' "$B" || die "patch point moved: libc::FILE in bindings.rs"
! grep -q '"C-unwind"' "$B" || die "patch point moved: C-unwind in bindings.rs"
M="$PATCHED/flecs_ecs/src/addons/meta/meta_fn_types.rs"
[[ "$(head -2 "$M")" == $'#[cfg(not(target_family = "wasm"))]\nuse crate::core::{Entity, WorldRef};' ]] \
    || die "patch point moved: the WorldRef import in meta_fn_types.rs"
sed_in_place '1d' "$M" || die "cannot patch $M (WorldRef import)"

# --- 3. the build ---------------------------------------------------------
NSYS="$(native_path "$SYSROOT")"
export CFLAGS_wasm32_unknown_unknown="-isystem $NSYS/include/wasm32-wasip1 -D__wasi__ -D__COSMOCC__"
export CARGO_TARGET_WASM32_UNKNOWN_UNKNOWN_RUSTFLAGS="-C link-arg=-L$NSYS/lib/wasm32-wasip1 -C link-arg=-lc"
cd "$ROOT"
cargo build -p camcraft-app --target wasm32-unknown-unknown ${PROFILE_ARGS[@]+"${PROFILE_ARGS[@]}"} \
    --config "patch.crates-io.flecs_ecs.path='$(native_path "$PATCHED/flecs_ecs")'" \
    --config "patch.crates-io.flecs_ecs_sys.path='$(native_path "$PATCHED/flecs_ecs_sys")'"

OUT="${CARGO_TARGET_DIR:-$ROOT/target}/wasm32-unknown-unknown/$([[ ${#PROFILE_ARGS[@]} -gt 0 ]] && echo release || echo debug)/camcraft-cli.wasm"
[[ -f "$OUT" ]] || die "build reported success but $OUT is missing"
echo "build-app-wasm: linked $OUT"

# --- 4. rungs 2-3: the UI links for wasm32 ---------------------------------
# Built as a cdylib (`cargo rustc --crate-type`, so the manifest keeps its
# plain rlib and native builds link nothing extra): that is the form whose
# exports survive the link, and the browser shell's entry is an export
# (`camcraft_start`, egui_app/shell_web.rs). The native window
# (shell_window, egui-winit, rfd) is cfg'd out. The camcraft-ui *binary* is
# not linked for wasm32 any more: it has no command line to serve there and
# its `main` only refuses by name.
cargo rustc -p camcraft-ui --lib --crate-type cdylib --target wasm32-unknown-unknown ${PROFILE_ARGS[@]+"${PROFILE_ARGS[@]}"}     --config "patch.crates-io.flecs_ecs.path='$(native_path "$PATCHED/flecs_ecs")'"     --config "patch.crates-io.flecs_ecs_sys.path='$(native_path "$PATCHED/flecs_ecs_sys")'"
UI_OUT="$(dirname "$OUT")/camcraft_ui.wasm"
[[ -f "$UI_OUT" ]] || die "build reported success but $UI_OUT is missing"
echo "build-app-wasm: linked $UI_OUT"

# --- 5. rung 3: the page a browser loads ----------------------------------
# wasm-bindgen writes the JS glue (camcraft-ui.js) and the module it loads;
# the page and the WASI shim are copied beside them.
BINDGEN_VER="$(grep -A1 '^name = "wasm-bindgen"$' "$LOCK_KEEP"| sed -n 's/^version = "\(.*\)"$/\1/p')"
[[ -n "$BINDGEN_VER" ]] || die "Cargo.lock names no wasm-bindgen version"
BINDGEN="$(command -v wasm-bindgen || true)"
if [[ -z "$BINDGEN" || "$("$BINDGEN" --version 2>/dev/null)" != "wasm-bindgen $BINDGEN_VER" ]]; then
    BINDGEN="$TOOLS/bindgen/bin/wasm-bindgen"
    if [[ "$("$BINDGEN" --version 2>/dev/null || true)" != "wasm-bindgen $BINDGEN_VER" ]]; then
        # The one step that fetches and builds outside code: the CLI is not
        # in the workspace lock and has no checksum in this repo, and its
        # build scripts run. It is announced, and CAMCRAFT_WASM_NO_INSTALL=1
        # refuses it with what to do by hand instead.
        INSTALL_ROOT="$(native_path "$TOOLS/bindgen")"
        INSTALL_CMD="cargo install wasm-bindgen-cli --version $BINDGEN_VER --locked --root $INSTALL_ROOT"
        if [[ "${CAMCRAFT_WASM_NO_INSTALL:-}" == "1" ]]; then
            die "no wasm-bindgen $BINDGEN_VER on PATH or under $TOOLS/bindgen, and CAMCRAFT_WASM_NO_INSTALL=1 forbids installing it. Put that exact version on PATH (it must match the wasm-bindgen crate in Cargo.lock), or run: $INSTALL_CMD"
        fi
        echo "build-app-wasm: no wasm-bindgen $BINDGEN_VER on PATH - installing wasm-bindgen-cli $BINDGEN_VER from crates.io (downloads and builds it and its dependencies) into $INSTALL_ROOT"
        echo "build-app-wasm:   running: $INSTALL_CMD"
        echo "build-app-wasm:   set CAMCRAFT_WASM_NO_INSTALL=1 to refuse this and be told what to install by hand"
        cargo install wasm-bindgen-cli --version "$BINDGEN_VER" --locked --root "$INSTALL_ROOT" \
            || die "cannot install wasm-bindgen-cli $BINDGEN_VER"
    fi
fi
echo "build-app-wasm: using $("$BINDGEN" --version) at $BINDGEN"
WEB_OUT="${CARGO_TARGET_DIR:-$ROOT/target}/web"
rm -rf "$WEB_OUT"
mkdir -p "$WEB_OUT"
"$BINDGEN" --target web --no-typescript --out-dir "$(native_path "$WEB_OUT")" --out-name camcraft-ui "$(native_path "$UI_OUT")"     || die "wasm-bindgen failed on $UI_OUT"
cp "$ROOT/web/index.html" "$ROOT/web/wasi_shim.js" "$WEB_OUT/"
[[ -f "$WEB_OUT/camcraft-ui.js" && -f "$WEB_OUT/camcraft-ui_bg.wasm" ]] || die "wasm-bindgen wrote no glue into $WEB_OUT"
grep -q 'camcraft_start' "$WEB_OUT/camcraft-ui.js" || die "the glue does not export camcraft_start - the shell cannot be started"
# Every WASI import the module asks for must be answered by the shim, or
# the page fails at instantiation with a link error instead of a message.
# The names are read from the module's import section (module name, one
# length byte, field name).
# (`|| true`: grep finding nothing must reach the named message below, not
# end the script through pipefail.)
WASI_FNS="$(LC_ALL=C grep -a -o 'wasi_snapshot_preview1[^a-z_][a-z_0-9]*' "$WEB_OUT/camcraft-ui_bg.wasm" | LC_ALL=C sed 's/^wasi_snapshot_preview1.//' | sort -u || true)"
[[ -n "$WASI_FNS" ]] || die "found no WASI imports in $WEB_OUT/camcraft-ui_bg.wasm - the module should import its clock and log calls; re-check how the import section is read"
for fn in $WASI_FNS; do
    grep -q "^export function $fn(" "$WEB_OUT/wasi_shim.js" || die "web/wasi_shim.js does not answer the WASI import $fn"
done
echo "build-app-wasm: WASI imports answered by the shim: $(echo $WASI_FNS)"
echo "build-app-wasm: page ready in $WEB_OUT (serve it over http and open index.html)"
