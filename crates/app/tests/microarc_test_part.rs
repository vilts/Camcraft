//! #785: the microARC 4 hardware test part, built through the app's own
//! commands - the job the tester kit `docs/tester-kits/tormach-microarc-4th/`
//! ships. A 3/4" round bar in the 4th-axis chuck, axis along world X, the
//! free end at X0 (G54 X0 at the stock face, Y0 Z0 on the A centerline):
//! two opposite shallow flats at A-90 and A90 and a shallow cross slot back
//! at A0, each an indexed A then an ordinary 3-axis cut. (The face
//! underneath, the pose law's A0 B180, is A180 on this table since #797 -
//! `fourth_axis_pose` pins it; the kit keeps its +-90 pair.) The posted program is the
//! kit's golden; a continuous wrap op on the same bar must be refused by
//! name (simultaneous 4-axis is unconfirmed on PathPilot, #785).
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
        helix_radius: 1.0,
        links: camcraft_app::links::Links::PLANAR,
        tilt,
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
        helix_radius: 0.5,
        links: camcraft_app::links::Links::PLANAR,
        tilt: None,
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
        ("FLAT A-90", "EM 1/4 3FL", flat(Some(Tilt { a_deg: -90.0, b_deg: 0.0, origin: [0.0; 3] }))),
        ("FLAT A90", "EM 1/4 3FL", flat(Some(Tilt { a_deg: 90.0, b_deg: 0.0, origin: [0.0; 3] }))),
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

/// The continuous op the kit carries for the follow-up: a wrapped pass
/// around the diameter turns A inside the operation, which PathPilot is
/// not confirmed to run - the post refuses it by name and writes nothing.
#[test]
fn a_wrapped_op_on_the_test_part_is_refused_by_name() {
    let world = build();
    let mut h = History::default();
    let wrap = OpParams::RotaryParallel {
        tolerance: 0.1,
        stepover: Some(2.0),
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
    };
    let ball = tool("BN-0600", 0.125, 8000.0, 8.0 * IN);
    h.execute(&world, ok(AddTool::new(&world, "BN 1/8", ball), "ball"));
    let created = CreateOperation::new(&world, "WRAP", "BN 1/8", SrcRef::Entity("bar core".into()), wrap);
    h.execute(&world, created.expect("create the wrap op"));
    h.execute(&world, GenerateOperation::new(&world, "WRAP").expect("generate"));
    let out = std::env::temp_dir().join(format!("camcraft_microarc_wrap_{}.nc", std::process::id()));
    let err = camcraft_app::posting::post_setup(&world, &out).expect_err("a wrap must not post");
    assert!(!out.exists(), "a refused post writes nothing");
    // #797: the machine check reads the wrap's poses through the tool
    // axis - every one is an angle about X this table reaches (the pass
    // under the bar, A0 B180, is A180) - so the refusal is the post's,
    // naming the op: the turning A, the wall the follow-up lifts.
    assert!(err.contains("simultaneous 4-axis motion in operation WRAP"), "{err}");
    let check = camcraft_app::machine::check_document(
        &world,
        &camcraft_app::machine::of(&world).1.expect("the mounted machine"),
    )
    .expect("checks");
    assert!(check.problems.iter().all(|p| !p.starts_with("WRAP:")), "{:?}", check.problems);
    // The same refusal, straight from the engine on the same stream.
    let records = camcraft_app::posting::stream_records(&world).expect("the stream");
    let mch = camcraft_app::post::machine::mirror(
        &camcraft_app::machine::parse_mch(
            camcraft_app::posting::MACHINES
                .iter()
                .find(|(n, _)| *n == "Tormach 770MX + microARC 4")
                .map(|(_, t)| *t)
                .expect("shipped"),
        )
        .expect("parses"),
    );
    let cps = std::fs::read_to_string(repo("assets/posts/tormach.cps")).expect("the post");
    let post = camcraft_app::post::engine::run(&cps, &mch, &camcraft_app::post::cnc::serialize(&records))
        .expect("the engine runs");
    assert!(
        post.warnings.iter().any(|w| w.contains("simultaneous 4-axis motion in operation WRAP")),
        "{:?}",
        post.warnings
    );
}
