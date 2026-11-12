## #1011 - Lathe face-drilling leaves the hole at the stream's exit level

**What was decided.** `fanuc-lathe.cps` takes #994's hop law. A face
cycle (G83/G84/G85) replaces only the chain that stays on its hole: the
hole's XY (on centre), at or below its R level. The first rapid that
leaves the hole (on a lathe that is the exit up to clearance, Z30 in the
axial-drill golden) ends the group with `G80` and is posted as the stream
says. So the tool rises to the verified exit level before the home move
(`G28 U0 W0`). A feed or arc that leaves the hole while a group is open
is refused by name ("a cutting move leaves its hole inside a face-cycle
group"), not dropped. `tormach-lathe.cps` is unchanged. It writes no
canned cycle and already posts the whole chain as plain moves, exit
included (its `axial_drill_*` goldens end `G0 X0 Z30` before `G30`).
The new read-back pin runs on it too, so it stays that way.

**Why.** The old post swallowed every move after the cycle until the
section ended. The tool then left the hole at R, and `G28 U0 W0` (X and
Z together) started from below the level the path was verified at. That
is the same class of bug #994 fixed in the mill posts, so the same law
applies and the two families read the same way.

**Considered and dropped.**
- Writing the exit as an extra `G00 Z<clearance>` taken from the
  `ClearancePlane` PARAM. Dropped: the stream's own exit rapid is the
  verified motion. Posting it as the stream says is #994's law, and
  needs no second source of the exit level.
- Re-seating the positioning block (Z first when below R, as #994's
  `cycleSeat`). Dropped here: a lathe drills one on-axis hole, and the
  cycle's `G00 X0 Z<R>` is written only when the stream's approach has not
  already put the tool there. The axial-drill stream always has. Left
  as it was, out of scope for #1011.
- A record with no R word (legacy streams only; `CycleRec.r_z` is not
  optional). Its R level is taken as unbounded, so the whole on-axis
  chain stays swallowed as before, and only a move off the axis closes
  the group. Cost to change: one line in `onCycle`.

**Cost to change later.** Small. The rule is three helpers in
`fanuc-lathe.cps` (`onHole` / `chainRapid` / `chainCut`) and the pin
`crates/app/tests/lathe_cycle_exit.rs`.
