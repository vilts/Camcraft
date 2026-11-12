## #916 - M12.1 stage 4: STEP import through solidcore-step behind the kernel seam, benched against truck

Bench only: `crates/app` still imports STEP through truck-stepio; truck and
ruststep stay. Everything is inside `crates/kernel`; no other workspace
crate gained a dependency.

**1. The pin.** `crates/kernel` takes `solidcore`, `solidcore-step` and
`solidcore-convert` from `https://github.com/NeoTech/solidcore` at
`rev = "55d0e7638b9e1ebbf91ab8c10657e595c114ecda"` (the fork's tag
`camcraft-pin-2026-11-11`, upstream main of 2026-11-11 19:30 UTC), as the
maintainer decided on the issue. `solidcore-ops` and `solidcore-record` come
with them (path dependencies of `solidcore-step`); `solidcore-tess` at the
same rev is a dev-dependency for the mesh bench. `Cargo.lock`: six git
packages (`solidcore`, `-convert`, `-ops`, `-record`, `-step`, `-tess`) in
place of the one crates.io `solidcore` (552 -> 557 packages), no new
crates.io package (`serde`, `serde_json`, `spade`, `rayon` were already
locked). An earlier wording of this entry, of the commit message and of the
PR said five and 556: that count was taken before the mesh bench's
dev-dependency was added. The starting pin was kept: every kernel suite is green on
it, so no nearer commit was needed.

