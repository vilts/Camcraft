//! Example catalog contract (#2392, #2393): every case under
//! `examples/catalog/<family>/<case>/` carries its model generator/mesh,
//! `build.camscript`, `<case>.camproj`, `<case>.nc`, and `README.md`.
//! Each case opens, regenerates every operation, verifies clean, and posts
//! the committed `.nc` byte for byte; the seed case rebuilds end to end
//! from `build.camscript`. The turning case (#2394) also pins what its
//! program shows: the taper and the thread passes.
//!
//! Gate map (#2393): covers app::commands, app::model, app::operation,
//! app::posting, app::project, app::script, app::mcp, app::cli,
//! app::setup, app::world, post::post, sim::verify.
//! Provenance: clean-room test suite over CamCraft's own commands, script
//! host, and example catalog cases.

use std::fs;

#[path = "example_catalog/support.rs"]
mod support;
use support::*;

include!("example_catalog/case_probe_face.rs");

#[path = "example_catalog/contract.rs"]
mod contract;

#[path = "example_catalog/refusal.rs"]
mod refusal;

/// #2865: three original parts demonstrate the five additional turning kinds.
#[test]
fn turning_cases_cover_five_additional_kinds() {
    for (case, kinds) in [
        ("bushing", &["BORING", "FORM TURNING"][..]),
        ("contour-finishes", &["CHAMFERING", "MULTI-PASS TURNING"][..]),
        ("taper-finish", &["TAPER TURNING"][..]),
    ] {
        let dir = repo(&format!("examples/catalog/turning/{case}"));
        assert!(dir.is_dir(), "catalog case turning/{case} is missing");
        let program = fs::read_to_string(dir.join(format!("{case}.nc"))).expect("read case NC");
        let actual: Vec<&str> = program
            .lines()
            .filter(|line| line.starts_with("(OPERATION: "))
            .filter_map(|line| line.split(" - ").nth(1))
            .collect();
        assert_eq!(actual, kinds, "turning/{case} operation kinds");
        assert!(program.contains("G18"), "turning/{case} states the ZX plane");
        assert!(!program.lines().any(|line| word(line, 'Y').is_some()));
    }
}

/// #2971: keep both named results for the groove and part-off case.
#[test]
fn groove_part_off_case_has_two_named_test_results() {
    assert!(repo("examples/catalog/turning/groove-part-off").is_dir());
    let output = std::process::Command::new(std::env::current_exe().expect("test executable"))
        .args(["--list", "--format", "terse"])
        .output()
        .expect("list catalog tests");
    assert!(output.status.success());
    let listing = String::from_utf8_lossy(&output.stdout);
    for action in ["regenerates", "rebuilds"] {
        let name = format!("catalog_turning_groove_part_off::{action}: test");
        assert!(listing.lines().any(|line| line == name), "missing case test {name}");
    }
}

mod catalog_multiaxis_indexed {
    use super::*;
    #[test]
    fn posts_two_single_axis_indexes_and_one_tilted_pocket() {
        use camcraft_app::operation::{self, OpParams};
        use camcraft_app::{model, project, setup, world};
        let dir = repo("examples/catalog/multiaxis/indexed");
        let program = fs::read_to_string(dir.join("indexed.nc")).expect("indexed NC case");
        let w = world::new_world();
        let notes = project::load_noted(&w, &dir.join("indexed.camproj")).expect("indexed project");
        assert!(notes.is_empty(), "load notes: {notes:?}");
        let ops = setup::operation_order(&w);
        assert_eq!(ops.len(), 3, "two indexed faces and one tilted face");
        for ((name, entity), (a, b)) in ops.iter().zip([(90.0, 0.0), (-90.0, 0.0), (30.0, 30.0)]) {
            assert_eq!(operation::status_of(&w, entity), operation::OpStatus::Ok, "{name} saved cache");
            let op = model::operation_of(&w, entity).expect("pocket operation");
            let OpParams::Pocket { tilt: Some(tilt), .. } = op.params else {
                panic!("{name} must be an indexed pocket");
            };
            assert_eq!((tilt.a_deg, tilt.b_deg), (a, b), "{name} orientation");
            let blocks = section(&program, name);
            assert!(blocks.iter().any(|l| word(l, 'A') == Some(a)), "{name} posts A{a}");
            assert!(blocks.iter().any(|l| word(l, 'B') == Some(b)), "{name} posts B{b}");
            assert!(blocks.iter().any(|l| l.starts_with("G01 ")), "{name} cuts");
        }
        let part = model::find_by_name(&w, "indexed").expect("embedded model");
        assert!(camcraft_app::model_source::of(&w, &part).is_none(), "portable model");
    }
    #[test]
    fn regenerates() {
        run_case("multiaxis", "indexed", "regenerates", regenerate);
    }
    #[test]
    fn rebuilds() {
        run_case("multiaxis", "indexed", "rebuilds", rebuild);
    }
}

