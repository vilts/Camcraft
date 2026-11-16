//! The solid law at the app seam (Prime Directive): an operation whose
//! tool stack enters part material that no cut plane slices - a drill
//! through a thin blind floor into the air under it, a facing pass under
//! a slab hanging over the part, a drill through an undercut lip into
//! the cavity below - was silent under the per-plane verifier (shown
//! here by a replica of its containment law, which read only
//! `slice::material_above` at each move's deepest plane), and is now a
//! Gouge on the generated operation that blocks the post. Plus the cost
//! of the law on the user's own part (`testdata/models/Milling-test.step`).
//!
//! Provenance: our own fixtures, specced from the Prime Directive; no
//! vendor material.
//!
//! Gate map (solid law): covers app::operation (generate, contain_into),
//! sim::verify (engine b2, solid_hits), app::posting.

use camcraft_app::cl::{MoveKind, Toolpath};
use camcraft_app::commands::{AddTool, CreateOperation, GenerateOperation, History, ImportModel, SetStock};
use camcraft_app::drilling::{CycleChoice, Hole};
use camcraft_app::geo2d::Point2;
use camcraft_app::geometry::{Mesh, Point3, Triangle, cube, slice};
use camcraft_app::model;
use camcraft_app::operation::{Height, OpParams, SrcRef};
use camcraft_app::posting;
use camcraft_app::schema::Location;
use camcraft_app::stock::StockShape;
use camcraft_app::tools::catalog;
use camcraft_app::verify::Warning;
use camcraft_app::world::new_world;
use flecs_ecs::prelude::World;

fn walls(l: &[(f64, f64)], z0: f64, z1: f64, out: bool, f: &mut Vec<Triangle>) {
    let mut v: Vec<(f64, f64)> = l.to_vec();
    if !out {
        v.reverse();
    }
    for k in 0..v.len() {
        let (a, b) = (v[k], v[(k + 1) % v.len()]);
        let (a0, b0, b1, a1) = (Point3::new(a.0, a.1, z0), Point3::new(b.0, b.1, z0), Point3::new(b.0, b.1, z1), Point3::new(a.0, a.1, z1));
        f.push(Triangle::new(a0, b0, b1));
        f.push(Triangle::new(a0, b1, a1));
    }
}

fn cap(o: [(f64, f64); 4], i: Option<[(f64, f64); 4]>, z: f64, up: bool, f: &mut Vec<Triangle>) {
    let p = |q: (f64, f64)| Point3::new(q.0, q.1, z);
    let mut push = |a: (f64, f64), b: (f64, f64), c: (f64, f64)| {
        f.push(if up { Triangle::new(p(a), p(b), p(c)) } else { Triangle::new(p(a), p(c), p(b)) });
    };
    match i {
        None => {
            push(o[0], o[1], o[2]);
            push(o[0], o[2], o[3]);
        }
        Some(i) => {
            for k in 0..4 {
                let n = (k + 1) % 4;
                push(o[k], o[n], i[n]);
                push(o[k], i[n], i[k]);
            }
        }
    }
}

fn rect(x0: f64, y0: f64, x1: f64, y1: f64) -> [(f64, f64); 4] {
    [(x0, y0), (x1, y0), (x1, y1), (x0, y1)]
}

/// A plate Z [9, 20] with a 20 x 20 pocket down to a 1 mm floor (Z 9..10)
/// on four corner legs: air under the floor.
fn table() -> Mesh {
    let (o, h) = (rect(-20.0, -20.0, 20.0, 20.0), rect(-10.0, -10.0, 10.0, 10.0));
    let mut f = Vec::new();
    walls(&o, 9.0, 20.0, true, &mut f);
    cap(o, None, 9.0, false, &mut f);
    cap(o, Some(h), 20.0, true, &mut f);
    walls(&h, 10.0, 20.0, false, &mut f);
    cap(h, None, 10.0, true, &mut f);
    for (x, y) in [(-20.0, -20.0), (14.0, -20.0), (-20.0, 14.0), (14.0, 14.0)] {
        f.extend(cube(Point3::new(x, y, 0.0), Point3::new(x + 6.0, y + 6.0, 8.5)).facets.iter().cloned());
    }
    Mesh::from_facets(f)
}

