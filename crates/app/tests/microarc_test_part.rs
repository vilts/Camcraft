//! #785: the microARC 4 hardware test part, built through the app's own
//! commands - the job the tester kit `docs/tester-kits/tormach-microarc-4th/`
//! ships. A 3/4" round bar in the 4th-axis chuck, axis along world X, the
//! free end at X0 (G54 X0 at the stock face, Y0 Z0 on the A centerline):
//! two opposite shallow flats at A-90 and A90 and a shallow cross slot back
//! at A0, each an indexed A then an ordinary 3-axis cut. (The face
//! underneath, the pose law's A0 B180, is A180 on this table since #797 -
//! `fourth_axis_pose` pins it; the kit keeps its +-90 pair.) The posted program is the
//! kit's golden; a continuous wrap op on the same bar posts as wrapped A
//! (#796: A cumulative, its turning feeds in G93), pinned by its golden.
//! Provenance: our own test over our own commands, machine file and post.
//!
//! Regenerate the program and the project after an intentional change:
//! `WRITE_GOLDEN=1 cargo test -p camcraft-app --test microarc_test_part`
//!
//! Gate map (M4.4): covers app::posting (post_setup), post::post
//! (tormach.cps), app::operation (tilted Slot, Rotary Parallel), app::project.

use camcraft_app::commands::{
    AddTool, CreateOperation, GenerateOperation, History, ImportModel, LoadMachine, SelectPost,
    SetStock,
};
use camcraft_app::geometry::{Mesh, Point3, Triangle};
use camcraft_app::geo2d::Point2;
use camcraft_app::machining3d::Entry3D;
use camcraft_app::operation::{Height, OpParams, SrcRef, Tilt};
use camcraft_app::schema::{Location, PostRef};
use camcraft_app::stock::StockShape;
use camcraft_app::tools::{Tool, catalog};
use flecs_ecs::prelude::World;
use std::path::{Path, PathBuf};

const IN: f64 = 25.4;
/// 3/4" bar, 1.5" sticking out of the chuck.
const R: f64 = 0.375 * IN;
const STICK: f64 = 1.5 * IN;
const KIT: &str = "docs/tester-kits/tormach-microarc-4th";

fn repo(rel: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../").join(rel)
}

/// The bar's outer skin as a closed 48-gon prism along X, -STICK..0.
/// The MODEL is the core the cuts never reach (radius 0.340", below the
/// 0.020" cross-slot floor): the verification gate judges gouge against the
/// model, and the real finished shape is the stock minus the cuts.
fn prism(r: f64) -> Mesh {
    let n = 48;
    let pt = |x: f64, j: usize| {
        let t = std::f64::consts::TAU * (j % n) as f64 / n as f64;
        Point3::new(x, r * t.cos(), r * t.sin())
    };
    let mut f = Vec::new();
    let (x0, x1) = (-STICK, 0.0);
    for j in 0..n {
        let (a0, a1, b0, b1) = (pt(x0, j), pt(x0, j + 1), pt(x1, j), pt(x1, j + 1));
        f.push(Triangle::new(a0, a1, b1));
        f.push(Triangle::new(a0, b1, b0));
        f.push(Triangle::new(Point3::new(x0, 0.0, 0.0), a1, a0));
        f.push(Triangle::new(Point3::new(x1, 0.0, 0.0), b0, b1));
    }
    Mesh::from_facets(f)
}

fn tool(base: &str, dia_in: f64, rpm: f64, feed: f64) -> Tool {
    let mut t = catalog()
        .into_iter()
        .find(|(n, _)| *n == base)
        .map(|(_, t)| t)
        .unwrap_or_else(|| panic!("catalog {base}"));
    t.diameter = dia_in * IN;
    t.shank_d = t.diameter;
    if base.starts_with("BN") {
        t.corner_r = t.diameter / 2.0;
    }
    t.rpm = rpm;
    t.feed = feed;
    t
}

/// A flat 0.015" deep, 1/2" long, one 1/4" pass wide, on the face the
/// tilt stands the tool over. The slot runs along the bar: an A tilt
/// keeps the frame's X on world X (`cl::frame`'s basis rule).
fn flat(tilt: Option<Tilt>) -> OpParams {
    let (a, b) = (Point2::new(-0.75 * IN, 0.0), Point2::new(-0.25 * IN, 0.0));
    OpParams::Slot {
        a,
        b,
        width: 0.25 * IN,
        stock_to_leave: 0.0,
        clearance_z: Height::fixed(R + 0.5 * IN),
        retract_z: Some(Height::fixed(R + 0.1 * IN)),
        top_z: Height::fixed(R),
        bottom_z: Height::fixed(R - 0.015 * IN),
        stepdown: 0.015 * IN,
        stepover: 0.1 * IN,
        feed: 15.0 * IN,
        plunge_feed: 4.0 * IN,
        climb: true,
        entry: Entry3D::Plunge,
        helix_radius: 1.0, blend_turns: false, blend_radius: None,
        links: camcraft_app::links::Links::PLANAR,
        tilt,
        extend_start: 0.0,
        extend_end: 0.0,
        picks: Default::default(),
    }
}

