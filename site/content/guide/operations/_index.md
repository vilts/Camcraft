+++
title = "Operations"
description = "The operation kinds, their settings tabs, and generating toolpaths."
sort_by = "weight"
weight = 3
+++

An operation is one machining step: a tool, an area to cut, and how to cut
it. The ribbon shows the operation groups the mounted machine can run, so a
mill document has no turning operations and a lathe document no 3D
milling. A mill-turn shows both.

## Operation kinds

| Group | Operations |
|---|---|
| Milling 2D | Face, Contour, Pocket, Slot, Trochoidal slot, Trace, Adaptive, Offset, Engrave, Chamfer, Circular, Thread |
| Milling 3D | 3D Pocket, 3D Finish, Deburr |
| Drilling | Drill, Bore, Probe |
| Multi-axis | 5-Axis Contour, 5-Axis Parallel, Swarf, Adv. Swarf, Rotary Contour, Rotary Parallel, Rotary Offset, Multi-Axis Rough, 5-Axis Flow, Multi-Axis Contour |
| Turning | Facing, Profile Rough, Profile Finish, Groove Rough, Groove Finish, Part, Threading, Chamfer, Taper, Multi-Pass, Form, Bore |
| Plasma | Plasma Profile |
| Wire EDM | Wire Contour |
| Sinker EDM | Sinker Burn, Side Burn |

Each 3D milling kind has its own section, with its settings, defaults
and common refusals, in [3D milling operations](@/guide/operations/operations-3d.md).

Some kinds carry several strategies. The 3D kinds cover adaptive roughing
per level, waterline, parallel, scallop, radial, spiral, morph, flow,
pencil and steep/shallow finishing. Drill covers spot, drill, peck, deep,
tap and bore cycles, and **Detect holes** finds the holes in the model for
you. Milling operations can also be indexed (3+2) to a tilted plane on a
machine that can tilt.

Each 2D milling and hole kind has its own section, with its settings,
defaults and common refusals, in [2D milling operations](@/guide/operations/operations-2d.md).

## The settings tabs

An operation's page groups its settings on tabs. A kind shows only the
tabs it has settings for:

- {{ <icon name="sec-geometry" /> }} **Geometry:** the tool and what to cut. Pick faces, edges or loops in
  the viewport, then set containment, offset, extension and extra or open
  boundaries where the kind has an area.
- {{ <icon name="sec-heights" /> }} **Heights:** clearance, retract, top and bottom heights, and where each
  is measured from.
- {{ <icon name="sec-passes" /> }} **Passes:** stepover, stepdown, stock to leave and the kind's own
  strategy settings.
- {{ <icon name="sec-leads" /> }} **Lead in / Lead out:** how the tool enters, moves between levels and
  leaves the cut, and the links between cuts.
- {{ <icon name="sec-feeds" /> }} **Feeds:** spindle speed and feed rates.
- {{ <icon name="sec-machine" /> }} **Machine:** only on the few kinds that drive a machine function of
  their own.

Edits on a page are pending until you click **Apply** or {{ <icon name="act-generate" /> }} **Generate**.
Both are undoable with Ctrl+Z (Cmd+Z on macOS).

Each turning kind has its own section, with its settings, defaults and
common refusals, in [Turning operations](@/guide/operations/operations-turning.md).

## What a click picks

**Pick** buttons use a faint selection tint when idle. An armed button
uses the full selection colour and an outline. This marks the active pick
target. Hover a selected face, edge or corner in the viewport to highlight
its row on the Geometry tab.

Every page that takes a pick from the viewport (operation geometry,
boundaries, setup alignment, fixture mates, the work origin) follows the
same rule:

- **A face is the whole face.** Clicking anywhere on a flat face, or on a
  smooth curved wall, picks all of it, never one of the triangles the model
  is drawn with.
- **An edge is a real model edge:** where two faces meet at a crease, or the
  open border of a sheet. The lines between the triangles inside a face, or
  around a smooth wall, are never pickable.
- **A point is a corner** where model edges meet, or the end of an open
  edge. A vertex inside a face or along a round edge is not a point.
- **Only what you can see.** A click takes the surface in front, and an
  edge or a corner only when it is visible there. Nothing behind the part
  is picked through it; turn the view to reach it.
- **Only what the page asks for.** Near a feature, a corner wins over an
  edge and an edge over a face, but only among the kinds the page takes: a
  plane is picked by a face, so it never snaps to an edge.

Edge mode picks the face between the edges when no edge is under the
cursor; Alt+click (Option+click on macOS) picks a single edge segment instead of the whole chain.
The same rule holds for picks an AI agent makes over MCP by feature id: an
id a click could not have picked is refused by name.

## Order

Operations post in the order the browser lists them. Which order to choose,
and why, is covered in [Order of operations](@/guide/check-and-post/order-of-operations.md).

## Generating

{{ <icon name="act-generate" /> }} **Generate** computes the toolpath and checks it. An operation can be in
one of three states (shown by its browser icon):

- {{ <icon name="badge-never" /> }} **No toolpath yet:** it has never been generated.
- {{ <icon name="badge-stale" /> }} **Stale:** something it depends on (the model, the stock, the tool, the
  work offset or its own settings) has changed since it was generated.
  Generate it again.
- {{ <icon name="badge-ok" /> }} **Generated:** the cached toolpath matches its inputs.

When CamCraft cannot compute a safe path for some part of an operation {{ <icon name="badge-warning" /> }}, it
does not hand you a partial or guessed one: generation stops with a
message that names the setting or the geometry at fault. Moves the checks
flag as unsafe are marked in the viewport, and a message that names a place
can be clicked to frame it.

## Checking

- {{ <icon name="act-show-paths" /> }} **Paths** ({{ <icon name="act-hide-paths" /> }} when hidden) shows every cached toolpath at once.
- {{ <icon name="act-simulate" /> }} **Simulate** plays the program against the stock with a transport bar;
  drag the playback slider to jump.
- {{ <icon name="tree-stock" /> }} **Deviate** compares the machined stock with the model and colours where
  material is left or where the program cut into the part. **Frame rest**
  and **Frame gouge** take the view to the worst spot of each.

The multi-axis kinds, Probe, Plasma Profile and the EDM kinds (Wire
Contour, Sinker Burn, Sinker Side Burn) each have their own reference, with settings, defaults and common refusals:
[Multi-axis operations](@/guide/multi-axis/operations-multiaxis.md),
[Probing](@/guide/operations/operations-probing.md),
[Plasma cutting](@/guide/operations/operations-plasma.md) and
[Wire and sinker EDM](@/guide/operations/operations-edm.md).
