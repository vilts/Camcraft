//! #3300: a generated 5-axis program states its rotaries before its
//! first tilted feed. Swarf, 5-Axis Contour and 5-Axis Flow generate on
//! the tilt head (`assets/machines/tilthead.mch`, A and B rates stated),
//! and the Fanuc post must post them: the opening moves state the
//! upright Z column, the post commands it as A0 B0 at the clearance, and
//! the first tilted feed turns from that known angle. Before the fix the
//! post refused the first tilted feed (the A angle was not known).
//! Provenance: analytic test geometries (spherical cap, blade cylinder);
//! RS-274 modal rotary words; our own design notes.
//! Gate map: covers app::posting (generate + post seam), motion stamp
//! opening pose, post::dump, assets/posts/fanuc.cps.

use camcraft_app::cl::{is_vertical, MoveKind, Toolpath};
use camcraft_app::commands::{
    AddTool, CreateOperation, CreateSetup, GenerateOperation, History, ImportModel, LoadMachine,
    SetStock, SetWcsOrigin,
};
use camcraft_app::geometry::{Mesh, Point3, Triangle, quat_from_axis_angle, IDENTITY_Q};
use camcraft_app::operation::{self, Height, HeightReference, OpParams, SrcRef};
use camcraft_app::schema::Location;
use camcraft_app::stock::StockShape;
use camcraft_app::tools::{Tool, catalog};
use camcraft_app::{model, posting, world::new_world};
use flecs_ecs::prelude::World;
use std::path::PathBuf;

const FANUC: &str = include_str!("../../../assets/posts/fanuc.cps");
const BALL: &str = "BN-0600";
const SWARF_BALL: &str = "SWARF-BN-0600";

fn ok<T, E: std::fmt::Debug>(r: Result<T, E>, what: &str) -> T {
    r.unwrap_or_else(|e| panic!("{what}: {e:?}"))
}

fn cap_facets() -> Vec<Triangle> {
    const TAU: f64 = std::f64::consts::TAU;
    let (rad, max, nl, nc) = (12.0, std::f64::consts::FRAC_PI_3, 4usize, 12usize);
    let s = |x: f64, y: f64, z: f64| Point3::new(x, y, z - 9.0);
    let ring = |k: usize| -> Vec<Point3> {
        let ph = max * k as f64 / nl as f64;
        let (z, rr) = (rad * ph.cos(), rad * ph.sin());
        (0..nc).map(|j| s(rr * (TAU * j as f64 / nc as f64).cos(), rr * (TAU * j as f64 / nc as f64).sin(), z)).collect()
    };
    let (apex, first, mut f) = (s(0.0, 0.0, rad), ring(1), Vec::new());
    for j in 0..nc {
        f.push(Triangle::new(apex, first[j], first[(j + 1) % nc]));
    }
    for k in 1..nl {
        let (a, b) = (ring(k), ring(k + 1));
        for j in 0..nc {
            let n = (j + 1) % nc;
            f.push(Triangle::new(a[j], b[j], b[n]));
            f.push(Triangle::new(a[j], b[n], a[n]));
        }
    }
    let (rim, c) = (ring(nl), s(0.0, 0.0, rad * max.cos()));
    for j in 0..nc {
        f.push(Triangle::new(c, rim[j], rim[(j + 1) % nc]));
    }
    f
}

/// The issue's closed cylinder along X: radius 12, length 48.
fn blade_facets() -> Vec<Triangle> {
    const TAU: f64 = std::f64::consts::TAU;
    let (rad, x_len, nc, nx) = (12.0, 24.0, 16usize, 6usize);
    let v = |j: usize, i: usize| {
        Point3::new(-x_len + 2.0 * x_len * i as f64 / nx as f64, rad * (TAU * j as f64 / nc as f64).cos(), rad * (TAU * j as f64 / nc as f64).sin())
    };
    let mut f = Vec::new();
    for j in 0..nc {
        for i in 0..nx {
            let (a, b, c, d) = (v(j, i), v((j + 1) % nc, i), v((j + 1) % nc, i + 1), v(j, i + 1));
            f.push(Triangle::new(a, b, c));
            f.push(Triangle::new(a, c, d));
        }
        f.push(Triangle::new(Point3::new(x_len, 0.0, 0.0), v(j, nx), v((j + 1) % nc, nx)));
        f.push(Triangle::new(Point3::new(-x_len, 0.0, 0.0), v((j + 1) % nc, 0), v(j, 0)));
    }
    f
}

