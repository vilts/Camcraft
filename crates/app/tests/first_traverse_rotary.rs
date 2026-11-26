//! #2965: the first traverse of the rotary, multi-axis and cross-drill
//! kinds. After a tool change the post sends Z, then X/Y at that
//! height, then the rotary words: the traverse comes from an unknown
//! X/Y with the rotaries at a pose the operation does not state.
//!
//! - A rotary kind reads its work swept about the fitted axis: a square
//!   bar laid on X clears its upright top at a clearance of 8.5, but
//!   its edges turn up to 7 * sqrt(2) = 9.899, so the post is held by
//!   name. The same program at 12 has no first-traverse verdict.
//! - A 5-axis kind reads its tip in the part frame: a far clamp over
//!   the clearance plane holds the post; the same clamp under it does
//!   not add the verdict.
//! - A cross drill on a mill is refused by name before any traverse
//!   (radial holes are live-tooling work).
//!
//! Provenance: our own fixtures (the rotary suite's turned bar, the
//! multi-axis suite's dome, the cross-drill suite's punched bar); the
//! shop rule "never traverse through uncut stock or steel"
//! (machine-tool handbooks). No vendor material.
//!
//! Gate map (#2965): covers app::operation (first_traverse, generate),
//! app::posting, sim::verify (first_traverse).

mod common;
mod punched_bar;

use camcraft_app::commands::{AddTool, CreateOperation, GenerateOperation, History, ImportModel, SetStock, TagFixture};
use camcraft_app::drilling::{CrossHole, CycleChoice};
use camcraft_app::geometry::{cube, Mesh, Point3, Triangle, IDENTITY_Q};
use camcraft_app::operation::{Height, OpParams, OpStatus, SrcRef};
use camcraft_app::schema::Location;
use camcraft_app::stock::StockShape;
use camcraft_app::tools::catalog;
use camcraft_app::verify::Warning;
use camcraft_app::{model, posting};
use flecs_ecs::prelude::World;

const BALL: &str = "BN-0600";

fn tool(world: &World, h: &mut History, name: &str) {
    let t = catalog().into_iter().find(|(n, _)| *n == name).map(|(_, t)| t).expect("catalog");
    h.execute(world, AddTool::new(world, name, t).expect("tool"));
}

/// Generate "OP" from `src` and read its status and verdicts.
fn generate(world: &World, h: &mut History, tool: &str, src: &str, p: OpParams) -> (OpStatus, Vec<Warning>) {
    h.execute(world, CreateOperation::new(world, "OP", tool, SrcRef::Entity(src.into()), p).expect("create"));
    h.execute(world, GenerateOperation::new(world, "OP").expect("generate"));
    let e = model::find_by_name(world, "OP").expect("op");
    let cached = model::toolpath_of(world, &e).expect("cached path");
    (camcraft_app::operation::status_of(world, &e), cached.warnings.clone())
}

fn first_low(warns: &[Warning]) -> Option<(usize, f64, f64, bool)> {
    warns.iter().find_map(|w| match *w {
        Warning::FirstTraverseLow { at, height_mm, top_mm, fixture, .. } => Some((at, height_mm, top_mm, fixture)),
        _ => None,
    })
}

fn held_by_name(world: &World, top: &str) {
    let err = posting::post_setup_unwritten(world).expect_err("the program must not post");
    for text in ["OP", "the first rapid (move 0)", top] {
        assert!(err.contains(text), "{text}: {err}");
    }
}

fn rotary_parallel(clearance: f64) -> OpParams {
    OpParams::RotaryParallel {
        tolerance: 0.1,
        stepover: Some(2.0),
        clearance_z: Height::fixed(clearance),
        retract_z: None,
        top_z: Height::fixed(6.0),
        bottom_z: Height::fixed(1.0),
        feed: 500.0,
        plunge_feed: 100.0,
        climb: true,
        lead_deg: 0.0,
        lag_deg: 0.0,
        max_tilt_deg: 60.0,
        stated_axis: None,
        picks: Default::default(),
    }
}

/// The rotary suite's turned bar laid along X (axis through the origin)
/// in a 14 x 14 square bar of stock: upright top Z7, swept top 9.899.
fn laid_square_bar(clearance: f64) -> (World, OpStatus, Vec<Warning>) {
    let world = camcraft_app::world::new_world();
    let mut h = History::default();
    tool(&world, &mut h, BALL);
    let at = Location { pos: Point3::new(0.0, 0.0, -7.0), rot: IDENTITY_Q };
    h.execute(&world, SetStock::new(&world, StockShape::Box { dx: 32.0, dy: 14.0, dz: 14.0 }, at).expect("stock"));
    h.execute(&world, ImportModel::from_mesh("bar", Mesh::from_facets(common::rotary_bar::laid())));
    let (status, warns) = generate(&world, &mut h, BALL, "bar", rotary_parallel(clearance));
    (world, status, warns)
}

#[test]
fn a_rotary_kind_reads_its_square_bar_swept_about_the_axis() {
    let swept = 7.0 * 2f64.sqrt();
    let (world, status, warns) = laid_square_bar(8.5);
    assert_eq!(status, OpStatus::Ok);
    let (at, height, top, fixture) = first_low(&warns).unwrap_or_else(|| panic!("clearance 8.5: no first-traverse verdict in {warns:?}"));
    assert_eq!((at, fixture), (0, false));
    assert!((top - swept).abs() < 1e-6, "the bar's edge turned up: {top}, want {swept}");
    assert!(height < top && height > 7.0, "the start clears the upright top only: {height}");
    held_by_name(&world, "under the stock top at Z9.899");

    let (_, status, warns) = laid_square_bar(12.0);
    assert_eq!(status, OpStatus::Ok);
    assert_eq!(first_low(&warns), None, "clearance 12 stands over the swept bar: {warns:?}");
}

