+++
title = "Toolpath strategy"
description = "Adaptive and conventional roughing, climb and conventional milling, finishing strategies and rest machining."
weight = 3
+++

## The principles

**Conventional roughing** clears a pocket in offset rings or a raster. At
corners and when a ring closes, the tool suddenly engages more material, so
the load spikes. Feed and depth must be set for the worst point.

**Adaptive (constant engagement) roughing** shapes the path so the tool's
engagement stays near a set limit everywhere, with arcs instead of sharp
turns. Because the load is steady, it allows a deeper axial cut and a
higher feed, and spreads wear along the whole flute length.

**Climb versus conventional milling.** In climb milling the tool rotates
with the feed at the contact point: the chip starts thick and ends thin. It
usually gives a better finish and less rubbing, and pulls the tool into the
cut, so it needs a machine without backlash. Conventional milling starts
thin and ends thick, and pushes the tool away. Roughing a hard surface scale
sometimes favours it. Climb is the usual default.

**Choosing a finishing strategy by surface.** Steep walls suit passes that
follow constant Z levels (waterline). Shallow and flat areas suit passes
that follow the surface in a raster, because on a waterline pass the spacing
grows as the surface flattens. Floors suit a single flat pass. Most 3D
parts need both: split by slope, so each strategy cuts the surfaces it is good at.

**Rest machining.** After a large tool, material stays in corners and
narrow areas. A later operation with a smaller tool can be limited to
just those areas instead of re-cutting everything.

## What CamCraft does

- **Adaptive** is an operation kind (2D), with a **Max engagement** angle,
  **Forward step**, **Stock to leave** and a choice of how the tool returns
  between passes. The 3D pocket also has an adaptive strategy per level.
- **Climb milling** is a check box on the Feeds tab of the milling kinds.
- **3D Finish** offers several patterns: Waterline, Parallel, Scallop, Radial,
  Spiral, Contour, Project, **Steep/Shallow**, Flat, Pencil and Corner. Steep/Shallow splits the
  surface by slope, with a setting for each side.
- **Rest roughing** is a check box on the 3D pocket: a closing ladder cuts
  what the levels leave standing. Across setups, the stock can be carried from
  the previous setup so later checks see what the earlier one left.
- Pencil finishing rides the concave corners that a larger reference ball
  leaves standing.

The full list of kinds and their settings is on the
[Operations](@/guide/operations/_index.md) page; ordering them is covered by
[Order of operations](@/guide/check-and-post/order-of-operations.md).