// These full-case checks exceed the 60 s debug budget. The heavy lane keeps
// both checks; example_catalog_adaptive pins the committed program in quick.
mod catalog_2d_adaptive_pocket {
    use super::*;
    #[test]
    #[ignore = "heavy lane (#3163 2d/adaptive-pocket): exceeds its 60 s debug budget - run with --release -- --ignored"]
    fn regenerates() {
        run_case("2d", "adaptive-pocket", "regenerates", regenerate);
    }
    #[test]
    #[ignore = "heavy lane (#3163 2d/adaptive-pocket): exceeds its 60 s debug budget - run with --release -- --ignored"]
    fn rebuilds() {
        run_case("2d", "adaptive-pocket", "rebuilds", rebuild);
    }
}

mod catalog_2d_contour_tabs {
    use super::*;
    #[test]
    fn regenerates() {
        run_case("2d", "contour-tabs", "regenerates", regenerate);
    }
    #[test]
    fn rebuilds() {
        run_case("2d", "contour-tabs", "rebuilds", rebuild);
    }
}

mod catalog_2d_edge_work {
    use super::*;
    #[test]
    fn regenerates() {
        run_case("2d", "edge-work", "regenerates", regenerate);
    }
    #[test]
    fn rebuilds() {
        run_case("2d", "edge-work", "rebuilds", rebuild);
    }
}

mod catalog_2d_first_part {
    use super::*;
    #[test]
    fn regenerates() {
        run_case("2d", "first-part", "regenerates", regenerate);
    }
    #[test]
    fn rebuilds() {
        run_case("2d", "first-part", "rebuilds", rebuild);
    }
}

mod catalog_2d_hole_work {
    use super::*;
    #[test]
    fn regenerates() {
        run_case("2d", "hole-work", "regenerates", regenerate);
    }
    #[test]
    fn rebuilds() {
        run_case("2d", "hole-work", "rebuilds", rebuild);
    }
}

// #2861: the case 2d/slots, its two case tests and its program pin.
include!("example_catalog/case_2d_slots.rs");

// #2856: the refusal case 2d/drill-overfeed and its two case tests.
include!("example_catalog/case_2d_drill_overfeed.rs");
include!("example_catalog/case_3d_rough_finish.rs"); // #2866: 3d/rough-finish

mod catalog_turning_bushing {
    use super::*;
    #[test]
    fn regenerates() {
        run_case("turning", "bushing", "regenerates", regenerate);
    }
    #[test]
    fn rebuilds() {
        run_case("turning", "bushing", "rebuilds", rebuild);
    }
}

mod catalog_turning_contour_finishes {
    use super::*;
    #[test]
    fn regenerates() {
        run_case("turning", "contour-finishes", "regenerates", regenerate);
    }
    #[test]
    fn rebuilds() {
        run_case("turning", "contour-finishes", "rebuilds", rebuild);
    }
}

