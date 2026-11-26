+++
title = "Waterjet cutting"
description = "The Waterjet Profile operation: kerf, cut direction, leads, tabs, the waterjet machine and post it needs, and the refusals you are most likely to meet."
weight = 7
+++

This page is the reference for **Waterjet Profile**, the one kind in the
**Waterjet** group of the [operations](@/guide/operations/_index.md) ribbon.
For the tabs themselves and what a click picks, see that page; for what a
refusal means in general, see [Reading a refusal](@/guide/reference/refusals.md).

Lengths are in mm (inches in an inch document), feeds in mm/min (in/min),
times in seconds.

Blue shows the operation travel; dashed orange shows approach and retract; gray shows the workpiece. These schematics are not generated NC paths and have no scale.

## The machine, the tool and the post

**A waterjet machine.** Waterjet Profile needs a setup whose machine is a
waterjet (see [Machines](@/guide/check-and-post/machines.md)).

**A nozzle.** The tool is a **Waterjet nozzle**, and its diameter is the
kerf width. Another tool kind is refused ("a waterjet profile mounts a
waterjet nozzle, not a ... - its diameter is the kerf width").

**A waterjet post.** The generic ISO abrasive waterjet post
(`waterjet-iso.cps`) serves waterjet machines only. It opens and closes
the jet for each contour, and it can send arcs as straight lines for a
control without G2/G3. When a post without the jet hooks meets a
waterjet section, the post warnings name it ("... does not define onJetOn
/ onJetOff - use a waterjet post").

## Waterjet Profile

{{ <icon name="op-waterjet-profile" /> }} ![Top view of jet travel from a pierce point in the scrap along an arc lead and around the outer sheet profile.](../../../operation-paths.svg#WaterjetProfile)

**For:** cutting parts out of flat sheet: one through-cut per contour,
with the kerf offset to the waste side, leads that start in the scrap,
and holes cut before the outline that frees the part.

**Picks:** the closed contours and open runs to cut, on the Geometry tab.
Without picks it cuts the through-silhouette of the model.

| Tab | Setting | Default |
|---|---|---|
| Geometry | Kerf width | Nozzle: the nozzle's diameter |
| Geometry | Compensation | In computer (offset path); or In control (G41/G42) |
| Geometry | Direction | Outer CW, holes CCW; or Reversed: outer CCW, holes CW |
| Geometry | Open run side | On the centre line |
| Geometry | Send arcs as lines | off (line tolerance 0.01 when on) |
| Heights | Clearance (jet off) / Top of sheet / Bottom of sheet | from the setup |
| Passes | Hold with tabs | off |
| Lead in / Lead out | Lead shape | Arc |
| Lead in / Lead out | Lead-in length / Lead-out length | 3 / 2 |
| Feeds | Cut feed | 100 |
| Feeds | Pierce time / Abrasive delay | 1 / 0 |

**Kerf.** The path is the jet centre: each cutting move stands half the
kerf off the part on the waste side. One through-cut is made at the sheet
top; there are no depth steps.

**Leads.** The pierce is made in the scrap. A lead with no room beside
the part is shortened, and a tight hole is pierced at its centre.

**Feeds.** State the cut data for the material and the sheet thickness
from the nozzle's recipe. The default feed and pierce time are only a
safe start.

**Common refusals:** "waterjet profile: no contour to cut"; "waterjet
profile: the sheet's bottom (...) stands above its top (...)"; "the kerf
width must be a positive number (it is ...)"; "the cut feed must be a
positive number (it is ...)".
