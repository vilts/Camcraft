//! M11.3 owed (RECOMMENDED "M11.3: the centre-cutting law along the
//! posed axis" Owed (2)), through the application: the law reaches the
//! multiaxis kinds end to end through `operation::generate`. A 5-axis
//! finish (Contour5D over a dome), Multi-Axis Rough (the stamped pocket
//! ladder) and Rotary Offset (the wrapped ladder) are each judged on
//! their FINAL posed path for a tool that is not centre-cutting -
//! ramped or refused by name - while the centre-cutting twin's path is
//! the generator's own.
//!
//! Provenance: the centre-cutting / non-centre-cutting end mill
//! distinction of machining handbooks; our own fixtures (the dome, the
//! operations suite's slab-and-boss, the rotary suite's turned bar).
//!
//! Gate map (M11.3): covers app::operation (generate, plunge_solid
//! audit_posed / guard_posed / audit_bar), motion::multiaxis (contour5d,
//! rough, rotary_offset, rotary_bar, rotary twins under the law),
//! motion::plunge_law (Bar, first_bar_plunge).

use flecs_ecs::prelude::World;

use camcraft_app::cl::Toolpath;
use camcraft_app::commands::{AddTool, History, ImportModel, SetStock};
use camcraft_app::geometry::{cube, Mesh, Point3, Triangle, IDENTITY_Q};
use camcraft_app::machining2d::GenError;
use camcraft_app::operation::{self, Height, OpParams, SrcRef};
use camcraft_app::schema::{Location, Operation};
use camcraft_app::stock::StockShape;
use camcraft_app::tools::{catalog, Tool};
use camcraft_app::world::new_world;
use camcraft_motion::approach::{CENTER_CUT_REFUSAL, NOT_CENTER_CUTTING};
use camcraft_motion::plunge_law::{first_bar_plunge, first_solid_plunge, Bar, Solid};

const BALL: &str = "BN-0600";
const R: f64 = 3.0;

/// The world: `stock`, the model `mesh` as "part", and the catalog ball
/// twice - as itself, and as "NC", the same body not centre-cutting.
fn world_with(stock: StockShape, at: Point3, mesh: Mesh) -> World {
    let world = new_world();
    let mut h = History::default();
    let s = SetStock::new(&world, stock, Location { pos: at, rot: IDENTITY_Q });
    h.execute(&world, s.expect("stock"));
    let t = catalog().into_iter().find(|(n, _)| *n == BALL).map(|(_, t)| t).expect("catalog ball");
    h.execute(&world, AddTool::new(&world, BALL, t.clone()).expect("tool"));
    h.execute(&world, AddTool::new(&world, "NC", Tool { center_cutting: false, ..t }).expect("tool"));
    h.execute(&world, ImportModel::from_mesh("part", mesh));
    world
}

fn op(tool: &str, params: OpParams) -> Operation {
    Operation {
        tool: tool.into(),
        src: SrcRef::Entity("part".into()),
        params,
        stamp: None,
        setup: None,
        area: Default::default(),
        coolant: Default::default(),
        after: Default::default(),
        suppressed: false,
    }
}

/// The centre-cutting and the non-centre-cutting verdicts of one op.
fn both(world: &World, params: OpParams) -> (Toolpath, Result<Toolpath, GenError>) {
    let (cc, _) = operation::generate(world, &op(BALL, params.clone()), "").expect("the centre-cutting tool generates");
    let nc = operation::generate(world, &op("NC", params), "").map(|(t, _)| t);
    (cc, nc)
}

