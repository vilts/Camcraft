//! #3210: a constant-taper wire cut judged against a meshed cone. The
//! cone's top and base are meshed at different chord counts (128 and
//! 64 segments), each inside the importer's chord promise, so its
//! facets stand off the true cone by up to that chord. At the cone's
//! true angle the cut posts: the kerf law reads the facets under the
//! model's chord tolerance and the report line says so. A taper that
//! leans the wire farther than the wall leaves the base oversize: the
//! undercut law blocks the post by name. A taper that leans it less
//! still gouges.
//! Not exercised on a real EDM control.
//!
//! Gate map (#3210): covers app::operation::wire_verify (the chord
//! tolerance of a taper cut) and sim::verify::wire (the undercut law).
//! Provenance: our own tests over our own synthetic fixture.

use flecs_ecs::prelude::World;

use camcraft_app::commands::{AddTool, CreateOperation, GenerateOperation, History, ImportModel, LoadMachine};
use camcraft_app::geometry::{Mesh, Point3, Triangle};
use camcraft_app::machine::parse_mch;
use camcraft_app::model;
use camcraft_app::operation::{FeatureSel, GeoSel, Height, OpParams, SrcRef, WireContourOp, WireSide};
use camcraft_app::posting;
use camcraft_app::world::new_world;

const UV: &str = include_str!("edm/wire-iso-fixture.mch");
const WIRE: &str = "Generic ISO Wire EDM";
const H: f64 = 12.0;
/// The cone's true wall angle, degrees.
const ANGLE: f64 = 5.194;
/// The base radius: its 64-gon stands 0.0042 mm inside the true circle
/// at an edge's middle, inside the importer's 0.005 mm chord promise.
const R_BASE: f64 = 3.5;
const N_BASE: usize = 64;

fn ring(r: f64, n: usize, z: f64, turn: f64) -> Vec<Point3> {
    (0..n)
        .map(|i| {
            let t = std::f64::consts::TAU * (i as f64 + turn) / n as f64;
            Point3::new(r * t.cos(), r * t.sin(), z)
        })
        .collect()
}

/// A cone punch: the base a 64-gon of radius `R_BASE` at Z0, the top a
/// 128-gon narrower by the wall's lean at `H`. Each base vertex stands
/// between two top vertices, so the wire laid from the top ring passes
/// the base ring's vertices at its chords' middles.
fn cone() -> Mesh {
    let r_top = R_BASE - H * ANGLE.to_radians().tan();
    let lo = ring(R_BASE, N_BASE, 0.0, 0.25);
    let hi = ring(r_top, 2 * N_BASE, H, 0.0);
    let (c0, c1) = (Point3::new(0.0, 0.0, 0.0), Point3::new(0.0, 0.0, H));
    let mut f = Vec::new();
    for i in 0..hi.len() {
        f.push(Triangle::new(c1, hi[i], hi[(i + 1) % hi.len()]));
    }
    for j in 0..N_BASE {
        let (b0, b1) = (lo[j], lo[(j + 1) % N_BASE]);
        let (t0, t1, t2) = (hi[2 * j], hi[2 * j + 1], hi[(2 * j + 2) % hi.len()]);
        f.push(Triangle::new(c0, b1, b0));
        f.push(Triangle::new(b0, b1, t2));
        f.push(Triangle::new(b0, t2, t1));
        f.push(Triangle::new(b0, t1, t0));
    }
    Mesh::from_facets(f)
}

fn top_face(world: &World) -> GeoSel {
    let e = model::find_by_name(world, "part").expect("part");
    let f = model::features_of(world, &e).expect("features");
    let id = f
        .faces
        .iter()
        .filter(|fa| fa.normal[2].abs() > 0.99)
        .filter(|fa| fa.loops.iter().flat_map(|l| &l.verts).any(|&v| (f.verts[v as usize][2] - H).abs() < 1e-9))
        .max_by(|a, b| a.area.total_cmp(&b.area))
        .map(|fa| fa.id)
        .expect("a top face");
    GeoSel { entity: "part".into(), feature: FeatureSel::Face(id) }
}

