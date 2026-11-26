//! The compensation seam law at a user-picked start (#1473 item 4): a
//! picked ring under control or wear compensation keeps its seam where
//! the user put it, so the law the automatic mid-run seam meets by
//! construction is proved here, and a pick it cannot hold is refused by
//! the pick's name - never moved to the mid-run seam.
//!
//! Provenance: RS-274 / ISO 6983 G41 / G42 / G40 and the published
//! startup / cancel behaviour of Fanuc-style controls - the control
//! builds the offset square to the first block after the engage and
//! withdraws it square to the last block before the cancel, so the two
//! offsets agree only when those blocks run on one straight line
//! through the seam; under control compensation the programmed part
//! contour must also run straight there (a part vertex has no single
//! normal to engage on). Our own model; no vendor material consulted.

use crate::cl::Move;
use crate::geo2d::{Point2, Region};

use super::contour::ContourParams;
use super::contour_comp::closest;
use super::face::GenError;
use super::ring::dist;

fn xy(m: &Move) -> (Point2, Point2) {
    (Point2::new(m.from.x, m.from.y), Point2::new(m.to.x, m.to.y))
}

fn dir(a: Point2, b: Point2) -> Option<Point2> {
    let l = dist(a, b);
    (l > 1e-9 && l.is_finite()).then(|| Point2::new((b.x - a.x) / l, (b.y - a.y) / l))
}

/// Prove the seam at the picked start `q`: `first_cut` is the ring's
/// first move after the engage, `last_cut` its last before the cancel,
/// `s_c` the seam (the pick's projection on the centre ring). `prog`
/// is the programmed part contour under control compensation, `None`
/// for wear (the programmed moves are the centre moves).
pub(super) fn picked_seam(
    p: &ContourParams,
    q: Point2,
    first_cut: &Move,
    last_cut: &Move,
    s_c: Point2,
    prog: Option<&Region>,
) -> Result<(), GenError> {
    let refuse = |why: String| {
        GenError::Invalid(format!(
            "start point ({:.3}, {:.3}): {} compensation needs the seam on a straight run - {why}; \
             pick a point beside a straight wall or clear the start point",
            q.x,
            q.y,
            p.links.approach.compensation.label()
        ))
    };
    let at = format!("at ({:.3}, {:.3})", s_c.x, s_c.y);
    if first_cut.arc.is_some() || last_cut.arc.is_some() {
        return Err(refuse(format!("{at} the ring cuts an arc")));
    }
    let (a0, a1) = xy(first_cut);
    let (b0, b1) = xy(last_cut);
    let (Some(t1), Some(t2)) = (dir(a0, a1), dir(b0, b1)) else {
        return Err(refuse(format!("{at} a cut beside the seam has no length")));
    };
    // The engage offset (square to the first cut) and the cancel offset
    // (square to the last) part by the radius times the turn's sine.
    let r = p.tool_diameter * 0.5;
    let room = p.tolerance.max(1e-6) + 0.02;
    let sine = t1.x * t2.y - t1.y * t2.x;
    if t1.x * t2.x + t1.y * t2.y <= 0.0 || (sine * r).abs() > room {
        return Err(refuse(format!("{at} the ring turns a corner")));
    }
    let Some(prog) = prog else {
        return Ok(());
    };
    // Control: the part contour the post programs must run straight at
    // the seam too - along the first cut, its nearest point off its
    // vertices.
    let Some((li, ei, s_p, _)) = closest(prog, s_c) else {
        return Err(refuse(format!("{at} there is no part contour")));
    };
    let pts = prog.loops().get(li).map(|l| l.points()).unwrap_or(&[]);
    let n = pts.len();
    let (Some(&e0), Some(&e1)) = (pts.get(ei), pts.get((ei + 1) % n.max(1))) else {
        return Err(refuse(format!("{at} there is no part contour")));
    };
    let on_vertex = dist(s_p, e0) <= room || dist(s_p, e1) <= room;
    let parallel = dir(e0, e1).is_some_and(|t| ((t1.x * t.y - t1.y * t.x) * r).abs() <= room);
    if on_vertex || !parallel {
        return Err(refuse(format!(
            "{at} the part contour turns a corner at ({:.3}, {:.3})",
            s_p.x, s_p.y
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cl::MoveKind;
    use crate::geo2d::Loop;
    use crate::geometry::Point3;
    use crate::links::{Approach, Compensation, Links};
    use crate::machining2d::{Lead, Side};

    fn params() -> ContourParams {
        ContourParams {
            tool_diameter: 6.0,
            stock_to_leave: 0.0,
            clearance_z: 5.0,
            retract_z: 2.0,
            top_z: 0.0,
            bottom_z: -4.0,
            stepdown: 2.0,
            feed: 500.0,
            plunge_feed: 100.0,
            climb: true,
            side: Side::Outside,
            lead: Lead::Perpendicular { length: 5.0 },
            tolerance: 0.01,
            tab_count: 0,
            tab_width: 0.0,
            tab_height: 0.0,
            entry: crate::entry::Entry3D::Plunge,
            helix_radius: 1.0,
            blend_turns: false,
            blend_radius: None,
            links: Links {
                approach: Approach { compensation: Compensation::Control, ..Approach::DEFAULT },
                ..Links::PLANAR
            },
            picks: crate::path_picks::PathPicks::default(),
        }
    }

    fn square() -> Region {
        let pts = [(0.0, 0.0), (40.0, 0.0), (40.0, 40.0), (0.0, 40.0)];
        let l = Loop::new(pts.iter().map(|&(x, y)| Point2::new(x, y)).collect());
        l.and_then(|l| Region::new(vec![l])).expect("square")
    }

    fn cut(a: (f64, f64), b: (f64, f64)) -> Move {
        Move::new(MoveKind::Feed, Point3::new(a.0, a.1, -2.0), Point3::new(b.0, b.1, -2.0), Some(500.0))
    }

    #[test]
    fn a_straight_centre_chord_over_a_part_vertex_is_refused_by_name() {
        // The seam on a chord of the outside corner arc: the centre cuts
        // run on one line, but the part contour has its vertex there.
        let s = 40.0 + 3.0 * std::f64::consts::FRAC_1_SQRT_2;
        let (first, last) = (cut((s, s), (s - 0.3, s + 0.3)), cut((s + 0.3, s - 0.3), (s, s)));
        let (p, sq, q, seam) = (params(), square(), Point2::new(43.0, 43.0), Point2::new(s, s));
        let why = picked_seam(&p, q, &first, &last, seam, Some(&sq)).err().map(|e| e.to_string()).unwrap_or_default();
        assert!(why.contains("start point (43.000, 43.000)") && why.contains("part contour turns a corner"), "{why}");
        // Wear programs the centre moves: one straight chord holds its law.
        assert!(picked_seam(&p, q, &first, &last, seam, None).is_ok());
    }

    #[test]
    fn a_seam_mid_run_on_a_straight_wall_holds() {
        let (first, last) = (cut((43.0, 20.0), (43.0, 30.0)), cut((43.0, 10.0), (43.0, 20.0)));
        let (q, seam) = (Point2::new(44.0, 20.0), Point2::new(43.0, 20.0));
        assert!(picked_seam(&params(), q, &first, &last, seam, Some(&square())).is_ok());
    }
}
