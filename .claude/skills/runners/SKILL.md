---
name: runners
description: The self-hosted GitHub Actions runners and workflows of NeoTech/Camcraft - the cut runner (`cut.yml`), the post-merge gate (`post-merge.yml`), the PR test runner (`pr-test.yml`), the billed-minutes budget of `release.yml`, machine requirements, runner labels, registration and the safety rules. Use when you install or change a runner, change a file in `.github/workflows/`, start or read a `pr-test.yml`, `post-merge.yml` or `cut.yml` run, or must decide if a trigger is permitted.
---

# runners - self-hosted runners and the workflows on them

The rules are in AGENTS.md: "GitHub Actions minutes are budgeted", "The cut
runner", "The post-merge gate", "The PR test runner". This skill has the
details. If the two disagree, AGENTS.md is correct.

## Triggers and budget

- The budget is 3000 billed minutes each month. Windows counts 2x and macOS
  10x.
- Nothing runs on a PR trigger. The quick and batch tiers run locally
  (`gate` skill) or on a self-hosted runner.
- The permitted triggers are `workflow_dispatch`, the tag push of
  `release.yml`, and the push to `main` of `post-merge.yml` (self-hosted
  jobs only).
- Never add a `push`, `pull_request` or `schedule` trigger without the
  maintainer's approval.
- Never add a trigger that a fork PR can start to a workflow with
  self-hosted jobs. A fork PR must not run code on the maintainer's machine.
- `release.yml` runs on a cut tag and publishes the artifact.
  `aur-package.yml` is a job that `release.yml` calls. `ci.yml` and
  `nightly.yml` are removed.
