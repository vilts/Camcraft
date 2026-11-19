---
name: gate
description: Run and select the CamCraft test gates with `cargo xtask gate` - the quick, batch and release tiers, one test by name, `--only` / `--failed` / `--dry-run`, the heavy lanes, the slow binaries, the narrow gate ladder for a subagent, the shared sccache compile cache, the pixel-snapshot lane and the local batch gate when the post-merge runner is down. Use before you run tests for a patch, a merge batch or a cut, when a gate step is red and you want to run only that step, or when a worktree builds slowly.
---

# gate - run the correct tests, and no more

The rules are in AGENTS.md: "Three gate tiers", "Test the change, not
everything", "Pixel snapshots are release-lane", "Fixes are tested in the
background". This skill has the mechanics. If the two disagree, AGENTS.md is
correct.

## The command

`cargo xtask gate <tier>` runs on Windows, macOS and Linux (#748). The
wrappers `scripts/gate.ps1` (`-Tier quick`, `-Test <name>`) and
`scripts/gate.sh` (`quick`, `test <name>`) forward to it. Logs go to
`gate-logs/<tier>-<timestamp>/`.

| Tier | Command | Runs |
|---|---|---|
| quick | `cargo xtask gate quick [--base main]` | Only the test binaries that the diff touches, from the gate maps and the touched crates. One build for each crate, then one step for each binary. Debug. No heavy lanes. Target: 5 minutes. |
| batch | `cargo xtask gate batch` | One debug build (`--no-run`), then one step for each test target (`<crate> suites: lib`, `<crate> suites: <binary>`) that is not in `SLOW_BINARIES`, the `camcraft-kernel` suites (#1702), the ui suites from `crates/ui`, clippy. No release-mode heavy lanes. Target: 20 minutes. |
| heavy | `cargo xtask gate heavy [--base main]` | The release `--ignored` heavy lanes that the diff reaches, also through the crate dependencies (#2568). One release build of the reached crates, then one step for each reached target. Before a PR is ready and at its review. |
| release | `cargo xtask gate release` | One step for each debug test target (`lib`, `bins`, each binary, `doc`) and the `--ignored` heavy lanes in release. Only at a cut. On the cut runner: `cargo xtask gate release --no-snapshots`. |
| test | `cargo xtask gate test <name> [--crate c] [--binary b] [--exact]` | One test by name. The plain form is `cargo test -p <crate> --test <binary> -- <name> --exact`. |

Flags:

- `--dry-run` (each tier): print the plan and run nothing.
- `--only <step>` (each tier but `test`, repeatable): run only the named
  steps. Give the step name or its first word, for example `kernel`,
  `clippy`, `ui`.
- `--failed gate-logs/<run>` (each tier but `test`): run only the steps
  that failed in that run (#1702).
- `--dry-run` with `--only` or `--failed` shows the selected steps.

A fix for one red step then costs one step, not a full run.

## One step for each test binary (#1941)

- Quick, batch and release plan one step for each test target, as the heavy lanes do (#1714). A step name is
  `<crate> suites: lib`, `<crate> suites: bins`, `<crate> suites: <binary>` or `<crate> suites: doc`. The ui
  steps are `ui suites: ...` and run in `crates/ui`.
- Quick and batch build one time first (`debug build (--no-run)`, or `<crate> build (--no-run)` in quick when
  a crate has more than one step). Each step then calls `cargo test -p <crate> --test <binary>`, which finds
  the build ready. A step has its own log, its own time and its own pass or fail line.
- Run one binary again: `cargo xtask gate batch --only "app suites: cmds"`. `--only "app suites"` selects
  each app step. `--failed gate-logs/<run>` runs only the red steps.
- Quick and batch run the steps one after the other. The release tier runs them in the waves of `lanes.rs`.
- Each step starts cargo, about 0.5 s on Windows. A batch plan has about 700 steps.

## What quick selects

Quick prints its selection before it runs:

- The changed `#[test]` functions of a changed suite (`--exact`).
- The gate-mapped suites of a changed source module. The header of each
  suite has the map: `Gate map (M4.4): covers app::x, app::y...`.
- A change to shared test code (`tests/common`, goldens, snapshots) runs the
  suites of that crate.
- A change in geometry, cl or a `Cargo.toml` falls back to batch.
- A change in `xtask/` runs the unit tests of xtask.
- `camcraft-kernel` runs in quick only when kernel is touched.

## Job budget (#2057)

- One budget sets the jobs of the whole gate: 6 by default.
  `CAMCRAFT_GATE_JOBS=<n>` (1 or more) changes it.
- A step that runs alone gets `-j <budget>` and `--test-threads=<budget>`.
- The lanes of a release wave share the budget. N lanes get
  `-j <budget / N>` each, minimum 1.
- A wave has at most `budget / 2` lanes. A budget of 6 gives 3 lanes of 2
  jobs.
- The gate prints the budget, the lane limit and the jobs of each lane at
  the start, and `-- wave K: N lane(s) x -j M` before each wave. The
  `--dry-run` output shows the same lines.
- **The sum of `-j` of all builds on one machine stays at or below its core
  count.** This includes gates, reviews, worker builds and plain `cargo`
  calls. When other builds run, lower the budget of each one (for example
  two gates at `CAMCRAFT_GATE_JOBS=3` on 6 cores). A plain `cargo` call
  takes its own `-j`.

## Slow binaries and heavy lanes

- Batch does not run the debug binaries in `SLOW_BINARIES`
  (`xtask/src/plan.rs`, pinned by its unit tests): motion `finish3d`,
  `pivot5d`, `flow`; app `center_cut_multiaxis`. Each takes 40-50 s. Release
  runs them.
- A test that takes more than about 60 s in debug moves to the heavy lane,
  or gets a smaller fixture. The ignore reason names the lane.
- Heavy lane tests are `#[ignore]`d. They measure geometry (envelope scans,
  full-precision projections) or generate over the large fixtures. Run them
  with `cargo test --release -p camcraft-app -- --ignored`, at a cut and
  before a push of risky work. Do not run them for each edit.
- In the release tier, the heavy lanes of motion and app are one release
  build (`--no-run`). Then there is one step for each test target that can
  hold an ignored test: `<crate> heavy: lib`, `bins`, `doc` and `<crate>
  heavy: <binary>` (#1714).
- One red binary is one red step. Its note names the failed tests and each
  test that libtest reports as running for more than 60 s.
- Run one heavy binary again with `--only "app heavy: <binary>"`.
- `cargo xtask gate heavy [--base main]` runs only the heavy lanes that the
  diff reaches (AGENTS.md "Heavy lanes at review", `xtask/src/heavy.rs`).
  `--dry-run` prints the reach and the reason for each lane.
  - A changed suite file reaches its own heavy binary.
  - Shared test code under `crates/<crate>/tests/` reaches the heavy lanes
    of that crate.
  - A changed source file, `Cargo.toml` or `build.rs` of a crate reaches the
    heavy lanes of that crate and of each crate that depends on it. The
    dependencies are read from `crates/*/Cargo.toml`.
  - A workspace manifest, `Cargo.lock`, `.cargo/` or toolchain change
    reaches each heavy lane.
  - A post script, machine file, catalog case or fixture reaches the heavy
    binaries that read it.
  - A diff that reaches no heavy lane gives a plan with no step.
  - The steps have the names of the release tier (`<crate> heavy:
    <target>`), so `--only` and `--failed` select them.
- When you move a test to a lane, the `#[test]` count before and after must
  be the same, and the ignored set keeps its golden asserts.

## The narrow ladder (gate subagent)

1. Build one time: `cargo test -p camcraft-app -p camcraft-ui --no-run`.
2. Select the binaries from the gate maps, not from the full tree.
3. Run each test binary that the diff touches
   (`target\debug\deps\<name>-<hash>.exe`). Run them in parallel: they take
   no cargo lock. Take the binary paths from the cargo JSON output. Do not
   glob `*.exe`: that can start the GUI.
4. Report pass or fail for each binary, with the failure output.

The budget is 5 minutes of wall time. A gate subagent runs this ladder. The
main session owns the code and the decision to commit. Never give one
subagent the code change and its verification.

## UI suites and snapshots

- The UI snapshot binaries find their goldens from `crates/ui`. Run them
  with that working directory. From a different directory they fail with
  "Missing snapshot".
- The snapshot lane is in the release tier only: debug, `--ignored
  snapshot`, working directory `crates/ui`.
  - `cargo test -p camcraft-ui --lib -- --ignored snapshot`
  - `cargo test -p camcraft-ui --test gpu_render -- --ignored snapshot`
- Do not use `UPDATE_SNAPSHOTS=1` on a PR or at a merge. The goldens are made
  new at the cut only (`release-cut` skill).
- `VisualRunner` is enabled automatically. Windows show, run fast, and stay
  open if a person closes them early. A hidden run stops after 30 s. The
  window closes 10 s after the last pending test. After a crash or a timeout
  the window stays open.

## Shared compile cache (optional, #900)

Each worktree has its own `target/`, thus a new worktree compiles each
third-party crate again. [sccache](https://github.com/mozilla/sccache) makes
those compilations cache hits.

- Install: `cargo install sccache --locked` (user account only).
- `cargo xtask gate` sets `RUSTC_WRAPPER=sccache` and
  `SCCACHE_CACHE_SIZE=20G` for its cargo calls when `sccache` is on PATH.
- An existing `RUSTC_WRAPPER`, or `CAMCRAFT_NO_SCCACHE=1`, has priority.
- A machine without sccache builds as before.
- For plain `cargo`, set `RUSTC_WRAPPER=sccache` only when `sccache` is on
  PATH. Cargo fails if the wrapper is missing.
- Never export `CARGO_TARGET_DIR`. sccache hashes the `CARGO_*` variables,
  thus that variable makes each worktree miss the cache.
- Release packaging never uses the cache. `cargo xtask release` clears the
  wrapper for the packaging build.
- The cache is in the sccache default directory: Windows
  `%LOCALAPPDATA%\Mozilla\sccache\cache`, Linux `~/.cache/sccache`.
  `SCCACHE_DIR` moves it.
- Dev builds have line tables only. `--config profile.dev.debug=true` gives
  the variable debug information back for a debugger.

## Local batch gate when the post-merge runner is down

`post-merge.yml` tests each push to `main` (`runners` skill). Use this
procedure only when its run is queued with no runner, or when it skipped
itself for a cut and the cut was abandoned.

1. Run one gate for each merge batch, not one for each merge.
2. Dispatch a gate-runner subagent. It runs `cargo xtask gate batch` on
   `origin/main` in the MAIN checkout and uses the `target/` of that
   checkout. Never use a new worktree for it: its `target/` is about 25 GB.
3. Do not wait for it. Claim the next issue and work in a worktree.
4. The gate runs in the main checkout only when all of these are true:
   - The checkout is on a clean `main` (`git pull --ff-only` first).
   - No `release/*` branch is checked out and no cut is in progress.
   - No other gate runs there.
5. Do not start a cut until a running gate is complete.
6. Check the free disk space before you dispatch. Run `cargo clean` on the
   main `target/` only when no gate or cut runs and it is larger than about
   60 GB.
7. A red result is a new issue with `source:agent`. It is `P1` if the output
   is wrong or there is a gouge, else `P2`. The issue names the failed tests
   and the merge that broke them. A red result does not revert the merge.

To hand-test a build between cuts: run `cargo xtask release --version <next>
--no-tag --skip-gate` on a scratch branch off main (it refuses `--no-tag` on
main). Then discard the branch and its version bump. Make no tag and no push.
