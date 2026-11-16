# scripts

## Gate tiers (`cargo xtask gate`)

`cargo xtask gate` runs the test gate in three tiers so a change never costs
hours - one Rust implementation (`xtask/`, #748) on Windows, macOS and
Linux. Every step is one cargo command (`-j 6`, `--test-threads=6`), logged
to `gate-logs/<tier>-<timestamp>/`; the run ends with a per-step pass/fail
summary (failing binaries named) and a non-zero exit on any failure.
`--dry-run` (or `--plan`) prints the plan without running anything.

| Tier | When | What |
|---|---|---|
| `quick` | every patch (agent / gate-runner), ~minutes | what the diff touches, in debug |
| `batch` | before committing a batch | every debug suite minus `SLOW_BINARIES`, ui (CWD `crates/ui`), clippy `-D warnings` |
| `release` | at a cut | everything: all debug suites incl. slow and kernel, ui, clippy, then `--release -- --ignored` for motion and app |

The tiers run locally only (the Actions budget): nothing runs on push, PR or a
schedule.

The command is the same on every OS:

    cargo xtask gate quick [--base main] [--heavy]
    cargo xtask gate batch
    cargo xtask gate release

from PowerShell or cmd on Windows, or a shell on macOS / Linux, e.g.

    PS C:\src\camcraft> cargo xtask gate quick --dry-run
    ~/src/camcraft$ cargo xtask gate quick --base origin/main

The old entry points still work and forward to it: `scripts/gate.ps1`
(`-Tier quick`, `-Test <name>`, ...) and `scripts/gate.sh` (`quick`,
`test <name>`, ...).

**quick** diffs the working tree against `merge-base(<base>, HEAD)`
(committed + uncommitted + untracked) and prints what it chose before
running it:

- a changed suite file (`crates/<c>/tests/<b>.rs`) runs only the `#[test]`
  fns whose lines the diff hunks touch (`--exact`); a change outside any test
  fn (header, helpers, `use`) or a new suite runs the whole binary. Changed
  `#[ignore]`d fns are heavy-lane: skipped (the binary is still compiled)
  unless `--heavy`, which runs them `--release -- --ignored`.
- a changed source file (`crates/<c>/src/<mod>/<sub>.rs`) runs the crate's
  lib tests plus the suites whose `Gate map` header names the module -
  narrowed to the suites naming `mod::sub` / `mod (.., sub, ..)` when any
  do. Suites of other crates match when they name `<c>::<mod>`.
- `tests/common`, `tests/golden`, snapshots or a crate `Cargo.toml` run the
  crate's lib and all of its suites.
- a touch to a shared crate (`geometry`, `cl`) or the workspace manifest /
  lock falls back to `batch`.

`SLOW_BINARIES` (`xtask/src/plan.rs`) lists the debug binaries over ~40 s;
batch skips them, release runs them. A test that takes more than ~60 s in
debug belongs in the heavy lane (`#[ignore = "heavy lane: ..."]`), not here.

### Single tests

    cargo xtask gate test <filter> [--crate app] [--binary operations] [--exact]

Runs exactly the matching `#[test]` fn(s) and nothing else. Without
`--binary` the binary is found by searching `fn <filter>` under
`crates/*/tests` and `crates/*/src` (`--crate` narrows it); a substring
filter by default, the exact fn name with `--exact`. A match that is
`#[ignore]`d runs in release with `--ignored`; lib unit tests run via
`--lib`; ui tests run from `crates/ui`. Example:

    cargo xtask gate test an_open_level_holds_the_planner_laws --exact

## Decisions

`cargo xtask decisions --version X.Y.Z` gathers `docs/decisions/*.md` into
`RECOMMENDED.md` at a cut (release step 2); `collect-decisions.ps1` /
`.sh` forward to it.

## Release

`cargo xtask release` cuts a release (xtask/src/release*.rs, #749):
preflight, the release gate tier (in process), version bump, commits
(`--no-tag` on a `release/vX.Y.Z` branch: no tag - main is tagged after the
release PR merges, AGENTS.md "A cut goes PR review"), release build of the
product packages, signing (Windows, if configured), the portable zip (the
section below, `cargo xtask package`) and, on request, the installer
(Windows). On Linux, macOS and FreeBSD the packaging phase runs that OS's
`package-*.sh` instead, and `--installer` is refused by name.
`release.ps1` is a thin wrapper forwarding its old switches.

    cargo xtask release --version 0.8.0 --no-tag --title "..."

Switches: `--skip-gate`, `--skip-build`, `--installer`, `--dry-run` (prints
every command, changes nothing). The wrapper takes the old spelling
(`-SkipGate`, `-SkipBuild`, `-Installer`, `-DryRun`).
Update RELEASE_NOTES.md / TESTING.md first; the cut commits them.
It never pushes; it prints the push command for you to run.

## Portable zip (`cargo xtask package`)

The Windows portable tree and its zip, and nothing else (no bump, no
commit, no build). The layout lives once, in `xtask/src/release_pack.rs`:
`cargo xtask release`'s zip phase and `release.yml`'s Windows job both call
this, so the local cut and the published release cannot drift apart.

    cargo xtask package [--version X.Y.Z] [--out dist] [--bin-dir target/release] [--dry-run]

- Stages `<out>/camcraft-<ver>-win-x64/`: `camcraft-ui.exe`,
  `camcraft-cli.exe`, `post.exe` from `--bin-dir`; `assets/{posts,machines,
  ui,scripts,themes}`; `RELEASE_NOTES.md`, `TESTING.md`, `LICENSE`, `NOTICE`.
  Then zips that folder to `<out>/camcraft-<ver>-win-x64.zip` with
  PowerShell's `Compress-Archive` (`pwsh` when on PATH, else Windows
  PowerShell). The Inno Setup installer is built from the same stage.
- Without `--version` the version is read from `<bin-dir>/post.exe
  --version` - the binary we ship is its one truth. Relative paths are
  taken from the workspace root.
- Under GitHub Actions it appends `version=`, `stage=` and `zip=` to
  `$GITHUB_OUTPUT` and `CAMCRAFT_VERSION=` to `$GITHUB_ENV` (the installer
  and publish steps read them).
- The zip is the Windows package: a real run on macOS / Linux / FreeBSD is
  refused by name (their packages come from `package-*.sh`); `--dry-run`
  prints the file list on any host.

## Installer (M13.3)

The portable zip stays the release and the primary test vehicle. The
installer is additive: `installer.ps1` compiles `installer/camcraft.iss`
(Inno Setup 6) against the SAME staged tree the zip was made from and
writes `dist/camcraft-<ver>-win-x64-setup.exe` beside the zip.

    cargo xtask release --version 0.8.0 --no-tag --installer         # as a phase of the cut
    powershell -File scripts/installer.ps1 -Version 0.8.0            # on an existing dist/ stage

- Per-user by default: no administrator prompt, installs under
  `%LOCALAPPDATA%\Programs\camcraft`, Start Menu shortcuts (app, hand-test
  checklist, release notes, uninstaller), optional desktop shortcut. The
  first page offers an all-users install (Program Files, elevated).
- Upgrades install over the previous version in place (fixed AppId); the
  uninstaller removes what was installed and leaves user files alone.
- Locally, a missing `ISCC.exe` is a named skip ("installer skipped: ...")
  and the cut still succeeds with the zip. In CI (`release.yml`) Inno Setup
  is preinstalled on `windows-latest` and the step is required.
- Why Inno Setup: one script file, no extra toolchain on the runner, free
  for any use; WiX needs its own CLI and XML authoring, MSIX cannot install
  at all unsigned.

## Signing (M13.3)

`sign.ps1` signs exes and the installer with `signtool` (SHA-256 digest,
RFC-3161 timestamp so signatures outlive the certificate). It is called by
`cargo xtask release` after the build and before the zip; `release.yml` does
the same.

The installer and the uninstaller Inno Setup embeds in it are signed by
ISCC itself, through `sign.ps1`: when `sign.ps1 -Status` reports a
certificate, `installer.ps1` passes `/DSignToolName=camcraft` (which turns
on `SignTool=camcraft` and `SignedUninstaller=yes` in `camcraft.iss`) and
`/Scamcraft=powershell.exe ... sign.ps1 -Path $f`, so ISCC runs the same
signtool command on the uninstaller and on the setup exe. Without a
certificate neither switch is passed (`installer signing: off ...` in the
log) and the build is the unsigned one it always was.

With no certificate configured it prints `unsigned: no certificate
configured` and exits 0: the unsigned release is a legitimate release, as
every cut so far has been (SmartScreen will warn on first run). A
certificate that IS configured but cannot be used (missing signtool, bad
path, signtool failure) fails the cut - a half-configured signing setup is
an error, silence is not.

Configure through the environment only; nothing is committed:

| Variable | Meaning |
|---|---|
| `CAMCRAFT_SIGN_THUMBPRINT` | SHA-1 thumbprint of a certificate in your Windows certificate store (local cuts) |
| `CAMCRAFT_SIGN_CERT_PATH` + `CAMCRAFT_SIGN_CERT_PASSWORD` | a PFX file and its password |
| `CAMCRAFT_SIGN_TIMESTAMP_URL` | optional; overrides the default public RFC-3161 server |

In CI the optional repository secrets are `CAMCRAFT_SIGN_CERT_BASE64` (the
PFX, base64-encoded) and `CAMCRAFT_SIGN_CERT_PASSWORD`; the workflow writes
the PFX to the runner's temp dir, signs, and deletes it. Unset secrets mean
an unsigned release, not a failed one.

Obtaining a certificate: buy an OV or EV code-signing certificate from any
public CA (EV ones live on hardware tokens or a cloud signing service, which
is the local-thumbprint route).
