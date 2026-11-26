+++
title = "Crate map"
description = "The workspace crates and how they depend on each other."
weight = 1
+++

CamCraft is a Cargo workspace. Each crate holds one concern.

| Crate | Package | Role |
|---|---|---|
| `crates/geometry` | `camcraft-geometry` | Meshes, slicing, 2D regions, mesh repair |
| `crates/cl` | `camcraft-cl` | The cutter-location (CL) move stream |
| `crates/tools` | `camcraft-tools` | Tool library and tool laws |
| `crates/motion` | `camcraft-motion` | Toolpath generators: 2D, 3D, drilling, multi-axis, turning |
| `crates/sim` | `camcraft-sim` | Stock simulation and the verifier |
| `crates/post` | `camcraft-post` | The `.cps` post-processor engine and the standalone `post` binary |
| `crates/kernel` | `camcraft-kernel` | The B-rep kernel seam: a thin `Kernel` trait (primitives, booleans, offset, blends, measure) and the bench that compares candidate engines |
| `crates/app` | `camcraft-app` | Document model on a Flecs ECS world, commands with undo/redo, operations, posting, the `camcraft-cli` binary and its MCP server |
| `crates/ui` | `camcraft-ui` | The egui desktop interface |
| `xtask` | `xtask` | Project tooling: the test gates, packaging and release steps |

`assets/` holds the post processors (`assets/posts/*.cps`), machine
definitions (`assets/machines/*.mch`), UI icons, fonts and themes.
`testdata/` holds the test models.

## Layering

The layering is strict: **ui → app → flecs**. UI panels, widgets and
dialogs never touch the ECS world directly; they queue commands, and only
app-layer commands and systems make raw Flecs calls. ECS components are
declared in one place, `crates/app/src/schema.rs`, and are registered for
every binary at startup.

## Code shape

- Source files stay under about 400 lines; a growing file is split along
  its domain seam. Post-processor scripts are exempt: a `.cps` file is one
  script the post engine runs whole.
- Every module starts with a one-line provenance comment: what it
  implements and which public source it was written from.
- No platform-specific assumptions in core code: the same source builds on
  Windows, Linux and macOS.

## Never crash, never gouge

The project's first rule: output is correct, or it is refused with a
diagnostic. `unwrap()`, `expect()` and `panic!()` are not used outside
tests; errors are handled or propagated. Degenerate input (zero-area
triangles, broken STEP files, nonsense tolerances) gives a clean error, not
a crash or a hang. Anything a collision check or simulation flags as unsafe
blocks the post.

## Clean room

CamCraft reimplements user-visible behaviour (operation types, parameters,
workflow) from public documentation, G-code standards (RS-274, ISO 6983)
and machining handbooks. No vendor code, text, icons, tool libraries or
post processors are read or copied, and new toolpath algorithms are written
from published technical literature. The full protocol is in `AGENTS.md`.