mod catalog_turning_taper_finish {
    use super::*;
    #[test]
    fn regenerates() {
        run_case("turning", "taper-finish", "regenerates", regenerate);
    }
    #[test]
    fn rebuilds() {
        run_case("turning", "taper-finish", "rebuilds", rebuild);
    }
}

mod catalog_turning_cone_thread {
    use super::*;
    #[test]
    fn regenerates() {
        run_case("turning", "cone-thread", "regenerates", regenerate);
    }
    #[test]
    fn rebuilds() {
        run_case("turning", "cone-thread", "rebuilds", rebuild);
    }
}

mod catalog_turning_groove_part_off {
    use super::*;
    #[test]
    fn regenerates() {
        run_case("turning", "groove-part-off", "regenerates", regenerate);
    }
    #[test]
    fn rebuilds() {
        run_case("turning", "groove-part-off", "rebuilds", rebuild);
    }
}

/// The value of an address word (`X`, `Z`) in one NC block.
fn word(block: &str, address: char) -> Option<f64> {
    block
        .split_whitespace()
        .find_map(|w| w.strip_prefix(address))
        .and_then(|v| v.parse().ok())
}

/// The blocks of one operation: from its `(OPERATION: <name> ` comment
/// to the next operation comment.
fn section<'a>(program: &'a str, op: &str) -> Vec<&'a str> {
    let head = format!("(OPERATION: {op} ");
    program
        .lines()
        .skip_while(|l| !l.starts_with(&head))
        .skip(1)
        .take_while(|l| !l.starts_with("(OPERATION: "))
        .collect()
}

/// #2394: the turning case shows a spun profile - a cone with a threaded
/// end. The committed program holds the six operations in order, the
/// 20 mm to 10 mm taper over 40 mm in the finish ride, and the M10x1.5
/// thread passes to the 8.16 mm root on the Fanuc-style lathe post.
#[test]
fn turning_cone_thread_case_posts_the_taper_and_the_thread() {
    let dir = repo("examples/catalog/turning/cone-thread");
    let case = CatalogCase { family: "turning".into(), name: "cone-thread".into(), dir: dir.clone() };
    assert!(dir.is_dir(), "catalog case turning/cone-thread is missing: {}", dir.display());
    assert_case_layout(&case);
    let program = fs::read_to_string(dir.join("cone-thread.nc")).expect("read cone-thread.nc");

    assert!(program.contains("G18"), "a lathe program states the ZX plane");
    assert!(!program.lines().any(|l| word(l, 'Y').is_some()), "a 2-axis lathe program has no Y word");
    let kinds: Vec<&str> = program
        .lines()
        .filter(|l| l.starts_with("(OPERATION: "))
        .filter_map(|l| l.split(" - ").nth(1))
        .collect();
    assert_eq!(
        kinds,
        ["FACING", "PROFILE ROUGH", "PROFILE FINISH", "GROOVE FINISH", "THREADING", "PARTING"],
        "face, rough profile, finish profile, the neck, the thread, part-off - in this order"
    );

    // The taper: every finish block on the cone cuts X and Z together at
    // 10 mm of diameter over 40 mm of length (-0.25), and the blocks span
    // the cone between the shoulder (Z20) and the thread diameter (Z60).
    let finish = section(&program, "Finish profile");
    let cone: Vec<(f64, f64)> = finish
        .iter()
        .filter(|l| l.starts_with("G01"))
        .filter_map(|l| Some((word(l, 'X')?, word(l, 'Z')?)))
        .filter(|&(x, z)| x < 20.0 && (20.0..=58.5).contains(&z))
        .collect();
    assert!(cone.len() >= 2, "the finish ride has no blocks on the cone: {finish:?}");
    let span = cone[cone.len() - 1].1 - cone[0].1;
    assert!(span > 30.0, "the cone blocks span {span} mm of the 40 mm taper: {cone:?}");
    for w in cone.windows(2) {
        let ((x0, z0), (x1, z1)) = (w[0], w[1]);
        let slope = (x1 - x0) / (z1 - z0);
        assert!((slope + 0.25).abs() < 0.005, "X{x0} Z{z0} to X{x1} Z{z1} is off the taper: {slope}");
        assert!(x0 < 20.0 && x1 > 10.0, "X{x0} to X{x1} is outside the 20 to 10 mm cone");
    }

    // The thread: G32 passes at the 1.5 mm lead, the last one at the
    // M10x1.5 root diameter, from the end face side down to the neck.
    let thread = section(&program, "Thread");
    assert!(thread.iter().any(|l| l.starts_with("(T0303 ") && l.contains("THREADING INSERT")), "{thread:?}");
    assert!(thread.iter().any(|l| l.starts_with("G32 ") && l.ends_with("F1.5")), "no G32 pass at lead 1.5: {thread:?}");
    assert!(thread.contains(&"G01 X8.16 Z77.4 F0.05"), "the last pass enters at the 8.16 mm root: {thread:?}");
    assert!(thread.contains(&"G32 Z63.9 F1.5"), "each pass ends at the neck, Z63.9: {thread:?}");
    assert!(!program.contains("G76"), "this case posts G32 passes, not the G76 cycle");

    // The neck and the part-off: the 3 mm blade, to X8 and to the axis.
    // The part-off plane is Z15, the end face of the part. The whole 3 mm
    // kerf is on the bar side (Z12 to Z15), so the orientation 2 blade
    // posts its low edge at Z12 (#3452).
    assert!(section(&program, "Neck").contains(&"G01 X8. Z60. F0.02"), "the neck is 8 mm at Z60");
    assert!(section(&program, "Part off").contains(&"G01 X0. Z12. F0.02"),
        "the part-off reaches the axis at the low edge Z12, its kerf Z12..15 below the part (#3452)");
}