/// The cone imported as "part" and a capture punch cut around its top
/// face at `taper_deg`, generated.
fn punch(taper_deg: f64) -> World {
    let world = new_world();
    let mut h = History::default();
    let m = parse_mch(UV).expect("the fixture parses");
    h.execute(&world, LoadMachine::new(&world, "wire-chord.mch".into(), Some(m)).expect("mount"));
    h.execute(&world, AddTool::new(&world, "EDM-W025", camcraft_app::wire_tools::wire_tool(0.25)).expect("wire"));
    h.execute(&world, ImportModel::from_mesh("part", cone()));
    let src = SrcRef::Selection(vec![top_face(&world)]);
    let op = WireContourOp {
        lead_arc_radius: None,
        top_z: Height::fixed(H),
        bottom_z: Height::fixed(0.0),
        condition: "C110".into(),
        side: WireSide::Punch,
        picks: camcraft_app::path_picks::PathPicks { start: vec![camcraft_app::path_picks::PathPick::xy(camcraft_app::geo2d::Point2::new(-10.0, 0.0))], exit: Vec::new() },
        tabs: None,
        capture: true,
        both_pieces_restrained: false,
        stock_centered_in_xy_travel: false,
        taper_deg,
        burnout: None,
        clearance: camcraft_app::operation::DEFAULT_CLEARANCE,
        four_axis: None,
        skims: Vec::new(),
        sharp_corners: false,
    };
    let c = CreateOperation::new(&world, "Punch1", "EDM-W025", src, OpParams::WireContour(op)).expect("op");
    h.execute(&world, c);
    let g = GenerateOperation::new(&world, "Punch1").expect("the cone generates");
    h.execute(&world, g);
    world
}

fn post(world: &World, tag: &str) -> Result<String, String> {
    let out = std::env::temp_dir().join(format!("camcraft-wire-chord-{tag}-{}.nc", std::process::id()));
    let cps = posting::builtin_post_text(WIRE).expect("registered");
    let posted = posting::post_script(world, &out, cps);
    let _ = std::fs::remove_file(&out);
    posted.map(|p| p.program)
}

/// Done means: the cone posts at its true angle; the kerf law reads its
/// facets under the model's chord tolerance and the line says so.
#[test]
fn a_meshed_cone_posts_at_its_true_angle_under_the_chord_tolerance() {
    let world = punch(ANGLE);
    let e = model::find_by_name(&world, "Punch1").expect("op");
    let OpParams::WireContour(op) = model::operation_of(&world, &e).expect("op").params else { panic!("wire") };
    assert!(!op.sharp_corners, "the chord fixture keeps sharp corners off");
    let clause = posting::wire_clause(&world);
    assert!(clause.problems.is_empty(), "{:?}", clause.problems);
    let line = clause.line.expect("the report line");
    assert!(line.contains("kerf judged under the model's chord tolerance 0.0050 mm"), "{line}");
    post(&world, "true").expect("the true angle posts");
}

/// Done means: a taper that leans the wire past the wall leaves the
/// base oversize, and the undercut law blocks the post by name.
#[test]
fn a_taper_past_the_wall_is_refused_as_an_undercut() {
    let world = punch(5.3);
    let err = post(&world, "steep").expect_err("an undercut never posts");
    assert!(err.starts_with("post blocked") && err.contains("Punch1: wire undercut: the kerf of move "), "{err}");
    assert!(err.contains("past the model's chord tolerance 0.0050 mm"), "{err}");
    assert!(!err.contains("wire gouge"), "{err}");
}

/// Guard: a taper that leans the wire less than the wall still gouges
/// past the chord tolerance.
#[test]
fn a_taper_short_of_the_wall_still_gouges() {
    let world = punch(5.1);
    let err = post(&world, "shallow").expect_err("a gouge never posts");
    assert!(err.starts_with("post blocked") && err.contains("Punch1: wire gouge"), "{err}");
}
