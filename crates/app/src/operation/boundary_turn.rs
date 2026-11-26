//! The geometry helpers of the area-controls law (M11.5), split from
//! `boundary` along the seam: closing an open chain against a far
//! circle (its two sides), turning a region or a path about Z (Face laid
//! along an open chain), and Face's bounding-band honesty check.
//!
//! Provenance: our own; the side construction is the textbook way to let
//! an open polyline split a plane (run it on along its end tangents and
//! close it with a far arc), the rotation is the plane rotation matrix.
//! No vendor material consulted.

use super::*;

pub(super) use camcraft_geometry::boundary_chains::snap_turn;

pub(super) fn sides_of_chain(pts: &[Point2], reach: &Region) -> Result<(Region, Region, f64), GenError> {
    camcraft_geometry::boundary_chains::sides_of_chain(pts, reach).map_err(super::boundary_chain_error)
}

/// Rotate a region about the origin.
pub(super) fn turn_region(r: &Region, a: f64) -> Result<Region, GenError> {
    if a == 0.0 {
        return Ok(r.clone());
    }
    let (s, c) = exact_sin_cos(a);
    let mut loops = Vec::with_capacity(r.loops().len());
    for l in r.loops() {
        loops.push(Loop::new(
            l.points().iter().map(|p| Point2::new(c * p.x - s * p.y, s * p.x + c * p.y)).collect(),
        )?);
    }
    Ok(Region::new(loops)?)
}

fn exact_sin_cos(a: f64) -> (f64, f64) {
    let q = a / std::f64::consts::FRAC_PI_2;
    if (q - q.round()).abs() < 1e-12 {
        return match (q.round() as i64).rem_euclid(4) {
            0 => (0.0, 1.0),
            1 => (1.0, 0.0),
            2 => (0.0, -1.0),
            _ => (-1.0, 0.0),
        };
    }
    a.sin_cos()
}

/// Rotate a path about the Z axis through the origin, on the lattice.
/// XY arcs keep their sense (a rotation preserves orientation); a path
/// with an arc in another plane cannot turn and refuses by name.
pub(super) fn turn_path(path: Toolpath, a: f64) -> Result<Toolpath, GenError> {
    if a == 0.0 {
        return Ok(path);
    }
    let (s, c) = exact_sin_cos(a);
    let q = crate::cl::lattice;
    let rot = |p: Point3| Point3::new(q(c * p.x - s * p.y), q(s * p.x + c * p.y), p.z);
    let mut out = path;
    for m in &mut out.moves {
        m.from = rot(m.from);
        m.to = rot(m.to);
        if let Some(arc) = &mut m.arc {
            if arc.plane != crate::cl::ArcPlane::Xy {
                return Err(GenError::Invalid(
                    "the facing path carries an arc outside the XY plane and cannot be laid along the open boundary".into(),
                ));
            }
            let p = arc.center;
            arc.center = Point2::new(q(c * p.x - s * p.y), q(s * p.x + c * p.y));
        }
    }
    Ok(out)
}

/// A refusal raised in the turned frame, carried back to the part's
/// frame through the same turn `-a` the path takes (#1017): its spot and
/// its ghost move with the path, so the viewport marks the cause where
/// the part is. A turn of nothing leaves the refusal untouched; a ghost
/// that cannot turn is dropped, never drawn in the wrong frame.
pub(super) fn turn_refusal(e: GenError, a: f64) -> GenError {
    if a == 0.0 {
        return e;
    }
    let (s, c) = exact_sin_cos(a);
    let q = crate::cl::lattice;
    e.map_location(|p| Point3::new(q(c * p.x - s * p.y), q(s * p.x + c * p.y), p.z))
        .map_ghost(|t| turn_path(t.clone(), a).unwrap_or_default())
}

