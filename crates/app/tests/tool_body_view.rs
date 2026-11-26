//! #371: the tool-body reads the editor and the simulation take - axis
//! presets, the placement preview, tip and axis picks on the model-frame
//! mesh, the body posed at a tool position, the body of an operation's
//! tool, and the file read's refusals. Our own tests.
//!
//! Gate map (M4.4): covers app::tool_body_view, app::tool_body, app::pick.

use camcraft_app::commands::{AddTool, CreateOperation, History, SetStock, SetToolBody, ToolBodySpec};
use camcraft_app::geometry::{Point3, Triangle};
use camcraft_app::operation::{Height, OpParams, SrcRef};
use camcraft_app::schema::Location;
use camcraft_app::stock::StockShape;
use camcraft_app::tool_body_view::{self as view, AXIS_CHOICES};
use camcraft_app::tools::{catalog, Tool};
use camcraft_app::world::new_world;

const TOOL: &str = "EM-0400-2FL";

fn catalog_tool(name: &str) -> Tool {
    catalog().into_iter().find(|(n, _)| *n == name).map(|(_, t)| t).expect("catalog entry")
}

fn p(x: f64, y: f64, z: f64) -> Point3 {
    Point3::new(x, y, z)
}

/// A closed `n`-gon prism of circumradius `r` about model Z over `[z0, z1]`.
fn prism(r: f64, z0: f64, z1: f64, n: usize) -> Vec<Triangle> {
    let q = |k: usize, z: f64| {
        let a = std::f64::consts::TAU * k as f64 / n as f64;
        p(r * a.cos(), r * a.sin(), z)
    };
    let mut v = Vec::new();
    for k in 0..n {
        let (a0, a1, b0, b1) = (q(k, z0), q(k + 1, z0), q(k, z1), q(k + 1, z1));
        v.push(Triangle::new(a0, a1, b1));
        v.push(Triangle::new(a0, b1, b0));
        v.push(Triangle::new(p(0.0, 0.0, z0), a1, a0));
        v.push(Triangle::new(p(0.0, 0.0, z1), b0, b1));
    }
    v
}

fn close(a: [f64; 3], b: [f64; 3]) -> bool {
    (0..3).all(|i| (a[i] - b[i]).abs() < 1e-9)
}

#[test]
fn the_six_presets_name_themselves_and_scaled_axes_still_match() {
    for (name, d) in AXIS_CHOICES {
        assert_eq!(view::axis_label(d), Some(name));
    }
    assert_eq!(view::axis_label([0.0, 0.0, 7.5]), Some("+Z"));
    assert_eq!(view::axis_label([1.0, 1.0, 0.0]), None);
    assert_eq!(view::axis_label([0.0, 0.0, 0.0]), None);
}

#[test]
fn the_preview_is_the_placement_the_command_lands() {
    let mesh = prism(20.0, 0.0, 30.0, 16);
    let body = view::preview(&mesh, [0.0, 0.0, 1.0], 50.0).expect("a collar previews");
    assert!((body.profile().expect("a spindle body").bottom() - 50.0).abs() < 1e-6, "origin 50 above the tip puts the foot there");
    assert!((body.profile().expect("a spindle body").top() - 80.0).abs() < 1e-6);
    // The command placing the same spec stores the same facets.
    let world = new_world();
    let mut history = History::default();
    history.execute(&world, AddTool::new(&world, TOOL, catalog_tool(TOOL)).expect("tool"));
    let spec = ToolBodySpec { source: "c.stl".into(), facets: mesh.clone(), axis: [0.0, 0.0, 1.0], offset: 50.0 };
    history.execute(&world, SetToolBody::new(&world, TOOL, Some(spec)).expect("body"));
    let e = camcraft_app::model::find_by_name(&world, TOOL).expect("tool entity");
    let landed = camcraft_app::tool_body::body_of(&world, &e).expect("landed");
    assert_eq!(&*landed.facets, &*body.facets);
    // A placement that leaves the axis outside the body refuses by name.
    // (z 5..30 along model Z, so the +X axis runs beside the body; the
    // offset lifts its foot above the tip so the axis law is the one hit.)
    let off = view::preview(&prism(20.0, 5.0, 30.0, 16), [1.0, 0.0, 0.0], 25.0).expect_err("X axis misses a Z collar's box");
    assert!(off.contains("surround"), "{off}");
}

#[test]
fn a_tip_pick_takes_the_hit_facets_nearest_corner_and_zeroes_it() {
    let mesh = prism(20.0, 10.0, 40.0, 16);
    // Straight down onto the top cap near the rim at +X.
    let hit = view::pick(&mesh, [19.0, 0.5, 100.0], [0.0, 0.0, -1.0]).expect("the cap is hit");
    assert!((hit.hit[2] - 40.0).abs() < 1e-9);
    assert!(close(hit.vertex, [20.0, 0.0, 40.0]), "{:?}", hit.vertex);
    let n = hit.normal.expect("cap facet has area");
    assert!(n[2].abs() > 1.0 - 1e-9, "a cap normal is vertical: {n:?}");
    // Up the bottom cap's normal side: a pick from below sees z = 10.
    let low = view::pick(&mesh, [0.0, 1.0, -100.0], [0.0, 0.0, 1.0]).expect("bottom cap");
    let off = view::tip_offset_at(low.hit, [0.0, 0.0, 1.0]).expect("finite");
    assert!((off + 10.0).abs() < 1e-9, "the bottom at z 10 is the tip: offset -10, got {off}");
    let body = view::preview(&mesh, [0.0, 0.0, 1.0], off).expect("placed");
    assert!(body.profile().expect("a spindle body").bottom().abs() < 1e-6, "the picked point lands on the tip");
    assert!(view::pick(&mesh, [100.0, 100.0, 100.0], [0.0, 0.0, -1.0]).is_none(), "a miss is None");
    assert!(view::tip_offset_at([1.0, 2.0, 3.0], [0.0, 0.0, 0.0]).is_none());
}

