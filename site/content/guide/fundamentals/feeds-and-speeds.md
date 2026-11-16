+++
title = "Feeds and speeds"
description = "Surface speed, chip load, engagement and chip thinning, and where CamCraft stores and limits them."
weight = 1
+++

Feeds and speeds are the two numbers that decide whether a cut is fast,
clean and survivable: how fast the tool spins, and how fast it moves.

## The principles

**Surface speed** is how fast the cutting edge travels through the material
at the tool's outer diameter. It depends on the work material and the tool
material, not on the tool size. **Chip load** (feed per tooth, `fz`) is how
much material each flute takes per revolution. Too little rubs instead of
cutting and wears the edge; too much overloads it.

The two formulas that turn them into machine settings:

- spindle speed: `n = Vc x 1000 / (pi x D)`, with `Vc` in m/min, `D` in mm and
  `n` in rpm (in inch units, `n = Vc x 12 / (pi x D)` with `Vc` in feet per
  minute and `D` in inches);
- feed rate: `vf = fz x z x n`, with `z` the number of flutes.

**Engagement.** Radial engagement (`ae`, the stepover) is how much of the
diameter is in the cut sideways; axial engagement (`ap`, the stepdown) is how
deep. Heavy axial depth with light radial engagement is the usual way to use
the full flute length.

**Chip thinning.** When the radial engagement is less than half the
diameter, each flute leaves the cut before it has taken a full-thickness
chip. The real chip is thinner than `fz`. For a square-shouldered cut the
ratio is `2 x sqrt(r - r^2)`, with `r = ae / D`. To keep the intended chip
thickness at light stepovers, raise the programmed feed by the inverse of that
ratio. The compensation has a limit: at very light stepovers the factor runs
away, so it is held.

**Plunge and ramp feeds.** Plunging loads the tool axially, where an end mill
is weakest and chips cannot escape, so plunge feed is much lower than cutting
feed. A ramp or helix entry spreads the descent over a longer path.

**Reading the cut.** A steady, even sound is right. A scream or chatter means
speed or engagement is wrong. Chips should come off as clean, uniformly
coloured curls. Blue or dark chips in steel, or melted, welded chips in
aluminium, mean too much heat for the feed. These are cues for the first
cuts, not numbers.

**A less rigid machine** needs less engagement and a lower feed before it
needs a lower speed. Lighter radial engagement is usually the first change.

## What CamCraft does

On an operation's **Feeds** tab:

- **Spindle speed** is shown and editable as rpm, or as surface speed
  through the tool's diameter: m/min in a millimetre document, surface
  feet per minute (SFM) in an inch one. The speed lives on the tool.
- **Feed** and **Plunge feed** are separate numbers on the operation.
- **Climb milling** is a check box (see
  [Toolpath strategy](@/guide/fundamentals/toolpath-strategy.md)).
- The **Cutting data** group holds the document's workpiece material, the
  tool's edge material (HSS or carbide) and flute count. With all three
  stated, **Suggest feeds & speeds** shows a starting rpm and feed from
  the formulas above. Nothing changes until you click **Apply
  suggestion**, and the apply is one undoable step.
- The suggestion's numbers come from a small built-in table of workpiece
  groups (steel, stainless, cast iron, non-ferrous, plastics) with
  conservative starting values. They are a starting point, not a limit.
  The table does not speak for every tool kind: it says so by name when it
  cannot suggest.
- Chip thinning compensation is applied only where the operation states an
  optimal engagement load. Adaptive clearing does.
- Lathe tools can state a constant surface speed and a feed per revolution;
  the post then writes constant surface speed with a maximum-rpm clamp.

**Limits CamCraft enforces.** The suggestion is clamped to the mounted
machine's stated spindle range and feed ceiling, and each clamp is named
in the suggestion. Independently, the program gate refuses to post an
operation whose tool runs above or below the machine's stated rpm, and one
whose estimated spindle power exceeds the machine's stated power (only when
the machine, material, flutes and engagement are all stated; CamCraft makes no
claim from an unstated number).

A lathe setup can also state a **workholding rpm limit** (the chuck or
collet rating). Under constant surface speed the rpm ceiling becomes the
lowest of the machine's, the tool's and the workholding's. Under constant
rpm, an operation over that limit is refused by name, not capped, because
the cut was planned at that speed. The limit is lathe-only; mill setups
have no equivalent setting. See [Setups](@/guide/set-up/setups.md) and
[Workholding](@/guide/set-up/workholding.md).

CamCraft does not measure sound or chip colour and does not model machine
rigidity. The first cut is where you learn whether the numbers fit.
