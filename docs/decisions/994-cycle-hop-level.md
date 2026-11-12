## #994 - canned-cycle posts traverse between holes at the hop, not at R

**What was decided.**
- The six posts that write canned-cycle words (`fanuc`, `haas`, `mazak`,
  `tormach`, `doosan-dnm`, `sinumerik`) treat as the cycle's chain only the
  moves that stay on its hole: the hole's XY, at or below its R plane. The
  first rapid that leaves the hole (the hop up, the traverse, the exit
  rapid) ends the group with `G80` and is posted as the stream states it. So
  the posted traverse between holes and the exit from the last hole are the
  stream's own rapids, at the hop / clearance the path was verified at.
- Each hole returns to R. The Fanuc-family and PathPilot posts write `G99`
  in every cycle-opening block (RS-274 / ISO 6983: G99 returns to R, G98 to
  the level the tool stood at when the cycle was read). The once-per-program
  `G98` line is gone.
- Sinumerik writes no return word, as before. The post did not invent G98
  or G99 for it, and its retract mode is not assumed: the tool is seated on
  R over the hole before the cycle is read, so a control returning to the
  initial level lands on R too.
- The seat (`cycleSeat`) covers a stream whose cycle is anchored on the
  rapid down to R, so the cycle record arrives before that rapid. It goes up
  to R first when the tool is below it, then over the hole, then down, so it
  never runs under the straight rapid the stream states. A cycle that is its
  section's first motion is not seated: there is no stream position yet, and
  the cycle positions from the height it stands at, as before.
- A cutting move (linear or arc) that leaves the hole while a group is open
  is refused by name ("a cutting move leaves its hole inside a canned-cycle
  group"), the way Heidenhain already refuses an inconsistent chain. Before,
  such moves were swallowed without a word.
- Pinned by `crates/post/tests/cycle_hop.rs`. One generic checker reads a
  posted program back the way a control runs it: it positions over the hole
  at the current level, returns per G98/G99, takes the lower level when no
  mode is stated, and takes the lower end of a rapid as a dogleg's Z. It
  checks that every XY traverse after the first hole runs at Z >= the
  stream's `RetractPlane` and that the tool leaves the cut at Z >= its
  `ClearancePlane`. It runs on all six posts, on the recorded
  `drill_retract` stream and on a variant anchored on the rapid to R. The
  #1002 `drill_retract` goldens are recorded for all six (out of
  `KNOWN_BUGS`).

**Why.** The bug was G98 read with the tool already on R (the stream's
move 0 is the rapid down to R, posted before the cycle). Its "initial
level" was therefore R, and the cycle swallowed the hop rapids. The posted
traverse ran at R, lower than the simulated and verified motion (P0).
Posting the stream's own hop rapids does not depend on how a dialect
defines its initial level, and it works for any hop or clearance the
generator chooses.

**Considered and dropped.**
- *G98 with the initial level raised to the hop* (skip the rapid down to
  R, so the cycle is read at the hop). The post has no lookahead to know a
  cycle follows a rapid, and the clearance exit after the last hole would
  still need its own move. Dropped.
- *Expanded chain for Sinumerik.* It would be faithful, but it loses the
  cycle's dwell, tap and bore semantics (tapping cannot be expanded; see
  Heidenhain's refusals) and needs new refusals. The seat makes the return
  level unambiguous without that cost.
- *Keeping bare X Y modal continuations between holes.* A modal hole
  traverses at the cycle's return level. Keeping it would need G98 and a
  per-hole initial level, the ambiguity this fixes. After a section's first
  motion, a cycle read away from its hole is seated, which posts the
  stream's own traverse and opens a new group. The continuation branch now
  fires only for a second cycle on the same hole.
- *Closing the group on a leaving cut and posting it.* A mis-anchored
  stream would then post the hole's plunges a second time. Refused instead.

**Cost to change.** Six `.cps` files carry the same `onHole` /
`chainRapid` / `chainCut` / `cycleSeat` helpers (a `.cps` has no module
system). A new cycle post copies them and joins `CYCLE_POSTS` in
`cycle_hop.rs`.
