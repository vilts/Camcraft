+++
title = "Plasma cutting"
description = "The Plasma Profile operation: kerf, cut direction, leads, small holes, the plasma table and post it needs, and the refusals you are most likely to meet."
weight = 5
aliases = ["guide/operations-plasma/"]
+++

This page is the reference for **Plasma Profile**, the one kind in the
**Plasma** group of the [operations](@/guide/operations/_index.md) ribbon. For
the tabs themselves and what a click picks, see that page; for what a
refusal means in general, see [Reading a refusal](@/guide/reference/refusals.md).

Lengths are in mm (inches in an inch document), feeds in mm/min (in/min).

Blue shows the operation travel; dashed orange shows approach and retract; gray shows the workpiece. These schematics are not generated NC paths and have no scale.

## The machine, the tool and the post

**A plasma table.** Plasma Profile needs a setup whose machine is a
plasma table, such as the built-in **Tormach 1300PL**; a mill, lathe or
mill-turn is refused ("a Plasma Profile operation is plasma cutting work;
the setup's machine is a ... - mount a plasma table first"). A plasma
table in turn takes no other kind ("... the setup's machine is a plasma
table, which cuts only plasma operations (a Plasma Profile)").

**A torch.** The tool is a plasma torch, and its diameter is the kerf
width. Anything else is refused ("a plasma profile mounts a plasma torch,
not a ... - its diameter is the kerf width"); a torch states no rpm ("a
plasma torch turns no spindle - its rpm must be 0 ...").

**A plasma post.** The **Tormach PathPilot Plasma** post serves plasma
tables, and only them. The machine check judges X/Y travel and feed
against the table, and refuses a spindle word, a tilted or turned move, or
more tools than the table carries. Z is the torch lifter: the control sets
pierce and cut height, so cutting depth and Z travel are not judged, and
the 3D stock simulation does not run for a torch. The through-cut is
judged instead by the plasma laws at the post gate (below).

## Plasma Profile

{{ <icon name="op-plasma-profile" /> }} ![Top view of torch travel from a pierce point along the lead and outer plate profile.](../../../operation-paths.svg#PlasmaProfile)

**For:** cutting parts out of flat plate: one through-cut per contour,
with the kerf offset to the waste side, arc leads, holes before the
outline that frees the part, and a slower centre-pierced cut for small
holes.

**Picks:** the closed contours and open runs to cut, on the Geometry tab.
Without picks it cuts the model's through-silhouette. A closed contour is
cut with the kerf on the scrap side; an open run is cut on its centre
line, with no kerf side and no leads.

| Tab | Setting | Default |
|---|---|---|
| Geometry | Kerf width | Torch: the torch's diameter |
| Geometry | Compensation | In computer (offset path); or In control (G41/G42) |
| Geometry | Direction | Outer CW, holes CCW; or Reversed: outer CCW, holes CW |
| Heights | Clearance (torch off) | 25 above the stock top |
| Heights | Top of plate / Bottom of plate | model top / model bottom |
| Heights | Pierce height / Cut height | Controller (blank) |
| Passes | Small hole below | 32 |
| Passes | Small hole feed | 65 % |
| Passes | Overburn | 0 |
| Lead in / Lead out | Lead-in length / Lead-out length | 5 / 3 |
| Feeds | Cut feed / Pierce delay | AutoFS (blank: the controller's material table) |

**Kerf.** The path is the torch centre: every lit move stands half the
kerf off the part on the waste side. **In control** posts the part
outline with G41/G42 and lets the control offset it; it needs straight
leads at least the kerf radius long.

**Direction.** The default runs outer contours clockwise and holes
counter-clockwise, so the arc's square edge falls on the part and the
bevelled edge on the scrap. **Reversed** swaps both, and the path says so.

**Leads.** Arcs on the scrap side. A lead with no room beside the part is
shortened, and the path names it ("lead-in shortened to ... mm - the
stated ... mm found no room beside the part").

**Small holes.** A hole under **Small hole below** across, or under 1.5x
the plate thickness, leads in from near its centre, takes no lead-out
(the **Overburn** runs past the start instead) and is cut at **Small hole
feed** of the cut feed.

**Heights and feeds.** One through-cut at the plate top; there are no
depth steps. Blank heights, feed and pierce delay are left to the
controller (AutoFS); a stated one must be positive ("a stated feed must
be positive - leave it blank for the controller's").

**The plasma laws.** Generation refuses any lit move whose kerf would
reach the part, and any edge the kerf can never reach. At the post gate,
cut order and cut crossing refuse; a kerf crossing and a tip-up warn,
named on the line:

- "contour C...: move ... would carry the kerf into the part near X... Y...
  - refused";
- "contour C...: the torch never comes within a kerf of the part edge near
  X... Y... - a slot, notch or keyway narrower than the kerf (... mm) is
  bridged by the offset, not cut; widen it past the kerf or use a narrower
  kerf";
- "cut order: contour C... is cut after contour C..., which encloses it -
  a hole after its outline (or an island after its hole) cuts into a part
  already freed";
- "cut crossing: contour C...'s cut crosses contour C...'s - the kerf runs
  through a part already cut";
- "kerf crossing: contour C... comes within a kerf of contour C...'s cut -
  the torch height control can dive into the open kerf".

**Common refusals:** "plasma profile: no contour to cut"; "picked contour
... crosses itself - a cut path cannot cross its own kerf"; "picked
contour ... has no area (a zero-length or collinear ring) - there is
nothing to cut around"; "control compensation (G41/G42) needs a straight
lead-in of at least the kerf radius (... mm) - the control builds the
offset over it; the lead-in is ... mm"; "the clearance plane (...) must
stand above the cut plane (...)"; "plasma profile: the plate's bottom
(...) stands above its top (...)".
