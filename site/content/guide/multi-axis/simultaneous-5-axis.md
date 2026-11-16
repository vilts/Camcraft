+++
title = "5-axis simultaneous"
description = "Swarf, flow, contour and roughing with the tool axis moving: lead, lag and the tilt limit."
weight = 4
+++

In simultaneous 5-axis work the part stands still and the **tool axis moves**
on every move, while the linear axes follow. CamCraft has seven operations
for it (5-Axis Contour, 5-Axis Parallel, 5-Axis Flow, Swarf, Advanced Swarf,
Multi-Axis Rough and Multi-Axis Contour), all in the **Multi-axis** group.
Each operation's settings, defaults, lead, lag, max tilt, shank avoidance
and refusals are in the
[Multi-axis operations](@/guide/multi-axis/operations-multiaxis.md) reference. This page
covers what that reference leaves out: what the machine and post must offer,
the checks, and a recipe.

## What it needs

- A machine that tilts in two directions: `tilthead.mch` is the only shipped
  file with two tilting rotaries. The Doosan and microARC 4 files have a
  single A table, which can reach a pose only in the plane across A; the
  machine check refuses a pose it cannot reach.
- A post that writes TCP: `fanuc.cps` or `doosan-dnm.cps`. Every other
  shipped post refuses a tilted move, so a 5-axis program does not post
  with them. There is no 5-axis post for Heidenhain, Siemens, Haas or Mazak
  yet.

## Checks

See [Checks](@/guide/multi-axis/checks.md). In short: the machine check judges
the posed moves against rotary travel, the leaning-tool reach laws judge the
shank and holder against the part and fixtures, and the stock simulation
does not judge posed programs.

## Recipe: finishing a ruled wall with Swarf

1. Mount `tilthead.mch` with `fanuc.cps` or `doosan-dnm.cps`.
2. Import a part with a ruled wall (a bladed or tapered side wall).
3. Add **Swarf** with a flat or bull-nose tool. Leave lead and lag at
   zero, set Max tilt to 90, and leave **Stepover** empty so the tolerance
   sets it.
4. Run it. If the model is doubly curved, the refusal names it; use
   **5-Axis Parallel** instead.
5. Post, then read the machine check and any warnings before cutting.

## Not implemented

- Singularity detection or avoidance: CamCraft does not model what a
  control does when a rotary passes through a pole.
- Collision of the machine's own spindle and head body against the work.
  Machine files can state a spindle body size, but no check uses it; the
  checks cover the tool, shank, holder and fixtures.
- Judging a posed program in the stock simulation.
