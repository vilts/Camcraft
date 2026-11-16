//! Radial retracts vs fixtures (M13.8): Rotary Parallel links its
//! passes radially - out to the retract cylinder, index, back in - so
//! a pass under the bar's equator retracts DOWNWARD, its whole stack
//! sweeping toward whatever workholding sits below the bar. The
//! fixture laws (c2)/(d2) judge every posed move with its real pose:
//! steel planted where a downward link's holder sweeps convicts and
//! blocks the post; the same program with the steel moved clear is
//! silent.
//! Provenance: our own fixtures (the rotary suite's turned bar laid
//! along x); the law is the Prime Directive's "never crash".
//!
//! Gate map (M4.4): covers app::fixture, sim::fixture, app::operation, app::posting, sim::verify, motion::multiaxis.

use flecs_ecs::prelude::World;

use camcraft_app::cl::MoveKind;
use camcraft_app::commands::{
    AddTool, CreateOperation, GenerateOperation, History, ImportModel, SetFixtureFixed, SetStock,
    TagFixture,
};
use camcraft_app::verify::Warning;
use camcraft_app::geometry::{quat_from_axis_angle, Mesh, Point3, Triangle};
use camcraft_app::model;
use camcraft_app::operation::{Height, OpParams, SrcRef};
use camcraft_app::posting;
use camcraft_app::schema::Location;
use camcraft_app::stock::StockShape;
use camcraft_app::tools::catalog;

/// The turned bar's meridian (rho, z): wall r6, shoulder, neck r5.
const MERIDIAN: [(f64, f64); 6] = [(0.0, -16.0), (6.0, -16.0), (6.0, -2.0), (5.0, 2.0), (5.0, 16.0), (0.0, 16.0)];

/// The bar laid down: axis world x, crown world z.
fn laid(n: usize) -> Vec<Triangle> {
    let pt = |rho: f64, z: f64, j: usize| {
        let th = std::f64::consts::TAU * j as f64 / n as f64;
        Point3::new(z, rho * th.cos(), rho * th.sin())
    };
    let mut f = Vec::new();
    for w in MERIDIAN.windows(2) {
        let ((ra, za), (rb, zb)) = (w[0], w[1]);
        for j in 0..n {
            let k = (j + 1) % n;
            let (a1, a2, b1, b2) = (pt(ra, za, j), pt(ra, za, k), pt(rb, zb, j), pt(rb, zb, k));
            if ra <= 1e-12 {
                f.push(Triangle::new(a1, b2, b1));
            } else if rb <= 1e-12 {
                f.push(Triangle::new(a1, a2, b2));
            } else {
                f.push(Triangle::new(a1, a2, b2));
                f.push(Triangle::new(a1, b2, b1));
            }
        }
    }
    f
}

/// Rotary Parallel over the laid bar, its raster run down past the
/// equator (bottom 8 under the axis): the overrun passes beside the
/// bar hold radials pointing below the horizon.
fn world() -> (World, History) {
    let world = camcraft_app::world::new_world();
    let mut h = History::default();
    let tool = catalog().into_iter().find(|(n, _)| *n == "BN-0600").map(|(_, t)| t).expect("catalog ball");
    h.execute(&world, AddTool::new(&world, "BN-0600", tool).expect("tool"));
    let loc = Location { pos: Point3::new(-16.0, 0.0, 0.0), rot: quat_from_axis_angle(1, std::f64::consts::FRAC_PI_2) };
    let stock = SetStock::new(&world, StockShape::Cylinder { diameter: 12.4, height: 32.0 }, loc);
    h.execute(&world, stock.expect("bar stock"));
    h.execute(&world, ImportModel::from_mesh("bar", Mesh::from_facets(laid(16))));
    let params = OpParams::RotaryParallel {
        tolerance: 0.1,
        stepover: Some(2.0),
        clearance_z: Height::fixed(25.0),
        retract_z: None,
        top_z: Height::fixed(6.0),
        bottom_z: Height::fixed(-8.0),
        feed: 500.0,
        plunge_feed: 100.0,
        climb: true,
        lead_deg: 0.0,
        lag_deg: 0.0,
        max_tilt_deg: 60.0,
    };
    let op = CreateOperation::new(&world, "OP", "BN-0600", SrcRef::Entity("bar".into()), params);
    h.execute(&world, op.expect("create"));
    h.execute(&world, GenerateOperation::new(&world, "OP").expect("generate"));
    (world, h)
}

fn cache(world: &World) -> camcraft_app::schema::Toolpath {
    model::find_by_name(world, "OP").and_then(|e| model::toolpath_of(world, &e)).expect("cached")
}

/// A closed axis-aligned box as facets (its world AABB is the box).
fn cube(lo: Point3, hi: Point3) -> Vec<Triangle> {
    vec![Triangle::new(lo, Point3::new(hi.x, hi.y, hi.z), Point3::new(lo.x, hi.y, hi.z))]
}

fn plant(world: &World, h: &mut History, lo: Point3, hi: Point3) {
    h.execute(world, ImportModel::from_mesh("chuck", Mesh::from_facets(cube(lo, hi))));
    h.execute(world, TagFixture::new(world, "chuck", None).expect("tag the steel"));
}

fn fixture_problems(world: &World) -> Vec<String> {
    posting::verification_report(world)
        .problems
        .into_iter()
        .filter(|p| p.contains("fixture"))
        .collect()
}

