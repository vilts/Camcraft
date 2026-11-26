//! M11.3 owed (RECOMMENDED "M11.3: the centre-cutting law in finish,
//! drilling and face"): the law reaches the multiaxis kinds. A posed
//! descent is judged along the tool's OWN axis - a drop down a tilted
//! axis, or a world-vertical drop under a lean inside the core's cone -
//! and a tool that is not centre-cutting is ramped or refused by name;
//! a centre-cutting tool's path is untouched; a descent in air passes.
//!
//! Provenance: the centre-cutting / non-centre-cutting end mill
//! distinction of machining handbooks; the posed reading is this
//! crate's own geometry (`plunge_law::posed_axial`, `cleared_along`).
//!
//! Gate map (M11.3): covers camcraft_motion::plunge_law (posed_axial,
//! cleared_along, first_solid_plunge, audit), camcraft_motion::finishing3d
//! (center_cut guard_as), camcraft_motion::multiaxis (contour5d,
//! parallel5d under the law); the unposed cone of M11.3 owed (1) - a
//! steep 3-axis descent is a plunge too, inside the same 30 deg cone.

use camcraft_motion::approach::CENTER_CUT_REFUSAL;
use camcraft_motion::cl::{Move, MoveKind, PathBuilder, Toolpath};
use camcraft_motion::finishing3d::{center_cut_guard_as, Finish3DParams, FinishPattern};
use camcraft_motion::geo2d::{Loop, Point2, Region};
use camcraft_motion::geometry::{Point3, Triangle};
use camcraft_motion::machining2d::GenError;
use camcraft_motion::multiaxis::{contour5d, parallel5d, AxisControl};
use camcraft_motion::plunge_law::{audit, first_solid_plunge, posed_axial, Solid};

fn rect(x0: f64, y0: f64, x1: f64, y1: f64) -> Region {
    let l = Loop::new(vec![Point2::new(x0, y0), Point2::new(x1, y0), Point2::new(x1, y1), Point2::new(x0, y1)]);
    Region::new(vec![l.expect("loop")]).expect("region")
}

fn refused(what: &str, e: &GenError) {
    assert!(format!("{e:?}").contains(CENTER_CUT_REFUSAL), "{what}: refused, but not by the law: {e:?}");
}

/// A unit axis leaning `deg` off vertical toward +X.
fn lean(deg: f64) -> [f64; 3] {
    let t = deg.to_radians();
    [t.sin(), 0.0, t.cos()]
}

/// A posed path: rapid to `start` over the stock, then one feed down
/// the posed axis by `depth`, holding `axis`.
fn posed_drop(start: Point3, axis: [f64; 3], depth: f64) -> Toolpath {
    let mut b = PathBuilder::new(Point3::new(start.x, start.y, 30.0));
    b.set_axis(Some(axis));
    b.push(MoveKind::Rapid, start, None);
    let end = Point3::new(start.x - axis[0] * depth, start.y - axis[1] * depth, start.z - axis[2] * depth);
    b.push(MoveKind::Plunge, end, Some(100.0));
    b.finish()
}

#[test]
fn a_drop_down_a_tilted_axis_into_solid_is_refused_for_a_tool_that_is_not_centre_cutting() {
    let stock = rect(-30.0, -30.0, 30.0, 30.0);
    for solid in [Solid { top: 10.0, footprint: Some(&stock) }, Solid { top: 10.0, footprint: None }] {
        for deg in [0.0, 20.0, 40.0, 75.0] {
            // 10 mm down the axis from Z 11: below the top at every lean.
            let t = posed_drop(Point3::new(0.0, 0.0, 11.0), lean(deg), 10.0);
            assert!(first_solid_plunge(&t, 3.0, &solid).is_some(), "{deg} deg: an axial drop into solid");
            refused(&format!("{deg} deg"), &audit(&t, false, 3.0, &solid, "the test drop").expect_err("refused"));
            // The centre-cutting tool passes the same path.
            assert_eq!(audit(&t, true, 3.0, &solid, "the test drop"), Ok(()));
        }
    }
}

