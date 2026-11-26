//! Pins of the multi-axis Deburr (#1894): the flank lies on the
//! chamfer plane, steep edges break, Axis4 keeps the rotary plane, and
//! the holder escape tilts or refuses by name.

use crate::cl::{MoveKind, Toolpath};
use crate::finishing3d::{deburr, DeburrBreak, DeburrEdges, DeburrParams, UnreachableEdges};
use crate::geometry::{cube, Point3, Triangle};
use crate::machining2d::GenError;

use super::avoid::{shank_hits_at, Shank};
use super::deburr_axis::{multiaxis_deburr, AxisDeburrParams};
use super::deburr_pose::DeburrAxisMode;

fn boxy() -> Vec<Triangle> {
    cube(Point3::ZERO, Point3::new(40.0, 40.0, 15.0)).facets.to_vec()
}

fn base() -> DeburrParams {
    DeburrParams {
        width: 0.5,
        tip_angle_deg: 90.0,
        clearance_z: 30.0,
        top_z: 15.0,
        bottom_z: 1.0,
        feed: 400.0,
        plunge_feed: 100.0,
        min_edge_angle_deg: 0.0,
        exclude_holes: false,
        hole_min_diameter: 0.0,
        hole_max_diameter: 0.0,
        unreachable: UnreachableEdges::SkipWithNote,
        sizing: DeburrBreak::default(),
    }
}

fn params(mode: DeburrAxisMode, max_tilt_deg: f64) -> AxisDeburrParams {
    AxisDeburrParams {
        base: base(),
        mode,
        lead_deg: 0.0,
        lag_deg: 0.0,
        max_tilt_deg,
        tool_diameter: 6.0,
        shank: None,
    }
}

fn run(facets: &[Triangle], p: &AxisDeburrParams) -> Result<Toolpath, GenError> {
    multiaxis_deburr(facets, p, &DeburrEdges::default())
}

fn feeds(path: &Toolpath) -> impl Iterator<Item = (Point3, [f64; 3])> + '_ {
    path.moves.iter().filter(|m| m.kind == MoveKind::Feed).filter_map(|m| Some((m.to, m.axis?)))
}

#[test]
fn level_top_edge_stands_the_axis_upright_on_the_3d_chamfer_line() {
    // A 90-deg cone on the 90-deg top-front edge (y = 0, z = 15): the
    // natural pose is vertical, and the tip stands on the 3D Deburr's
    // chamfer line z - y = 15 - s, s = w / (2 sin 45).
    let path = run(&boxy(), &params(DeburrAxisMode::Axis5, 30.0)).expect("the box breaks");
    let s = 0.5 / (2.0 * 45f64.to_radians().sin());
    let front: Vec<_> = feeds(&path).filter(|(p, _)| p.y < 0.0 && p.z > 10.0 && p.x > 1.0 && p.x < 39.0).collect();
    assert!(!front.is_empty(), "the top-front edge is ridden");
    for (p, a) in front {
        assert!((a[2] - 1.0).abs() < 1e-9, "upright axis on a level 90-deg edge, got {a:?}");
        assert!((p.z - p.y - (15.0 - s)).abs() < 1e-9, "tip on the chamfer line, got {p:?}");
    }
}

#[test]
fn vertical_corner_edges_break_where_the_3d_deburr_skips_them() {
    let flat = deburr(&boxy(), &base()).expect("the 3D ride runs");
    assert!(flat.notes.iter().any(|n| n.contains("steeper than the cone's flank")), "{:?}", flat.notes);
    assert!(flat.breaks.iter().all(|b| b.edge.windows(2).all(|w| (w[1].z - w[0].z).abs() < 1e-9)));

    let path = run(&boxy(), &params(DeburrAxisMode::Axis5, 45.0)).expect("the tilted ride runs");
    let vertical = path.breaks.iter().filter(|b| b.edge.windows(2).all(|w| (w[1].z - w[0].z).abs() > 1e-6)).count();
    assert_eq!(vertical, 4, "each vertical corner edge declares a framed break");
    for b in &path.breaks {
        assert!(b.is_sane() && b.volume.is_some(), "a framed, sane break: {b:?}");
    }
    // The tilt on a vertical edge is the most upright one inside the cap.
    let tilted = feeds(&path).filter(|(_, a)| a[2] < 0.99).collect::<Vec<_>>();
    assert!(!tilted.is_empty());
    assert!(tilted.iter().all(|(_, a)| a[2] > 0.5), "a vertical edge leans the axis well up");
}

#[test]
fn axis4_keeps_every_axis_in_the_rotary_plane_and_skips_the_corners() {
    let path = run(&boxy(), &params(DeburrAxisMode::Axis4, 45.0)).expect("the rotary ride runs");
    let n = path.moves.iter().filter_map(|m| m.axis).inspect(|a| assert!(a[0].abs() < 1e-9, "axis off the YZ plane: {a:?}")).count();
    assert!(n > 0);
    assert!(path.notes.iter().any(|m| m.contains("no pose inside the max tilt, the rotary plane")), "{:?}", path.notes);
}