/// A spherical cap (radius 12, to 60 degrees off the pole) on z = 0.
fn dome() -> Mesh {
    const TAU: f64 = std::f64::consts::TAU;
    let (radius, polar_max, n_lat, n_lon) = (12.0, std::f64::consts::FRAC_PI_3, 6usize, 24usize);
    let ring = |k: usize| -> Vec<Point3> {
        let ph = polar_max * k as f64 / n_lat as f64;
        let (z, rr) = (radius * ph.cos(), radius * ph.sin());
        (0..n_lon).map(|j| Point3::new(rr * (TAU * j as f64 / n_lon as f64).cos(), rr * (TAU * j as f64 / n_lon as f64).sin(), z)).collect()
    };
    let apex = Point3::new(0.0, 0.0, radius);
    let first = ring(1);
    let mut f: Vec<Triangle> = (0..n_lon).map(|j| Triangle::new(apex, first[j], first[(j + 1) % n_lon])).collect();
    for k in 1..n_lat {
        let (a, b) = (ring(k), ring(k + 1));
        for j in 0..n_lon {
            let n = (j + 1) % n_lon;
            f.push(Triangle::new(a[j], b[j], b[n]));
            f.push(Triangle::new(a[j], b[n], a[n]));
        }
    }
    Mesh::from_facets(f)
}

const DOME_CLEAR: f64 = 25.0;

/// A 5-Axis Contour over the dome in a 40 x 40 x 12 block, a clamp of
/// `top` mm far off the block and off every move.
fn contour5d_beside_a_clamp(top: f64) -> (World, OpStatus, Vec<Warning>) {
    let world = camcraft_app::world::new_world();
    let mut h = History::default();
    tool(&world, &mut h, BALL);
    let at = Location { pos: Point3::ZERO, rot: IDENTITY_Q };
    h.execute(&world, SetStock::new(&world, StockShape::Box { dx: 40.0, dy: 40.0, dz: 12.0 }, at).expect("stock"));
    h.execute(&world, ImportModel::from_mesh("dome", dome()));
    let clamp = model::spawn(&world, "Clamp1");
    model::set_facets(&world, &clamp, cube(Point3::new(150.0, 150.0, 0.0), Point3::new(160.0, 160.0, top)).facets.to_vec().into());
    model::set_rev(&world, &clamp, 1);
    h.execute(&world, TagFixture::new(&world, "Clamp1", None).expect("tagged"));
    let p = OpParams::Contour5D {
        tolerance: 0.1,
        clearance_z: Height::fixed(DOME_CLEAR),
        retract_z: None,
        top_z: Height::fixed(11.0),
        bottom_z: Height::fixed(7.0),
        feed: 500.0,
        plunge_feed: 100.0,
        climb: true,
        lead_deg: 15.0,
        lag_deg: 0.0,
        max_tilt_deg: 60.0,
        shank_avoid: false,
        picks: Default::default(),
    };
    let (status, warns) = generate(&world, &mut h, BALL, "dome", p);
    (world, status, warns)
}

#[test]
fn a_5_axis_kind_reads_a_far_clamp_over_its_clearance_plane() {
    let (world, status, warns) = contour5d_beside_a_clamp(40.0);
    assert_eq!(status, OpStatus::Ok);
    assert_eq!(first_low(&warns), Some((0, DOME_CLEAR, 40.0, true)), "{warns:?}");
    held_by_name(&world, "under the fixture top at Z40.000");

    let (_, _, warns) = contour5d_beside_a_clamp(20.0);
    assert_eq!(first_low(&warns), None, "a clamp under the plane: {warns:?}");
}

/// A cross drill on a mill never reaches a first traverse: radial holes
/// are live-tooling work, and generation refuses them by name. The
/// lathe and mill-turn cross drill is outside the mill law (decision
/// #2965).
#[test]
fn a_cross_drill_on_a_mill_is_refused_before_any_traverse() {
    let world = camcraft_app::world::new_world();
    let mut h = History::default();
    tool(&world, &mut h, "DR-0400");
    let at = Location { pos: Point3::new(0.0, 0.0, punched_bar::FOOT), rot: IDENTITY_Q };
    h.execute(&world, SetStock::new(&world, StockShape::Cylinder { diameter: 40.0, height: 100.0 }, at).expect("stock"));
    // One r2 hole plunging -X from the bar's face at z 0, 6 mm deep.
    let hole = || CrossHole { origin: Point3::new(10.0, 0.0, 0.0), dir: Point3::new(-1.0, 0.0, 0.0), radius: 2.0, entry_d: 0.0, floor_d: -6.0, through: false };
    h.execute(&world, ImportModel::from_mesh("B", punched_bar::punched_bar(10.0, punched_bar::FOOT, punched_bar::TOP, &[hole()])));
    let p = OpParams::Drill {
        break_through: None,
        break_through_from: Default::default(),
        clearance_z: Height::fixed(25.0),
        retract_z: None,
        top_z: Height::fixed(20.0),
        bottom_z: None,
        cycle: CycleChoice::Auto,
        peck: 0.0,
        dwell: 0.0,
        feed: 120.0,
        plunge_feed: 80.0,
        holes: vec![],
        cross: vec![hole()],
        picks: Default::default(),
    };
    h.execute(&world, CreateOperation::new(&world, "OP", "DR-0400", SrcRef::Entity("B".into()), p).expect("create"));
    let err = GenerateOperation::new(&world, "OP").err().map(|e| format!("{e:?}")).unwrap_or_default();
    assert!(err.contains("OP") && err.contains("live-tooling work"), "refused by name: {err}");
    assert!(posting::post_setup_unwritten(&world).is_err(), "no program posts");
}