/// The G word and the Z word of each motion block of a posted program.
fn motion_blocks(nc: &str) -> Vec<(&str, f64)> {
    nc.lines()
        .filter_map(|line| {
            let mut words = line.split_whitespace();
            let g = words.next().filter(|w| matches!(*w, "G00" | "G01" | "G02" | "G03"))?;
            let z = words.find_map(|w| w.strip_prefix('Z'))?.parse::<f64>().ok()?;
            Some((g, z))
        })
        .collect()
}

/// #2859: the case 2d/contour-tabs posts what its README says. Each cut
/// run has a lead-in arc and a linear lead-out. The levels are Z4, Z2 (the
/// tab top) and Z0. The Z0 level lifts over each of the four tabs.
#[test]
fn contour_tabs_case_posts_the_lead_moves_and_the_tab_lifts() {
    let nc_path = repo("examples/catalog/2d/contour-tabs/contour-tabs.nc");
    let nc = fs::read_to_string(&nc_path)
        .unwrap_or_else(|e| panic!("case 2d/contour-tabs must hold {}: {e}", nc_path.display()));
    let blocks = motion_blocks(&nc);

    // The outline is cut clockwise (G02 corners). Only a lead-in arc turns
    // the other way (G03).
    let lead_ins = |z: f64| blocks.iter().filter(|b| b.0 == "G03" && b.1 == z).count();
    assert_eq!(lead_ins(4.0), 1, "one lead-in arc on the whole ring at Z4");
    assert_eq!(lead_ins(2.0), 1, "one lead-in arc on the whole ring at Z2, the tab top");
    assert_eq!(lead_ins(0.0), 4, "one lead-in arc for each run between two tabs at Z0");

    // A lift goes from a cut level to a height above the stock top (Z6).
    // The block before it is the end of the linear lead-out. #3678: a rise
    // after a G01 that went down is a peck retract, not a lift. Tab windows
    // 1 to 3 start where the levels above left stock under the disc, so
    // each dive pecks Z4, Z2, Z0: two retracts each.
    let rises: Vec<usize> =
        (2..blocks.len()).filter(|&i| blocks[i - 1].1 <= 4.0 && blocks[i].1 > 6.0).collect();
    let peck = |i: usize| blocks[i - 1].0 == "G01" && blocks[i - 2].1 > blocks[i - 1].1;
    let (pecks, lifts): (Vec<usize>, Vec<usize>) = rises.into_iter().partition(|&i| peck(i));
    assert_eq!(pecks.len(), 6, "tab windows 1 to 3 each peck to Z0 with two retracts");
    assert_eq!(lifts.len(), 6, "two whole rings and four tab runs each end with a lift");
    for &i in &lifts {
        assert_eq!(blocks[i - 1].0, "G01", "the block before lift {i} is the linear lead-out");
    }
    let tab_lifts = lifts.iter().filter(|&&i| blocks[i - 1].1 == 0.0).count();
    assert_eq!(tab_lifts, 4, "the Z0 level lifts four times: one lift for each tab");
    assert!(!blocks.iter().any(|b| b.1 < 0.0), "no block goes below the stock bottom (Z0)");
}

