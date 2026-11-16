+++
title = "Proving out a program"
description = "Simulation, verify and what its refusals mean, a dry run, single block and the first-article check."
weight = 7
+++

## The principles

A program is a claim that a path is safe and removes the right material. Prove it in
stages, each one cheaper than a crash:

1. **Simulate** the stock being cut, and compare the result with the model.
2. **Dry run** at the machine with the spindle off and the tool well above
   the part (for example with Z offset raised), watching the whole path for
   anything unexpected.
3. **Single block** the first cuts: one line at a time, with feed and
   rapid overrides turned down, a hand on the stop, watching distance to go.
4. **First article.** Cut one part, measure it, and only then run the batch.

Software checks the model, stock, tool and machine facts it was given. It
cannot see a clamp you forgot to enter or a wrong tool length.

## What CamCraft does

- **Simulate** plays the program against the stock. **Deviate** colours
  where material is left or where the program cut into the part, and **Frame
  rest** and **Frame gouge** go to the worst spot of each. **Paths** shows
  the cached toolpaths together.
- **Verify** runs at **Post**. It writes the program only when every
  operation is generated and current, none carries a warning, and the whole
  program passes: no gouge into the part, no move strikes a fixture, no rapid
  runs through stock, and no move exceeds a limit the mounted machine states.
  See [Posting](@/guide/check-and-post/posting.md).
- **What a refusal means.** The message names the operation and the
  problem. A gouge refusal means the simulated cutter removed model
  material; a warning refusal names the flagged moves, which are marked in
  the viewport. The table of common messages is in
  [Reading a refusal](@/guide/reference/refusals.md) and
  [Troubleshooting](@/guide/reference/troubleshooting.md).
- **Why refuse rather than warn.** A warning on a dialog gets clicked
  through and the program is posted anyway. Where the output could drive
  the tool into the part or a fixture, CamCraft writes nothing, and leaves any
  existing file untouched. The two named exceptions are advisory and show on the
  operation without holding the post: a scallop note (a finish quality
  matter) and a probe search distance.
- **"Not judged" is not "fine".** If a check cannot run to completion, for
  example a hidden-material scan that hits its budget, that is reported as
  not judged and holds the post, never counted as zero.
- **Setup sheets** list each setup's tools in change order, with estimated
  time, for the operator.

A green gate is not a guarantee. Run the dry run, single block and
first-article steps above on any new program.
