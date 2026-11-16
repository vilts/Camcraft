+++
title = "Surface finish and tolerance"
description = "Stepover and scallop height, finish allowance, tolerance settings and holding size across setups."
weight = 6
+++

## The principles

**Scallop height.** A ball or round-nosed tool stepping across a curved surface
leaves a ridge between passes. For a ball of radius `R` and a stepover `s`,
the ridge height is `h = R - sqrt(R^2 - (s/2)^2)`, about `s^2 / (8R)`. To hit
a target height, the stepover is about `sqrt(8 R h)`. Halving the stepover
cuts the ridge to a quarter, at twice the cutting time. On a flat floor
with a flat end mill the stepover leaves no ridge, but it must still overlap.

**Finish allowance.** Leave a small, even layer for the finish pass and
cut it with a light, constant load. Too little and the finish tool rubs
on uneven roughing; too much and it deflects.

**Tolerance on the model against the toolpath.** The model has dimensional
tolerances (what the drawing allows). The toolpath has a tolerance of its
own: the largest error from approximating curves with short moves. Keep
the toolpath tolerance well inside the part tolerance, say a fraction of
it. A tighter value makes bigger programs and slower motion on the
machine.

**Holding tolerance across setups.** Each re-clamp adds position error.
Features that must hold a tight relationship are best cut in the same
setup, or located from a shared datum such as a bored hole or a machined
face.

## What CamCraft does

- **Tolerance** is a setting on the milling operations. In the 3D finish
  kinds it is the finish budget: lines and levels are spaced from it, so the
  cusp stays within it. Where the stepover is **Auto**, CamCraft computes it
  from the tolerance and the tool by the scallop law; a stated stepover
  overrides it.
- **A scallop check** on the 3D finish reports a ridge that stands outside
  the tolerance. It is shown and named on the operation, but it is a
  quality note and never blocks the post (it is neither a gouge nor a
  collision).
- **Stock to leave** is the finish allowance.
- **Setups** carry stock and the work offset between setups (**Carry stock
  and WCS over**), and a later setup can see the material an earlier one
  left. See [Setups](@/guide/set-up/setups.md).

CamCraft does not read tolerances from the model, and it does not check a
programmed size against a drawing. The program is a path toward the
model's geometry; whether the part meets the drawing is decided by
measuring it. The setup accuracy across re-clamps is your workholding
and touch-off, not something CamCraft can verify.
