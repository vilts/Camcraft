+++
title = "Rigidity, chatter and deflection"
description = "Why long tools, thin walls and loose setups cut badly, and what to change."
weight = 4
+++

## The principles

**Tool length to diameter.** A tool bends like a cantilever: deflection
grows with the cube of its stick-out and falls with the fourth power of its
diameter. Beyond roughly four times its diameter in stick-out a tool needs
lighter cuts, and beyond six or so it needs much lighter ones. A deflected
tool cuts a tapered wall, leaving the part wider at the bottom than the top.

**Thin walls and floors.** A thin wall flexes away from the tool, springs back
and then gets cut again, which gives chatter, poor size and a rough finish. A
thin floor does the same under axial load.

**Leave stock for stiffness, and finish it last.** Rough while the part is
still a thick, stiff block. Leave a finishing allowance, and cut the thin
features to size in a final light pass, ideally in several shallow passes so
the wall is supported by uncut material below the tool.

**Part movement.** Cutting releases stress in the material, and clamping can
distort a thin part. Roughing both sides before finishing either, and
loosening and re-clamping lightly before a finish pass, are common ways to
let it settle.

**When it chatters.** Chatter is a self-excited vibration: a regular pitch
and a wavy surface. Change one thing at a time: reduce radial engagement,
shorten the stick-out, check the workholding, and then shift the spindle speed up or down a
little (the speed that avoids a resonance is found by trying). Lower feed with the same speed
usually makes it worse because the chip gets too thin.

## What CamCraft does

- **Stock to leave** on the Passes tab is the finishing allowance: roughing
  stops short of the model by that amount, and a later operation finishes it.
  Order the finish last; see
  [Order of operations](@/guide/check-and-post/order-of-operations.md).
- **Contour tabs** (tab count, width and height) keep a profiled part
  attached to the stock while its outline is cut.
- **Stepover, stepdown and engagement** are per operation, so a light finish
  pass on a thin wall is a separate operation with its own settings.
- **Holders and stick-out** are stored on the tool, and collisions with them
  are checked ([Tool selection](@/guide/fundamentals/tool-selection.md)).
- **Workholding** and fixtures belong to the setup, and fixture collisions are
  checked; see
  [Workholding](@/guide/set-up/workholding.md).

CamCraft does not model tool deflection, part flex or chatter. It cannot warn
you that a wall is too thin or a tool too long for a cut. That judgement,
and the first-cut check, are yours.
