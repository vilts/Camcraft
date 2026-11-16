+++
title = "Chip evacuation and coolant"
description = "Getting chips out of deep cuts, and the coolant settings CamCraft writes."
weight = 5
+++

## The principles

**Chips must leave the cut.** A chip that stays in a deep pocket or slot gets
cut again. That heats the edge, scars the surface and can jam the tool. The
risk grows with depth, with narrow slots (where chips have nowhere to go) and
with sticky materials.

**Deep pockets and slots.** Cut in steps of limited depth rather than the
full flute length. A slot gives chips only the flute channel to escape, so use a
lighter cut, a lower stepdown or a tool with fewer flutes.

**Peck drilling.** In a deep hole, the drill retracts part way, or fully, between
pecks so chips are cleared and coolant can reach the tip. A hole deeper than
a few diameters usually needs it.

**Air, mist or flood.** Flood coolant cools and flushes chips, and suits steel
and deep cuts. Mist is lighter. A blast of air clears chips and avoids the
thermal shock of intermittent coolant on carbide, a common choice for
aluminium and plastics. Through-tool coolant delivers fluid at the tip
of a deep hole.

## What CamCraft does

- **Peck and deep drilling.** The drill kind offers spot, drill, peck, deep,
  tap and bore cycles, with a peck depth, and a dwell at the bottom.
- **Stepdown** limits depth per level in pockets and slots.
- **Coolant** is a per-operation choice: **Off**, **Flood** (the default),
  **Mist**, **Air** or **Through tool**. The post switches the machine's own
  code on for the operation. **Air** is an air blast that clears chips
  without a fluid. A choice the machine states no code for, such as air on a
  machine with no air entry, or through-tool coolant on a machine without
  it, blocks the post by name, rather than being written as a guess. Coolant
  modes are declared by the machine file; see Machine types. A machine whose
  only coolant is air starts new operations on **Air**.

CamCraft does not model chips, so it cannot tell you that a slot will pack. Read the
first cut.
