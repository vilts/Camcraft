//! Fixed steel under a spindle-C live-tool op (issue #486), end to
//! end: a Rotary Contour on a mill-turn's C turns the bar under the
//! fixed radial tool, so on the machine the tool stands on the +X
//! station at every C. A block declared FIXED where the station's
//! shank passes is struck on moves whose part-frame tip is on the far
//! side of the bar - the box laws used to read fixed steel in the part
//! frame there and stayed silent. Generation and the post gate now
//! both convict those moves.
//!
//! Gate map (M4.4): covers app::fixture, sim::fixture, app::operation (rotary_gate's
//! C stamp, the workholding verdict at generation), app::posting (the
//! fixture walk), sim::verify::c_steel.
//! Lane shape (M4.4): all tests here run in seconds.
//! Provenance: our own fixtures (c_axis_live's standing bar); the law
//! is the Prime Directive's "never crash".

use flecs_ecs::prelude::World;

use camcraft_app::commands::{
    AddTool, CreateOperation, GenerateOperation, History, ImportModel, LoadMachine, SetFixtureFixed, SetStock,
    TagFixture,
};
use camcraft_app::geometry::{Mesh, Point3, Triangle};
use camcraft_app::machine::{AxisLimits, Machine};
use camcraft_app::model;
use camcraft_app::operation::{Height, OpParams, SrcRef};
use camcraft_app::posting;
use camcraft_app::schema::Location;
use camcraft_app::stock::StockShape;
use camcraft_app::tools::catalog;
use camcraft_app::verify::Warning;

const TAU: f64 = std::f64::consts::TAU;

fn mill_turn() -> Machine {
    Machine {
        model: "TestBench mill-turn".into(),
        kind: Some("mill-turn".into()),
        axes: vec![
            AxisLimits { name: "X".into(), feed: Some(4000.0), ..Default::default() },
            AxisLimits { name: "Y".into(), feed: Some(4000.0), ..Default::default() },
            AxisLimits { name: "Z".into(), feed: Some(4000.0), ..Default::default() },
            AxisLimits { name: "C".into(), rotary: true, axis: Some([0.0, 0.0, 1.0]), ..Default::default() },
        ],
        ..Default::default()
    }
}

