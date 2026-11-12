## #1201 - the gate detects a moved checkout (VOID) instead of guarding it

**What was decided.** `cargo xtask gate` (every tier, and the release gate run in process) reads HEAD and
`git --no-optional-locks status --porcelain` when it starts, again before each step, and at the end. If either
reading changed, it prints a `VOID:` line naming what moved (the two commits, or the status entries that differ),
stops, and exits non-zero with no verdict. The comparison is a pure function (`xtask/src/checkout.rs`), unit-tested
on fake readings. AGENTS.md and the `work-issue` skill say a worktree agent never switches, pulls, merges, resets,
checks out or stashes in the main checkout; its only git there is `fetch` and `worktree add`.

**Detection only, no guard.** The gate takes no lock and installs no hook. The issue offered a
`.git/index.lock` or a `reference-transaction` hook as the guard; the comment on the issue showed the hook leaves
the checkout half-moved. A lock the gate holds is only safe if released on every exit path (a Ctrl-C leaves a
stale `index.lock`, which blocks all git work in that checkout until someone deletes it by hand). That is not
robust at effort:medium, so the gate only reports.

**Why.** Both spoiled gates ran to completion and printed a verdict over a mix of commits. Detection turns that into
a loud, non-green run. It does not stop the pull; the rule that workers do not pull into the main checkout does that.

**Considered and dropped.**
- `reference-transaction` hook: git rewrites index and tree before the ref update, so it refuses after the files
  have moved (the issue's third spoiled gate).
- `.git/index.lock` held by the gate: correct while held, but a leaked lock blocks git. Left for a later issue if
  detection proves too weak.
- A long-lived detached gate worktree nobody pulls into: removes the race entirely, at the cost of one more
  `target/` (10-20 GB). Named on the issue; a separate decision.

**Cost to change later.** Low: the guard is one module plus the `check` calls in `Run::plan`. A lock could be added
behind the same `check` points.

**Known limit.** A step that writes a tracked or untracked file in the checkout (a snapshot writer, a generated
file not in `.gitignore`) also changes the status and voids its own run. That is intended: the run then names the file.