#[test]
fn a_world_vertical_drop_under_a_lean_inside_the_core_cone_is_a_plunge() {
    // The 5-axis stamp drops the BALL straight down under a held lean:
    // inside asin(CORE) = 30 deg the leading point is the uncut core.
    let a = |deg: f64| {
        let l = lean(deg);
        Point3::new(l[0], l[1], l[2])
    };
    let drop = Move::new(MoveKind::Plunge, Point3::new(0.0, 0.0, 12.0), Point3::new(0.0, 0.0, 6.0), Some(100.0));
    assert!(posed_axial(&drop, a(0.0)));
    assert!(posed_axial(&drop, a(25.0)));
    assert!(!posed_axial(&drop, a(35.0)), "a wider lean is a ramp");
    let up = Move::new(MoveKind::Feed, Point3::new(0.0, 0.0, 6.0), Point3::new(0.0, 0.0, 12.0), Some(100.0));
    assert!(!posed_axial(&up, a(10.0)), "a retract never plunges");
    // Under a lean of 20 deg the vertical drop into solid is convicted.
    let mut b = PathBuilder::new(Point3::new(0.0, 0.0, 30.0));
    b.set_axis(Some(lean(20.0)));
    b.push(MoveKind::Rapid, Point3::new(0.0, 0.0, 12.0), None);
    b.push(MoveKind::Plunge, Point3::new(0.0, 0.0, 6.0), Some(100.0));
    let t = b.finish();
    assert!(first_solid_plunge(&t, 3.0, &Solid { top: 10.0, footprint: None }).is_some());
}

#[test]
fn a_posed_drop_in_air_or_onto_a_cleared_column_passes() {
    // Off the stock, or above its top: air.
    let away = rect(100.0, 100.0, 120.0, 120.0);
    let t = posed_drop(Point3::new(0.0, 0.0, 12.0), lean(30.0), 6.0);
    assert_eq!(audit(&t, false, 3.0, &Solid { top: 10.0, footprint: Some(&away) }, "air"), Ok(()));
    assert_eq!(audit(&t, false, 3.0, &Solid { top: 1.0, footprint: None }, "air"), Ok(()));
    // A second drop down the same tilted column, after a cut at the
    // first drop's bottom swept across the axis, is cleared.
    let ax = lean(30.0);
    let s = Point3::new(0.0, 0.0, 4.0);
    let bottom = Point3::new(s.x - ax[0] * 3.0, s.y, s.z - ax[2] * 3.0);
    // The sweep runs across the lean (along Y), at the drop's own
    // depth along the axis and in Z.
    let mut b = PathBuilder::new(Point3::new(bottom.x, -5.0, bottom.z));
    b.set_axis(Some(ax));
    b.push(MoveKind::Feed, Point3::new(bottom.x, 5.0, bottom.z), Some(500.0));
    b.push(MoveKind::Rapid, Point3::new(bottom.x, 5.0, 12.0), None);
    b.push(MoveKind::Rapid, s, None);
    b.push(MoveKind::Plunge, bottom, Some(100.0));
    let t = b.finish();
    // The sweep is not axial, and the drop's column lies over it.
    assert_eq!(first_solid_plunge(&t, 3.0, &Solid { top: 10.0, footprint: None }), None);
}

// ------------------------------------------------- the 5-axis kinds

/// A spherical cap of radius 15 about the origin (apex Z 15): near
/// the apex its normals stand close to vertical, so the stamp's lean
/// there stays inside the core's cone.
fn dome() -> Vec<Triangle> {
    let (radius, polar_max, n_lat, n_lon) = (15.0_f64, 0.9_f64, 8usize, 16usize);
    let ring = |k: usize| -> Vec<Point3> {
        let ph = polar_max * k as f64 / n_lat as f64;
        let (z, rr) = (radius * ph.cos(), radius * ph.sin());
        (0..n_lon)
            .map(|j| {
                let th = std::f64::consts::TAU * j as f64 / n_lon as f64;
                Point3::new(rr * th.cos(), rr * th.sin(), z)
            })
            .collect()
    };
    let apex = Point3::new(0.0, 0.0, radius);
    let mut facets = Vec::new();
    let mut prev = ring(1);
    for j in 0..n_lon {
        facets.push(Triangle::new(apex, prev[j], prev[(j + 1) % n_lon]));
    }
    for k in 2..=n_lat {
        let cur = ring(k);
        for j in 0..n_lon {
            facets.push(Triangle::new(prev[j], cur[j], cur[(j + 1) % n_lon]));
            facets.push(Triangle::new(prev[j], cur[(j + 1) % n_lon], prev[(j + 1) % n_lon]));
        }
        prev = cur;
    }
    facets
}

fn fparams(pattern: FinishPattern) -> Finish3DParams {
    Finish3DParams {
        gap_link: camcraft_motion::finishing3d::GapLink::Jump,
        pattern,
        stepover: None,
        tool_diameter: 6.0,
        tolerance: 0.1,
        clearance_z: 25.0,
        retract_z: None,
        top_z: 15.0,
        bottom_z: 13.0,
        feed: 500.0,
        plunge_feed: 100.0,
        climb: true,
        ramp: false,
        min_step: None,
        ramp_angle: 45.0,
        entry_ramp: false,
        steep_stepover: None,
        pencil_reference: None,
        waterlines: true,
    }
}