/// The holder's middle on the deepest-pointing link rapid: where a
/// downward index carries the stack under the bar's equator.
fn under_holder(world: &World) -> Point3 {
    let t = cache(world).path;
    let tool = catalog().into_iter().find(|(n, _)| *n == "BN-0600").map(|(_, t)| t).expect("ball");
    let g = tool.geom();
    let reach = g.flute_len + g.shank_len + 0.5 * g.holder_len;
    let m = t
        .moves
        .iter()
        .filter(|m| m.kind == MoveKind::Rapid && m.from != m.to)
        .filter_map(|m| m.axis.map(|a| (m, a)))
        .min_by(|(_, a), (_, b)| a[2].total_cmp(&b[2]))
        .expect("the links index at rapid");
    let (mv, a) = m;
    assert!(a[2] < -0.3, "the raster's overrun leaves passes under the equator: deepest pose {a:?}");
    Point3::new(mv.to.x + reach * a[0], mv.to.y + reach * a[1], mv.to.z + reach * a[2])
}

#[test]
fn steel_under_the_equator_convicts_a_downward_radial_link() {
    let (world, mut h) = world();
    assert!(fixture_problems(&world).is_empty(), "no steel, no fixture problem");
    let c = under_holder(&world);
    // A 4 mm block of chuck at the holder's middle, well clear of the
    // bar (the holder stands past flute + shank from the tip).
    let e = 2.0;
    plant(&world, &mut h, Point3::new(c.x - e, c.y - e, c.z - e), Point3::new(c.x + e, c.y + e, c.z + e));
    let probs = fixture_problems(&world);
    assert!(!probs.is_empty(), "a holder swept through steel under the bar must convict");
    let path = std::env::temp_dir().join(format!("camcraft-rotfix-hit-{}.nc", std::process::id()));
    let _ = std::fs::remove_file(&path);
    assert!(posting::post_setup(&world, &path).is_err(), "the convicted program never posts");
    assert!(!path.exists(), "nothing written");
}

#[test]
fn the_same_steel_moved_clear_is_silent() {
    let (world, mut h) = world();
    let c = under_holder(&world);
    // The same block, carried 200 mm down the bar's axis - past the
    // raster's overrun and every link.
    let (e, dx) = (2.0, 200.0);
    plant(&world, &mut h, Point3::new(c.x + dx - e, c.y - e, c.z - e), Point3::new(c.x + dx + e, c.y + e, c.z + e));
    let probs = fixture_problems(&world);
    assert!(probs.is_empty(), "steel clear of every link is silent: {:?}", &probs[..probs.len().min(4)]);
}

/// The op's own fixture verdict, in the gate's wording (`OP: <hit>`).
fn generated_hits(world: &World) -> Vec<String> {
    cache(world)
        .warnings
        .iter()
        .filter(|w| matches!(w, Warning::FixtureHit { .. }))
        .map(|w| format!("OP: {w}"))
        .collect()
}

/// M13.8 owed: generation runs the post gate's workholding laws - the
/// spindle on posed rapids, the steel split by whether it turns - so the
/// op's warnings and the gate's problems are one verdict, hit for hit.
#[test]
fn the_generate_time_verdict_matches_the_post_gates() {
    let (world, mut h) = world();
    let c = under_holder(&world);
    let e = 2.0;
    plant(&world, &mut h, Point3::new(c.x - e, c.y - e, c.z - e), Point3::new(c.x + e, c.y + e, c.z + e));
    // Workholding is a live gate input, never a stamp: the steel was
    // planted after the path, so the op is regenerated to judge it.
    h.execute(&world, GenerateOperation::new(&world, "OP").expect("regenerate"));
    let generated = generated_hits(&world);
    assert!(!generated.is_empty(), "the under-bar block convicts at generation too");
    assert_eq!(generated, fixture_problems(&world), "generation and the gate speak one verdict");
}

/// The same block declared FIXED (a tailstock, not a chuck): the rotary
/// turns the part so the tool only ever meets it from above - clear in
/// the machine frame, so the generate-time hit clears with the gate's.
#[test]
fn a_fixed_block_clear_in_the_machine_frame_clears_the_generated_hit() {
    let (world, mut h) = world();
    let c = under_holder(&world);
    let e = 2.0;
    plant(&world, &mut h, Point3::new(c.x - e, c.y - e, c.z - e), Point3::new(c.x + e, c.y + e, c.z + e));
    h.execute(&world, SetFixtureFixed::new(&world, "chuck", true).expect("declare it fixed"));
    h.execute(&world, GenerateOperation::new(&world, "OP").expect("regenerate"));
    let generated = generated_hits(&world);
    assert!(generated.is_empty(), "a fixed block under the bar is clear on the machine: {generated:?}");
    assert!(fixture_problems(&world).is_empty(), "and the gate agrees");
    // Back to turning (undo the regenerate, then the declaration): the
    // path-frame verdict returns, at generation and at the gate alike.
    assert!(h.undo(&world) && h.undo(&world), "two units to walk back");
    h.execute(&world, GenerateOperation::new(&world, "OP").expect("regenerate"));
    let generated = generated_hits(&world);
    assert!(!generated.is_empty(), "turning again, the block convicts");
    assert_eq!(generated, fixture_problems(&world));
}
