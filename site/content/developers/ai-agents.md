+++
title = "Contributing with AI agents"
description = "Getting started with Claude Code on CamCraft, the issue-to-merge flow, and what we learned running agents on several machines."
weight = 5
+++

Most CamCraft changes are written by AI coding agents (Claude Code, Codex
and others) that contributors run on their own machines. This page is the
practical side: how to start, how work flows from an issue to `main`, and
the gotchas we hit. The rules themselves live in
[`AGENTS.md`](https://github.com/NeoTech/Camcraft/blob/main/AGENTS.md); where
this page and `AGENTS.md` differ, `AGENTS.md` is right.

Questions and coordination happen on the
[CamCraft Discord](https://discord.gg/bBWjkAgX5).

## Your first hour

1. **Say hello** on the [CamCraft Discord](https://discord.gg/bBWjkAgX5): what
   CNC machine you have, and what you would like to work on.
2. **What you need:**
   - a GitHub account, and the [GitHub CLI](https://cli.github.com/) logged
     in (`gh auth login`) - agents use it to read issues and open PRs;
   - [Rust](https://rustup.rs/) stable, and `git`; on Linux the GUI build
     also needs `pkg-config`, `libwayland-dev`, `libxkbcommon-dev` and
     `libx11-dev` (Debian/Ubuntu names);
   - an AI coding agent. Claude Code needs a paid Claude plan or API credits;
     a Pro plan is enough to start, heavier use wants a Max plan. Other agents
     (Codex and the like) work too - they read the same `AGENTS.md`;
   - free disk: about 30 GB to start, more per extra worktree (see below).
3. **Fork, clone, build:** fork `NeoTech/Camcraft` on GitHub, clone your fork,
   add the original as `upstream`, then see it run:

   ```sh
   cargo run --release -p camcraft-ui -- examples/first-part.camproj  # the app, with a sample part
   cargo xtask gate quick                                            # the tests your change touches
   ```

   The first build takes a while; later ones are fast. The `/fork-sync`
   skill keeps your fork's `main` and process files in step with the
   project.
4. **Find a first issue:** open, unclaimed bugs with a small effort label -
   [P3/P7 bugs, effort low or medium](https://github.com/NeoTech/Camcraft/issues?q=is%3Aopen+label%3Abug+-label%3Aclaimed+-label%3Ain-review+label%3Aeffort%3Alow%2Ceffort%3Amedium).
   Read the issue and its comments, then claim it with a comment.
5. **Ask your agent to do it:** in Claude Code, `/work-issue <number>` runs the
   whole flow below. Read its draft PR before you mark it ready.

## Getting started with Claude Code

1. Install [Claude Code](https://claude.com/claude-code) and a Rust stable
   toolchain, then fork and clone the repository.
2. Start Claude Code in the clone. It reads `AGENTS.md` automatically, and the
   project skills in `.claude/skills/` are available as slash commands:
   - `/work-issue` - claim one issue, branch from the latest `main`, fix it
     with a pinning test, run the quick gate, open a draft PR.
   - `/gh-issues` - read, file and update issues the way the project expects.
   - `/gate` - run a test gate tier.
3. Optional: CamCraft has an MCP server (`camcraft-cli mcp`), so an agent can
   drive a CAM document headlessly to reproduce a bug or check a result
   ([AI and automation](@/guide/automate/ai.md)).

A good first task is a `P3` or `P7` issue with `effort:low` or
`effort:medium`. Bugs (`P1`-`P3`) are always the most welcome.

## From issue to `main`

1. **Look at what is open first.** Before you start, search the open PRs and
   issues for the same change: another number for the same bug, or a PR that
   changes the same files (`gh pr list --state open --json
   number,title,files`). If someone has it, comment there instead of
   starting a second copy - two PRs for one change cannot both land.
2. **Pick and claim.** Open issues are the work list, ordered by priority
   (`P1` first; `P?` is paused, leave it alone). Claim an issue by
   commenting `claimed: <who> · <plan>` - a comment is a claim. Read the
   comments first: someone else's live claim means the issue is taken.
3. **Branch from the latest `main`**, one issue per branch, ideally one git
   worktree per issue.
4. **Fix with a pinning test** - a test that fails before the fix and passes
   after. Run the quick gate: `cargo xtask gate quick --base main`.
5. **Open a draft PR.** The body is checked by reviewers, so include it all
   the first time:
   - what changed, the readings (numbers, before and after), the pinning
     test names, and what was *not* exercised;
   - `Changelog:` with one line (or `none` for docs/tests);
   - `Fixes #N` (or `Refs #N` for partial work);
   - `Process change: yes` if you touched a process file (`AGENTS.md`,
     `.claude/`, `xtask/`, the gate scripts);
   - **before/after screenshots** for any visible UI change, as images on
     an images-only branch of your fork (never in the PR branch itself);
   - a reason for any new top-level test binary (or add the test to an
     existing suite).
6. **Mark it ready** when the gate is green and the body is complete.
7. **Review.** The project's reviewers check it: they build it merged with
   `main`, run the tests it touches and read it against the project rules.
   Changes that can affect posted G-code, collisions, simulation or geometry
   get a deeper safety review. Findings come back as a PR comment; fix them
   on the same branch with a plain push (never force-push).
8. **Merge.** Once the review is clean, the maintainers merge it. If `main`
   moved and your PR conflicts, merge `main` into your branch when asked -
   you do not have to chase every merge.

**A series starts with its shape.** If a feature needs several stacked PRs,
open the first one first and say it starts a series. When it sets a shape
others build on (a type, a file format, an API, a project-file field), it
gets a design review before the later parts go in - cheaper than reworking
five PRs.

Missing paperwork (screenshots, `Process change:`, `Changelog:`) is the most
common reason for a second review round. Ten seconds on the body saves an
hour of re-review.

## Tips and gotchas

- **Disk.** Each worktree has its own `target/`: 10-25 GB for a normal
  build, and 150-200 GB once the release-mode heavy lanes have run in it.
  Remove a worktree once its PR merges (`git worktree remove`), and watch
  free space if you run several agents.
- **Compile cache.** [sccache](https://github.com/mozilla/sccache) makes a
  fresh worktree cheap: `cargo xtask gate` uses it automatically when it is on
  `PATH`. Never set `CARGO_TARGET_DIR` (it breaks the cache across worktrees
  and mixes crates between concurrent builds). If builds hang, an sccache
  server may be wedged: `sccache --stop-server` and build again.
- **One gate per checkout.** Do not pull or switch branches in a checkout
  while a gate runs there; give each concurrent job its own worktree.
- **Long commands.** Builds and gates take minutes. Run them in the
  background (or in a subagent) instead of blocking the session.
- **Failures that are not yours.** If a heavy test fails, check it on bare
  `main` first: a failure that also happens on `main` does not block your PR.
- **Windows.** The repository pins LF line endings (`.gitattributes`). GPU UI
  tests can be flaky on hybrid-GPU laptops (two graphics adapters).
- **macOS.** The default APFS volume is case-insensitive: tests that create
  files whose names differ only by case behave differently there.

## More than one computer

Agents scale well across machines if each one runs its own small job pool:

- **A queue per machine.** A text file of jobs (`work N`, `review N`,
  `fixup N`) and a maximum number of concurrent jobs; a loop starts the next
  job when a slot frees. A coordinator session feeds the queues over SSH.
- **One worktree and one `target/` per job.** Shared build directories
  corrupt each other.
- **Avoid duplicate work.** Before queuing a review or a fix, check that no
  other machine is already running one for the same PR and commit. Two
  reviewers in one worktree overwrite each other.
- **Match the job to the machine.** Big PRs (gates of 1,000+ test steps)
  belong on the fastest machines; small reviews run anywhere.
- **Pace the subscriptions.** Give each AI plan a weekly budget and spread
  it over the week, so a busy day does not exhaust it; send cheap work
  (triage, issue sweeps, research) to cheaper models.

## Using subagents

- Let **subagents** run gates, reviews and research while the main session
  keeps planning; their results come back as a short summary.
- **Never review your own change** with the agent that wrote it - an
  independent reviewer finds what the author's context hides.
- Use the **stronger model for safety-tier work** (toolpaths, posts,
  simulation, geometry) and cheaper ones for paperwork checks and triage.
- Keep subagent briefs specific: the issue or PR number, what to check, and
  what not to touch.

## Not a coder? Ways to help without an agent

### The issues are the backlog

Every piece of work on CamCraft is an open
[GitHub issue](https://github.com/NeoTech/Camcraft/issues): bugs, features,
research, docs. There is no separate roadmap. That means the issue list is
where help lands, even without writing code:

- **Read and comment.** "I see this too, on <machine>, in version X" or a
  model that shows the problem makes an issue easier to fix and moves it up.
- **File good issues.** Use the templates (bug, toolpath, UI/UX, feature).
  A good bug report has: what you did, what happened, what you expected,
  the CamCraft version, your machine and controller, and the project file
  (`.camproj`) or the model if you can share it. One problem per issue.
- **Check fixes.** When an issue says "fixed in PR #N", trying it on your
  machine and commenting the result is real help.

### Contribute with designs

Test parts are valuable: real shapes find real bugs. If you design parts
(in SolidCraft or any CAD):

- Share **your own** models, or ones whose license allows it, with the issue
  they show a problem in (attach a STEP, or the `.camproj` with it).
- A part that "should be easy but CamCraft refuses or cuts it wrongly" is the
  best kind. Say what you expected to happen.
- Do not upload vendor CAD or downloaded models whose license forbids
  sharing; describe the shape instead.

## I have a CNC machine - how do I get it working in CamCraft?

1. **Check what exists.** CamCraft ships machine files in
   [`assets/machines/`](https://github.com/NeoTech/Camcraft/tree/main/assets/machines)
   and posts (G-code dialects) in
   [`assets/posts/`](https://github.com/NeoTech/Camcraft/tree/main/assets/posts):
   Mach3, Mach4, LinuxCNC, grbl/grblHAL, Tormach PathPilot, Fanuc, Haas,
   Heidenhain, Sinumerik, Mazak and more. If your controller's dialect is
   there, pick a similar machine file and set your travel - you can start
   today.
2. **If your CNC machine is missing**, open an issue (or ask on Discord)
   titled `Machine: <make model>` with:
   - the controller and its version (for example "Mach3", "LinuxCNC 2.9",
     "Fanuc 0i-MF");
   - travel in X, Y and Z, and inch or mm;
   - spindle speed range, tool changer or manual changes, coolant, probe;
   - which work offsets you use (G54...);
   - a short program the controller is known to accept that you wrote by
     hand or took from the controller's manual.
3. **First cut is an air cut.** Run every new program above the part (or in
   the controller's simulator) before you cut metal or wood, and report what
   the controller did - alarms, the line it stopped on, anything unexpected.

## Clean room: what we can and cannot use

CamCraft is a clean-room project: it is built only from public standards,
controller and machine manuals, general machining knowledge and our own
tests - never from another CAM product's code or output. That keeps the
project free to use and share. In practice:

- **Welcome:** your own models and project files; programs you wrote by
  hand; controller and machine manuals; public G-code references
  (RS-274, ISO 6983); measurements and photos from your own machine.
- **Please do not send:** G-code or post processors produced by another CAM
  program (for example Fusion 360 or HSMWorks), their tool libraries,
  screenshots of their dialogs or text copied from their help. Even as a
  "this is how it should look" example, we cannot read it. Describe the
  behaviour you want in your own words instead.

## Your agent helps with more than code

Ask it, in plain words, whenever something is in the way:

- **Explain a step.** "What does this review comment ask me to do?", "What
  does this error mean?", "Walk me through this issue."
- **Screenshots.** It can crop a screenshot to the part that matters, or
  render a CamCraft panel headlessly through the UI test harness - handy for
  the before/after images a UI change needs.
- **Write it up.** Turn what you saw into a clear bug report, a PR
  description or a comment, with the details reviewers look for.
- **Read the manual for you.** Find the part of your controller's manual
  that matters, or check a short G-code program line by line.
- **Check before you send.** "Does my PR body have everything the
  reviewers want?"

## If something is unclear

1. Ask your agent first - it has read the project rules.
2. Ask on the [CamCraft Discord](https://discord.gg/bBWjkAgX5). No question
   is too small.
3. If this page or the guide is unclear, that is a bug: say so on Discord or
   open an issue. We would rather fix the docs than lose a helper.

## We are all volunteers

Everyone here gives their own time, and every bit helps: a comment, a test
on your machine, a typo fix, a model that shows a bug, a PR. Be patient -
a review can take a few hours or a day. Be kind - assume good intent,
thank people, and keep feedback about the work, not the person. New
helpers are always welcome.

## FAQ

**I don't know Rust. Can I still help?**
Yes. The agent writes the Rust; you steer it, read what it says it did,
and check the result does what the issue asks. Plenty of help needs no
code at all: testing a build on your real machine and controller, sending a
model and the G-code that went wrong, checking a post against your
controller's manual, trying a release and saying what confused you, or
improving the guide.

**AGENTS.md is long. Do I have to read it?**
No - your agent reads it. Start with "Your first hour" above. When a review
asks for something, the comment says what and why.

**I'm worried I will break something.**
You can't break `main`: every change goes through a PR, a review and the
tests, and nothing reaches a machine without that. A draft PR that goes
nowhere costs nothing; close it and try another issue.

**My agent went off in the wrong direction.**
Stop it, look at what it changed (`git diff`), and either guide it back or
start over on a fresh branch. Keep issues small: one bug, one PR.

**How much does it cost, and how much time?**
A small fix is usually one agent session and an hour or two of your
attention spread over a day (the review happens while you do other things).
Pick `effort:low` or `effort:medium` issues at first; they use the least of
your AI plan.

**My computer is small or slow.**
Work one issue at a time in one worktree, install sccache, and remove
worktrees after their PRs merge. Testing and reporting help just as much
as code.

**I have a CNC machine but no development setup.**
CNC machine owners are some of the most useful helpers: real controllers
catch what tests cannot. See "I have a CNC machine" above, and say what you
have on Discord; we send small programs to check (always air-cut or dry-run first).

**I'm overwhelmed. Where do I even start?**
Say hello on Discord and tell us what you have and what you enjoy. Someone
will point you at one small thing. Every contributor here started with one
small issue.

## Where to talk

- [CamCraft Discord](https://discord.gg/bBWjkAgX5) - questions, test
  machines, coordination.
- [craftydevs.org](https://www.craftydevs.org/) - CamCraft and SolidCraft,
  the open-source CAM and CAD pair, with links to each project's site,
  GitHub and status dashboard.

Thanks to SolidCraft's contributor guide, whose "look at what is open
first", "running in minutes" and "first PR of a series" ideas we adapted
here (Apache-2.0).
