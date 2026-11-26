+++
title = "Multi-axis"
description = "Indexed and simultaneous multi-axis work: what CamCraft does for each kind, how to set it up, and what is checked."
sort_by = "weight"
weight = 5
+++

Multi-axis machining adds rotary axes to the three linear ones. This section
covers the four kinds of multi-axis work, what CamCraft does for each, what
you set up, and what it checks before a program posts. It describes the
program on `main` and says plainly where a feature does not exist yet.

| Kind | The rotary axes | What CamCraft calls it | Page |
|---|---|---|---|
| 4-axis indexed (3+1) | One rotary turns to a fixed angle, then 3-axis cutting | A tilted plane (3+2) with the A angle only | [Indexed work](@/guide/multi-axis/indexed.md) |
| 5-axis indexed (3+2) | Two rotaries hold a fixed orientation, then 3-axis cutting | A tilted plane (3+2) with A and B | [Indexed work](@/guide/multi-axis/indexed.md) |
| 4-axis continuous | One rotary turns while cutting | Rotary Contour, Rotary Parallel, Rotary Offset | [Continuous 4-axis](@/guide/multi-axis/continuous-4-axis.md) |
| 5-axis simultaneous | All axes move together | 5-Axis Contour, 5-Axis Parallel, Swarf, Advanced Swarf, 5-Axis Flow, Multi-Axis Rough, Multi-Axis Contour | [Simultaneous 5-axis](@/guide/multi-axis/simultaneous-5-axis.md) |

Read first [Machines, kinematics and TCP](@/guide/multi-axis/machines-and-tcp.md):
which machine file you mount decides what is allowed, and the post you pick
decides whether tilted moves post at all. The list of
[checks](@/guide/multi-axis/checks.md) says what is refused and what only warns.

## Machines that ship with a rotary axis

Of the machine files in `assets/machines/`, four state a rotary axis. Every
other shipped machine is 3-axis (or a lathe).

| Machine file | Rotary axes | Notes |
|---|---|---|
| `doosan-dnm.mch` (Doosan DNM 5700 + rotary table) | A about X, travel stated as 360000 degrees, no `nose_to_axis` | A table. Pairs with `doosan-dnm.cps`. The catalog gives no nose-to-axis figure, so every op refuses until the setup states its own (`SetSetupNoseToAxis`). The plain 3-axis machine is `doosan-dnm-3axis.mch` (Doosan DNM 5700). |
| `tormach-770mx-microarc4.mch` (Tormach 770MX + microARC 4) | A about X, no stated A travel limit, `nose_to_axis` 10.6 | A table. Pairs with `tormach.cps`, which indexes A and posts an A that turns inside an operation (`G93`). The file states an A rate of 360 degrees per minute. |
| `tilthead.mch` (Tilt-head 5-axis) | A about X and B about Y, each -120 to 120 degrees, carrying the head | Head-head tilt. The 5-axis machine in the set. |
| `millturn.mch` (Multitasking 5-axis) | C about Z, -360 to 360 degrees | Mill-turn (live tooling); a C axis tilts nothing. |

To add a rotary to your own machine file, follow
[`docs/MACHINE-ONBOARDING.md`](https://github.com/NeoTech/Camcraft/blob/main/docs/MACHINE-ONBOARDING.md):
state the rotary part with its `axis` direction, its `travel` in degrees, and
(for a table that turns the work) its `nose_to_axis`.

## Related pages

- A complete indexed job on the Tormach 770MX and the microARC 4, from
  the model to the posted program, is in
  [Your first 4-axis part](@/guide/getting-started/first-4axis-part.md).
- A continuous job on the same machine, from the model to the posted
  program and an air cut, is in
  [Your first continuous 4-axis program](@/guide/getting-started/first-continuous-4axis-part.md).
- The per-operation reference for the multi-axis operations is in
  [Multi-axis operations](@/guide/multi-axis/operations-multiaxis.md).
- What each machine type means for setups and operations is in
  [Machine types](@/guide/check-and-post/machines.md).
- If a post is refused, see [Reading a refusal](@/guide/reference/refusals.md).
