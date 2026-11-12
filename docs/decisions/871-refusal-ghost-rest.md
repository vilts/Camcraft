## #871 - the refusal ghost, part-way refusals (2026-11-12)

**Decided.** A generator that refuses part-way now hands back the moves
it had built, wherever those moves are in hand at the refusal and the
generator works in a frame `run_job` already maps to world (no frame, or
a 3+2 tilt mapped through `world_path`). Each adopter is one
`.with_ghost(&path)` or `.map_err(|e| e.with_ghost_moves(b.moves()))`
on an error path. The success path does not change, so every CL/NC
golden stays byte-identical. `GenError::with_ghost_moves(&[Move])` is
new: it is `with_ghost` over a builder's moves, so no generator had to
`finish()` early or move its control flow.

| Generator | Refusal | Ghost |
|---|---|---|
| 2D Contour (`contour_impl`) | control comp with no lead-in/out; `push_span` | rings so far |
| Bore | per-hole pass budget (`cut_levels`) | holes bored so far |
| Circular | per-ring pass budget | rings so far |
| Trochoidal slot | level emit (helix-turn cap); loops cross themselves | levels so far; the finished path |
| 3-D ladder (Pocket3D, Multi-Axis Rough) | `hsm::plan_level` (adaptive level refusal) | levels so far (Rotary Offset still drops it, `without_ghost`) |
| Plasma profile | no cut / G41 span off its cut; cover and kerf laws; torch spans | contours so far; the whole cut |
| Probing | a touch that does not run toward its target | the built cycle |
| Turning form | crown on or through the axis | valleys so far |
| Turning groove (blade) | station's rung budget | stations so far |

**Nothing useful to show (listed, by reason).**
- *Refusals before any move* (parameter, region, tool doors; the bore and
  thread-mill hole checks). Nothing was built. Not owed (#261).
- *"Path left the finite horizon" tails* (bore, circular, contour, pocket,
  slots, chamfer, face, thread mill, trochoid slot, contour chain, the
  finishing tails, rotary, stamp, cross, cycle, probing). `Ghost::of`
  refuses a non-finite move by law, so the ghost would always be dropped.
- *Drilling cycles* (axial `cycle.rs`, radial `cross.rs`). Every refusal
  is a per-hole door that already names that hole's mouth (#261 part 2).
  The holes before it are straight plunges at other mouths. Their ghost
  would add dots, not a reading of what went wrong. Kept as the issue
  listed it.
- *Rotary Offset's inner ladder*. Its moves are in the development (wrap
  space), not where the part is. `without_ghost` stays, on purpose.
- *Trochoidal slot's lifted-return trial*. The `trial` path is a scratch
  level at z 0..1, not the path that would post. A ghost of it would show
  a path that never existed.
- *Boundary turn mid-rotation* (`boundary_turn.rs`). The moves are half
  rotated when it refuses, so the ghost would draw in neither frame.

**Owed (needs a frame map or a merge, not a one-liner; follow-up issue #1015).**
- *The finishing kinds that Rotary contour and parallel also run*
  (`finishing3d` raster, rings, swarf). Rotary calls them on frame-local
  facets, and no tilt frame maps those out. Adopting a ghost there first
  needs `without_ghost` (or a `to_world` map) at `multiaxis/rotary.rs`'s
  calls, or a ghost would draw in the wrong place.
- *Rotary links and posing* (`rotary_links.rs`, `rotary.rs` `posed`). The
  frame is rotary-local and needs `f.to_world` plus the tip offset.
- *Face under an area turn* (`generate.rs` `turn_path(face_over(..)?)`).
  The refusal's spot and any ghost are in the turned frame and need
  `turn_path(-turn)`. The spot is already misplaced there today.
- *Cross drill's app-side laws* (`rapids_clear`, `cross_holder` on the
  spindle-shifted `t`). These need the same shift `in_world` applies.
- *Merges*. Refusals that wrap an inner refusal already carrying its own
  ghost (`pocket3d_air_split`'s material leg, `run_rest`'s rest ladder, the
  stitched contour's runs in `generate.rs`) would need the outer moves
  merged in front of the inner ghost. `with_ghost` keeps the inner one.
- *Multiaxis stamp, swarf link and curve walks*, and the app's rotary
  gate and fixture route. Each is a one-liner but is posed or unposed
  world. Left for the follow-up so each one gets a frame check, not a
  sweep.

**The two match sites.** `cross_spindle::in_world` rewrites a `Ghosted`
refusal like the located one. Its sentence is rewritten, its spot moves
by the hole shift, and its moves are carried by the same translation. If
there is no shift to read, or the ghost holds an arc that the XY carry
would have to re-centre, it drops the ghost (and the spot), so nothing is
drawn in the spindle frame. `picks::door_refusal` reads a `Ghosted`
refusal's words like a plain `Invalid` one. The pick shows words only, so
the ghost is dropped there. A third site the survey found,
`body_judged::name_enclosure`, now goes through `GenError::map_msg`, so
a ghost-carrying holder refusal keeps both its "(enclosure)" words and
its ghost.

**Colour.** The ghost uses #698's ground-aware dimming. On the dark ground
it is the gouge colour scaled, byte for byte what #861 drew. On a light
ground it fades toward the ground and never toward black. Cutting moves
keep 0.9 of the gouge colour (`ground_ink::faded`, the light half of
`dimmed` now open for a caller's own share), so they clear WCAG 1.4.11's
3:1 on the light viewport in every colour-vision palette. Rapids take
`dimmed`'s own 0.6, so they are fainter. This is pinned by
`the_ghost_reads_on_the_light_ground_and_keeps_its_dark_tones`.

**Dropped.** `dimmed`'s 0.6 for the cutting moves too: it reads 2.3:1 on
the light viewport, under 3:1. Darkening toward black on light: it reads,
but it turns the red to ink, which #698 retired for dimmed paths. A
`Ghost` merge API for the nested refusals: not needed by any site adopted
here.

**Cost to change later.** Small. Each adopter is one `map_err` on an error
path, and dropping one is deleting it. The colour is two constants.