- `freebsd-backfill.yml` (#2378) is `workflow_dispatch` only. It builds the
  FreeBSD package of an existing `vX.Y.0` tag (input `tag`) and attaches it
  to that release. It never moves a tag and never replaces an asset.
- `cut.yml` (#1413) runs the release tier of a cut on the cut runner. It
  starts `release.yml`; it does not replace it.
- Billed minutes of one cut: `cut.yml` 0 (self-hosted). `release.yml` about
  230 by the job times of v0.18.0 (Linux jobs about 50, Windows 10 x2, macOS
  16 x10). The measured value is still to be recorded from the release run
  of the first `cut.yml` cut (Actions -> the run -> Usage).

## The cut runner (#1413)

The jobs of `cut.yml` run on `runs-on: [self-hosted, linux]`, never on a
GitHub-hosted runner. The machine needs:

- Linux x86-64, registered to NeoTech/Camcraft at repository level, with
  the labels `self-hosted`, `Linux`, `X64`. An added name label is
  permitted. Size: 16 cores and about 64 GB RAM. Fewer cores work, but
  slower.
- Job budget (#2057): the sum of `-j` of all builds on one machine stays
  at or below its core count. The gate uses `CAMCRAFT_GATE_JOBS` (default
  6) for all its lanes together. A runner that is the only build on its
  machine can set `CAMCRAFT_GATE_JOBS` to its core count in its `.env`. A
  runner that shares the machine with other builds keeps the sum of their
  budgets at or below the core count.
- Disk: 100 GB free or more for the work directory (a clean `target/` with
  the release heavy lanes is 20-30 GB), plus the sccache cache.
- On the PATH of the runner service: the Rust toolchain (`~/.cargo/bin`, a
  stable rustc that builds the workspace), `git`, `gh`.
- The Linux build packages that the linux job of `release.yml` installs:
  `pkg-config`, `libwayland-dev`, `libxkbcommon-dev`, `libx11-dev`.
- For the UI suites, which open windows: `xvfb` (the workflow puts the gate
  in `xvfb-run -a` when no display is set) and the Mesa software drivers
  (`mesa-vulkan-drivers`, `libgl1-mesa-dri`).
- sccache: in the `.env` of the runner, set `RUSTC_WRAPPER` to the sccache
  binary and `SCCACHE_CACHE_SIZE` (for example 40G). Never set
  `CARGO_TARGET_DIR`. Each run builds in the `target/` of the workspace,
  which `actions/checkout` cleans; the compile cache keeps that cheap.

Registration:

1. The maintainer makes a token: Settings -> Actions -> Runners -> New
   self-hosted runner, Linux x64.
2. In the runner directory: `./config.sh --unattended --url
   https://github.com/NeoTech/Camcraft --token <token> --name <name>
   --labels <extra labels> --work <work dir>`.
3. Run it as a service: `./svc.sh install`, or a user systemd unit with
   linger enabled, so that it continues after logout.

Safety:

- Run the runner as an account without privileges. No sudo without a
  password. The `docker` group is equal to root.
- Keep "Require approval for all outside collaborators" on for fork PR
  workflows (Settings -> Actions -> General).
- Never approve the workflow run of a fork PR that adds or changes a file
  in `.github/workflows/`. Such a file can name `self-hosted`.

## The post-merge gate (#1451)

- `post-merge.yml` runs `cargo xtask gate batch` on each push to `main`,
  and on `workflow_dispatch` (input `ref`). It never runs on a PR trigger.
- Its jobs use the runner label `camcraft-post-merge` (the Linux runner
  `uma`). That runner does not have `camcraft-pr-test`, thus a PR test never
  waits for a gate.
- A group of merges tests only the newest head (`cancel-in-progress`).
- It gives way to the cut. It skips itself (green, with a notice) while a
  `cut.yml` run is queued or in progress, or while an open `release/vX.Y.Z`
  PR from this repository exists (`.github/scripts/post-merge-skip.sh`).
- It reports through `.github/scripts/post-merge-report.sh`. Only its
  report job has `issues: write`.
- A red run opens or comments on the one issue `main is red (post-merge
  gate)`. The issue is `P2`. It is `P1` only when a failed suite is a
  wrong-output suite: a golden CL/NC suite or a recorded-post-stream suite,
  named in the anchored `OUTPUT_SUITES` list of the script.
- The next green run closes the issue. The workflow closes only an issue
  with `source:agent`.
- If the runner is down, use the local batch gate (`gate` skill).

## The PR test runner (#1420)

`pr-test.yml` runs the build and tests of a PR review on the maintainer's
self-hosted runners. It bills no minutes and needs no 10-20 GB worktree on
the reviewer's machine.

- Trigger: `workflow_dispatch` only. Inputs: the PR number and the head SHA
  that the reviewer read. The token is read-only. There are no secrets.
- It checks out `refs/pull/<N>/head` and refuses a different head.
- It refuses a diff (listed with `--no-renames`) that touches: `build.rs`,
  `.github/`, `scripts/`, a `.cargo/` directory at any depth, a
  `Cargo.toml` or `Cargo.lock`, a toolchain file, `xtask/`.
- It trial-merges with main, then runs `cargo xtask gate quick --base
  origin/main` and clippy on the touched crates. The ui suites run in
  `xvfb-run` with the Mesa software drivers.
- Only the PR-reviewer agent starts it, after it read the diff.
- It runs only for a collaborator's PR. The workflow checks the author's
  push access and refuses other authors. Other PRs build locally. The
  refusal list does not stop hostile test code: that is the reason for this
  rule.
- Start it only when a runner is online and no cut uses the runner. Never
  send a "read, not built" PR.
- If there is no runner, or the run is cancelled, times out or does not
  start, the review builds locally or says "not built". It never says
  "pass".
- The job uses the runner label `camcraft-pr-test`. Each self-hosted runner
  with the label can take it: Linux (`len`) or macOS (`m3`).
- On macOS the UI suites open real windows, thus that runner needs a
  logged-in session. The per-OS pinned readings run the arm of that OS.
- Each PR test runner has its own OS user, HOME, cargo home and sccache
  directory. It shares none of them with the cut runner.
- Windows-only readings (`cfg(windows)`, per-OS pinned values) run locally
  for the touched suites.
