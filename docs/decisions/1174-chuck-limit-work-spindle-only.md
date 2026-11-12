## #1174 - The workholding rpm limit judges the work spindle, keyed on the post that runs

**What was decided.** `setup_workholding::program_problems_by` judges each
operation by what the post that will run it does with its section, from
two shared readings and no list of kinds of its own:

- the section: `lathe_voice::voice_of` (#1190's one derivation, also the
  lathe door at post; its live-tool test is `post::dialect::states_live_tool`,
  the same test `post::dump` writes `PARAM LiveTool` from - the stream is
  byte-identical);
- the script: `post::dialect::work_voice(cps)` - the manifest's
  `capabilities` mask read three ways (turning bit -> `WorkSpindle`,
  milling bit only -> `ToolSpindle`, neither or none -> `Undeclared`).
  `dialect::speaks_turning` (#1190's door) is now `work_voice ==
  WorkSpindle`, so both rules read one mask reader.

| Post dialect | Section (`voice_of`) | What the chuck limit judges |
|---|---|---|
| turning (`fanuc-lathe.cps`, `tormach-lathe.cps`) | `WorkSpindle` (turning kinds, on-centre Drill) | the stated rpm: G97 refused by name above the limit, at it allowed; the G96 clamp min(machine, tool, chuck) untouched |
| turning | `LiveTool` (moves state C) | the machine's C rate, max(C feed, C rapid) / 360; no C rate -> refused by name |
| turning | `Refused` (milling kinds, off-centre or tilted drill, probe) | nothing - the lathe door refuses it by name before a line posts |
| milling (`capabilities = 1`), on a mill or no machine | any | nothing: the S word drives the TOOL spindle, which no chuck holds (turning kinds cannot be created there). A workholding limit stated on such a setup has no effect on what that post writes |
| milling, on a MILL-TURN | `WorkSpindle` (turning kinds, on-centre Drill) | the stated rpm (third review: a facing op through `fanuc.cps` / `haas.cps` posted `S3000 M3` under a 2000 rpm chuck - main refused it). A turning pass only cuts if the work turns, so whatever the post calls itself its S turns the chuck |
| milling, on a mill-turn | `LiveTool` | the C rate (the engine refuses a live section through a milling post anyway) |
| milling, on a mill-turn | `Refused` (a milling section) | nothing - the tool spindle cuts; no chuck turns at that S |
| milling, on a plain LATHE | any plain section | judged as undeclared (next row): a lathe's one spindle holds the chuck, so whatever the script calls itself its S turns the work, and the lathe door does not fire for a milling script |
| undeclared (no mask; #1199) | `WorkSpindle` or `Refused` | the stated rpm - the stricter reading while a limit is stated (the door does not fire for it). Whoever settles #1199 changes this arm and `lathe_voice::at_post` together |
| undeclared | `LiveTool` | the C rate, as above |

The dialect is the one of the script that RUNS: `post_script` /
`post_mounts` with an explicit script judge every op by it; the gate and
per-mount / per-op posts judge each op by its own setup's post. The
machine's kind enters only through the milling-post rows, and only to
widen what is judged. Judging was chosen over a door that refuses turning
kinds through a milling post: judging adds a refusal only when the speed
is over the limit, while a door would refuse every turning op through a
mill post on a mill-turn, limit stated or not - changing programs that
post today. No machine mounted (or a mill) under a lathe post is judged like
a lathe - the second review's fail-open.

Refusal text prints whole rpm, or one decimal when the two numbers would
print the same ("2000.4 rpm ... maximum of 2000.0 rpm").

**Why.** What spins the chuck is the post voicing a section on the work
spindle, not the operation's kind (the first review: a mill-turn's
on-centre drill passed) nor the machine's kind (the second review: no
machine, or a mill, under a lathe post passed). Reading the door's own
derivation keeps the gate and the door from disagreeing.

**Considered and dropped.**
- Our own `post::section_spindle` (Work / LiveTool / Posed) and a text
  scan of the manifest: folded into #1190's `dialect.rs` / `lathe_voice`
  so there is one reader of each question (coordinator, 2026-11-12).
- Judging `Refused` sections under a turning post: the door refuses them
  first; judging them too would print a second, misleading reason (the
  10000 rpm cross drill "turns the work").
- Judging a C section by the speed C actually reaches: a second copy of
  the post's composite-feed math; the stated C rate is the stricter ceiling.
- Letting an unrated C or an undeclared script pass: a limit that is
  skipped is a hazard.

**Cost to change later.** `work_speed` in `setup_workholding.rs` and
`dialect::work_voice`; pins in `lathe_workholding_kinds`,
`lathe_workholding_live`, `lathe_workholding_rpm`.
