+++
title = "Check and post"
description = "Inspect the simulated cut, check the operation order and write the NC program."
sort_by = "weight"
weight = 4
+++

Use [Simulate and Deviate](@/guide/operations/_index.md#checking) to inspect
the cut against the stock and model. Check the
[order of operations](@/guide/check-and-post/order-of-operations.md)
before posting.

[Posting](@/guide/check-and-post/posting.md) explains the post processors
and the [verification checks](@/guide/check-and-post/posting.md#what-is-checked-before-anything-is-written)
that must pass before CamCraft writes a program. Choose the appropriate
[machine type](@/guide/check-and-post/machines.md) and post for the job.
If a check refuses the program, read the
[refusal](@/guide/reference/refusals.md) before trying again.
