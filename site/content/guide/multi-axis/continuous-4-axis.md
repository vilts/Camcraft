+++
title = "Continuous 4-axis: rotary operations"
description = "Rotary Contour, Rotary Parallel and Rotary Offset: finishing and roughing a turned shape on a rotary."
weight = 3
+++

In continuous 4-axis work the rotary turns while the tool cuts. In CamCraft
this is the three **Rotary** operations (Rotary Contour, Rotary Parallel and
Rotary Offset), which machine a shape of revolution such as a shaft or a
cam blank. Their settings, defaults and refusals are in the
[Multi-axis operations](@/guide/multi-axis/operations-multiaxis.md) reference; this page
covers what the reference does not: how the axis meets the machine, feed, posts
and a worked recipe.

## How the axis is found

You do not state the rotary's position. The generator fits the axis of
revolution from the model's mesh. A mesh with no axis of revolution is
refused by name, and the message points at the 5-axis operations; a part
standing on its axis is pointed at Rotary Contour from Rotary Parallel and
back.

The fitted axis then has to match a rotary the mounted machine states. If
the machine file lists rotaries with directions and none lies on that line,
the operation is refused by name. The letter that posts is the machine's: A
on the Doosan DNM or the microARC 4 file (A about X), C on `millturn.mch`.
If the machine states no rotary directions, nothing is checked at
generation and the machine check at post catches it.

So the model must be placed so its axis lies along the rotary's line. On an
A table the axis goes along X.

## Feed on a rotary move

`tormach.cps` uses inverse-time feed (`G93`) for simultaneous A-about-X
moves (issue #796). Its time is the longer of two: the time for the tool to go
along the posted trajectory at the operation's feed, and the time for A to
turn at the rate that the machine file states. A move that turns A only
stays in `G94` with F in degrees per minute. `G94` comes back before each
move that is not a `G93` feed and at the section end. The machine file must
state the A rate (`max_normal_speed`), or the post refuses.
[Your first continuous 4-axis program](@/guide/getting-started/first-continuous-4axis-part.md)
reads such a program line by line.

The other shipped posts write a plain F word without inverse-time or
degrees-per-minute conversion for a turning move. Where the machine file states a rotary's rate, the machine
check judges the move against it, and `fanuc-lathe.cps` refuses a C-axis
rapid when the machine states no C rate. For those posts, a control that requires inverse-time feed for simultaneous
moves cannot use the posted program as it stands.

## Posts for continuous rotary moves

Posts with TCP output write a rotary that changes while cutting:
`fanuc.cps` and `doosan-dnm.cps`. `tormach.cps` writes an A about X that
changes inside an operation without TCP: it turns each point into the
table's frame itself, keeps A counting past 360 (PathPilot does not roll A
over), and writes the moves that turn A in inverse time (`G93`), held to
the feed and to the machine's stated A rate (issue #796). The others
refuse any tilt. See
[Machines, kinematics and TCP](@/guide/multi-axis/machines-and-tcp.md).

## Checks

The rotary operations run a radial field check: the cutter's band against
the fitted skin, the stock and the part, for gouges. A mesh imported coarser
than promised gives its excess back from the allowance, and past that the
operation is refused. The stock is the setup's stock solid.

## End faces and the chuck

Rotary Parallel finishes the whole model, and that includes its two end
faces, down to the axis. A bar on a rotary is held at one end. If the
model ends where the bar goes on into the chuck, a pass on that end face
is a cut through the bar. Rotary Parallel thus reads the stock (issue
[#2343](https://github.com/NeoTech/Camcraft/issues/2343)). Where the stock
goes on past an end of the model by more than one tool diameter, the
centre of the ball does not go past that end: the end face is left, and a
note on the operation names the end. Where the stock ends with the model,
or goes on by one tool diameter or less, the end face is cut to the axis.
Make the stock as long as the real bar, so that CamCraft sees the material
that is there, and prove a program with an air cut. No setting changes
this rule.

Rotary Offset takes two turns of the rotary for each level that cuts. The
shipped microARC 4 file states no A travel limit and permits continuous A
counts. The former -360..360 travel refusal no longer applies (issue
[#2344](https://github.com/NeoTech/Camcraft/issues/2344)). A custom machine
file can state A travel; the post refuses a count past that limit. This
does not verify the control settings, cables or fixtures of a real machine.

## Recipe: finishing a shaft on an A table

1. Mount the Doosan DNM or microARC 4 machine, and its post.
2. Import the shaft with its axis along X, and place the work offset on
   the rotary's centreline.
3. Add **Rotary Offset** to rough: set the radial **Stepdown** and the
   top and bottom stations.
4. Add **Rotary Contour** with the ball tool, tolerance 0.05, lead and
   lag at zero.
5. Run both and read the refusals, if any. Post with the Doosan post.

## Rotary Flow availability

Rotary Flow is a four-axis finishing kernel for a ball-nose tool. It
uses two or more on-model guide chains that wind in the same direction
around a stated rotary axis. It makes flow-line passes between adjacent
guides and checks the swept ball against the part. The desktop and MCP
operation lists do not offer Rotary Flow yet.

## Not implemented

- Wrapping a 2D drawing (engraving text or a pattern) around a cylinder.
  Engrave and Trace cut flat planes; no operation unrolls a drawing onto a
  rotary.
- A stated rotary position: the axis comes from the mesh.
- Inverse-time feed in posts other than `tormach.cps`; Tormach supports
  `G93` for simultaneous A-about-X moves only.
- A setting for an axial limit of Rotary Parallel. The stock alone decides
  which end face is left.
