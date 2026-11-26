# Plate edge work

This original 60 x 40 x 6 mm plate compares four closed edge operations.
The stock is the finished plate. This case does not rough a blank.

| Operation | Tool | Inspect |
| --- | --- | --- |
| Chamfer | 6 mm, 90 degree cone | A 0.8 mm wide chamfer on the top outline. |
| Deburr | Same cone | A 0.3 mm edge break. The tip window excludes the bottom edges. |
| Engrave | 6 mm end mill | A shallow closed perimeter mark at Z 5.8. |
| Trace | Same end mill | The same closed path and depth as the engraved mark. |

Engrave and Trace intentionally share the plate boundary. Compare their
posted paths. They show closed perimeter marking, not text or open curves.
Chamfer and Deburr also compare two edge treatments on the same model.
These repeated cuts form a debugging example, not a production sequence.
Deburr reports skipped vertical corner edges that the vertical cone cannot
reach. This informational note leaves the top outline cut complete.

Inspect the four operation sections in `edge-work.nc`. Each must contain
cutting moves. Check the cone offsets and the shallow marking depth.
The project embeds its original mesh and has no external model link.

Regenerate the model with `python3 model/gen_edge_work_obj.py`.
From the repository root, rebuild with
`cargo xtask catalog --case 2d/edge-work`.
Run `cargo xtask catalog --check` and
`cargo test -p camcraft-app --test example_catalog` to check reproducibility.
