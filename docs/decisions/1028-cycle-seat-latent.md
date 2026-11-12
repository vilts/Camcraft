## #1028 - a cycle seated away from its hole traverses at the hop; probe moves seat the next cycle

**What was decided.**
- In the six canned-cycle mill posts (`fanuc`, `haas`, `mazak`,
  `sinumerik`, `tormach`, `doosan-dnm`), `cycleSeat` treats a seat *away*
  from the hole (no stream rapid brought the tool over it) as a traverse
  the stream did not state. It runs at the section's hop: up to
  `max(RetractPlane, R)` when the tool is below it, then over the hole,
  then down to R. A tool already above the hop traverses where it stands.
  The seats that stay over the hole (only up or down to R) are unchanged.
- The hop is the section's own `PARAM RetractPlane`, captured in
  `onParameter` and forgotten at section end. A section that states none
  has no hop, so an away seat is refused by name ("a canned cycle is read
  away from its hole with no rapid to it, and the section states no
  RetractPlane to traverse at"). The post never invents a height.
- Probe moves (`onProbeMove`, `onProbeRetreat`, `onProbeGuard`) now go
  through `probeSeat()`. It sets `seatKnown`, the way `chainRapid` and
  `chainCut` do for other motion, and closes an open cycle group with
  `G80`, because no probe move is part of a hole's chain. The engine
  already tracks the tool position through probe records (the touch's
  expected point, the back-off's expected point, the guard's target), so
  a cycle read after probing is seated from there.
- Pinned in `crates/post/tests/cycle_hop.rs` on all six posts:
  - `back_to_back_cycle_records_traverse_at_the_hop`: the recorded
    `drill_retract` stream with hole 1's hop, traverse and descent cut
    out.
  - `back_to_back_cycle_records_without_a_hop_are_refused_by_name`: the
    same stream without `RetractPlane`.
  - `a_cycle_read_after_probing_is_seated_at_the_hop`: a section whose
    first motion is a guard, a touch and a back-off, then a cycle away
    from the touch.

  The checker gained `check_from` (watch traverses from the first known
  position, not only after the first hole), G90/G91 incremental reading,
  and G31 / G38.2-G38.5 read as straight moves. Its own pin is
  `the_checker_reads_probe_moves_and_incremental_back_offs`.

- The conformance synth stream (`crates/app/tests/conformance.rs`,
  `synth_stream_with`) was the one fixture of this shape: cycle records
  back to back, with no `RetractPlane`. With this change it would be
  refused. The decision (option 1) is that it now states
  `PARAM ClearancePlane 10` and `PARAM RetractPlane 5` (above its R plane
  2), placed after the origin PARAMs as the recorded `drill_retract`
  stream has them. So it states what every generated stream states.
- The six `synth_*.nc` goldens (fanuc, haas, mazak, sinumerik, tormach,
  doosan-dnm) now lock the hop traverse. Each hole climbs to the hop
  (work Z0), traverses there, and drops to R (Z-3), where before it
  traversed at R. Sinumerik's N numbers shift with the added blocks. The
  Heidenhain and Carvera synth goldens do not move.

**Why.** Both streams are latent: no generator writes them today (the
generator always writes the chain and the rapids that leave it). A
legacy or foreign stream that does would post an XY traverse at R, or at
the probe's back-off height, which is lower than any verified motion: the
R-plane traverse #994 removed. The hop is the height the stream says
traverses run at, so it is the only height the post can use without
inventing one.

**Considered and dropped.**
- *Always refuse an away seat.* That is safe, but it throws away a stream
  whose section states its hop, and the issue preferred seating.
- *Use the engine's inherited `properties.RetractPlane`.* The engine keeps
  PARAMs across sections, so a section without its own `RetractPlane`
  would borrow the previous operation's hop. Dropped in favour of a
  per-section capture, so an unstated hop is refused instead.
- *ClearancePlane as the seat height.* It is higher and always safe, but
  it is not the height the stream verifies traverses at, and it would
  differ from the `G99` hop moves the generator writes. Dropped.
- *Leaving the group open across a probe move.* A G31 / G38.n / MEAS move
  inside an armed G7x/G8x group is outside what the cycle words describe.
  Closing with `G80` costs one line and is what every other leaving motion
  does.

- *Synth fixture: leaving rapids instead of the planes (option 2).*
  This drifts the fixture further from real streams, which state their
  planes. The hop seat would then go untested in conformance. Dropped.
- *Synth fixture: keep the R-level traverse for a stream that states no
  hop (option 3).* This undoes the issue: it is the traverse #994 and
  #1028 remove. Dropped.

**Cost to change.** The same three helpers (`hopZ`, `probeSeat`, the away
branch of `cycleSeat`) are in each of the six `.cps` files. A new cycle
post copies them, and `cycle_hop.rs` covers it once it joins
`CYCLE_POSTS`.
