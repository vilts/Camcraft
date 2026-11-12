## #93 - editing each tool kind by its parameters inside the outline editor (slice 3)

**Decided:** the Draw cutter view carries the numbers each milling kind's
outline is drawn from, right above the points table: cutter Ø, the kind's
tip (a ball's radius shown read-only, a bull's corner R, a drill's point
angle, a chamfer mill's cone angle; nothing for the flat kinds), flute
length, shank Ø and shank length, plus a Neck (Ø and length). They edit
the editor's same draft numbers the left column edits, so the two can
never disagree.

**Parametric is regenerated, Custom is carried.** A number edit redraws a
Parametric outline (as before). A Custom outline is carried onto the new
numbers by `cutter_fit::refit`: the tip within the flutes is regenerated
from the kind (it is the tip the generators cut with, so a hand-shaped
tip that disagrees in shape was display-only anyway), everything drawn
above the flute top keeps its shape and moves with the flute top, the
shank top moves to the new one, the stated shank radius follows the new
shank, and anything wider than the new shank is clamped to it (its arcs
go straight). The result is still judged by `cutter_profile::check`, so
an edit that cannot be carried is refused by name, live. Considered and
dropped: scaling the whole outline (a ball or a drill cone distorts, an
arc's radius stops fitting its chord); refusing every number edit under a
drawn cutter, as slice 2 did (the user cannot edit the tool by its
parameters at all then).

**The neck is a parameter of the outline, not a new tool number.** `Tool`
states no neck, and adding one would change the tool record and every
reader for a display-only shape. `cutter_fit::necked` draws the kind's
tip, a straight neck up from the flute top and the stated shank;
`neck_of` reads it back only when the outline is exactly that shape, so a
hand-drawn neck of another shape shows no neck parameter (turning it on
replaces the drawing). A neck must be below the shank's diameter and
inside its length, else refused by name. Cost to change later: a
`neck_d`/`neck_len` on `Tool` would make `necked` its `parametric`.

**One command for numbers and outline: `ReshapeTool`.** `ChangeTool`
refuses an edit that strands a drawn cutter, and `SetToolCutter` judges
against the numbers already landed, so neither order of the two lands a
carried outline. `ReshapeTool` runs every `ChangeTool` law on the new
numbers (`ChangeTool::judged` with the cutter agreement skipped) and
judges the outline against the NEW numbers, in one undo step. Apply
cutter queues it when the draft numbers moved (else `SetToolCutter` as
before); the editor's Apply queues it when a drawn cutter is landed and
the edit moves a number the outline is drawn to - carrying the LANDED
outline, so unapplied point edits stay Apply cutter's. `ChangeTool` alone
(and over MCP, #734) keeps refusing, so nothing strands a drawn cutter
without saying so. MCP: `set_tool_cutter_params` (params subset + neck)
drives the same command; `get_tool_cutter_profile` reports the neck.

**Not here:** feeding the drawn profiles to the simulation and the
collision laws stays #852 (PR #957).