/// #2862: the hole-work example keeps the four hole operations and posts
/// drilling cycles plus a thread helix with simultaneous XY and Z motion.
#[test]
fn hole_work_case_posts_drilling_cycles_and_thread_helix() {
    let dir = repo("examples/catalog/2d/hole-work");
    let case = CatalogCase { family: "2d".into(), name: "hole-work".into(), dir };
    assert_case_layout(&case);
    let nc = fs::read_to_string(case.dir.join("hole-work.nc")).expect("read hole-work.nc");
    let kinds: Vec<_> = nc.lines().filter(|line| line.starts_with("(OPERATION: "))
        .filter_map(|line| line.split(" - ").nth(1)).collect();
    assert_eq!(kinds, ["DRILL", "BORE", "CIRCULAR", "DRILL", "THREAD"]);
    let section = |name: &str| -> Vec<&str> {
        let header = format!("(OPERATION: {name} ");
        nc.lines().skip_while(|line| !line.starts_with(&header)).skip(1)
            .take_while(|line| !line.starts_with("(OPERATION: ")).collect()
    };
    let bolt = section("Bolt circle");
    for xy in ["X0. Y30.", "X30. Y0.", "X0. Y-30.", "X-30. Y0."] {
        assert!(bolt.iter().any(|line| line.starts_with(&format!("G81 {xy} Z-2.803 "))),
            "missing bolt-circle drilling cycle at {xy}");
    }
    for name in ["Bore", "Circular"] {
        assert!(section(name).iter().any(|line| line.starts_with("G03 ")),
            "{name} must retain circular interpolation");
    }
    assert!(section("Tap drill").contains(&"G81 X0. Y0. Z4. R20. F80."));
    let thread = section("Thread");
    let helix: Vec<_> = thread.iter().filter(|line| {
        let words: Vec<_> = line.split_whitespace().collect();
        matches!(words.first(), Some(&"G02" | &"G03"))
            && ['X', 'Y', 'Z', 'I', 'J'].iter().all(|address|
                words.iter().any(|word| word.starts_with(*address)))
    }).collect();
    assert_eq!(helix.len(), 12, "three thread passes each have four helical blocks");
    let z = |line: &str| -> f64 {
        line.split_whitespace().find_map(|word| word.strip_prefix('Z'))
            .expect("helix Z").parse().expect("numeric helix Z")
    };
    for pass in helix.as_chunks::<4>().0 {
        assert_eq!(z(pass[0]), 8.313, "thread entry rises a quarter pitch");
        assert_eq!(z(pass[3]), 9.875, "thread exit rises another quarter pitch");
        assert!((z(pass[2]) - z(pass[1]) - 0.625).abs() < 0.001,
            "half a turn must rise half the 1.25 mm pitch");
    }
}
