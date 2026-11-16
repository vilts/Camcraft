# AGENTS.md

## What This Is

A clean-room implementation of HSMWorks: a CAM system that turns 3D models into
NC programs (G-code) for CNC milling machines. Rust, Flecs ECS, egui.

"HSMWorks" is a trademark of Autodesk. This project is independent and
unaffiliated. It reimplements user-visible behaviour (operation types,
parameters, workflow) from public documentation and general machining
knowledge, and from nothing else.

## How to read this file

This file holds the rules. A procedure that only one task uses is in a skill.
Load the skill (Skill tool) before you do the task. If a skill and this file
disagree, this file is correct.

| Task | Skill (`.claude/skills/`) |
|---|---|
| Read, file, label, comment on or close an issue | `gh-issues` |
| Check, mirror, release a claim; work a `needs-research` issue | `issue-claims` |
| Work one issue from claim to draft PR | `work-issue` |
| Set or check an `effort:*` label | `classify-effort` |
| Review open pull requests | `review-prs` |
| Merge a batch of `merge-ready` PRs | `merge-ready` |
| Refresh a conflicting PR, or re-check `conflicts` PRs | `resolve-conflicts` |
| Run a gate, select tests, use the compile cache | `gate` |
| Cut a release | `release-cut` |
| Work in a fork of NeoTech/Camcraft | `fork-sync` |
| Install or change a self-hosted runner or a workflow | `runners` |
| Drive CamCraft through MCP | `camcraft-mcp` |

A rule name in **bold** is a stable name. Skills, agents and code comments
cite it. Do not rename one without a search for its citations.

The maintainer's dated words behind each rule, and the superseded rules, are
in the git history of this file (`git log -p -- AGENTS.md`; the last long
form is at commit `b0a95adc9`).

## Autonomous Work Law

The maintainer trusts the recommendations that agents make while he is away.
He reviews them later. The work must not interrupt him.

- **Autonomous always.** Work the open GitHub issues (Issue Law) and do not
  ask for decisions. When there is a choice, take the recommended option and
  continue. A one-line status is permitted. A blocking question is not.
- **Every recommendation is recorded.** Each decision taken unattended gets
  an entry: what was chosen, why, and what was dropped. On an issue branch
  the entry goes in that PR's own file, `docs/decisions/<issue>-<slug>.md`.
  Never write it into `RECOMMENDED.md`: the cut gathers the files there
  (`cargo xtask decisions`). `docs/decisions/README.md` has the format.
- **Never stop, never recap mid-roadmap.** When the gate of an issue is
  green, claim the next issue. A session summary is for compaction, not for
  the maintainer.
- **Counter-measure against stalling.** If the work can stop before the
  issue list is empty, start a subagent that reminds the session to claim
  the next issue. A stall is a bug.
- **Side work rides subagents.** Subagents do all testing, documentation and
  knowledge-graph updates. The main session plans, writes code and decides.
  A subagent's completion notice starts the next step. Never give one
  subagent both a code change and its verification.

## Issue Law - work is tracked as GitHub issues

The work list is the open GitHub issues of the repository, read and written
with `gh`. `ROADMAP.md` and `DONE.md` are retired; read them only for history
(`git show v0.14.0:ROADMAP.md`, `git show v0.8.1:DONE.md`). Decisions go to
`docs/decisions/`. Shipped changes go to `CHANGELOG.md` at the cut.

- **Use the gh-issues skill.** Load it before any work that reads or changes
  an issue. Tell a subagent that does issue work to load it too.
- **Pick.** List the open issues without `claimed`, `blocked`, `in-review`,
  `needs-research` and `P?`:
  `gh issue list --state open --search "-label:claimed -label:blocked -label:in-review -label:needs-research -label:\"P?\""`.
  Remove each issue that has a live claim comment (**A claim comment is a
  claim**). Take the lowest priority number first, then the oldest. At the
  same number, a bug that a user reported comes before owed work.
- **Priorities: `P1`..`P9`, lower is picked first.**

  | Label | Means |
  |---|---|
  | `P1` | Unsafe or wrong output: a gouge or collision that passes, wrong posted G-code, a crash, data loss |
  | `P2` | Main is broken, or the last release has a regression with no workaround: main does not build, a gate on main is red, the app cannot open a project that it saved |
  | `P3` | Wrong behaviour that a user finds, with a workaround: a legal program refused, a misleading message, a dialog that cannot do what it says |
  | `P4` | Work that the maintainer asked for, for the next cut |
  | `P5` | The agent surface and the tooling follow what shipped: an MCP tool or skill row for a shipped command, a gate or process fix that blocks contributors |
  | `P6` | Accepted enhancements, not scheduled |
  | `P7` | Owed polish and small fixes: wording, layout, a missing test, a refusal that could say more |
  | `P8` | Larger ideas that need design or research before code |
  | `P9` | Someday: not picked while other work is free |

  - `P1`-`P3` are for defects only. "Wanted" is never a reason for them.
  - Only `P1` and `P2` hold a release cut. A `P3` is worked before planned
    work, but a cut does not wait for it.
  - Set `P4` only where the maintainer asked for the work in his own words.
    The issue quotes those words.
  - A follow-up found during work or review is `P7`, unless it is a defect.
    A defect is `P1`-`P3` by the table.
  - An agent that files an issue sets exactly one of `P1`..`P9`.
  - An issue without a priority is not triaged. It is not the lowest. The
    first agent that reads it for a pick sets its priority and its effort
    label before the sort.
  - If an issue has two priority labels, report it to the maintainer. Read
    it by the lower number until he decides.
