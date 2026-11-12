## #1205 - the hole-wall excuse measures a facet about its hole's axis

**What was decided.** The containment kinds' (drill, bore, thread mill,
cross drill) hole-wall excuse in the solid law no longer reads a facet by
its corners against the tool's own disc. The operation now names its
holes' axes (`verify::solid_hits_walled`, `HoleAxis`), and a facet is
excused only when it is a ROUND wall of one of them: its sagitta about
that axis - its nearest vertex's radius less its nearest approach to the
axis (point-to-segment, or point-in-triangle, in the plane square to the
axis) - is at most `WALL_SAG` = 0.05 mm. The penetration excused is the
smaller of that sagitta and the old edge-dip bound, so the law can only
convict more than before, never less. A facet with any non-finite
coordinate, a facet over the axis, a hole axis that is not along the
move's tool axis, or a law with no holes named excuses nothing.

**Why.** The old bound (`chord_dip`: a chord with both ends outside the
tool disc dips at most `rho - sqrt(rho^2 - l^2/4)`) is a fact about the
tool, not about the wall: it grows as the tool shrinks against the hole,
and every flat whose corners stand outside the tool was excused by it. A
thread mill cut 0.32 mm into the flats of an octagon on r 5 with 0
warnings (PR #1099's review). Measuring about the hole's axis says what
the excuse was meant to say: the tool may reach the facet's corners'
circle and no farther. The cap is needed on top: an octagon IS an
inscribed polygon of its corners' circle (sagitta 0.38 mm on r 5), so the
nearest-point measurement alone would still excuse a cut to its corners.
Only a stated bound on what counts as tessellation tells a coarse circle
from a flat.

**WALL_SAG = 0.05 mm.** Five gouge tolerances. It covers every hole the
committed fixtures model (32-gons to r 5: 0.024 mm; 128-gons) and any
32-gon to r 10 / 64-gon to r 41, and it is under a tenth of an octagon's
or a hex's flats on any hole bigger than ~1.3 mm across. The worst cut it
can let pass is 0.05 + the 0.01 gouge tolerance, radially, into a wall
that is round to within 0.05 - against no bound at all before. A hole
imported coarser than that (a 16-gon on r 5 stands 0.096 inside) now
convicts: a false gouge on a concave wall is the safe side (the #864 law
says the same), and the user re-exports finer or declares the hole at
its flats.

**Considered and dropped.**
- *Nearest point without a cap* (the issue's first wording): still
  excuses the octagon and the D-bore's ends; drops the probe.
- *An angular cap* (a facet may subtend at most N degrees about the
  axis): scales with the hole, so a 24-gon on r 50 (0.43 mm flats) would
  pass. An absolute depth is what a gouge is measured in.
- *The model's own chord tolerance* (STEP's `CHORD_CAP` + excess): an STL
  carries none, and every committed fixture is an STL-shaped mesh coarser
  than the STEP promise. Worth revisiting if imports ever carry a stated
  chord error for every format.
- *Keeping the tool-frame rule and only tightening it* (vertices outside
  `rho - allow` measured at the facet's nearest point): that makes the
  excuse vacuous (pen is then at most `allow`), convicting every
  32-gon golden.
- *Requiring the facet to sit near the declared radius*: the declared
  radius means different things per kind (a thread's minor, a bore's
  finish, a drill's own size); the axis alone with the cap is the tighter
  test and needs no per-kind reading.

**Cost to change later.** `WALL_SAG` is one constant
(`crates/sim/src/verify/hole_wall.rs`), pinned at its boundary by
`hole_wall_flats::the_wall_excuse_ends_at_its_stated_sagitta`. PR #1099's
thread-mill grant (`thread_bore.rs` `wall_radius`, `material_source.rs`
`major_rims`) still measures by vertices; its review asks for the same
nearest-point reading, and both should end on this one rule.
