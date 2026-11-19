//! Pins the quick tier's selection against the REAL suite headers (#922): a change under a crate
//! that M4.1 split out of `app` must select the app suites whose Gate map names that module.
//! Provenance: our own build law (AGENTS.md "Test the change, not everything"), #919 research.

use std::path::{Path, PathBuf};

use crate::plan::{Plan, Step};
use crate::quick::{self, Diff, Quick};
use crate::scan;

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).parent().map(Path::to_path_buf).unwrap_or_default()
}

/// The `--test` binaries of the `<crate> suites` step that quick plans for one changed file.
fn selected(file: &str, krate: &str) -> Vec<String> {
    selected_at(&root(), file, krate)
}

fn selected_at(root: &Path, file: &str, krate: &str) -> Vec<String> {
    let p = planned_at(root, file);
    let got = selected_in(&p, krate);
    eprintln!("selection {file} -> {krate}: {} suite(s)", got.len());
    got
}

fn planned_at(root: &Path, file: &str) -> Plan {
    let changed = vec![file.to_string()];
    let hunks = |_: &str| -> Vec<String> { Vec::new() };
    let diff = Diff { changed: &changed, untracked: &[], hunks: &hunks, before: &|_| None };
    let Ok(Quick::Plan(p)) = quick::plan(root, &diff, false) else { panic!("{file} did not plan") };
    p
}

fn selected_in(p: &Plan, krate: &str) -> Vec<String> {
    let steps: Vec<&Step> = p.steps.iter().filter(|s| s.name.starts_with(&format!("{krate} suites: "))).collect();
    let got: Vec<String> = steps
        .iter()
        .flat_map(|s| s.args.iter().enumerate().filter(|(_, x)| *x == "--test").filter_map(|(i, _)| s.args.get(i + 1).cloned()))
        .collect();
    got
}

fn count(file: &str, krate: &str) -> usize {
    selected(file, krate).len()
}

struct InputFixture(PathBuf);

impl InputFixture {
    fn new(name: &str) -> Self {
        Self(std::env::temp_dir().join(format!("xtask-{name}-{}", std::process::id())))
    }

    fn write(&self, path: &str, text: &str) {
        let path = self.0.join(path);
        std::fs::create_dir_all(path.parent().expect("fixture parent")).expect("fixture directory");
        std::fs::write(path, text).expect("fixture source");
    }
}

impl Drop for InputFixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// #2740: importer changes also run the independent STEP and feature-edge checks.
#[test]
fn importer_changes_select_step_and_feature_edge_suites() {
    let assert_suites = |root: &Path, file: &str| {
        let plan = planned_at(root, file);
        let kernel = selected_in(&plan, "kernel");
        for must in ["step_agreement", "machining_mesh"] {
            assert!(kernel.iter().any(|b| b == must), "{file}: {must} missing from {kernel:?}");
        }
        let app = selected_in(&plan, "app");
        assert!(app.iter().any(|b| b == "feature_edge_g1_join"), "{file}: feature_edge_g1_join missing from {app:?}");
    };
    assert_suites(&root(), "crates/app/src/geometry/import/step.rs");
    let temp = InputFixture::new("import-paths");
    temp.write("crates/app/tests/feature_edge_g1_join.rs", "");
    temp.write("crates/kernel/tests/step_agreement.rs", "");
    temp.write("crates/kernel/tests/machining_mesh.rs", "");
    for file in [
        "crates/app/src/geometry/import/mod.rs",
        "crates/app/src/geometry/import/step.rs",
        "crates/app/src/geometry/import/stl.rs",
        "crates/kernel/src/solidcraft/mod.rs",
        "crates/kernel/src/solidcraft/zip.rs",
        "crates/kernel/src/solidcore_step.rs",
        "crates/kernel/src/solidcore_import.rs",
    ] {
        assert_suites(&temp.0, file);
    }
}

/// #2740: the regression fixture selects the real model-directory readers.
#[test]
fn changed_testdata_selects_its_directory_readers() {
    let file = "testdata/models/bar_cross_holes.step";
    let plan = planned_at(&root(), file);
    let kernel = selected_in(&plan, "kernel");
    for must in ["step_agreement", "machining_mesh"] {
        assert!(kernel.iter().any(|b| b == must), "{file}: {must} missing from {kernel:?}");
    }
    let app = selected_in(&plan, "app");
    assert!(app.iter().any(|b| b == "feature_edge_g1_join"), "{file}: feature_edge_g1_join missing from {app:?}");
}

