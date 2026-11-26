//! #2600: a tilted (3+1 / 3+2) slot that cuts the part is refused at the
//! post, as the same slot is refused upright. The reported part is a D50 x
//! 40 bar along X with two cross holes, from a STEP file, in a fitted box
//! stock on a 4th-axis mill; the slots are 6 mm wide and 0.5 mm into the
//! bar. Before the fix the STEP import left the bar's wall up to 25 mm
//! inside the true cylinder, so the tilted slots cut "air" and posted.
//! Provenance: our own fixture (`testdata/models/bar_cross_holes.step`, the
//! QA report's part); the law is the Prime Directive's "correct, or visibly
//! refused".
//!
//! Gate map (M4.4): covers app::geometry::import::step, app::posting, app::operation.

use flecs_ecs::prelude::World;

use camcraft_app::commands::{AddTool, CreateOperation, GenerateOperation, History, ImportModel, LoadMachine, SetStock};
use camcraft_app::geo2d::Point2;
use camcraft_app::geometry::{Mesh, Point3, Triangle, IDENTITY_Q};
use camcraft_app::machining3d::Entry3D;
use camcraft_app::model;
use camcraft_app::operation::{Height, OpParams, SrcRef, Tilt};
use camcraft_app::posting;
use camcraft_app::schema::Location;
use camcraft_app::stock::StockShape;
use camcraft_app::tools::catalog;
use camcraft_app::verify::Warning;
use camcraft_app::world::new_world;

const R: f64 = 25.0;
const LEN: f64 = 40.0;

fn step_bar() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testdata/models/bar_cross_holes.step")
}

/// A closed bar of radius [`R`] along X, from x 0 to [`LEN`], as a mesh.
fn meshed_bar(n: usize) -> Mesh {
    let pt = |x: f64, j: usize| {
        let th = std::f64::consts::TAU * (j % n) as f64 / n as f64;
        Point3::new(x, R * th.cos(), R * th.sin())
    };
    let (c0, c1) = (Point3::new(0.0, 0.0, 0.0), Point3::new(LEN, 0.0, 0.0));
    let mut f = Vec::new();
    for j in 0..n {
        let (a0, b0, a1, b1) = (pt(0.0, j), pt(0.0, j + 1), pt(LEN, j), pt(LEN, j + 1));
        f.push(Triangle::new(a0, a1, b1));
        f.push(Triangle::new(a0, b1, b0));
        f.push(Triangle::new(c0, a0, b0));
        f.push(Triangle::new(c1, b1, a1));
    }
    Mesh::from_facets(f)
}

/// The reported slot: 6 mm wide, from x 10 to x 30, 0.5 mm under the
/// bar's skin along the tool.
fn slot(tilt: Option<Tilt>) -> OpParams {
    OpParams::Slot {
        links: camcraft_app::links::Links::PLANAR,
        a: Point2::new(10.0, 0.0),
        b: Point2::new(30.0, 0.0),
        width: 6.0,
        stock_to_leave: 0.0,
        clearance_z: Height::fixed(40.0),
        retract_z: Some(Height::fixed(30.0)),
        top_z: Height::fixed(25.0),
        bottom_z: Height::fixed(24.5),
        stepdown: 0.5,
        stepover: 3.0,
        feed: 400.0,
        plunge_feed: 300.0,
        climb: true,
        entry: Entry3D::Plunge,
        helix_radius: 1.0, blend_turns: false, blend_radius: None,
        tilt,
        extend_start: 0.0,
        extend_end: 0.0,
        picks: Default::default(),
    }
}

fn tilt(a_deg: f64) -> Option<Tilt> {
    Some(Tilt { basis_version: 0, a_deg, b_deg: 0.0, origin: [0.0, 0.0, 0.0] })
}

/// `bar` in its fitted stock on the 4th-axis mill, with the slot generated
/// upright (`S0`) and on the two sides (`S90`, `S-90`).
fn world_with(bar: ImportModel) -> World {
    let world = new_world();
    let mut history = History::default();
    let asset = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/machines/tormach-770mx-microarc4.mch");
    history.execute(&world, LoadMachine::from_file(&world, &asset).expect("mount"));
    let (name, tool) = catalog().into_iter().find(|(n, _)| *n == "EM-0600-4FL").expect("catalog entry");
    history.execute(&world, AddTool::new(&world, name, tool).expect("tool"));
    history.execute(&world, bar);
    let loc = Location { pos: Point3::new(LEN * 0.5, 0.0, -R), rot: IDENTITY_Q };
    let stock = StockShape::Box { dx: LEN, dy: 2.0 * R, dz: 2.0 * R };
    history.execute(&world, SetStock::new(&world, stock, loc).expect("stock"));
    for (op, tilt) in [("S0", None), ("S90", tilt(90.0)), ("S-90", tilt(-90.0))] {
        history.execute(&world, CreateOperation::new(&world, op, "EM-0600-4FL", SrcRef::Stock, slot(tilt)).expect("create"));
        history.execute(&world, GenerateOperation::new(&world, op).expect("generate"));
    }
    world
}

