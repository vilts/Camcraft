# CamCraft

A CAM system that turns 3D models into NC programs (G-code) for CNC milling
machines and lathes. Written in Rust on a Flecs ECS core with an egui
interface.

CamCraft is a clean-room implementation of HSMWorks' user-visible behavior:
its operation types, parameters and workflow, specced from published
documentation and general machining knowledge. "HSMWorks" is a trademark of
Autodesk; this project is independent and unaffiliated. No HSMWorks code,
text, assets or post processors were used.

> **Status: pre-1.0, for testing.** Every toolpath is verified against the
> part, the stock and the fixtures before it can be posted, and anything that
> cannot be proven safe is refused by name. Still, check every program in
> your machine's own simulation and dry-run it before cutting metal.

> **Experimental work in progress - use with caution.** CamCraft is under
> heavy, fast-moving development. Features land daily, some are half-built
> on purpose, and a release can change behaviour you relied on yesterday.
> Treat every posted program as a first draft: simulate it, dry-run it with
> the spindle off and the Z raised, and keep a hand on the feed hold. Our
> unofficial testing philosophy, borrowed from a certain tiny lord with a
> very large castle:
>
> *"Some of you may die, but it's a sacrifice I am willing to make."*
>
> Please don't let that be your spindle, your vise or your fingers. Report
> anything odd as an issue - a crash, a toolpath that looks wrong, or a
> refusal you think is mistaken - so the next person doesn't find it the
> hard way.

## What it does

- **Import** STL, OBJ and STEP models; repair meshes (weld vertices, fill
  holes).
- **Set up** stock, work offsets (G54–G59), fixtures and machines (mill,
  5-axis, lathe, mill-turn, and the Tormach 770MX, 770MX (ML), 770M,
  1100M, 1100MX, 1500MX, PCNC 440, PCNC 770 (earlier and Series 3) and
  PCNC 1100 (Series I, II and 3) mills).
- **2D milling:** face, contour (with G41/G42 compensation), pocket,
  adaptive clearing, slot, trace, engrave, chamfer, circular, bore and
  thread milling.
- **3D milling:** adaptive roughing per level, waterline, parallel,
  scallop, radial, spiral, morph, flow, pencil and steep/shallow finishing.
- **Drilling:** spot, drill, peck, deep, tap and bore cycles, with hole
  recognition.
- **Multi-axis:** 3+2 indexing, rotary contour/parallel/offset, 5-axis
  contour and swarf.
- **Turning:** facing, profile roughing and finishing, grooving, parting,
  threading, boring and axial drilling.
- **Area control:** pick faces, edges or loops, then set containment,
  offset, extension and extra or open boundaries on every operation that
  has an area.
- **Verify and simulate:** a solid-law verifier (gouge, rest, rapid and
  fixture checks), stock simulation with playback, and a deviation view.
- **Post:** Fanuc, Haas, Mazak, Sinumerik, Heidenhain, Tormach PathPilot
  and Fanuc Lathe, written as scriptable `.cps` post processors.
- **Setup sheets:** the Setups page lists each setup's tools in change
  order with cut and rapid lengths and estimated time, and **Export setup
  sheets…** writes one text page per setup. Posting does not write them.

## Download

