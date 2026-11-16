+++
title = "Machine types"
description = "Mill, router, lathe, mill-turn, plasma and EDM: what each allows and how it changes setups and posting."
weight = 3
aliases = ["guide/machines/"]
+++

A machine file (`.mch`) declares a **kind**: `mill`, `lathe`, `mill-turn`,
`plasma`, `wire_edm` or `sinker_edm`. A file that declares nothing is read
from its axes (a mill unless it has a lathe's X/Z pair or a turning
rotary); plasma and EDM are never guessed, only declared. The kind decides
which operations the document accepts, which posts the picker lists and
which limits the program is checked against. See [Setups](@/guide/set-up/setups.md)
for mounting a machine and [Reading a refusal](@/guide/reference/refusals.md) for the
messages below.

| Kind | Operation families allowed | Posts that serve it | Shipped machine files |
|---|---|---|---|
| Mill | Milling 2D, Milling 3D, Drilling, Multi-axis | every milling post | VMC 3-axis, Tilt-head 5-axis, Tormach, Carvera |
| Lathe | Turning, plus the axial Drill | Fanuc Lathe, Tormach PathPilot Lathe | Turning center 2-axis, Tormach 8L, 15L, 15LX |
| Mill-turn | milling and turning | every post except plasma | Multitasking 5-axis |
| Plasma | Plasma Profile only | Tormach PathPilot Plasma | Tormach 1300PL |
| Wire / sinker EDM | Wire Contour (wire EDM); Sinker Burn, Side Burn (sinker EDM) | Generic ISO Wire EDM, Sodick Wire EDM (Mark 21); Sinker EDM (ISO) | supply a machine file with the required EDM data |

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
**Doosan DNM 5700** is the 3-axis machine; **Doosan DNM 5700 + rotary
table** is the separate rotary configuration. Pick the one that matches
your installed hardware.

The Machine page has a **Library** picker and **Load other**, **Edit** and
**Clear** actions. The overview shows the file's stated limits. A blank
means **not stated**. The editor names invalid inputs and blocks saving
until all refusals are fixed.

In the browser's View controls, select **Show work envelope** to draw the
active setup's machine travel as an outline. Mount a machine first. The
outline uses its stated travel and the work origin. Missing travel data
produces a reason instead of a guessed volume. This is a display aid;
posting still checks the path against the machine limits.

Common refusals: `a <kind> operation cannot run on its setup's milling
machine` (a turning or plasma operation on a mill), `move <i> tilts the
tool; <machine> states no tilting rotary`, and `move <i> commands <axis>
<angle> deg, the travel is <lo>..<hi>`.

## Router

CamCraft has no separate router kind: a router is a machine file that
declares `mill`, so everything above applies. Nothing in the code treats a
router differently, so there are no router-only operations or limits; its
travel, spindle range and tool table are whatever its file states. The
Tormach 24R and xsTECH files in `assets/machines/` are mills in this sense.
Sheet-goods practice (spoilboard, tabs, hold-down) is handled by the
ordinary milling operations and the setup's fixtures, not by the machine.

The library also supplies **Sienci AltMill 2x4**, **4x4** and **4x8**
machines, paired with the **grblHAL** post. **grblHAL 3-axis** and
**TinyG 3-axis** are generic controller examples. Check their travel,
spindle and retract settings against your actual machine before use.

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

Posts: Fanuc Lathe (the default for a lathe) and Tormach PathPilot Lathe.

Common refusals: `a Drill operation would be posted on the work spindle
with a hole off the spindle axis (X.. Y.. from the work offset)`, `turns
the work at <rpm> rpm (G97), over setup '<name>''s workholding maximum of
<limit> rpm`, `move <i> reaches <v> in <axis>, the stroke is <lo>..<hi>`,
and `<post> does not serve <machine>, a turning machine` (a milling post
on a lathe setup).

## Mill-turn

Both families are admitted: turning on the work spindle and milling with
live tooling on the tool spindle (the C axis). Its posts are every post
except plasma. For a live-tool move the machine's C rate is what is judged,
and the work's chuck rpm limit applies to sections that run on the work
spindle. Each shipped post script declares its dialect in
`capabilities` (1 = milling, 2 = turning).

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
admitted on a plasma table only (not on a mill-turn). A spindle word, a
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

Wire Contour runs on a machine file that declares `wire_edm`. The
**Generic ISO Wire EDM** post takes its EDM words from the file's
`edm.codes`. A user `.cps` file can also declare `wireEdm` in its manifest.
The **Sodick Wire EDM (Mark 21)** post writes 2-axis contours. It refuses
taper and 4-axis cuts. Its output has not been verified on a Sodick control.

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
