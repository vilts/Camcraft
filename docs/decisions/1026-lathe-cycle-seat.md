## #1026 - Lathe face cycles are seated by the hop law, never one diagonal rapid

**What was decided.** `fanuc-lathe.cps` takes #994's `cycleSeat`. A face
cycle (G83/G84/G85) is read with the tool on centre at its R level. If
the tool stands somewhere else when the cycle record arrives, the post
seats it in up to three plain `G00` blocks: `Z<R>` first when the tool
is below R, then `X0` when it is off centre, then `Z<R>` when it is
above R. The old single `G00 X0 Z<R>` is gone, so the tool never
reaches the seat by one diagonal X+Z rapid below R. Three cases cannot
be seated, and each is refused by name with no cycle written:
- the cycle is its section's first motion, so there is no stream
  position to seat from ("approach the hole first");
- the record's bottom is not below its R, so the post cannot tell which
  way is clear;
- a legacy record has no R word and starts off the axis.

A legacy no-R record that is already on the axis posts as before. When
a group is still open and its next cycle needs seating, the post writes
`G80` before the seat moves.

**Axis assumption.** "Away from the work" is +Z. The post's own
convention, stated in its header and in `zWord`, is that program Z runs
along the spindle axis with the face at the free end. Its face cycles
drill toward -Z from R to the bottom, and the #1011 `onHole` test
(`Z <= R`) already relies on that. The common public lathe G-code
convention says the same: Z+ points away from the chuck and the work
face. The post does not just assume this. It checks the record itself
(bottom below R) and refuses a cycle that says otherwise.

**tormach-lathe.** No change. It writes no cycle and no positioning
block of its own. Every motion line it posts is one of the stream's
moves, in order (pinned by
`tormach_lathe_adds_no_positioning_move_to_an_off_seat_face_cycle`).
So the only diagonal it can post is one the stream itself states,
which is the generator's verified path.

**Why.** The old block was a straight X+Z rapid from wherever the tool
stood to the R point. From below R and off centre, that line runs below
R toward the face and can pass through the part. The mill posts have
seated this way since #994, so both families now read the same.

**Considered and dropped.**
- Seating a section's first-motion cycle from an assumed tool-change
  position (home is clear on both axes). Dropped: the post does not
  write a home before the tool change, so where the turret stands is
  unknown. The issue says to refuse in that case. Real axial-drill
  streams always approach the hole before the cycle record, so they are
  unaffected. The synthetic test fixtures that put the cycle first now
  carry an approach rapid.
- Moving X at the current height when the tool is below R, then Z up.
  Dropped: that is exactly the traverse below R that the hop law
  forbids.
- Seating on a level above R (the exit or clearance). Dropped: R is the
  level the cycle returns to and the stream's chain starts from. Seating
  higher adds motion the stream does not state.

**Cost to change later.** Small. The rule is `cycleSeat` in
`fanuc-lathe.cps` plus its `seatKnown` flag, pinned by
`crates/app/tests/lathe_cycle_exit.rs`, whose checker now also reads
the seat.