/// The cross slot back at A0: 0.020" deep, one 1/8" pass across the top
/// of the bar 1/2" from the free end (0.160" long, centre to centre) - the
/// third index turns the table back through 90 degrees.
fn cross() -> OpParams {
    OpParams::Slot {
        a: Point2::new(-0.5 * IN, -0.08 * IN),
        b: Point2::new(-0.5 * IN, 0.08 * IN),
        width: 0.125 * IN,
        stock_to_leave: 0.0,
        clearance_z: Height::fixed(R + 0.5 * IN),
        retract_z: Some(Height::fixed(R + 0.1 * IN)),
        top_z: Height::fixed(R),
        bottom_z: Height::fixed(R - 0.020 * IN),
        stepdown: 0.010 * IN,
        stepover: 0.05 * IN,
        feed: 8.0 * IN,
        plunge_feed: 2.0 * IN,
        climb: true,
        entry: Entry3D::Plunge,
        helix_radius: 0.5, blend_turns: false, blend_radius: None,
        links: camcraft_app::links::Links::PLANAR,
        tilt: None,
        extend_start: 0.0,
        extend_end: 0.0,
        picks: Default::default(),
    }
}

fn ok<T, E: std::fmt::Debug>(r: Result<T, E>, what: &str) -> T {
    r.unwrap_or_else(|e| panic!("{what}: {e:?}"))
}

/// The document: machine, post, stock, model, tools, the three indexed ops.
fn build() -> World {
    let world = camcraft_app::world::new_world();
    let mut h = History::default();
    h.execute(&world, ok(LoadMachine::builtin(&world, "Tormach 770MX + microARC 4"), "machine"));
    // #902: the inch program these numbers read is an inch document's.
    h.execute(&world, ok(camcraft_app::commands::SetUnits::new(&world, camcraft_app::units::Unit::Inches), "inch document"));
    h.execute(&world, ok(SelectPost::new(&world, PostRef::Named("Tormach PathPilot".into())), "post"));
    // The bar's bounding box as stock: tangent to the round bar on the
    // four faces the job cuts, so every cut starts at the real surface.
    let stock = SetStock::new(
        &world,
        StockShape::Box { dx: STICK, dy: 2.0 * R, dz: 2.0 * R },
        Location { pos: Point3::new(-STICK / 2.0, 0.0, -R), ..Location::default() },
    );
    h.execute(&world, ok(stock, "stock"));
    h.execute(&world, ImportModel::from_mesh("bar core", prism(0.340 * IN)));
    h.execute(&world, ok(AddTool::new(&world, "EM 1/4 3FL", tool("EM-0600-4FL", 0.25, 6000.0, 15.0 * IN)), "T1"));
    h.execute(&world, ok(AddTool::new(&world, "EM 1/8 2FL", tool("EM-0300-2FL", 0.125, 8000.0, 8.0 * IN)), "T2"));
    let ops: [(&str, &str, OpParams); 3] = [
        ("FLAT A-90", "EM 1/4 3FL", flat(Some(Tilt { basis_version: 1, a_deg: -90.0, b_deg: 0.0, origin: [0.0; 3] }))),
        ("FLAT A90", "EM 1/4 3FL", flat(Some(Tilt { basis_version: 1, a_deg: 90.0, b_deg: 0.0, origin: [0.0; 3] }))),
        ("CROSS SLOT A0", "EM 1/8 2FL", cross()),
    ];
    for (name, t, p) in ops {
        h.execute(&world, ok(CreateOperation::new(&world, name, t, SrcRef::Stock, p), name));
        h.execute(&world, ok(GenerateOperation::new(&world, name), name));
    }
    world
}

