## #946 - Gouge law (b) reads XZ / YZ arcs on their XY footprint in Boundary mode

**Decided.** In Boundary mode an XZ / YZ (G18 / G19) arc now answers to
the wall edges on its true XY footprint (`sim::verify::arc_wall_plane`),
as XY arcs have since #905, instead of only at the union of its chord
joints:

- **Planar** arc (normal coordinate equal at both ends): the footprint is
  one exact segment along X (G18) or Y (G19), from the least to the
  greatest value the in-plane coordinate takes over the sweep - the
  endpoints, or `c +- r` where the sweep passes the coordinate's turn
  (read at the radius band's outer edge, `ARC_PAD` past the larger
  endpoint radius, so it never reads shorter than the cut). Read by the
  straight-move branch, float for float.
- **Helical** arc: a polyline of at most `MAX_ARC_CHORDS` pieces through
  points of the arc (sagitta 1e-4 mm), every piece read `pad` nearer than
  it stands: `r d^2 / 8` (the linear-interpolation bound; the normal
  coordinate is linear in the sweep, so it adds nothing) plus the radius
  band's width. Never farther than the truth; at most ~0.1 um + band
  nearer.
- Degenerate arcs read as `cl::arc_chords` cuts them (zero radius or
  sweep: the straight move); a non-finite centre or coordinate fails
  closed (`Trace::Unread`, a gouge).

**Why.** The issue's P1 argument (the projected trace is a line, so the
joint reading is near exact) does not hold: the joints stand up to a
chord length apart along that line. With r 200 and the 0.01 mm verdict
tolerance the joints of the pinned arc stand 1.92 mm apart in XY, so a
wall vertex 3.90 mm off the footprint (90 um past the 3.99 mm floor of a
4 mm cutter) read >= 4.016 mm from every joint and passed. The pin
`a_vertex_between_a_vertical_arcs_joints_past_the_floor_is_a_gouge`
fails without the fix and passes with it.

**Considered and dropped.**
- Reading the planar arc by the endpoints' XY segment (`from`-`to`): an
  arc that passes the coordinate's turn reaches past both ends.
- Reading every vertical arc as the padded polyline: works, but a planar
  arc would carry a pad (false refusals of up to the pad) for no reason
  when its exact footprint is one segment.
- An exact distance for the helical footprint (a sine curve against a
  segment): no closed form; the bounded polyline is honest and cheap.

**Also from the issue.** `boundary_arc_walls.rs` moved to `crates/sim/tests/`
(#921's rule, it uses only `verify` re-exports); the per-edge `Vec` in
`XyArc::dist_seg` is a fixed 4-slot buffer. The model-backed pin
(`a_model_corner_between_a_vertical_arcs_joints_is_judged`) exercises the
model branch of the wall reading; on its own fixture the solid law (b2)
convicts the same corner too, so it pins coverage and verdict, not the
wall reading alone.

**Cost to change later.** Local to `arc_wall_plane.rs`; the `Trace` enum
gained a `Poly` variant and `Trace::of` now always returns a trace.
