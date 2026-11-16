+++
title = "Tool selection and reach"
description = "Choosing the shortest tool that reaches, and how CamCraft checks reach and holders."
weight = 2
+++

## The principles

**Shortest tool that reaches.** A tool's stiffness falls with the cube of
its unsupported length. Use the shortest tool and the least stick-out (the
length from the holder face to the tip) that reaches the deepest feature
with clearance for the holder. See [Rigidity](@/guide/fundamentals/rigidity.md).

**Diameter against the part.** A tool cannot cut an inside corner sharper
than its own radius, so the smallest inside corner radius limits the largest
tool for finishing that corner. Roughing can use a bigger tool; a smaller one
then cleans up what is left (rest machining, see
[Toolpath strategy](@/guide/fundamentals/toolpath-strategy.md)). In a
narrow pocket the tool must also fit the narrowest width with room to move.

**Flutes and material.** Fewer flutes leave more room for chips, which suits
soft, gummy material and deep slots. More flutes allow a higher feed at the
same chip load and give a finer finish in harder material. Carbide runs
faster and stiffer than HSS but chips more easily under shock.

**Holder clearance.** The holder is wider than the cutting tool. Reaching down a
wall or into a pocket, the holder or shank can hit the part or a clamp
before the tip does.

## What CamCraft does

A tool stores its diameter, flute length, shank diameter and length, holder
diameter and length, flute count, edge material and whether it cuts to centre.
A holder can also be a modelled body, a mesh or a drawn profile, and the
checks then read the body instead of a plain cylinder.

- **Reach is checked at generate time for drilling.** A drill may go no
  deeper into material than its usable flute, and its holder must stay clear
  of the stock along every move. Both refuse by name, before the path can
  reach the post.
- **Collisions are checked on the simulated program.** The simulation flags
  moves where the shank or holder carries through material, where a move
  runs the tool through a fixture, and where a rapid goes through stock. The
  flagged moves are marked in the viewport, and a flagged program does not
  post.
- **Machine tool limits** (tool length and diameter the magazine accepts)
  are checked against the mounted machine when it states them.

What CamCraft does not have is a separate report of "the smallest inside
radius in this part against your tool." A tool too large for a corner
simply leaves material there; **Deviate** shows the material left, and
rest machining with a smaller tool can take it.

See [Operations](@/guide/operations/_index.md) and the
[glossary](@/guide/reference/glossary.md) for holder and stick-out terms.
