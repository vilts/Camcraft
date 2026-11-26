# Indexed and tilted pockets

This original example shows 3+1 indexed work on two opposite faces and 3+2 work on a sloped face.
Each operation uses a fixed tool axis during the cut.
The rotary axes move between operations.

The starting stock is a 30 × 30 × 35 mm box, centered at the origin.
Its bounds are X−15 to X15, Y−15 to Y15, and Z−17.5 to Z17.5.
The script fits this box to the model with zero margin.
The finished model removes one corner above the plane `n · p = 15`, where `n = (√3/4, −0.5, 0.75)`.
The model also has three square pockets, each 8 × 8 mm and 2 mm deep along its face normal.

| Order | Operation | Pocket center (mm) | Index | Floor distance along tool axis (mm) |
| --- | --- | --- | --- | --- |
| 1 | Front pocket | (−5, −15, −5) | A90 B0 | 13 |
| 2 | Back pocket | (−5, 15, −5) | A−90 B0 | 13 |
| 3 | Tilted pocket | (6.495191, −7.5, 11.25) | A30 B30 | 13 |

The first two operations index only A.
The third operation indexes both A and B.
The built-in `Tilt-head 5-axis` machine supplies the rotary axes.
All three operations use the 6 mm flat end mill `EM-0600-4FL`, without a holder.
They use 2 mm stepdowns, 2 mm stepovers, and zero wall allowance.
The compact stock keeps the largest projected stock height above the floor within the tool's 15 mm flute length.
That distance is approximately 14.122 mm for the tilted operation.
No verification check is disabled.

Each operation uses the whole model as its source.
CamCraft currently refuses tilted operations that use picked source faces.
The whole-model path preserves the material sections around each pocket.
The box stock includes material above the sloped face, so the third operation also removes this corner stock.
Its starting height comes from the stock bounds along the tilted tool axis.

Inspect the three pocket orientations in the project.
Inspect the A and B index words before each operation's cutting moves in `indexed.nc`.
The model mesh is embedded in `indexed.camproj`; the project has no external model link.
The generator checks that the mesh is closed and consistently oriented.

The script requires a clean verification result before it saves the project or posts the program.
Stock simulation cannot judge posed operations; the geometric gouge and collision checks still run.
Verification does not prove complete stock removal.
The 6 mm cutter leaves material in the sharp inside corners of each square pocket.
Its radius is 3 mm.
No physical CNC machine was run.
The example has no fixture or holder model.

From the repository root, regenerate the model and rebuild the case:

```sh
python3 examples/catalog/multiaxis/indexed/model/gen_indexed_obj.py
cargo xtask catalog --case multiaxis/indexed
```

Check the catalog and its project tests:

```sh
cargo xtask catalog --check
cargo test -p camcraft-app --test example_catalog
```
