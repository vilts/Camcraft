# Machine onboarding

Use this guide when documenting a machine definition or checking whether an existing CamCraft machine/post combination fits a proposed job. A machine file records only facts the current reader understands; it is not a controller profile, operation-capability certificate, or proof that a real machine is safe to run. A passing software test or synthetic fixture never certifies a physical machine.

## Collect evidence before editing

Record the exact machine variant and options, the manufacturer/OEM and model number, machine family (`mill`, `lathe`, or `mill-turn`), controller make/model and software version, and the intended control dialect and CamCraft post. Note whether specifications vary by spindle, gearing, option package, or controller revision. Do not infer a control dialect from the machine brand.

For each supported fact, capture its value, units, source, and any conditions or uncertainty:

- **Kinematics and axes:** linear/rotary axis letters, which physical member moves, axis direction, usable travel min/max, and the coordinate convention/zero those limits use. Record rotary direction, angular limits and resolution separately from linear travel. Include maximum normal and rapid rates for each axis where documented.
- **Units:** distinguish the machine's programming length units from angles. CamCraft scales inch linear distances and distance-per-minute rates to millimetres; rotary travel and rates remain degrees and degrees/minute.
- **Spindle/tooling:** minimum and maximum RPM; each documented gear/belt range; power; tool-changer capacity; tool length/diameter envelope; spindle count; coolant names and their actual controller words. Record rigid-tapping support explicitly when the source says yes or no.
- **Special capabilities and limits:** live tooling/C-axis commands, tilting/RTCP geometry, part transfer, or other options only where the current schema and post can express and check them. Separate “documented unsupported” from “not documented”; do not turn unknown into a guessed limit or capability.
- **Post fit:** name the exact post and dialect version being evaluated, and list operations/cycles the post writes, refuses, or only expands into ordinary moves. A generic “Fanuc-compatible” or “supports turning” label is not evidence for every cycle or option.

### Cite sources without importing vendor material

Use public machine-tool specification sheets, manuals, and controller references for numeric machine facts and observable control behavior, as allowed by the clean-room rules in [`AGENTS.md`](../AGENTS.md). Record enough bibliographic detail for another contributor to locate the source: publisher/manufacturer, document title and revision/date, exact section/table/page, public URL, date consulted, and stated license/reuse terms where available. A public URL alone does not grant redistribution rights.

Do not commit vendor manuals, scans, screenshots, copied vendor `.mch`/`.cps` files, or substantial copied prose/tables unless an explicit license permits it. Create an original CamCraft definition and post from cited facts and public standards; do not adapt a vendor file or template. If a relevant fact is not published or its reuse status is unclear, mark it unknown and ask the maintainer to resolve the evidence gap rather than guessing.

## Current `.mch` format and registration seams

Start with [`assets/machines/AGENTS.md`](../assets/machines/AGENTS.md) and the original examples [`vmc.mch`](../assets/machines/vmc.mch) and [`lathe.mch`](../assets/machines/lathe.mch). The reader consumes JSON with these established areas:

- `general`: `model`, `vendor`, and `kind` (`mill`, `lathe`, or `mill-turn`). The built-in style currently requires `general.vendor` to be `Camcraft`; retain the machine maker/model as cited source information rather than silently changing that project convention. The visible library label is separately supplied by the registry.
- `kinematics.default.units` and the nested `parts` tree: table/head and linear/rotary axis parts. Linear `travel` is in program coordinates; rotary travel is in degrees. `controller.synced_configuration.parts` supplies coordinate order and normal/rapid rates.
- `head.spindle`: min/max RPM, power, documented `ranges`, and `rigid_tapping`; `head.tool_station`: tool envelopes, coolant choices, and complete `live_tool` word sets where applicable. `tooling.number_of_tools` records changer capacity.