/// #2740: resolve paths in directory-suite helpers without reading fixture files.
#[test]
fn fixture_paths_select_cross_crate_directory_helpers() {
    let temp = InputFixture::new("input-paths");
    let write = |path: &str, text: &str| temp.write(path, text);
    write("crates/kernel/tests/reader/main.rs", "mod helper;\n");
    write("crates/kernel/tests/reader/helper.rs", concat!(
        "const MODELS: &str = \"../../../../testdata/models/nested\";\n",
        "const LEGACY: &str = include_str!(\"../../../app/tests/fixtures/legacy.camproj\");\n",
        "const SNAPSHOT: &str = \"../../../ui/tests/snapshots/layout\";\n",
    ));
    write("crates/kernel/tests/golden_reader.rs", "const GOLDEN: &str = \"../app/tests/golden/nested\";\n");
    write("crates/kernel/tests/formatted_reader.rs", "const GOLDEN: &str = \"crates/app/tests/golden/nc/{sub}/arc_planes.nc\";\n");
    write("crates/kernel/tests/unrelated.rs", "const MODELS: &str = \"../../testdata/other\";\n");
    for (file, must) in [
        ("testdata/models/nested/new.step", "reader"),
        ("testdata/models/nested/deleted.step", "reader"),
        ("crates/app/tests/fixtures/legacy.camproj", "reader"),
        ("crates/ui/tests/snapshots/layout/new.png", "reader"),
        ("crates/app/tests/golden/nested/new.nc", "golden_reader"),
        ("crates/app/tests/golden/nc/fanuc/arc_planes.nc", "formatted_reader"),
    ] {
        let got = selected_at(&temp.0, file, "kernel");
        assert!(got.iter().any(|b| b == must), "{file}: {must} missing from {got:?}");
        assert!(!got.iter().any(|b| b == "unrelated"), "{file}: unrelated fixture reader selected: {got:?}");
    }
    assert!(selected_at(&temp.0, "testdata/models_extra/new.step", "kernel").is_empty());
}

/// #2775: a fixture-only edit must run library readers, including nested test helpers.
#[test]
fn fixture_only_changes_select_library_readers_and_keep_integration_readers() {
    let temp = InputFixture::new("library-input-paths");
    temp.write("crates/app/src/lib.rs", "mod geometry;\n");
    temp.write("crates/app/src/geometry/mod.rs", "#[cfg(test)]\nmod tests;\n");
    temp.write("crates/app/src/geometry/tests.rs", "mod helper;\n");
    temp.write("crates/app/src/geometry/tests/helper.rs", "const MODELS: &str = \"../../testdata/models\";\n");
    temp.write("crates/ui/src/lib.rs", "#[cfg(test)]\nmod tests {\n    const MODELS: &str = \"../../testdata/models/{file}\";\n}\n");
    temp.write("crates/kernel/src/lib.rs", "const OTHER: &str = \"../../testdata/other\";\n");
    temp.write("crates/app/tests/reader.rs", "const MODELS: &str = \"../../testdata/models\";\n");
    for file in ["testdata/models/box.step", "testdata/models/deleted.step"] {
        let plan = planned_at(&temp.0, file);
        for krate in ["app", "ui"] {
            let libs: Vec<_> = plan.steps.iter().filter(|s| {
                s.name == format!("{krate} suites: lib")
                    && s.args.iter().any(|a| a == "--lib")
            }).collect();
            assert_eq!(libs.len(), 1, "{file}: {krate} library reader must run once: {:?}", plan.steps);
            assert!(!libs[0].args.iter().any(|a| a == "--test" || a == "--exact"), "library target must run whole");
        }
        assert!(plan.steps.iter().all(|s| !s.name.starts_with("kernel ")), "unrelated library selected: {:?}", plan.steps);
        assert_eq!(selected_in(&plan, "app"), ["reader"], "integration reader must remain selected");
        assert!(!plan.steps.iter().any(|s| s.args.windows(2).any(|w| w == ["--test", "lib"])), "lib is not an integration target");
    }
}

fn assert_real_library_readers(file: &str) {
    let plan = planned_at(&root(), file);
    for krate in ["app", "ui"] {
        assert!(plan.steps.iter().any(|s| {
            s.name == format!("{krate} suites: lib") && s.args.iter().any(|a| a == "--lib")
        }), "{file}: {krate} library reader missing: {:?}", plan.steps);
    }
}

/// #2775: unchanged real app/UI sources read this model directory from library tests.
#[test]
fn changed_box_fixture_selects_real_library_readers() {
    assert_real_library_readers("testdata/models/box.step");
}

/// #2775: the review's second fixture-only probe must also select the app/UI libraries.
#[test]
fn changed_bspline_fixture_selects_real_library_readers() {
    assert_real_library_readers("testdata/models/bspline_discontinuous.step");
}

