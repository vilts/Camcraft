## #983 - every adaptive level that falls back to rings is named

**What was decided.** The "radius 5.6 cut by rings with no note" seen
while working #882 was reproduced and traced: it was not a planner
fallback. In the variant "outline thinned at 0.25 um, no inset" (held
branch `issue/882-adaptive-open-simplify` with `INSET = 0`), the 5.0
level's microlift returns lift to 5.6 and one down ramp runs 5.7 -> 5.6
-> 5.5 with a vertex on 5.6. The rotary_adaptive level reader counts any
plane a descent ends on as a cut level, so it read the lifted runs at
5.6 as a level with 7 planar moves, no arc and no load, and called it
ring-cut and unnamed. The reader now reads only the ladder's own planes
(`top - k * stepdown`); `read_levels` takes the stepdown.

The planner side was audited arm by arm. Every arm in
`machining3d::hsm::plan_level` that hands an Adaptive level to the
Offset rings already named the level (seam crossed, narrowed helix, hard
planner refusal on the wrap, and `adaptive_passes`' own fallback with
its reason). They now all go through one helper, `rings_named`, that
writes `<why> - Offset rings cut <level> instead`. The note text is
unchanged. A guard was added for the planner's own fallback: if
`adaptive_passes` ever hands back rings without a note ending in the
level's tag, `plan_level` adds one ("the HSM plan was not posted").
Toolpaths and verdicts do not change.

**Why.** "Log diagnostics, never silence." One way out keeps a future
arm from switching strategy without saying so. The reader fix makes the
"every ring-cut level is named" check rest on the ladder's own levels,
so a sub-micron change in the microlift lift heights no longer makes up
a level.

**Considered and dropped.**
- Naming every rest-pass and offset-shell level separately. Those
  passes cut rings by rule for the whole pass, not because the planner
  refused a level. They are said once (`REST_RINGS`, `SHELL_RINGS`), as
  before, and the rotary ladder runs neither.
- Loosening `the_r62_neck_levels_are_refused_for_travel_by_name` so that
  5.7 and 5.2 may be either refused or planned. That pin flips under
  #882's variants because of the microlift acceptance knife edge
  (#982). That issue owns it, and both outcomes are already named.
- Reading ring-cut levels from the notes instead of the program's shape.
  The shape reader is what catches an unnamed switch, so it stays.

**Cost to change later.** Small: one helper and one guard in `hsm.rs`,
and one extra argument on the test reader.
