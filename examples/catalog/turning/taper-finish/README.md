# Taper turning: finish ride on a prepared cone

This original cone shows Taper Turning. The machine is
`assets/machines/lathe.mch` (`Lathe`). The setup uses the `Fanuc Lathe` post.
All dimensions are in millimetres.

## Geometry and stock

The cone spans Z0 to Z60. Its diameter decreases linearly from 20 at Z0 to
16 at Z60. `model/gen_taper_obj.py` makes the original closed mesh by
revolving this meridian with 48 angular segments. It checks that the edges
close.

The script embeds the mesh in the project and also uses it as the stock
shape. Thus the blank already has the taper. This example shows a finish
ride or spring pass, with clearance for radial retraction. It does not
remove the bulk material from a cylindrical blank.

## Operation and tool

`Taper finish` uses the `TN-1600-0R8` insert. Its window spans Z0 to Z60,
with bottom diameter 20 and top diameter 16. It uses a 0.05 mm tolerance
and Z70 clearance.

## What to inspect

Open `taper-finish.camproj` and inspect the cone, stock shape and finish
ride. Regenerate the operation and verify the setup.

In `taper-finish.nc`, find `TAPER TURNING`. The feed moves change X and Z
together, from near diameter 20 to diameter 16. The radial retract returns
to X20.1. X uses diameter values. The posted coordinates include the insert
orientation offsets.

## Rebuild

Run these commands from the repository root:

```sh
python3 examples/catalog/turning/taper-finish/model/gen_taper_obj.py
cargo xtask catalog --case turning/taper-finish
cargo xtask catalog --check
cargo test -p camcraft-app --test example_catalog
```

From this case directory, `camcraft-cli script build.camscript` also rebuilds
the project and NC program. The script stops if verification fails.

The tool holder body is not modelled or judged. These files are regression
examples. Machine setup checks and a machine prove-out remain necessary.