/// #2411: the DXF profile MCP suite must run when its app entry points change.
#[test]
fn tool_dxf_profile_mcp_is_selected_for_its_app_entry_points() {
    for file in [
        "crates/app/src/mcp/mod.rs",
        "crates/app/src/tool_profile_dxf.rs",
        "crates/app/src/commands/mod.rs",
    ] {
        let got = selected(file, "app");
        assert!(got.iter().any(|b| b == "tool_dxf_profile_mcp"), "{file}: DXF profile MCP suite missing from {got:?}");
    }
}

/// #2361: rotated WCS coverage must run when its app entry points change.
#[test]
fn wcs_rotation_control_is_selected_for_its_app_entry_points() {
    let src = scan::read(&root().join("crates/app/tests/wcs_rotation_control.rs"));
    assert_eq!(src.matches("Gate map").count(), 1, "the WCS suite must have one combined map");
    let map = scan::gate_map(&src).expect("WCS control gate map");
    let modules: Vec<_> = map.split(|c: char| !(c.is_alphanumeric() || c == '_' || c == ':')).collect();
    for must in [
        "app::commands", "motion::drilling", "geometry::geo2d", "app::geometry",
        "app::machine", "app::model", "app::operation", "app::posting", "app::schema",
        "geometry::geometry::stock", "app::world", "tools::catalog", "app::stock_sim",
    ] {
        assert!(modules.contains(&must), "WCS control map must retain {must}: {map}");
    }
    for file in [
        "crates/app/src/commands/mod.rs",
        "crates/app/src/operation/mod.rs",
        "crates/app/src/posting.rs",
        "crates/app/src/stock_sim.rs",
    ] {
        let got = selected(file, "app");
        assert!(got.iter().any(|b| b == "wcs_rotation_control"), "{file}: WCS suite missing from {got:?}");
    }
}

#[test]
fn post_change_selects_the_post_dialect_suites() {
    let got = selected("crates/post/src/post/mod.rs", "app");
    assert!(got.iter().any(|b| b == "conformance"), "conformance missing from {got:?}");
    // #921 / #926: fanuc_post and the heidenhain / doosan suites live in camcraft-post, so a post change selects them there.
    let own = selected("crates/post/src/post/mod.rs", "post");
    for must in ["fanuc_post", "heidenhain_cycles", "heidenhain_block_form", "doosan_dnm_post"] {
        assert!(own.iter().any(|b| b == must), "{must} missing from {own:?}");
    }
    assert!(count("crates/post/src/post/mod.rs", "app") >= 20, "post change selects too few app suites");
}

/// A file in a submodule narrows to the suites naming that submodule, and still picks some.
#[test]
fn post_submodule_change_narrows_but_still_selects() {
    let got = selected("crates/post/src/post/engine.rs", "app");
    assert!(!got.is_empty());
    let own = selected("crates/post/src/post/engine.rs", "post");
    assert!(own.iter().any(|b| b == "heidenhain_cycles"), "{own:?}");
}

#[test]
fn sim_verify_change_selects_the_verify_suites() {
    let got = selected("crates/sim/src/verify/mod.rs", "app");
    for must in ["verify", "warned_moves"] {
        assert!(got.iter().any(|b| b == must), "{must} missing from {got:?}");
    }
    assert!(count("crates/sim/src/verify/mod.rs", "app") >= 20, "verify change selects too few app suites");
}

/// #2307: verification changes must select the fig8 regression suite.
#[test]
fn sim_verify_change_selects_the_fig8_regression_suite() {
    let got = selected("crates/sim/src/verify/mod.rs", "app");
    assert!(got.iter().any(|b| b == "adaptive_fig8_crossing"), "adaptive_fig8_crossing missing from {got:?}");
}

/// #2307: both posting modules must select their shipped-file regression suite.
#[test]
fn posting_changes_select_the_shipped_from_disk_suite() {
    for file in ["crates/app/src/posting.rs", "crates/app/src/posting_files.rs"] {
        let got = selected(file, "app");
        assert!(got.iter().any(|b| b == "shipped_from_disk"), "{file}: shipped_from_disk missing from {got:?}");
    }
}

#[test]
fn motion_machining2d_change_selects_the_machining2d_suites() {
    let got = selected("crates/motion/src/machining2d/mod.rs", "app");
    assert!(got.iter().any(|b| b == "golden_cl"), "golden_cl missing from {got:?}");
    assert!(count("crates/motion/src/machining2d/mod.rs", "app") >= 20, "machining2d selects too few");
}

