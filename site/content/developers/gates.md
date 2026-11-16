+++
title = "Test gates"
description = "Three gate tiers, run locally with cargo xtask gate."
weight = 3
+++

Tests run locally, on any OS, through one command from the workspace's
`xtask/` crate: `cargo xtask gate <tier>`. There are three tiers, so a
change is never held up by hours of unrelated tests.

| Tier | When | What it runs | Target |
|---|---|---|---|
| `quick` | every change | only the test binaries and test functions the diff touches, in debug | 5 minutes or less |
| `batch` | before a batch of commits | every debug suite except the slow ones, the UI suites, and clippy | 20 minutes or less |
| `release` | at a release cut | everything, including the ignored heavy lanes in release mode | as long as it takes |

```sh
cargo xtask gate quick                 # what this branch touches, against main
cargo xtask gate quick --base main     # name the base explicitly
cargo xtask gate batch
cargo xtask gate release
cargo xtask gate test <name> [--crate c] [--exact]   # one test
cargo xtask gate quick --dry-run       # print the plan, run nothing
```

Logs go to `gate-logs/<tier>-<timestamp>/`. The scripts
`scripts/gate.ps1` (PowerShell) and `scripts/gate.sh` (bash) are thin
wrappers that forward to `cargo xtask gate`. The `cargo xtask gate` command
itself is the same on Windows, macOS and Linux.

## Test the change, not everything

A fix is proved by the tests that pin it, run one by one. A whole suite
runs only when the change touches what that suite covers; each suite's
header names the modules it covers (its gate map), and `quick` picks
suites from those maps.

## Where tests live

- `crates/app/tests/`: app and command tests, with shared fixtures in
  `tests/common/`.
- `crates/ui/tests/`: UI tests that start the real app; run them with the
  working directory `crates/ui`.
- Each other crate carries its own tests. Golden toolpath and G-code
  outputs live under `tests/golden/`.
- Tests that take more than about a minute in debug are marked
  `#[ignore]` with their heavy lane named, and run in the release tier.
- UI pixel snapshots run only in the release tier. A pull request never
  changes a snapshot PNG; structural tests assert what each snapshot
  guards, as text that holds on every OS.

## GitHub Actions

Nothing runs on push or pull request: Actions minutes are budgeted. GitHub
runs only `release.yml`, on a pushed release tag, to build and publish the
release (and this site).
