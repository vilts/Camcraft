+++
title = "2D milling operations"
description = "One section per 2D milling and hole kind: what it is for, its key settings and defaults, what it picks, and the refusals you are most likely to meet."
weight = 1
aliases = ["guide/operations-2d/"]
+++

This page is the reference for the kinds in the **Milling 2D** and
**Drilling** groups of the [operations](@/guide/operations/_index.md) ribbon
(Probe has a page of its own to come). For
the tabs themselves and what a click picks, see that page; for what a
refusal means in general, see [Reading a refusal](@/guide/reference/refusals.md).

Lengths are in mm (inches in an inch document), feeds in mm/min (in/min).
The defaults below are what a freshly created operation starts with: feed
1200, plunge feed 300, climb milling on, clearance 25 above the stock top,
top at the model top, bottom at the model bottom, stepdown 5, stepover 3
and, where a kind has one, a retract plane 10 above the stock top. A
retyped operation keeps the numbers it already had. Pocket, 2D Adaptive
and 2D Offset size their entry helix to 0.4 x the tool radius when created
with a tool.

Every kind checks its tool. Face, Contour, Pocket, Adaptive, Offset, Slot,
Trochoidal slot, Circular and Bore cut with a flat end mill or bull-nose;
Trace and Engrave take any mill or a taper tool; V-Carve takes a chamfer
mill (V-bit) only; Slot Undercut takes a
T-slot, keyseat, dovetail or lollipop cutter only. A wrong tool is refused by
name ("'<tool>' is a <kind>; a <operation> operation cuts with a ..."),
and a document with no suitable tool is told to add one to the tool
library.

Use Ctrl+click to build a feature selection from multiple edges. CamCraft
accepts the first edge even when that edge alone cannot define a region;
**Generate** checks the completed selection. An armed Boundary row accepts
faces and edges regardless of the toolbar's pick mode. Its open boundary
can hold separate chains that keep the area between them. Crossing,
touching or ambiguous chains are refused; Contour still rides one open
boundary chain. Include and Exclude boundaries must remain closed.

On **Geometry > Boundary > Tool**, **Inside boundary offset** moves the
picked edge or face loop before tool containment: a positive value grows
the area outward and a negative value shrinks it. **Outside boundary
offset** moves only the stock-side limit of an open boundary; the picked
edge stays put. For example, Outside boundary offset -2 mm pulls the air
passes 2 mm inward without moving the part-side edge. A closed area has
no outer side, so that setting has no effect there. Crossing or ambiguous
boundaries still refuse; correct the picks rather than adding an offset.

Blue shows the operation travel; dashed orange shows approach and retract; gray shows the workpiece. These schematics are not generated NC paths and have no scale.

## DXF profiles

