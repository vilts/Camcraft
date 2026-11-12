## #818 - the index-swing check judges any work-turning rotary about a level line

Follow-up from the review of PR #815 (#813). Sources (behaviour only):
RS-274 / ISO 6983 rotary lettering (A about X, B about Y, C about Z); plain
geometry; the #804 and #813 decisions (`804-a-swept-radius.md`,
`813-swing-gaps.md`).

**Judge every level line, not only X.**
- **Decided:** `rotary_line` now gives the rotary's line, not just "X or
  not": `Horizontal(u)` (a stated level vector, normalised, or an unstated
  `A`/`B`), `Vertical` (a stated vector along Z, or an unstated `C`),
  `Leaning` (a stated vector that is neither, or a zero or non-finite one)
  and `Unknown` (no vector, no RS-274 letter). Every work-turning rotary
  on a level line is judged like the microARC's A: the tip at the safe Z
  (`nose_to_axis - tool length`) against the farthest turning vertex from
  the line through the WCS origin, `|(p - o) x u|`, by `MARGIN_MM`. An
  unstated `nose_to_axis` refuses by name, as for A; a non-finite one (only
  reachable from a hand-built `Machine`, the parser already refuses it)
  refuses by name too.
- **Why:** a vertical tool tip at height h is at least h from ANY level
  line through the origin (its distance is the hypotenuse of h and the
  level offset), so the A law holds for any level u unchanged; only the
  swept radius depends on u. A table B indexing unjudged was the same
  silent pass #813 closed for an A.
- **Level means level:** a vector counts as level (or vertical) only when
  its off component is within 1e-6 of its length - 1 um of height over a
  metre of travel. The old A test allowed about 5.7 degrees of lean and
  then measured about X; a leaning line is not level, and at the far end
  of the travel the tip can be much nearer to it than h.

**A leaning line refuses; it is not judged "conservatively".**
- **Decided:** a work-turning rotary whose stated vector leans (or is
  zero, or not finite) refuses every op by name: "about a line that is
  neither level nor vertical ... its index swing is not judged". An
  unknown line still refuses first (#813), then a leaning one.
- **Why:** the tip's distance to a leaning line depends on where along the
  line the tool stands at the index (the G30 position, which the program
  does not know); over the travel a rising line reaches the tool's height,
  so no bound short of the machine's whole envelope holds. A bound that
  big would refuse everything anyway, and a stated reason is clearer.
- **Dropped:** judging a leaning line by its vertical component (a
  cos-scaled height) - not conservative along the line; ignoring it like
  a C - a silent pass.

**Two work-turning rotaries: the sweep from the WCS origin.**
- **Decided:** when a machine turns the work on more than one rotary (a
  trunnion's tilting A carrying a C, a B carrying a C), the swept radius is
  the farthest turning vertex from the WCS origin itself, and the message
  says so. The least stated `nose_to_axis` of the level ones is used; any
  one unstated refuses.
- **Why:** each turn moves the other rotary's line, so the distance to one
  line is not kept; the distance to the origin is (every line passes
  through it), and the tip is at least h from the origin too. Without
  this, generalising past "the first A" would judge a stacked table about
  one line while the other turn swings the work out from it. No shipped
  machine turns the work on two rotaries, so nothing shipped changes.
- **Dropped:** judging each line on its own - not conservative once one
  turn carries the other's line.

**A vertical line stays unjudged.** A C (vector along Z, or unstated and
named C) alone keeps every point's height, so a tool above the stock top
stays clear; it is neither judged nor refused.

**The load note follows the clause.** A pre-#804 project's rotary with no
`turns_part` is named on load unless its line is vertical - an A, a B, a
leaning or an unknown line - so a saved table B is no longer skipped.

**Cost to change later:** the classification and the stacked sweep are
local to `posting_index_swing.rs`; a tighter stacked bound (sweeping the
actual turn sequence) would replace one call.
