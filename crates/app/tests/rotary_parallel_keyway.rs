//! #2970 review: Rotary Parallel's two halves on a part that is not
//! round. A keyed bar in a coaxial round bar stock: the path that keeps
//! the single frame's cuts in the keyway and adds the turned half's
//! leaves no blocking finding but the keyway corners' rest (gouge within
//! 0.05 mm, reported in the keyway; no rapid in the stock), and its
//! links are the radial relink's.
//!
//! Provenance: our own fixture (a round bar with a keyway in its side).
//! Gate map (M4.4): covers app::operation, sim::verify, motion::multiaxis.

use camcraft_app::cl::MoveKind;
use camcraft_app::commands::{AddTool, CreateOperation, GenerateOperation, History, ImportModel, SetStock};
use camcraft_app::geometry::{quat_from_axis_angle, Mesh, Point3, Triangle};
use camcraft_app::model;
use camcraft_app::operation::{self, Height, OpParams, OpStatus, SrcRef};
use camcraft_app::schema::Location;
use camcraft_app::stock::StockShape;
use camcraft_app::tools::catalog;
use camcraft_app::verify::Warning;
use camcraft_app::world::new_world;

const TAU: f64 = std::f64::consts::TAU;

/// A closed prism along +x from `x0` to `x1` over the cross-section
/// `ring` (y, z), counter-clockwise and star-shaped from the axis.
fn prism(ring: &[(f64, f64)], x0: f64, x1: f64) -> Vec<Triangle> {
    let p = |x: f64, (y, z): (f64, f64)| Point3::new(x, y, z);
    let mut t = Vec::new();
    for k in 0..ring.len() {
        let (a, b) = (ring[k], ring[(k + 1) % ring.len()]);
        t.push(Triangle::new(p(x0, a), p(x0, b), p(x1, b)));
        t.push(Triangle::new(p(x0, a), p(x1, b), p(x1, a)));
        t.push(Triangle::new(p(x0, (0.0, 0.0)), p(x0, b), p(x0, a)));
        t.push(Triangle::new(p(x1, (0.0, 0.0)), p(x1, a), p(x1, b)));
    }
    t
}

fn arc(r: f64, n: usize, from: f64, to: f64) -> Vec<(f64, f64)> {
    (0..=n).map(|j| from + (to - from) * j as f64 / n as f64).map(|th| (r * th.cos(), r * th.sin())).collect()
}

/// A round bar R 10 along x from 0 to 60, with a keyway in its +y side
/// over stations 20..40: walls at z = +-5, floor at y = 6.
fn keyway_bar() -> Vec<Triangle> {
    let a = (5.0f64 / 10.0).asin();
    let round = &arc(10.0, 64, 0.0, TAU)[..64];
    let mut key = arc(10.0, 60, a, TAU - a);
    key.extend([(6.0, -5.0), (6.0, 5.0)]);
    let mut t = prism(round, 0.0, 20.0);
    t.extend(prism(&key, 20.0, 40.0));
    t.extend(prism(round, 40.0, 60.0));
    t
}

#[test]
fn a_keyed_bar_cut_all_the_way_round_leaves_only_the_corner_rest() {
    let world = new_world();
    let mut h = History::default();
    let tool = catalog().into_iter().find(|(n, _)| *n == "BN-0600").map(|(_, t)| t).expect("catalog ball");
    h.execute(&world, AddTool::new(&world, "BN-0600", tool).expect("tool"));
    // A coaxial round bar a hair over the part, laid along x.
    let loc = Location { pos: Point3::new(0.0, 0.0, 0.0), rot: quat_from_axis_angle(1, std::f64::consts::FRAC_PI_2) };
    h.execute(&world, SetStock::new(&world, StockShape::Cylinder { diameter: 20.4, height: 60.0 }, loc).expect("bar stock"));
    h.execute(&world, ImportModel::from_mesh("key", Mesh::from_facets(keyway_bar())));
    let op = OpParams::RotaryParallel {
        tolerance: 0.1,
        stepover: Some(1.5),
        clearance_z: Height::fixed(25.0),
        retract_z: None,
        top_z: Height::fixed(10.2),
        bottom_z: Height::fixed(-10.2),
        feed: 500.0,
        plunge_feed: 100.0,
        climb: true,
        lead_deg: 0.0,
        lag_deg: 0.0,
        max_tilt_deg: 60.0,
        stated_axis: None,
        picks: Default::default(),
    };
    h.execute(&world, CreateOperation::new(&world, "OP", "BN-0600", SrcRef::Entity("key".into()), op).expect("create"));
    h.execute(&world, GenerateOperation::new(&world, "OP").expect("generate"));
    let e = model::find_by_name(&world, "OP").expect("op");
    assert_eq!(operation::status_of(&world, &e), OpStatus::Ok);
    let c = model::toolpath_of(&world, &e).expect("cached");
    // The keyway's inner corners are rest a 6 mm ball cannot reach: the
    // envelope verdict reports them (rest about 2 mm), on the single-frame
    // path as on this one (measured: gouge 0.020 / rest 1.99 there, 0.020
    // / 1.95 here). That one verdict may stand: its gouge within 0.05 mm,
    // reported at a move over the keyway - stations 20..40 widened by the
    // ball, on the keyway's +y side. The verdict names the move nearest
    // the deviation in plan (XY) only, so its height is not read (here:
    // a plunge at the keyway's end, station 40). Nothing else may block -
    // no gouge past it, no rapid in the stock, no shank or holder hit.
    let r = 3.0;
    let centre = |m: &camcraft_app::cl::Move| {
        let a = m.axis.unwrap_or([0.0, 0.0, 1.0]);
        Point3::new(m.to.x + r * a[0], m.to.y + r * a[1], m.to.z + r * a[2])
    };
    let in_keyway = |at: usize| {
        c.path.moves.get(at).map(centre).is_some_and(|p| (20.0 - r..=40.0 + r).contains(&p.x) && p.y > 0.0)
    };
    let blocking: Vec<_> = c
        .warnings
        .iter()
        .filter(|w| w.blocks_post())
        .filter(|w| !matches!(w, Warning::Envelope { at, gouge_mm, .. } if *gouge_mm <= 0.05 && in_keyway(*at)))
        .collect();
    assert!(blocking.is_empty(), "the keyed bar leaves no blocking finding but the keyway corner rest: {blocking:?}");
    // The passes go round: cuts stand over and under the axis plane.
    let centre_z = |m: &camcraft_app::cl::Move| m.to.z + r * m.axis.map_or(1.0, |a| a[2]);
    let feeds: Vec<_> = c.path.moves.iter().filter(|m| m.kind == MoveKind::Feed).collect();
    assert!(feeds.iter().any(|m| centre_z(m) > 5.0) && feeds.iter().any(|m| centre_z(m) < -5.0), "both halves cut");
    // Links: every rapid that ends inside the clearance cylinder (25)
    // is none - the relink retracts along the radius onto it first.
    for m in c.path.moves.iter().filter(|m| m.kind == MoveKind::Rapid) {
        let rho = (m.to.y * m.to.y + m.to.z * m.to.z).sqrt();
        assert!(rho >= 25.0 - 1e-6, "a rapid ends inside the clearance radius: {m:?}");
    }
    assert!(c.path.notes.iter().any(|n| n.starts_with("Rotary Parallel: the passes go all the way round")), "{:?}", c.path.notes);
}
