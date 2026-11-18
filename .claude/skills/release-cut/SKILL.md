---
name: release-cut
description: Cut a CamCraft minor or major release, step by step - the cut list and the cut line (the maintainer names what the cut includes; an open P1/P2 that is not listed goes under Known issues), review and merge of the waiting feat PRs, the release branch with decisions, release notes and changelog rows, the release gate on the cut runner (`cut.yml` stage gate), the release PR, the tag (`cut.yml` stage tag), `release.yml`, the `shipped in` comments and the documentation site check. Use when the maintainer asks for a cut, a release, a tag or "ship X.Y.0", or when you must know what an issue or PR that is not on the cut list does to a cut.
---

# release-cut - PR review, merge to main, tag

The rules are in AGENTS.md "Commits and Releases": "Versioning is semantic",
"When to cut", "A cut goes PR review -> merge to main -> tag", "Every cut tag
is published". This skill has the procedure. If the two disagree, AGENTS.md
is correct.

Cuts and tags stay with the maintainer. Do not start a cut without his
instruction. Only minor and major versions are cut: there are no patch cuts.

From step 1 to the tag, nothing else merges to `main`.

## Step 1 - The cut list and the cut line, then review and merge

The cut list (AGENTS.md "The cut list", "The cut line"; replaces the start
condition of #1424):

1. A cut does not clear all `P1` and `P2` issues. The maintainer decides
   what the cut includes.
2. List the candidates one time:
   - each open `P1` and `P2` issue
     (`gh issue list --state open --search 'label:P1,P2' --limit 200`);
   - each open `feat` or breaking PR that waits for the cut.
3. Ask the maintainer one time, in the question dialog (`AskUserQuestion`):
   each candidate with "in" or "out" recommended and one line of reason.
   Recommend "in" only for an item that has a PR open or a fix that takes
   less than about one hour. His answer is the cut list.
4. The cut line: the list is now closed. Record the UTC time and the list
   in the release PR body.
5. Nothing is added after the cut line. This includes a new `P1`, a defect
   that the review of a listed PR finds in other code, and a PR that became
   `merge-ready` later. File the issue. Tell the maintainer in one line if
   it is a `P1`. Continue. Do not ask to add it.
6. Only the maintainer adds an item, in his own words. No agent holds,
   delays or restarts a cut for an item that is not on the list.
7. Name each open `P1` or `P2` that is not on the list under "Known issues"
   (step 2 and step 3). Its fix merges after the tag and ships in the next
   minor.
8. A listed item that is not ready when the others are merged: ask the
   maintainer one time (wait for it, or remove it from the list).
9. Always in the cut, without a list entry: a fix for a main that does not
   build or whose gate is red, and a fix for a failure of the cut's own
   release gate.
10. From the cut line, only listed items merge to `main`.

Review and merge:

1. Each open `in-review` `feat` or breaking PR gets a review by a subagent
   that did not write it (`/code-review` on the PR, or a reviewer that gets
   the diff and not the author's conclusion). Fix the findings on that
   branch and push.
2. A PR that does not merge cleanly is refreshed at merge time, in the order
   that `cargo xtask merge-plan` gives (AGENTS.md "Refresh only at merge
   time"): `git merge origin/main` on its branch, quick gate, push. An old
   branch that still appends to `RECOMMENDED.md` keeps the two sides.
3. Stacked PRs: retarget each child to `main` before you merge its parent
   (AGENTS.md "Overlapping issues stack"). Then merge the parent first.
4. Remove a worktree that holds the branch before the merge. The local
   branch delete fails if you do not.
5. `gh pr merge M --merge --delete-branch`. Use merge commits, never squash
   or rebase: the branch commits keep the hashes that the issue comments
   name.

## Step 2 - Release branch

1. Sync main: `git pull --ff-only`. Then `git switch -c release/vX.Y.Z`.
   The version is the highest bump of the commits since the last tag
   (AGENTS.md "Versioning is semantic").
2. Run `cargo xtask decisions --version X.Y.Z`. It appends each
   `docs/decisions/*.md` to `RECOMMENDED.md` under a `vX.Y.Z` heading and
   removes the files.
3. Write `RELEASE_NOTES.md`.
   - While the major is 0, a breaking change bumps MINOR. The notes then say
     "breaking" first.
   - Add a "Known issues" section: each open `P1` or `P2` that is not on
     the cut list, by number and title.
   - Add a "Process changes" section: each `PROCESS_VERSION` since the
     previous tag, from the pinned issue #681.
4. Write the `CHANGELOG.md` rows for each PR merged since the previous tag:
   one row for each behaviour PR, from its `Changelog:` line, plus the rows
   marked `Unreleased`. Fix PRs merge between cuts, thus include them. The
   row format:
   `| <short merge commit> | <YYYY-MM-DD> | <change, one line> (#N, PR #M) | X.Y.Z |`.
5. Commit items 2-4 together: `docs: X.Y.Z release notes; changelog`.
6. Run `cargo xtask release --version X.Y.Z --no-tag --skip-gate`. It
   commits the version bump and makes no tag. It refuses `--no-tag` on main.
   - Its local zip (and, on Windows with `--installer`, the installer) is a
     hand-test build only. `release.yml` builds the published zip, installer
     and packages from the tag.
   - A packaging failure after its phase 4 does not undo the bump commit.
7. Two checks stay local, on Windows, before the push:
   - The pixel-snapshot lanes, working directory `crates/ui`:
     `cargo test -p camcraft-ui --lib -- --ignored snapshot` and
     `cargo test -p camcraft-ui --test gpu_render -- --ignored snapshot`.
     The goldens were made on Windows. Make them new here, in one commit,
     only after a human looks at the `.diff.png`.
   - Optional: the Windows arms of the per-OS pinned readings (for example
     the move counts of issue 1402 in `crates/app/tests/pocket3d_hsm.rs`).
     The Linux runner checks only the Linux arm.
   - A cut without a Windows machine skips the two checks and says so in the
     release PR.

## Step 3 - Release gate on the cut runner, then the release PR

1. `git push -u origin release/vX.Y.Z`.
2. Start the gate: `gh workflow run cut.yml -f version=X.Y.Z -f stage=gate`
   (or Actions -> Cut -> Run workflow, `version` X.Y.Z, `stage` gate).
   - It checks out `release/vX.Y.Z`.
   - It refuses a branch whose `Cargo.toml` or `RELEASE_NOTES.md` does not
     name X.Y.Z, or whose tag exists.
   - It runs `cargo xtask gate release --no-snapshots`.
   - It uploads `gate-logs/` as an artifact, green or red.
   - When green, it records the tested commit.
   - The `dry_run` input prints the plan only. Use it to try a new runner.
3. Red: nothing is tagged. Fix on the release branch, push, and start the
   gate again. After a limited fix, `--only` / `--failed` (`gate` skill) run
   only the red steps locally before the next full gate.
4. While the gate runs: `gh pr create --base main --title "chore(release):
   X.Y.Z"`. It is reviewed as each PR is.
   - The body has the cut list and the UTC time of the cut line (step 1).
   - A comment names each open `P1` / `P2` that is not on the cut list, as
     "Known issues".
5. When the gate is green and the PR is reviewed: `gh pr merge --merge
   --delete-branch`.
6. Do not push to the branch after the green gate. The tag stage refuses a
   merge whose tree is not the gated tree.

## Step 4 - Tag the release merge commit, on the cut runner

1. Start Cut again: `gh workflow run cut.yml -f version=X.Y.Z -f stage=tag`.
2. It takes `tip`, the commit of the newest green gate run for X.Y.Z, and
   `m`, the head of main. It refuses unless all of these are true:
   - `m` is the merge of `tip` by the release PR (`m^2 == tip`): nothing
     else merged since step 1.
   - `git diff $tip $m` is empty: the gated tree is the tagged tree.
   - The tag is free.
3. Then it makes `git tag -a vX.Y.Z -m "CamCraft X.Y.Z" $m`, pushes it, and
   starts `release.yml` on the tag (`gh workflow run release.yml --ref
   vX.Y.Z`). A tag pushed with the workflow token starts no workflow, thus
   the explicit start. On a tag ref, `release.yml` publishes as on a pushed
   tag.
4. `dry_run` does each check and makes no tag.
5. A refusal means that something got to main during the cut, or that the
   branch moved after the gate. Start the cut again from step 2 on a new
   release branch.
6. Fallback when the runner is down: run the gate locally, then `git tag`
   and `git push origin vX.Y.Z` by hand. The push starts `release.yml`. The
   same empty-diff rule applies.
7. Comment `shipped in vX.Y.Z` on each issue closed since the previous tag:
   the merged PRs of the cut and the fix PRs merged since. Remove each
   `in-review` label that is left.

Never move an existing tag and never push it again. Never force-push. A fix
after a cut merges to `main` and ships in the next minor.

## Step 5 - Check the documentation site (#688)

- The same tag run makes the Zola site in `site/` from the tagged tree and
  deploys it to GitHub Pages (`release.yml` jobs `site` and `pages`).
  `pages` waits for `publish`, thus the download links of the site never
  point at a release that does not exist.
- The browser build (`wasm`, #1103) is served at `/app/`. A failed wasm
  build leaves `/app/` as the "not part of this copy" page with a warning.
  It does not fail the cut.
- Commit nothing for the site. Add no trigger.
- The cut is complete when the `pages` job is green and
  `https://neotech.github.io/Camcraft/` shows "CamCraft X.Y.Z".
- Run a red `pages` job again (Actions "Re-run failed jobs"). Never repair
  it by moving the tag.
- Local preview: `zola build` in `site/` (`site/README.md`).