/// A block Z [0, 20] with a 24 x 24 cavity Z [5, 15] under a 10 x 10
/// opening: the lip Z [15, 20] overhangs the cavity.
fn undercut() -> Mesh {
    let (o, c, h) = (rect(-20.0, -20.0, 20.0, 20.0), rect(-12.0, -12.0, 12.0, 12.0), rect(-5.0, -5.0, 5.0, 5.0));
    let mut f = Vec::new();
    walls(&o, 0.0, 20.0, true, &mut f);
    cap(o, None, 0.0, false, &mut f);
    cap(o, Some(h), 20.0, true, &mut f);
    walls(&h, 15.0, 20.0, false, &mut f);
    cap(c, Some(h), 15.0, false, &mut f);
    walls(&c, 5.0, 15.0, false, &mut f);
    cap(c, None, 5.0, true, &mut f);
    Mesh::from_facets(f)
}

/// A post Z [0, 10] and, apart from it, a slab Z [15, 16] over it all.
fn post_under_slab(slab: bool) -> Mesh {
    let mut f: Vec<Triangle> = cube(Point3::new(-5.0, -5.0, 0.0), Point3::new(5.0, 5.0, 10.0)).facets.to_vec();
    if slab {
        f.extend(cube(Point3::new(-20.0, -20.0, 15.0), Point3::new(20.0, 20.0, 16.0)).facets.iter().cloned());
    }
    Mesh::from_facets(f)
}

fn world_with(mesh: Mesh, tool: &str) -> (World, History) {
    let world = new_world();
    let mut h = History::default();
    h.execute(&world, ImportModel::from_mesh("part", mesh));
    let loc = Location { pos: Point3::new(0.0, 0.0, -2.0), ..Default::default() };
    h.execute(&world, SetStock::new(&world, StockShape::Box { dx: 60.0, dy: 60.0, dz: 22.0 }, loc).expect("stock"));
    let mut t = catalog().into_iter().find(|(n, _)| *n == tool).map(|(_, t)| t).expect("catalog");
    if tool == "DR-0400" {
        // A long-series flute: these holes run 14-15 mm from the z20
        // stock top, past the jobber's 12 mm - issue #84's reach law
        // would refuse them before the law under test speaks.
        t.flute_len = 30.0;
    }
    h.execute(&world, AddTool::new(&world, tool, t).expect("tool"));
    (world, h)
}

fn run(world: &World, h: &mut History, tool: &str, params: OpParams) -> (Toolpath, Vec<Warning>) {
    let op = CreateOperation::new(world, "Op1", tool, SrcRef::Entity("part".into()), params);
    h.execute(world, op.expect("create"));
    h.execute(world, GenerateOperation::new(world, "Op1").expect("generate"));
    let e = model::find_by_name(world, "Op1").expect("op");
    let tp = model::toolpath_of(world, &e).expect("cached path");
    (tp.path.clone(), tp.warnings.clone())
}

fn gouges(w: &[Warning]) -> usize {
    w.iter().filter(|w| matches!(w, Warning::Gouge { .. })).count()
}