#[test]
fn the_microarc_test_part_posts_its_indexed_program() {
    let world = build();
    let out = std::env::temp_dir().join(format!("camcraft_microarc_{}.nc", std::process::id()));
    let posted = camcraft_app::posting::post_setup(&world, &out);
    let _ = std::fs::remove_file(&out);
    let posted = posted.unwrap_or_else(|e| panic!("the indexed test part must post: {e}"));
    let nc = posted.program;
    let golden = repo(&format!("{KIT}/microarc_test_part.nc"));
    if std::env::var_os("WRITE_GOLDEN").is_some() {
        std::fs::write(&golden, &nc).expect("write the kit program");
        camcraft_app::project::save(&world, &repo(&format!("{KIT}/microarc_test_part.camproj")))
            .expect("save the kit project");
    } else {
        let want = std::fs::read_to_string(&golden).expect("the kit program");
        assert_eq!(nc, want, "{} is stale; regenerate with WRITE_GOLDEN=1", golden.display());
        // The kit's project is this same document, saved by the app.
        let proj = std::fs::read_to_string(repo(&format!("{KIT}/microarc_test_part.camproj")))
            .expect("the kit project");
        for name in ["Tormach 770MX + microARC 4", "FLAT A-90", "FLAT A90", "CROSS SLOT A0"] {
            assert!(proj.contains(name), "the kit project names {name}");
        }
    }
    // Three indexes, in order, each alone after Z goes up.
    let lines: Vec<&str> = nc.lines().collect();
    let idx: Vec<&str> = lines.iter().copied().filter(|l| l.starts_with("G0 A")).collect();
    assert_eq!(idx, ["G0 A-90", "G0 A90", "G0 A0"], "{nc}");
    for (i, l) in lines.iter().enumerate() {
        if l.starts_with("G0 A") {
            assert_eq!(lines[i - 1], "G53 G0 Z0", "{nc}");
        }
        assert!(!(l.starts_with("G1") && l.contains(" A")), "no A on a cut: {l}");
    }
    // Every cut stays within 0.021" of the bar's skin (Z is the radius
    // after each index) and inside the 1.5" stick-out.
    for l in lines.iter().filter(|l| l.starts_with("G1 ")) {
        for t in l.split(' ') {
            let v: Option<f64> = t.get(1..).and_then(|s| s.parse().ok());
            match (t.chars().next(), v) {
                (Some('Z'), Some(z)) => assert!((0.375 - 0.0201..=0.375 + 0.2).contains(&z), "{l}"),
                (Some('X'), Some(x)) => assert!((-1.5..=0.0).contains(&x), "{l}"),
                _ => {}
            }
        }
    }
}

/// The continuous op (#796): a Rotary Parallel wrapped round the bar
/// turns A inside the operation. The post writes it as wrapped A - A
/// counting on (never back through 0), the turning feeds in G93 - and the
/// program is pinned by its golden.
const WRAP_GOLDEN: &str = "crates/app/tests/golden/nc/tormach/wrap_rotary_parallel_microarc4.nc";

fn wrap_world() -> World {
    let world = build();
    let mut h = History::default();
    let wrap = OpParams::RotaryParallel {
        tolerance: 0.5,
        stepover: Some(3.0),
        clearance_z: Height::fixed(R + 0.5 * IN),
        retract_z: None,
        top_z: Height::fixed(R),
        bottom_z: Height::fixed(-R),
        feed: 8.0 * IN,
        plunge_feed: 2.0 * IN,
        climb: true,
        lead_deg: 0.0,
        lag_deg: 0.0,
        max_tilt_deg: 60.0,
        stated_axis: None,
        picks: Default::default(),
    };
    let ball = tool("BN-0600", 0.125, 8000.0, 8.0 * IN);
    h.execute(&world, ok(AddTool::new(&world, "BN 1/8", ball), "ball"));
    // This fixture's controller table is fixed by the committed wrap golden.
    // The indexed-only fixture keeps its own T1/T2 table.
    for (name, number) in [("BN 1/8", 1), ("EM 1/4 3FL", 2), ("EM 1/8 2FL", 3)] {
        let entity = camcraft_app::model::find_by_name(&world, name).expect("fixture tool");
        let tool = camcraft_app::model::tool_of(&world, &entity).expect("tool definition");
        entity.entity_view(&world).set(camcraft_app::schema::ToolDef { tool, number: Some(number) });
    }
    let created = CreateOperation::new(&world, "WRAP", "BN 1/8", SrcRef::Entity("bar core".into()), wrap);
    h.execute(&world, created.expect("create the wrap op"));
    h.execute(&world, GenerateOperation::new(&world, "WRAP").expect("generate"));
    world
}

fn num(t: &str) -> Option<f64> {
    t.get(1..).and_then(|s| s.parse().ok())
}

