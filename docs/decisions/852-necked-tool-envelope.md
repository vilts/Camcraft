## #852 - the stack laws and the simulation read a tool's drawn neck

**Decided.** A tool's drawn cutter outline (`ToolCutter`, #93 slice 2)
now feeds the shank band of every stack law that reads one, and the
simulated tool, but only over the shank's span (flute top to stickout)
and only where it is narrower than the stated shank: `neck::neck_bands`
takes the outline's own conservative bands (`SpunProfile::bands`) over
that span, caps each at the stated shank radius, fills any gap at that
radius and merges to `MAX_BANDS` (merging only grows). It rides as a new
`HolderStack::neck` field, read through `HolderStack::shank_bands` (the
fixture laws c2/d2, the posed and mesh sweeps) and carried on the sim
`Cutter` as `NeckBands` (reach laws c/c3, the solid law's atoms). The
simulation's cutter meridian is drawn from the same bands.

**Why.** One source for the judged envelope, the same seam the drawn
holder already uses (`tool_body::stack`, #835/#843), so the law and the
picture cannot disagree. Putting the neck in the stack meant the laws
that already take `(ToolGeom, HolderStack)` needed no new argument.

**Safety rules taken.**
- No drawn cutter, one that no longer `agrees` with the numbers, or one
  that narrows nowhere carries no neck: the shank is the exact
  `(flute_len, shank_radius, shank_len)` band every law read before, so
  verdicts are bit-identical (pinned).
- A carried neck that does not run contiguously flute top to stickout,
  is non-finite, wider than the shank or has too many bands falls back
  to the cylinder (`shank_bands`, and `NeckBands::fits` in
  `Cutter::judged`).
- A holder body reaching down beside the shank widens the neck band it
  stands beside (the cutter's `BodyBands` skip those bands, trusting the
  shank to cover them).
- The neck is computed against the stored tool record (the numbers the
  outline was drawn to), then judged against the judged record.
- A drawn neck is a moved input: it enters the operation stamp
  (`tool_cutter::digest`, bytes only when there is a neck, so no other
  stamp moves). A drawn cutter with no neck still moves no stamp.

**Considered and dropped.**
- A neck diameter / reach length field on `Tool` (the 2026-11-11 data
  point on the issue): a new record field, file format and UI; the drawn
  outline already states it and is law-checked. Can be added later as a
  way to *draw* the outline.
- Passing the neck as a separate argument to every law: more churn for
  the same reading.
- Feeding the generators (multi-axis tilt avoidance `motion::multiaxis::
  avoid`), drill reach (`operation::drill_reach`, which reads the holder
  face, not the shank radius) and the probe stylus (`probe_op::
  stylus_cutter`, its own one-band body): they do not share this seam
  and stay on the stated cylinder - conservative - left to #842.

**Cost to change.** Contained: `crates/tools/src/neck.rs`, the field on
`HolderStack`, `NeckBands` in `sim::verify::body_bands`, and the three
app call sites (`operation::job`/`generate`, `fixture_struck`,
`sim_tool_body`).