/// The document on the tilt head: part, both balls, Setup1, G54 at Z 50.
fn document(facets: Vec<Triangle>, stock: (StockShape, Location)) -> (World, History) {
    let world = new_world();
    let mut h = History::default();
    h.execute(&world, ImportModel::from_mesh("part", Mesh::from_facets(facets)));
    let tool = catalog().into_iter().find(|(n, _)| *n == BALL).map(|(_, t)| t).expect("ball");
    h.execute(&world, ok(AddTool::new(&world, BALL, tool.clone()), "add tool"));
    let swarf = Tool { flute_len: 60.0, shank_d: tool.diameter, ..tool };
    h.execute(&world, ok(AddTool::new(&world, SWARF_BALL, swarf), "add swarf tool"));
    h.execute(&world, ok(CreateSetup::new(&world, None, None), "Setup1"));
    let mch = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets/machines/tilthead.mch");
    h.execute(&world, ok(LoadMachine::from_file(&world, &mch), "mount tilthead"));
    h.execute(&world, ok(SetWcsOrigin::new(&world, Location { pos: Point3::new(0.0, 0.0, 50.0), rot: IDENTITY_Q }), "G54"));
    h.execute(&world, ok(SetStock::new(&world, stock.0, stock.1), "stock"));
    (world, h)
}

fn generate(world: &World, h: &mut History, tool: &str, params: OpParams) -> Toolpath {
    h.execute(world, ok(CreateOperation::new(world, "OP", tool, SrcRef::Entity("part".into()), params), "create"));
    h.execute(world, ok(GenerateOperation::new(world, "OP"), "generate"));
    let e = model::find_by_name(world, "OP").expect("op");
    let cached = model::toolpath_of(world, &e).unwrap_or_else(|| panic!("no path: {:?}", operation::status_of(world, &e)));
    assert!(!cached.warnings.iter().any(|w| w.blocks_post()), "blocking warnings {:?}", cached.warnings);
    cached.path
}

fn has_word(line: &str, letter: char, value: Option<f64>) -> bool {
    line.split_whitespace().any(|w| {
        w.strip_prefix(letter)
            .and_then(|v| v.parse::<f64>().ok())
            .is_some_and(|v| value.is_none_or(|want| (v - want).abs() < 1e-6))
    })
}

/// The law of the issue, for one generated operation.
fn opening_states_rotaries_before_first_tilted_feed(kind: &str, world: &World, path: &Toolpath) {
    let out = std::env::temp_dir().join(format!("camcraft_3300_{kind}_{}.nc", std::process::id()));
    let posted = posting::post_script(world, &out, FANUC);
    let _ = std::fs::remove_file(&out);
    let prog = posted.unwrap_or_else(|e| panic!("{kind}: the Fanuc post must post the program: {e}")).program;
    let lines: Vec<&str> = prog.lines().collect();
    // The stream: every move before the first lean states the Z column.
    let first_lean = path
        .moves
        .iter()
        .position(|m| m.axis.is_some_and(|a| !is_vertical(&a)))
        .unwrap_or_else(|| panic!("{kind}: the case must lean"));
    for (i, m) in path.moves[..first_lean].iter().enumerate() {
        assert!(m.axis.is_some_and(|a| is_vertical(&a)), "{kind}: opening move {i} states no upright pose: {m:?}");
    }
    assert_eq!(path.moves[0].kind, MoveKind::Rapid, "{kind}: the opening move is a positioning rapid");
    let zero = lines
        .iter()
        .position(|l| has_word(l, 'A', Some(0.0)) && has_word(l, 'B', Some(0.0)))
        .unwrap_or_else(|| panic!("{kind}: the program states A0 B0:\n{prog}"));
    let first_g01_turn = lines
        .iter()
        .position(|l| l.starts_with("G01") && (has_word(l, 'A', None) || has_word(l, 'B', None)))
        .unwrap_or_else(|| panic!("{kind}: a tilted feed posts:\n{prog}"));
    assert!(zero < first_g01_turn, "{kind}: A0 B0 comes before the first tilted feed:\n{prog}");
    assert!(lines[zero].starts_with("G00"), "{kind}: the zero rides a rapid: {}\n{prog}", lines[zero]);
    assert!(
        lines[..zero].iter().any(|l| l.starts_with("G00") && has_word(l, 'Z', None) && !has_word(l, 'X', None)),
        "{kind}: the tool retracts to a clearance before the rotaries move:\n{prog}"
    );
}