- **`P?` is paused, not prioritised.** The maintainer holds a `P?` issue. No
  agent picks it, claims it, researches it, files sub-issues for it, or
  changes its labels. Agents can read it, link it, and comment facts on it.
  `P?` is used instead of a `P1`..`P9` label, never with one: report an issue
  that has the two. An agent never sets `P?` on a new issue.
- **Two lanes, two issues each.** At most four issues are in work: two in the
  bug lane and two in the enhancement lane. An issue with `enhancement` and
  without `bug` is in the enhancement lane. All other issues are in the bug
  lane. An issue holds its slot from the claim until its PR is open. A review
  fix later uses the same issue and no new slot. The bug lane uses the Pick
  order. The enhancement lane order is: issues filed by a user (no
  `source:agent`) first, then the lower number, then the oldest. Fill a free
  slot immediately. Each slot has its own worktree and branch. Check the free
  disk space first: one `target/` is 10-20 GB.
- **Start from the latest main, on a branch of its own.** One issue, one
  branch, `issue/N-<slug>`. No commit for an issue goes to `main`.
  - Main session: `git fetch origin && git switch main && git pull --ff-only
    origin main`, then `git switch -c issue/N-<slug>`.
  - Worktree agent: `git fetch origin && git worktree add --no-track -b
    issue/N-<slug> .claude/worktrees/issue-N origin/main`.
  - Worktrees are in the gitignored `.claude/worktrees/`, never outside the
    repository. Remove a worktree when its PR is pushed.
