---
name: fork-sync
description: Keep a fork of NeoTech/Camcraft in step with upstream - sync the fork's `main` to the newest cut tag (never to `upstream/main`), and pull the process files (AGENTS.md, skills, agents, xtask, gate wrappers) at the start of each session when upstream's `.claude/PROCESS_VERSION` is higher. Use at the start of each session in a repository that has an `upstream` remote, before you claim work, and when a PR from a fork conflicts with upstream's main.
---

# fork-sync - a fork follows cut tags; its rules follow upstream

The rule is in AGENTS.md: "A fork syncs from upstream at each cut tag, not
continuously". If this skill and AGENTS.md disagree, AGENTS.md is correct.

## 1. Is this a fork?

`git remote get-url upstream`. A fork of NeoTech/Camcraft has this remote.
The canonical repository does not: there, use AGENTS.md "Start from the
latest main" and stop here.

## 2. Process files, at the start of each session

Process files do not change product code. Thus the fork's code stays on the
tag while its rules stay current. This is the one exception to "sync only to
a cut tag".

1. `git fetch upstream --tags`.
2. Compare `git show upstream/main:.claude/PROCESS_VERSION` with the local
   `.claude/PROCESS_VERSION`.
3. If upstream's number is higher, pull only the process files:

   ```
   git checkout upstream/main -- AGENTS.md .claude/agents .claude/skills \
     .claude/PROCESS_VERSION xtask .cargo/config.toml scripts/gate.ps1 \
     scripts/gate.sh scripts/release.ps1 scripts/collect-decisions.ps1 \
     scripts/collect-decisions.sh crates/ui/AGENTS.md
   ```

4. Commit on a `process-sync` branch: `chore(process): sync process files
   to upstream PROCESS_VERSION <n>`.
5. Merge the branch into the fork's `main`. If the fork protects `main`,
   open a PR there.
6. Read AGENTS.md again before you continue.

If the fork's cut tag is older than `xtask/` (the first cut after #748), add
`"xtask"` to the `members` of the root `Cargo.toml` in the same commit, and
let `Cargo.lock` take the new package. The wrappers call `cargo xtask`, which
needs the member. It adds no product code.

The number moves a short time after the files. A fork that synced between
the two pulls the same files again at the bump. That does no harm.

The pinned issue #681 ("Process changes") says what changed and why. A fork
maintainer subscribes to it.

## 3. Code, at each cut tag

1. Find the newest cut tag:
   `git tag -l 'v*.*.0' --sort=-v:refname | head -1`.
2. Check if the fork's `main` contains it:
   `git merge-base --is-ancestor <tag> main`.
3. If it does not: `git switch main && git merge --ff-only <tag>`. Use `git
   merge <tag>` if the fork's `main` has its own commits. Then `git push
   origin main`.
4. Sync only to a cut tag, never to `upstream/main`. A tag is the tested,
   released state. Upstream's `main` moves many times each hour.

## 4. Issue work in a fork

- Branch issue work from the fork's synced `main` (AGENTS.md "Start from
  the latest main").
- A PR into upstream targets upstream's `main`.
- If the PR conflicts there, merge the upstream change into that branch
  only (`git merge upstream/main`). Never merge it into the fork's `main`.
  AGENTS.md "Refresh only at merge time" says when.
- Write the tag in the PR body: "fork synced at vX.Y.0".
