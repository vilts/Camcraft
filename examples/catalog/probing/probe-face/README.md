# Probe before Face

An original 100 x 60 x 12 mm block sits in 100 x 60 x 13 mm stock.
The program probes the stock top before it faces the block to its final height.
The case uses the default 3-axis mill and the generic Fanuc post.

## What it shows

1. **Probe (`Probe1`, `PRB-0400`):** Touches the stock top at `[0, 0, 13]` in the negative Z direction. The 4 mm probe uses a fast touch and a slow touch. The operation selects the `G54` work offset.
2. **Face (`Face1`, `EM-1000-4FL`):** Faces the stock at `Z12` with a 10 mm end mill. It removes the 1 mm allowance above the model.

## What to look at

- In `probe-face.nc`, the `Probe1` section comes before the `Face1` section.
- The probe section contains `G31` touches at `F300` and `F30`. Its `G91 G01` moves retract the probe.
- `G10 L2 P1 Z[#5223+#190]` updates the G54 Z offset before the Face section starts.
- The tool changes from the probe to the end mill before the Face operation starts.
- The Face section contains `G01` cutting moves at `Z12`.
- The project embeds the model. It opens without a link to the generator's working directory.

## Files

- `model/generate.py` creates the original block mesh in `model/probe-face.obj`.
- `build.camscript` creates the operations through undoable commands, generates them, verifies the result, saves the project, and posts the program.
- `case.toml` links the case to the Probe and Face guide entries.
- `probe-face.camproj` holds the project and the generated toolpaths.
- `probe-face.nc` holds the committed program.

## Rebuild and verify

From the repository root:

```sh
cargo xtask catalog --case probing/probe-face
cargo xtask catalog --check
cargo test -p camcraft-app --test example_catalog
```