- **A worktree agent never touches the main checkout's HEAD or tree.** In the
  main checkout it runs `git fetch origin` and `git worktree add` only. It
  does not run `switch`, `pull`, `merge`, `reset`, `checkout` or `stash`
  there. A gate can be in progress in that checkout, and a pull makes its
  result void (#1201).
- **A fork syncs from upstream at each cut tag, not continuously.** Its
  process files are the exception: they sync at the start of each session.
  The `fork-sync` skill has the procedure. A repository with an `upstream`
  remote is a fork.
- **Process files** are: `AGENTS.md`, `.claude/agents/`, `.claude/skills/`,
  `.claude/PROCESS_VERSION`, `xtask/`, `.cargo/config.toml`,
  `scripts/gate.ps1`, `scripts/gate.sh`, `scripts/release.ps1`,
  `scripts/collect-decisions.ps1`, `scripts/collect-decisions.sh` and
  `crates/ui/AGENTS.md`.
- **Process changes bump `PROCESS_VERSION` and are announced.**
  - A PR that changes a process file does not change
    `.claude/PROCESS_VERSION`. Its body has the line `Process change: yes`.
  - The reviewer requests a change if that line is missing, or if the PR
    edits `PROCESS_VERSION`.
  - The merging session is the one writer. After a merge batch with one or
    more process PRs, it runs `cargo xtask process-bump` once, in a worktree
    off the new `main`. It opens the one-line PR `chore(process):
    PROCESS_VERSION <n>` and merges it. That PR is MECHANICAL: the merging
    session reads it, and it needs no agent review.
  - Then the session comments on the pinned issue #681: `PROCESS_VERSION
    <n>` · PR #M (each process PR of the batch) · what changed · what a fork
    must do.
  - At a cut, the release notes list the versions since the previous tag in
    a "Process changes" section.
- **Overlapping issues stack.** An issue that changes code of an open PR
  branches off that PR's branch, and its PR says "stacked on #M". Before the
  parent merges, retarget the child first (`gh pr edit <child> --base main`).
  Then merge the parent. Then the child takes `git merge origin/main`, is
  gated again and merges. Never merge the parent first: `--delete-branch`
  deletes the child's base branch and GitHub closes the child PR. An overlap
  that cannot stack waits for the parent to merge.
- **Claim before any code.** Run `gh issue edit N --add-label claimed
  --add-assignee @me`, then comment `claimed: <session or agent name> · <UTC
  time> · worktree/branch <name> · plan: <one line>`. Do not work an issue
  that has `claimed`. There are two exceptions. One: the last claim comment
  is from an agent of the owner account, is more than 4 hours old, and the
  claimant made no later comment. Then comment "reclaimed from <old claimant>
  (stale)" and take the issue. Two: a contributor's claim that is withdrawn,
  released or stale (`issue-claims` skill). Never claim a `P?` issue.
- **A claim comment is a claim.** Only collaborators can set labels. Other
  persons claim an issue with a comment that starts with `claimed:` or
  `/claim`. Read the comments before you claim (`gh issue view N
  --comments`). A live claim comment by a person who is not the owner account
  means that the issue is taken, with or without the label.
  - **A claim must be live.** A contributor's claim is live while one of
    these is true: the claimant has an open PR for the issue; the claimant's
    branch for it has a commit in the last 48 hours; the claimant commented
    on the issue in the last 48 hours. If none is true, the claim is stale
    and cannot hold the issue.
  - A claim that is withdrawn, released or stale has ended. An ended claim
    is never live again. Only a new claim line is a claim.
  - The `issue-claims` skill has the mirror, release and maintenance steps
    and the fixed comment texts.
- **Agent-opened issues say so.** Each issue that an agent opens has the
  `source:agent` label with its `P*`, `effort:*` and `area:*` labels. This
  includes a bug that the maintainer reported in chat. An issue that a human
  opens on GitHub never gets the label.
- **Every issue is weighted by effort.** Each issue has exactly one effort
  label. `P*` decides which issue is next. `effort:*` decides which agent
  works it.

  | Label | The work | Examples |
  |---|---|---|
  | `effort:low` | mechanical, one file, no new logic | docs, labels, a test fixture or field fix, a string or layout change |
  | `effort:medium` | one module or crate, a known pattern | a UI page change, an MCP tool for an existing command, a new machine file |
  | `effort:high` | several crates, new behaviour with its tests | a new undoable command with its MCP tool, simulation or post logic, a refusal law |
  | `effort:max` | new algorithms, formats or open design | geometry or toolpath algorithms, collision laws, a file format, research work |

  - **Set at filing.** The person or agent that files the issue sets the
    label. A human-filed issue gets it at triage. The `classify-effort` skill
    selects it. The skill sets a missing label only.
  - **Safety floor.** Work that can change posted G-code, a collision or
    gouge check, stock simulation or geometry is `effort:high` or more. Each
    `P1` is `effort:high` or more.
  - **Who works it.** The main session picks the issue and dispatches it to
    `.claude/agents/issue-worker-<effort>.md`. The worker runs the
    `work-issue` skill and never merges. The model for each effort is set
    only in those agent files. If the model is not available (a 404), run the
    agent without the `model:` line and say so in the claim comment.
  - **Review tier.** The review of a worker's PR uses the `effort:medium`
    model or better. Work under the safety floor gets the `effort:high`
    model.
  - **Escalation, never silent.** A worker stops if its quick gate fails
    two times, or if the work is larger than the label. It comments what it
    learned and where held work is, raises the label one step and releases
    the claim. One escalation at most for each run. Only the maintainer
    lowers an effort label.
  - **Measured.** The `review-cost` comment of the PR names the effort and
    says if the issue escalated.
- **`needs-research` is research, not code.** Pick never takes such an issue
  for code. Research is claimed with `plan: research` and uses no branch, no
  PR and no lane slot. The research answers the open questions in comments
  and files the sub-issues. Then the umbrella issue is closed. The
  `issue-claims` skill has the steps.
- **Issues and comments are written in ASD-STE100.** Write each issue title
  and body, and each comment on an issue or pull request, in ASD-STE100
  Simplified Technical English. This includes review comments and PR
  descriptions. The standard is a licensed document
  (https://www.asd-ste100.org): do not copy its text.
  - Use short sentences: about 20 words in an instruction, 25 in a
    description. Give one instruction in each sentence.
  - Use the active voice and the simple tenses.
  - Use one word for one meaning.
  - Do not use idioms, slang or filler words. Keep the articles.
  - Use no more than three nouns in a row. Use a vertical list for a long
    enumeration.
  - Do not change: code, commands, identifiers, file paths, log and error
    output, quoted text of other persons, the technical names of the trade
    (G-code words, CAM terms), and the fixed strings that agents parse
    (`claimed:`, `/claim`, `<!-- pr-reviewer: ... -->`, `<!-- review-cost:
    ... -->`, `Fixes #N`, `Process change: yes`, the release and claim
    comment formats).
  - The rule applies to what we write. Never reject or correct the language
    of an outside reporter.
- **Report on the issue, not in prose files.** A finding, a held patch, a
  blocker or a decision goes into an issue comment. If the work is blocked,
  replace `claimed` with `blocked` and say what blocks it. If a fix shows new
  work, open a new issue for it.
