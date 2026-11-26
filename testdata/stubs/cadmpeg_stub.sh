#!/bin/sh
# A stand-in for the cadmpeg command line, for crates/app/tests/cadmpeg_import.rs
# (#1873). It is not cadmpeg and holds none of its code. A test makes a folder,
# links this file there as `cadmpeg`, and writes small files beside the link:
#   version      the text `--version` prints
#   version.delay `--version` waits one second before it prints
#   check.json   what `check --json` prints on standard output
#   check.err    what `check` prints on standard error
#   check.exit   the exit status of `check` (0 when absent)
#   check.hang   `check` writes its process id to check.pid and does not end in time
#   convert.step the file `convert` writes to its -o path (none when absent)
#   convert.exit the exit status of `convert` (0 when absent)
#   convert.flood `convert` prints 20 MB (a fixed amount: a broken limit must
#                not fill the disk) and then does not end in time
# Each call is appended to `calls`, one argument per line, a blank line between.
here=$(dirname "$0")
{
    for a in "$@"; do printf '%s\n' "$a"; done
    printf '\n'
} >> "$here/calls"
status() { if [ -f "$here/$1" ]; then cat "$here/$1"; else echo 0; fi; }
case "$1" in
--version)
    if [ -f "$here/version.delay" ]; then sleep 1; fi
    cat "$here/version"
    ;;
check)
    if [ -f "$here/check.hang" ]; then
        echo $$ > "$here/check.pid"
        exec sleep 30
    fi
    if [ -f "$here/check.json" ]; then cat "$here/check.json"; fi
    if [ -f "$here/check.err" ]; then cat "$here/check.err" >&2; fi
    exit "$(status check.exit)"
    ;;
convert)
    if [ -f "$here/convert.flood" ]; then
        head -c 20000000 /dev/zero
        exec sleep 30
    fi
    out=
    while [ $# -gt 0 ]; do
        if [ "$1" = "-o" ]; then out=$2; fi
        shift
    done
    if [ -f "$here/convert.step" ] && [ -n "$out" ]; then cp "$here/convert.step" "$out"; fi
    exit "$(status convert.exit)"
    ;;
*)
    echo "error: unrecognized subcommand '$1'" >&2
    exit 2
    ;;
esac
