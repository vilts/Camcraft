## #879 - Reword the remaining 'setup furniture' errors; pin the frame-watch clamp

**What was decided.** Every user-facing string that said "furniture" already
reads in plain English on upstream/main (the repair, update, tool and setup
messages now say "a setup's stock or work offset"), so no string was
reworded. The work is the test hardening: a `FrameWatch::excuse` clamp case
(`an_excuse_is_clamped_to_the_time_since_the_last_mark`) pins the 200 ms
commands phase after a 5 s excuse at +210 ms.

**Why.** The issue's line numbers had drifted and the wording was fixed by
#873/#875/#878 before this branch was cut; a re-reword would only churn text.

**Considered and dropped.**
- A repo-wide source scan that fails on any string literal containing
  "furniture". Dropped: string literals span lines and identifiers such as
  `is_furniture_shape` sit beside them, so a scan would need a Rust lexer to
  avoid false positives. The existing guards cover the refusal paths
  (`setup_refusals_never_say_furniture` in `crates/app/tests/setups.rs`,
  and the `repair_model` assertion in `crates/app/tests/repair_model.rs`).

**Cost to change later.** Test-only; a broader guard can be added without
touching product code.