/// The turned bar standing on the spindle (c_axis_live's): wall r6,
/// shoulder, neck r5.
fn bar() -> Vec<Triangle> {
    const MERIDIAN: [(f64, f64); 6] = [(0.0, -16.0), (6.0, -16.0), (6.0, -2.0), (5.0, 2.0), (5.0, 16.0), (0.0, 16.0)];
    let pt = |rho: f64, s: f64, j: usize| {
        let th = TAU * j as f64 / 16.0;
        Point3::new(rho * th.cos(), rho * th.sin(), s)
    };
    let mut f = Vec::new();
    for w in MERIDIAN.windows(2) {
        let ((ra, za), (rb, zb)) = (w[0], w[1]);
        for j in 0..16 {
            let k = (j + 1) % 16;
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

fn world() -> (World, History) {
    let world = camcraft_app::world::new_world();
    let mut h = History::default();
    let tool = catalog().into_iter().find(|(n, _)| *n == "BN-0600").map(|(_, t)| t).expect("catalog ball");
    h.execute(&world, AddTool::new(&world, "BN-0600", tool).expect("tool"));
    let stock = SetStock::new(&world, StockShape::Box { dx: 14.0, dy: 14.0, dz: 34.0 }, Location::default());
    h.execute(&world, stock.expect("stock"));
    h.execute(&world, ImportModel::from_mesh("bar", Mesh::from_facets(bar())));
    h.execute(&world, LoadMachine::new(&world, "millturn.mch".into(), Some(mill_turn())).expect("machine"));
    let params = OpParams::RotaryContour {
        tolerance: 0.05,
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

/// The C (degrees) folded into (-180, 180].
fn wrap(c: f64) -> f64 {
    let w = c.rem_euclid(360.0);
    if w > 180.0 { w - 360.0 } else { w }
}

/// The feed move whose tip stands nearest the far side of the bar (C
/// 180), and the machine-frame point its shank's middle passes: the
/// station ray at the tip's radius plus flute and half the shank.
fn far_side_shank() -> (usize, Point3) {
    let (world, _) = world();
    let path = cache(&world).path;
    let g = catalog().into_iter().find(|(n, _)| *n == "BN-0600").map(|(_, t)| t).expect("ball").geom();
    let (i, m) = path
        .moves
        .iter()
        .enumerate()
        .filter(|(_, m)| m.kind == camcraft_app::cl::MoveKind::Feed && m.rotary.is_some())
        .min_by(|a, b| {
            let d = |m: &camcraft_app::cl::Move| (180.0 - wrap(m.rotary.unwrap_or(0.0)).abs()).abs();
            d(a.1).total_cmp(&d(b.1))
        })
        .expect("the contour cuts under C");
    let c = wrap(m.rotary.unwrap_or(0.0));
    assert!(c.abs() > 170.0, "a cut on the far side of the bar: C {c}");
    let rho = m.to.x.hypot(m.to.y);
    (i, Point3::new(rho + g.flute_len + 0.5 * g.shank_len, 0.0, m.to.z))
}

fn plant_fixed(world: &World, h: &mut History, at: Point3) {
    let e = 1.0;
    let (lo, hi) = (Point3::new(at.x - e, at.y - e, at.z - e), Point3::new(at.x + e, at.y + e, at.z + e));
    let cube = vec![Triangle::new(lo, Point3::new(hi.x, hi.y, hi.z), Point3::new(lo.x, hi.y, hi.z))];
    h.execute(world, ImportModel::from_mesh("steady", Mesh::from_facets(cube)));
    h.execute(world, TagFixture::new(world, "steady", None).expect("tag the steel"));
    h.execute(world, SetFixtureFixed::new(world, "steady", true).expect("declare it fixed"));
}

/// Hits on moves whose tip is more than 120 deg round from the station
/// side - where the part-frame reading of fixed steel at +X is blind.
fn far_side_hits(world: &World) -> Vec<usize> {
    let c = cache(world);
    c.warnings
        .iter()
        .filter_map(|w| match w {
            Warning::FixtureHit { at, .. } => Some(*at),
            _ => None,
        })
        .filter(|&at| c.path.moves.get(at).and_then(|m| m.rotary).is_some_and(|c| wrap(c).abs() > 120.0))
        .collect()
}

#[test]
fn fixed_steel_at_the_station_convicts_cuts_on_the_far_side_of_the_bar() {
    let (k, steady) = far_side_shank();
    let (world, mut h) = world();
    plant_fixed(&world, &mut h, steady);
    h.execute(&world, GenerateOperation::new(&world, "OP").expect("regenerate"));
    let far = far_side_hits(&world);
    assert!(far.contains(&k), "the far-side cut {k} meets the steady at the station: {far:?}");
    let gate = posting::fixture_findings(&world).expect("judged");
    assert!(gate.hits.iter().any(|(_, w)| matches!(w, Warning::FixtureHit { at, .. } if *at == k)), "the post gate convicts it too");
}

#[test]
fn fixed_steel_off_the_station_stays_clear() {
    // The same steady carried 200 mm out along the station: past the
    // whole stack at every C.
    let (_, steady) = far_side_shank();
    let (world, mut h) = world();
    plant_fixed(&world, &mut h, Point3::new(steady.x + 200.0, steady.y, steady.z));
    h.execute(&world, GenerateOperation::new(&world, "OP").expect("regenerate"));
    let hits: Vec<_> = cache(&world).warnings.into_iter().filter(|w| matches!(w, Warning::FixtureHit { .. })).collect();
    assert!(hits.is_empty(), "{hits:?}");
    assert!(posting::fixture_findings(&world).expect("judged").hits.is_empty());
}
