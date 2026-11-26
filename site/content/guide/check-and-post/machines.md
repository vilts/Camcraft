+++
title = "Machine types"
description = "Mill, router, lathe, mill-turn (live tooling), plasma and EDM: what each allows and how it changes setups and posting."
weight = 3
aliases = ["guide/machines/"]
+++

A machine file (`.mch`) declares a **kind**: `mill`, `router`, `lathe`, `mill-turn`,
`plasma`, `wire_edm`, `sinker_edm` or `waterjet`. A file that declares
nothing is read from its axes (a mill unless it has a lathe's X/Z pair or
a turning rotary); plasma, EDM and waterjet are never guessed, only
declared. The kind decides
which operations the document accepts, which posts the picker lists and
which limits the program is checked against. See [Setups](@/guide/set-up/setups.md)
for mounting a machine and [Reading a refusal](@/guide/reference/refusals.md) for the
messages below.

| Kind | Operation families allowed | Posts that serve it | Shipped machine files |
|---|---|---|---|
| Mill | Milling 2D, Milling 3D, Drilling, Multi-axis | every milling post | VMC 3-axis, Tilt-head 5-axis, Tormach, Carvera |
| Router | the same operations as Mill, with router options | every milling post | Tormach 24R, xsTECH, Sienci AltMill |
| Lathe | Turning, plus the axial Drill | Fanuc Lathe, Tormach PathPilot Lathe | Turning center 2-axis, Tormach 8L, 15L, 15LX |
| Mill-turn (live tooling) | milling and turning | every post except plasma | Multitasking 5-axis |
| Plasma | Plasma Profile only | Tormach PathPilot Plasma | Tormach 1300PL |
| Wire / sinker EDM | Wire Contour (wire EDM); Sinker Burn, Side Burn (sinker EDM) | Generic ISO Wire EDM, Sodick Wire EDM (Mark 21), G-EDM Wire EDM; Sinker EDM (ISO) | Sodick ALC400G (complete its EDM data); supply a sinker machine file |

A **waterjet** machine file needs linear X and Y axes; Z is optional. It
is refused by name if it states a spindle, live tooling, a rotary axis or
milling coolant. The matching **Waterjet nozzle** tool kind turns no spindle
(its rpm must be zero), needs a positive kerf, and can hold cut recipes per
material and thickness: cut feed, kerf, pierce time, abrasive delay,
re-pierce time and pierce method. The machine check judges X/Y travel and
feed limits, including arc extremes, and says which Z and spindle checks do
not apply. The [Waterjet Profile](@/guide/operations/operations-waterjet.md)
operation cuts on it, and the generic ISO abrasive waterjet post
(`waterjet-iso.cps`) posts it.

The picker lists posts for the setup's machine kind. A loaded post also
refuses by name any operation or move that it cannot express.

## Mill

The default. The ribbon shows the milling sections (2D, 3D, Drilling,
Multi-axis). Rotary axes in the file unlock the rotary and multi-axis
operations; an axis that states a leaning direction (a tilting head or
trunnion) tilts the cutter, while a C table only turns the part. A program
that asks for a tilt the machine cannot hold is refused by name.

For setting up 3+2, continuous 4-axis and 5-axis work, which machines and
posts allow it and what is checked, see
[4-axis and 5-axis machining](@/guide/multi-axis/_index.md).

Checked against the machine file at post, when it states them: axis travel
(arc bulges included), feed ceilings, spindle rpm range, spindle power,
tool length and diameter against the tool table, changer capacity, and
rotary letters and angles.

