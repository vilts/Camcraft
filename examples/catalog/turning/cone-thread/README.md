# Cone with a threaded end (Facing, Profile Rough, Profile Finish, Groove Finish, Threading, Parting)

A spun turning profile: a 30 mm bar turned to a cone with an M10x1.5 threaded end, on a 2-axis lathe with the Fanuc-style lathe post.

## The part

The bar is on the Z axis. Z0 is at the chuck and Z79 is the free end.

| Z | Feature |
|---|---|
| 15 to 20 | collar, 30 mm diameter (the bar's own diameter) |
| 20 to 60 | cone, 20 mm down to 10 mm diameter over 40 mm |
| 60 to 63 | neck (thread relief), 8 mm diameter |
| 63 to 78 | M10x1.5 external thread, 15 mm long, 0.92 mm deep |

The 15 mm below the part is the chuck's grip. The part-off plane is Z15.

## Walk through

`build.camscript` does these steps through CamCraft's undoable commands.

1. **Import two models.** `cone-blank` is the turned blank: the part before the neck and the thread are cut. `cone-thread` is the finished part. The thread is modelled as ten V grooves, because the Threading operation cuts a modelled thread.
   The script uses `set_model_link` with `path: null` after the imports. Both meshes stay embedded in the project. Opening the project does not depend on the working directory.
2. **Set the stock.** A cylinder, 30 mm diameter, 79 mm long.
3. **Mount the lathe.** `load_machine` mounts the built-in `Lathe` (`assets/machines/lathe.mch`). `add_setup` declares the setup `Turning`, and `set_setup_post` gives it the post `Fanuc Lathe` (`assets/posts/fanuc-lathe.cps`).
4. **Add the tools.** T1 `TN-1600-0R8` (turning insert, 0.8 mm nose), T2 `GT-0300` (3 mm grooving blade), T3 `TD-1200-60` (60 degree threading insert, 1.2 mm wide, defined in the script).
5. **Add the operations**, in program order:

| # | Operation | Kind | Tool | Source | What it cuts |
|---|---|---|---|---|---|
| 1 | `Face` | Facing | T1 | `cone-blank` | 1 mm off the free end, Z79 to Z78, in two passes |
| 2 | `Rough profile` | Profile Rough | T1 | `cone-blank` | the thread diameter, the cone and the shoulder, 1 mm rungs, 0.25 mm left |
| 3 | `Finish profile` | Profile Finish | T1 | `cone-blank` | one ride of the nose over the same profile |
| 4 | `Neck` | Groove Finish | T2 | `cone-thread` | the 8 mm neck, one plunge at Z60 |
| 5 | `Thread` | Threading | T3 | `cone-thread` | M10x1.5, four passes to the 8.16 mm root |
| 6 | `Part off` | Parting | T2 | `cone-thread` | the part from the bar at Z15 |

6. **Generate, verify, save, post.** The script stops if the verify verdict is not clean.

## What to inspect in `cone-thread.nc`

- **The lathe dialect:** `G18`, `G99` (feed for each revolution), diameter X words, `T0101` / `T0202` / `T0303`.
- **The taper:** in `Finish profile`, the blocks `G01 X19.812 Z20.` to `G01 X10.212 Z58.4` cut X and Z together. X falls 10 mm over 40 mm of Z. X and Z are the insert's control point (nose orientation 2): one nose radius (0.8 mm) from the nose centre toward the axis and toward the chuck.
- **The thread:** in `Thread`, each pass is a `G01` plunge at Z77.4 and a run of `G32 Z... F1.5` blocks down to Z63.9. The pass depths are X9.5, X9., X8.5 and X8.16.
- **The neck:** `G01 X8. Z60.` with the 3 mm blade.
- **The part-off:** pecks at Z12 down to `X0.`. The 3 mm kerf is Z12 to Z15, below the part's end face.

## Limits that shaped this case

Each limit has its own issue. The case can become simpler when they are fixed.

- **Two models:** Profile Finish reads the thread valleys and the neck step of the finished part as rest, and blocks the post ([#2880](https://github.com/NeoTech/Camcraft/issues/2880)).
- **G32 passes, not the G76 cycle:** a thread pass cannot start in front of the end face ([#2879](https://github.com/NeoTech/Camcraft/issues/2879)), so a G76 cycle would start inside the part ([#2878](https://github.com/NeoTech/Camcraft/issues/2878)). The G32 passes plunge at feed.
- **A sharp shoulder:** a fillet at the shoulder sends the profile rides far out of the bar ([#2881](https://github.com/NeoTech/Camcraft/issues/2881)).
- **The verify verdict** says that the lathe stock simulation does not cut threads. The thread is judged by its own operation verdict.
- **The holders are not judged:** the tools have no modelled body.

## Files

- `model/gen_cone_thread_obj.py` — clean-room generator: one meridian polyline for each model, revolved in 48 steps, checked closed.
- `model/cone-blank.obj`, `model/cone-thread.obj` — the generated meshes that `build.camscript` imports.
- `build.camscript` — headless script that builds `cone-thread.camproj` and posts `cone-thread.nc`.
- `cone-thread.camproj` — committed project with both meshes embedded, no model source links, and generated toolpaths.
- `cone-thread.nc` — committed Fanuc lathe program, verified byte for byte in `crates/app/tests/example_catalog.rs`.

## Rebuild and verify

From the repository root:

```sh
python3 examples/catalog/turning/cone-thread/model/gen_cone_thread_obj.py
cargo xtask catalog --case turning/cone-thread
cargo test -p camcraft-app --test example_catalog
```

The app tests `every_catalog_case_opens_regenerates_verifies_and_posts_byte_for_byte`
and `every_catalog_case_rebuilds_from_camscript` check that both models keep
their embedded meshes without source links. They cover the committed and
rebuilt projects. The UI test `catalog_cases_open_and_frame_cleanly` checks
opening the committed project from `crates/ui`:

```sh
cd crates/ui
cargo test -p camcraft-ui --test catalog_render
```
