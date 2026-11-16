+++
title = "Order of operations"
description = "How to sequence a job's operations, and how CamCraft helps you check the order."
weight = 1
aliases = ["guide/order-of-operations/"]
+++

The order of a program is a machining decision, and it is one of the main
skills in CAM. The same operations in a different order can cut faster, hold
better, leave a nicer finish or break a tool. CamCraft posts operations in
the order the browser shows them, so the order you choose is the order the
machine runs.

## Principles, and why

- **Face first.** Facing makes one flat datum and a known Z. Every depth that
  follows is measured from a surface you cut, not from a rough saw cut.
- **Rough before finish.** Roughing removes most of the material and leaves an
  even allowance. The finish pass then cuts a light, constant load, which keeps
  deflection and tool wear steady and the surface consistent.
- **Heavy cuts while the part is rigid.** Remove bulk while the stock is still
  a solid block. Thin walls, tall ribs and small bosses go last, because once
  they stand alone they flex, chatter and can move under the cut.
- **Big tools first, then small tools for what they left.** A large cutter
  clears quickly but cannot enter tight corners. A smaller one then cleans up
  only what remains, so it spends little time cutting and wears less.
- **Holes in stages.** Spot or centre first so the drill starts true, then
  drill, then ream, bore, tap or thread mill. Whether to drill before or after
  a pocket over the same spot depends on chip clearing and on breakthrough:
  drilling first gives the pocket cutter a start hole, drilling afterwards
  means the drill meets a flat floor instead of an interrupted one.
- **Chamfers and deburring last.** Break an edge only after the faces that make
  it are finished, or the next pass leaves a new burr or removes the chamfer.
- **Tool changes against feature grouping.** Doing every operation for one tool
  together saves tool changes. Doing every operation for one feature together
  can save re-positioning and keeps a feature's roughing and finishing close
  in time. Neither always wins: prefer fewer changes unless the part moves
  or cools between the passes that must match.
- **Group by setup.** Do in the first setup whatever the second one needs in
  order to hold and locate the part: a clean datum face, a squared edge, a
  locating bore. See [Setups](@/guide/set-up/setups.md).
- **Turning.** Face the end, rough the outside, finish the outside, groove,
  thread, then part off. Do inside work (drilling and boring) before the part
  is weakened by deep grooves or a long overhang.
- **Multi-axis.** Group the passes that share a tilt or index angle, and
  order the indexes so the rotary axis travels as little as possible.

These are habits, not laws. A thin, long part may be better finished in
the middle of the program to keep it stiff, and a soft material may not need
a separate finish at all. Choose for the part in front of you.

## How CamCraft supports the order

### Reordering

Drag an operation in the browser to move it, or call the `reorder_operation`
MCP tool to move one before or after another operation of the same setup.
Both run the same undoable command, so one undo reverts the move. Moving an
operation into a different setup is refused: the operation belongs to one
setup, so create it there instead.

The order is saved with the project and is the single program order. Posting
reads it, and so do simulation and rest material, so what you see in the
browser is what is posted and simulated. See [Posting](@/guide/check-and-post/posting.md).

### Later operations see earlier ones

For the operation kinds themselves, see [Operations](@/guide/operations/_index.md).

CamCraft cuts the stock forward in program order, so an operation sees the
material the earlier ones left:

- **3D Pocket** has a **Rest roughing** option: a closing ladder that cuts
  what the level passes left standing, at half the step.
- The **Drill** reach and holder checks read the surface the setup's earlier
  operations leave. Drilling after a pocket is judged against the pocket
  floor, not the original top of the stock.
- A new setup can take its stock from what the setup before it leaves behind,
  so verification, rest machining and the fixture checks of that setup see
  the machined part. See
  [Carrying stock to the next setup](@/guide/set-up/setups.md#carrying-stock-to-the-next-setup).

Because of this, moving an operation can change what another one does. After
a reorder, regenerate the operations after it and check the result.

### Refusals that depend on order

CamCraft refuses rather than cut a bad program (see
[Reading a refusal](@/guide/reference/refusals.md)). Some refusals follow from the
order, because they judge the tool against the material that stands at that
point:

- A drill whose flute does not reach the hole bottom is refused with a message
  such as "tool ... flute reaches ... mm - past it the shank feeds into uncut
  stock (a collision)". Because the check reads the earlier operations' stock,
  moving the drill after a pocket that lowered the surface can clear it, and
  moving it before the pocket can bring it back.
- A tap is judged at its thread core. A tap whose pitch is too coarse to
  leave a core is refused by name ("tap leaves no thread core").

Checks that look at the order itself (for example, refusing to thread mill a
hole the program has not yet drilled) are not part of this release. Until
they land, sequence spot, drill, then thread or tap yourself, and use the
simulation to confirm.

### The simulation as the check

Step through **Simulate** in program order before posting, and use **Deviate**
to see where material is left or where the program cut into the part. A
wrong order often shows there first: a tool cutting air, a finish pass
meeting a thick shoulder, a drill entering uncut stock.

## A worked example

A small plate, 80 x 50 x 20 mm, with a pocket, four through holes and a
chamfered outer edge. A first draft, in the order the operations were made:

| # | Operation | Tool |
|---|---|---|
| 1 | Chamfer outer edge | 90 degree chamfer mill |
| 2 | Pocket (finish, full depth) | 6 mm end mill |
| 3 | Drill four holes | 8 mm drill |
| 4 | Face top | 50 mm face mill |
| 5 | Pocket (rough) | 12 mm end mill |

After reordering with the browser (or `reorder_operation`):

| # | Operation | Tool |
|---|---|---|
| 1 | Face top | 50 mm face mill |
| 2 | Pocket (rough) | 12 mm end mill |
| 3 | Pocket (finish) | 6 mm end mill |
| 4 | Drill four holes | 8 mm drill |
| 5 | Chamfer outer edge | 90 degree chamfer mill |

Why the second order is better:

- Facing moves to the start, so every later depth measures from the cut top.
- The pocket is split into rough then finish. The large cutter does the bulk
  and the small one only cleans up and finishes, instead of the small cutter
  doing everything.
- The holes follow the pocket, so the drill is not interrupted by a rough
  pocket wall and its reach is checked against the surface that now stands.
- The chamfer goes last, after the top face and pocket edges it breaks are
  finished, so no later pass leaves a burr on it.
- The big tools run first, and the delicate finishing runs last.

To get from the first list to the second, move "Face top" and the rough
pocket to the top, then the finish pocket and the drill, and "Chamfer outer
edge" to the end. Then simulate and check the result before posting.
