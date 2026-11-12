## #1091 - The rapid law reads the stock's top under the dive, not its highest point

**What was decided**

1. **Reproduced with stock only.** A casting set as "from model" stock,
   solid to Z 20 over a 16 mm hole's mouth with a boss to Z 30 18 mm
   away, then removed from the tree (the stock keeps its copy) so the
   program verdict does not judge it as a part. A part-sourced Bore
   (part imported) and a stock-sourced Bore (no part at all) both
   rapid to Z 22 and were convicted `RapidInStock { at: 0 }`.
2. **Re-read the convicted dive, do not change the column law.** A new
   sim reading, `verify::acquit_by_stock_top`, works the way
   `acquit_by_prior` does: it only removes a conviction, and only from
   an upright vertical rapid down in a column mode. The column law
   (`rapids.rs`, `Rest::surface_before`) and its flat cap are
   unchanged, so every other conviction - lateral rapids, the posed and
   radial readings, the solid law - keeps its verdict.
3. **The bound is exact and conservative.** The re-reading takes the
   highest vertex of every stock or part facet whose plan comes within
   the tool's disc. On a closed mesh that is never below the material
   standing anywhere in the disc. It reads each band of the stack
   separately: the flute at its full radius (not `r - gouge_tol`, and
   with no ball lift) from the tip, the shank from the flute's top, and
   the holder from the shank's top. The dive is acquitted only when
   every band ends above its disc's top, less `Z_TOL`. A boss reaching
   under the rim, or under a wide holder over a stubby tool, still
   convicts. It fails closed: a non-finite vertex, a non-finite stack
   number, no judged stock (the op's region stood in for it), or a
   flute disc that meets no facet all leave the conviction standing.
4. **Only a proven closed shell is read (PR #1098 review).** The bound
   holds only for a closed surface. In an open mesh (a boss without its
   top facets) the facets over the dive are missing, and the dive was
   acquitted into stock that is there. So every solid must pass
   `stock_top_shell::closed_shell`, or the conviction stands. The gate
   checks:
   - the solid has facets, and every vertex is finite;
   - no facet is degenerate in 3-D (two equal vertices, or zero area);
   - every undirected edge (endpoints matched exactly, bit for bit) is
     used by **exactly two facets, once in each direction**. That
     refuses a hole, a non-manifold edge, a duplicated facet and a
     flipped neighbour;
   - the facets form one edge-connected shell;
   - the signed volume is positive (outward-wound).

   It is computed once per solid per operation, in one O(F) pass with a
   hash map of edges, never per rapid. It is stricter than the proof
   needs on purpose: an inside-out shell, or several shells handed in
   as one solid, are refused even though the bound would hold for them,
   because a refusal only keeps a conviction.

   **What the acquittal now relies on:**
   - The solids are closed surfaces. The gate checks this exactly.
   - A point enclosed by a closed surface sends its upward ray across
     it, so a facet covering its plan stands above it. This ray argument
     holds even for a self-intersecting surface, and the gate does not
     test self-intersection.
   - The meshes are the stock and part the program really meets. The
     whole verifier rests on this too.
   - The tool stack and the model excess are read as item 5 says.

   No cheaper independent cross-check proves an empty column.
5. **Band stack and model excess (merged main after #971, #1064,
   #957/#993).**
   - *Band stack:* the re-reading is never narrower than the laws read
     the stack.
     - The flute zone is read at `tool.radius`.
     - The shank zone is read at the **stated** `shank_radius`. #957
       made the laws read a drawn neck's narrower bands there, and
       `NeckBands` are never wider than the stated shank, so reading the
       stated shank is the conservative side.
     - Above the shank's top, it reads `max(holder_radius,
       shank_radius)`, the bound `BodyBands` puts on a holder body
       there, and on upward without end.
     - Every raw number is checked as finite and non-negative before
       the `max`. `f64::max` drops a NaN, and the unit test caught a NaN
       holder radius slipping through that way.
   - *Excess:* a coarse model's real surface may stand up to
     `model_excess` (`e`) outside its facets, sideways and upward. Every
     band's disc grows by `e`, and the tip is compared against the
     facets' top plus `e`, so a dive that clears the meshed material by
     less than the excess stays convicted. A non-finite excess keeps
     every conviction, and a negative excess counts as zero (as
     `excess_floor` reads it). I chose this over "keep every conviction
     when an excess is present" because it stays exact at full fidelity
     and stays conservative on a coarse model. The excess is the op's
     own model excess, applied to the stock and part solids alike. A
     stock mesh coarser than its source has no excess of its own here,
     and the column law has none either.
6. **Which stocks can be acquitted (third review, #1056).** A
   from-model stock's chord is read only through its source model
   (`model_fidelity::coarse_source`, by name). Once the source is
   deleted, its excess reads 0, and a coarse casting's dive cleared the
   mesh by less than the real surface may stand above it, then posted.
   The rule, `model_fidelity::stock_fidelity_known(setup)`, decides
   which stock is passed to the re-reading:
   - **Box or cylinder stock, or no stock entity:** acquittable. There
     is no mesh to be coarse.
   - **From-model stock whose source is provably the model it was
     copied from:** acquittable. Its `ReducedFidelity` record gives the
     chord, and no record means an exact import, because the importer
     writes the record for every coarse import.
   - **Any other from-model stock:** never acquitted. Both sites pass no
     stock, so every conviction stands. That covers a gone source, a
     different model under the old name, a source re-meshed without its
     copy, and a stock or source with no facets.

   *Proof of the source (fourth review), option (a):* the stock's own
   facets are compared with the named model's current local facets.
   They must be the same `Arc` (the copy `SetStock` took, untouched), or
   the same count with every coordinate equal bit for bit. Of the
   reviewer's two options, I chose (a) over (b), a fingerprint stored
   with the stock:
   - It needs no stored state, no schema or project-file field, and no
     "old file" rule.
   - It compares the geometry the chord describes, not a digest of it.
   - It is an exact equality, not a 128-bit hash, and it adds no second
     fingerprint derivation (`stock_print` stays the only one).
   - Cost: `Arc::ptr_eq` while the copy is untouched. Otherwise (after a
     project load, the two lists are separate allocations) one O(F)
     pass, once per operation generation, never per rapid.

   Placement is not compared on purpose. The chord is a property of the
   mesh, and a moved source carries the same mesh and record
   (`MoveModel` carries a from-model stock with it). A re-import in
   place (`UpdateModel`) trades the copy and the fidelity record
   together, so it stays known. No command renames a model (only
   `RenameOperation`, which relabels an operation), so a rename cannot
   reach the name.

   One remaining case: an identical mesh that carries a different
   fidelity record (the same facets imported once coarse-flagged and
   once exact) would be read through the present model's record. The
   facets are identical, so the mesh the verdict reads is the same.
   Only the record's claim about the real surface differs, and the
   importer never writes two records for one mesh.
   - **Carried stock:** `carried_stock::judged` always judges a box (the
     declared box, or the box enclosing the carried body), and that box
     reads no lower than the flat cap. The flag is still read from the
     setup's declared stock, so a carried case over a declared
     from-model stock with a lost source is refused too.
   - **Part models:** they are in the tree whenever they are the source.
     A missing record means known exact, so they need no guard.

   When #1127 (#1056) merges, a from-model stock carries its own chord.
   The guard (`// #1056` in `model_fidelity.rs`) can then read that
   chord instead of requiring the source. The pinning fixtures now keep
   the casting in the tree, marked raw stock only (#1094 / PR #1097,
   `SetStockSourceOnly`), with the part beside it, instead of deleting
   the casting.
7. **The solids are `judged_stock` (the stock the column law judged,
   #1066: the setup's or carried stock, or the stock itself for a
   stock-sourced Bore/Thread) and the op's own material.** Both are
   read in the path's frame: world at the upright site, and the
   plane's local facets at the 3+2 site. Thread's descent is a
   vertical rapid, so it is re-read the same way.
   `acquit_by_prior` still runs afterwards, for stock the program ahead
   removed.
8. **Pinned by** `crates/app/tests/stepped_stock_rapid.rs` (the dive
   over the low part passes and posts. With the boss over the hole, or
   with its edge 1.5 mm beside the dive's centre under the 4 mm rim, the
   dive is convicted and the post is blocked by the rapid law. Each
   case runs for both sources). The review's open boss (its top facets
   missing, over the hole) stays convicted and blocks the post, for both
   sources; this test fails with the gate disabled. Also pinned by
   `sim::verify::stock_top`'s unit tests: the holder band; the part
   counting too; an open shell, a non-manifold or duplicated edge, a
   degenerate facet, an inside-out or split solid, an empty solid and a
   non-finite vertex all keep the conviction; and the posed, lateral and
   radial/revolved/rotary exclusions. A dive 0.3 mm over the meshed
   step, or with its rim 0.3 mm short of the boss's wall, is acquitted
   at full fidelity and stays convicted with 0.5 mm of excess
   (`a_coarse_models_excess_is_read_up_and_sideways`). End to end:
   - a coarse casting (chord 2.5) whose source is deleted, and an exact
     one whose source is deleted, both stay convicted, and the post is
     blocked by the rapid law
     (`a_coarse_stock_whose_source_is_gone_is_never_acquitted`, which
     fails with the guard off);
   - an exact model imported under the deleted coarse casting's name
     stays convicted, and the post is blocked
     (`an_exact_model_imported_under_the_old_name_proves_nothing`);
   - the guard follows the mesh, not the name: a copy left untouched, a
     moved source and an in-place re-import are known, and a source
     re-meshed behind the copy is not
     (`the_guard_follows_the_mesh_not_the_name`). Both of these tests
     fail with a name-only guard;
   - with the source kept, chord 2.5 is convicted and chord 1.0 is
     cleared (`a_coarse_stock_whose_source_stands_reads_its_excess`);
   - box stocks are pinned unchanged by `ladder_entry_height`. The `ladder_entry_height` pins and the #96 pins are
   unchanged.

**Why**

The Prime Directive asks for correct output or a visible refusal. A
refusal of a legal program is not a gouge, but it leaves a user with a
casting no way to post. Loosening the law itself (a per-sample local
cap inside `surface_before`) would move every column verdict and lean
on the footprint's sampling bound. A re-reading that can only acquit,
and only on an exact upper bound of the material, cannot let a dive
into material pass.

**Considered and dropped**

- *A local cap per footprint sample inside `Rest::surface_before`.*
  This would also cover lateral rapids, but it inherits the footprint's
  sampling bound (a boss sliver between samples could pass) and it
  changes every column law that reads the cap.
- *The stock dexel (`stock_grid`) as the prior even when nothing is
  ahead.* The grid samples cell centres, so a boss edge between centres
  reads low. That is not an upper bound.
- *A new `VerifyContext` field.* It would touch about 50 construction
  sites for a reading that only needs the dive's convictions.

**Left as it is (follow-ups filed)**

- Lateral rapids under the clearance plane over a stepped stock still
  read the flat cap. They over-report and do not hide anything (#1095).
- A stock-from-model's source model left in the tree is judged as a
  part by the program verdict, so the post reports a gouge. The
  reproduction removes the casting to keep that out of the way (#1094).
- A model that is neither the op's source nor inside the stock (a
  second part poking up through a low step) is not in the solids. The
  flat cap did not read it there either, since `stock_top_z` is the
  stock's and the source's top.

**What it would cost to change later**

- Lateral rapids could take the same re-reading over the swept capsule.
  That is a second reading in `stock_top.rs`, and the rest of the law
  stays unchanged.
- If verify ever gets a true stock model (#928), this re-reading is
  replaced by the column law reading that model directly.
