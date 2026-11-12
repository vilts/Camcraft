## #1094 - A from-model stock's source is judged as the part unless the user marks it raw stock only

**Decided (after review of PR #1097).** The program verdict leaves a model out
only when the user said so: a checkbox on the stock page, "The source model is
raw stock only (not judged as the part)", shown for a stock made from a model.
**Off is the default** and every older document's state: every non-fixture
model is judged exactly as before. The mark is one undoable command
(`SetStockSourceOnly`), with an MCP tool (`set_raw_stock_only`) running the same
command, a component on the stock entity (`schema::StockSourceOnly { source }`)
and an optional project-file key (`raw_stock_only`, absent = false).

**The mark is self-validating where it takes effect (third review; the
orchestrator's direction, taken unattended).** A third command (`DeleteFixture`)
was found removing a model without dropping its mark - command-by-command drops
will be missed again by the next model-removing command someone adds. So the
mark carries a fingerprint of the marked model's geometry as the verdict reads
it (`stock_print`: facet count, then every vertex coordinate of the local facets
as `f64` bits in facet order, then the `Location` position and rotation bits;
two independent 64-bit hashes - FNV-1a over the bytes and a splitmix64 fold over
the words - so 128 bits, never a 32-bit hash). `stock_source` honours the mark
ONLY while a model of that name exists, is still the from-model stock's source,
and prints the same now. Any mismatch voids it: the model is judged as the part
and a named note ("the raw-stock-only mark on 'X' no longer matches the model
... and was ignored") rides the gate's visible, non-blocking line (status line,
NC header, script/MCP `not_judged`) and the stock page - never silent, never a
load error. The facet hashes are memoised per revision on the model
(`GeomPrintMemo`: revision, the facets' `Arc` pointer and length), so a
200k-facet model is hashed once per revision, not once per post.
- The same mesh at the same place prints the same across save/load (f64
  round-trips exactly in the file) and undo/redo; an identical re-import of the
  casting under its name keeps the mark - it is the same casting. Moving the
  model, or editing/repairing it without a dropping command, voids the mark with
  the note; the user re-ticks.
- Project file: `raw_stock_print` is saved beside `raw_stock_only`. A file with
  the flag but no print (written by head 56e30159, or by hand) loads unmarked
  with a load note, never as marked-without-check. A build from before this PR
  ignores both keys and judges the model (the safe direction); the print does not
  need to survive any older reader, because no released build reads the flag.
- Dropped: patching each model-removing command (it is what failed three times);
  binding to the entity id (undo of a delete respawns a new entity, and a load
  renumbers everything); reusing `model::geometry_signature` (a 64-bit digest of
  derived features, not of the raw vertices and placement the verdict reads).

**The per-command drops stay as tidiness (second review).** The mark
names its source by name, so a model imported later under a deleted casting's
name must never find it waiting. Chosen mechanism: every command that ends the
model or the stock the mark was given to drops the mark *inside its own undo
step* (`commands::stock_source_only::MarkDrops`, snapshotted at construction):
- `DeleteEntity` / `DeleteFixture` of the source model (or `DeleteEntity` of
  the stock) drops it; undo of the delete restores the model AND the mark,
  redo drops it again;
- `UpdateModel` (re-import in place, and `RepairModel` through it) drops it -
  new geometry was never said to be raw stock; undo restores both;
- `SetStock` - any new stock, another shape or the same model again - drops it;
  only undo brings it back (stock -> box -> back to the casting comes back
  unmarked).
- No model rename command exists (only `RenameOperation`); a future one must
  carry the mark with the model (or drop it) the same way.
- Project load: the mark lands only when its source model is in the file; one
  naming a missing model is read as unmarked (judged) with a load note, not
  refused - the safe direction, and an older file still opens.
Dropped: binding the mark to the entity id or a geometry stamp instead. An
entity id cannot survive undo of a delete (the model respawns as a new entity,
so undo could not restore the mark) nor a project load; a geometry stamp would
keep a mark on an identical re-import, and needs the model loaded before the
stock. Dropping in the commands keeps undo exact and leaves no dangling mark.

Even when on, `stock_source::stock_only_model` keeps it honest:
- it holds only while every stock the program starts from (the own stock of
  each setup the unsuppressed ops machine in; a setup taking what the one
  before leaves is skipped) is a copy of that same model and carries the mark.
- an operation of the document - any setup, suppressed or not - that names the
  model in its source (`Entity`, any `Selection` entity) or in a boundary pick
  of its area controls puts it back in the verdict. Chosen over refusing the
  command while such an op exists: a refusal at tick time cannot stop an op
  added later, the reader-side rule holds whatever order things happen in.
- a mark that leaves no other non-fixture model with a body (facets) refuses
  the post by name ("nothing is judged as the part: ...") - the stricter of
  refuse or warn; an empty model is nothing to judge.

**The hint is the only inference.** A gouge refusal whose worst gouge lies in
the bounds of a program stock's source model that no operation names adds one
sentence saying what the setting MEANS: if that model is the finished part, the
program cuts into it; only if it is the raw casting or blank (the finished part
being its own model) tick the box. It is worded so it cannot read as "tick this
to make the error go away"; a stock made from the part itself still gets it,
because the code cannot tell a part from a casting - that is the user's call.
When a mark is set but an op names the model, the hint says why it did not
hold. It changes no verdict.

**Why explicit, not inferred (orchestrator's direction, taken unattended).**
The first version inferred "only the stock's source" from three clauses (the
source of every program stock, named by no op, while some op names another
model). The review reproduced a real 5 mm gouge into the finished part posting
with an empty gate: a stock made from the part itself plus any unrelated model
an op touches (live or suppressed) met the clauses, and boundary picks were not
seen. Any rule that guesses which model is the part can be met by a job it did
not foresee - the suggested envelope test is defeated by a deburr or engrave
job on a finished part with a small reference body inside its envelope - and a
wrong guess posts a gouge, the worst failure this project has. Per the rule
that a CAM choice is a user parameter with a safe default, the user decides.

**Considered and dropped.**
- *The three-clause inference* (above) - removed.
- *Patching the inference clause by clause* (unsuppressed ops only, picks,
  an envelope containment test) - each patch narrows the hole, none closes it.
- *Refusing the mark while an op names the model* - kept the reader-side rule
  instead (see above).
- *A named warning instead of a refusal when nothing is left to judge* - the
  stricter reading was taken.

**Project file.** No version bump: the key is additive and optional, and an
older reader that drops it judges the casting - a false refusal at worst,
never a missed gouge. An unmarked document saves byte-for-byte as before.

**Other readers of "every non-fixture model", unchanged.**
- `model_fidelity::verdict_tol` (#864/#911 model-excess law) still reads every
  non-fixture model, marked or not: a coarse casting can still tighten or refuse
  the verdict - a false refusal at worst.
- Per-op collision, rapid and cross-drill laws read the op's own source or the
  stock (`operation::job`), never the document part set.
- Rest material reads the stock walk, not the part set.
- Alignment, picks, fixture clash, `part_centre`, the tree and the GPU part
  list are placement and display, not judgement.

**Cost to change later.** One module (`crates/app/src/stock_source.rs`), the
command, two verdict call sites and the gate clause in `posting_sim.rs`.
