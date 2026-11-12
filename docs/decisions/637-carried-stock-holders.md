## #637 - carried stock on the field engine: the four questions answered, and the holders (step E3)

Steps A-E2c (#646, #649, #651, #656, #782, #784, #817, #831) shipped the
field's side of a carry in v0.18.0: the view, the two-sided column read,
the bake, the cut after a flip. This entry settles the issue's four open
questions against that code and records how the app holders moved.

**What was decided.**

1. **Representation: a transformed query frame, never a re-sample - and
   the bake is a re-index, not a re-sample either.** A signed distance is
   invariant under rigid motion, so the carried solid is the same field
   read through the turn (`CarriedField`). Under the eight lattice turns a
   vertical column of the new frame is a vertical column of the field's
   own, so a verdict column is one exact two-sided read (`column_span`).
   To cut after a carry the turn's signed permutation is moved into the
   bricks (`SdfField::turned`): the lattice is origin-locked, so cell
   centres land on cell centres and every stored value is copied. It is
   exact enough for the verdicts: the sim lane `carry_engine_agreement`
   holds tops and floors to 1e-6 per column through all eight turns, under
   `FIELD_VERDICT_EPS` (1e-5), and no verdict tolerance moves.
   One thing the app adds: the document's stock lattice starts at the
   stock's corner, not at a multiple of the cell. The field arm therefore
   builds its solid in the lattice's own frame (corner at the origin) and
   keeps the corner as the view's translation, so every verdict column
   sits on a field cell centre from the first cut on.
2. **The floor: the lower end of the column's one material interval.**
   A closed solid has no floor array; `column_span` reads `[floor, top]`
   from the shell crossings and the cut records (the monotone law for
   spanning tools, the up-march after a flip), and refuses by name where a
   bounded tool cut near the floor. A column cut through holds nothing.
   The column engine keeps the tool's own depth there (`top < floor`,
   both finite) and the verdict still convicts it; the read-back used to
   write "air" (`-inf`), which a verdict reads as "nothing to judge" - a
   false clear. Decided: **a cut-through column reads its top at the
   stock's own floor and its floor at the stock's own top** (both finite,
   `top < floor`): the column stays empty, and where the part stands the
   verdict convicts it from the top, and from below once the floor is
   judged. The field does not know how far below the stock's floor the
   tool travelled, so the gouge DEPTH there is the stock's, not the
   tool's: at least the part's full thickness, since the part lies inside
   the stock. The class (gouge) is the same as the column engine's.
3. **The clip is the column law as a solid, not a plain intersection.**
   The column clip lowers a column to the entered setup's stock top only
   where that stock's grid has a column; outside it what stands is left
   alone, and the floor is not touched. As a solid that is the carried
   body minus the half-infinite prism above the new stock's top over its
   footprint: `max(d_body, -d_prism)`. Max/min of signed distances keep
   the zero set exactly (they bound the distance, they do not equal it),
   and the verdicts read only membership along columns, so the clip is
   applied where the columns are read. A full intersection with the new
   block (`Clip::Block`, step C) would also delete what stands outside the
   new block and below its floor - less material than the column engine
   reports, the unsafe direction - so the holders do not use it. The
   footprint and tops are the entered setup's own stock grid, kept with
   the turns applied since, so a box and a cylinder clip alike and a later
   flip turns the ceiling into a floor bound.
4. **The agreement lane.** Three layers:
   - sim, quick lane: `carry_engine_agreement` (the read through every
     turn, cuts after a flip) and new `carried_stock` pins for the holder
     type (offset lattice, clip, cut-through read) on both engines.
   - app, heavy lane: `carried_engine_agreement` (`#[ignore]`, release):
     the scenarios of `setup_carry`, `rest_across_setups` and
     `carried_verify` built once and read on both engines - the program
     verdict column for column within `FIELD_VERDICT_EPS` with the same
     class, counts and aggregates; the incoming body's tops, floors,
     empties, volume and `floor_judged`; the rest grid; the bounds verify
     reads. Zero findings, printed whole before the suite fails.
   - the engine is picked per thread (`stock_sim::carry::with_engine`),
     so one process runs both; the existing carried suites can be run
     whole on the field arm with `CAMCRAFT_CARRY_ENGINE=field`.

**The holders.** camcraft-sim gains `CarriedStock` (`Column(Body)` |
`Field(FieldBody)`): build, carry, clip, simulate, and one read-back
(`body()`) onto the column body, so `deviate`, `display`, `volume` and
`bounds` stay the column carry's own code on both arms (step D's
decision, kept). `stock_sim_carry::Held`, `rest/across::sweep` and
`rest::PriorPlan` hold `Stock` / `CarriedStock` instead of `Dexel` /
`Body`. The column engine stays the default, so every shipped verdict,
golden and stamp is byte-identical; #589 flips the default.

**Why.** The rule for every choice: the field read never reports less
material than is there, and never turns a convicted column into "no
verdict". Reading back onto the column body keeps one verdict
implementation (the one the goldens already prove) instead of a second
two-sided verdict on the field.

**Considered and dropped.**
- *A field stock with the column's semi-infinite floor* (ten spans of
  phantom material below, clipped at read-back, as `Stock::block` does):
  it would keep the tool's depth below the stock, but after a flip every
  cut sweeps through the phantom above the part - the 35-40x cost noted
  on #586. A closed solid costs the same both ways up.
- *`Clip::Block` for the app clip*: drops material outside the new block
  (above).
- *A cylinder stock on the field arm from its mesh*: the mesh is an
  inscribed polygon, so rim columns would hold less than the column
  engine's disc. Refused by name on the field arm for now
  (`FIELD_CARRY_SHAPE_REFUSAL`); the column arm is unchanged.
- *Passing the engine as a parameter through every rest and job call*:
  some twenty signatures for a switch #589 removes again.

**Cost to change later.** The cut-through read is one function
(`FieldBody::body`); a field that records the tool's depth below the
stock could replace it without touching the holders. The engine switch
is one thread-local and one environment read in `stock_sim_carry`.