/// #1068: the operations suite generates every operation through the app layer, so it pins the
/// motion generators end to end - a change to one of them must select it.
#[test]
fn motion_generator_change_selects_the_operations_suite() {
    let map = scan::gate_map(&scan::read(&root().join("crates/app/tests/operations.rs")))
        .expect("operations.rs gate map");
    for m in ["machining2d", "machining3d", "drilling", "finishing3d", "multiaxis", "plunge_law"] {
        assert!(map.contains(&format!("motion::{m}")), "operations.rs must name motion::{m}: {map}");
    }
    let got = selected("crates/motion/src/multiaxis/mod.rs", "app");
    assert!(got.iter().any(|b| b == "operations"), "operations missing from {got:?}");
}

/// No app suite may name a module that lives in another crate as `app::<module>`.
#[test]
fn no_gate_map_names_a_moved_module_as_app() {
    let moved = [
        "post", "verify", "sim", "solid_rest", "cl", "geo2d", "stock", "tools", "orientation", "spun",
        "cutter_profile", "holder_library", "machining2d", "machining3d", "drilling", "finishing3d",
        "multiaxis", "turning", "links", "load", "plasma", "probing",
    ];
    let mut stale = Vec::new();
    let mut unmapped = Vec::new();
    for b in scan::binaries(&root(), "app") {
        let file = root().join("crates/app/tests").join(format!("{b}.rs"));
        let gm = scan::gate_map(&scan::read(&file)).unwrap_or_default();
        if gm.is_empty() {
            unmapped.push(b.clone());
        }
        for m in moved {
            let hit = gm.split(|c: char| !(c.is_alphanumeric() || c == '_' || c == ':')).any(|w| {
                w == format!("app::{m}") || w == format!("camcraft_app::{m}") || w.starts_with(&format!("app::{m}::"))
            });
            if hit {
                stale.push(format!("{b}: app::{m}"));
            }
        }
    }
    assert!(stale.is_empty(), "Gate maps naming moved modules as app::<module>: {stale:?}");
    assert!(unmapped.is_empty(), "suites without a Gate map: {unmapped:?}");
}

/// #921: the tests that need nothing from `app` live in the crate that owns the behaviour, so batch
/// runs them there and quick selects them for a change in that crate (own-crate Gate maps).
#[test]
fn moved_suites_live_in_their_owner_crate_and_not_in_app() {
    let owners: [(&str, &[&str]); 5] = [
        ("post", &["heidenhain_arc_centre", "heidenhain_block_form", "heidenhain_cycles", "probe_spindle",
            "probe_spindle_stop", "comment_escape", "lathe_post_refusals", "doosan_dnm_post"]),
        ("sim", &["boundary_walls", "real_tip"]),
        ("motion", &["arcs"]),
        ("cl", &["origin_approach"]),
        ("app", &[]),
    ];
    let app = scan::binaries(&root(), "app");
    let motion = scan::binaries(&root(), "motion");
    for (krate, names) in owners {
        let have = scan::binaries(&root(), krate);
        for n in names {
            assert!(have.iter().any(|b| b == n), "{n} missing from crates/{krate}/tests");
            assert!(!app.iter().any(|b| b == n), "{n} still in crates/app/tests");
            if krate != "motion" {
                assert!(!motion.iter().any(|b| b == n), "{n} still in crates/motion/tests");
            }
            let src = scan::read(&root().join(format!("crates/{krate}/tests/{n}.rs")));
            assert!(!src.contains("camcraft_app"), "{n} imports camcraft_app");
        }
    }
}

/// #1021: a file a parent module pulls in with `#[path = "posting_index_swing.rs"] pub mod
/// index_swing;` is `app::posting::index_swing`, so the suites whose map names
/// `app::posting (index_swing)` run for it - not none, as when the file's own name was taken for
/// a top-level module `app::posting_index_swing` that no map names.
#[test]
fn a_path_included_file_selects_its_parents_submodule_suites() {
    let got = selected("crates/app/src/posting_index_swing.rs", "app");
    assert!(got.iter().any(|b| b == "index_swing"), "index_swing missing from {got:?}");
    // A union, never fewer (#1033 review): suites that name the file's own stem still run.
    for (f, must) in [
        ("posting_sim.rs", "wall_column_gouge"),
        ("posting_sim.rs", "contour_arc_sim"),
        ("posting_suppressed.rs", "rest_across_setups"),
        ("posting_plasma.rs", "plasma_gate"),
    ] {
        let got = selected(&format!("crates/app/src/{f}"), "app");
        assert!(got.iter().any(|b| b == must), "{f}: {must} missing from {got:?}");
    }
    // The other #[path] siblings of posting.rs resolve the same way and still select suites.
    for f in ["posting_mounts.rs", "posting_per_op.rs", "posting_nose.rs", "posting_sim.rs"] {
        assert!(count(&format!("crates/app/src/{f}"), "app") > 0, "{f} selects nothing");
    }
}
