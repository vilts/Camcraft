## #1015 - the refusal ghost where a frame map or a merge came first (2026-11-12)

**Decided.** The rule every site follows: a ghost is in the frame of
the path the generator would have returned. `run_job` already maps
that frame to world (no frame, or a 3+2 tilt through `world_path`), so
a ghost in the output frame lands where the part is. Where a generator
plans in a frame of its own and maps out at the end (the rotary kinds,
the cross drill's spindle shift), the ghost takes the same map the
path does. When no map could be proved, the site has no ghost and is
listed below with the reason.

| Site | Ghost |
|---|---|
| `finishing3d` raster: Parallel/Scallop/ParallelMulti's crossing pass, `rings_and_tail` (raster, blend, radial, spiral, flow, shadow), `swarf_multi`'s rungs | the moves built so far (`with_ghost_moves(b.moves())`) |
| `finishing3d::waterline` (rings) | the rings so far. Today only a non-finite facet or a geo error can refuse there, and both stop at the first level, so no ghost is seen yet |
| Rotary Contour / Parallel's calls of `waterline` / `swarf` on frame-local facets | carried out through `posed`, the same pose law and world exit as the finished path (`rotary::to_world`). A ghost `posed` refuses is dropped. The spot rides `f.to_world` |
| Rotary Parallel's `radial_relink` (links refused part-way) | the passes linked so far, carried out the same way |
| 5-axis stamp (`stamp_axes`: Multi-Axis Rough, Contour/Parallel 5D, Flow 5D, swarf): shank refusal | the plan before the refused move. This is world, as the stamp's input is, and **not posed** (the vertical ball's tips about the same ball centres, within one ball radius of the posed tips) |
| Contour/Parallel 5D, Swarf 5D: a refusal in the finishing plan | the plan's ghost passes up as is (world, unposed) |
| Swarf link laws (`link_swings`, `guard_turns`) | the posed path they judged (world) |
| Multi-axis curve walk (`contour_curve`) | the chains walked before the refused one (world, posed) |
| Rotary gate (`stamp_spindle_c`) | the kernel's path it judged (world, since the rotary kinds return world) |
| Fixture route (`plan_links`) | the path (upright mill only, frame None, so world) |
| Cross drill's app-side laws (`rapids_clear`, `cross_holder`) | the spindle-frame chain moved back by `(ox, oy)`, the same shift the posted chain takes. A chain with an arc has no ghost |
| Cross drill's `part_clear` | the world chain it judged |
| Nested: `pocket3d_air_split`'s material leg, `run_rest`'s rest ladder, the stitched contour's runs | `GenError::after_moves(outer)`: the outer moves go in front of the inner ghost, or become the ghost when the inner refusal carried none. Its only `pocket3d_air_split` caller (Rotary Offset) still drops the ghost on purpose (development space, #871) |
| Face under an area turn | handled by #1023 (`boundary_turn::turn_refusal` / `face_over_back`). Not touched here, so the stacks do not conflict |

**Listed without a ghost, by reason.**
- *Rotary posing (`rotary::posed`) itself.* Its refusals are a cycle in
  the base or an empty base (nothing to show), and the finite-horizon
  tail (a ghost would be dropped by law).
- *The rotary letter gate (`rotary_letter`).* It runs before the kernel,
  so no move exists yet.
- *The curve walk's own `walk`.* It holds contact rests, not moves. The
  refusal is mapped at `contour_curve`, where the moves are.
- *`run_rest`'s sim steps between the ladders* (the lattice budget). The
  issue did not list them. The coarse ladder is in hand there, so they
  could adopt `with_ghost(&coarse)` later if wanted.
- *A ghost the rotary exit refuses.* It is dropped and never drawn in
  the planning frame.

**Considered and dropped.**
- Posing the 5-axis stamp's partial plan for its ghost. The stamp's
  pose memory (`held`, `leaned`) runs over the whole station list, so a
  posed prefix would need the stamp's own loop to build moves part-way.
  The unposed world plan is the honest picture of what was being posed,
  and it is in the right frame.
- Relinking the rotary raster's ghost before posing it. The raster's
  own Z lifts are posed as they are. The relink can refuse in its own
  right, and the cut points, the part of the ghost that reads, are the
  posted ones either way.
- A ghost of the plain `f.to_world` map for the rotary ghost. It puts
  every tip one ball radius off along the frame axis instead of the
  radial pose, so it would not be where the path posts.

**Cost to change later.** Small. Each site is one `map_err` on an
error path. `after_moves` is one method over `Ghost::after`. No success
path changed, so every golden stays byte-identical.
