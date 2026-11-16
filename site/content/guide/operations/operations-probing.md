+++
title = "Probing"
description = "The Probe operation: what it finds, its settings and defaults, what it writes into the program, and the guards around the stylus."
weight = 3
aliases = ["guide/operations-probing/"]
+++

This page is the reference for **Probe**, in the **Drilling** group of
the [operations](@/guide/operations/_index.md) ribbon. For the tabs themselves
and what a click picks, see that page; for what a refusal means in
general, see [Reading a refusal](@/guide/reference/refusals.md).

Lengths are in mm (inches in an inch document), feeds in mm/min (in/min).

Blue shows the operation travel; dashed orange shows approach and retract; gray shows the workpiece. These schematics are not generated NC paths and have no scale.

## Probe

{{ <icon name="op-probe" /> }} ![View of a probe approaching a surface, measuring and backing off.](../../../operation-paths.svg#Probe)

**For:** measuring the part on the machine with a spindle touch probe and
either setting a work offset from what it finds or checking the part
against the model. It cuts nothing and removes no stock.

**The tool.** A touch probe; anything else is refused: "a probe operation
mounts a touch probe, not a ...". The stylus ball's diameter is the
tool's diameter.

**Picks.** One feature on the Geometry tab seeds the operation; the pick
writes the mode, directions and expected contact, and one undo takes it
back:

- a flat face: a **Face** touch against its outward normal;
- a straight edge: an **Edge** touch on the side face under it;
- a corner: an **Outside corner**, one X touch and one Y touch;
- a round vertical wall or rim: a **Bore centre** or **Boss centre**.

What the probe cannot touch honestly is refused: "the picked face is not
planar - a face touch needs a flat face"; "the picked face looks down -
the stylus only descends in Z; probe it from a side"; "the picked face
stands ... off X, Y and Z - a probe touches along an axis"; "the picked
wall is not vertical (a cone, fillet or tilted wall) - a bore or boss
centre is found on a vertical round wall".

| Tab | Setting | Default |
|---|---|---|
| Geometry | Find | Face (Edge, Outside corner, Bore centre, Boss centre) |
| Geometry | Touch toward | -Z (a corner: X and Y touch directions, +Y for Y) |
| Geometry | Radius (bore / boss) | 10 |
| Geometry | Expected contact X / Y / Z | 0, 0, 0 (the pick sets it) |
| Heights | Clearance (safe plane) | 25 above the stock top |
| Passes | Approach clearance | 5 |
| Passes | Probe distance | 1 |
| Passes | Back-off | 1 |
| Passes | Depth below edge / Corner inset | 3 / 5 |
| Feeds | Seek feed / Measure feed | 300 / 30 |
| Machine | Result | Set G54 (Report and stop, Set G54 to G59) |
| Machine | Tolerance | 0.5 |

Each touch is a fast seek, a back-off and a slow measuring re-probe; a
centre is the mean of opposite wall touches. The **Probe distance** is a
search limit past the expected contact, not a destination: the probe
stops where it touches, and alarms if nothing trips by then.

**What it writes.** The post writes each touch as a probing (skip) move,
the back-off as an incremental move from wherever the stylus tripped, and
the moves between touches as protected moves that stop on an unexpected
contact. Then the result: with **Set G54..G59** the found position is
written into that work offset; with **Report and stop** nothing is set.
Either way, a found position further than **Tolerance** from the model
stops the program with a message, and a touch that finds nothing alarms.
The Fanuc, Haas, Heidenhain, Sinumerik, Mazak, Doosan, Tormach and Carvera
posts write probing; a post script that cannot is refused by name
("probing: this post script does not define onProbeMove - it cannot write
a probing (skip) move, so the probe operation is refused").

**The guards.**

- Every stylus move is judged against the model: a ball that comes nearer
  than its radius to the part anywhere before or after the expected
  contact is a stylus driven through material, flagged as a gouge, and the
  post is held. A modelled probe body is judged above the stylus too.
- A probe distance over 2 mm is flagged ("probe distance ... mm at move
  ...: the search may run that far past the expected contact, beyond the
  2.000 mm a stylus is trusted to overtravel (the post is not held)").

**Common refusals:** "the back-off must not exceed the approach
clearance"; "probe feeds must be positive and the measuring feed no
faster than the seek"; "an edge is probed along X or Y, not Z"; "a corner
takes one X touch and one Y touch"; "a bore of radius ... mm has no room
for a ... mm stylus ball and its back-off"; "the safe plane Z ... does not
clear the probed feature (top ...) by the stylus ball and the approach";
"the probe tolerance must be positive"; "the work offset to set must be
one of G54..G59".