const LEAN: AxisControl = AxisControl { lead_deg: 0.0, lag_deg: 0.0, max_tilt_deg: Some(45.0) };

fn run(f: &[Triangle], contour: bool, p: &Finish3DParams) -> Result<Toolpath, GenError> {
    if contour {
        contour5d(f, p, &LEAN, None)
    } else {
        parallel5d(f, p, &LEAN, None)
    }
}

#[test]
fn the_5_axis_kinds_ramp_or_refuse_a_tool_that_is_not_centre_cutting() {
    let f = dome();
    let stock = rect(-30.0, -30.0, 30.0, 30.0);
    let mut convicted = 0;
    for (contour, kind, pattern) in
        [(true, "5-Axis Contour", FinishPattern::Waterline), (false, "5-Axis Parallel", FinishPattern::Parallel)]
    {
        let p = fparams(pattern);
        for solid in [Solid { top: 15.0, footprint: None }, Solid { top: 15.0, footprint: Some(&stock) }] {
            let own = run(&f, contour, &p).expect("posed path");
            // The centre-cutting tool's path is the generator's own, byte for byte.
            let cc = center_cut_guard_as(kind, true, &solid, &p, |q| run(&f, contour, q)).expect("centre-cutting");
            assert_eq!(cc, own, "{kind}");
            let plunges = first_solid_plunge(&own, 3.0, &solid).is_some();
            convicted += usize::from(plunges);
            match center_cut_guard_as(kind, false, &solid, &p, |q| run(&f, contour, q)) {
                Ok(t) => {
                    assert_eq!(first_solid_plunge(&t, 3.0, &solid), None, "{kind}: {:?}", t.notes);
                    if plunges {
                        assert!(t.notes.iter().any(|n| n.contains("ramp instead")), "{kind}: {:?}", t.notes);
                    } else {
                        assert_eq!(t, own, "{kind}: a clean path is untouched");
                    }
                }
                Err(e) => {
                    assert!(plunges, "{kind}: refused a path that never plunged: {e:?}");
                    refused(kind, &e);
                    assert!(format!("{e:?}").contains(kind), "{kind}: the refusal names the kind: {e:?}");
                }
            }
        }
    }
    assert!(convicted > 0, "the posed entries plunge into the stock over the dome");
}

#[test]
fn the_convicted_5_axis_descent_is_a_posed_one() {
    // The ring entry over the dome holds a lean (off vertical): the
    // vertical-only reading would have missed it entirely. The ladder
    // starts one rung under the apex: M13.9's slope-aware ladder puts
    // the first ring of a crown that begins AT the apex within a few
    // degrees of it (a near-vertical pose, 0.998), and the lean this
    // test reads is the one a rung further down the flank.
    let f = dome();
    let t = run(&f, true, &Finish3DParams { top_z: 14.0, ..fparams(FinishPattern::Waterline) }).expect("posed path");
    let (xy, z) = first_solid_plunge(&t, 3.0, &Solid { top: 15.0, footprint: None }).expect("a plunge");
    let m = t
        .moves
        .iter()
        .find(|m| (m.to.x - xy.x).abs() < 1e-9 && (m.to.y - xy.y).abs() < 1e-9 && m.to.z == z)
        .expect("the convicted move");
    let a = m.axis.expect("a posed descent");
    assert!(a[2] < 0.99, "the pose leans: {a:?}");
    assert!((m.to.x - m.from.x).hypot(m.to.y - m.from.y) > 1e-3, "not a world-vertical drop");
}

#[test]
fn a_5_axis_path_whose_descents_fall_through_air_passes_untouched() {
    let f = dome();
    let away = rect(100.0, 100.0, 120.0, 120.0);
    for (contour, pattern) in [(true, FinishPattern::Waterline), (false, FinishPattern::Parallel)] {
        let p = fparams(pattern);
        let own = run(&f, contour, &p).expect("posed path");
        for solid in [Solid { top: 10.0, footprint: Some(&away) }, Solid { top: 1.0, footprint: None }] {
            let t = center_cut_guard_as("5-Axis", false, &solid, &p, |q| run(&f, contour, q)).expect("air passes");
            assert_eq!(t, own);
        }
    }
}

// ------------------------------- the unposed cone (M11.3 owed (1))