/// The wrapped job's trig differs at last-place rounding boundaries across
/// platforms. Keep topology, mode words, XYZ and ordinary feeds byte exact;
/// only motion A (one 0.001-degree step) and G93 F have numeric allowances.
/// G93 F is inverse time: a 0.001-degree endpoint rounding change has a
/// larger absolute effect on short, high-F turns. The 0.749 spread
/// reported in #2342 is reproduced by 360/0.693 versus 360/0.694
/// (floored to 0.001), near F519: about 0.15%. Allow 0.2%
/// relative to the smaller F, including headroom for word flooring. Unlike
/// an absolute 0.75 allowance, this permits only 0.01067 at the slowest
/// turning feed F5.335. Independent tip-feed and A-rate checks still apply.
fn compare_wrap_golden(actual: &str, expected: &str) -> Result<(), String> {
    let actual: Vec<_> = actual.lines().collect();
    let expected: Vec<_> = expected.lines().collect();
    if actual.len() != expected.len() {
        return Err(format!("line count {} != {}", actual.len(), expected.len()));
    }
    let mut g93 = false;
    for (line, (got, want)) in actual.iter().zip(&expected).enumerate() {
        let got: Vec<_> = got.split(' ').collect();
        let want: Vec<_> = want.split(' ').collect();
        if got.contains(&"G93") { g93 = true; }
        if got.contains(&"G94") { g93 = false; }
        let turning_motion = (got.contains(&"G0") || got.contains(&"G1")) && got.iter().any(|w| w.starts_with('A'));
        let turning_feed = g93 && got.contains(&"G1") && turning_motion;
        if got.len() != want.len() {
            return Err(format!("line {} word count differs", line + 1));
        }
        for (got, want) in got.iter().zip(&want) {
            if got == want { continue; }
            let tolerance = if turning_motion && got.starts_with('A') && want.starts_with('A') {
                0.001
            } else if turning_feed && got.starts_with('F') && want.starts_with('F') {
                match (num(got), num(want)) {
                    (Some(a), Some(b)) if a > 0.0 && b > 0.0 => 0.002 * a.min(b),
                    _ => return Err(format!("line {}: invalid G93 feed {got} or {want}", line + 1)),
                }
            } else {
                return Err(format!("line {}: {got} != {want}", line + 1));
            };
            if !matches!((num(got), num(want)), (Some(a), Some(b))
                if a.is_finite() && b.is_finite() && (a - b).abs() <= tolerance + 1e-10)
            {
                return Err(format!("line {}: {got} != {want} (allowance {tolerance})", line + 1));
            }
        }
    }
    Ok(())
}

#[test]
fn wrap_golden_allows_only_bounded_platform_rounding() {
    let windows = "G93 G1 X0 Y0 Z0 A-41.921 F116.959\nG1 X0 Y0 Z0 A-45 F116.921";
    let linux = "G93 G1 X0 Y0 Z0 A-41.922 F116.921\nG1 X0 Y0 Z0 A-45 F116.959";
    assert!(compare_wrap_golden(linux, windows).is_ok());
    // The exact Windows pair from the parent review, plus representative
    // rate-capped pairs reproducing its other reported absolute spreads.
    for (actual, expected) in [
        ("209.668", "209.79"),
        ("132.109", "132.158"),
        ("385.852", "386.266"),
        ("518.731", "519.48"),
    ] {
        let actual = format!("G93 G1 X0 A1 F{actual}");
        let expected = format!("G93 G1 X0 A1 F{expected}");
        assert!(compare_wrap_golden(&actual, &expected).is_ok());
        assert!(compare_wrap_golden(&expected, &actual).is_ok());
    }
    // A relative allowance stays tight at low F and rejects larger
    // timing changes at high F, in either comparison direction.
    for (actual, expected) in [("5.35", "5.335"), ("520.52", "519.48")] {
        let actual = format!("G93 G1 X0 A1 F{actual}");
        let expected = format!("G93 G1 X0 A1 F{expected}");
        assert!(compare_wrap_golden(&actual, &expected).is_err());
        assert!(compare_wrap_golden(&expected, &actual).is_err());
    }
    for changed in [
        linux.replace("A-41.922", "A-41.923"),
        linux.replace("F116.921", "F116.65"),
        linux.replace("X0", "X0.0001"),
        linux.replace("G93", "G94"),
        linux.replace("G1", "G0"),
        linux.replace(" A-45", ""),
        format!("{linux}\nM5"),
        linux.replace("F116.921", "FNaN"),
    ] {
        assert!(compare_wrap_golden(&changed, windows).is_err(), "accepted {changed}");
    }
    assert!(compare_wrap_golden("G0 X0 Y0 Z0.8746 A-42.718", "G0 X0 Y0 Z0.8746 A-42.717").is_ok());
    assert!(compare_wrap_golden("G0 X0 Y0 Z0.8746 A-42.719", "G0 X0 Y0 Z0.8746 A-42.717").is_err());
    assert!(compare_wrap_golden("G94 G1 A3 F359.999", "G94 G1 A3 F360").is_err());
    assert!(compare_wrap_golden("G1 X0 F1.001", "G1 X0 F1").is_err());
}

