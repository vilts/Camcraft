+++
title = "Post and machine quirks"
description = "The start block, units, tool-centre-point output and checking the first program on a new machine."
weight = 8
+++

## The principles

**A post is a dialect.** The same toolpath is written differently for
different controls: how a tool change is called, how a canned cycle is
written, which words a control accepts on one line.

**A safe start block.** A program should not depend on what the last one
left behind. The first block states the modes explicitly: absolute
positioning (G90), cutter compensation off (G40), tool length compensation
cancelled (G49), canned cycles cancelled (G80), the work offset (G54 and its
relatives) and the plane. Then the tool change and length offset.

**Inch or millimetre.** A control left in inch by the last program reads
millimetre numbers as inches, a 25-times error. The unit is stated by G20 (inch)
or G21 (millimetre), in a block of its own before any motion, per RS-274.

**Tool centre point (TCP) or not.** On a machine with tilting axes, the
control can either hold the tool tip at the programmed point while the axes
tilt (TCP, also called RTCP or TCPM, with a word such as G43.4), or the
program can be written in terms of the axis positions. A three-axis machine has
neither.

**A new machine's first program.** Cut air first. Check the units, the
work offset and tool length numbers against what the machine holds, the
spindle direction, and how it handles a tool change. Then proceed to
[proving out](@/guide/fundamentals/proving-out.md).

## What CamCraft does

- **Built-in posts** are scripts: Fanuc, Haas, Siemens 840D, Heidenhain,
  Mazak, Tormach PathPilot, Makera Carvera and Fanuc Lathe, and you can load your own.
  The setup chooses its post and machine; see [Setups](@/guide/set-up/setups.md).
- **Start blocks.** At each operation the Fanuc-family post writes absolute
  mode, the work offset, cutter compensation off, length offset
  cancelled and canned cycles cancelled, then the tool change and length
  offset. The Haas post also states the XY plane.
- **Units.** The Fanuc and Haas posts write G21 (millimetres) in a block of
  its own before any motion. The Carvera firmware is millimetres only, and
  the post raises a warning when the machine file is in inches. Making the program unit a
  document setting is an open issue (#902), so for an inch control check the
  first block of the file. See
  [Units and coordinates](@/guide/reference/units-and-coordinates.md).
- **Tilted output.** The Fanuc post arms its tool-centre-point mode (G43.4)
  when the path tilts the tool. The intermediate stream is specified so that a
  post that keeps the machine prismatic refuses a tilted record rather
  than reinterpret it ([the stream
  spec](https://github.com/NeoTech/Camcraft/blob/main/docs/CNC-INTERMEDIATE.md)).
  See Machine types and
  [Multi-axis operations](@/guide/multi-axis/operations-multiaxis.md).
- **Machine files** state facts (axis travel, spindle range, tool changer,
  rigid tapping, coolant codes), and the program is checked against the
  facts stated. A machine file is not a guarantee that a real machine is
  safe: check its numbers against the manual. Documenting a new machine is
  covered in the [machine onboarding
  guide](https://github.com/NeoTech/Camcraft/blob/main/docs/MACHINE-ONBOARDING.md).

Post output is always a first draft. Read the first program line by line
against your control's manual before running it. See
[Posting](@/guide/check-and-post/posting.md).
