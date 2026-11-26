# Outside groove and part-off

This case cuts an outside groove with Groove Rough and Groove Finish, then separates the part with Parting.
It uses the built-in `Lathe` machine (`assets/machines/lathe.mch`) and the `Fanuc Lathe` post.

## The part and tool

The stock is a 30 mm diameter cylinder from Z0 to Z90.
The finished part extends from Z15 to Z90. The stock below Z15 provides the chuck grip.
An annular groove extends from Z35 to Z41. Its floor has a 22 mm diameter, 4 mm below the outside radius.

All three operations use `GT-0300`, a 3 mm grooving blade.
The script embeds the original generated mesh in the project and removes its source link.
Opening the project does not require a model file at its original path.

| Order | Operation | What it shows |
|---|---|---|
| 1 | `Groove rough` | Two blade positions cover the 6 mm groove. A 0.25 mm radial allowance remains. |
| 2 | `Groove finish` | Two plunges remove the allowance and reach the 22 mm floor diameter. |
| 3 | `Part off` | Radial pecks of 0.5 mm reach the axis with the whole kerf below the part's end face at Z15. |

## What to inspect

Open `groove-part-off.camproj` and check the operation order, groove floor, and separation plane.
The script generates all three operations and requires a clean verification result before it saves and posts.

Inspect these features in `groove-part-off.nc`:

- `G18`, `G99`, and `T0101` select the lathe plane, feed for each revolution, and grooving blade.
- X words specify diameters. The rough groove plunges reach `X22.5`, then the finish plunges reach `X22.`.
- Groove plunges use posted Z35 and Z38. The blade centers are Z36.5 and Z39.5. Orientation 2 places the control point at the blade's low Z edge.
- The part-off plane Z15 is the part's end face. The kerf spans Z12 to Z15, in the bar below the part, and the blade center is Z13.5. Orientation 2 posts its low edge at Z12.
- The part-off pecks reach `X0.`. Successive pecks decrease the diameter by 1 mm.
- Rapid moves at `X30.1` clear the 30 mm stock diameter.

The tools have no modeled holder body. This case does not demonstrate holder collision checks.
The project and NC program are regression fixtures. Check the setup and tooling before machining.

## Files

- `model/gen_groove_part_off_obj.py` creates the original mesh by revolving a meridian in 48 steps. It checks that the mesh is closed.
- `model/groove-part-off.obj` is the generated mesh.
- `build.camscript` creates the project through existing undoable commands, generates, verifies, saves, and posts.
- `groove-part-off.camproj` stores the embedded mesh and generated operations.
- `groove-part-off.nc` stores the expected posted program.

## Rebuild and verify

Run these commands from the repository root:

```sh
python3 examples/catalog/turning/groove-part-off/model/gen_groove_part_off_obj.py
cargo xtask catalog --case turning/groove-part-off
cargo xtask catalog --check
cargo test -p camcraft-app --test example_catalog
```

The catalog checks reopen and regenerate the saved project. They verify the result and compare the posted program byte for byte.
They also rebuild each case from its script and compare the resulting NC program.
