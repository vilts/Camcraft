## #1188 - a stop or go-home after a live-tool section posts at the end of that section

**What was decided.** `fanuc-lathe.cps`'s section end no longer returns
from the live-tool branch before `afterOp()`. Every section, turning or
C-axis live tool, now ends with its own after-operation words. The live
section ends in this order:

1. `G80` if a cycle group is open, then `M9` (coolant off) - shared with
   the turning branch, unchanged;
2. the machine's live spindle-off word (`live_tool.spindle_off`);
3. the machine's C-axis-off word (`live_tool.c_axis_off`);
4. `G99` (feed per revolution back, as before);
5. the asked `M01` / `M00` / `G28 U0` + `G28 W0`.

**Why this order.** It is the turning branch's order (coolant off,
spindle stop, spindle-mode reset, then the stop words) with the live
section's own release words in the spindle-stop place. A stop is where an
operator reaches into the machine (chips, checking a hole, flipping a
part), so the driven tool must already be stopped and the C axis handed
back to the turning spindle before it: an `M00` with the live tool still
turning, or the C axis still engaged when the operator restarts into a
turning section, is its own hazard. The spindle-off comes before the
C-axis-off, as it did before this fix: the driven tool stops while C still
holds the part where it cut, and only then is the spindle released. The
go-home moves only X and Z (`U0`, `W0`), after C is released, so no home
move drags an engaged C. `G99` before the stop words mirrors the turning
branch's `G97` before them: the modal reset is part of the section's
release, the stop words its last act. No new code is invented: every word
is the machine file's or one the post already wrote.

**Also:** with the live section refused (the machine states no live-tool
words, `liveOk` false), the ask is still consumed at that section's end,
so it can never ride into the next section even in a program that is not
released.

**Checked and found fine.** Every other `endPreamble` / `onSectionEnd` /
`startPreamble` early return in `fanuc-lathe.cps` and `tormach-lathe.cps`
(the `sectionOpen` guards only); `tormach-lathe.cps` has no live branch
and calls `afterOp()` on every section end. The mill posts' section ends
(`fanuc`, `haas`, `heidenhain`, `mazak`, `sinumerik`, `doosan-dnm`) have
no early return before `afterOp()`. `tormach.cps`, `carvera.cps` and
`tormach-plasma.cps` write no after-operation words and do not declare
`afterModes`, so the engine refuses a stop or home ask for them by name.

**Considered and dropped.** Writing the stop before `G99` (it would leave
the control in feed per minute across an operator stop - harmless, but not
the turning branch's shape); a separate `afterOp()` call inside the live
branch (two call sites to keep in step instead of one).

**Cost to change later.** One function in one script; the order is pinned
by `script_fanuc_lathe`'s after-live tests and the `live_then_stop`
recorded golden.