#[test]
fn refuse_names_the_unreachable_corner_edges() {
    let mut p = params(DeburrAxisMode::Axis4, 45.0);
    p.base.unreachable = UnreachableEdges::Refuse;
    let err = run(&boxy(), &p).expect_err("the vertical corners have no rotary-plane pose");
    assert!(format!("{err:?}").contains("set to Refuse"), "{err:?}");
}

#[test]
fn lead_beyond_the_tilt_cap_is_refused_at_the_door() {
    let mut p = params(DeburrAxisMode::Axis5, 10.0);
    p.lead_deg = 20.0;
    let err = run(&boxy(), &p).expect_err("lean outside the cap");
    assert!(format!("{err:?}").contains("lead minus lag"), "{err:?}");
}

/// A plate with a tall post in front of its top-front edge, and a tool
/// whose holder stands over the post when the axis is upright.
fn plate_and_post() -> (Vec<Triangle>, AxisDeburrParams) {
    let mut f = cube(Point3::ZERO, Point3::new(40.0, 40.0, 10.0)).facets.to_vec();
    f.extend(cube(Point3::new(18.0, -12.0, 0.0), Point3::new(22.0, -8.0, 60.0)).facets.iter().copied());
    let mut p = params(DeburrAxisMode::Axis5, 0.0);
    p.base.top_z = 10.0;
    p.base.clearance_z = 70.0;
    p.shank = Some(Shank { flute_len: 8.0, shank_r: 4.0, shank_len: 12.0, holder_r: 8.0, holder_len: 40.0 });
    (f, p)
}

#[test]
fn the_holder_over_a_post_refuses_by_name_without_tilt() {
    let (f, mut p) = plate_and_post();
    p.base.unreachable = UnreachableEdges::Refuse;
    let err = run(&f, &p).expect_err("no tilt room past the post");
    let msg = format!("{err:?}");
    assert!(msg.contains("blocked by the shank or holder") && !msg.contains("; 0 blocked"), "{msg}");
}

#[test]
fn the_holder_tilts_clear_of_the_post_inside_the_cap() {
    let (f, mut p) = plate_and_post();
    p.max_tilt_deg = 60.0;
    let path = run(&f, &p).expect("the escape finds room");
    let sh = p.shank.expect("a shank");
    let boxes: Vec<[f64; 6]> = f
        .iter()
        .map(|t| {
            [
                t.a.x.min(t.b.x.min(t.c.x)),
                t.a.y.min(t.b.y.min(t.c.y)),
                t.a.z.min(t.b.z.min(t.c.z)),
                t.a.x.max(t.b.x.max(t.c.x)),
                t.a.y.max(t.b.y.max(t.c.y)),
                t.a.z.max(t.b.z.max(t.c.z)),
            ]
        })
        .collect();
    // Every posed move on the plate's front edge stands clear, and the
    // stretch in front of the post leans.
    let front: Vec<_> = feeds(&path).filter(|(q, _)| q.y < 0.0 && q.y > -3.0 && q.z > 7.0).collect();
    assert!(!front.is_empty(), "the plate's front edge is ridden");
    for (tip, a) in &front {
        let c = Point3::new(tip.x + 3.0 * a[0], tip.y + 3.0 * a[1], tip.z + 3.0 * a[2]);
        assert!(!shank_hits_at(c, *a, 3.0, &sh, &f, &boxes), "a posed move touches at {tip:?} {a:?}");
    }
    assert!(front.iter().any(|(q, a)| (q.x - 20.0).abs() < 4.0 && a[2] < 0.99), "the axis leans at the post");
}

#[test]
fn a_run_ends_where_the_chamfer_plane_turns_at_a_corner() {
    // A square sheet's four rims chain into one loop (two rims meet at
    // each corner), and each keeps the upright axis, so no axis turn
    // ends the run at a corner: only the plane law does. A run that
    // turned the corner at the contact would drag the tip across it.
    let (a, b, c, d) = (
        Point3::new(0.0, 0.0, 15.0),
        Point3::new(40.0, 0.0, 15.0),
        Point3::new(40.0, 40.0, 15.0),
        Point3::new(0.0, 40.0, 15.0),
    );
    let sheet = [Triangle { a, b, c }, Triangle { a, b: c, c: d }];
    let path = run(&sheet, &params(DeburrAxisMode::Axis5, 30.0)).expect("the sheet breaks");
    assert!(!path.breaks.is_empty());
    for b in &path.breaks {
        let frames = &b.volume.as_ref().expect("a framed cap").frames;
        let n0 = frames[0].outward;
        for f in frames {
            let d = n0[0] * f.outward[0] + n0[1] * f.outward[1] + n0[2] * f.outward[2];
            assert!(d > 10f64.to_radians().cos() - 1e-9, "a run turns its chamfer plane: {n0:?} to {:?}", f.outward);
        }
    }
}


#[path = "deburr_safety_tests.rs"]
mod safety;