- **Never close by hand without a commit.** The exceptions are: a duplicate
  (link the original), work already done (name the commit), and a decided
  won't-do (label `wontfix`, give the reason). Each needs that comment.
- **User-reported bugs reopen, never re-file.** If the user still sees the
  bug in a build that has the fix, reopen the same issue with what the user
  saw. The claim starts again.
- **Giving up without a fix.** Remove `claimed`, comment what you learned,
  and leave the issue open. Name a pushed branch with held work in that
  comment. Delete an empty branch.
- **Changelog.** Each PR that changes behaviour has its row text in its body
  (`Changelog:` line). The row is written into `CHANGELOG.md` on the release
  branch at the cut, not on the issue branch. Docs-only and
  release-mechanics PRs need no row.

## Pull Request Law

- **Done on a branch = pushed + PR, never merged to main.** The fixing
  commit ends its body with `Fixes #N` (`Refs #N` for partial work, which
  does not close). When the gate is green:
  1. `git push -u origin issue/N-<slug>`.
  2. `gh pr create --draft --base main --head issue/N-<slug> --title
     "<conventional subject>" --body-file <file>`. The body says what
     changed, the readings, the pinning tests, what was not exercised, a
     `Changelog:` line (the row text, or `none`), and ends with `Fixes #N`.
  3. Replace `claimed` with `in-review` on the issue. Comment `fix on
     issue/N-<slug> · PR #M · pinned by <test name(s)> · merges when
     reviewed` (a `feat`: `merges at the next cut`).

  A review finding is fixed on the same branch with a plain push. The fixer
  adds `claimed` and a claim comment during the work, then sets `in-review`
  again. An issue never has `in-review` without an open PR: a PR closed
  unmerged changes `in-review` to `blocked` (held) or to nothing (dropped),
  with an issue comment.
- **When PRs merge.** Merges are merge commits (`gh pr merge M --merge
  --delete-branch`; a fork PR without `--delete-branch`). Never squash or
  rebase. Never merge a draft.
  - A `fix` / `perf` / `refactor` PR merges when its independent review is
    clean and its quick gate is green.
  - A docs-, process- or CI-only PR merges when it is reviewed.
  - A `feat` or breaking PR merges at the minor/major cut.
  - Nothing merges while a cut is between its step 1 and its tag.
  - The review is always by an agent that did not write the change.
- **When a PR merges**, remove `in-review` from its issue. GitHub closes the
  issue from `Fixes #N`.
- **Every PR opens as a draft.** It stays a draft while work, gates and
  review fixes go onto it.
  - Flag it ready (`gh pr ready <M>`) when the maintainer says so, or when a
    quick pass is good. The quick pass reads the PR and does not build. It
    confirms: the PR merges cleanly with `main`; the author's quick gate is
    green on the final head, with the pinning tests named; the body is
    complete; the PR obeys this file.
  - The full independent review comes after that, on the ready PR.
  - A ready PR takes no more pushes. For any change, first make it a draft
    again (`gh pr ready <M> --undo`), say why in a PR comment, push, gate
    again, then flag it ready again.
  - Merge-time refresh, class SAME (only a merge of main, no hand edits):
    this is not new work. The review stays valid and the refresh is pushed
    without the change to draft.
  - Merge-time refresh with hand edits (MECHANICAL or DELTA): this is new
    work. The PR goes to draft first, is pushed, and is reviewed as usual.
  - The merging session flags the PR ready immediately before the merge, with
    one PR comment that names the class and the review that it relies on.
    Nothing is pushed after that flag.
  - A PR with an orange `conflicts` verdict is a draft.
- **Conflicted PRs are re-merged by their author; the maintainer
  re-checks.** The `resolve-conflicts` skill does the two parts.
  - Contributor mode: merge main into the branch (never rebase, never
    force-push). Keep the two sides where each added lines. Take main's
    snapshot PNGs and changelog files. Run the targeted gate, push, and
    comment "ready for a re-check".
  - If the two sides changed the same logic, or a safety rule could be
    lost, stop and report the hunk. Do not guess.
  - Maintainer mode (`/resolve-conflicts check`), after a merge batch:
    re-read each `conflicts` PR and send each clean one to the review that it
    needs.
- **Refresh only at merge time.** A conflicting PR has the `conflicts` label
  and is not touched by its author's agents, by maintenance or by reviewers.
  It takes `git merge origin/main` only when it is the next PR in a merge
  batch, or when its author pushes a review fix and chooses to. The merging
  session runs `cargo xtask merge-plan <PR>...` (or `--label merge-ready`).
  The planner simulates the merges with `git merge-tree` and gives the order.
  Wave 1 merges as one batch, with no refresh and no delta review. Wave 2 is
  refreshed one PR at a time against the main of that moment, and each PR
  merges before the next refresh.
