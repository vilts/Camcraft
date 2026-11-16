# Golden CL files

Committed toolpath dumps for every 2D generator (AGENTS.md: "golden
CLFile/G-code outputs committed under `tests/golden/`"). The behavioral
assertions in `tests/machining2d.rs` say a generator is *reasonable*; a
golden says the exact output did not move. Both are required, and every
new generator test that produces a path must also dump it into a golden -
a generator change that only checks numeric properties can still reorder
half a path unnoticed.

## Format

`common::gen_to_cltext(&Toolpath) -> String` - ours, not a vendor CL
format. One file, one generated path:

```
; HSMW-CL v1 scale=0.001mm moves=412
START 0 0 5000
Rapid ...
Plunge 12000 5000 -2000 F100000
Feed ...
```

Coordinates are integers at `CL_SCALE` (0.001 mm = 1 um, the same
fixed-point resolution the geo2d layer bridges to). Every move line shows
its kind, its destination, and its feed in mm/min (x1000) when the move
has one. The dump carries no timestamps and no iteration-order-sensitive
data, so it is byte-stable across machines and runs - generator code is
pure and ordered by construction.

Paths that carry arc moves dump as **v2**: the header says `v2`, and an
arc line replaces the kind token with `Arc` and gains the center and
sense after the feed:

```
Arc 24000 10000 -2000 F500000 I20000 J10000 S1
```

An arc is always a planar feed move (`PathBuilder::push_arc` refuses
anything else), so the token loses nothing but the redundancy; `S` is
`+1` ccw, `-1` cw. A straight-line path dumps identically in either
version - the header only changes when `Toolpath::has_arcs()`.

Plasma paths (#670) dump as **v5**: a `; PLASMA` header after
`START` and `; TORCH ON` / `; TORCH OFF` lines around each lit contour
(docs/CNC-INTERMEDIATE.md, "Plasma records") - the kerf, the torch
state, the cut order and each contour's nesting pinned with the moves.

## Naming

By operation and shape: `face_<shape>.cl`, `contour_<shape>.cl`,
`pocket_<strategy>_<shape>.cl`, `slot_<shape>.cl`, `trace_<shape>.cl`,
`wire_contour_<side>.cl` (#532: one profile cut as a die and as a punch -
plain v1 files, a vertical wire cut states no pose and no feed),
`wire_4axis_punch.cl` (#537: a square-bottom / round-top punch, a pose
on every move).
(G-code goldens `post_<op>_<machine>.ngc` join the directory when the
post arrives, M1.5.)

## Workflow

Tests read the file and compare (`common::write_golden`): a mismatch
fails with a "stale; regenerate with WRITE_GOLDEN=1" hint, and a missing
file fails the same way - never silently generating it. To regenerate:

```
WRITE_GOLDEN=1 cargo test -p camcraft-app --test golden_cl
git diff crates/app/tests/golden/
```

Read the diff. A golden changes only when the generator changed on
purpose; the diff *is* the review of that change. Commit generator code
and its goldens together.