/// Every slot is convicted of a gouge at generation, and the post refuses
/// and writes no file.
fn assert_refused(world: &World, tag: &str) {
    for op in ["S0", "S90", "S-90"] {
        let warnings = model::find_by_name(world, op).and_then(|e| model::toolpath_of(world, &e)).map(|c| c.warnings).unwrap_or_default();
        assert!(warnings.iter().any(|w| matches!(w, Warning::Gouge { .. })), "{tag}: {op} cuts 0.5 mm into the bar and must be a gouge: {warnings:?}");
    }
    let path = std::env::temp_dir().join(format!("camcraft-2600-{tag}-{}.nc", std::process::id()));
    let _ = std::fs::remove_file(&path);
    let posted = posting::post_setup(world, &path);
    let wrote = path.exists();
    let _ = std::fs::remove_file(&path);
    let err = posted.err().unwrap_or_else(|| panic!("{tag}: the post must refuse"));
    assert!(err.contains("cuts past the finish surface"), "{tag}: refused by name: {err}");
    assert!(!wrote, "{tag}: no file is written");
}

/// The pin: the reported STEP bar. Red before the fix (S90 and S-90 came
/// out clean and the program posted).
#[test]
fn a_tilted_slot_into_the_step_bar_is_refused() {
    assert_refused(&world_with(ImportModel::new(&step_bar()).expect("the bar imports")), "step");
}

/// The guard: on a bar whose mesh IS the cylinder, the tilted slots were
/// already convicted at generation - the tilt was never the blind spot.
#[test]
fn a_tilted_slot_into_a_meshed_bar_is_refused() {
    assert_refused(&world_with(ImportModel::from_mesh("bar", meshed_bar(96))), "mesh");
}

/// The cause, pinned on its own: every facet of the imported bar's wall
/// lies on the cylinder. Before the fix a wall facet ran from the end rim
/// to a cross hole's rim a quarter turn away, through the solid.
#[test]
fn the_step_bars_wall_facets_lie_on_the_cylinder() {
    let (mesh, chord) = camcraft_app::geometry::import::step::parse_with_chord(&std::fs::read(step_bar()).expect("fixture")).expect("imports");
    assert!(chord <= camcraft_app::geometry::import::step::CHORD_CAP + 1e-9, "full fidelity: {chord}");
    let rad = |p: Point3| p.y.hypot(p.z);
    let mut wall = 0usize;
    for t in mesh.facets.iter() {
        let v = [t.a, t.b, t.c];
        // A wall facet: every vertex on the cylinder, and not an end cap.
        if v.iter().all(|p| rad(*p) > R - 0.01) && !(v.iter().all(|p| p.x < 1e-6) || v.iter().all(|p| p.x > LEN - 1e-6)) {
            wall += 1;
            for (a, b) in [(t.a, t.b), (t.b, t.c), (t.c, t.a)] {
                let mid = Point3::new(0.5 * (a.x + b.x), 0.5 * (a.y + b.y), 0.5 * (a.z + b.z));
                assert!(rad(mid) > R - 0.02, "a wall facet edge runs {} mm inside the cylinder: {a:?} - {b:?}", R - rad(mid));
            }
        }
    }
    assert!(wall > 0, "fixture: the bar has a wall");
}

/// #2713: a wall facet's normal follows the cylinder. The #2600 split left
/// flat "cap" facets (three points on the wall, the apex over the long
/// edge's middle) whose normal stood up to 65.5 degrees off the radius; the
/// pick then showed their edges as creases on the smooth wall.
#[test]
fn the_step_bars_wall_facet_normals_follow_the_cylinder() {
    let (mesh, _) = camcraft_app::geometry::import::step::parse_with_chord(&std::fs::read(step_bar()).expect("fixture")).expect("imports");
    // Two facets that each stand under this off the radius bend by less
    // than the pick's 40 degree crease angle plus the cylinder's own turn.
    const LIMIT_DEG: f64 = 25.0;
    let rad = |p: Point3| p.y.hypot(p.z);
    let (mut wall, mut off, mut worst) = (0usize, 0usize, 0.0f64);
    for t in mesh.facets.iter() {
        let v = [t.a, t.b, t.c];
        // A wall facet: every vertex on the cylinder, and not an end cap.
        if !v.iter().all(|p| (rad(*p) - R).abs() < 0.01) || v.iter().all(|p| p.x < 1e-6) || v.iter().all(|p| p.x > LEN - 1e-6) {
            continue;
        }
        let (e1, e2) = ([t.b.x - t.a.x, t.b.y - t.a.y, t.b.z - t.a.z], [t.c.x - t.a.x, t.c.y - t.a.y, t.c.z - t.a.z]);
        let n = [e1[1] * e2[2] - e1[2] * e2[1], e1[2] * e2[0] - e1[0] * e2[2], e1[0] * e2[1] - e1[1] * e2[0]];
        let len = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
        let (cy, cz) = ((t.a.y + t.b.y + t.c.y) / 3.0, (t.a.z + t.b.z + t.c.z) / 3.0);
        if len <= 0.0 {
            continue;
        }
        wall += 1;
        let deg = ((n[1] * cy + n[2] * cz) / (len * cy.hypot(cz))).clamp(-1.0, 1.0).acos().to_degrees();
        worst = worst.max(deg);
        off += usize::from(deg > LIMIT_DEG);
    }
    println!("wall facets {wall} of {}, normal over {LIMIT_DEG} degrees off the radius: {off}, worst {worst:.1}", mesh.facets.len());
    assert!(wall > 1000, "fixture: the bar has a wall: {wall}");
    assert_eq!(off, 0, "wall facets stand off the cylinder's normal, worst {worst:.1} degrees");
}
