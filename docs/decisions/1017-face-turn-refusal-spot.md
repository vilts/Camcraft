## #1017 - Face under an area turn: a refusal's spot goes back through the turn

**Decided.** `face_over` runs in the turned frame; its refusal now leaves
through `boundary_turn::face_over_back`, which maps the spot (and a ghost,
if the refusal carries one) through the same `-turn` as the path
(`turn_refusal`). A ghost that cannot turn (an arc outside XY) is dropped,
never drawn in the wrong frame. With no turn the refusal is untouched.

**Why.** The path was turned back and the spot was not, so the viewport
marked a point that is not on the part.

**Considered and dropped.**
- *An end-to-end test through `generate`.* Not reachable today: a turn only
  comes from an open chain, which replaces the region with the stock side
  (the base's own holes are gone), and every hole that survives (an
  exclude, part material above Cut Z) is also in `forbidden`, so
  `face_band_sane` refuses it first with no spot. The only spot `face_over`
  names (two bridged holes closer than the tool) therefore cannot arise
  under a turn yet. The pin is a unit test over `face_over_back` with a real
  holed region, so the fix holds when a later change lets such a refusal
  through (#871 and #1015 add ghosts here).
- *Mapping inside `face_over`.* It knows no turn; the frame is the caller's.
- *Depending on #1016's `with_ghost_moves`.* Not on this base; `map_ghost`
  and `map_location` already carry both.

**Cost to change.** None; the helper is one function in `boundary_turn.rs`.