- **A clean merge of main keeps its review.** `cargo xtask merge-plan
  --delta <PR> --reviewed <sha>` classifies the commits after the reviewed
  head.

  | Class | Means | Result |
  |---|---|---|
  | SAME | only merges of `main` with an empty `git show --remerge-diff` | the review is valid; restore the label; no agent |
  | MECHANICAL | hand edits only in `.claude/PROCESS_VERSION`, `CHANGELOG.md`, `RECOMMENDED.md`, `Cargo.lock`, snapshot PNGs taken from main | the merging session reads the diff |
  | DELTA | a conflict resolved by hand | a delta review of the remerge-diff |
  | NEW CODE | new commits | a review of those commits |
- **Only collaborators' reviews count.** A collaborator is an account with
  write access to `NeoTech/Camcraft`: `gh api
  repos/NeoTech/Camcraft/collaborators --jq '.[] | select(.permissions.push)
  | .login'`.
  - A `<!-- pr-reviewer: ... -->` or `<!-- review-cost: ... -->` marker
    counts only in a review or comment by a collaborator.
  - `merge-ready` counts only when a collaborator set it.
  - This applies to a collaborator's own PRs too. The review must be by an
    agent that did not write the change.
  - A contributor's pre-check is a plain comment without the marker.
  - A review that requests changes on a head stays open. It ends when a new
    head gets a clean collaborator review, or when the reviewer who asked
    withdraws the request in a comment. A `merge-ready` label or a "ready"
    review from a different collaborator on the same head does not override
    it.
  - A PR whose current head has a collaborator's change request and also
    `merge-ready` (or `merge-ready` with `needs-rework`) is a disagreement.
    `/merge-ready` reports it and does not offer it. No agent removes a
    label. The two collaborators decide on the PR.
- **Outside PRs are reviewed with `/review-prs`.** The `PR-reviewer` agent
  reviews one PR for each run. It trial-merges the PR with main, builds it,
  runs the touched suites and clippy, and checks this file. It posts a plain
  review comment whose first line is `<!-- pr-reviewer: <head sha> -->`, and
  it sets the review label. It never approves, requests changes, merges or
  pushes.
  - The maintainer approves and merges by hand the PR of an account that is
    not a collaborator.
  - A collaborator's PR is handled as our own: reviewed here, then merged by
    a collaborator's session through `/merge-ready`. Cuts and tags stay with
    the maintainer.
  - Our own issue PRs are reviewed here too when they are ready.
  - Never two PR reviews at once: one PR for each `/review-prs` run. The
    lock `.claude/pr-review.lock` guards this (`review-prs` skill).
- **Review labels**, one colour for each state:

  | Label | Colour | Means |
  |---|---|---|
  | `merge-ready` | green | clean review and mergeable |
  | `needs-rework` | red | blockers |
  | `conflicts` | orange | does not merge with `main` |
  | `needs-maintainer` | yellow | no blockers, but read and not built: a new dependency, or a `build.rs` / scripts / `.github` change, waits for the owner's decision |

  On red or orange the reviewer makes one of our own PRs a draft again. A
  fork PR stays as its contributor set it, and the comment says what to fix.
- **`main` is protected on GitHub.** Each change gets to `main` through a
  PR. A direct push is refused. There is no force-push and no deletion. Open
  review conversations must be resolved before a merge. The repository
  permits merge commits only. Tags are not protected.
- **Never force-push**: not `main`, not a branch with an open PR.

## Clean-Room Protocol

- Do not read, decompile, disassemble or trace HSMWorks binaries or source.
  Write no code derived from their code.
- Do not copy their text: UI strings, docs, help content, original parameter
  names, comments, or bundled post processors.
- Use no asset from their installer or install tree. Icons, images and
  tool/NC libraries are made from scratch.
- Reference material is limited to: Autodesk's published user documentation
  (behaviour only, not implementation), public G-code standards (RS-274, ISO
  6983), machine-tool handbooks, and our own design notes.
- Write new geometry or toolpath algorithms from published technical
  literature, not from an inspection of HSMWorks output.
- Each module has a one-line provenance comment: what it implements, and
  from which public source.

## Prime Directive: Never Crash, Never Gouge

A CAM program drives a spindle in front of a human. The output is correct,
or it is visibly refused with a diagnostic. A silent wrong answer (a crash,
or a toolpath that gouges, collides or cuts in the wrong place) is the worst
failure. There are no exceptions.

- Handle errors or propagate them. `unwrap()`, `expect()` and `panic!()`
  are forbidden outside tests and `#[cold]` unreachable arms. No panic in a
  system or on a path that a user can start.
- Degenerate input (zero-area triangles, self-intersecting shells, bad
  tolerances, corrupt STL/STEP models) gives a clean error. It does not
  crash or hang.
- A result that a collision check or simulation finds unsafe must block the
  post. It never passes silently.
- Use safe numeric types for geometry: saturating, checked or clamped
  operations. No unchecked floats in bounds or index math.
