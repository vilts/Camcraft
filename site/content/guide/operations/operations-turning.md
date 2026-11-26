+++
title = "Turning operations"
description = "One section per turning kind: what it is for, its key settings and defaults, the tool it takes, and the refusals you are most likely to meet."
weight = 4
aliases = ["guide/operations-turning/"]
+++

This page is the reference for the kinds in the **Turning** group of the
[operations](@/guide/operations/_index.md) ribbon: Facing, Profile Rough,
Profile Finish, Groove Rough, Groove Finish, Part, Threading, Chamfer,
Taper, Multi-Pass, Form and Bore. The ribbon shows them on a lathe or
mill-turn (live tooling) document. For the tabs themselves, see that page; for what a
refusal means in general, see [Reading a refusal](@/guide/reference/refusals.md).

> **Two meanings of "mill-turn".**
>
> - **Mill-turn (live tooling)**: a lathe, or a multitasking turning
>   centre, that also mills with driven tools in its turret, usually with
>   a C axis and sometimes a Y axis. This is the machine kind that
>   CamCraft calls **Mill-turn**: the built-in `millturn.mch`
>   (Multitasking 5-axis) and the **Mill-turning** tab on the ribbon.
> - **Turning on a mill**: a mill used as a lathe. The part turns in the
>   mill spindle, the lathe tools stand on the table, and each tool has
>   its own work offset. CamCraft does not support this yet (#2580).
>
> In this guide, "mill-turn" always means the live-tooling kind.

Lengths are in mm (inches in an inch document), feeds in mm/min (in/min).
The defaults below are what a freshly created operation starts with:
feed 1200, plunge feed 300, clearance 25 above the stock top, Top of
Part Z at the model top, the low height (Bottom of Part Z, Facing Plane
Z or Parting Plane Z) at the model bottom, and tolerance 0.05. A retyped
operation keeps the numbers it already had.

Blue shows the operation travel; dashed orange shows approach and retract; gray shows the workpiece. These schematics are not generated NC paths and have no scale.

## What every turning kind shares

**No picks.** A turning kind works from the model turned about its axis:
its limit is the turned profile's own silhouette, read between the two
heights. The Geometry tab carries only the tool, and the Boundary
settings do not apply.

**The tool.** Each kind checks its tool kind: Facing, Profile Rough,
Profile Finish, Chamfer, Taper and Multi-Pass take a turning insert;
Groove Rough, Groove Finish and Part a grooving blade; Threading a
threading insert; Form a form gauge; Bore a boring bar. A wrong tool is
refused by name ("'<tool>' is a <kind>; a <operation> operation cuts
with a ..."). The insert's nose radius, the blade's width, the thread
insert's width and form angle, and the form gauge's band width and crown
radius are set on the tool, not on the operation.

**The spindle axis.** The lathe programs every diameter about world Z
through the work offset. A part whose axis leans off world Z, or stands
off the origin, by more than the post can write is refused, before
generation and again at the post: "the part's axis of revolution is not
the lathe spindle: it leans ... degrees off world Z ...; align the part's
axis with world Z and move the work offset onto it".

**Surface speed and feed per rev.** A turning insert, grooving blade,
form gauge or boring bar can state turning data on the tool page:
**Surface speed** (m/min, ft/min in an inch document), **Feed per rev**
(mm/rev) and its own **Max rpm**. With it stated, every turning kind
except Threading posts constant surface speed (G96); without it, the
tool's rpm is posted (G97). Threading always runs at a constant rpm: a
synchronized pass needs one. The Feeds tab's suggestion reads the stock's
swing diameter; applied, it sets the mm/min feeds that post as the
table's feed per rev.

**The rpm ceiling.** A setup can state its chuck's or collet's rated
maximum rpm (the workholding row on the Setups page). Constant surface
speed is clamped to the lowest of the machine's, the tool's and that
limit. A constant-rpm section over it is refused at the post, never
quietly capped: "<op>: turns the work at ... rpm (G97), over setup
'...''s workholding maximum of ... rpm - lower tool ...'s rpm or state
the chuck's real rating".

**The holder.** A lathe-held tool's body is checked on every move against
the stock as it stands at that instant; only the cutting footprint may
touch the material.

**Not on the work spindle.** A lathe post turns the work for the turning
kinds and for a Drill on the spindle axis. A milling kind sent to a lathe
post is refused ("a <kind> operation would be posted on the work spindle
- ..."), unless its path states a C angle: that is live-tool work, with
the tool turning and the work held on C.

## Facing

<!-- catalog:examples:start -->
**Example cases**

- [turning/cone-thread](https://github.com/NeoTech/Camcraft/tree/main/examples/catalog/turning/cone-thread)
<!-- catalog:examples:end -->

{{ <icon name="op-facing" /> }} ![Side view of radial passes facing the end of a rotating bar.](../../../operation-paths.svg#Facing)

**For:** facing the end of the bar square to the axis, stepping in from
the part top to the facing plane. Choose it to clean the end before
profiling; a profile kind turns along the bar, not across its end.

| Tab | Setting | Default |
|---|---|---|
| Heights | Clearance Z / Top of Part Z / Facing Plane Z | 25 above stock top / model top / model bottom |
| Heights | Stepdown (axial infeed per rung) | 0.5 |
| Passes | Tolerance | 0.05 |
| Feeds | Feed / Plunge feed | 1200 / 300 |

The stepdown must stay under twice the insert's nose radius, or the
ridge between rungs would stand.

**Common refusals:** "a stepdown of ... is past the nose's reach ... -
tighten the stepdown"; "facing is an arc, not a line - a nose radius of
... is a mathematical point, and a point cannot face"; "nothing to face -
the facing plane stands at or above the top of both the part and its
stock"; "the facing ladder would run ... stations - the stepdown is far
too fine ...".

## Profile Rough

<!-- catalog:examples:start -->
**Example cases**

- [turning/cone-thread](https://github.com/NeoTech/Camcraft/tree/main/examples/catalog/turning/cone-thread)
<!-- catalog:examples:end -->

{{ <icon name="op-profile-rough" /> }} ![Side view of successive roughing passes outside a turned profile.](../../../operation-paths.svg#ProfileRough)

**For:** roughing the outside profile down a ladder of radial rungs,
leaving a skin for a finish pass. Choose it before Profile Finish; choose
Multi-Pass for roughing and finishing in one operation.

| Tab | Setting | Default |
|---|---|---|
| Heights | Clearance Z / Top of Part Z / Bottom of Part Z | 25 above stock top / model top / model bottom |
| Heights | Stepdown (radial shell per rung) | 0.5 |
| Heights | Stock to leave | 0.25 |
| Passes | Tolerance | 0.05 |
| Feeds | Feed / Plunge feed | 1200 / 300 |

The tool plunges in at one end of the window and slides out at the
other.

**Common refusals:** "a stepdown of ... is past the nose's reach: a ball
of radius ... clears a shell ... thick ... - tighten the stepdown";
"stock to leave of ... is stock to remove - the finish pass owns the
profile itself ..."; "nothing to turn - the height window holds no
length of the bar"; "the roughing ladder holds no rung"; "the roughing
ladder would run ... radial rungs - the stepdown is far too ...".

## Profile Finish

<!-- catalog:examples:start -->
**Example cases**

- [turning/cone-thread](https://github.com/NeoTech/Camcraft/tree/main/examples/catalog/turning/cone-thread)
<!-- catalog:examples:end -->

{{ <icon name="op-profile-finish" /> }} ![Side view of one finishing pass along a turned profile.](../../../operation-paths.svg#ProfileFinish)

**For:** one ride of the insert's nose along the finished profile.
Choose it after Profile Rough (or after a stock that is already close).

| Tab | Setting | Default |
|---|---|---|
| Heights | Clearance Z / Top of Part Z / Bottom of Part Z | 25 above stock top / model top / model bottom |
| Passes | Tolerance | 0.05 |
| Feeds | Feed / Plunge feed | 1200 / 300 |

There is no stepdown: one ride is the pattern.

**Common refusals:** "profiling is an arc, not a line - a nose radius of
... is a mathematical point, and a point cannot ride a profile"; "nothing
to turn - the height window holds no length of the bar"; "the turned
profile has nothing to ride".

## Groove Rough

<!-- catalog:examples:start -->
**Example cases**

- [turning/groove-part-off](https://github.com/NeoTech/Camcraft/tree/main/examples/catalog/turning/groove-part-off)
<!-- catalog:examples:end -->

{{ <icon name="op-groove-rough" /> }} ![Side view of successive blade plunges across a groove.](../../../operation-paths.svg#GrooveRough)

**For:** sinking a groove with a grooving blade, plunge by plunge, with a
skin left on the walls and floor. Choose it over Profile Rough where the
profile is undercut (a groove a nose insert cannot reach); finish with
Groove Finish.

| Tab | Setting | Default |
|---|---|---|
| Heights | Clearance Z / Top of Part Z / Bottom of Part Z | 25 above stock top / model top / model bottom |
| Heights | Stepdown | 0.5 |
| Heights | Stock to leave | 0.25 |
| Passes | Tolerance | 0.05 |
| Feeds | Plunge feed | 300 |

The blade's width is the kerf: the groove is cut by centres across the
window.

**Common refusals:** "the window is ... long and the blade ... wide -
narrower than the blade: a groove cannot fit where it was asked"; "a
blade of width ... cuts no band ..."; "stock to leave of ... is stock to
remove - the floor is the floor; negative leave is not a thing";
"grooving needs a positive stepdown"; "the center sweep would run more
than ... passes - the blade is far too narrow for the window ...".

## Groove Finish

<!-- catalog:examples:start -->
**Example cases**

- [turning/cone-thread](https://github.com/NeoTech/Camcraft/tree/main/examples/catalog/turning/cone-thread)
- [turning/groove-part-off](https://github.com/NeoTech/Camcraft/tree/main/examples/catalog/turning/groove-part-off)
<!-- catalog:examples:end -->

{{ <icon name="op-groove-finish" /> }} ![Side view of blade plunges to the finished groove floor.](../../../operation-paths.svg#GrooveFinish)

**For:** one plunge per centre to the groove's floor, with no leave.
Choose it after Groove Rough.

| Tab | Setting | Default |
|---|---|---|
| Heights | Clearance Z / Top of Part Z / Bottom of Part Z | 25 above stock top / model top / model bottom |
| Passes | Tolerance | 0.05 |
| Feeds | Plunge feed | 300 |

**Common refusals:** the window and blade refusals of Groove Rough, and
"nothing to groove - the height window holds no length of the bar".

## Part

<!-- catalog:examples:start -->
**Example cases**

- [turning/cone-thread](https://github.com/NeoTech/Camcraft/tree/main/examples/catalog/turning/cone-thread)
- [turning/groove-part-off](https://github.com/NeoTech/Camcraft/tree/main/examples/catalog/turning/groove-part-off)
<!-- catalog:examples:end -->

{{ <icon name="op-part-off" /> }} ![Side view of one radial parting path toward the spindle axis.](../../../operation-paths.svg#Part)

**For:** cutting the finished part off the bar with a grooving blade, at
the parting plane. The ladder runs down through the bar's full height and
the last rung puts the tip on the axis. Choose it last.

| Tab | Setting | Default |
|---|---|---|
| Heights | Clearance Z / Parting Plane Z | 25 above stock top / model bottom |
| Heights | Stepdown (radial infeed per rung) | 0.5 |
| Passes | Tolerance | 0.05 |
| Passes | The part is | Cut-off piece |
| Passes | Back stock | 0 |
| Feeds | Plunge feed | 300 |
| Bar feeder | Load bar before the cut / Bar advance | off / Feeder stop |
| Bar feeder | Unload part after the cut / Transfer to sub-spindle | off / off |
| Bar feeder | Probe budget | Off |

**The part is** selects which piece is the finished part. **Cut-off piece**
keeps the piece cut off the free end. **Chuck piece** keeps the piece in the
chuck and cuts a stub off the free end. The whole kerf stays on the scrap
side of the parting plane.

**Back stock** moves the whole kerf away from the parting plane. The part
keeps this much material on its end face for a later facing pass. Zero puts
the blade's flank on the plane.

The Bar feeder tab shapes no toolpath: it says what the posted program
signals around the cut. **Transfer to sub-spindle** is refused at the
program gate on a machine that states no second spindle. A **Probe
budget** posts a measurement of the finished part, and holds the post
when material stands above the model past it.

**Common refusals:** "nothing to part - the parting plane stands off the
end of the bar"; "nothing to part - the parting plane stands in the air
past the bar's top"; "parting needs a positive stepdown".

## Threading

<!-- catalog:examples:start -->
**Example cases**

- [turning/cone-thread](https://github.com/NeoTech/Camcraft/tree/main/examples/catalog/turning/cone-thread)
<!-- catalog:examples:end -->

{{ <icon name="op-threading" /> }} ![View of synchronized helical threading passes on a rotating bar.](../../../operation-paths.svg#Threading)

**For:** single-point threading of a modeled thread (a revolved V
section) with a threading insert, in passes of rising depth. Every pass
runs toward the chuck.

| Tab | Setting | Default |
|---|---|---|
| Heights | Clearance Z / Top of Part Z / Bottom of Part Z | 25 above stock top / model top / model bottom |
| Passes | Tolerance | 0.05 |
| Passes | Pitch | 2 |
| Passes | Depth | 1.08 |
| Passes | Infeed | 0.4 |
| Passes | Hand | Right hand |
| Feeds | Feed / Plunge feed | 1200 / 300 |

The V's angle comes from the tool (its form angle), not the operation.
Right hand posts under M03; left hand under M04, with the insert mounted
for reverse rotation. Threading posts a constant rpm (G97).

**Repeat each depth**, off by default on Passes, repeats each roughing
pass at the same depth, flank and helix phase. It does not repeat the
finishing or root spring passes. G32/G33 output carries those repeated
passes; Fanuc and Tormach PathPilot G76 cycles refuse the option because
their cycles cut each roughing depth once.

With the Fanuc **G76** post option, the finish allowance and spring count
describe the verified passes. The default metric thread now writes
`R0.4`, rather than `R0`, to avoid an extra full-depth pass. G32 remains
the default output. G76 refuses alternating infeed, a one-pass thread,
or a doubled flank angle outside 0, 29, 30, 55, 60 or 80 degrees. It also
refuses a rounded-word schedule that cannot match the verified pass count.
These are software checks; Fanuc flank G76 behavior still needs a control
run. Read the program and confirm the control's cycle before machining.

**Common refusals:** "nothing to thread - no valley ... down from the
bar's surface stands in the window: threading cuts a MODELED thread ...";
"a ... wide insert does not fit a ... pitch - the V is wider than the
groove it was asked to cut ..."; "a thread ... deep on a bar of ... crest
radius runs past the axis ..."; "a thread ... deep within its own
tolerance ... has no valley that can be told from its crest - deepen the
thread or tighten the tolerance"; "the bar's axis stands ... degrees off
the spindle ..."; "the thread would run more than ... turns ...";
"threading needs a positive pitch" (or depth, or infeed).

## Chamfer

<!-- catalog:examples:start -->
**Example cases**

- [turning/contour-finishes](https://github.com/NeoTech/Camcraft/tree/main/examples/catalog/turning/contour-finishes)
<!-- catalog:examples:end -->

{{ <icon name="op-chamfering" /> }} ![Side view of a turning pass across a modeled chamfer.](../../../operation-paths.svg#Chamfering)

**For:** riding a chamfer the model already has, at a stated angle, with
a turning insert. Choose it over Profile Finish to cut one modeled cone
on its own, inside a narrow window.

| Tab | Setting | Default |
|---|---|---|
| Heights | Clearance Z / Top of Part Z / Bottom of Part Z | 25 above stock top / model top / model bottom |
| Passes | Tolerance | 0.05 |
| Passes | Angle from cylinder | 45° |
| Feeds | Feed / Plunge feed | 1200 / 300 |

**Common refusals:** "the model is not a ...-degree chamfer in this
window - its skin stands ... off the authored cone at station ... ;
model the face or narrow the window to it"; "a chamfer of ... degrees is
a face - facing is its own kind"; "a chamfer of ... degrees is a
cylinder - the kind only speaks a cone ...".

## Taper

<!-- catalog:examples:start -->
**Example cases**

- [turning/taper-finish](https://github.com/NeoTech/Camcraft/tree/main/examples/catalog/turning/taper-finish)
<!-- catalog:examples:end -->

{{ <icon name="op-taper-turning" /> }} ![Side view of a turning pass along a modeled taper.](../../../operation-paths.svg#TaperTurning)

**For:** riding a modeled taper between two stated diameters with a
turning insert.

| Tab | Setting | Default |
|---|---|---|
| Heights | Clearance Z / Top of Part Z / Bottom of Part Z | 25 above stock top / model top / model bottom |
| Passes | Tolerance | 0.05 |
| Passes | Bottom Diameter / Top Diameter | 20 / 16 |
| Feeds | Feed / Plunge feed | 1200 / 300 |

The diameters are a promise the model must keep within the tolerance.

**Common refusals:** "the model is not the authored cone - its skin
stands ... off the taper line at station ... ; rough the bar near it or
profile the cone into the model"; "a diameter of ... is not a size - the
taper's ends stand on the axis or ..."; "a taper that stands on the axis
at both ends is a line, not a part ...".

## Multi-Pass

<!-- catalog:examples:start -->
**Example cases**

- [turning/contour-finishes](https://github.com/NeoTech/Camcraft/tree/main/examples/catalog/turning/contour-finishes)
<!-- catalog:examples:end -->

{{ <icon name="op-multipass-turning" /> }} ![Side view of roughing and finishing passes along a turned profile.](../../../operation-paths.svg#MultiPassTurning)

**For:** roughing and finishing the outside profile in one operation: the
Profile Rough ladder, then finishing rides that share out the leave.
Choose it over a Profile Rough plus Profile Finish pair when one tool
does both.

| Tab | Setting | Default |
|---|---|---|
| Heights | Clearance Z / Top of Part Z / Bottom of Part Z | 25 above stock top / model top / model bottom |
| Passes | Tolerance | 0.05 |
| Passes | Stepdown | 0.5 |
| Passes | Stock to leave | 0.25 |
| Passes | Finishing passes | 3 |
| Feeds | Feed / Plunge feed | 1200 / 300 |

**Common refusals:** "multi-pass turning with no finishing ride leaves
the stock standing - the finish owns the profile"; "... finishing rides
is a polishing sequence, not a turning program - the kind caps the
ladders at 64"; the stepdown and stock-to-leave refusals of Profile
Rough.

## Form

<!-- catalog:examples:start -->
**Example cases**

- [turning/bushing](https://github.com/NeoTech/Camcraft/tree/main/examples/catalog/turning/bushing)
<!-- catalog:examples:end -->

{{ <icon name="op-form-turning" /> }} ![Side view of a form tool plunging into a curved groove.](../../../operation-paths.svg#FormTurning)

**For:** sinking a modeled form (a revolved groove with a curved floor)
with a form gauge whose ground crown matches it. Choose it over Groove
Rough when the floor is an arc the gauge carries.

| Tab | Setting | Default |
|---|---|---|
| Heights | Clearance Z / Top of Part Z / Bottom of Part Z | 25 above stock top / model top / model bottom |
| Passes | Tolerance | 0.05 |
| Feeds | Feed / Plunge feed | 1200 / 300 |

There is no depth to set: the model states it, and the crown comes from
the tool.

**Common refusals:** "nothing to form - no surface stands ... down from
the bar in this window ..."; "a crown of radius ... is no arc at all - a
flat floor is a wide blade ..."; "a ... wide band is ground past the
reach of its own ... crown ... grind narrower or crown shallower"; "the
window holds ... formed surfaces - more than this kernel will program in
one operation; split the window"; "the crown that fits the valley at
station ... stands on the axis or through it - that is a hole, and holes
are boring work".

## Bore

<!-- catalog:examples:start -->
**Example cases**

- [turning/bushing](https://github.com/NeoTech/Camcraft/tree/main/examples/catalog/turning/bushing)
<!-- catalog:examples:end -->

{{ <icon name="op-boring-turning" /> }} ![Side view of a boring bar following an existing hole wall.](../../../operation-paths.svg#BoringTurning)

**For:** turning the inside wall of a hole the part already has, with a
boring bar riding the wall on its nose. Choose it for an inside diameter
the model has; where the model is solid on the axis it refuses, since
boring does not drill.

| Tab | Setting | Default |
|---|---|---|
| Heights | Clearance Z / Top of Part Z / Bottom of Part Z | 25 above stock top / model top / model bottom |
| Passes | Tolerance | 0.05 |
| Feeds | Feed / Plunge feed | 1200 / 300 |

The size the bore holds comes from the bar's nose radius, set on the
tool.

**Common refusals:** "nothing to bore - no hole's air stands open in the
window: at station ... the axis is solid metal, and drilling is not this
op's promise"; "the window's bottom stands within the nose's reach of the
hole's floor ..."; "the window's top stands within the nose's reach of a
ceiling ... lower top Z to at most ..."; "nothing to bore - the window
holds no wall off the axis".