Moving the pin (the maintainer's procedure): `gh repo sync
NeoTech/solidcore`, pick the commit, tag it `camcraft-pin-<date>` in the
fork, change every `rev` in `crates/kernel/Cargo.toml` together, run the
kernel suites and re-triage whatever pinned verdict moved.

**2. What the pin changed against 1.0.1.** The package still says
`version = "1.0.1"`, but the commit is 2973 commits past the
`solidcore-v1.0.1` tag (628 files of `crates/solidcore/src` changed). The
seam's library code compiled unchanged. Three differences reached the
suites, each decided:

- `Brep` is no longer `Clone`. Two test helpers cloned one; they now copy
  with the kernel's `brep_copy` (corrupt_input.rs) or measure in place
  (import_corpus.rs). No library change.
- A NaN offset distance is now refused by the port (`Err`); 1.0.1 returned
  an unchanged copy. The pinned port verdict is re-triaged to `Err`. The
  seam's own guard already refused it first, so no behaviour changed.
- The native reader now refuses a blank file, plain text, binary noise and
  a header with no body itself (`Err failed`); 1.0.1 returned `Ok` with an
  empty B-rep and only the seam's heal guard refused them. Four rows of the
  pinned table moved in both columns; all four were findings against the
  port in decision 27 and are closed by the pin.

All 21 default-lane tests and the 2 heavy-lane tests of stages 1-3 pass on
the pin with those edits.

**3. The importer (`camcraft_kernel::solidcore_step`).** `read_step` gives
the whole picture of a file (solids, refused items with stage and reason,
length units, notes); `import_step` hands out a model only when EVERY solid
item came through. Dropped: importing the solids that read and noting the
rest - a part with a body missing is a toolpath through that body.

- Size bound: 64 MiB (`MAX_STEP_BYTES`, the native reader's bound; the
  reader's own cap is 512 MiB). The reader caps entity, record, parameter,
  face, loop and solid counts itself.
- Time bound: the caller's `CancelToken`. It is polled by the Part 21
  parser, between stages and solids, and inside the kernel's loops (the
  token is installed on the kernel for the call and removed after). The
  result is `KernelError::TimedOut` (a new variant). Dropped: a deadline
  from `std::time::Instant` inside the library - it panics on
  wasm32-unknown-unknown, and the crate must build there. Known gap: the
  reader's topology pass (`read_solids`) has no cancellation hook at the
  pin; it runs to its end, bounded by the caps (#939).
- A panic in the reader or the kernel is caught and becomes
  `KernelError::Failed`; after a panic or a time-out the kernel state is
  replaced by a fresh one (upstream's own rule after a cancelled call).
- The record is built with `to_kernel_body(.., trusted = false)`: the
  kernel heals it as it builds it. Then the seam's stage-3 laws apply
  unchanged: integrity check, closed manifold solid, positive finite
  volume and area, vertex-edge gap at or under `HEAL_GAP_LIMIT` (1e-3 mm),
  no edge without a 3D curve. A sheet (open shells) is refused by name.
- Units: every solid is in millimetres. The file's stated length units are
  returned (`[25.4]` for an inch file). A length unit the reader cannot
  convert is REFUSED by the seam: the reader alone reads it as millimetres
  without a word. A file stating two different units gets a note, not a
  refusal: the reader reads each solid in its own context's unit
  (crates/app refuses such a file today; that difference is for stage 5).

**4. Findings against the port at the pin (pinned, not papered over).**

- `to_kernel_body` hands out a B-rep whose cached `Face::shell` is behind
  its face-use links, on every fixture; the port's integrity check refuses
  it. The seam calls `Brep::refresh_cached_topology()` (public; changes the
  cached fields only) after the build. To report upstream (#939).
- The port's "tight" bounding box is wrong for a trimmed cylinder face:
  Milling-test.step is a 60 mm disc with a flat at Y = 19.719 and the box
  says Y max = 30.0. Volume and area are right. See the table. Nothing may
  size stock or place a part from `StepSolid::measure`'s box until this is
  understood (#939).
- Two solid items on one shell import as two coincident solids. That is
  what the file states; the importer does not look for coincidence.

**5. The bench (tests/step_agreement.rs).** Every `.step` under
`testdata/models`, both importers, verdicts pinned; a fixture with no row
fails. Tolerances: relative volume and area 1e-3 (truck's numbers are a
mesh's at the app's chord, 0.005 mm, 0.01 mm for the metre file where
truck's floor binds); box corners two chords. It runs in about a second in
release, so it stays in the default lane; no heavy lane was needed.

| File | Unit (mm) | Importers | solidcore exact V / A (mm) | truck mesh dV, dA | solidcore mesh dV, dA |
|---|---|---|---|---|---|
| box.step | 1 | agree | 1 / 6 | 0, 0 | 0, 0 |
| empty_list.step (#887) | 1 | agree | 1 / 6 | 0, 0 | 0, 0 |
| tool_csys_box_iso13399.step | 1 | agree | 16000 / 4000 | 0, 0 | 0, 0 |
| sample_ap214.step (3 solids) | 1000 | agree | 17152.096 / 13877.096 | -1.5e-4, -1.9e-4 | -3.1e-5, -4.5e-5 |
| Milling-test.step | 1 | DISAGREE: bounding box | 52808.744 / 11143.378 | -1.2e-4, -1.2e-4 | -5.6e-5, -7.5e-5 |
| Milling-test-inch.step | 25.4 | DISAGREE: bounding box | 52808.744 / 11143.378 | -1.2e-4, -1.2e-4 | -5.6e-5, -7.5e-5 |
| tool_csys_none / _iso13399 / _degenerate | 1 | both refuse (no solid) | - | - | - |

The disagreement: solidcore's box of Milling-test is
[-30, -30, -5]..[30, 30, 20]; truck's mesh and solidcore's own mesh both
stop at Y = 19.719, as the file's vertices do. solidcore is the wrong side.

**5a. The corrupt corpus (tests/step_import.rs).** 19 files made from
box.step: blank, foreign text, noise, header only, no entities, three
truncations, a parameter list nested 100000 deep, a dangling face, a solid
that names itself, a plane placed by itself, a face dropped, a vertex moved
0.5 mm, a vertex at 1e300, a one-coordinate point, an unconvertible length
unit, a solid stated twice. Each runs under a 60 s bound that cancels the
import. No panic, no hang. solidcore refuses 17 by name and imports 2 (the
one-coordinate point is the origin, which ISO 10303-42 allows; the solid
stated twice is two solids). truck-stepio, read the way the bench reads it
(no app-level checks after the mesh), refuses the 10 that break the Part 21
syntax and answers `Ok` for the other 9 - five of them with an open mesh (a
dropped, dangling or unreadable face; a moved or far-away vertex). That
column is a finding about the path the app uses today, recorded, not
chased here.

**6. Tessellation (issue point 4).** solidcore now has a mesher,
`solidcore-tess` (the workspace's "one mesher"; `plan(record, tolerance,
size)` then `TessPlan::evaluate()`), which meshes the STEP reader's B-rep
record directly at a chord tolerance in millimetres. Benched at the app's
0.005 mm cap on the same files:

- watertight on every fixture when vertices are welded by exact position
  (0 open triangle sides; edges are sampled once and shared). truck's
  per-face meshes are not: 854 open sides on Milling-test, 3196 on
  sample_ap214 by the same count;
- closer to the exact volume and area than truck's (table), with more
  triangles (6228 against 952 on Milling-test, 11894 against 3178 on
  sample_ap214);
- no face left out, none reported over tolerance, no lenient fill;
- its bounding box matches truck's mesh within two chords on every file.

Two cautions for stage 5. Its own docs call it a DISPLAY mesher ("a
display mesh decides nothing a model depends on: it is drawn, never
measured"), and it reports faces it could not mesh in `PlanStats` instead
of failing - a machining import must turn any `unmeshed`, `lenient`,
`over_tolerance` or `edges_capped` entry into a refusal. And the bench
measures volume, area, box and watertightness, not the largest distance
from mesh to surface; that deviation measure is still owed (#940). The caps of
#689 / #864 map onto `MeshTolerance::chord(mm)` directly: the record is
already in millimetres, so no per-unit conversion of the cap is needed.

**7. wasm32 (issue point 5).** `cargo check -p camcraft-kernel --target
wasm32-unknown-unknown` passes with the STEP importer in the library. The
importer uses no clock and no thread. Not run: a wasm32 test execution.

**Cost to change later.** The pin is three `rev` strings (four with the
dev-dependency). The importer's laws are one function (`build`); the
bench's tolerances are two constants.
