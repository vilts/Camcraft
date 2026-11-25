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
| batch | `cargo xtask gate batch` | One debug build (`--no-run`) for each crate, then one step for each test target (`<crate> suites: lib`, `<crate> suites: <binary>`) that is not in `SLOW_BINARIES`, the `camcraft-kernel` suites (#1702), the ui suites from `crates/ui`, clippy. The steps run in the parallel lanes of `lanes.rs` (#2727). No release-mode heavy lanes. Target: 20 minutes. |
| heavy | `cargo xtask gate heavy [--base main]` | The release `--ignored` heavy lanes that the diff reaches, also through the crate dependencies (#2568). One release build of the reached crates, then one step for each reached target. A tool, not a merge condition (#3556): a PR never runs the full reach. Only a `P1` PR under the safety floor runs one targeted step with `--only "<crate> heavy: <target>"`. The wave form `--covers <PR>:<head sha>` (#3460) stays as an option. |
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
- `--lane N/M` (batch only): run lane N of M of the batch plan (#2908). See
  "Batch lanes for separate runners".

A fix for one red step then costs one step, not a full run.

## One step for each test binary (#1941)

- Quick, batch and release plan one step for each test target, as the heavy lanes do (#1714). A step name is
  `<crate> suites: lib`, `<crate> suites: bins`, `<crate> suites: <binary>` or `<crate> suites: doc`. The ui
  steps are `ui suites: ...` and run in `crates/ui`.
- Quick and batch build first (`debug build: <crate> (--no-run)` for each crate in batch, or
  `<crate> build (--no-run)` in quick when a crate has more than one step). Batch builds each crate alone
  (#2727): one build of all the crates gives a shared dependency the features of all of them, and each step
  then compiles its crate again. Each step then calls `cargo test -p <crate> --test <binary>`, which finds
  the build ready. A step has its own log, its own time and its own pass or fail line.
- Run one binary again: `cargo xtask gate batch --only "app suites: cmds"`. `--only "app suites"` selects
  each app step. `--failed gate-logs/<run>` runs only the red steps.
- Quick runs the steps one after the other. If a build step (`debug build (--no-run)` or
  `<crate> build (--no-run)`) fails, quick stops there (#3601). The summary lists each later step as
  `SKIP ... not run (build failed: <step>)`, and the gate exits red. Each such step gets a log with that one
  line, so `cargo xtask gate quick --failed gate-logs/<run>` runs the build step and each step that it
  stopped. If the build log of a `--no-run` step has a rustc metadata
  error (E0786 `found invalid metadata files`, E0460 `found possibly newer version`), the step prints a
  `BUILD FAULT` block: the crate of the error, then two things to try, one at a time: the same gate with
  `CAMCRAFT_GATE_JOBS=4`, and `cargo clean -p <package of the step>`. The gate does not retry or clean by
  itself. Batch and release run them in the lanes of `lanes.rs`: the builds
  first and alone, then a shared queue (longest first, by the newest `timings.tsv`), then the `ISOLATED`
  targets, the ui steps and clippy one at a time. A quick gate that falls back to batch uses the lanes too.
  `CAMCRAFT_GATE_JOBS=<cores>` sets the job budget that the lanes share.
- Each step starts cargo, about 0.5 s on Windows. A batch plan has about 700 steps.

## Batch lanes for separate runners (#2908)

`cargo xtask gate batch --lane N/M` runs one part of the batch plan. M jobs,
one for each lane, run the full batch tier. The lanes can run on M runners at
the same time, or one after the other on one runner. The result is the same.

- The split reads only the plan. It reads no timing file. Thus each machine
  computes the same lanes for the same commit.
- Each test step and clippy is in exactly one lane. The steps are dealt in
  plan order, one lane after the other: first the steps that share the
  machine, then the steps that run alone (`lanes::ISOLATED`, ui, clippy).
- A lane builds only the crates that it tests (`debug build: <crate>`).
- `--lane 1/1` is the full batch plan. M is 16 at most.
- `--lane` on a different tier, a lane out of range, or a lane with no step
  is an error with a message. It is not a green run.
- `--dry-run`, `--only` and `--failed` apply to the steps of the lane.
- A lane run writes `lane-result.tsv` into its `gate-logs/batch-<stamp>/`,
  with the step logs. The file has the lane, the runner name
  (`RUNNER_NAME`), the result (`green`, `red`, `void`), the wall time, the
  planned step count, and one row for each step that ran.

`cargo xtask gate-verdict <dir>...` reads each `lane-result.tsv` below the
directories (the `gate-logs/` of one machine, or the downloaded artifacts of
all lanes). It prints one Markdown verdict: a row for each lane with its
runner, time, result and record path, then the red steps by name.

- The exit code is 0 for green and 1 for red.
- The verdict is red when a step is red, and also when a lane has no record,
  has two records, is void, ran fewer steps than its plan, or has a record
  that cannot be read. A lane that did not report is never green.
- The failure of a red step is in the log of that step, in the artifact of
  its lane. No second run is necessary to read it.

`post-merge.yml` does not use the lanes yet. That workflow change is a
different PR (#2908).

## What quick selects

Quick prints its selection before it runs:

- The changed `#[test]` functions of a changed suite (`--exact`).
- The gate-mapped suites of a changed source module. The header of each
  suite has the map: `Gate map (M4.4): covers app::x, app::y...`.
- A change to shared test code (`tests/common`, goldens, snapshots) runs the
  suites of that crate.
- A change in `assets/posts/`, `assets/machines/`, `examples/`,
  `crates/post/src/` or `crates/motion/src/` runs the app suites that post
  the shipped examples: `example_catalog*`, `first_part_programs` and the
  others in `xtask/src/example_suites.rs` (#3521).
- A change in geometry, cl or a `Cargo.toml` falls back to batch.
- A change in `xtask/` runs the unit tests of xtask.
- `camcraft-kernel` runs in quick only when kernel is touched.

## Job budget (#2057)

- One budget sets the jobs of the whole gate. The default is the available
  CPU count from `std::thread::available_parallelism()`, with a floor of 2
  and a cap of 64. If CPU detection fails, the default is 2.
- `CAMCRAFT_GATE_JOBS=<n>` overrides it with any positive whole number.
  The override has no floor or cap. An empty value uses the default.
  A bad value uses the default and prints a diagnostic that names the value.
- A step that runs alone gets `-j <budget>` and `--test-threads=<budget>`.
  Builds, `ISOLATED` tests, UI steps and clippy still run alone.
- Batch and release queues share the budget. N lanes get
  `-j <budget / N>` each, minimum 1.
- A queue has at most `max(1, budget / 2)` lanes. A budget of 6 gives 3
  lanes of 2 jobs. A budget of 32 allows 16 lanes of 2 jobs.
- The gate prints the chosen budget and its source: available CPUs, the
  fallback, or `CAMCRAFT_GATE_JOBS`. It also prints the lane limit and the
  jobs of each lane. The `--dry-run` output shows the same information.
- **The sum of `-j` of all builds on one machine stays at or below its core
  count.** This includes gates, reviews, worker builds and plain `cargo`
  calls. When other builds run, lower the budget of each one (for example
  two gates at `CAMCRAFT_GATE_JOBS=3` on 6 cores). A plain `cargo` call
  takes its own `-j`.

## Stall watchdog (#3303)

- Each gate step runs under a watchdog. Output is the growth of the step
  log (the child and each descendant write there) and of a piped stdout.
- A debug step with no output for 15 minutes is a stall. A `--release` or
  `--ignored` step (a release link, a heavy test) gets 4 times the time:
  60 minutes.
- `CAMCRAFT_GATE_STALL_MIN=<n>` sets the debug stall time in whole minutes
  (1 or more). The release time is 4 times the value. A bad value uses the
  default and prints a diagnostic that names the value, as for
  `CAMCRAFT_GATE_JOBS`.
- At a stall the gate kills the process tree of the step. It prints
  `STALL <minutes>min <step>`, the last lines of the step and the processes
  that were alive. The log gets an `error: gate watchdog: STALL` line, thus
  `--failed` selects the step again.
- The summary has a `STALL` row, not a `FAIL` row. The other steps go on.
- Exit code: 1 for any `FAIL` row or a void run, 3 for `STALL` rows only,
  else 0.

## Slow binaries and heavy lanes

- Batch does not run the debug binaries in `SLOW_BINARIES`
  (`xtask/src/plan.rs`, pinned by its unit tests): motion `finish3d`,
  `pivot5d`, `flow`; app `center_cut_multiaxis`. Each takes 40-50 s. Release
  runs them.
- A test that takes more than about 60 s in debug moves to the heavy lane,
  or gets a smaller fixture. The ignore reason names the lane.
- Heavy lane tests are `#[ignore]`d. They measure geometry (envelope scans,
  full-precision projections) or generate over the large fixtures. Run them
  with `cargo test --release -p camcraft-app -- --ignored`, at a cut, in
  the cut gate on the cut runner. Do not run them for each edit or for
  each PR (AGENTS.md "Heavy lanes at review", #3556).
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
  - The command is a tool. Its result is not a condition for "ready" or
    for a merge (#3556).
- The targeted heavy step (AGENTS.md "Heavy lanes at review"): only a `P1`
  PR under the safety floor gets it. Find the binary that holds the named
  heavy tests of the changed code with `--dry-run`, then run its one step:
  `cargo xtask gate heavy --base main --only "<crate> heavy: <target>"`.
  Never run the whole reach for it. A `P2` or lower PR runs no heavy step.
- The wave form (#3460), an optional tool:
  `cargo xtask gate heavy --base origin/main --covers <PR>:<head sha>
  [--covers ...]` in the trial tree of a merge wave. The `merge-ready`
  skill makes the tree and posts the record when a session chooses to run
  it. No merge waits for it.
  - The flag is repeatable and applies to the heavy tier only. A bad value,
    and `--covers` with `--only` or `--failed`, is refused with the usage
    text.
  - The tool refuses a sha that is not a commit, a head that is not an
    ancestor of `HEAD`, and a working tree with changes.
  - The plan header and the summary print `covers: #<PR> <sha>` for each
    head, `tree: <HEAD^{tree}>` and `base: <ref> <merge-base sha>`.
  - A real run writes `covers.txt` to the log directory: the header, each
    step with its result and note, the counts (planned, ran, ok, failed),
    the wall time and the verdict. The verdict is `PASS`, `FAIL`,
    `INCOMPLETE` (a planned step did not run), `VOID` or `NO LANE` ("the
    wave reaches no heavy lane"). Only `PASS` covers the heads.
    `--dry-run` prints the header and runs nothing.
  - A wave record covers a PR only for the head that it names. A
    safety-floor PR is covered only if the trial tree contains its final
    head.
  - **Owed heavy result** (#3467) is retired (#3556): no PR is owed heavy,
    and no merge waits for a heavy record.
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
- Each gate cargo call gets `CARGO_INCREMENTAL=0`, and the gate removes the
  `CARGO_PROFILE_*` variables of the caller (#3150). These inputs change
  the unit hash. A different value gives each unit a new directory under
  `target/<profile>/build/`, and cargo does not remove the old one. The
  gate prints a note when `RUSTFLAGS` is set. The summary gives the number
  of unit directories that the run added: a second run on the same tree
  adds none. For a plain `cargo test` in a shared build worktree, use the
  same env.
- At the end of each run (not `--dry-run`), the gate removes superseded
  test-harness units (#3169). A unit goes only when this run wrote a newer
  variant of the same test target. Libraries and binaries stay. The sweep
  does nothing while a cargo lock is held or another process uses the
  tree's `target/`. The summary gives the directories and GB removed.
  `CAMCRAFT_GATE_SWEEP=off` turns the sweep off.

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
