+++
title = "3D milling operations"
description = "One section per 3D milling kind: what it is for, its key settings and defaults, what it picks, and the refusals you are most likely to meet."
weight = 2
aliases = ["guide/operations-3d/"]
+++

This page is the reference for the kinds in the **Milling 3D** group of
the [operations](@/guide/operations/_index.md) ribbon: 3D Pocket, 3D Finish and
Deburr. For the tabs themselves and what a click picks, see that page; for
what a refusal means in general, see [Reading a refusal](@/guide/reference/refusals.md).

Lengths are in mm (inches in an inch document), feeds in mm/min (in/min).
The defaults below are what a freshly created operation starts with: feed
1200, plunge feed 300, climb milling on, clearance 25 above the stock top,
top at the model top, bottom at the model bottom, stepdown 5 and stepover
3. A retyped operation keeps the numbers it already had.

The three kinds work from the model itself: the
area they work in is the model's shadow seen from above, and the levels,
surfaces and edges come from the model's facets. Deburr can narrow its
edge set with picked chains, as described below. 3D Pocket and 3D Finish take
the Boundary settings on the Geometry tab (containment, offset, extra
boundaries) to work a smaller area; **3D Pocket** also supports **Do not
touch** protected faces and edges (keeping flutes, shank and holder
clear across every cut, entry, shell lift, rest pass and rapid link at the
configured **Clearance** plus **Stock to leave**, highlighted in the
viewport while selected). Deburr has no area to bound.

**Breaking file change:** Do not touch protection uses project format
version 73. Builds that read only version 72 or earlier refuse the newer
file instead of dropping the protection. Older projects load with empty
protection. Keep an older copy when you need an older build.

In a part-aligned milling setup, **3D Pocket**, **3D Adaptive** and
**3D Finish** read the model, heights and area controls in that setup's
frame. Picked source faces and Boundary controls are supported there.
This does not enable operation-plane tilt or other unverified kinds;
those still receive named refusals. See [Setups](@/guide/set-up/setups.md).