fn finish5d_heights() -> (Height, Height) {
    (Height::of(HeightReference::TopOfModel, -0.5), Height::of(HeightReference::TopOfModel, -2.5))
}

fn box_stock() -> (StockShape, Location) {
    (StockShape::Box { dx: 30.0, dy: 30.0, dz: 10.0 }, Location { pos: Point3::new(0.0, 0.0, -5.0), rot: IDENTITY_Q })
}

#[test]
fn contour5d_opening_states_rotaries_before_the_fanuc_tilted_feed() {
    let (top_z, bottom_z) = finish5d_heights();
    let params = OpParams::Contour5D {
        tolerance: 0.1, clearance_z: Height::fixed(20.0), retract_z: None, top_z, bottom_z,
        feed: 500.0, plunge_feed: 100.0, climb: true, lead_deg: 15.0, lag_deg: 0.0, max_tilt_deg: 60.0, shank_avoid: false,
        picks: Default::default(),
    };
    let (world, mut h) = document(cap_facets(), box_stock());
    let path = generate(&world, &mut h, BALL, params);
    opening_states_rotaries_before_first_tilted_feed("Contour5D", &world, &path);
}

#[test]
fn flow5d_opening_states_rotaries_before_the_fanuc_tilted_feed() {
    let (top_z, bottom_z) = finish5d_heights();
    let params = OpParams::Flow5D {
        tolerance: 0.1, stepover: Some(3.0), clearance_z: Height::fixed(20.0), retract_z: None, top_z, bottom_z,
        feed: 500.0, plunge_feed: 100.0, climb: true, lead_deg: 15.0, lag_deg: 0.0, max_tilt_deg: 60.0, shank_avoid: false,
        picks: Default::default(),
    };
    let (world, mut h) = document(cap_facets(), box_stock());
    let path = generate(&world, &mut h, BALL, params);
    opening_states_rotaries_before_first_tilted_feed("Flow5D", &world, &path);
}

#[test]
fn swarf_opening_states_rotaries_before_the_fanuc_tilted_feed() {
    let stock = (
        StockShape::Cylinder { diameter: 24.4, height: 48.0 },
        Location { pos: Point3::new(-24.0, 0.0, 0.0), rot: quat_from_axis_angle(1, std::f64::consts::FRAC_PI_2) },
    );
    let params = OpParams::Swarf {
        tolerance: 0.1, stepover: Some(2.0), clearance_z: Height::fixed(25.0), retract_z: None,
        top_z: Height::fixed(11.0), bottom_z: Height::fixed(5.0), feed: 500.0, plunge_feed: 100.0,
        climb: true, lead_deg: 15.0, lag_deg: 0.0, max_tilt_deg: 60.0,
        picks: Default::default(),
    };
    let (world, mut h) = document(blade_facets(), stock);
    let path = generate(&world, &mut h, SWARF_BALL, params);
    opening_states_rotaries_before_first_tilted_feed("Swarf", &world, &path);
}
