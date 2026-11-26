# Drill Overfeed (a refused post)

An 80 x 50 x 10 mm original test plate with two 6 mm through-holes at `(±25, 0)`.
This case shows a refusal: CamCraft does not post the program.

## What it shows

1. **Drill (`Drill1`, `DR-0600`):** The operation drills the two holes. Its feed and plunge feed are 400 mm/min.
   That is above the feed that the tool states for the work material (#2598).
2. The operation generates. The post refuses the program and names the operation and the feed.
   No NC program is written.

The script saves `drill-overfeed.camproj` before it asks for the post.
Open the project and post it to see the same refusal.
Lower the feed of `Drill1` to the tool's feed, or raise the tool's feed, to clear the refusal.

## Files

- `model/gen_drill_overfeed_obj.py` — generator for the watertight plate mesh.
- `model/drill-overfeed.obj` — generated mesh imported by `build.camscript`.
- `build.camscript` — headless script that builds and saves `drill-overfeed.camproj`, then asks for the post.
- `case.toml` — `expect = "refusal"`: the case result is a refusal, not a program.
- `drill-overfeed.camproj` — committed project file.
- `refusal.txt` — the refusal lines of the script run, compared byte for byte in `crates/app/tests/example_catalog.rs`.

## Rebuild and verify

From the repository root:

```sh
cargo xtask catalog --case 2d/drill-overfeed
cargo test -p camcraft-app --test example_catalog catalog_2d_drill_overfeed
```
