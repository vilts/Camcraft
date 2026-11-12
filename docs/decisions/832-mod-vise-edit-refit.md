## #832 - Mod vises: edit and re-fit a placed pair; the dual-station block held back

**Decided: a placed jaw block is edited by `SetModJaw` and the adjustable
block re-fitted by `RefitModVise`, one undo step each.** `SetModJaw`
regenerates the block's solid through `UpdateModel` (the way `SetVise`
does) and swaps its marker in the same step; the block keeps its hole and
turn, and the edit is refused by name when the block would no longer fit
there (a bolt or pin off the grid, via the new `modvise::fit::footprint_at`;
hanging off the plate, via the plate seat), when it changes the block's
side, or when its face would reach into the stock. A face lifted off the
stock is a warning. `RefitModVise` runs the same `fit_pair` the pair was
placed with, with the fixed block where it stands, so it re-picks the
adjustable block's hole and slide. Every refusal of the fit comes through
by name, and nothing moves on a refusal. *Why:* one fit and one footprint
law for placing and re-fitting, so the two can't drift apart. Considered
and dropped: re-fitting by undoing the pair and placing it again (that
loses any edits made since), and a re-fit that also moves the fixed block
(the fixed block is pinned on purpose, and moving it is a re-place).

**Decided: `RefitModVise::to_partner` finds the fixed block itself, but
only when it is unambiguous.** A pair is not linked in the data. Each block
is a rider of its plate, so the partner is the single fixed jaw block in
the same setup on the same plate. If there are none, or more than one, the
command refuses and names them, and the caller then names the fixed block
(`RefitModVise::new`, the MCP `fixed` argument). *Why:* adding a pair link
to the file format would have meant a new key and migration rules, for a
lookup that is unique in every layout we ship. Changing this later costs
one optional key.

**Decided: a jaw that was not in front of the stock before an edit counts
as clear.** This is the follow-up from the #833 review. `judge_face` now
backs both `SetPlate`'s re-seat check and `SetModJaw`. A `face_gap` of
`None` before the edit reads as an infinite gap. So a plate edit (or a
figure edit) that brings a jaw from beside the stock to 16 mm inside it is
refused, where before it passed with only the clash warning. The lift-off
warning still needs a measured gap before the edit. It is pinned by
`a_plate_edit_that_brings_a_jaw_into_a_stock_it_was_beside_is_refused`.

**Held back: the dual-station block (part 1 of #832).** This needs a design
decision this tier should not take alone. A dual-station block clamps two
workpieces, but a CamCraft setup holds exactly one stock, and fixtures are
judged per setup (`fixture::tagged` filters by the fixture's setup). There
are two ways to give the second station its own stock, and both break the
Prime Directive:
- Take the second station's stock from a second setup. That setup's
  program would then run with vise steel it does not own, so its collision
  and fixture checks would never see the vise. That is a silent gouge path.
- Duplicate the steel into each setup. Then two copies stand in one place,
  and each setup's fixture-vs-fixture and clash checks see the other's copy.

Placing a dual block with one station clamped and the other left empty is
not "fitted per station". The block geometry itself (a second insert on the
rear face) is simple. The open question is the multi-stock model: either
several stocks in one setup, or fixtures shared between setups. That is
`effort:max`/research work. #832 stays open for it, with this note on the
issue.

**Save/load:** no new keys. `SetModJaw` and `RefitModVise` write only
existing `mod_jaw` and `on_plate` fields, which #830's lenient
`fixture_keys` reader already loads through `modvise_dto`. Old files load
unchanged.
