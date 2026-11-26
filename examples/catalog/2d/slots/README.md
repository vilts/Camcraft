# Slots (Slot, Trochoidal slot)

A 100 x 60 x 12 mm clean-room block with two closed slots along X. Each slot is 10 mm wide, 6 mm deep and 60 mm long, with round ends. A Slot cuts the slot at `Y15`. A Trochoidal slot cuts the slot at `Y-15`. The two operations use the same 6 mm tool, the same width and the same floor.

## What it shows

1. **Slot (`Slot1`, `EM-0600-4FL`):** Cuts the slot from `a` (`X-25 Y15`) to `b` (`X25 Y15`) in rows parallel to the centre line. The stepover is 2 mm, so each level has three rows: `Y13`, `Y15` and `Y17`. The stepdown is 3 mm, so the levels are `Z9` and `Z6` (the slot floor). The tool plunges straight down at the start of each level.
2. **Trochoidal slot (`Trochoidal slot1`, `EM-0600-4FL`):** Cuts the slot from `a` (`X-25 Y-15`) to `b` (`X25 Y-15`) in loops at the full depth. The tool goes down to `Z6` on a helix around `a`. Then it makes 113 loops. Each loop cuts forward on a `G03` arc and comes back 0.3 mm above the floor (`return: "microlift"`). The last move is a half circle around `b`.

## The two programs

| | Slot | Trochoidal slot |
|---|---|---|
| Motion blocks | 19 | 803 |
| Arc blocks (`G02`, `G03`) | 0 | 459 |
| Cut levels | `Z9`, `Z6` | `Z6` only |
| Depth of cut | 3 mm | 6 mm |
| Cutting feed | `F600` | `F1200` |
| Length of the feed moves | 329 mm | 1455 mm |
| Time of the feed moves | 0.60 min | 1.28 min |

- The Slot program is short. The row at `Y15` cuts with the full tool diameter in the material. That is why its depth of cut and its feed are small.
- The Trochoidal slot program is 42 times longer. Each loop cuts only a thin crescent. `max_engagement: 60.0` keeps the contact angle at or below 60 degrees. That is why it can cut the full depth at two times the feed.
- `step: 0.8` is the largest step that the planner can use. The posted step is 0.44 mm, because the engagement limit is the tighter limit.

## What to look at

- In `slots.nc`, the Slot has no arc block. The moves between two rows are straight lines, for example `G01 X27. Y15. Z9.` after the row at `Y13`.
- The Slot leaves material in its round ends: at most 0.86 mm, on the wall at 45 degrees. The rows touch the round end only at their end points. `verify` does not report this. #2906 is the bug for it.
- The Trochoidal slot starts with four `G03` blocks at `F150` that go down from `Z14` to `Z6`. This is the helix entry.
- Each loop is the same group of seven blocks: one step along the wall at `Y-17`, two `G03` arcs at `Z6.` and `F1200`, one lift to `Z6.3`, two `G02` arcs back at `F2400`, and one move down to `Z6.` at `F150`.
- The Trochoidal slot cuts its two round ends completely: a full circle around `a` and a half circle around `b`, each of 2 mm radius.
- The model has flat chords on the round ends (64 for each half circle). The chords are inside the true 10 mm circle by less than 0.002 mm.

## Files

- `model/gen_slots_obj.py` — generator for the block and its two slots, with a watertight check and an area check.
- `model/slots.obj` — generated mesh imported by `build.camscript`.
- `build.camscript` — headless script building `slots.camproj` and posting `slots.nc` through CamCraft's undoable commands.
- `slots.camproj` — committed project with the two generated toolpaths.
- `slots.nc` — committed Fanuc NC program verified byte-for-byte in `crates/app/tests/example_catalog.rs`.

## Rebuild and verify

From the repository root:

```sh
cargo xtask catalog --case 2d/slots
cargo test -p camcraft-app --test example_catalog
```