Every kind checks its tool: 3D Pocket cuts with a flat end mill, ball-nose
or bull-nose, 3D Finish with a ball-nose only, Deburr with a chamfer mill.
A wrong tool is refused by name ("'<tool>' is a <kind>; a <operation>
operation cuts with a ..."), and a document with no suitable tool is told
to add one to the tool library.

Blue shows the operation travel; dashed orange shows approach and retract; gray shows the workpiece. These schematics are not generated NC paths and have no scale.

## 3D Pocket

<!-- catalog:examples:start -->
**Example cases**

- [3d/rough-finish](https://github.com/NeoTech/Camcraft/tree/main/examples/catalog/3d/rough-finish)
<!-- catalog:examples:end -->

{{ <icon name="op-pocket3d" /> }} ![View of roughing passes at successive levels around a 3D model.](../../../operation-paths.svg#Pocket3D)

**For:** roughing a 3D part level by level, from Top Z down to Bottom Z.
At each level the tool clears around the material that stands above it,
so the levels follow the model's shape rather than one flat floor. Choose
it over 2D Pocket when the part is a 3D shape, and leave stock for a
3D Finish to take.

**Picks:** none needed; it roughs around the whole model. Operations ahead
of it in the program count: it only cuts where their material is still
standing.

A whole-model roughing path clears out to the stock outline, including
stock far outside the model's silhouette. At an open-sided notch it runs
past the open edge so it does not leave a tool-radius ridge. Other parts
remain protected; a path that cuts one produces a warning and cannot post.
Picked and Boundary-controlled areas keep their stated limits. **3D
Adaptive** uses this same roughing path with its adaptive strategy.

| Tab | Setting | Default |
|---|---|---|
| Heights | Stepdown | 5 |
| Heights | Hop below clearance (Retract Z) | off: every link climbs to Clearance Z |
| Passes | Stepover | 3 |
| Passes | Stock to leave | 0 |
| Passes | Strategy | Auto (Zigzag, Waterline, Adaptive, Offset rings) |
| Passes | Optimal load (% of D) | about 14.6 (used by Adaptive and Offset rings) |
| Passes | Method (Adaptive only) | Spiral (Trochoidal) |
| Passes | Offset shells | off |
| Passes | Rest roughing | off |
| Passes | Feed optimization | off |
| Lead in / Lead out | Entry | Helix, radius 1 (Plunge, Ramp, Arc) |
| Lead in / Lead out | Between passes / Between levels | Keep down / Clearance plane |

Adaptive plans each level at a constant load; a level its planner refuses
is cut with Offset rings, and the path says so. Under Adaptive, **Method**
chooses Spiral or Trochoidal. Trochoidal clears each level with loops
that stay inside the area of that level. The loops enter by their own
bore helix and link over the retract plane, so the Entry and link
settings are not read on a trochoidal level. A level that the loops
cannot cut is cut by the Spiral plan, and the path names its Z. Offset
shells and Rest roughing keep Offset rings. **Offset shells** lets the
rings follow the model's surface between levels, so a steep wall is cut
like a waterline instead of a staircase. **Rest roughing** adds a closing
ladder at half the step for what the levels leave standing. **Feed
optimization** slows heavier bites below the programmed feed (never
above it).

When **3D Adaptive** falls back to Offset rings, it now raises an
**Adaptive fell back to Offset rings** warning with the area count.
Read the operation's warnings and path notes: the notes name each area
and why the adaptive planner could not cut it. For example, an offset
shell keeps Offset rings because the adaptive plan's lifted returns are
proved on a plane. The fallback warning is advisory: **the post is not
held**, because the rings still undergo verification. A collision or
gouge warning still blocks posting. To change the plan, inspect the named
area and adjust its strategy or parameters, then regenerate.

**Common refusals:** "nothing to cut: the model fills the pocket at every
level"; "nothing left to cut - the operations ahead in the program already
machined this area"; "helix radius must be positive and at most the tool
radius"; "optimal load must be in (0, 50] % of the tool diameter";
"retract plane sits inside the material".

When space remains but the cutter cannot fit, the refusal names the tool
diameter and stock to leave. Use a smaller tool or reduce the allowance
only if the job permits it. This differs from a model that fills every
level, where there is no target to clear.

Before generating an upright 3D Pocket, CamCraft checks whether a wider
shank or holder would reach material at **Bottom of Part Z**. Contact is
also refused. The message names the tool and its shank or holder band:
raise Bottom of Part Z or use a longer-reaching tool. This conservative
depth check does not credit lateral room or material removed earlier,
so a deep job can refuse even where there appears to be side clearance.

![CamCraft 3D pocket roughing waterline levels clearing stock above a curved dome surface](../../../images/guide/op-3d-pocket-roughing.png)

## 3D Finish

<!-- catalog:examples:start -->
**Example cases**

- [3d/rough-finish](https://github.com/NeoTech/Camcraft/tree/main/examples/catalog/3d/rough-finish)
<!-- catalog:examples:end -->

{{ <icon name="op-finish3d" /> }} ![View of parallel finishing passes following a curved surface.](../../../operation-paths.svg#Finish3D)

**For:** finishing 3D surfaces with a ball-nose, to a stated tolerance.
One kind carries every finishing pattern; choose the pattern by the
ground. Use it after a 3D Pocket has roughed the part.

**Picks:** none needed; the path covers the whole model. With Boundary
settings, the whole-model path is clipped to the area, and it leaves and
re-enters through the clearance plane.

**Do not touch** on **Geometry > Boundary > Boundaries** also protects
model faces or edge chains on 3D Finish. Choose **Pick protected**, pick
the items, choose **Done**, set **Clearance**, then **Apply**. CamCraft
restricts the finishing area so the full tool assembly (flutes, shank and
holder) stays clear from the cut depths up to the clearance plane.
Clearance adds to positive **Stock to leave**. The final check includes cuts,
entries, lifts and rapid links; an unsafe path is refused and cannot post.
Protected items stay highlighted while the operation is selected.

| Tab | Setting | Default |
|---|---|---|
| Heights | Top Z / Bottom Z | model top / model bottom: the first ring's plane and the last level's |
| Heights | Hop below clearance (Retract Z) | off |
| Passes | Pattern | Parallel |
| Passes | Tolerance | 0.05 |
| Passes | Stepover | Auto: from tolerance and tool by the scallop law |
| Passes | Minimum step | Auto |
| Passes | Ramp between lines / Ramp ring entries | off / off |
| Passes | Ramp angle (when ramping) | 45° |
| Passes | Steep stepover (Steep and shallow only) | Auto |
| Passes | Reference ball (Pencil only) | None: every concave corner |

For patterns that add wall rings after the shallow passes, **Passes >
Steep flanks** defaults to **Waterlines**. Select **Leave uncut**
(`waterlines: false`) to skip the rings on flanks steeper than 45°.
The operation warns that the steep flanks remain uncut. Check the
remaining stock before the next operation. Generation refuses the
operation if there is no shallow cut. Waterline, Blend and Morph
require the rings.

**Gap link** (`gap_link`) chooses how 3D Finish crosses a gap between
surface runs. **Jump**, the default, keeps the retract hop. **Straight**
tries a feed link at the runs' tip heights and checks the swept ball
against the model. Where that line would cut the model, CamCraft keeps
the Jump link. Straight therefore does not guarantee a continuous feed
across every gap. This choice applies to the raster runs; geodesic rings
keep their hops. Older projects keep Jump.

There is no stepdown: the levels drop by 2 x sqrt(tool radius x
tolerance), so the cusp under the ball stays within the tolerance.

![CamCraft 3D parallel finishing passes flowing smoothly over a spherical dome with a ball-nose cutter](../../../images/guide/op-3d-finish-parallel.png)

The patterns, in the Pattern list's order:

- **Waterline:** level rings on the walls only.
- **Parallel**, **Parallel - multi**, **Scallop:** lines over the flat
  ground first (along one axis, both axes, or the diagonals), then the
  rings take the walls. **Ramp** is Parallel with the sloped entries
  always on.
- **Radial**, **Spiral**, **Morphed spiral:** chords through the centre,
  one curve winding in, or that curve spaced by the slope under it.
- **Contour**, **Project:** concentric loops of the model's outline, or the
  outline lifted onto the surface.
- **Steep and shallow:** the rings stay off shallow ground, and the walls
  take their own spacing (Steep stepover).
- **Blend:** the lines keep the flat ground and the rings every run on
  ground past 30 degrees, so the band up to 45 degrees is cut by both and
  the border's staircase is shaved away.
- **Flat:** the floors only.
- **Pencil**, **Corner:** along each concave corner, cleaning what a
  fatter reference ball (Pencil) or the operation's own ball (Corner)
  leaves there.
- **Geodesic:** rings that march out from the model's open edge along the
  surface. A closed solid has no open edge and is refused.
- **Morph:** the waterline rings plus a guide halfway between each pair
  of levels.
- **Flow:** lines along the surface's own grain; on ground with no grain
  they become the straight raster.

**Common refusals:** "3D Finish needs a ball-nose tool, '<tool>' is a
..."; "3D finish tolerance ... mm reaches the scallop ceiling"; "no surface
reaches within one ball radius of the requested depths"; "... needs ...
levels (cap ...): refine the spacing or the tolerance"; "raster needs ...
lines at stepover ... (cap ...): widen the stepover or the tolerance";
"geodesic needs an open boundary to march from - this model is a closed
shell"; "flat-area finishing found no floor flatter than one tolerance
inside the programmed depths"; "pencil reference diameter ... does not
exceed the op's ball".

## Deburr

<!-- catalog:examples:start -->
**Example cases**

- [2d/edge-work](https://github.com/NeoTech/Camcraft/tree/main/examples/catalog/2d/edge-work)
<!-- catalog:examples:end -->

{{ <icon name="op-deburr" /> }} ![View of a toolpath following exposed edges of a 3D model.](../../../operation-paths.svg#Deburr)

**For:** breaking exposed edges of the model with a chamfer mill:
an open model's rim, or a convex crease where two faces meet. The cone
lies on the break. Choose it over 2D Chamfer for a light break on the
model's own edges at any height; choose 2D Chamfer for a chamfer of a
stated width and angle on picked edges.

**Picks:** edge chains select only the exposed edges on those chains.
With a whole model, or picks containing no edge chain, Deburr finds the
edges automatically. Face picks name the model; they do not limit the
ride to that face. Boundary settings are refused: there is no area to
bound. Sizing and pass count live on Geometry; there is no separate
Passes or Lead in / Lead out tab.

| Tab | Setting | Default |
|---|---|---|
| Geometry | Break mode | Constant width (or Constant depth) |
| Geometry | Break width | 0.5; shown in Constant width mode |
| Geometry | Break depth | 0.25; shown in Constant depth mode |
| Geometry | Break profile | Chamfer (Round is refused with the current chamfer mill) |
| Geometry | Number of passes | 1 (1 to 100) |
| Geometry | Minimum edge angle | 40 degrees for a new operation |
| Geometry | Skip round holes | off; optional minimum and maximum hole diameter |
| Geometry | Unreachable edges | Skip with note (or Refuse) |
| Heights | Window Top Z / Window Bottom Z | model top / model bottom |
| Feeds | Feed / Plunge feed | 1200 / 300 |

On **Geometry**, **Constant width** holds the chamfer's **chord width**
across the edge. **Constant depth** holds the perpendicular depth from
the original edge to the chamfer face. Neither is a direct tool-tip Z
offset: CamCraft finds the cone's contacts with both adjoining faces.
An open rim uses an orthogonal virtual second face. A face without a
valid cone contact is refused rather than assigned an approximate cut.

For example, a 0.5 mm chord on a 90-degree edge has a perpendicular
break depth of 0.25 mm. Set **Number of passes** to 2 to approach that
break in two equal size steps, from shallow to full size. Each pass
retracts to clearance before entering again. **Constant width** skips
wedges sharper than 30 degrees; **Constant depth** skips wedges flatter
than 150 degrees. The path notes report these skipped edges.

The **Window Top Z / Window Bottom Z** planes limit the tool tip, not
the edge itself. An edge running outside the window is clipped along its
own line. Every pass must fit: "a pass leaves the tip window" means widen
the window or use fewer passes. **Round** remains unavailable with a
sharp-cone chamfer mill: "a Round break profile needs a ball-tipped
cutter"; choose **Chamfer**.

**Breaking file change:** project format version 76 stores the new break
sizing. Loading a version 75 or earlier Deburr multiplies its old width
by sqrt(2), converting the old setback to chord width on a 90-degree
edge, and selects one Constant width Chamfer pass. For example, an old
0.5 mm width becomes about 0.7071 mm. Older builds cannot read the newer
project format; keep an older copy if needed and regenerate the Deburr
to check its actual face contacts before posting.

On Geometry, **Exclude the picked edges** adds picked edge chains to the
excluded list. Excluded edges stay sharp, including when they are also
picked for cutting. A missing excluded chain is refused by name.

The minimum edge angle removes shallow convex creases from the automatic
edge set. The hole filter leaves recognized round-hole rims inside the
stated diameter range. These two filters do not remove an explicitly
picked edge. Each skipped group gives a toolpath note. Older files with
no minimum angle keep zero degrees, so their automatic edge set stays
unchanged. A stored edge-chain selection now cuts only those edges and
must regenerate once.

**Unreachable edges** controls edges steeper than the cone can reach.
**Skip with note** leaves them and reports the skipped count. **Refuse**
stops generation, names the count and leaves no toolpath. Edges outside
the height window do not count. Read the notes before posting.

**Common refusals:** "Deburr rides its cone over the edge - it needs a
chamfer mill"; "the break reaches ... mm up the cone's flank; '<tool>' has
... mm of flute"; "deburr found no exposed edge inside the programmed
window"; "deburr clearance plane below the window top - the plunge would
start inside the cut".

## On a reduced-fidelity model

A STEP model imported coarser than the importer promises is marked as
reduced fidelity. 3D Pocket and 3D Finish are still checked against it,
with their tolerances tightened by how much coarser it is: stricter, never
looser. When the excess uses up a whole tolerance (for 3D Finish, its own
Tolerance), the post refuses the operation: "model '...' was imported too
coarsely to verify; re-import with a larger STEP facet budget". Deburr's
checks do not read the excess yet, so it is refused on any reduced-
fidelity model: "... this kind of operation cannot be verified against it;
re-import with a larger STEP facet budget".