/// The containment law the verifier read before: each cutting move's
/// centre, sampled every 0.25 mm, inside the PLAIN section at its
/// deepest plane.
fn plain_section_hits(t: &Toolpath, f: &[Triangle]) -> usize {
    t.moves
        .iter()
        .filter(|m| m.kind != MoveKind::Rapid)
        .filter(|m| {
            let Ok(s) = slice::material_above(f, m.from.z.min(m.to.z)) else {
                return true;
            };
            let (a, b) = (Point2::new(m.from.x, m.from.y), Point2::new(m.to.x, m.to.y));
            let n = ((b.x - a.x).hypot(b.y - a.y) / 0.25).ceil().clamp(1.0, 4096.0) as usize;
            (0..=n).any(|k| {
                let u = k as f64 / n as f64;
                s.contains(Point2::new(a.x + (b.x - a.x) * u, a.y + (b.y - a.y) * u))
            })
        })
        .count()
}

fn assert_blocked(world: &World, tag: &str) {
    let nc = std::env::temp_dir().join(format!("camcraft-solid-law-{tag}-{}.nc", std::process::id()));
    let _ = std::fs::remove_file(&nc);
    let err = posting::post_setup(world, &nc).expect_err("a gouge blocks the post");
    assert!(err.contains("post blocked"), "{err}");
    assert!(!nc.exists(), "blocked output is never written");
}

fn drill_op(at: Point2, floor: f64, bottom: Option<f64>) -> OpParams {
    OpParams::Drill {
        clearance_z: Height::fixed(30.0),
        retract_z: None,
        top_z: Height::fixed(22.0),
        bottom_z: bottom.map(Height::fixed),
        cycle: CycleChoice::Auto,
        peck: 0.0,
        dwell: 0.0,
        feed: 130.0,
        plunge_feed: 100.0,
        holes: vec![Hole { xy: at, radius: 2.0, entry_z: 20.0, floor_z: floor, through: false, stepped: false }],
        cross: Vec::new(),
    }
}

#[test]
fn a_drill_through_a_thin_blind_floor_into_air_is_a_gouge_and_never_posts() {
    let (world, mut h) = world_with(table(), "DR-0400");
    let (_, w) = run(&world, &mut h, "DR-0400", drill_op(Point2::new(0.0, 0.0), 10.0, None));
    assert_eq!(gouges(&w), 0, "to the floor is clean: {w:?}");
    // Bottom Z 5: through the 1 mm floor, the tip in the air under it.
    let (world, mut h) = world_with(table(), "DR-0400");
    let (t, w) = run(&world, &mut h, "DR-0400", drill_op(Point2::new(0.0, 0.0), 10.0, Some(5.0)));
    assert_eq!(plain_section_hits(&t, &table().facets), 0, "no plane at the tip's depth holds the floor");
    assert!(gouges(&w) > 0, "the floor is steel: {w:?}");
    assert_blocked(&world, "floor");
}

#[test]
fn a_drill_through_an_undercut_lip_is_a_gouge_and_never_posts() {
    // Through the opening: clean.
    let (world, mut h) = world_with(undercut(), "DR-0400");
    let (_, w) = run(&world, &mut h, "DR-0400", drill_op(Point2::new(0.0, 0.0), 6.0, None));
    assert_eq!(gouges(&w), 0, "through the opening is clean: {w:?}");
    // At x 8 the lip stands over the cavity: the drill bores the lip.
    let (world, mut h) = world_with(undercut(), "DR-0400");
    let (t, w) = run(&world, &mut h, "DR-0400", drill_op(Point2::new(8.0, 0.0), 6.0, None));
    assert_eq!(plain_section_hits(&t, &undercut().facets), 0, "the cavity's plane holds no steel at the tip");
    assert!(gouges(&w) > 0, "the lip is steel: {w:?}");
    assert_blocked(&world, "lip");
}

