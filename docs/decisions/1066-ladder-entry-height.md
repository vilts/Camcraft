## #1066 - A stock-sourced Bore's entry rapid under a taller block is judged

**What was decided**

1. **Reproduced, and raised to P0.** The planar ladder starts its first
   descent at the top of the cut plus the plunge height (`descent_from`),
   and rapids down to that plane. Pocket and Contour take the top from
   their Top height, and Bore takes it from the hole's `entry_z`. With the
   block standing 5 mm over the mouth, all three rapid 3 mm into uncut
   stock. Pocket, Contour and a part-sourced Bore were already convicted
   by the rapid law (#96, `RapidInStock`), and the post gate blocked them.
   A **Bore whose source is the stock** passed silently and posted.
2. **Fix the law's reading, not the ladder.** The column law asks
   `Rest::in_stock`, which falls back to the op's region when
   `VerifyContext::stock` is `None`, and a stock-sourced op leaves it
   `None`. For Bore and Thread that region is the stock footprint with
   the holes' rims punched out. `material_source::judged_stock` now hands
   verify the stock's own facets for a stock-sourced Bore or Thread, in
   both the upright and the 3+2 reading. The ladder and its generated
   path are unchanged. The dive is named and the post refuses it, the
   same way Pocket and Contour already are, and a face ahead to the
   mouth still acquits it (`acquit_by_prior`).
3. **Pinned by `crates/app/tests/ladder_entry_height.rs`** (four tests,
   each over Pocket, Bore and Contour): convicted and blocked under a
   block 5 mm over the mouth; silent when the block is 1.5 mm over the
   mouth (inside the plunge height, fed through); silent with stock to
   the mouth; acquitted by a face ahead.

**Why**

The Prime Directive asks for correct output or a visible refusal. The
law already gave that on every other source and kind. The gap was one
fallback that read "where the walls stand" as "where the material is".
Mending the fallback catches the case the same way the siblings catch
it, with the same message, and keeps the prior-stock acquittal for
free.

**Considered and dropped**

- *Raise the ladder's top to the stock as it stands (prior stock / stock
  top)*: this would change every 2D path's entry for a case the law
  already refuses on Pocket and Contour. It would also hide a Top height
  the user set too low; #104 (`top_stock`) explains that case rather
  than silently moving it.
- *A Bore-arm refusal mirroring Drill's R-plane law*: it duplicates the
  rapid law and the prior-stock read, for one kind only.
- *Set `stock_facets` for every stock-sourced op in `job.rs`*: that
  field feeds a dozen other laws (solid, plunge, carried stock), which
  is more blast radius than the gap needs. Every other stock-sourced
  kind's region already is the stock's footprint (or an area-controlled
  part of it), so it keeps today's reading byte for byte.

**What it would cost to change later**

- Another kind whose region punches material out of the stock (a new
  hole-list ride) joins the `matches!` in `judged_stock`.
- If the ladder is ever made to start over the stock as it stands, the
  tests flip from "convicted" to "clean" on the 5 mm case. The other
  three cases hold as they are.
