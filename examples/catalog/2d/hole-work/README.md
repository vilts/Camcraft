# Hole work

This case shows Drill, Bore, Circular and Thread on an original
90 x 90 x 20 mm plate. The plate has four drill holes on a 60 mm bolt
circle. Two larger holes and one blind threaded hole show the other
operations. All dimensions are in millimetres.

| Operation | Tool | Position and cut |
|---|---|---|
| Bolt circle | DR-0600, diameter 6 | Four holes at (0, 30), (30, 0), (0, -30), (-30, 0) |
| Bore | EM-0600-4FL, diameter 6 | Diameter 16 at (-30, 30), through the plate |
| Circular | EM-0600-4FL, diameter 6 | Diameter 10 at (30, 30), down to Z0 |
| Tap drill | TAP-DRILL, diameter 6.6 | Blind hole at (0, 0), point down to Z4 |
| Thread | TM-0600-P125, diameter 6 | Right-hand internal M8 x 1.25 at (0, 0), Z8 to Z20 |

The drill and end mill use 25 mm flutes. `TAP-DRILL` starts from the
catalog drill and changes its diameter, shank diameter and flute length.
The tools have no modelled holders.

## What to inspect

- `Bolt circle` posts four G81 cycles. `Tap drill` posts one more G81 cycle.
- `Bore` posts arcs around (-30, 30) at successive depths.
- `Circular` uses a plunge entry and arcs around (30, 30).
- `Thread` posts G03 blocks with simultaneous XY and Z motion. The
  multi-form thread mill cuts one helical turn for each radial pass.
  Its tip moves between Z8 and Z9.875. Its axial cutting teeth cover
  the intended 12 mm thread length; the helix does not rise to Z20.
- The saved project embeds the mesh. It opens without a model source link.

The generated mesh uses 96 flats around each hole. The drill, bore and
circular holes have 0.2 mm of radial envelope clearance. The thread hole
uses the M8 x 1.25 minor radius, 3.3235 mm. Its modelled floor is Z4.
The model does not contain thread teeth.

The Circular cut uses one radial pass. A larger cut with several radial
passes exposed verification refusals tracked in #2898.

The drill point reaches Z-2.803 on the bolt circle. The bore reaches Z-1
because of its break-through setting. The verification report names these
moves below the stock. No fixture or spoil board is modelled. Provide
clearance below the plate before adapting this program to a machine.
This case has not run on a machine or in a control simulator.

## Rebuild

From the repository root:

```sh
python3 examples/catalog/2d/hole-work/model/gen_hole_work_obj.py
cargo xtask catalog --case 2d/hole-work
cargo xtask catalog --check
cargo test -p camcraft-app --test example_catalog
```

The generator checks that each mesh edge has one reverse edge. The build
script creates the document through commands, generates all operations,
requires a clean verification result, saves the project and posts the NC
program with the default Fanuc mill post.
