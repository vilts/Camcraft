# Bushing: boring and form turning

This original part shows Boring and Form Turning on one axis. The machine is
`assets/machines/lathe.mch` (`Lathe`). The setup uses the `Fanuc Lathe` post.
All dimensions are in millimetres.

## Geometry and stock

The part has a 20 mm outside diameter and spans Z0 to Z60. Its blind bore
has a 12 mm diameter and a floor at Z20. An outside groove spans Z28 to Z32.
The groove has a 6 mm crown radius and a 16 mm minimum diameter at Z30.

`model/gen_bushing_obj.py` makes both original meshes by revolving their
meridians with 48 angular segments. It checks that their edges close.
The blank has the same outside cylinder and an 11.2 mm pilot hole to Z20.
The pilot hole clears the boring bar before it cuts.

The script embeds both meshes in the project. It uses `bushing-blank` as
stock and marks that source as raw stock only. The finished `bushing` model
remains the part used for verification.

## Operations and tools

| Operation | Tool | What it shows |
|---|---|---|
| Bore finish | `BR-1000` | Finish the bore wall only between Z42 and Z58. |
| Form groove | `FG-0600-04` | Plunge the crown gauge at Z30 to form the outside groove. |

Both operations use a 0.05 mm tolerance and Z70 clearance. This is a partial
machining example. It does not finish the full bore depth or make the pilot
hole.

## What to inspect

Open `bushing.camproj` and inspect the pilot hole, the boring window and the
groove section. Regenerate both operations and verify the setup.

In `bushing.nc`, find `BORING` and `FORM TURNING`, in that order. The bore
cuts near X12. The form plunge cuts near X16 at Z30. X uses diameter values.
The posted coordinates include the insert orientation offsets.

## Rebuild

Run these commands from the repository root:

```sh
python3 examples/catalog/turning/bushing/model/gen_bushing_obj.py
cargo xtask catalog --case turning/bushing
cargo xtask catalog --check
cargo test -p camcraft-app --test example_catalog
```

From this case directory, `camcraft-cli script build.camscript` also rebuilds
the project and NC program. The script stops if verification fails.

The tool holder bodies are not modelled or judged. These files are regression
examples. Machine setup checks and a machine prove-out remain necessary.