A DXF profile belongs to a setup's stock; it is not a solid model. See
[Place a DXF profile](@/guide/set-up/setups.md#place-a-dxf-profile).
A 2D operation can name the profile as its source, but **generation from
its chains is not available yet**. Generate refuses by name and leaves no
toolpath. Import a solid model for a job you need to machine now. A
profile has no model top or bottom: use stock or fixed height references.

## Face

<!-- catalog:examples:start -->
**Example cases**

- [2d/first-part](https://github.com/NeoTech/Camcraft/tree/main/examples/catalog/2d/first-part)
- [probing/probe-face](https://github.com/NeoTech/Camcraft/tree/main/examples/catalog/probing/probe-face)
<!-- catalog:examples:end -->

{{ <icon name="op-face" /> }} ![Top view of parallel facing passes across the stock top.](../../../operation-paths.svg#Face)

**For:** skimming the top of the stock flat, usually the first operation.
Choose it over a pocket when the area to clear is the whole stock top, not
a region inside the part.

**Picks:** none needed. It faces the whole stock outline by default; picks
bound it to a smaller area.

| Tab | Setting | Default |
|---|---|---|
| Heights | Cut Z | stock top + 0 |
| Heights | Stage down to Cut Z (Top Z, Stepdown) | off: one level |
| Passes | Stepover (Y spacing) | 3 |
| Passes | Holes | Lift over (or Cut across air, lift over islands; Cut straight through) |
| Passes | Pass starts at | Auto, not used: the rows run edge to edge, so there is no inside or outside order |

On **Passes**, turn on **Arc turns at row ends** to round the turns
between facing rows. With **Arc radius = half the row spacing**, the
turn is a semicircle where it fits. To set a smaller **Arc radius**,
clear that checkbox. Where there is room, CamCraft joins two quarter
arcs with a straight move outside the facing area.

**Do not touch** is on **Geometry > Boundary > Boundaries**. Choose
**Pick protected**, pick model faces or edge chains, then choose **Done**
and **Apply**. **Clearance** adds space around the full tool assembly
(flutes, shank and holder). Facing rows stop short of protected items;
the material there stays unfaced. CamCraft refuses a cut with no safe
facing room. Protected items stay highlighted while the operation is selected.

**Common refusals:** "staged facing needs a positive stepdown";
"staged top below the finished height"; "clearance plane not above the
first cut level".

## 2D Contour

<!-- catalog:examples:start -->
**Example cases**

- [2d/contour-tabs](https://github.com/NeoTech/Camcraft/tree/main/examples/catalog/2d/contour-tabs)
<!-- catalog:examples:end -->

{{ <icon name="op-contour" /> }} ![Top view of one offset profile followed around a wall.](../../../operation-paths.svg#Contour)

**For:** one wall, inside or outside, followed at a set of levels: profiles,
bosses and bores. Choose it over a pocket when only the wall needs cutting.

**Picks:** faces or closed edge chains for the wall; an open chain can be
added as an open boundary.

| Tab | Setting | Default |
|---|---|---|
| Geometry | Side | Auto (Inside, Outside); Auto may resolve to Inside from the picked rings, and says so |
| Geometry | Tolerance | 0.01 |
| Heights | Retract Z (between passes) | stock top + 10 |
| Heights | Stepdown | 5 |
| Passes | Stock to leave | 0 |
| Lead in / Lead out | Lead in/out | Linear, length 8 (None, Arc, Tangential, Perpendicular) |
| Lead in / Lead out | Tab count / width / height | 0 / 5 / 1 |
| Lead in / Lead out | Entry | Plunge, helix radius 1 |

**Side Auto on a picked ring** reads the part at the ring's own level. It
looks just under the top and just over the bottom of the picks' height
span and asks which side holds material. A blind-bore rim is therefore cut
from inside, even though the bore floor fills the part's outline from
above. If the two levels disagree, or material is on both sides, Auto
refuses by name: "Side Auto cannot tell which side of the picked ring holds
the material at Z ...: ... - set Side to Inside or Outside". Set Side
yourself then. If no level shows material either side (a sheet part), the
part's outline decides.

A picked ring whose chosen side stands in the part at every level is
refused too, instead of cutting only the top plane in air. For example,
Outside on a hole wall: the message names the ring, the part top and the
fix: "the picked ring at (x, y) meets its wall at no level below the part
top Z ...: the cutter Outside this ring stands in the part material - if
the ring bounds a hole, set Side to Inside; else check the tool
containment". A whole-model Contour is unchanged.

An open run picked on a part cuts on its air side, offset by the tool
radius plus stock to leave. For a run picked on the stock instead,
**Side** chooses the offset: Inside goes toward the stock and Outside
goes away from it. Auto goes away from the part; it needs a part whose
side is clear at every level. An ambiguous side is refused. Inspect the
generated path before posting.
CamCraft refuses an open run whose air side changes or whose offset cuts
the kept part. A part-source run with no part material beside it keeps a
centreline ride and the path notes say so.

On **Passes**, turn on **Arc turns at corners** to round corners that
turn toward air. Corners that turn toward the part stay sharp, and the
path notes identify them. **Arc radius = tool radius** is the automatic
setting. Clear it to enter an **Arc radius**.

An inside **Ramp** entry descends along the lead and ring at the entry
angle. An unset entry angle means 3 degrees, independently of the link
angle. If there is too little room for that ramp, the path reports its
substitution and uses the plunge feed. Read the generated entry notes.

<div class="guide-pair">

![2D contour toolpath without lead-in plunging directly onto the workpiece corner](../../../images/guide/op-2d-lead-before.png)
*Before: Lead set to None plunges directly onto the contour.*

![2D contour toolpath with six millimeter arc lead-in curving tangentially into the wall](../../../images/guide/op-2d-lead-after.png)
*After: Arc lead-in enters tangentially from clear stock.*

</div>

**Common refusals:** "the bore is narrower than the tool - switch Side to
Outside or pick a smaller tool"; "cannot cut the gap at (x, y): it is
narrower than the tool's ... diameter"; "a Contour's open boundary must be
an open chain".

## 2D Pocket

<!-- catalog:examples:start -->
**Example cases**

- [2d/adaptive-pocket](https://github.com/NeoTech/Camcraft/tree/main/examples/catalog/2d/adaptive-pocket)
- [2d/first-part](https://github.com/NeoTech/Camcraft/tree/main/examples/catalog/2d/first-part)
- [multiaxis/indexed](https://github.com/NeoTech/Camcraft/tree/main/examples/catalog/multiaxis/indexed)
<!-- catalog:examples:end -->

{{ <icon name="op-pocket" /> }} ![Top view of passes clearing the inside of a pocket.](../../../operation-paths.svg#Pocket)

**For:** clearing the inside of a closed area down to a floor, with islands
left standing. **Detect pockets / slots** on the Model page finds pockets
and creates one per row.

**Picks:** a floor face or a closed edge chain; an open chain is refused
for an include or exclude boundary.

| Tab | Setting | Default |
|---|---|---|
| Passes | Strategy | Auto (Zigzag, Waterline, Adaptive, Offset rings); Auto takes Waterline for a round outline, Zigzag otherwise |
| Passes | Stepover | 3 |
| Passes | Stock to leave | 0 |
| Passes | Optimal load (% of D) | about 14.6 (used by Adaptive and Offset rings) |
| Passes | Pass starts at | Auto (Inside, Outside); used by Waterline and Offset rings |
| Heights | Stepdown | 5 |
| Lead in / Lead out | Entry | Helix, radius 1 (Plunge, Ramp, Arc) |

![2D pocket operation selected with Passes parameter panel and multi-level pocket toolpath in the viewport](../../../images/guide/op-2d-pocket-params.png)

<div class="guide-pair">

![2D zigzag pocket toolpath with wide four point five millimeter stepover leaving coarse pass spacing](../../../images/guide/op-2d-stepover-before.png)
*Before: wider stepover (4.5 mm) spaces passes further apart.*

![2D zigzag pocket toolpath with fine one point eight millimeter stepover filling dense floor passes](../../../images/guide/op-2d-stepover-after.png)
*After: smaller stepover (1.8 mm) packs passes closely across the floor.*

</div>

<div class="guide-pair">

![2D pocket waterline toolpath using direct vertical plunge entry straight into the pocket floor](../../../images/guide/op-2d-entry-before.png)
*Before: Plunge entry dives straight along Z.*

![2D pocket waterline toolpath using helical spiral entry ramping gradually into each pocket depth](../../../images/guide/op-2d-entry-after.png)
*After: Helix entry spirals down along the programmed radius.*

</div>

**Pass starts at** (Passes tab) sets where each level of rings starts.
It is the same option as on 2D Offset:

- **Auto** cuts the rings from the middle of the area outward. This is
  the order that 2D Pocket always cut.
- **Outside** starts at the outer edge of the area and works in. With an
  Outside containment, the outer edge is the stock edge.
- **Inside** starts at the island or part wall and works out.

Only the ring strategies (Waterline, Offset rings) have a ring order.
Zigzag rows and rest machining keep their own order. When you set Inside
or Outside on them, the path notes say that the option is not used.

**Common refusals:** "pocket is narrower than the tool"; "stepover leaves
bands the tool cannot reach"; a pick that "projects to a line from above
and bounds no area".

Pocket, 2D Adaptive and 2D Offset can also use two open part edges as the
walls of a band. The stock outline closes its ends. Both walls must be
level at the same height and run alongside one another without crossing.
**Extend past** extends the band's open ends along the walls, without
widening it. With zero extension, the tool centre stops a radius inside
the stock edge; an extension of at least one radius clears that edge.
Mixed open and closed picks, ambiguous bands and tool containment On or
Outside are refused.

A U-shaped blind slot can use its floor face or all three rim edges.
The two side walls and closed end define the slot; the stock outline
closes its open mouth. **Extend past** opens that mouth beyond the stock
edge. A missing wall, unequal wall heights or an ambiguous mouth is refused.

**Arc turns at corners and links** rounds pocket-family corners.
Its optional **Arc radius** defaults to the tool radius when unstated.
A stated radius must be at least 0.01 mm and no larger than the tool
radius. The path notes report corner material left by the rounding.
Adaptive's Trochoidal method refuses this option because its loops
already supply the arc motion.

**Rest machining** on **Passes** lets a smaller tool cut what earlier
roughing left. Enable it, choose **Rest stock from > Previous operations**,
then **Apply**. CamCraft reads the stock left by earlier operations of this
setup at every cut depth; it hides the entered previous-tool dimensions.
**Rest allowance** grows the rest area to overlap the earlier cut.
For example, rough a pocket with a 12 mm cutter, then place a 6 mm Pocket
after it to clear the remaining corners. Generate the program in order.
If earlier operations cleared nothing at those depths, the whole pocket
is cut; if nothing remains, generation reports "no rest material".

Rest supports Waterline, Offset rings and Zigzag; Auto uses Waterline.
The old Pocket Adaptive strategy is refused. Missing or stale earlier
toolpaths are refused by name: "generate the program in order" or
"regenerate the program in order". An earlier suppressed operation with
a recorded cut also refuses; unsuppress it or choose **Entered tool**,
which trusts the dimensions you type instead of reading previous cuts.
Previous-operations rest is refused on a tilted Pocket; clear the tilt or
use Entered tool. Pre-drilled entry and rest cannot be combined; turn
off one of them.

## 2D Adaptive

<!-- catalog:examples:start -->
**Example cases**

- [2d/adaptive-pocket](https://github.com/NeoTech/Camcraft/tree/main/examples/catalog/2d/adaptive-pocket)
<!-- catalog:examples:end -->

{{ <icon name="op-adaptive2d" /> }} ![Top view of looping pocket passes with small radial engagement.](../../../operation-paths.svg#Adaptive2D)

**For:** roughing a pocket with a constant side load on the tool, so it can
run deeper and faster than a fixed stepover allows. Choose it over Pocket
for hard materials or deep roughing; it is Pocket's machinery with the
spacing set by the load.

**Picks:** as Pocket.

| Tab | Setting | Default |
|---|---|---|
| Passes | Optimal load (% of D) | about 14.6 |
| Passes | Stock to leave | 0 |
| Heights | Stepdown | 5 |
| Lead in / Lead out | Entry | Helix, radius 1 |

**Method** chooses Spiral or Trochoidal. Trochoidal follows the pocket's
medial lanes, including parallel branches around islands, then cleans up
corners. With **Method = Trochoidal**, these additional controls appear:

| Tab | Setting | Default |
|---|---|---|
| Passes | Max engagement | 90 degrees; the forward step tightens to hold this limit |
| Passes | Loop width | All the room; optionally limit the band swept by loop centres |
| Passes | Return | Auto: a 0.3 mm microlift if clearance permits, otherwise Retract |
| Passes | Microlift height | 0.3 mm when Microlift is selected |
| Feeds | Return feed | Cutting feed; optionally state another feed |

Max engagement must be above 0 and at most 180 degrees. A stated loop
width must be positive; the available channel limits its actual width.
A microlift above clearance is refused.
Choose Retract when the return must rise higher. These controls do not
apply to Spiral.

**Pass starts at** (Passes tab) sets where each level starts. It is the
same option as on 2D Offset:

- **Auto** is the plan that 2D Adaptive always cut.
- **Outside** starts at the outer edge of the area and works in. Where
  the area reaches past the stock edge with room for an entry in air,
  the adaptive plan cuts the band from the air inward and ends on the
  part wall. Where it does not (for example an Outside containment that
  stops where the tool touches the stock, or a closed pocket), Offset
  rings cut the area from its outer edge in, and the operation's notes
  say so.
- **Inside** starts in the middle of a pocket without islands and grows
  out to its wall (the adaptive plan). An area with islands or a part in
  it is cut with Offset rings from the island or part wall out, and the
  notes say so.

The Trochoidal method has no ring order: it refuses Inside and Outside.

**Do not touch** (Geometry tab, **Boundaries**): as 2D Offset, with both
methods. 2D Adaptive plans one area for all levels, so its keep-out covers
each level from the floor up to the clearance height. Beside a protected
item that is lower than the top of the cut, it leaves more stock than
2D Offset, which keeps out level by level.

**Common refusals:** as Pocket. With **Method = Trochoidal**, a **Pass
starts at** other than Auto.

## 2D Offset

<!-- catalog:examples:start -->
**Example cases**

No catalog case yet.
<!-- catalog:examples:end -->

{{ <icon name="op-offset2d" /> }} ![Top view of concentric offset passes inside a pocket.](../../../operation-paths.svg#Offset2D)

**For:** roughing a pocket in concentric rings from the boundary inward at a
fixed stepover. Choose it over Adaptive when an even offset pattern is
wanted rather than a constant load.

**Picks:** as Pocket. **Settings:** Stepover 3, Stepdown 5, Stock to leave
0, Entry Helix radius 1.

**Known issue in 0.26.0:** **Inside** and **Outside** on 2D Offset can
produce ramped linear cuts and lifts instead of the intended level rings.
Inspect these paths before posting; use **Auto** if this affects the job.
The report is [#3741](https://github.com/NeoTech/Camcraft/issues/3741),
with follow-ups [#3744](https://github.com/NeoTech/Camcraft/issues/3744),
[#3745](https://github.com/NeoTech/Camcraft/issues/3745),
[#3746](https://github.com/NeoTech/Camcraft/issues/3746),
[#3747](https://github.com/NeoTech/Camcraft/issues/3747) and
[#3748](https://github.com/NeoTech/Camcraft/issues/3748).
This release note applies until a build with the fixes ships.

**Pass starts at** (Passes tab) sets where each level starts:

- **Auto** cuts the rings from the middle of the area outward. This is
  the order that 2D Offset always cut.
- **Outside** starts at the outer edge of the area and works in. Each
  ring follows the outer edge, one stepover in from the ring before, and
  ends where it meets the part. The part wall comes last. With an
  Outside containment, the outer edge is the stock edge: each level
  starts outside the stock and ends on the part wall. Use it to stop
  full-width slots in the stock.
- **Inside** starts at the part or island wall and works out. Each ring
  follows the walls, and the outer edge comes last.
- Some cuts still take the full tool width. The toolpath then shows a
  **Pass start** warning that names the case:
  - **Outside** on a closed pocket: the first ring runs along the pocket
    wall.
  - **Inside** on an Outside containment: the first ring runs along the
    part wall.
  - **Inside** round an island: the first ring runs along the island wall.
  - A place narrower than the tool plus one stepover, or stock that the
    start edge does not reach.
- Material inside a hole (the bore of a hollow boss, the stock inside a
  ring-shaped part) starts from its own edge. With **Inside** it has no
  hole to start from, so it is cut from the middle outward, as **Auto**
  cuts it, and a note says so.
- If the rings from the start side would leave material standing, that
  area is cut in the **Auto** order instead, and a **Pass start** warning
  says how much area the rings would have left.

**Do not touch** (Geometry tab, **Boundaries**): pick faces or edge chains
to protect from every cut, plunge, ramp, helix, keep-down and rapid link
across the full tool assembly (flutes, shank and holder). **Clearance**
sets extra 3D standoff in addition to **Stock to leave**. Protected
faces and edges stay highlighted in the viewport while the operation is
selected.

**Breaking file change:** protected operations require project format
version 73. An older build that supports only version 72 or earlier
refuses the newer file. It cannot open it and ignore the protection.
Older projects load with no protected picks. Keep an older copy if you
must return to an older build.

**Common refusals:** as Pocket; picking an item already in the
operation's machining selection or Include boundary; or leaving no safe
cutting room around a protected item.

## Slot

<!-- catalog:examples:start -->
**Example cases**

- [2d/slots](https://github.com/NeoTech/Camcraft/tree/main/examples/catalog/2d/slots)
<!-- catalog:examples:end -->

{{ <icon name="op-slot" /> }} ![Top view of straight passes between the slot endpoints.](../../../operation-paths.svg#Slot)

**For:** a straight slot of a given width between two points. The slot is
typed in or seeded from suitable picked slot geometry;
**Detect pockets / slots** can create one along a recognised slot's centreline.

| Tab | Setting | Default |
|---|---|---|
| Geometry | A X, A Y / B X, B Y | (-20, 0) / (20, 0) |
| Geometry | Slot width | 4 |
| Geometry | Extend past A / Extend past B | 0 / 0 |
| Passes | Stepover / Stock to leave | 3 / 0 |
| Passes | Pass starts at | Auto (fixed) |
| Heights | Stepdown | 5 |
| Lead in / Lead out | Entry | Plunge, helix radius 1 |

Slot cuts one continuous pass at each depth. It follows the centreline
when the cutting width equals the tool diameter. A wider cut joins the
rows and finishes with a lap along the walls.

On **Passes**, turn on **Arc turns at row ends** to round the corners
where the slot rows join. **Arc radius = tool radius** is the automatic
setting. Clear it to enter an **Arc radius**. A single-row slot has no
turn to round. On wider slots, read the path notes for material left by
rounding.

On Face, 2D Contour and Slot, arc turns are off by default. An explicit
radius must be at least 0.01 mm and at most the tool radius.

The A and B fields sit in their own **Manual** section on the Geometry tab;
Slot width and the extension rows stay outside it. While the Geometry tab
is open on the operation, with nothing picked and Properties visible,
the viewport marks A and B with crosses and draws the line between them.
The marks follow each edit before you choose Apply. They are display only;
the committed edit is the undoable parameter change.

**Common refusals:** "slot is narrower than the tool"; "slot width must be
positive".

**Extend past A** and **Extend past B** run the pass beyond each end,
along the slot's length. Values must be finite and non-negative.
Extensions still pass through the part checks. An extension into kept
material blocks posting.

## Trochoidal slot

<!-- catalog:examples:start -->
**Example cases**

- [2d/slots](https://github.com/NeoTech/Camcraft/tree/main/examples/catalog/2d/slots)
<!-- catalog:examples:end -->

{{ <icon name="op-trochoidal-slot" /> }} ![Top view of overlapping loops progressing along a slot.](../../../operation-paths.svg#TrochoidalSlot)

**For:** a slot wider than the tool, cleared with small looping passes that
keep the tool's engagement low. Choose it over Slot in hard material or
when a full-width slotting cut would overload the tool.

**Geometry:** typed in like Slot.

| Tab | Setting | Default |
|---|---|---|
| Geometry | A / B | (-20, 0) / (20, 0) |
| Geometry | Slot width | 16 |
| Geometry | Loop width | All the room (the width less the tool) |
| Heights | Stepdown | 5 |
| Passes | Forward step | 1 |
| Passes | Max engagement | 90 deg |
| Passes | Return | Microlift, height 0.3 (or Retract) |
| Passes | Smoothing radius | 1 |
| Passes | Pass starts at | Auto (fixed) |
| Feeds | Return feed | 2 x the cutting feed |

At each depth, the tool opens an entry bore and cuts overlapping loops
along the slot. A Microlift return retraces the arc it just cut at a
slightly higher Z. The cut advances in this sequence so each loop starts
from material cleared by the preceding cut.

**Common refusals:** "the channel ... is too narrow for a ... tool to loop
in - cut it as a plain Slot"; "a loop width of ... leaves the channel";
"a ... mm microlift would feed the return above the clearance plane ...
use the Retract return to hop higher".

## Slot Undercut

<!-- catalog:examples:start -->
**Example cases**

No catalog case yet.
<!-- catalog:examples:end -->

{{ <icon name="op-slot-undercut" /> }} ![Section view of the cutter going down outside the wall, feeding sideways under the roof and back out.](../../../operation-paths.svg#SlotUndercut)

**For:** the part of a slot that is wider below than at its mouth: a
T-slot, a keyseat, a dovetail, a round undercut groove. The cutter's head
is wider than its neck, so it cannot go down into the slot from above. It
goes down outside the wall, feeds sideways into the slot at depth, cuts
every pass there, feeds back out the same way and only then lifts.

**Tool:** a T-slot, keyseat, dovetail or lollipop cutter. In the tool
library the diameter is the head's, the flute length is the head's height,
and the shank diameter and length are the neck's. A dovetail's tip angle
is the angle between its bottom face and its flank (60 for a 60 degree
dovetail). A lollipop's flute length is where the neck leaves the ball.

**Geometry:** typed in. A and B are the ends of the slot's centre line.
Bulge 0 is a straight slot; any other value makes the centre line an arc
from A to B (the tangent of a quarter of its angle, positive
counter-clockwise).

| Tab | Setting | Default |
|---|---|---|
| Geometry | A / B | (-20, 0) / (20, 0) |
| Geometry | Bulge | 0 (straight) |
| Geometry | Undercut width | 20 |
| Geometry | Entry run-in | 15 |
| Geometry | Neck slot width | 0 (nothing above the neck) |
| Heights | Bottom Z | the undercut's floor (the cutter's tip) |
| Passes | Stepover | 3 |

The undercut width is measured at the cutter's widest point: the rim of a
T-slot or keyseat cutter, the tip corner of a dovetail, the equator of a
lollipop. The first pass runs on the centre line. Each pair of passes
after it widens the undercut by the stepover.

The entry run-in is the distance from A, back along the slot's line, to
the point where the cutter goes down and comes up. Put that point outside
the stock, or in a pocket that an earlier operation cut.

Cut the slot for the neck first, with a Slot operation, and give its width
as the neck slot width. A dovetail or a keyseat that is open above needs
no neck slot.

**Checks that hold the post:** a Z move while material stands in or over
the cutter's head ("moves the undercut cutter in Z with material in or
over its head"); the neck or the holder in material above the head ("shank/
holder ... collides with the uncut stock"), which is what a missing or too
narrow neck slot gives; the head or the neck inside the part.

**Common refusals:** "entry slot ... mm is too narrow for the ... mm cutter
neck"; "undercut width ... mm is narrower than the ... mm cutter head";
"Slot Undercut cuts with a T-slot, keyseat, dovetail or lollipop cutter".

**Simulation:** the stock view removes the true head and neck shape, so
the undercut shows under its roof. The program check that runs before a
post reads the stock from above and leaves an undercut's stock as it
stood; the checks above judge the undercut instead.

## Trace

<!-- catalog:examples:start -->
**Example cases**

- [2d/edge-work](https://github.com/NeoTech/Camcraft/tree/main/examples/catalog/2d/edge-work)
<!-- catalog:examples:end -->

{{ <icon name="op-trace" /> }} ![Top view of a centreline path following a picked chain.](../../../operation-paths.svg#Trace)

**For:** running the tool's centre exactly along picked edges at a depth:
grooves, scribed lines, cutting along a chain. No tool offset is applied,
so any mill or a taper tool may be used.

**Picks:** edge chains, open or closed. **Settings:** Top Z, Bottom Z and
Stepdown 5 on Heights, feeds on Feeds. It has no Passes tab. **Common refusal:** "empty trace
region" when nothing usable is picked.

To mark stock without a 3D part, place a sketch on the stock and pick the
profile, individual chains or a layer. Trace and Engrave follow open and
closed chains and keep circular arcs. The tool lifts between separate
strokes. Verification checks the mark against the stock and can block
posting if the cut is unsafe.

**Do not touch** on **Geometry > Boundary > Boundaries** protects model
faces or edge chains. Choose **Pick protected**, finish picking, and
**Apply**. Trace splits the cutting runs around protected items and
checks entries, lifts and rapid links against the full tool assembly.
**Clearance** adds standoff; a completely blocked cut is refused by name.
Do not put the same item in the machining and protected selections.

## Engrave

<!-- catalog:examples:start -->
**Example cases**

- [2d/edge-work](https://github.com/NeoTech/Camcraft/tree/main/examples/catalog/2d/edge-work)
<!-- catalog:examples:end -->

{{ <icon name="op-engrave" /> }} ![Top view of an engraving path following the marking centreline.](../../../operation-paths.svg#Engrave)

**For:** the same centreline ride as Trace, named for engraving text and
markings with a V-bit. Choose Engrave when the job is marking, Trace when
it is a functional groove; the path is the same.

**Picks and settings:** as Trace.

**Do not touch** on **Geometry > Boundary > Boundaries** also applies
to Engrave. Pick protected model faces or edge chains and set **Clearance**
as on Trace. The path leaves gaps around them and keeps the flutes,
shank and holder clear, including on entries, lifts and rapid links.
A cut with no safe room is refused. Choose **Apply** to commit the picks.

## V-Carve

<!-- catalog:examples:start -->
**Example cases**

No catalog case yet.
<!-- catalog:examples:end -->

{{ <icon name="op-v-carve" /> }} ![Top view of the V-bit tip following the centre line of a picked outline.](../../../operation-paths.svg#VCarve)

**For:** carving letters, logos and other closed shapes with the cone of a
V-bit. The tip follows the centre line of each picked outline and goes as
deep as the shape is wide at that place, so the cone touches the two walls
together. Choose it over Engrave when the shape has width, not only a
stroke.

The centre line is the **medial axis**: centres of circles that fit inside
the outline and touch its walls. Their local radius and the V-bit's
included angle set the tip depth below Top Z. For example, a 90-degree
V-bit in a straight 2 mm wide stroke needs about 1 mm depth (the path
keeps a small wall allowance); a narrower stroke rises toward the
surface. **Bottom Z** on **Heights** caps that depth.
If Bottom Z is only 0.5 mm below Top Z, this stroke keeps an uncleared
floor unless **Flat clearing** on **Passes** is enabled. Pick closed
outlines and a V-bit; the tool and missing-height refusals below name what
must be corrected.

**Tool:** a chamfer mill (V-bit). Its tip angle sets the cone, and its
flute must cover the deepest cut.

**Picks:** closed outlines. The V-bit cuts inside them.

| Tab | Setting | Default |
|---|---|---|
| Heights | Top Z | the model top (the start depth) |
| Heights | Bottom Z | 2 under Top Z (the maximum depth of the tip) |
| Heights | Stepdown | 5 |
| Passes | Flat clearing | off |
| Passes | Floor stepover | 0.25 (shown only with flat clearing on) |

Where the shape is wider than the cone at the maximum depth, the tip stops
at Bottom Z. Flat clearing then cuts that floor flat at the floor
stepover; with it off, the wide places keep the material that the cone
does not reach.

**Common refusals:** "V-Carve cuts with the cone of a V-bit - it needs a
chamfer mill (V-bit)"; "the cone of ... has ... mm of flute, short of the
... mm maximum depth of this V-Carve"; "V-Carve needs Top of Part Z: set
the height".

## 2D Chamfer

<!-- catalog:examples:start -->
**Example cases**

- [2d/edge-work](https://github.com/NeoTech/Camcraft/tree/main/examples/catalog/2d/edge-work)
<!-- catalog:examples:end -->

{{ <icon name="op-chamfer2d" /> }} ![Section view of the chamfer tool cone lying along the chamfer face.](../../../operation-paths.svg#Chamfer2D)

**For:** breaking a top edge with a chamfer mill in one pass: the tool's
cone lies on the chamfer face. Choose it over Deburr for a chamfer of a
stated width on 2D edges.

**Picks:** the edges or faces whose top edge gets the chamfer. Needs a
taper (chamfer) tool whose cone matches the angle.

To chamfer only part of an edge loop, pick one or more open chains.
Connected picks make one run. Separate chains get separate runs.
The cutter enters and leaves each run on the air side. Each run needs
a model wall at the chamfer depth. Leave area-shaping **Boundary**
settings off for open chains.

| Tab | Setting | Default |
|---|---|---|
| Geometry | Chamfer width | 0.8 |
| Geometry | Chamfer angle | 45° |
| Geometry | Side | Auto |
| Geometry | Tolerance | 0.01 |

The floor is not set: it follows from the width and angle. It has no Passes
tab.

**Common refusals:** "2D Chamfer rides its face on a cone - it needs a
chamfer mill"; "a 45-degree chamfer takes a 90-degree cone; 'T5' has ...
degrees"; "the cone of ... is ... mm of flute, short of the ... mm depth
this face stands at".

## Circular

<!-- catalog:examples:start -->
**Example cases**

- [2d/hole-work](https://github.com/NeoTech/Camcraft/tree/main/examples/catalog/2d/hole-work)
<!-- catalog:examples:end -->

{{ <icon name="op-circular" /> }} ![Top view of concentric passes clearing a circular pocket.](../../../operation-paths.svg#Circular)

**For:** a round pocket of a given centre and radius, cut in rings, with a
flat floor or a domed one (**Vault - dome floor**). Choose it over Pocket
when the circle is a dimension, not a model face.

| Tab | Setting | Default |
|---|---|---|
| Geometry | Center X / Center Y | 0 / 0 |
| Geometry | Radius | 25 |
| Geometry | Vault - dome floor | off |
| Passes | Stepover / Stock to leave | 3 / 0 |
| Heights | Stepdown | 5 |
| Lead in / Lead out | Entry | Plunge, helix radius 1 |

Between its rings, Circular climbs vertically to the **Clearance** plane
set on **Heights**, crosses there, and enters the next ring. It does not
make a diagonal link through the uncut core. This applies to flat and
**Vault** floors. For example, with Top Z 0 and Clearance Z 10 mm, the
transfer between rings crosses at Z 10 even when the rings cut below
Z 0. A "clearance plane below the first cut level" is refused; raise
Clearance before generating.

**Common refusals:** "circle radius must be positive"; "stepover must stay
within the cutter radius - the last ring could leave an uncut core".

## Thread

<!-- catalog:examples:start -->
**Example cases**

- [2d/hole-work](https://github.com/NeoTech/Camcraft/tree/main/examples/catalog/2d/hole-work)
<!-- catalog:examples:end -->

{{ <icon name="op-thread" /> }} ![View of the helical tool travel around an internal thread.](../../../operation-paths.svg#Thread)

**For:** milling an internal thread with a helical thread mill. Choose it
over a tap cycle for large threads, hard materials, or a machine without a
rigid-tap spindle. The pitch comes from the tool.

**Picks:** holes, found with **Detect holes**. Tool: a thread mill (a tap
tool carrying the pitch).

| Tab | Setting | Default |
|---|---|---|
| Heights | Bottom Z | each hole's own floor (override optional) |
| Passes | Radial infeed per pass | 0.1 |
| Feeds | Climb milling | on (a right-hand thread) |

**Common refusals:** "thread milling needs at least one hole"; "a thread mill
needs a positive pitch to time its helix against"; "infeed must sit between
0 and the ... mm thread depth this pitch stands at".

## Drill

<!-- catalog:examples:start -->
**Example cases**

- [2d/drill-overfeed](https://github.com/NeoTech/Camcraft/tree/main/examples/catalog/2d/drill-overfeed)
- [2d/first-part](https://github.com/NeoTech/Camcraft/tree/main/examples/catalog/2d/first-part)
- [2d/hole-work](https://github.com/NeoTech/Camcraft/tree/main/examples/catalog/2d/hole-work)
<!-- catalog:examples:end -->

{{ <icon name="op-drill" /> }} ![Side view of an axial drilling stroke and retract.](../../../operation-paths.svg#Drill)

**For:** drilling, spot drilling, tapping and cycle boring with canned
cycles. **Detect holes** fills the hole list from the model; a new Drill
starts empty and refuses to generate until it has holes.

| Tab | Setting | Default |
|---|---|---|
| Heights | R plane (feed starts here) | model top + 0 |
| Heights | Bottom Z | each hole's own floor; **Override the hole floors** to set one |
| Passes | Cycle | Auto: the tool decides (a tap taps, a boring bar bores, a drill chooses by depth against diameter); or Spot, Drill, Peck, Tap (rigid), Bore, Back bore, Tap (floating holder) |
| Passes | Peck depth | Auto (by the cycle) |
| Passes | Dwell at bottom | 0 s |

Tools: drill, tap or boring bar.

A cross drill can use a reduced-fidelity model when its mesh error leaves
a positive verification tolerance. CamCraft increases the required
stand-off, entry, reach and holder clearance to account for that error.
It also tightens the part and radial rapid checks. If the error uses the
full tolerance, import the model again at a higher step budget.

The **Bore** cycle (an expanded bore: feed in, then rapid out) needs a
spindle stop before the retract. Heidenhain, TinyG and Carvera cannot do
that, so they refuse it, with or without a dwell: "bore (G86) requires a
spindle stop before rapid retract - expanded boring is not supported by
this post; refused, not posted". Ordinary drilling stays available on
those posts. Choose another post, or another cycle.

**Common refusals:** "no holes to drill"; "the tip meets steel ... past the
mouth, short of the ... mm asked - ... match the depth and size to the
model"; "tap leaves no thread core".

## Bore

<!-- catalog:examples:start -->
**Example cases**

- [2d/hole-work](https://github.com/NeoTech/Camcraft/tree/main/examples/catalog/2d/hole-work)
<!-- catalog:examples:end -->

{{ <icon name="op-bore" /> }} ![View of circular interpolation inside a hole.](../../../operation-paths.svg#Bore)

**For:** clearing and finishing a hole with an end mill smaller than the
hole. Wide holes use concentric rings at each level; radial sweeps open
each ring band and the last ring finishes the wall. Choose it over a
Drill boring cycle when there is no boring bar of the right size.

**Picks:** holes, found with **Detect holes**.

| Tab | Setting | Default |
|---|---|---|
| Heights | Stepdown | 5 |
| Heights | Bottom Z | each hole's own floor |
| Passes | Pass starts at | Auto |
| Passes | Stock to leave | 0 |
| Passes | Tolerance | 0.01 |

**Pass starts at** sets the order of the rings at each level. **Auto**
cuts from the centre out and ends on the wall. A hole has no inner wall,
so **Inside** is the same order. **Outside** cuts the wall ring first and
then works in to the centre. Each level still starts with the dive at the
centre.

**Common refusals:** "boring needs at least one hole"; "clearance plane
below a hole entry".

For a blind hole, the ring spacing uses the cutter's reach at the floor,
so a bull-nose does not leave an interior floor ridge. Small holes keep a
single ring when its floor bands meet. A plan needing more than 4,096
rings is refused before generation; review the hole and tool sizes.

A Bore **Ramp** entry zig-zags along the first radial sweep. An unset
entry angle means 3 degrees. A separate link angle does not steepen it.
If the sweep is too short, the path states that it substitutes a plunge
at the plunge feed; the return descent to a ring is also named in the
entry notes.
