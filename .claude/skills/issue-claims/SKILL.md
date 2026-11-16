---
name: issue-claims
description: Decide if a claim on a NeoTech/Camcraft issue is live, stale, withdrawn or released, mirror a contributor's claim comment to the `claimed` label, release an ended claim with the fixed comment texts, run the claim maintenance sweep, and work a `needs-research` umbrella issue into sub-issues. Use before you claim an issue that has claim comments or the `claimed` label, at each issue pick and each `/review-prs` run (maintenance), when a contributor writes `claimed:` or `/claim` or withdraws, and when you research a `needs-research` issue.
---

# issue-claims - who holds an issue, and when it is free again

The rules are in AGENTS.md: "Claim before any code", "A claim comment is a
claim", "A claim must be live", "`needs-research` is research, not code".
This skill has the steps and the fixed texts. If the two disagree, AGENTS.md
is correct. Load `gh-issues` for the `gh` mechanics.

Two kinds of claim:

| Claimant | Claim | Stale after |
|---|---|---|
| An agent of the owner account | `claimed` label, assignee, claim comment | 4 hours with no newer comment from the claimant |
| A contributor (cannot set labels) | A comment that starts with `claimed:` or `/claim` | 48 hours with none of the three signs below |

Never change the labels of a `P?` issue, stale claim or not. Report it to
the maintainer.

## 1. Check before you claim

1. `gh issue view N --comments`.
2. Find the newest claim line and its author.
3. Read the claimant's LAST comment, not only the claim line.
4. A live claim by a person who is not the owner account means that the
   issue is taken. Do not work it, with or without the `claimed` label.

## 2. Is a contributor's claim live?

It is live while one or more of these is true:

- The claimant has an open PR for the issue.
- The claimant's branch for the issue has a commit in the last 48 hours.
- The claimant commented on the issue in the last 48 hours. Each comment of
  the claimant counts: the claim, progress, findings. A research claim has no
  branch, thus the comment keeps it live.

Ended is stronger than live:

- A comment that withdraws or releases a claim is not activity.
- An ended claim is never live again.
- Only a NEW `claimed:` or `/claim` comment after the end is a claim.
- Never mirror an ended claim from its old comment.

## 3. Mirror a live claim

An issue with a live claim comment and no label:

1. Add `claimed` and the claimant as assignee. If GitHub refuses the
   assignee, set the label only.
2. Reply one time: "Claim recorded for @<login> - label and assignee set
   (only collaborators can set labels; your comment is the claim)."

Only a LIVE earlier claim of a different person holds an issue against a new
claimant. If a new claimant comes after a stale claim: release the stale
claim first (step 4), then mirror the new claim. The label and the assignee
always follow the newest live claim.

## 4. Release a stale claim

With none of the three signs, the claim is stale. Do not mirror it. If the
issue has `claimed`:

1. Remove the label and the assignment.
2. Comment: "claim by @<login> released (stale: no open PR, branch commit or
   comment from the claimant for 48 h) - issue is free again; claim it again
   with a new comment if you are working on it".

That comment ends the claim. A later comment by the claimant does not start
it again.

## 5. Withdrawn or released

A claim also ends when:

- Its comment is deleted.
- The claimant's later comment withdraws it: `unclaim`, `/unclaim`, "claim
  withdrawn", "releasing this claim", "the issue is free".
- The maintainer releases it in a comment.
- An agent released it as stale (step 4).

Then remove `claimed` and the assignment, and comment "claim by @<login>
withdrawn - issue is free again". The comment is not necessary where a
release comment already says so.

## 6. Owner-account agent claims

- A `claimed` label from an agent of the owner account follows the 4 hour
  rule, not the 48 hour rule.
- If the last claim comment is more than 4 hours old and the claimant made
  no later comment: comment "reclaimed from <old claimant> (stale)" and take
  the issue.

## 7. Maintenance sweep

Each `/review-prs` run (its step 4) and each issue pick does this for the
issues that it touches:

1. Mirror the live claims that are comments only (step 3).
2. Remove the label where the claim is withdrawn, released or stale (steps
   4 and 5).
3. Apply the 4 hour rule to owner-account agent claims (step 6).

## 8. Work a `needs-research` issue

A `needs-research` issue is a large idea with open questions. Pick never
takes it for code.

1. Claim it as in AGENTS.md "Claim before any code", with `plan: research`.
   Use no branch, no PR and no lane slot.
2. Answer the open questions in issue comments: the research and the
   recorded decisions.
3. File the sub-issues. Each has one `P*`, one `effort:*`, `area:*`,
   `source:agent` (when an agent files it), a "Done means" line and "Part of
   #N".
4. Link the sub-issues from the umbrella issue.
5. When each sub-issue is filed, remove `needs-research` and close the
   umbrella with a comment that names them. Do not keep it open: Pick must
   not take it for code.

Only the sub-issues are picked for code.