- Validate early. Reject a malformed setup or parameter when the command is
  created, not in a generator.
- Prefer recoverable states. If an operation can only be computed in part,
  mark it "incomplete". Do not emit a toolpath that is subtly wrong.
- Log diagnostics. Never hide them.
- Fix a leak when you find it. A leak is a critical bug.

## Everything Is a Command

A UI state change is a command with entity references, not a direct write to
a field. Undo (Ctrl+Z) and redo (Ctrl+Y) go through the command history and
invert it. The history is the single source of truth for undo and redo. Each
change from the UI goes through it, parameter edits in the operation
property panel included.

- `CreateOperation` / `DeleteOperation`: operation entities.
- `ChangeTool` / `EditOperationParams`: operations and setups whose tool,
  stock or depth changed. Write the new value when the command is created,
  not in a system. Undo must restore the previous value.
- `SelectEntity`: the selection.
- `ActivateSetup`: the active setup.
- Ephemeral UI state (hover, drag previews, simulation playback position) is
  immediate-mode only and cannot be undone.

Systems and UI never write these fields directly. If a UI change has no
command, that is a bug: add the command.

**Commands ship with an MCP tool** (#128, #356). A PR that adds a
user-facing command also adds its MCP tool (and script verb where
applicable) in `crates/app/src/mcp/`. Alternatively it files a `source:agent`
follow-up issue `MCP: <command>` and names it in the PR body. MCP tools call
the same undoable commands as the GUI. Each MCP issue has `area:mcp`. Owed
agent-surface work is `P5`. A defect in MCP code is `P1`-`P3` by severity.

Components are registered automatically: declare them in
`crates/app/src/schema.rs`, and the startup hook
(`flecs_bits::register_schema`) does the rest in each binary. Components are
Rust-owned types (`Vec`/`Box` fields), freed by the delete hooks that
flecs_ecs registers. The invariant: when an entity is deleted, all its
resources are freed (for example the cached toolpath and rest-material meshes
of a deleted operation). If a type adopts C-style `raw_data` storage, it must
use a per-type arena, never a global allocator, and it must have a delete
hook that frees all that it owns.

## Assets

UI work also obeys `crates/ui/AGENTS.md` (the UI steering document).

`assets/ui/` holds PNGs for toolbar and tree icons. `assets/ui/fonts/` holds
the fonts. The UI must render them: each panel's draw code must use them.

- Icon sources: free sets that need no attribution to end users (MIT, ISC,
  Apache-2.0, BSL). The primary set is
  [Tabler Icons](https://tabler-icons.io) (MIT). Never the free tier of
  icons8. Do not pay for a set without need.
- 32x32 PNGs, drawn at 16 px.
- Keep the copyright and license texts of each bundled set in
  `assets/ui/LICENSES.md`. Third-party notices (for example the BSL text of
  clipper2-rust) go there too. Retention is a license condition.
- Draw an original glyph for an operation when no generic set has one.
- All icons are referenced from `crates/ui/src/egui_app/icons.rs`.
- Icons and post-processor templates are original, or licensed without
  attribution. Never adapt them from HSMWorks material.

## Dependencies

- Layers are strict: ui → app → flecs. UI panels, widgets and dialogs do not
  use `sys_*` or world-level calls. Only app-layer systems and commands use
  raw Flecs calls.
- Schema components (`#[derive(Component)]` structs and their CPOD decls)
  are in `crates/app/src/schema.rs`. Other app modules declare no
  components.
- Geometry parts are components (`app::model::Part`). Traverse them with
  `model::child_parts` / `model::child_parts_of`, in UI and in systems.
  Components do not reference each other. `Part` stores `child: Entity` (not
  the parent). Systems query directly. `decompose` stores facets directly
  into `Part`.
- Check only the touched crates, with `-p` for each: `camcraft-ui`,
  `camcraft-app`, `camcraft-motion`, `camcraft-sim`, `camcraft-post`,
  `camcraft-tools`, `camcraft-cl`, `camcraft-geometry`. Do not build the
  full workspace: that rebuilds flecs.

## Code shape

- **400-line files.** No source file grows past about 400 lines. Split it
  along the domain seam. Exempt: post-processor scripts
  (`assets/posts/*.cps`). One post script is one file for one machine
  dialect; review it for correctness, not for length.
- **One concern per subcrate.** Motion types, operations, tool management,
  heights/offsets, movement and simulation are isolated subcrates. A call
  between domains goes through the crate API, not through a private helper of
  the other domain.
- **`app` holds the document, nothing else.** New code goes in
  `crates/app/src` only if it reads or writes one of these five: the ECS
  world, the project file, a command, MCP, the CLI. Code that works only on
  domain values (cl, geometry, tools, post, sim, motion types) goes in the
  crate that owns them, and `app` re-exports it. The reviewer checks the
  `use` lines of a new file: if none names one of the five, the file is in
  the wrong crate.
- **No new dependency between our crates without a decision.** A new
  `camcraft-*` entry in `[dependencies]` or `[dev-dependencies]` of a
  `crates/*/Cargo.toml` needs a `docs/decisions/` entry in the same PR. The
  entry lists the imported items, says why the caller cannot pass the value
  in (a parameter, a closure, a recorded file), and names what rebuilds when
  the dependency changes. Do not add an edge for fewer than five items: the
  caller passes the value in. (The maintainer has still to confirm the
  wording of the five-item clause.)
- **Stays buildable everywhere.** No platform-specific assumptions in core
  code. Windows, Linux and macOS build from the same source.
- **Neutral naming.** The product name is camcraft. No string, comment or
  identifier has the old name hsmworks. True statements about Autodesk's
  product ("a clean-room implementation of HSMWorks", "no HSMWorks material
  consulted") stay.

## Testing

Tests of what `crates/app/src` defines (commands, the project file, the
schema, operations, MCP, the CLI) are in `crates/app/tests/`. UI and event
tests are in `crates/ui/tests/`; they start the real app through
`test_harness.rs` (helpers in `ui::test_harness`, app fixtures in
`crates/app/tests/common/mod.rs`). Each other crate has its own tests.

- **A test lives with the code it tests.** A new test file goes in the
  lowest crate that provides all that it imports. Its Gate map names that
  crate (`camcraft_post::post::engine`, not `app::post`). A file in
  `crates/app/tests/` must import something that `crates/app/src` defines. A
  test that needs the output of a different crate reads a recorded file
  (`.cl`, `.cnc`); it does not import that crate's generator.
- **Three gate tiers - never hours per change.** One command on all
  systems: `cargo xtask gate <tier>`. `scripts/gate.ps1` and
  `scripts/gate.sh` are wrappers for it. The `gate` skill has the flags, the
  test selection, the compile cache and the heavy lanes.

  | Tier | When | Target | Runs |
  |---|---|---|---|
  | quick | each patch, by the agent or gate-runner | ≤ 5 min | only the test binaries that the diff touches, one step for each binary; debug; no heavy lanes |
  | batch | before a batch is committed; each push to `main` | ≤ 20 min | one build, then one step for each debug test binary that is not slow (kernel and ui included), clippy |
  | release | only at a cut | - | one step for each debug test binary and the `--ignored` heavy lanes in release |
- **Test the change, not everything.** The tests that pin a fix prove it.
  Run them one by one: `cargo xtask gate test <name> [--crate c] [--binary b]
  [--exact]`. Run a full suite only when the change touches what that suite
  covers. Run the full set only at a release cut.
- **Shared compile cache** (optional, sccache, #900): never export
  `CARGO_TARGET_DIR`, because it makes each worktree miss the cache. The
  `gate` skill has the setup.
- A gate subagent runs the gate and reports pass or fail for each binary
  with the failure output. The main session owns the code and the decision
  to commit.
- A test that takes more than about 60 s in debug moves to the heavy lane
  (its ignore reason names the lane), or gets a smaller fixture.
- Tests that measure geometry (envelope scans, full-precision projections)
  or generate over the large fixtures are `#[ignore]`d and run in release at
  a cut, never for each edit. A split must not lose coverage: the `#[test]`
  count before and after is the same, and the ignored set keeps its golden
  asserts.
- **Pixel snapshots are release-lane** (#255). The PNG goldens
  (`crates/ui/tests/snapshots/*.png`) are `#[ignore]`d and run only in the
  release tier. A PR never changes a snapshot PNG. The goldens are made new
  one time at the cut, in one commit on the release branch, after a human
  looks at the `.diff.png`. The structural twin
  (`crates/ui/src/egui_app/tests/structural.rs`) asserts in the default
  suite what a snapshot guards: AccessKit labels, roles, enabled state,
  window presence, and the text and coarse position of the painted strips.
  A UI change updates those assertions, not the PNGs.
- Add headless tests for toolpath generation (golden CLFile and G-code
  outputs in `tests/golden/`), stock simulation and post processing. Add fuzz
  tests for the geometry importers and the toolpath generators: degenerate
  input must give an error, never a crash or a hang.
- **2D toolpath shape-mask tests** (`crates/motion/tests/common/shape_mask.rs`):
  compare a generated `Toolpath`'s swept disc footprint against the expected
  `Region` (overcut, leftover, progressive rim engagement, symmetry, stock
  extension) and write an RGB diff PNG on failure (do not commit the PNG).
- **GitHub Actions minutes are budgeted** (3000 each month; Windows counts
  2x, macOS 10x).
  - Nothing runs on a PR trigger.
  - A `push` trigger is permitted only on `main`, and only for a workflow
    whose jobs run on a self-hosted runner (`post-merge.yml`).
  - Never add a `push`, `pull_request` or `schedule` trigger without the
    maintainer's approval.
  - Never add a trigger that a fork PR can start to a workflow whose jobs
    run on a self-hosted runner. A fork PR must not run code on the
    maintainer's machine.
  - Never approve the workflow run of a fork PR that adds or changes a file
    in `.github/workflows/`.
- **The cut runner**, **The post-merge gate** and **The PR test runner**
  are self-hosted. The `runners` skill has their installation, labels and
  safety rules.
  - `cut.yml` runs the release tier of a cut on `[self-hosted, linux]`.
  - `post-merge.yml` runs the batch tier on each push to `main`, on the
    runner label `camcraft-post-merge`.
  - `pr-test.yml` (`workflow_dispatch` only) builds and tests a PR for a
    review, on the runner label `camcraft-pr-test`. Only the PR-reviewer
    agent starts it, after it read the diff, and only for a collaborator's
    PR. If no run completes, the review builds locally or says "not built".
    It never says "pass".

## Knowledge Graph

`graphify-out/` holds a graphify knowledge graph of this repository. It is
generated output: never edit it by hand, never commit it.

For a structural question, query it before you grep:

- `graphify query "<question>"`: how X works, what calls Y, a flow.
- `graphify path "A" "B"`: the shortest connection between two concepts.
- `graphify explain "X"`: a summary of a node and its neighbours.
- `graphify affected "X" --relation calls`: what breaks when X changes.

Keep it current:

- Code-only change: `graphify update` (AST only, no LLM) at the end of a
  task.
- `AGENTS.md` or icon change: `/graphify . --update`.
- Renames or refactors that move many files: a full `/graphify .` rebuild.
  The shrink guard refuses a smaller graph without `--force`.
- Ignore the warning that reads `tests/golden/*.cl` as Common Lisp.

Each edge is EXTRACTED (read from source), INFERRED (0.55-0.95) or AMBIGUOUS
(0.1-0.3). Check an INFERRED edge against the code before you act on it.
Never act on an AMBIGUOUS edge.

## Commits and Releases

Format ([Conventional Commits](https://github.com/qoomon/conventional-commits)):
`<type>(<optional scope>): <subject>`. Types: `feat`, `fix`, `perf`,
`refactor`, `test`, `docs`, `build`, `ci`, `chore`. A breaking change says
so: `feat!:` / `fix!:` or a `BREAKING CHANGE:` footer (an incompatible
project-file or post-output change, a removed parameter or command). The
body of the fixing commit ends with `Fixes #N`.

**Versioning is semantic: MAJOR.MINOR.PATCH, derived from the commits.** A
cut takes the highest bump of the commits since the last tag.

| Commits since the last tag | Bump |
|---|---|
| a breaking change | MAJOR (while the major is 0: MINOR, and the release notes say "breaking" first) |
| a `feat` | MINOR |
| only `fix` / `perf` / `refactor` | no cut: ships in the next minor |
| only `docs` / `test` / `ci` / `build` / `chore` | no release |

**When to cut: only MAJOR and MINOR.** There are no patch cuts. A fix
merges to `main` when it is reviewed and gated, and ships in the next minor
or major release. Cut when the `feat` PRs are gated and waiting, not on a
calendar. `release.yml` builds and publishes on `vX.Y.0` tags only.

**Fixes are tested in the background.** `post-merge.yml` runs the batch
tier on each push to `main`. A red run opens or updates the issue `main is
red (post-merge gate)`: `P2`, or `P1` when a wrong-output suite fails. The
next green run closes it. A red result does not revert the merge; the issue
is worked in the normal order. The main session does not start a gate for a
merge that the workflow covers. If the runner is down, the `gate` skill has
the local fallback: one batch gate for each merge batch, by a gate-runner
subagent, and the session does not wait for it.

**A cut goes PR review -> merge to main -> tag.** The `release-cut` skill
has the procedure. Its rules:

- A cut does not start while a `P1` or `P2` issue is open. The check is made
  one time, at the start. A `P1` or `P2` filed later does not restart the
  cut: it goes under "Known issues" and its fix ships in the next minor. Only
  the maintainer stops a cut. An agent that finds a late `P1` tells the
  maintainer in one line and continues.
- From step 1 to the tag, nothing else merges to `main`.
- The release tier runs on the cut runner (`cut.yml`). The tagged tree is
  exactly the gated tree.
- Cuts and tags stay with the maintainer.

**Every cut tag is published.** A cut is not complete until its tag is on
GitHub and `release.yml` has attached the artifact to a GitHub release. A
tag that is only local is an incomplete cut. Never move or push again an
existing tag. A fix after a cut merges to `main` and ships in the next
minor. At the cut, comment `shipped in vX.Y.Z` on each issue closed since
the previous tag.
