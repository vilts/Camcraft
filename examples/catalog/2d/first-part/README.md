# First Part (2D Face, 2D Pocket, Drill)

A 100 x 60 x 15 mm clean-room aluminum test plate machined from a 100 x 60 x 20 mm box stock.

## What it shows

1. **Face (`Face1`, `EM-1000-4FL`):** Faces the top 5 mm off the 20 mm stock down to `Z15` (top of the central island).
2. **2D Pocket (`2D Pocket1`, `EM-0600-4FL`):** Clears the 5 mm shelf around the 60 x 30 mm central island with a helical ramp entry down to `Z10`.
3. **Drill (`Drill1`, `DR-0600`):** Drills four 6 mm through-holes around the flange at `(±38, ±23)` using `G83` deep peck drilling (`Fanuc` built-in post).

## Images

These images use the same camera. Each operation image shows only that operation's toolpath.
The PNG files are published on the fork's `catalog-screenshots` branch.

Model without toolpaths:

![First-part model without toolpaths](https://raw.githubusercontent.com/mkinney/Camcraft/catalog-screenshots/catalog/2d/first-part/0-blank.png)

Face (`Face1`):

![First-part Face1 toolpath](https://raw.githubusercontent.com/mkinney/Camcraft/catalog-screenshots/catalog/2d/first-part/1-Face1.png)

2D Pocket (`2D Pocket1`):

![First-part 2D Pocket1 toolpath](https://raw.githubusercontent.com/mkinney/Camcraft/catalog-screenshots/catalog/2d/first-part/2-2D-Pocket1.png)

Drill (`Drill1`):

![First-part Drill1 toolpath](https://raw.githubusercontent.com/mkinney/Camcraft/catalog-screenshots/catalog/2d/first-part/3-Drill1.png)

## Files

- `model/gen_first_part_obj.py` — parametric watertight voxel-CSG generator for the test solid.
- `model/first-part.obj` — generated quad mesh imported by `build.camscript`.
- `build.camscript` — headless script building `first-part.camproj` and posting `first-part.nc` through CamCraft's undoable commands.
- `first-part.camproj` — committed project with generated toolpaths.
- `first-part.nc` — committed Fanuc NC program verified byte-for-byte in `crates/app/tests/example_catalog.rs`.

## Rebuild and verify

From the repository root:

```sh
cargo xtask catalog --case 2d/first-part
cargo test -p camcraft-app --test example_catalog
```