/// An UNPOSED path: rapid to Z 12 over the stock, then one straight
/// feed descending `drop` mm at `deg` degrees from horizontal (run
/// along +X).
fn unposed_descent(deg: f64, drop: f64) -> Toolpath {
    let run = drop / deg.to_radians().tan();
    let mut b = PathBuilder::new(Point3::new(0.0, 0.0, 30.0));
    b.push(MoveKind::Rapid, Point3::new(0.0, 0.0, 12.0), None);
    b.push(MoveKind::Feed, Point3::new(run, 0.0, 12.0 - drop), Some(100.0));
    b.finish()
}

#[test]
fn an_unposed_steep_descent_into_solid_is_a_plunge_like_its_posed_twin() {
    let stock = rect(-30.0, -30.0, 30.0, 30.0);
    for solid in [Solid { top: 10.0, footprint: Some(&stock) }, Solid { top: 10.0, footprint: None }] {
        // 80 deg from horizontal: 10 deg off straight down, inside the cone.
        let t = unposed_descent(80.0, 8.0);
        assert!(t.moves.iter().all(|m| m.axis.is_none()), "the path is unposed");
        assert!(first_solid_plunge(&t, 3.0, &solid).is_some(), "an 80 deg descent into solid is a plunge");
        refused("80 deg", &audit(&t, false, 3.0, &solid, "the test descent").expect_err("refused"));
        // The centre-cutting tool passes the same path, unchanged.
        assert_eq!(audit(&t, true, 3.0, &solid, "the test descent"), Ok(()));
        // The posed twin (held vertical) reads the same.
        let end = t.moves.last().map(|m| m.to).expect("a move");
        let mut b = PathBuilder::new(Point3::new(0.0, 0.0, 30.0));
        b.set_axis(Some([0.0, 0.0, 1.0]));
        b.push(MoveKind::Rapid, Point3::new(0.0, 0.0, 12.0), None);
        b.push(MoveKind::Feed, end, Some(100.0));
        assert_eq!(first_solid_plunge(&b.finish(), 3.0, &solid), first_solid_plunge(&t, 3.0, &solid));
    }
}

#[test]
fn the_unposed_cone_ends_at_30_degrees_off_vertical() {
    let solid = Solid { top: 10.0, footprint: None };
    // The boundary is 60 deg from horizontal (sin 30 = CORE). The cone
    // is closed in exact arithmetic (`lateral <= CORE * len`); AT 60 deg
    // rounding decides, so the law is pinned a hair either side, and the
    // boundary move itself reads exactly as `posed_axial` about +Z does
    // (one cone, posed or not - never widened).
    assert!(first_solid_plunge(&unposed_descent(60.01, 8.0), 3.0, &solid).is_some(), "60.01 deg: inside");
    assert!(first_solid_plunge(&unposed_descent(61.0, 8.0), 3.0, &solid).is_some(), "61 deg: inside");
    assert_eq!(first_solid_plunge(&unposed_descent(59.99, 8.0), 3.0, &solid), None, "59.99 deg: a ramp");
    assert_eq!(first_solid_plunge(&unposed_descent(59.0, 8.0), 3.0, &solid), None, "59 deg: a ramp");
    let edge = unposed_descent(60.0, 8.0);
    let m = edge.moves.last().expect("a move");
    assert_eq!(
        first_solid_plunge(&edge, 3.0, &solid).is_some(),
        posed_axial(m, Point3::new(0.0, 0.0, 1.0)),
        "60 deg: the unposed verdict is the cone's"
    );
}

#[test]
fn an_unposed_shallow_ramp_or_a_drop_in_air_stays_legal() {
    let solid = Solid { top: 10.0, footprint: None };
    // The default-style 3 deg ramp into solid is a ramp, not a plunge.
    let t = unposed_descent(3.0, 2.0);
    assert_eq!(first_solid_plunge(&t, 3.0, &solid), None);
    assert_eq!(audit(&t, false, 3.0, &solid, "the ramp"), Ok(()));
    // The steep descent off the stock falls through air.
    let away = rect(100.0, 100.0, 120.0, 120.0);
    let air = Solid { top: 10.0, footprint: Some(&away) };
    assert_eq!(audit(&unposed_descent(80.0, 8.0), false, 3.0, &air, "air"), Ok(()));
    // A climb up a steep line never plunges.
    let mut b = PathBuilder::new(Point3::new(0.0, 0.0, 2.0));
    b.push(MoveKind::Feed, Point3::new(1.0, 0.0, 9.0), Some(100.0));
    assert_eq!(first_solid_plunge(&b.finish(), 3.0, &solid), None);
}
