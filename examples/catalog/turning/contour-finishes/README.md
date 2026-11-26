# Contour: chamfering and multiple passes

This original stepped part shows Chamfering and Multi-Pass Turning. The
machine is `assets/machines/lathe.mch` (`Lathe`). The setup uses the
`Fanuc Lathe` post. All dimensions are in millimetres.

## Geometry and stock

The part spans Z0 to Z60. Its bottom corner has a 45 degree chamfer from
diameter 16 at Z0 to diameter 20 at Z2. The diameter stays 20 to Z12, then
steps to 18 and stays there to Z60.

`model/gen_contour_obj.py` makes the original part and cylindrical blank by
revolving their meridians with 48 angular segments. It checks that their
edges close. The blank is 20 mm in diameter and 60 mm long.

The script embeds both meshes in the project. It uses `contour-blank` as
stock and marks that source as raw stock only. The finished
`contour-finishes` model remains the part used for verification.

## Operations and tool

Both operations use the `TN-1600-0R8` insert, a 0.05 mm tolerance and Z70
clearance.

| Operation | What it shows |
|---|---|
| Chamfer bottom | Cut the 45 degree corner within Z0 to Z2. |
| Step rough and finish | Rough the whole profile within Z0 to Z60 with a 0.5 mm stepdown, then make three finishing rides. |

Roughing leaves 0.25 mm radially. The three finishing rides reduce the
remaining offsets to about 0.1667 mm, 0.0833 mm and zero. The final ride
reaches the model.

## What to inspect

Open `contour-finishes.camproj` and inspect the bottom chamfer, shoulder and
pass spacing. Regenerate both operations and verify the setup.

In `contour-finishes.nc`, find `CHAMFERING` before `MULTI-PASS TURNING`.
Inspect the diagonal chamfer moves, the roughing passes and the three
finishing rides. The final ride reaches diameter 18 on the upper step.
X uses diameter values. The posted coordinates include the insert
orientation offsets.

## Rebuild

Run these commands from the repository root:

```sh
python3 examples/catalog/turning/contour-finishes/model/gen_contour_obj.py
cargo xtask catalog --case turning/contour-finishes
cargo xtask catalog --check
cargo test -p camcraft-app --test example_catalog
```

From this case directory, `camcraft-cli script build.camscript` also rebuilds
the project and NC program. The script stops if verification fails.

The tool holder body is not modelled or judged. These files are regression
examples. Machine setup checks and a machine prove-out remain necessary.
