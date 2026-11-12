## #864 - a STEP model imported coarser than promised: flag it, tighten its gouge verdicts, refuse past that

**What was decided.** The maintainer's three-part decision on #864, built as
follows.

1. *Flag.* The STEP importer reports the chord error of the mesh it kept
   (`step::parse_with_chord`; the refinement schedule lives in
   `step_refine`, which keeps the tolerance of the pass it kept, not of a
   pass it threw away). A value above the 0.005 mm cap lands on the model as
   the component `schema::ReducedFidelity { chord_mm }`, through
   `ImportModel`, `UpdateModel`, `AddFixtureModel` and `ImportFixture`.
   It is written to the project file as `coarse_mm` on that entity only.
   The model page, the import status line, MCP `import_model` and MCP
   `get_job` show it. Models that met the promise, STL and OBJ meshes, and
   every model of an older project carry nothing and read as full fidelity.
2. *Tighten while honest.* `VerifyContext::model_excess` (achieved error
   less the cap) is a separate field, never mixed into `gouge_tol`, which is
   also the budget moves are read into chords at. The gouge laws subtract it
   from their tolerance while a positive tolerance is left: the section law
   (b), the solid law (b2), the envelope law's gouge half (f),
   `generate::contain_into`, the lathe's axial hole verdict, and the program
   sim's `deviate` tolerance (`model_fidelity::verdict_tol`).
3. *Refuse past that.* Once the excess uses a whole tolerance the laws read
   untightened and the program gate (`posting::verification_report`,
   `model_fidelity::gate_problems`) refuses each operation judged against
   that model by name: "OP: model 'part' was imported too coarsely to
   verify; re-import with a larger budget (surfaces are approximated to
   within 0.030 mm)". The program sim refuses to judge too, so no gouge line
   is read off such a mesh.

**Why.** A facet of a curved wall is a chord of the true surface. On a convex
wall it stands inside the part, so a cut up to the chord error deep into the
real wall reads as clean. Subtracting the excess makes the worst case on a
coarse model equal to the worst case on a model that met the promise. A
tolerance at or below zero would convict every exactly finished flat face,
which carries no chord error at all; that is a misleading verdict, so the
honest answer there is a refusal that says what to do.

**Decisions taken unattended.**

- *The refusal threshold is "nothing positive left", per law.* An operation's
  floor is the smaller of the 0.01 mm gouge tolerance and its own envelope
  tolerance (`fidelity_law::floor_of`). With the fixed 0.01 mm laws this
  means a mesh at 0.015 mm or coarser is refused. A margin above zero (to
  spare a model at 0.0149 mm from near-certain false gouges on concave
  walls) was considered and dropped: the decision text says "while the floor
  stays positive", and false gouges are the safe side.
- *Kinds whose gouge laws do not read the excess yet are refused on any
  reduced-fidelity model* (`fidelity_law::excess_aware`): the cone and
  engrave rides (2D chamfer, deburr, trace, engrave), probe, the cross drill,
  every multi-axis and rotary kind, turning, and plasma. Their verdicts live
  in generator construction laws and in the motion crate's meridian, stamp
  and radial-field scans. A refusal is the only verdict that cannot be
  looser; tightening those laws is owed and filed as a follow-up.
- *Collision laws are not gouge laws and are not tightened*: shank and holder
  reach (c, c3), rapids in stock (d), the turning rapid, bar and holder
  scans. They judge clearance against rest material at millimetre scale.
  While an operation still posts, the excess is under 0.01 mm. Filed with
  the follow-up, said in the PR.
- *Fixtures that came through the same importer.* A reduced-fidelity fixture
  is judged by its bounding box grown by its chord error on every side, and
  its mesh is not offered to the mesh law, which only ever acquits a box hit
  (`fixture::steel_box`, `fixture::meshes_for`). Stricter only; a
  full-fidelity fixture is untouched.
- *A stock copied from a model* reads the excess of the model it copies
  (same facets). A box or cylinder stock has none.
- *A repair keeps the flag* (`UpdateModel::keeping_fidelity`): welding and
  hole filling make the facets no truer to the surface. Any other update
  replaces the flag with the new mesh's. Undo restores it; deleting and
  undoing a model brings it back flagged.
- *A metre-unit STEP file* stops at the kernel's least tolerance, 1e-5 m =
  0.01 mm (#839). It now gets the same flag as an over-budget model, with
  0.005 mm of tolerance left, so its mill verdicts are tightened and it
  still posts. Scaling the geometry to millimetres before meshing would fix
  it at the root; it means transforming the kernel's shell before
  tessellation and was not small or clearly safe, so it is owed.
- *The flag threshold has a nanometre of slack* (`step::coarse`). An inch
  file's cap is divided into inches and multiplied back; that round trip
  must not flag a mesh that met the promise.
- *`VerifyContext` gained `Default`* as a struct-update base, so the new
  field is not spelled out at the 40-odd test construction sites and the
  next one will not touch them. The two production sites in `generate.rs`
  state `model_excess` explicitly.
- *"Re-import with a larger budget" is the maintainer's wording, but the
  budget is not yet a user setting* (`MAX_FACETS` is a constant). The
  refusal is still true and actionable in part (a smaller or simpler export,
  millimetre units). A user-facing budget is filed with the follow-up.

**Considered and dropped.**
- Logging only: the status quo the issue rejects.
- Recording the error on every STEP model: models that fit the budget would
  gain a project-file key and change saved bytes.
- Folding the excess into `gouge_tol`: it is also the chord-reading budget,
  and the reading must not get finer or coarser with the model.
- Curvature-aware excess, or offsetting the mesh outward: geometry
  algorithms, beyond the decision; the refusal covers the case they would.
- Refusing every reduced-fidelity model outright: simpler and safe, but it
  would make every metre-unit STEP file unpostable.

**Cost to change later.** Low. The law is one function
(`verify::excess_floor`) read by both the laws and the gate; the per-kind
table is `operation::fidelity_law`; making a kind excess-aware is threading
`material.excess` into its law and adding it to `excess_aware`.
