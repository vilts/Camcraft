# Rough and Finish (3D Pocket, 3D Finish Parallel and Waterline)

A 50 x 50 x 8 mm clean-room aluminum plate with a spherical dome on top
(36 mm base diameter, 10 mm high, top at `Z18`). The stock is a
50 x 50 x 22 mm box, so 4 mm of material is above the dome.

## What it shows

1. **3D Pocket (`3D Pocket1`, `EM-0600-4FL`):** Roughs the stock down to the
   plate top (`Z8`) in 2 mm levels, with a helical entry. The stock to
   leave is 0.5 mm, but the program applies it only sideways (#3669). It
   leaves no stock at the apex of the dome (the `Z18` level passes 0.0006 mm
   from it) and no stock on the plate (the last level is at `Z8`). On the
   flanks of the dome it leaves 0.21 to 0.38 mm.
2. **3D Finish, Parallel (`3D Finish Parallel`, `BN-0600`):** A raster of
   parallel passes along +X (direction angle 0) with a 1.5 mm stepover and a
   0.1 mm tolerance. The ball follows the dome and the plate top.
3. **3D Finish, Waterline (`3D Finish Waterline`, `BN-0600`):** Constant-Z
   loops around the dome. The step between the loops comes from the
   0.1 mm tolerance and the 3 mm ball radius.

## What to look at

- The roughing levels leave steps on the flanks of the dome. The two finish
  passes remove them.
- The case shows three open bugs. The pinned program shows the behaviour of
  today. Each fix makes the program again (`cargo xtask catalog --case
  3d/rough-finish`).
  - [#3669](https://github.com/NeoTech/Camcraft/issues/3669): the 3D Pocket
    applies the stock to leave only sideways. The apex of the dome and the
    plate get 0 mm.
  - [#3670](https://github.com/NeoTech/Camcraft/issues/3670): a re-entry in a
    level is a straight plunge into the stock, although the entry is a helix.
    The program has about 29 of them, for example NC lines 126-128.
  - [#3671](https://github.com/NeoTech/Camcraft/issues/3671) (fixed): the rapid
    between two levels stops at feed height above the last cut floor. The lowest
    `G0` in `3D Pocket1` is at `Z12`.
- The parallel passes are close on the flat top of the dome and far apart
  on its steep sides. The waterline loops are the opposite: close on the
  steep sides and far apart near the top. The two patterns together give
  an even finish.
- `cam.verify()` must report no finding before the post. The script stops
  if it does.

## Files

- `model/gen_rough_finish_obj.py` — parametric watertight height-field
  generator for the test solid.
- `model/rough-finish.obj` — generated triangle mesh imported by
  `build.camscript`.
- `build.camscript` — headless script that builds `rough-finish.camproj`
  and posts `rough-finish.nc` through CamCraft's undoable commands.
- `rough-finish.camproj` — committed project with generated toolpaths.
- `rough-finish.nc` — committed Fanuc NC program, verified byte for byte in
  `crates/app/tests/example_catalog.rs`.

## Rebuild and verify

From the repository root:

```sh
cargo xtask catalog --case 3d/rough-finish
cargo test -p camcraft-app --test example_catalog catalog_3d_rough_finish
```
