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
moves. It bounds the feed along the posted trajectory and the machine's
stated A rate, then restores units-per-minute feed (`G94`) at the section
end.

The other shipped posts write a plain F word without inverse-time or
degrees-per-minute conversion for a turning move. Where the machine file
states a rotary's rate, the machine check judges the move against it, and
`fanuc-lathe.cps` refuses a C-axis rapid when the machine states no C rate.
For those posts, a control that requires inverse-time feed for simultaneous
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

## Recipe: finishing a shaft on an A table

1. Mount the Doosan DNM or microARC 4 machine, and its post.
2. Import the shaft with its axis along X, and place the work offset on
   the rotary's centreline.
3. Add **Rotary Offset** to rough: set the radial **Stepdown** and the
   top and bottom stations.
4. Add **Rotary Contour** with the ball tool, tolerance 0.05, lead and
   lag at zero.
5. Run both and read the refusals, if any. Post with the Doosan post.

## Not implemented

- Wrapping a 2D drawing (engraving text or a pattern) around a cylinder.
  Engrave and Trace cut flat planes; no operation unrolls a drawing onto a
  rotary.
- A stated rotary position: the axis comes from the mesh.
- Inverse-time feed in posts other than `tormach.cps`; Tormach supports
  `G93` for simultaneous A-about-X moves only.