#[test]
fn posing_maps_the_tool_frame_onto_the_tip_and_axis() {
    let t = vec![Triangle::new(p(0.0, 0.0, 0.0), p(1.0, 0.0, 0.0), p(0.0, 0.0, 10.0))];
    let up = view::posed(&t, [5.0, 6.0, 7.0], [0.0, 0.0, 1.0]);
    assert!(close([up[0].b.x, up[0].b.y, up[0].b.z], [6.0, 6.0, 7.0]), "vertical is the identity plus the tip");
    let side = view::posed(&t, [0.0, 0.0, 0.0], [1.0, 0.0, 0.0]);
    assert!(close([side[0].c.x, side[0].c.y, side[0].c.z], [10.0, 0.0, 0.0]), "+Z lands on +X");
    let down = view::posed(&t, [0.0, 0.0, 0.0], [0.0, 0.0, -1.0]);
    assert!(close([down[0].c.x, down[0].c.y, down[0].c.z], [0.0, 0.0, -10.0]), "a flipped axis flips");
    let bad = view::posed(&t, [0.0, 0.0, 0.0], [f64::NAN, 0.0, 0.0]);
    assert!(close([bad[0].c.x, bad[0].c.y, bad[0].c.z], [0.0, 0.0, 10.0]), "a bad axis falls back to vertical");
}

#[test]
fn an_operation_reads_its_tools_body() {
    let world = new_world();
    let mut history = History::default();
    history.execute(
        &world,
        SetStock::new(&world, StockShape::Box { dx: 60.0, dy: 60.0, dz: 20.0 }, Location::default()).expect("stock"),
    );
    history.execute(&world, AddTool::new(&world, TOOL, catalog_tool(TOOL)).expect("tool"));
    let face = OpParams::Face {
        holes: Default::default(),
        tilt: None,
        blend_turns: false, blend_radius: None,
        stepover: 3.0,
        clearance_z: Height::fixed(25.0),
        cut_z: Height::fixed(19.5),
        top_z: None,
        stepdown: None,
        feed: 1200.0,
        plunge_feed: 300.0,
        climb: true,
        picks: Default::default(),
    };
    history.execute(&world, CreateOperation::new(&world, "Face1", TOOL, SrcRef::Stock, face).expect("op"));
    assert!(view::body_for_operation(&world, "Face1").is_none(), "no body yet");
    let t = catalog_tool(TOOL);
    let z0 = t.flute_len + t.shank_len;
    let spec = ToolBodySpec { source: "c.stl".into(), facets: prism(20.0, z0, z0 + 30.0, 16), axis: [0.0, 0.0, 1.0], offset: 0.0 };
    history.execute(&world, SetToolBody::new(&world, TOOL, Some(spec)).expect("body"));
    assert!(view::body_for_operation(&world, "Face1").is_some_and(|b| b.source == "c.stl"));
    assert!(view::body_for_operation(&world, "Nope").is_none());
}

#[test]
fn the_file_read_refuses_missing_and_unknown_files_by_name() {
    let dir = std::env::temp_dir().join(format!("cc-371-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    let missing = view::load(&dir.join("absent.stl")).expect_err("missing file");
    assert!(!missing.is_empty());
    let odd = dir.join("body.txt");
    std::fs::write(&odd, "x").expect("write");
    let e = view::load(&odd).expect_err("unknown extension");
    assert!(e.contains("unsupported"), "{e}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_display_assembly_runs_from_the_tip_through_the_shank_into_the_body() {
    let t = catalog_tool(TOOL);
    let zs = t.flute_len + t.shank_len;
    let zr = |tris: &[Triangle]| {
        tris.iter().flat_map(|q| [q.a.z, q.b.z, q.c.z]).fold((f64::INFINITY, f64::NEG_INFINITY), |a, z| (a.0.min(z), a.1.max(z)))
    };
    let r_at = |tris: &[Triangle], z: f64| {
        tris.iter().flat_map(|q| [q.a, q.b, q.c]).filter(|p| (p.z - z).abs() < 1e-9).map(|p| p.x.hypot(p.y)).fold(0.0, f64::max)
    };
    // Seated on the shank top.
    let body = view::preview(&prism(20.0, 0.0, 30.0, 16), [0.0, 0.0, 1.0], zs).expect("seated");
    let all = view::assembly(&t, &body);
    assert_eq!(zr(&all), (0.0, zs + 30.0));
    let steel = &all[..all.len() - body.facets.len()];
    assert_eq!(zr(steel), (0.0, zs), "cutter and shank stop at the shank top");
    assert!((r_at(steel, 0.0) - t.diameter * 0.5).abs() < 1e-9, "the cutter's radius at the tip");
    assert!((r_at(steel, zs) - t.shank_d * 0.5).abs() < 1e-9, "the shank's radius at its top");
    // Standing 10 mm clear: the shank is carried up to the body's foot.
    let high = view::preview(&prism(20.0, 0.0, 30.0, 16), [0.0, 0.0, 1.0], zs + 10.0).expect("clear");
    let all = view::assembly(&t, &high);
    let steel = &all[..all.len() - high.facets.len()];
    assert!((zr(steel).1 - (zs + 10.0)).abs() < 1e-9, "no floating body");
}