Windows x64, Linux x86_64, and universal macOS builds are attached to
[GitHub releases](https://github.com/NeoTech/Camcraft/releases). On Windows,
unzip the portable archive and run `camcraft-ui.exe`, or use the installer.
On Linux, install the DEB or RPM and launch **CamCraft** from the application
menu (or run `camcraft`), or extract the TAR.GZ and run `./camcraft-ui` from
its extracted directory. The AppImage runs after `chmod +x` and needs no
installation. To install the Snap downloaded from Releases, run
`sudo snap install --dangerous ./camcraft-*.snap`; it can then be launched
with `camcraft`. To install the Flatpak bundle, run
`flatpak install --user ./camcraft-*.flatpak` and launch it with
`flatpak run io.github.NeoTech.Camcraft` (the Freedesktop runtime downloads
separately). On macOS, open the DMG and drag `CamCraft.app` to Applications,
or extract the ZIP. The macOS bundle is ad-hoc signed and not notarized, so
the first launch may require Control-click → Open. The portable archives
include the command-line tools and assets. `TESTING.md` in each archive lists
hand-test checks.

## Build from source

Requires Rust 1.90 or newer (edition 2024).

```sh
cargo build --release -p camcraft-ui        # the desktop app
cargo run   --release -p camcraft-ui        # build and start it
cargo build --release -p camcraft-app --bin camcraft-cli   # headless CLI
cargo build --release -p camcraft-post --bin post          # standalone post processor
```

### Arch Linux package

The `aur/` directory contains a source package recipe for the tagged
CamCraft release. On Arch Linux, build and install it with:

```sh
cd aur
makepkg -si
```

The recipe installs the desktop app, command-line tools, and assets together.
The release workflow builds and checks the Arch package after its Linux job
and attaches the package to tagged releases. Publishing the recipe to the
AUR is a separate step after its Arch build is validated.

### FreeBSD package

On FreeBSD amd64, build the desktop app and command-line tools, then package
them with their assets:

```sh
cargo build --locked --release -p camcraft-ui -p camcraft-app -p camcraft-post
scripts/package-freebsd.sh
```

The release workflow runs this in a FreeBSD VM. Manual workflow runs upload
the `freebsd-amd64.tar.gz` archive as an artifact; tagged runs attach it to
the release.

Windows, Linux, macOS, and FreeBSD desktop releases are built on GitHub.
The core crates also build and test on macOS.

## Workspace layout

| Crate | Role |
|---|---|
| `crates/geometry` | Meshes, slicing, 2D regions, mesh repair |
| `crates/cl` | The cutter-location (CL) move stream |
| `crates/tools` | Tool library and tool laws |
| `crates/motion` | Toolpath generators (2D, 3D, drilling, multi-axis, turning) |
| `crates/sim` | Stock simulation and the verifier |
| `crates/post` | The `.cps` post-processor engine |
| `crates/app` | Document model, commands (undo/redo), operations, posting |
| `crates/ui` | The egui desktop interface |
| `crates/kernel` | Shared kernel types |

`assets/` holds the post processors, machine definitions, UI icons, fonts
and themes. `testdata/` holds the test models.

## Testing

```sh
cargo test -p camcraft-app -p camcraft-motion -p camcraft-sim \
           -p camcraft-geometry -p camcraft-cl -p camcraft-tools -p camcraft-post
cargo test -p camcraft-ui                        # run from crates/ui
cargo test --release -p camcraft-app -p camcraft-motion -- --ignored   # slow verdicts
```

Golden toolpath and G-code outputs live under each crate's `tests/golden/`.
Gates run locally through `cargo xtask gate` (see Contributing). GitHub
Actions runs only on a pushed release tag; the CI and nightly workflows in
`.github/workflows/` are manual.

## Project documents

- The roadmap is the repo's open GitHub issues. The old milestone archive,
  `ROADMAP.md`, was retired; read it from git history
  (`git show v0.14.0:ROADMAP.md`).
- `RECOMMENDED.md`: the decision ledger, with what was chosen, why, and
  what was dropped.
- [Machine onboarding](docs/MACHINE-ONBOARDING.md): source facts, registration,
  software-test seams, and current limitations for machine definitions/posts.
- `AGENTS.md`: engineering rules (clean-room protocol, the never-crash /
  never-gouge directive, layering, testing).
- `RELEASE_NOTES.md`, `TESTING.md`: the current release.

## Reporting issues

Use the issue templates on GitHub: **Toolpath / path planning bug** for
anything about the machine's motion, **UI / UX bug** for the interface, and
the general bug and feature templates for the rest. If a path would gouge or
collide and the app did not warn you, please say so. Those reports matter
most.

## Contributing

CamCraft is built largely by AI coding agents working under
[`AGENTS.md`](AGENTS.md), with a human maintainer approving what lands.
Outside contributors follow the same flow. The full rules live in
`AGENTS.md` ("Issue Law" and "Commits and Releases"); this is the short
version.

### Tools

- **`git`** for branches and commits. Never force-push `main` or a branch
  with an open PR.
- **`gh`** (the GitHub CLI) for everything on GitHub: reading, filing,
  claiming and commenting on issues, and opening pull requests. Agents load
  the `gh-issues` skill for issue work.
- **`graphify`** keeps a knowledge graph of the repo in `graphify-out/`
  (generated, never committed or hand-edited). Agents query it before
  grepping (`graphify query "<question>"`) and refresh it after code changes
  (`graphify update`).
- **`cargo xtask gate`** runs the test gates, the same on Windows, macOS
  and Linux (the `xtask/` crate; nothing to install beyond cargo):
  `cargo xtask gate quick` per change (only the test functions and suites
  the diff touches), `cargo xtask gate batch` before a batch of commits,
  `cargo xtask gate release` at a release cut, and
  `cargo xtask gate test <name> [--crate c] [--exact]` for a single test.
  `--dry-run` prints the plan without running it. `cargo xtask decisions
  --version X.Y.Z` gathers the decision files at a cut.
- **`scripts/gate.ps1`** (`-Tier quick`, `-Test <name>`, ...) and
  **`scripts/gate.sh`** (`quick [base]`, `test <name> [crate] [--exact]`,
  ...) are thin wrappers over `cargo xtask gate`, kept so the old commands
  still work.
- Project agents and skills for Claude Code live in `.claude/agents/` and
  `.claude/skills/` (gate runner, graph updater, PR reviewer).

### Issues

Work is the open GitHub issues, picked by priority, `P1` to `P9`, lowest
number first and oldest first within a number: `P1` is wrong output, gouge,
crash or data loss; `P2` a broken main; `P3` wrong behaviour a user hits;
`P4` work the maintainer asked for; `P5` agent surface and tooling; `P6`
accepted enhancements; `P7` owed polish; `P8` ideas needing research; `P9`
someday (AGENTS.md "Priorities" has the table). `P1`-`P3` are defects only.
Each issue carries one `P*` label and one or
more `area:*` labels. Issues opened by an agent also carry `source:agent`.

**`P?` means paused.** The maintainer puts `P?` on an issue (in place of
`P1`..`P9`) to hold it: nobody picks it up, claims it or changes its
priority until the maintainer swaps `P?` for a real priority or closes it.
Comments with relevant facts (a duplicate, a related fix) are still
welcome.

**`needs-research` issues are not work items yet.** The label marks a large
idea with open questions (a new machine type, a documentation site, a new
kernel): research and decisions come first, and the outcome is a set of
smaller, concrete issues, each with its own `P*`, `area:*` and a "Done means"
line. Nobody claims a `needs-research` issue to write code. Its work is
answering the open questions in comments and filing the sub-issues; once
every sub-issue is filed and linked from it, it is closed, and the
sub-issues are picked up like any other issue.

1. **Claim before you code.** Comment
   `claimed: <name> · <UTC time> · branch <name> · plan: <one line>`. The
   maintainer adds the `claimed` label and assignee for contributors who
   cannot set labels. A claim with no word from its claimant for 4 hours is
   stale and may be taken over.
2. **Report on the issue.** Findings, blockers and decisions go in issue
   comments. New work you discover becomes a new issue.
3. **Finishing** means a pushed branch and an open pull request, not a merge.
   The issue moves from `claimed` to `in-review`.

### Branches and pull requests

- Start from the latest `main`: `git fetch origin`, then branch
  `issue/<N>-<short-name>` (agents) or any branch on your fork.
- Commit subjects follow [Conventional Commits](https://github.com/qoomon/conventional-commits):
  `feat:`, `fix:`, `docs:`, `test:` and so on. Mark breaking changes with `!`
  or a `BREAKING CHANGE:` footer. An incompatible project-file or post-output
  change counts as breaking.
- The PR description says what changed, how it was tested, and names the
  tests that pin it. It carries a `Changelog:` line with the one-line entry
  (or `none` for docs and tests). It ends with `Fixes #N`, or `Refs #N` for
  partial work. Do not edit `CHANGELOG.md` in the PR; rows are written at
  release time.
- Gate it locally before asking for review: `cargo xtask gate quick` (any
  OS; `scripts/gate.ps1 -Tier quick` and `scripts/gate.sh quick` forward to
  it).
- Follow the engineering rules in `AGENTS.md`: the clean-room protocol, never
  crash and never gouge (no `unwrap`, `expect` or `panic!` outside tests;
  refuse bad input by name), every UI change as an undoable command, files
  under about 400 lines, and a provenance line on new modules.

### Draft first, then ready for review: the gates

Every pull request opens as a **draft** (`gh pr create --draft`) and stays a
draft while work, testing and fixes land on it. A draft is never reviewed
and never merged. It is flagged **ready for review** (`gh pr ready <N>`)
only when one of these holds:

1. **The maintainer says so**, or
2. **Its own gates pass:**
   - **Mergeable** - it merges cleanly with current `main` (merge `main`
     in and re-test if not);
   - **Tested** - the quick gate is green on its final commit
     (`cargo xtask gate quick`), with the
     pinning tests named in the description;
   - **Complete description** - what changed, how it was tested, what was
     *not* exercised, a `Changelog:` line, and `Fixes #N` / `Refs #N` when
     it works an issue (a docs or process PR the maintainer asked for
     directly has none);
   - **Laws** - the `AGENTS.md` rules above (clean-room, no panics,
     commands for UI changes, ~400-line files; post scripts
     `assets/posts/*.cps` are exempt from the line limit).

   A quick pass over these points is enough to flag a PR ready: it reads
   the PR, its stated test results and its mergeability, and does not
   rebuild (the author ran the quick gate). The full independent review
   comes after, on the ready PR.

**Once ready, the branch takes no more pushes.** A late fix, a needed merge
of `main` or any other change first turns it back into a draft
(`gh pr ready <N> --undo`), says why in a PR comment, is pushed and
re-tested, and only then is flagged ready again.

**Review labels.** The review of a ready PR sets one colour:

| Label | Meaning | What happens |
|---|---|---|
| `merge-ready` (green) | clean review and mergeable | waits for the maintainer's merge |
| `needs-rework` (red) | review found blockers | a maintainer PR goes back to draft; fix, push, flag ready again (a fork PR stays as its contributor set it; the review comment says what to fix) |
| `conflicts` (orange) | no longer merges with `main` | a maintainer PR goes back to draft; merge `main`, re-test, flag ready again (a fork PR: the contributor merges `main`) |
| `needs-maintainer` (yellow) | reviewed with no blockers, but it adds a dependency or changes `build.rs`, scripts or `.github`, so it was read, not built | waits for the maintainer's decision |

When a PR comes back from draft with a new head, that head gets a fresh
review; the marker `<!-- pr-reviewer: <head sha> -->` in the review comment
ties each verdict to the commit it judged.

### Review and merge: a manual gate

`main` is protected. Every change reaches it through a pull request, admins
included; force-pushes and deletions are blocked; review conversations must
be resolved; only merge commits are allowed.

- **Review of fork PRs.** The `PR-reviewer` agent (run by `/review-prs`)
  trial-merges the PR with current `main`, builds it, runs the suites it
  touches plus clippy, checks it against `AGENTS.md`, and posts its verdict
  as a comment. It never approves and never merges. A new push to the PR
  gets a fresh review. A PR that touches build scripts, CI workflows,
  scripts or dependencies is not built by the reviewer; it is flagged for
  the maintainer instead.
- **Review of the maintainers' own PRs.** Once flagged ready, an owner PR
  is reviewed by a subagent that did not write it (the same `PR-reviewer`,
  one review at a time) and labelled as above; findings are fixed on the
  branch (back to draft first) before it merges - a `fix` once clean, a
  `feat` at the release cut.
- **Approval is manual.** GitHub requires 0 approvals (the agents and the
  maintainer share one account, which cannot approve its own PR), so the
  gate is the maintainer's manual merge: the maintainer reads the review and
  approves and merges pull requests from forks by hand. Address the
  review's requested changes and push again to get there.
- **Workflows on forks** need the maintainer's approval to run, so a fork PR
  never spends the Actions budget on its own.
- **Docs-, process- and CI-only PRs** merge once reviewed, without
  waiting for a cut.
- **Releases.** Issue PRs merge at a release cut: reviewed, merged to `main`
  with merge commits, gated on a `release/vX.Y.Z` branch, merged through a
  release PR, then tagged. Versions follow semantic versioning from the
  commits (`fix` = patch, `feat` = minor, breaking = major; while the major
  is 0, breaking bumps the minor). Each issue gets a `shipped in vX.Y.Z`
  comment when its release is published. Only `vX.Y.0` tags (and manual
  runs) build the release artifact on GitHub.

## Supporting the project

The best way to support CamCraft is to point your coding agent at it. Pick an
open issue, or a pull request that needs work or a review, and let your agent
take it through the process described under [Contributing](#contributing).
Every fixed issue and every reviewed PR moves the project forward.

If you'd rather support it another way, you can
[chip in here](https://buy.stripe.com/8x2dR93Up10S6eP0FO4c800) or use the
**Sponsor** button at the top of the repository page. Contributions go
toward more agent subscriptions, which means more agents working through the
issue list.

## License

Apache License 2.0. See [`LICENSE`](LICENSE) and [`NOTICE`](NOTICE).

The copyright belongs to the CamCraft contributors, each holding the
rights to their own contributions, and not to any one person. By opening a
pull request you license your contribution under Apache-2.0 (section 5 of
the license). Code contributed while CamCraft was MIT-licensed is
redistributed under Apache-2.0, which MIT permits.

Third-party notices are in `assets/ui/LICENSES.md`.
