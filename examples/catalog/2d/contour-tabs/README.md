# Contour with Tabs (2D Contour, lead-in, lead-out, tabs)

A 90 x 50 x 6 mm clean-room plate with four 10 mm corner chamfers. One 2D Contour cuts it out of a 120 x 80 x 6 mm sheet. Four tabs hold the plate in the sheet.

## What it shows

1. **2D Contour (`2D Contour1`, `EM-0600-4FL`):** Cuts the outside of the plate, climb, in three levels: `Z4`, `Z2` and `Z0` (the sheet bottom). The stepdown is 2 mm.
2. **Lead-in:** A 3 mm arc (`lead: { arc: { radius: 3.0 } }`). Each cut run starts with one `G03` block that ends tangent to the outline.
3. **Lead-out:** A 4 mm line (`links.approach.lead_out`). Each cut run ends with one `G01` block before the tool lifts.
4. **Tabs:** Four tabs, 8 mm wide and 2 mm high. The levels `Z4` and `Z2` cut the whole ring. `Z2` is the tab top. The level `Z0` is four runs, and the tool lifts over each tab.

## What to look at

- In `contour-tabs.nc`, the levels `Z4` and `Z2` each have one `G03` lead-in and one lift.
- The level `Z0` has four `G03` lead-ins and four lifts. The first three lifts are `G01` moves to the retract height `Z11`. The last lift is the `G00` move to the clearance height `Z21`.
- The last three `Z0` runs start where the levels above did not cut under the full tool. Each dive there pecks: `G01` to `Z4`, `Z2`, then `Z0`, with a `G00` retract to `Z8` between the pecks.
- The gap between two `Z0` runs is larger than the tab width plus the tool diameter (14 mm). The tab fit moves the runs apart until the two lead moves leave the full 8 mm of the tab.
- The outline corners are `G02` arcs of 3 mm radius: the tool centre rolls around each sharp corner of the plate.
- `build.camscript` sets `lead` with `linear: null`. `add_operation` applies `params` as a merge patch on the seeded linear lead, and a lead holds one shape only (#2876).
- The program cuts to `Z0`, the bottom of the sheet. On a machine, the sheet is on a spoilboard.

## Files

- `model/gen_contour_tabs_obj.py` — generator for the plate: a prism over one octagon, with a watertight check.
- `model/contour-tabs.obj` — generated mesh imported by `build.camscript`.
- `build.camscript` — headless script building `contour-tabs.camproj` and posting `contour-tabs.nc` through CamCraft's undoable commands.
- `contour-tabs.camproj` — committed project with the generated toolpath.
- `contour-tabs.nc` — committed Fanuc NC program verified byte-for-byte in `crates/app/tests/example_catalog.rs`.

## Rebuild and verify

From the repository root:

```sh
cargo xtask catalog --case 2d/contour-tabs
cargo test -p camcraft-app --test example_catalog
```
