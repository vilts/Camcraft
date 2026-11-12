## #940 - M12.1: a machining mesh from solidcore-tess, its deviation benched, every planner shortfall a refusal

Stacked on #941 (#916). Seam / bench only: everything is in `crates/kernel`;
`crates/app` still meshes STEP through truck (#917 switches it). No new
dependency between workspace crates; `solidcore-tess` moved from a
dev-dependency to a library dependency of the kernel and `solidcore-record`
(already locked, a path dependency of `solidcore-step`) is named directly,
both at the pinned rev. `Cargo.lock`: one line (the kernel's dependency
list), no new package.

**1. The seam (`camcraft_kernel::machining_mesh::machining_mesh`).** A
`StepSolid` now keeps the reader's B-rep record it was built from
(`record`), and the seam meshes that record with `solidcore_tess::plan`
at `MeshTolerance::chord(tol_mm)` (chord alone, no display angle). It
hands out a `MachiningMesh` (welded positions in mm, triangles, the record
face of each triangle, a `MeshReport`) only when all of these hold, else a
named `MeshRefusal`:

- the tolerance is finite, positive and at least a millionth of the body
  (`MIN_TOL_FRACTION`): solidcore-tess silently raises a finer one to that
  floor, so the seam refuses it instead (#864's rule: a mesh coarser than
  promised is never hidden);
- every planner shortfall judged (point 2);
- at most `MAX_MACHINING_TRIANGLES` = 30 000 000 triangles (the app's own
  `MAX_FACETS`);
- every triangle measured, and the judged deviation (point 3) at or under
  the tolerance;
- the evaluator refuses no face and every record face has triangles;
- after welding by exact position: no open side, no side used more than
  twice, no side used twice in one direction (a turned triangle);
- a positive finite enclosed volume (outward-facing).

Bounds: the triangle budget, the planner's memory bounds (each one that
leaves the tolerance refuses), and the caller's `CancelToken`, polled before
and after planning and per face while measuring. Known gap: the planner has
no cancellation hook at the pin, so it runs to its end inside its memory
bounds (the same kind of gap as the reader's topology pass, #939). A panic
in the planner or the evaluator is caught and refused. No unwrap/expect/panic
in library code.

**2. Each `PlanStats` entry (issue point 2), decided:**

| Entry | Outcome | Why |
|---|---|---|
| `unmeshed` | refuse (`FaceLeftOut`) | a face with no triangles is a hole the tool falls through |
| `lenient` | refuse (`Lenient`) | the planner's own docs call each "still a defect" in its pcurves or seams; its trim cannot be trusted |
| `unsampled_edges` | refuse (`EdgeNotSampled`) | a checked closed solid has no free edge; one here is a defect |
| `over_tolerance`, `within == false` | refuse (`FaceCapped`) | a memory bound left the face off its tolerance |
| `over_tolerance`, `within == true` | note | the planner measured it inside after all; the seam measures it again |
| `edges_capped`, `within == false` | refuse (`EdgeCapped`) | the chords stand off the curve by more than the tolerance |
| `edges_capped`, `within == true` | note | only a side's length or turn rule is past; the chord sag is inside |
| `kept_strict` | note | the edge kept a finer sampling: valid, only finer |
| `averaged_poles` | note | a display normal; positions are unchanged and the machining mesh carries no normals |
| evaluator refusals (`TessPlan::evaluate`) | refuse (`FaceLeftOut`) | as `unmeshed` |

Dropped: a reduced-fidelity flag (#864 / PR #911) for any of them. That flag
is for a mesh that is whole but coarser than asked; every refusal above is a
mesh that is not whole or not measured, and the notes are not coarser.

Pinned by `every_planner_shortfall_is_a_refusal_or_a_note` on synthetic
`PlanStats` values. None of the fixtures provokes a real shortfall: at 0.005,
0.01, 0.001, 0.0002 and 0.0001 mm on Milling-test and sample_ap214 the
planner reports none. A crafted-record fixture per entry is a follow-up (#1029).

**3. The deviation measure (`camcraft_kernel::mesh_deviation`).**
solidcore-tess gives a per-triangle face (`Mesh::tri_face`; in the plan,
each `FacePlan` with the (u, v) of every node and the surface to evaluate it
on). Every triangle is sampled at 15 barycentric points (4 divisions a
side: corners, quarters and middles of the sides, 3 inside). At each:

- **parametric**: |S(u, v) - T(u, v)|, the plan surface at the interpolated
  (u, v) against the triangle's own point. Two-sided at the sample, and at
  a corner on a shared edge node it is the edge-to-surface gap.
- **analytic**: the closed-form distance from the triangle's point to the
  record face's untrimmed analytic surface (plane, cylinder, cone, sphere,
  torus, a ball patch's sphere). Independent of the planner's own surface
  conversion: a wrong conversion shows here (pinned by
  `the_deviation_measure_sees_a_moved_node_and_a_wrong_surface`).
- **judged** (what the tolerance binds): per face, analytic where the face
  is analytic, else parametric.

Why not parametric everywhere: on a rational spline's circle the parameter
does not run at constant speed along the arc, so the parametric distance
over-reads the sag by a few per cent - 5.058e-3 mm against a true 4.965e-3
on Milling-test at 0.005 mm, 1.05e-2 against 9.86e-3 on sample_ap214 at
0.01 mm. Judged parametrically, both fixtures would be refused although
their triangles are inside the tolerance.

NOT measured, stated: the triangle between the samples (a sampled maximum is
a lower bound of the true one; not proven); spline and revolved faces have
only the parametric reading (none on our fixtures); the trim (a triangle on
the right surface outside its face is caught only by the watertightness of
the welded mesh). Dropped: a Newton projection onto the plan surface for
non-analytic faces (no such face on the fixtures to pin it; a follow-up with
the proven bound, #1029).

**4. The bench (tests/mesh_deviation_bench.rs, kernel release lane,
`#[ignore]`, 1.4 s in release).** Every STEP fixture at 0.005 mm (#689's cap)
and 0.01 mm (the metre file's floor in #864), verdicts pinned. "common" is
one mesher-blind measure for both meshes: the 15 samples of each triangle
against the NEAREST untrimmed analytic face surface of the record (a lower
bound of the distance to the true face). Times are release, this machine;
the solidcore time includes planning, measuring, evaluating and welding.

| File (unit) | tol mm | sc-tess tris | judged | parametric | time | truck tris | truck chord | truck common | truck time |
|---|---|---|---|---|---|---|---|---|---|
| Milling-test (mm) | 0.005 | 6228 | 4.97e-3 | 5.06e-3 | 52 ms | 952 | 0.005 | 4.95e-3 | 4 ms |
| Milling-test (mm) | 0.01 | 5040 | 5.35e-3 | 5.35e-3 | 41 ms | 672 | 0.01 | 9.91e-3 | 3 ms |
| Milling-test-inch | 0.005 | 6248 | 4.97e-3 | 5.06e-3 | 50 ms | 952 | 0.005 | 4.95e-3 | 5 ms |
| Milling-test-inch | 0.01 | 5024 | 5.35e-3 | 5.35e-3 | 40 ms | 672 | 0.01 | 9.91e-3 | 3 ms |
| sample_ap214 (m, 3 solids) | 0.005 | 11894 | 4.95e-3 | 5.17e-3 | 209 ms | 3178 | **0.01** | **9.99e-3** | 24 ms |
| sample_ap214 (m, 3 solids) | 0.01 | 7920 | 9.86e-3 | 1.05e-2 | 146 ms | 3178 | 0.01 | 9.99e-3 | 23 ms |
| box, empty_list, tool_csys_box_iso13399 | both | 12 | 0 | 0 | <1 ms | 12 | - | 0 | 2 ms |
| tool_csys_none / _iso13399 / _degenerate | both | no solid | | | | | | | |

Readings: solidcore-tess holds every fixture inside the asked tolerance
(judged), at 0.993 of it at worst - its own sag refinement aims at the
tolerance itself, not under it. truck's mesh is as close at the same chord
on the mm and inch files, with 6.5x fewer triangles, but open (854 / 3196
open sides, #916); on the metre file truck cannot go under 0.01 mm (its
floor in that unit) and stands 9.99e-3 off - twice the 0.005 cap, the
reduced fidelity #864 flags - where solidcore-tess meets 0.005. The inch
file meshes to a slightly different count (6248 against 6228): the same
solid read through a 25.4 scale, not bit-identical.

**5. Triangle budget (issue point 3), for stage 5.** At 0.005 mm the
machining mesh is 3.7x (sample_ap214) to 6.5x (Milling-test) truck's
triangles. The app's `MAX_FACETS` (30 M) is kept as the seam's own cap, as a
refusal: a solid that truck meshes in more than about 4.5 M triangles may
not fit. Recommendation for #917: keep 30 M, and DROP the app's coarsening
retry loop on this path - a coarser mesh is past the tolerance the gouge
verdict relies on (#689), so a model too big at 0.005 mm is refused by name
(or meshed at a coarser tolerance the user is told about, #864's
reduced-fidelity flag), never coarsened in silence. solidcore-tess's own
memory bounds (`MAX_FACE_GRID`, `MAX_REFINE`, 20 000 nodes an edge) are per
face and per edge, and the seam refuses any one of them that leaves the
tolerance; a whole-body bound is the seam's cap. Memory per triangle in the
seam's output: 12 B indices + 4 B face id + about 12 B of welded position.
Time: about 8 us a triangle in release (planning, measuring and welding).

**6. wasm32 (issue point 4).** `cargo check -p camcraft-kernel --target
wasm32-unknown-unknown` passes with solidcore-tess in the library;
`cargo tree` shows no rayon on wasm32 (the mesher's manifest gates it, and
it then plans faces serially). solidcore-tess uses no clock outside its
tests and spawns no thread of its own. Not run: a wasm32 test execution.

**What upstream would need (for the maintainer; nothing filed upstream).**
A cancellation hook in `solidcore_tess::plan` (it takes no token); a
per-face measured sag in `PlanStats` for EVERY face, not only those a
memory bound limited (so the seam could judge the planner's own measure
instead of re-sampling); and a stated bound (proof or a margin) on the sag
between the planner's sample points, so "within" is a guarantee and not a
sample.

**Cost to change later.** The refusal table is one function
(`judge_stats`); the sampling density one constant (`SAMPLE_DIVISIONS`);
the budget one constant.
