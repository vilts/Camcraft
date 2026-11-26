# Adaptive roughing and Pocket finishing

This original case shows 2D Adaptive roughing followed by 2D Pocket finishing.
The stock is a 60 × 40 × 12 mm box, centered in X and Y, with its bottom at Z0.
The model has a U-shaped pocket with a floor at Z6 and an open end at X30.
The pocket spans X−10 to X30 and Y−10 to Y10.

Both operations use the 6 mm flat end mill `EM-0600-4FL`, without a holder.
Both select the recognized pocket floor.
The imported mesh is embedded in the project, so reopening it requires no external model path.

| Order | Operation | Wall stock | Bottom Z | Purpose |
| --- | --- | --- | --- | --- |
| 1 | `Adaptive rough` — 2D Adaptive | 0.5 mm | 6.5 mm | Leave 0.5 mm on the walls and floor. |
| 2 | `Pocket finish` — 2D Pocket, `offset_rings` | 0 mm | 6 mm | Remove the wall and floor allowances. |

The roughing floor allowance comes from the explicit bottom height of Z6.5.
`stock_to_leave` sets the radial wall allowance.
Adaptive rounds the wall corners and reports up to 0.71 mm of extra corner scallop beyond the nominal 0.5 mm wall allowance.
Pocket finishing recuts the full selected pocket region; this case does not use rest machining.

Inspect the operation order and the different bottom heights in the project and NC program.
Compare the Adaptive passes with the Pocket offset rings.
Check that the roughing operation stops above the floor and that finishing reaches Z6.
The 6 mm cutter leaves material in the model's sharp inside corners.
A clean verification result checks for gouges; it does not prove that all unwanted stock is removed.

Regenerate the original mesh from this case's `model/` directory:

```sh
python3 gen_adaptive_pocket_obj.py
```

Rebuild the project and NC program from the repository root:

```sh
cargo xtask catalog --case 2d/adaptive-pocket
```

`build.camscript` generates both operations, verifies them, saves `adaptive-pocket.camproj`, and posts `adaptive-pocket.nc`.
The generator checks that its mesh is watertight and consistently oriented before writing it.
