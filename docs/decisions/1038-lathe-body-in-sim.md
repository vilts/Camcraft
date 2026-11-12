## #1038 - the simulated turning tool is its lathe body, posed in the holder law's frame

**Decided.** A tool carrying a lathe-frame body (#369) is drawn in the
simulation and viewport as that body's own tool-frame facets
(`ToolBody::facets`, the mesh `LatheClearance` is derived from), not
revolved, posed at the playhead: the tip at the frame's position, +x
along the op path's signed radial, +z along the spindle, +y = z x x.
The frame is the holder law's own: `operation::lathe_body_axes` fits
`turning::Frame::about` to the op's source facets (`source_facets`, the
read the law and the lathe stock sim make) and takes the radial from
`SignedMeridian::of` on the op's cached path. It is cached per op, path
signature and body (by identity), so the fit runs once per change, not
per frame. The pose lives in a new `app::sim_lathe_body` and is
reached through the existing `SimToolCache::posed` seam the ui already
calls - no new ui drawing path.

**Why.** "What is drawn is what is checked": the law reads a body point
(x, y, z) from the tip at (station S, signed radius X) as standing
sqrt((X + x)^2 + y^2) from the axis at S + z. Posing the facets in that
exact frame makes every drawn vertex stand where the law reads it (the
tests check that identity vertex by vertex, OD and ID), so the drawn
holder can be neither smaller nor elsewhere than the judged one.

**Also decided.**
- A boring bar (`Bore`, which may carry either frame) with a lathe body
  draws the lathe body too; before, it drew the revolved bar stand-in,
  which is not what the holder law judges for it.
- The frame's `axis` from the sim script is not used for a lathe body:
  a turning move's tool frame is the spindle's, not the commanded axis.
- A lathe steel without a body draws nothing beyond the axis marker,
  exactly as before; an op with no cached path yet draws nothing.
- A frame that cannot be fitted (no spindle fits the source) is refused
  by name in the log once, and nothing is drawn - never a guessed pose.

**Considered and dropped.**
- Tinting the body on a `HolderHit` move (the issue's "could"): it
  needs a new colour channel through the ui part (the sim body part is a
  plain shaded model part today); left for a follow-up rather than
  widen this display change.
- Assuming world Z through the origin as the spindle: cheaper (no fit),
  but it is not the law's frame when the part's fitted axis is offset.

**Cost to change later.** Small: the pose is one pure function
(`sim_lathe_body::posed`) and one cached frame read.