Posts: Fanuc (the default), Haas, Siemens 840D, Heidenhain, Mazak,
Tormach PathPilot, Doosan DNM and Makera Carvera. Machine files:
`3axis-vmc`, `vmc`, `tilthead`, the Carvera pair and the Tormach mills.
To exchange tool tables with Tormach PathPilot (`.csv`) or Hurco WinMax
(`.TCM`), see
[Machine tool tables](@/guide/set-up/tools.md#machine-tool-tables-tormach-pathpilot-and-hurco-winmax).
**Doosan DNM 5700** is the 3-axis machine; **Doosan DNM 5700 + rotary
table** is the separate rotary configuration. Pick the one that matches
your installed hardware.

The {{ <icon name="tree-machine" /> }} Machine page has a **Library** picker and **Load other**, **Edit** and
**Clear** actions. The overview shows the file's stated limits. A blank
means **not stated**. The editor names invalid inputs and blocks saving
until all refusals are fixed.

In the browser's View controls, select **Show work envelope** to draw the
active setup's machine travel as an outline. Mount a machine first. The
outline uses its stated travel and the work origin. A mill's outline
stands on the table plane (Z = 0) and keeps its full Z travel height,
even when the machine's program-space travel reaches below the table. X
and Y keep the setup's work origin as their reference; the tooltip states
these references. Missing travel data
produces a reason instead of a guessed volume. This is a display aid;
posting still checks the path against the machine limits.

Common refusals: `a <kind> operation cannot run on its setup's milling
machine` (a turning or plasma operation on a mill), `move <i> tilts the
tool; <machine> states no tilting rotary`, and `move <i> commands <axis>
<angle> deg, the travel is <lo>..<hi>`.

## LinuxCNC 3-axis

For a LinuxCNC mill, mount **LinuxCNC 3-axis**
(`linuxcnc-3axis.mch`) and select the **LinuxCNC** post
(`linuxcnc.cps`, program extension `.ngc`). This is a generic example:
check the travel, feeds and spindle limits against the real machine.
It supports three linear axes; rotary work, tilted moves and control
compensation G41/G42 are refused.

The default retract is **G53** to the machine's stated `safe_retract` Z.
The generic file deliberately supplies none: posting refuses until you
state a safe retract or select a suitable alternative. **G28** and **G30**
use the controller's stored Z position only. **None** leaves that move to
the controller or M6 remap. Check those controller settings before use.

The post normally writes G43 H after M6. Turn off **Write G43 H after
M6** only when the remap applies tool length itself. Path control defaults
to G64 P with a 0.025 mm blend tolerance; G61 requests exact path.
Rigid tapping uses G33.1 and requires stated rigid-tapping support and a
spindle encoder. The generic file makes no such claim. Floating-holder
tapping uses G84.

## Router

A router declares `router` in its machine file. It follows the mill's
operation, posting and safety rules, using its stated travel, spindle
range and tool table. The Tormach 24R, xsTECH and Sienci AltMill files
declare this kind.

Router options stay on the **Milling** tab; there is no separate Router
tab. On the Setups page, **Sheet job** groups the sheet size, thickness,
Z zero, spoilboard allowance and clamp clearance. The 2D clearing pages
offer **Corner relief** for dogbone and T-bone reliefs. These controls are
also offered when no machine is mounted. A plain mill does not offer a
new router option, but switching machines keeps existing sheet jobs and
relief settings available instead of hiding or discarding them.

For example, mount **Sienci AltMill 4x4**, then use **Sheet job** to state
an 18 mm sheet with Z zero at its top and a 0.3 mm through-cut allowance.
Its deepest cut is then Z -18.3 mm. State the real hold-down clearance
before applying. Mounting a router does not relax the mill's checks:
an out-of-travel move or excessive spindle rpm is refused by name.

The library also supplies **Sienci AltMill 2x4**, **4x4** and **4x8**
machines, paired with the **grblHAL** post. **grblHAL 3-axis** and
**TinyG 3-axis** are generic controller examples. Check their travel,
spindle and retract settings against your actual machine before use. For a
complete sheet-goods walkthrough on the Sienci AltMill 4x4 (plus shop-router
notes for RichAuto DSP A11), see
[Your first router part](@/guide/getting-started/first-router-part.md).

## Lathe

The work spindle turns the part; X is a diameter. The ribbon shows Drilling
(the axial Drill) and Turning. Milling operations are refused on a lathe,
and the Drill is admitted only because a hole down the spindle axis is the
turret's simplest hole: a hole off the axis is refused by name.

A lathe setup can state a **workholding rpm limit**, the chuck or collet's
rated maximum. Under constant surface speed (G96) the posted rpm ceiling is
the lowest of the machine's, the tool's and the workholding's. Under
constant rpm (G97) an operation over the limit is refused at the program
gate, never silently capped. Slide strokes are judged on the posted path.

The Tormach 8L, 15L Slant-PRO and 15LX Slant-PRO files state rigid
tapping (from their operator manuals), so the Machines pane shows **yes**
for them. In the viewport, a lathe's spindle axis lies from left to right
and its work envelope is a cylinder about that axis.

Posts: Fanuc Lathe (the default for a lathe) and Tormach PathPilot Lathe.

Common refusals: `a Drill operation would be posted on the work spindle
with a hole off the spindle axis (X.. Y.. from the work offset)`, `turns
the work at <rpm> rpm (G97), over setup '<name>''s workholding maximum of
<limit> rpm`, `move <i> reaches <v> in <axis>, the stroke is <lo>..<hi>`,
and `<post> does not serve <machine>, a turning machine` (a milling post
on a lathe setup).

## Mill-turn

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

Both families are admitted: turning on the work spindle and milling with
live tooling on the tool spindle (the C axis). Its posts are every post
except plasma. For a live-tool move the machine's C rate is what is judged,
and the work's chuck rpm limit applies to sections that run on the work
spindle. Each shipped post script declares its dialect in
`capabilities` (1 = milling, 2 = turning).

A machine can state its C rotary axis along -Z in its `.mch` file. CamCraft
then reads positive C as clockwise about +Z. It flips an asymmetric C
travel range for the machine check, and the Fanuc Lathe post writes C
negated. The setup sheet says so under MACHINE AXES: "C rotary turns
clockwise about +Z (-Z axis) - posted C is negated for this machine". A C
axis that is not along Z, or whose axis is not a finite vector, is
refused by name. Curved (arc) moves can now ride a C turn: CamCraft
breaks them into short chords within tolerance.

Common refusals: `<post> is not a turning post - on <machine>, a mill-turning
machine, these sections turn the work` (turning sections through a milling
post), the same off-axis Drill refusal as a lathe, and `turns the work on
the C axis at up to <rpm> rpm ... over setup '<name>''s workholding
maximum`.

## Plasma

A tool-less cut: no spindle, so rpm and spindle power are not judged, and a
plasma machine file that states a spindle or a rotary is refused. Z is the
torch lifter, owned by the control (pierce and cut height), so Z depth and
Z travel are skipped with a named note. XY travel, XY feed and the changer
count are checked. Only the Plasma Profile operation exists, and it is
admitted on a plasma table only (not on a mill-turn machine). A spindle word, a
tilted or a turned move is refused, and the plasma laws (cut order,
crossing, kerf, tip-up) run at the post gate. The 3D stock simulation does
not run for a torch.

Post: Tormach PathPilot Plasma, the default, and the only post that serves
plasma. Machine file: `tormach-1300pl`.

Common refusals: `a <kind> operation is <family> work - a plasma table
cuts only plasma operations`, `tool <n> states <rpm> rpm - a plasma table
has no spindle to turn`, `move <i> tilts or turns the tool - <machine>
states no rotary axis`, and, when the machine file is read, `a plasma
machine has no spindle - a torch states no rpm, power, speed range or
tapping` and `a plasma table cuts flat sheet in X and Y - it states no
rotary axis`.

## Wire and sinker EDM

The library includes **Sodick ALC400G** (`sodick-alc400g.mch`): X/Y
travel is 400/300 mm and U/V travel is 150/150 mm, centred on zero in
the file. Its stated taper maximum is 25 degrees at 130 mm work height.
The file ships an example starter set (#3214): the conditions `C110`,
`C120` and `C130`, guide heights 0 and 130 mm, and automatic threading.
These are not Sodick figures. Replace them with the rows of your
control's condition table and your machine's guide heights before you
cut ([Conditions](@/guide/operations/operations-edm.md#wire-edm-the-machine-the-wire-and-the-post)).
The file supplies no wire diameter list and no EDM codes. The Sodick
post writes a vertical 2-axis cut only, so it refuses a taper.
The ALC400G control has not been confirmed to match the Mark 21 post.

Wire Contour runs on a machine file that declares `wire_edm`. The
**Generic ISO Wire EDM** post takes its EDM words from the file's
`edm.codes`. A user `.cps` file can also declare `wireEdm` in its manifest.
The **Sodick Wire EDM (Mark 21)** post writes 2-axis contours. It refuses
taper and 4-axis cuts. Its output has not been verified on a Sodick control.
The **G-EDM Wire EDM** post writes one 2-axis contour for a G-EDM
controller (G-RBLv3). It needs no `edm.codes`: you set the spark power at
the controller. Thread the wire at the start point before you start the
program. It refuses taper, 4-axis cuts, skims and wire offset (G41 / G42).
Its output has not been verified on a G-EDM controller.

For an angle-word taper, the machine must state `edm.taper_angle`,
including its side rule and mitre or cone corner rule. The generic wire
post writes that stated angle word. Verification uses the same corner
rule. Do not substitute another control's codes or corner rule.

The sinker EDM kind admits Sinker Burn and Sinker Side Burn on a machine
file that declares `sinker_edm`. **Sinker EDM (ISO)** takes every EDM code
from the file's `edm.codes`. The Machine window's sinker condition table
lets you edit each condition's wear rate and notes. Use measured data for
your electrode and work material.
The operations, their settings and their refusals are in
[Wire and sinker EDM](@/guide/operations/operations-edm.md).

## Changing a setup's machine

Operations are judged against the machine at post, so switching a setup to
another machine does not rewrite its operations; an operation the new kind
does not admit, or a limit the new file states, is refused when you post.
A post that does not serve the machine's kind is filtered out of the
picker and, if mounted anyway, refused by name at post (`<post> does not
serve <machine>, a <kind> machine`).
