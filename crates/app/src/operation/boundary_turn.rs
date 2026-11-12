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

/// Snap a turn to the exact quarter turns (the common straight edges),
/// so an axis-aligned chain rotates by exact signs, never by 1e-16.
pub(super) fn snap_turn(a: f64) -> f64 {
    let q = a / std::f64::consts::FRAC_PI_2;
    if (q - q.round()).abs() < 1e-9 {
        let k = (q.round() as i64).rem_euclid(4);
        return k as f64 * std::f64::consts::FRAC_PI_2;
    }
    a
}

/// Close an open chain against a far circle on each side: the left
/// region (walking the chain from its lexicographically lower end) and
/// the right one, both clipped to `reach`, plus the chain's end-to-end
/// direction. The chain runs on straight past each end along its end
/// tangent - the textbook way to let an open curve split a plane.
pub(super) fn sides_of_chain(pts: &[Point2], reach: &Region) -> Result<(Region, Region, f64), GenError> {
    let mut p: Vec<Point2> = Vec::with_capacity(pts.len());
    for q in pts {
        if p.last().is_none_or(|l: &Point2| (l.x - q.x).hypot(l.y - q.y) > 1e-9) {
            p.push(*q);
        }
    }
    let (Some(&first), Some(&last)) = (p.first(), p.last()) else {
        return Err(GenError::Invalid("the open boundary holds no points - re-pick it".into()));
    };
    if p.len() < 2 {
        return Err(GenError::Invalid("the open boundary is a single point - re-pick a real edge".into()));
    }
    if (last.x, last.y) < (first.x, first.y) {
        p.reverse();
    }
    let Some((lo, hi)) = reach.bounds() else {
        return Err(GenError::Empty("the stock outline is empty".into()));
    };
    let (mut blo, mut bhi) = (lo, hi);
    for q in &p {
        blo = Point2::new(blo.x.min(q.x), blo.y.min(q.y));
        bhi = Point2::new(bhi.x.max(q.x), bhi.y.max(q.y));
    }
    let c = Point2::new((blo.x + bhi.x) * 0.5, (blo.y + bhi.y) * 0.5);
    let big = (bhi.x - blo.x).hypot(bhi.y - blo.y) + 10.0;
    let dir = |a: Point2, b: Point2| {
        let (dx, dy) = (a.x - b.x, a.y - b.y);
        let n = dx.hypot(dy);
        (dx / n, dy / n)
    };
    let n = p.len();
    let (sx, sy) = dir(p[0], p[1]);
    let (ex, ey) = dir(p[n - 1], p[n - 2]);
    let s = Point2::new(p[0].x + sx * 2.0 * big, p[0].y + sy * 2.0 * big);
    let e = Point2::new(p[n - 1].x + ex * 2.0 * big, p[n - 1].y + ey * 2.0 * big);
    let ang = |q: Point2| (q.y - c.y).atan2(q.x - c.x);
    let (ts, te) = (ang(s), ang(e));
    let tau = std::f64::consts::TAU;
    let side = |ccw: bool| -> Result<Region, GenError> {
        // From the end's angle back to the start's, the long way round
        // the side we close.
        let mut sweep = if ccw { (ts - te).rem_euclid(tau) } else { -(te - ts).rem_euclid(tau) };
        if sweep.abs() < 1e-6 {
            sweep = if ccw { tau } else { -tau };
        }
        let mut ring: Vec<Point2> = Vec::with_capacity(n + 130);
        ring.push(s);
        ring.extend(p.iter().copied());
        ring.push(e);
        let steps = 128;
        for k in 0..=steps {
            let t = te + sweep * f64::from(k) / f64::from(steps);
            ring.push(Point2::new(c.x + 3.0 * big * t.cos(), c.y + 3.0 * big * t.sin()));
        }
        let lp = Loop::new(ring)?;
        let half = Region::new(vec![lp]).map_err(|_| {
            GenError::Invalid(
                "the open boundary crosses itself once run on past its ends - pick a chain that splits the stock in two".into(),
            )
        })?;
        Ok(reach.intersect(&half)?)
    };
    let theta = (p[n - 1].y - p[0].y).atan2(p[n - 1].x - p[0].x);
    Ok((side(true)?, side(false)?, theta))
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
/// by [`turn_path`], a refusal by [`turn_refusal`].
pub(super) fn face_over_back(
    laid: &Region,
    p: &machining2d::FaceParams,
    lead: f64,
    solid: Option<&Region>,
    turn: f64,
) -> Result<Toolpath, GenError> {
    let path = machining2d::face_over(laid, p, lead, solid).map_err(|e| turn_refusal(e, -turn))?;
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
            blend_turns: false,
            holes: machining2d::FaceHoles::Lift,
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
            let back = face_over_back(&laid, &facing(), 0.0, None, turn).expect_err("refused");
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
        let same = face_over_back(&laid, &facing(), 0.0, None, 0.0).expect_err("refused");
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