#[test]
fn facing_under_an_overhanging_slab_is_a_gouge_and_never_posts() {
    let face = OpParams::Face { holes: Default::default(),
        blend_turns: false,
        tilt: None,
        stepover: 4.0,
        clearance_z: Height::fixed(30.0),
        cut_z: Height::fixed(10.0),
        top_z: None,
        stepdown: None,
        feed: 1200.0,
        plunge_feed: 300.0,
        climb: true,
    };
    let (world, mut h) = world_with(post_under_slab(false), "EM-0800-4FL");
    let (_, w) = run(&world, &mut h, "EM-0800-4FL", face.clone());
    assert_eq!(gouges(&w), 0, "skimming the post top in the open is clean: {w:?}");
    let (world, mut h) = world_with(post_under_slab(true), "EM-0800-4FL");
    let (t, w) = run(&world, &mut h, "EM-0800-4FL", face);
    assert_eq!(plain_section_hits(&t, &post_under_slab(true).facets), 0, "no plane at Z 10 holds the slab");
    // The model law (b) read the same plane: nothing stands strictly above it.
    assert!(slice::material_above(&post_under_slab(true).facets, 10.0).is_ok_and(|s| s.is_empty()));
    assert!(gouges(&w) > 0, "the flute runs through the slab: {w:?}");
    assert_blocked(&world, "slab");
}

#[test]
fn the_solid_law_on_the_users_part_is_silent_and_cheap() {
    use camcraft_app::machining2d::{ContourParams, Lead, Side, contour_on_part};
    use camcraft_app::verify::{Cutter, Mode, SolidLaw, ToolGeom, VerifyContext, solid_hits, verify};
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testdata/models/Milling-test.step");
    let m = camcraft_app::geometry::import::load_model(&path).expect("the user's STEP imports");
    let (lo, hi) = m.facets.iter().flat_map(|t| [t.a.z, t.b.z, t.c.z]).fold((f64::INFINITY, f64::NEG_INFINITY), |(a, b), z| (a.min(z), b.max(z)));
    let region = slice::slice(&m.facets, hi - 1e-3).expect("top section");
    let p = ContourParams {
        tool_diameter: 10.0,
        stock_to_leave: 0.0,
        clearance_z: hi + 10.0,
        retract_z: hi + 5.0,
        top_z: hi,
        bottom_z: lo,
        stepdown: 2.5,
        feed: 1200.0,
        plunge_feed: 300.0,
        climb: true,
        side: Side::Auto,
        lead: Lead::Arc { radius: 3.0 },
        tolerance: 0.01,
        tab_count: 0,
        tab_width: 5.0,
        tab_height: 1.0,
        entry: camcraft_app::machining3d::Entry3D::Plunge,
        helix_radius: 1.0,
        blend_turns: false,
        links: camcraft_app::links::Links::PLANAR,
    };
    let t = contour_on_part(&region, &m.facets, &p).expect("contour");
    let g = ToolGeom { radius: 5.0, nose_radius: 0.0, flute_len: 22.0, shank_radius: 5.0, shank_len: 40.0, holder_radius: 22.0, holder_len: 35.0 };
    let law = SolidLaw { step: 0.25, allow: 0.01, hole_walls: false };
    let t0 = std::time::Instant::now();
    let w = solid_hits(&t, &m.facets, &Cutter::flat(&g), &law);
    let solid = t0.elapsed();
    let t1 = std::time::Instant::now();
    let all = verify(
        &t,
        &VerifyContext {
            region: &region,
            stock: None,
            model: Some(&m.facets),
            tool: &g,
            cutter: None,
            mode: Mode::Boundary,
            stock_top_z: hi,
            clearance_z: hi + 10.0,
            stock_to_leave: 0.0,
            gouge_tol: 0.01,
            z_tol: 1e-6,
            finish_tol: None,
            fixtures: &[],
            spindle: None,
            ..Default::default()
        },
    );
    let whole = t1.elapsed();
    eprintln!(
        "Milling-test.step: {} facets, {} moves ({} arcs): solid law {:?}, whole verify {:?}",
        m.facets.len(),
        t.moves.len(),
        t.arc_count(),
        solid,
        whole
    );
    assert_eq!(w, Vec::new(), "{:?}", t.notes);
    assert_eq!(all, Vec::new(), "{:?}", t.notes);
}
