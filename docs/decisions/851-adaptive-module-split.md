## #851 - Split the adaptive core without changing behavior

**Decision.** Extract `core_spiral` and `orbit` unchanged from `crates/motion/src/machining2d/adaptive/mod.rs` into a private `crates/motion/src/machining2d/adaptive/core.rs` child. Give only the moved methods the `pub(super)` visibility needed by their parent. Keep `SpiralTrial` and `SPIRAL_FRONT` local to the new module; leave shared `SPIRAL_STEPS` and `MAX_ROUNDS` in the parent because `troch_plan` and `rounds` also use them. Keep the parent orchestrator, plan/refusal order, public API, and algorithmic behavior unchanged; preserve constant values while moving only `SPIRAL_FRONT` as described. In `lift.rs`, reflow only three doc comments to reduce the file from 403 to 400 lines; preserve the provenance and executable source bytes while the separate #982 claim remains live. Add no dependency, tolerance change, algorithm, test, changelog entry, or golden update.

**Why.** The static baseline at `a6bd69d8527e0752cc58e6a02c1dae4971355268` has `adaptive/mod.rs` at 431 lines and `adaptive/lift.rs` at 403; every other file in the adaptive tree is at most 400 lines. A private core child isolates the spiral/orbit implementation at its natural seam without widening the API or changing the orchestration that controls fallback and refusals. The `lift.rs` comment-only reflow is the narrowest way to meet the same file-size limit without disturbing its executable body.

**Considered and dropped.** Moving the shared step/round limits was rejected because parent planning paths still consume them. Making the new module or methods public was rejected because only the existing parent needs access. Broader relocation, algorithm or tolerance edits, new tests, and golden changes were rejected as unnecessary for a behavior-preserving split and would enlarge the review surface.

**Static test and golden baseline (not executed).** The gate maps for `camcraft_motion::machining2d::adaptive` identify these motion integration suites:

| File | `#[test]` | `#[ignore]` |
| --- | ---: | ---: |
| `crates/motion/tests/adaptive_band.rs` | 2 | 0 |
| `crates/motion/tests/adaptive_hsm.rs` | 6 | 5 |
| `crates/motion/tests/adaptive_orbit_seam.rs` | 1 | 0 |
| `crates/motion/tests/pocket3d_hsm.rs` | 4 | 1 |
| **Mapped total** | **13** | **6** |

Thus 7 mapped tests are non-ignored by default; the 6 ignored tests are heavy-lane cases. Nearby but differently mapped coverage is `crates/motion/tests/rotary_adaptive.rs` (5 tests, 3 ignored; rotary/multiaxis) and `crates/app/tests/shoulder_adaptive.rs` (3 tests, none ignored; app operation/verify).

No dedicated 2D Adaptive `.cl` golden was found in the adaptive motion tests or app golden suite. `crates/motion/tests/adaptive_orbit_seam.rs` reads the 287-vertex input fixture `crates/motion/tests/fixtures/bore_z10_center.txt` and asserts the HSM plan retains fitted arcs; it is not a golden output. The adjacent `crates/app/tests/golden/finish3d_spiral_plateau.cl` is exercised as `FinishPattern::Spiral` by the separate 3D finishing generator, not 2D Adaptive. `crates/motion/tests/rotary_adaptive.rs` has an in-source `(count, digest)` assertion for the unchanged `OffsetRings` rotary program (`GOLDEN_RINGS = (5263, 11_592_310_142_122_113_043)`); it is not a golden for the moved 2D core.

**Recommended smallest gate after implementation (not run here).** Use the quick gate’s `machining2d::adaptive` map, limited to the four binaries above: `adaptive_band`, `adaptive_hsm`, `adaptive_orbit_seam`, and `pocket3d_hsm`; leave ignored heavy lanes to their release lane. Under the work-issue manual alternative, run those touched motion test binaries individually and `cargo clippy -p camcraft-motion --all-targets -- -D warnings`. Do not expand to unrelated app, rotary, or workspace-wide suites for this private, behavior-preserving move.

**Evidence boundary.** This is a static inventory and a preservation decision only. No Cargo command, test, compile, rustfmt, or native/runtime check was run; no pass is claimed. The source/test counts above describe the checked-in baseline, not execution results.

**Future cost.** The new private boundary keeps callers limited to the parent; future cross-child use must be deliberately routed or use a wider visibility. Reversing or extending this mechanical split later is local, while any algorithm or output change should be separately specified and validated against the affected motion suites.