/// Face laid in the turned frame and turned back to the part's: the path
/// by [`turn_path`], a refusal by [`turn_refusal`]. The user's start and
/// exit picks (#3326) arrive in the part's frame and are turned into the
/// laid frame, where the facer reads the corner nearest each.
pub(super) fn face_over_back(
    laid: &Region,
    p: &machining2d::FaceParams,
    lead: f64,
    solid: Option<&Region>,
    turn: f64,
    keep: Option<&machining2d::FaceKeep<'_>>,
    picks: &crate::path_picks::PathPicks,
) -> Result<Toolpath, GenError> {
    let mut laid_picks = picks.clone();
    if turn != 0.0 {
        let (s, c) = exact_sin_cos(turn);
        for k in laid_picks.start.iter_mut().chain(&mut laid_picks.exit) {
            k.xy = Point2::new(c * k.xy.x - s * k.xy.y, s * k.xy.x + c * k.xy.y);
        }
    }
    let path = machining2d::face_picked(laid, p, lead, solid, keep, &laid_picks).map_err(|e| turn_refusal(e, -turn))?;
    turn_path(path, -turn)
}

/// Face's honesty check under area controls: the facing law clears the
/// bounding band of its region (rows across the full Y span, an overrun
/// past each X end), so that band - in the turned frame `laid` lives in -
/// must not cross a zone the controls forbid. A crossing refuses by name;
/// a Pocket is the kind that follows a shaped area.
pub(super) fn face_band_sane(
    laid: &Region,
    forbidden: &[(&'static str, Region)],
    turn: f64,
    d: f64,
    stepover: f64,
) -> Result<(), GenError> {
    let Some((lo, hi)) = laid.bounds() else {
        return Ok(()); // the facing law refuses an empty footprint by name
    };
    let r = d * 0.5;
    let mid = (lo.y + hi.y) * 0.5;
    let (y0, y1) = (lo.y.min(mid - r), hi.y.max(mid + r));
    let mx = d + stepover.max(0.0) * 0.5;
    let band = Region::new(vec![Loop::new(vec![
        Point2::new(lo.x - mx, y0),
        Point2::new(hi.x + mx, y0),
        Point2::new(hi.x + mx, y1),
        Point2::new(lo.x - mx, y1),
    ])?])?;
    for (label, zone) in forbidden {
        let z = turn_region(zone, turn)?;
        if band.intersect(&z)?.area().abs() > 1e-2 {
            return Err(GenError::Invalid(format!(
                "Face clears the bounding band of its area, and that band crosses {label} - use a Pocket to follow a shaped area, or move the boundary"
            )));
        }
    }
    Ok(())
}

/// #3338: the kept side of an open boundary with its two lines offset
/// apart - the picked line by `picked`, the outer (stock) line by
/// `outer`. The kept side grows by the larger value, and `reach` grown
/// by `outer` clips the outer line back; the far side, shrunk by
/// `picked`, cuts the picked line later in the law.
pub(super) fn grow_apart(keep: &Region, reach: &Region, picked: f64, outer: f64, corner: CornerStyle) -> Result<Region, GenError> {
    let wide = super::boundary::grow(keep, picked.max(outer), corner)?;
    Ok(wide.intersect(&super::boundary::grow(reach, outer, corner)?)?)
}

#[cfg(test)]
mod law {
    use super::super::boundary::{Native, containment_delta};
    use super::*;

    fn square(h: f64) -> Region {
        Region::new(vec![
            Loop::new(vec![Point2::new(-h, -h), Point2::new(h, -h), Point2::new(h, h), Point2::new(-h, h)])
                .expect("loop"),
        ])
        .expect("region")
    }

    #[test]
    fn containment_is_the_tool_edge_geometry_relative_to_the_native_law() {
        let r = 5.0;
        assert_eq!(containment_delta(Native::ToolInside, Containment::Inside, r), 0.0);
        assert_eq!(containment_delta(Native::ToolInside, Containment::On, r), r);
        assert_eq!(containment_delta(Native::ToolInside, Containment::Outside, r), 2.0 * r);
        assert_eq!(containment_delta(Native::ToolOutside, Containment::Inside, r), -2.0 * r);
        assert_eq!(containment_delta(Native::ToolOutside, Containment::On, r), -r);
        assert_eq!(containment_delta(Native::ToolOutside, Containment::Outside, r), 0.0);
    }

    #[test]
    fn an_open_chain_splits_the_reach_into_its_left_and_right_sides() {
        let reach = square(10.0);
        // Walked from its lexicographically lower end: west to east at
        // y = 4, whichever way the points arrived.
        for pts in [
            vec![Point2::new(-3.0, 4.0), Point2::new(3.0, 4.0)],
            vec![Point2::new(3.0, 4.0), Point2::new(-3.0, 4.0)],
        ] {
            let (left, right, theta) = sides_of_chain(&pts, &reach).expect("sides");
            assert!((left.area().abs() - 20.0 * 6.0).abs() < 1e-3, "north strip");
            assert!((right.area().abs() - 20.0 * 14.0).abs() < 1e-3, "south part");
            assert!(left.contains(Point2::new(0.0, 8.0)) && right.contains(Point2::new(0.0, -8.0)));
            assert!(theta.abs() < 1e-12);
        }
        assert!(sides_of_chain(&[Point2::new(1.0, 1.0)], &reach).is_err(), "one point is no chain");
    }

    #[test]
    fn a_quarter_turn_is_exact_and_a_turn_and_back_is_the_identity() {
        assert_eq!(snap_turn(-std::f64::consts::FRAC_PI_2), 3.0 * std::f64::consts::FRAC_PI_2);
        assert_eq!(exact_sin_cos(std::f64::consts::PI), (0.0, -1.0));
        let mut b = crate::cl::PathBuilder::new(Point3::new(1.0, 2.0, 5.0));
        b.push(crate::cl::MoveKind::Feed, Point3::new(4.0, -3.0, 5.0), Some(100.0));
        let path = b.finish();
        for a in [std::f64::consts::FRAC_PI_2, std::f64::consts::PI, 0.7] {
            let back = turn_path(turn_path(path.clone(), a).expect("turn"), -a).expect("back");
            for (m, n) in path.moves.iter().zip(&back.moves) {
                assert!((m.to.x - n.to.x).abs() < 2e-6 && (m.to.y - n.to.y).abs() < 2e-6, "{a}");
            }
        }
        let r = turn_region(&square(3.0), std::f64::consts::FRAC_PI_2).expect("turn");
        assert!((r.area() - square(3.0).area()).abs() < 1e-9);
    }

    /// A 40 x 40 plate with two 6 x 6 holes at X 9..15, one over the other
    /// along Y with a 6 mm strip (Y -3..3) between them - in the part's
    /// own frame - and a facing law whose 10 mm tool cannot cut it.
    fn holed_plate() -> Region {
        let rect = |x0: f64, y0: f64, x1: f64, y1: f64, ccw: bool| {
            let mut v =
                vec![Point2::new(x0, y0), Point2::new(x1, y0), Point2::new(x1, y1), Point2::new(x0, y1)];
            if !ccw {
                v.reverse();
            }
            Loop::new(v).expect("loop")
        };
        Region::new(vec![
            rect(-20.0, -20.0, 20.0, 20.0, true),
            rect(9.0, -9.0, 15.0, -3.0, false),
            rect(9.0, 3.0, 15.0, 9.0, false),
        ])
        .expect("region")
    }

    fn facing() -> machining2d::FaceParams {
        machining2d::FaceParams {
            tool_diameter: 10.0,
            stepover: 6.0,
            clearance_z: 40.0,
            cut_z: 10.0,
            top_z: None,
            stepdown: None,
            feed: 1200.0,
            plunge_feed: 300.0,
            climb: true,
            blend_turns: false, blend_radius: None,
            holes: machining2d::FaceHoles::Lift,
            stock_top: None,
        }
    }

    /// #3326: a start pick is stated in the part's frame; under every
    /// turn the raster enters beside the part corner that the pick names.
    #[test]
    fn a_start_pick_names_the_part_corner_under_every_turn() {
        let part = Region::new(vec![Loop::new(vec![
            Point2::new(-30.0, -10.0),
            Point2::new(30.0, -10.0),
            Point2::new(30.0, 10.0),
            Point2::new(-30.0, 10.0),
        ])
        .expect("loop")])
        .expect("region");
        let pick = Point2::new(29.0, 9.0);
        let picks = crate::path_picks::PathPicks { start: vec![crate::path_picks::PathPick::xy(pick)], exit: Vec::new() };
        for turn in [0.0, std::f64::consts::FRAC_PI_2, std::f64::consts::PI, 3.0 * std::f64::consts::FRAC_PI_2] {
            let laid = turn_region(&part, turn).expect("laid");
            let path = face_over_back(&laid, &facing(), 0.0, None, turn, None, &picks).expect("faced");
            let at = path.moves.iter().find(|m| m.kind == crate::cl::MoveKind::Plunge).map(|m| m.to).expect("a plunge");
            // The entry stands a radius inside one edge and a radius out past the other.
            assert!((at.x - 30.0).abs() <= 5.0 + 1e-9 && (at.y - 10.0).abs() <= 5.0 + 1e-9, "turn {turn}: {at:?}");
        }
    }

    /// #1017: the refusal comes out of the turned frame; its spot must
    /// land where the cause stands on the part.
    #[test]
    fn a_refusal_from_the_turned_frame_names_its_spot_where_the_part_is() {
        let world = holed_plate();
        let in_strip = |p: Point3| (9.0..=15.0).contains(&p.x) && (-3.0..=3.0).contains(&p.y);
        let mut refused = 0;
        for turn in [std::f64::consts::FRAC_PI_2, std::f64::consts::PI, 3.0 * std::f64::consts::FRAC_PI_2, 0.7] {
            let laid = turn_region(&world, turn).expect("laid");
            // The rows run along the laid X: only a turned plate puts
            // the strip across them, so the refusal is real.
            let Err(e) = machining2d::face_over(&laid, &facing(), 0.0, None) else { continue };
            refused += 1;
            let raw = e.location().expect("the strip names its spot");
            let back = face_over_back(&laid, &facing(), 0.0, None, turn, None, &Default::default()).expect_err("refused");
            let at = back.location().expect("spot carried back");
            assert!(in_strip(at), "turn {turn}: {at:?} (laid frame said {raw:?})");
            assert!((at.z - 10.0).abs() < 1e-9, "{at:?}");
            assert_eq!(back.to_string(), e.to_string(), "the sentence is unchanged");
        }
        assert!(refused >= 2, "the quarter turns put the strip across the rows ({refused})");
    }

    #[test]
    fn with_no_turn_the_spot_is_unchanged() {
        // The strip across the rows as they are: holes side by side along X.
        let laid = turn_region(&holed_plate(), std::f64::consts::FRAC_PI_2).expect("laid");
        let e = machining2d::face_over(&laid, &facing(), 0.0, None).expect_err("refused");
        let same = face_over_back(&laid, &facing(), 0.0, None, 0.0, None, &Default::default()).expect_err("refused");
        assert_eq!(same.location(), e.location());
        assert!(same.location().is_some());
    }

    #[test]
    fn a_ghost_turns_back_with_the_spot_and_a_plain_refusal_passes() {
        let mut b = crate::cl::PathBuilder::new(Point3::new(1.0, 2.0, 5.0));
        b.push(crate::cl::MoveKind::Feed, Point3::new(4.0, 2.0, 5.0), Some(100.0));
        let e = GenError::invalid_at("x", Point3::new(3.0, 0.0, 5.0)).with_ghost(&b.finish());
        let back = turn_refusal(e, -std::f64::consts::FRAC_PI_2);
        let at = back.location().expect("spot");
        assert!(at.x.abs() < 1e-6 && (at.y + 3.0).abs() < 1e-6, "{at:?}");
        assert!(back.ghost().is_some(), "the ghost rides along");
        let plain = GenError::Invalid("p".into());
        assert_eq!(turn_refusal(plain, 1.0), GenError::Invalid("p".into()));
    }
}