The current parser is [`parse_mch`](../crates/post/src/machine/parse.rs#L158) and its results feed app-side checking in [`machine::check_document`](../crates/app/src/machine/mod.rs#L114). A mounted machine is mirrored for the post in [`post::machine::mirror`](../crates/post/src/post/machine.rs#L38) and passed by [`posting::machine_value`](../crates/app/src/posting.rs#L151). Absent/zero rates generally mean “not stated,” not zero capability; invalid or omitted values do not create a safe, proven limit. The app gate checks only the facts and behaviors it implements.

**Two source gaps matter when authoring data:**

1. `assets/machines/AGENTS.md` says `//` provenance notes are tolerated, but the checked-in parser calls `serde_json::from_str` directly and the shipped `.mch` examples are standard JSON. Do not add comments that could make a file unparsable. Until that mismatch is resolved, keep source citations in the machine contribution's documentation/decision record and PR evidence; do not invent a `provenance` JSON key that the reader ignores.
2. The parser reads `spindle.rigid_tapping` (a boolean; any other value refuses the file by name, #786). A stated `false` makes the app's program gate refuse every tap cycle by name, through any post; the post mirror omits it when unknown and the script-side default is permissive (`true`). Unknown therefore does not mean “tapping blocked.” State `false` only when the source confirms the machine cannot rigid-tap, and `true` only when it confirms it can. The typed `.mch` reader does not retain controller identity/dialect or a general supported/unsupported-operation list. The raw standalone-post path can expose a top-level `capabilities` object to a script, but app-mounted machines drop it, and no built-in post in this tree reads it. Do not add ad hoc keys and assume the app checks them; record controller facts separately and identify any needed code follow-up.

To add a library machine, create the original `.mch` in `assets/machines/`, then add its named `include_str!` entry to [`posting::MACHINES`](../crates/app/src/posting.rs#L122). Update the exact expectations in [`every_shipped_machine_mounts_and_speaks_its_declaration`](../crates/app/tests/machine.rs#L719): every entry must parse, be sane, and agree between declared kind and axis-based inference. For source-backed numeric behavior, follow the stronger value-and-mirror assertions in [`tormach_machines.rs`](../crates/app/tests/tormach_machines.rs#L115).

Machine and post are separate choices. The built-in `.cps` registry is [`posting::BUILTINS`](../crates/app/src/posting.rs#L33); lathe-post classification/default selection is separately listed there. If a new post is actually needed, follow [`assets/posts/AGENTS.md`](../assets/posts/AGENTS.md), register the script there, and add dialect golden/conformance coverage. Do not claim that registering a machine automatically selects or validates a matching post.

## Existing software examples and operation coverage

These are source-tree capabilities and test evidence, not guarantees for a particular customer's control. `KindInfo::ALL` is the current app operation catalogue ([milling rows](../crates/app/src/operation/kinds.rs#L1657), [turning rows](../crates/app/src/operation/kinds.rs#L2007)); actual machine-kind admission is also enforced there. The machine-program gate is exercised from `posting::verification_report` ([source](../crates/app/src/posting.rs#L501)); `machine_limits_block_the_post_and_within_limits_does_not` in `crates/app/tests/posting.rs` shows a stated limit blocking and then allowing the same generated program.

| Requested operation | Current tree evidence | Onboarding status |
|---|---|---|
| Milling face, contour, slot, pocket | Implemented; motion coverage is in [`machining2d.rs`](../crates/motion/tests/machining2d.rs). The app posting fixture also posts a contour plus peck-drill job ([`posting.rs`](../crates/app/tests/posting.rs#L109)). | Software-supported; still test the selected post/dialect and machine limits. |
| Milling chamfer | `Chamfer2D` is a listed milling kind; see [`chamfer_circular.rs`](../crates/motion/tests/chamfer_circular.rs). | Software-supported; not a promise that a controller has a matching canned cycle. |
| Milling drill, bore, tap | `Drill`, `Bore`, and thread-milling `Thread` are distinct listed kinds. Drill cycles include tapping; `tormach_machines.rs` tests a real app posting path that emits G84 on a rigid-tap-capable machine and refuses it on two machines marked unable. | Software-supported with machine/post-dependent tapping. A tapping tool/cycle is not the same as a thread-milling operation. |
| Turning face and profile turning | `Facing`, `ProfileRough`, and `ProfileFinish` are listed turning kinds; app fixtures/gates are in [`turning.rs`](../crates/app/tests/turning.rs) and [`turning_profile.rs`](../crates/app/tests/turning_profile.rs). | Software-supported and tested. |
| Turning groove | `GrooveRough`/`GrooveFinish` are listed; the blade kernel fixture is [`turning_blade.rs`](../crates/motion/tests/turning_blade.rs), with posted groove/part goldens in [`lathe_post.rs`](../crates/app/tests/lathe_post.rs#L314). | Software-supported and tested. |
| Turning tap | The lathe family admits the axial `Drill` kind and the Fanuc Lathe script has a G84 branch, but the existing app-level lathe test found here exercises axial drilling, not an end-to-end tap job. | **Post path exists; end-to-end lathe tap coverage is missing.** Do not describe it as verified for a machine until a focused fixture test covers the chosen machine and post. |
| Turning thread | `Threading` is a distinct turning operation. [`turning_thread.rs`](../crates/motion/tests/turning_thread.rs) covers its generated helix; [`lathe_post_g76.rs`](../crates/app/tests/lathe_post_g76.rs) covers Fanuc Lathe G32/G76 output. | Software-supported and tested. Thread cutting is not tapping. |
| Turning knurl | No knurl operation kind, generator, or test was found in the inspected tree. | **Not implemented in this tree; not assigned by this documentation issue.** Do not promise or silently substitute another operation. |

There is no standard turning STEP/job fixture under `testdata/models/`. Turning examples are deliberately synthetic closed revolves built in the tests: for example [`bar_mesh`](../crates/app/tests/turning.rs#L70), [`flanged_mesh`](../crates/app/tests/turning_profile.rs#L100), and the modeled groove/thread fixtures in the motion suites above. They are useful software fixtures, not real machine programs. For milling, [`Milling-test.step`](../testdata/models/Milling-test.step) is an existing D-shaped plate with a blind bore; app tests use it for imported-model area, contour, pocket and related geometry cases (for example [`lead_approach.rs`](../crates/app/tests/lead_approach.rs#L68) and [`pocket3d_hsm.rs`](../crates/app/tests/pocket3d_hsm.rs#L60)). It is not a universal sample job or machine qualification part.

### Focused existing test commands

From the repository root, these commands target already-existing executable examples; select only the checks relevant to the change and run them on the authorized development environment:

```text
cargo test -p camcraft-app --test machine every_shipped_machine_mounts_and_speaks_its_declaration -- --exact
cargo test -p camcraft-app --test posting machine_limits_block_the_post_and_within_limits_does_not -- --exact
cargo test -p camcraft-app --test tormach_machines every_tormach_machine_states_its_manuals_figures -- --exact
cargo test -p camcraft-app --test tormach_machines a_tap_job_posts_on_the_1100mx_and_is_refused_on_the_770m -- --exact
cargo test -p camcraft-app --test lathe_post an_axial_drill_posts_through_the_document_with_the_lathe_post_chosen -- --exact
cargo test -p camcraft-app --test lathe_post_g76 a_g76_thread_posts_to_the_lathe_golden -- --exact
cargo test -p camcraft-app --test lead_approach every_lead_option_on_the_users_part_verifies_silent -- --exact
```

For generator-level examples, see the motion test binaries [`machining2d`](../crates/motion/tests/machining2d.rs), [`chamfer_circular`](../crates/motion/tests/chamfer_circular.rs), and the turning suites linked above. These tests validate software fixtures and translations, not the correctness of an OEM's machine facts or an actual machine's behavior.

## Headless tooling and the acceptance boundary

The standalone [`post` binary](../crates/post/src/bin/post.rs#L15) consumes a `.cps`, `.cnc`/`.cl` stream, output path, and optional `--machine <mch>`. It runs the post engine on that stream; it does **not** call the app's `.mch` parser/checker or prove that a generated job, fixture, machine limits, or physical setup is valid. Its warnings can accompany a written output, so output existence is not a pass.

The stdio MCP server can import models, set stock, add/generate/verify/post operations, but its current tool table has no direct `.mch` load/set-machine tool; `get_job` also omits the machine and `post` accepts only output/post arguments ([`mcp/tools.rs`](../crates/app/src/mcp/tools.rs#L23), [`mcp/job.rs`](../crates/app/src/mcp/job.rs#L89)). `open_project` can load machine data already serialized in a `.camproj` ([project application](../crates/app/src/project/apply.rs#L566)), and `verify`/`post` can judge that job through the program gate. This is not direct validation of the `.mch` parser/registration path; use the Rust examples above for that seam.

No physical-machine procedure is part of this guide or issue. A qualified machine owner must separately review the program and perform acceptance under the manufacturer's instructions and the shop's controls. Passing a software test, generating a synthetic posted fixture, or seeing no software warning is not machine certification.
