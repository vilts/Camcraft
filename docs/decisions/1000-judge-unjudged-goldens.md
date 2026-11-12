## #1000 - the six UNJUDGED posted goldens, judged by their kinds' gates

**What was decided.** Five of the six posted goldens #862 left in
`solid_law_goldens::UNJUDGED` are now judged by the gate their kind's post
runs, in a new harness `crates/app/tests/posted_gates/mod.rs`. All five
verify clean, so no golden moved and none was regenerated:

- `slot_fullwidth_centerline`, `slot_wide_channel_rows` (Slot) and
  `trochoid_slot_small` (TrochoidalSlot) go through `verify` in their
  kind's mode (`Mode::Area`). The region is the one the operation layer
  builds, `operation::region_of` on a real operation of that kind (the
  circumscribed stadium of `a`, `b`, `width`), with no model, because
  `depth_law_of` gives these kinds `DepthLaw::None`.
- `trace_plate_with_hole` (Trace) goes through `verify` in `Mode::Engrave`.
  Its region is `region_of` for a Trace over the finished plate (40 x 20,
  8 x 8 through hole), which is the plate's section.
- `plasma_plate_with_holes` goes through the plasma laws
  (`verify::plasma::judge` on `PlasmaProgram::from_toolpath` of the CL
  golden), as `posting_plasma::plasma_clause` runs them. It has no findings
  at all: no refusals and no warnings. Its `_mm` posted twin posts the same
  CL, so it is covered by the same verdict.

Each program is first proved byte-identical to its CL golden, the way
`verdicts()` does it for the depth-law goldens. `the_slot_gate_bites`
proves the harness isn't vacuous: the wide slot's program, judged against
a stadium 2 mm narrower, gouges. The guard now checks the blocking
findings of all three gate families: depth laws, region laws, and plasma
refusals. Plasma warnings never block a post, so the guard doesn't count
them. The clean plasma verdict is pinned separately as an empty findings
list.

`arc_planes` stays in `UNJUDGED`, now with a precise reason. It is a
hand-built program in `arc_planes.rs` with no operation kind, part, region
or tool record, so no user's post gate can judge it. The gap is filed as
#1093: mark synthetic posting fixtures in the golden itself and drop the
by-name list.

**Why.** "The gate the user's post would run" is the op-level verify that
generation runs and that posting blocks on. For these kinds that means the
region laws without a model. For plasma it means the plasma clause. Using
`region_of` instead of copying the stadium math means the test reads the
same region the app builds, so a future change to the stadium (as in #510)
can't leave the harness judging a different shape.

**Considered and dropped.**
- Running the full app pipeline (`GenerateOperation`) and comparing its
  cached warnings. The op-generated path is not byte-identical to the
  golden (the goldens call the generators directly), so it would judge a
  different program.
- Putting the harness in `tests/common/`. Every app test binary compiles
  `common`, so the change would have made the quick gate run all ~250 app
  suites. It sits in a support directory (`tests/posted_gates/`, like
  `area_support/`) that only `solid_law_goldens` declares.
- Making `operation::stadium` public. It would need a src change, and
  `region_of` is already public and more faithful.
- Passing a stock region. The existing harness passes `stock: None`, so
  the region stands in for the footprint. Every rapid in these goldens is
  at clearance, so a stock box changes nothing here.
- Trace tool: a D3 end mill (`CENTERLINE` allows end mills). The golden
  records no tool. `Mode::Engrave` stands the gouge law down, so the
  radius barely matters. Engrave mode also skips the region (boundary) law,
  so a trace that strays off its own boundary would not be convicted by this
  check. That is the same gate the app runs for a Trace operation. The guard
  proves the golden passes the user's gate, not that the gate is complete for
  Trace (review of PR #1096).
- Giving `arc_planes` a refused-program header. It isn't refused; it just
  has no gate.

**Cost to change later.** Low. The harness is test-only. #1093 removes the
last `UNJUDGED` entry.
