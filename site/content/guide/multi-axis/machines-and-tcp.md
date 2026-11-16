+++
title = "Machines, kinematics and TCP"
description = "Table-table, head-head and tilt-head machines, which axis limits apply, and what each post does with tilted moves."
weight = 1
+++

## What a machine file tells CamCraft

The machine file's `parts` tree states every axis in the order it moves. A
rotary part carries an `axis` direction (the line it turns about), a
`travel` in degrees, and the parts below it. Three things follow from it:

- **Does the machine tilt the tool?** CamCraft answers this geometrically. A
  machine can tilt if some rotary states a direction that is not the Z
  column. A C table (axis along Z) tilts nothing. A rotary with no stated
  direction counts as tilting nothing: capability is claimed, never assumed.
- **Does the rotary turn the work or the tool?** A rotary with the head
  below it in the tree turns the tool; one without turns the work. The
  [index-swing check](@/guide/multi-axis/checks.md) applies only to work-turning
  rotaries.
- **Which limits apply?** Each rotary's travel is judged per move, and the
  linear axes are judged against their own strokes.

## Table, head and tilt-head machines

- **Table-table or table-only (a trunnion or a 4th-axis table).** The rotary
  turns the work. In the shipped set this is `doosan-dnm.mch` and
  `tormach-770mx-microarc4.mch`, each with one A table about X. The
  part sits on the rotary, so the work offset must lie on the rotary's
  centreline (see [Indexed work](@/guide/multi-axis/indexed.md)).
- **Head-head or tilt-head.** The rotaries carry the spindle, so the part
  stays still. `tilthead.mch` is this: A about X, then B about Y, then the
  head, each limited to -120..120 degrees.
- **Mill-turn.** `millturn.mch` has a C axis about Z. It is a spindle
  rotary for live-tool and rotary work, not a tilt.

Why it matters:

- **Reach.** The pose has to be inside every rotary's travel, and the
  linear moves inside their strokes. A tilt-head with +-120 degrees cannot stand the
  tool at a pose 150 degrees from vertical.
- **Which limits apply.** A machine whose only rotary is about X is judged
  through the tool axis: a stated pose such as A0 B180 reads as its A180.
  Every other machine is judged letter by letter: an A trunnion asked for a B
  turn is refused by name, since CamCraft never solves for another axis
  combination.
- **Singularities.** CamCraft does not solve inverse kinematics: the
  generators state the tool axis and the post (or the control) turns it into
  angles. A pose where two rotaries line up (tool straight along an axis) is
  where a real control swings an axis fast. CamCraft has no singularity
  check of its own; prove the program out in the control's own simulation.

## TCP, RTCP and no TCP

TCP (also called RTCP or TCPM) is the control's mode where the programmed
point is the tool tip, and the control moves the linear axes to keep the tip
on the path while the rotaries turn. Without it, the program has to contain
the compensating linear moves, and CamCraft's posts do not compute those.

The post reads `machine.rtcp` from the machine file (true when a rotary
states a direction off Z) and does the following:

| Post | Tilted moves |
|---|---|
| `fanuc.cps` | With `machine.rtcp` true, arms `G43.4 H<tool>` once per tool and writes the rotary words, modally. Refuses (counted warning, post blocked) a tilt on a machine with no tilting rotary, a letter the machine states no rotary for, a tilt inside a canned-cycle group, and a lone rotary stated along -X. |
| `doosan-dnm.cps` | The same `G43.4` path when `machine.rtcp` is true. When it is false, a no-TCP path indexes an A table only: it needs rotary clamp and unclamp codes declared by the machine or post options, and refuses B or C words. |
| `tormach.cps` | No TCP. A table about X is indexed between operations and A never changes inside one. See [Indexed work](@/guide/multi-axis/indexed.md). |
| `haas.cps`, `heidenhain.cps`, `mazak.cps`, `sinumerik.cps` | Refuse every tilted move: these posts have no RTCP output. |
| `carvera.cps` | Refuses every tilted move: the post is 3-axis. |
| `fanuc-lathe.cps`, `tormach-lathe.cps` | Lathe posts; `fanuc-lathe.cps` handles C-axis live-tool sections, and refuses A/B words. |

A refused tilt is a counted warning that blocks the post, so it cannot slip
into a file silently.

### When your control has no TCP

If the control has no TCP, use only indexed work (3+1 or 3+2) with a post that
indexes. Do not post simultaneous work to it. Known gap (#950): on the
no-TCP path of `doosan-dnm.cps`, the post assumes A0 at the start of the
program and after a tool change, so an A0 section writes no A word even
though the table may stand elsewhere. Until that is fixed, start every program
and every tool with an operation that moves A explicitly, or check the posted
file for the missing A word.