#[test]
fn wrapped_fixture_keeps_golden_tool_numbers() {
    let world = wrap_world();
    let out = std::env::temp_dir().join(format!("camcraft_microarc_numbers_{}.nc", std::process::id()));
    let posted = camcraft_app::posting::post_setup(&world, &out);
    let _ = std::fs::remove_file(&out);
    let nc = posted.expect("the wrapped fixture must post").program;
    let want = std::fs::read_to_string(repo(WRAP_GOLDEN)).expect("the committed wrap golden");
    // Pin comments, tool changes and length offsets independently of the
    // platform-dependent motion-line count tracked by #2483.
    let table = |program: &str| -> Vec<String> {
        program.lines().filter(|line| {
            line.starts_with("(T") && line.as_bytes().get(2).is_some_and(u8::is_ascii_digit)
                || line.starts_with('T')
                || line.split_whitespace().any(|word| word.starts_with('H') && word[1..].parse::<u32>().is_ok())
        }).map(str::to_owned).collect()
    };
    assert_eq!(table(&nc), table(&want), "the wrapped fixture must retain its golden T/H table");
}

#[test]
fn a_wrapped_op_on_the_test_part_posts_wrapped_a() {
    let world = wrap_world();
    let out = std::env::temp_dir().join(format!("camcraft_microarc_wrap_{}.nc", std::process::id()));
    let posted = camcraft_app::posting::post_setup(&world, &out);
    let _ = std::fs::remove_file(&out);
    let nc = posted.unwrap_or_else(|e| panic!("the wrapped op must post: {e}")).program;
    let golden = repo(WRAP_GOLDEN);
    if std::env::var_os("WRITE_GOLDEN").is_some() {
        std::fs::write(&golden, &nc).expect("write the wrap golden");
    } else {
        let want = std::fs::read_to_string(&golden).expect("the wrap golden");
        compare_wrap_golden(&nc, &want).unwrap_or_else(|why| panic!("{}: {why}; regenerate intentional changes with WRITE_GOLDEN=1", golden.display()));
    }
    let lines: Vec<&str> = nc.lines().collect();
    let wrap = lines.iter().position(|l| l.contains("OPERATION: WRAP")).expect("the WRAP section");
    let body = &lines[wrap..];
    // A counts on: consecutive A words never jump the long way round.
    let mut last: Option<f64> = None;
    let mut g93 = false;
    let mut turned_feeds = 0;
    for l in body {
        let ws: Vec<&str> = l.split(' ').collect();
        if ws.contains(&"G93") {
            g93 = true;
        }
        if ws.contains(&"G94") {
            g93 = false;
        }
        let a = ws.iter().find(|w| w.starts_with('A')).and_then(|w| num(w));
        if let (Some(p), Some(a)) = (last, a) {
            assert!((a - p).abs() <= 180.0, "A unwrapped: {p} -> {a} at {l}");
        }
        if g93 && let (Some(previous), Some(a)) = (last, a) {
            let feed = ws.iter().find(|w| w.starts_with('F')).and_then(|w| num(w)).expect("G93 F");
            assert!(feed > 0.0 && feed * (a - previous).abs() <= 360.0 + 1e-9, "A rate: {l}");
        }
        if a.is_some() {
            last = a;
        }
        if ws[0] == "G1" || ws.get(1) == Some(&"G1") {
            assert!(ws.iter().any(|w| w.starts_with('F')), "every feed carries F: {l}");
            let xyz = ws.iter().any(|w| w.starts_with('X') || w.starts_with('Y') || w.starts_with('Z'));
            if a.is_some() && xyz {
                assert!(g93, "a turning XYZ feed is in G93: {l}");
                turned_feeds += 1;
            } else {
                assert!(!g93, "an XYZ-only or A-only feed is in G94: {l}");
            }
        }
        if ws[0] == "M5" {
            assert!(!g93, "the section ends in G94");
            break;
        }
    }
    assert!(turned_feeds > 0, "the wrap turns A on its feeds\n{nc}");
}