/// The law's verdict on a non-centre-cutting run, as the suite reads
/// it: either refused BY NAME (the law's prefix and the kind), or a
/// path with no plunge into `solid` that ramps where the centre-cutting
/// path plunged. Returns whether it refused.
fn ramped_or_refused(kind: &str, cc: &Toolpath, nc: &Result<Toolpath, GenError>, solid: &Solid) -> bool {
    match nc {
        Err(e) => {
            let m = format!("{e:?}");
            assert!(m.contains(CENTER_CUT_REFUSAL), "{kind}: refused, but not by the law: {m}");
            assert!(m.contains(kind), "{kind}: the refusal names the kind: {m}");
            true
        }
        Ok(t) => {
            assert_eq!(first_solid_plunge(t, R, solid), None, "{kind}: a plunge into solid passed: {:?}", t.notes);
            if t.moves != cc.moves {
                assert!(t.notes.iter().any(|n| n.starts_with(NOT_CENTER_CUTTING)), "{kind}: {:?}", t.notes);
            }
            false
        }
    }
}

/// A spherical cap (radius 12, to 60 degrees off the pole) on z = 0.
fn dome() -> Mesh {
    const TAU: f64 = std::f64::consts::TAU;
    let (radius, polar_max, n_lat, n_lon) = (12.0, std::f64::consts::FRAC_PI_3, 6usize, 24usize);
    let ring = |k: usize| -> Vec<Point3> {
        let ph = polar_max * k as f64 / n_lat as f64;
        let (z, rr) = (radius * ph.cos(), radius * ph.sin());
        (0..n_lon).map(|j| {
            let th = TAU * j as f64 / n_lon as f64;
            Point3::new(rr * th.cos(), rr * th.sin(), z)
        }).collect()
    };
    let apex = Point3::new(0.0, 0.0, radius);
    let first = ring(1);
    let mut f = Vec::new();
    for j in 0..n_lon {
        f.push(Triangle::new(apex, first[j], first[(j + 1) % n_lon]));
    }
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

#[test]
fn contour5d_over_a_dome_ramps_or_refuses_a_tool_that_is_not_centre_cutting() {
    // A 40 x 40 block from z 0 to the apex: every ring entry lands in it.
    let world = world_with(StockShape::Box { dx: 40.0, dy: 40.0, dz: 12.0 }, Point3::ZERO, dome());
    let params = OpParams::Contour5D {
        tolerance: 0.1,
        clearance_z: Height::fixed(25.0),
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
    };
    let (cc, nc) = both(&world, params.clone());
    let (again, _) = operation::generate(&world, &op(BALL, params), "").expect("again");
    assert_eq!(cc, again, "the centre-cutting path is the generator's own, run for run");
    assert!(cc.notes.iter().all(|n| !n.starts_with(NOT_CENTER_CUTTING)), "{:?}", cc.notes);
    let solid = Solid { top: 12.0, footprint: None };
    assert!(first_solid_plunge(&cc, R, &solid).is_some(), "the fixture's ring entries plunge into the block");
    ramped_or_refused("5-Axis Contour", &cc, &nc, &solid);
}

/// The operations suite's slab with a boss: a 40 x 40 slab to z -10, a
/// 10 x 10 boss to z 0, under a 40 x 40 x 20 box to z 0.
fn slab_boss() -> Mesh {
    let mut f = cube(Point3::new(-20.0, -20.0, -20.0), Point3::new(20.0, 20.0, -10.0)).facets.to_vec();
    f.extend(cube(Point3::new(-5.0, -5.0, -11.0), Point3::new(5.0, 5.0, 0.0)).facets.iter().cloned());
    Mesh::from_facets(f)
}

#[test]
fn multiaxis_rough_is_judged_on_its_posed_path() {
    let world = world_with(StockShape::Box { dx: 40.0, dy: 40.0, dz: 20.0 }, Point3::new(0.0, 0.0, -20.0), slab_boss());
    let params = OpParams::MultiAxisRough {
        links: camcraft_app::links::Links::ROUGH3D,
        tolerance: 0.1,
        stepover: 3.0,
        stock_to_leave: 0.0,
        clearance_z: Height::fixed(5.0),
        retract_z: None,
        top_z: Height::fixed(0.0),
        bottom_z: Height::fixed(-10.0),
        stepdown: 5.0,
        feed: 800.0,
        plunge_feed: 200.0,
        climb: true,
        strategy: camcraft_app::machining2d::PocketStrategy::Zigzag,
        entry: camcraft_app::machining3d::Entry3D::Plunge,
        helix_radius: 1.0,
        optimal_load: camcraft_app::load::DEFAULT_OPTIMAL_LOAD,
        lead_deg: 0.0,
        lag_deg: 0.0,
        max_tilt_deg: 90.0,
        shank_avoid: false,
    };
    let (cc, nc) = both(&world, params);
    let solid = Solid { top: 0.0, footprint: None };
    // The pocket's own entry guard passed this ladder in pocket space;
    // the stamp then posed a row's run to the boss wall along the wall
    // normal, so the tool feeds within the core's cone of its own axis
    // into an uncleared column - the post-stamp audit refuses it.
    assert!(ramped_or_refused("Multi-Axis Rough", &cc, &nc, &solid), "the posed path is refused");
    let m = format!("{:?}", nc.as_ref().err());
    assert!(m.contains("Multi-Axis Rough's entry descent"), "refused by the post-stamp audit: {m}");
}

/// The rotary suite's turned bar standing on world Z: wall r6, neck r5.
fn turned_bar() -> Mesh {
    const MERIDIAN: [(f64, f64); 6] = [(0.0, -16.0), (6.0, -16.0), (6.0, -2.0), (5.0, 2.0), (5.0, 16.0), (0.0, 16.0)];
    let n = 16usize;
    let pt = |rho: f64, z: f64, j: usize| {
        let th = std::f64::consts::TAU * j as f64 / n as f64;
        Point3::new(rho * th.cos(), rho * th.sin(), z)
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
    Mesh::from_facets(f)
}

/// An oversized 20 mm bar over the 12 mm part, and the rough over it:
/// it peels from the bar's radius, every infeed is into the bar.
fn rotary_offset_on_the_bar() -> (World, OpParams) {
    let world = world_with(
        StockShape::Cylinder { diameter: 20.0, height: 32.0 },
        Point3::new(0.0, 0.0, -16.0),
        turned_bar(),
    );
    let params = OpParams::RotaryOffset {
        links: camcraft_app::links::Links::ROUGH3D,
        tolerance: 0.1,
        stepover: 4.0,
        stock_to_leave: 0.5,
        clearance_z: Height::fixed(20.0),
        top_z: Height::fixed(14.0),
        bottom_z: Height::fixed(-14.0),
        stepdown: 0.5,
        feed: 500.0,
        plunge_feed: 100.0,
        climb: true,
        strategy: camcraft_app::machining2d::PocketStrategy::Auto,
        entry: camcraft_app::machining3d::Entry3D::Plunge,
        helix_radius: 1.0,
        optimal_load: camcraft_app::load::DEFAULT_OPTIMAL_LOAD,
        lead_deg: 0.0,
        lag_deg: 0.0,
        max_tilt_deg: 60.0,
    };
    (world, params)
}

#[test]
fn rotary_offset_is_judged_on_its_wrapped_path() {
    let (world, params) = rotary_offset_on_the_bar();
    let (cc, nc) = both(&world, params);
    // The audit's coarse reading of the bar: everything below its top
    // over its outline - here, with no outline, everything below z 16.
    let solid = Solid { top: 16.0, footprint: None };
    assert!(first_solid_plunge(&cc, R, &solid).is_some(), "the wrapped infeeds plunge radially into the bar");
    // The wrap-space entry guard already ramps the infeeds; the final
    // posed path then passes the audit (and the suite's stricter
    // no-outline reading of the bar) - ramped, not plunged.
    assert!(!ramped_or_refused("Rotary Offset", &cc, &nc, &solid), "ramped, not refused");
    assert!(nc.as_ref().is_ok_and(|t| t.moves != cc.moves), "the infeeds ramp for the tool that is not centre-cutting");
}

/// #16: Rotary Offset's audit reads the TRUE bar - the stock cylinder
/// about the part's axis - not the box under its top: an infeed into
/// the bar convicts (and is ramped), the same descent out in the air
/// past its radius is clean where the box reading convicted it.
#[test]
fn rotary_offset_is_judged_against_the_true_cylinder() {
    let (world, params) = rotary_offset_on_the_bar();
    let (cc, nc) = both(&world, params);
    let up = Point3::new(0.0, 0.0, 1.0);
    let bar = Bar { origin: Point3::ZERO, dir: up, radius: 10.0, lo: -16.0, hi: 16.0 };
    let (xy, _) = first_bar_plunge(&cc, R, &bar).expect("the radial infeeds plunge into the cylinder");
    assert!(xy.x.hypot(xy.y) < 10.0, "the convicted tip is inside the bar: {xy:?}");
    let nc = nc.expect("ramped, not refused");
    assert_eq!(first_bar_plunge(&nc, R, &bar), None, "no infeed of the ramped path plunges into the bar");
    // The convicted infeed, lifted radially until its tip stands 1 mm
    // outside the bar: air on the cylinder, solid on the box reading.
    let i = cc.moves.iter().position(|m| first_bar_plunge(&Toolpath { moves: vec![*m], ..cc.clone() }, R, &bar).is_some());
    let mut m = cc.moves[i.expect("the infeed move")];
    let rho = m.to.x.hypot(m.to.y);
    let k = 11.0 / rho;
    let lift = |p: Point3| Point3::new(p.x * k, p.y * k, p.z);
    (m.from, m.to) = (lift(m.from), lift(m.to));
    let air = Toolpath { moves: vec![m], ..cc.clone() };
    assert_eq!(first_bar_plunge(&air, R, &bar), None, "a descent in the air outside the radius is clean");
    assert!(first_solid_plunge(&air, R, &Solid { top: 16.0, footprint: None }).is_some(), "the box reading convicted it");
}

/// The rotary twins under the forced sloped-entry retry
/// (`center_cut_guard_as` re-runs with `ramp` and `entry_ramp` on):
/// does the retry change the posed path at all, and does the retried
/// path still plunge into the bar? Rotary Contour's rings honour
/// `entry_ramp`; since #15 Rotary Parallel's radial relink ramps its
/// opening and link infeeds along the pass from the retract cylinder
/// wherever the ramp is proved clear of the part (the raster's upright
/// diagonal hops stand down: posed radially they ran down the axis) -
/// so the retried path no longer plunges into the bar.
#[test]
fn the_rotary_twins_under_the_forced_ramp_retry() {
    use camcraft_app::finishing3d::{Finish3DParams, FinishPattern};
    use camcraft_app::multiaxis::{rotary_contour, rotary_parallel, AxisControl};
    let axis = AxisControl { lead_deg: 0.0, lag_deg: 0.0, max_tilt_deg: Some(60.0) };
    let base = Finish3DParams {
        pattern: FinishPattern::Waterline,
        stepover: Some(2.0),
        tool_diameter: 2.0 * R,
        tolerance: 0.1,
        clearance_z: 20.0,
        retract_z: None,
        top_z: 14.0,
        bottom_z: -14.0,
        feed: 500.0,
        plunge_feed: 100.0,
        climb: true,
        ramp: false,
        min_step: None,
        ramp_angle: 45.0,
        entry_ramp: false,
        steep_stepover: None,
        pencil_reference: None,
    };
    let sloped = Finish3DParams { ramp: true, entry_ramp: true, ..base };
    let standing = turned_bar().facets.to_vec();
    let s = |p: Point3| Point3::new(p.z, p.x, p.y);
    let laid: Vec<Triangle> = standing.iter().map(|t| Triangle::new(s(t.a), s(t.b), s(t.c))).collect();
    // The oversized bar, read the audit's coarse way: below its top.
    let (c_plain, c_slope) = (rotary_contour(&standing, &base, &axis).expect("rc"), rotary_contour(&standing, &sloped, &axis).expect("rc sloped"));
    let lay = Finish3DParams { top_z: 6.0, bottom_z: 1.0, ..base };
    let lay_s = Finish3DParams { top_z: 6.0, bottom_z: 1.0, ..sloped };
    let (p_plain, p_slope) = (rotary_parallel(&laid, &lay, &axis).expect("rp"), rotary_parallel(&laid, &lay_s, &axis).expect("rp sloped"));
    assert_ne!(c_plain.moves, c_slope.moves, "Rotary Contour honours the retry");
    assert_ne!(p_plain.moves, p_slope.moves, "Rotary Parallel's infeeds ramp under the retry");
    let bar = Solid { top: 10.0, footprint: None };
    let plunge = first_solid_plunge(&p_plain, R, &bar);
    assert!(plunge.is_some(), "the laid bar's radial infeed plunges into the 20 mm bar");
    // #15: no infeed plunges any more. What the coarse no-footprint
    // solid still reads (everything under its top, even past the bar's
    // end) is a cut - the end-cap ring beyond x = -16, in air on the
    // real stock - never the radial infeed it read before.
    let left = first_solid_plunge(&p_slope, R, &bar);
    assert_ne!(left, plunge, "#15: the radial infeed ramps under the retry");
    assert!(left.is_none_or(|(xy, _)| xy.x.abs() > 16.0), "#15: an infeed onto the bar still plunges: {left:?}");
    assert!(
        p_slope.notes.iter().any(|n| n.starts_with("Rotary Parallel:") && n.contains("ramp in along the pass")),
        "the ramped infeeds are stated: {:?}",
        p_slope.notes
    );
}

/// #15: Swarf's first raster entry under the forced sloped-entry retry.
/// The raster's first drop onto its first line plunges into the bar
/// (read with the stock topping out at the Top of Part Z); under the
/// retry it becomes an air drop to that top behind the line's start
/// and a ramp down the line's tangent - proved clear of the part - so
/// that entry no longer plunges, and the note says which way it went.
#[test]
fn swarf_first_entry_ramps_under_the_forced_retry() {
    use camcraft_app::finishing3d::{Finish3DParams, FinishPattern};
    use camcraft_app::multiaxis::{swarf5d, AxisControl};
    let axis = AxisControl { lead_deg: 0.0, lag_deg: 0.0, max_tilt_deg: Some(60.0) };
    let base = Finish3DParams {
        pattern: FinishPattern::Parallel,
        stepover: Some(2.0),
        tool_diameter: 2.0 * R,
        tolerance: 0.1,
        clearance_z: 20.0,
        retract_z: None,
        top_z: 6.0,
        bottom_z: 1.0,
        feed: 500.0,
        plunge_feed: 100.0,
        climb: true,
        ramp: false,
        min_step: None,
        ramp_angle: 45.0,
        entry_ramp: false,
        steep_stepover: None,
        pencil_reference: None,
    };
    let sloped = Finish3DParams { ramp: true, entry_ramp: true, ..base };
    let s = |p: Point3| Point3::new(p.z, p.x, p.y);
    let laid: Vec<Triangle> = turned_bar().facets.iter().map(|t| Triangle::new(s(t.a), s(t.b), s(t.c))).collect();
    let plain = swarf5d(&laid, &base, &axis).expect("swarf");
    let slope = swarf5d(&laid, &sloped, &axis).expect("swarf sloped");
    let bar = Solid { top: 6.0, footprint: None };
    let first = first_solid_plunge(&plain, R, &bar);
    assert!(first.is_some(), "the plain raster's first entry drops into the bar");
    assert!(
        slope.notes.iter().any(|n| n.starts_with("swarf: the first raster entry ramps in")),
        "the lead-in is stated: {:?}",
        slope.notes
    );
    assert_ne!(first_solid_plunge(&slope, R, &bar), first, "the first entry no longer plunges where it did");
}
