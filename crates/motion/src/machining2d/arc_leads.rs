//! Lead-in and lead-out geometry for 2.5-D walks: the straight, arc and
//! tangential approaches onto a fitted path and off its end. Split from
//! `arcs.rs` (#689, the 400-line law); structure only.
//!
//! Provenance: the lead shapes (perpendicular, linear, quarter-arc and
//! tangential half-circle approaches tangent to the wall) are general
//! machining-handbook practice for contour entry and exit, and their arcs
//! are circular interpolation per RS-274 / ISO 6983 (G2/G3) solved with
//! plane geometry; no vendor material consulted.

use crate::geo2d::Point2;

use super::arcs::Seg;
use super::contour::Lead;

/// (plunge point, segments from it onto the path, ending exactly at
/// `s`).
pub(super) fn lead_in(s: Point2, t: Point2, air: Point2, lead: Lead) -> (Point2, Vec<Seg>) {
    match lead {
        Lead::None => (s, Vec::new()),
        // M11.3 C: straight in along the air normal, square to the wall.
        Lead::Perpendicular { length } => (
            Point2::new(s.x + air.x * length, s.y + air.y * length),
            vec![Seg::Line(vec![s])],
        ),
        Lead::Linear { length } => (
            Point2::new(s.x - t.x * length, s.y - t.y * length),
            vec![Seg::Line(vec![s])],
        ),
        Lead::Arc { radius } => {
            let o = Point2::new(s.x + air.x * radius, s.y + air.y * radius);
            let e = Point2::new(
                s.x + (air.x - t.x) * radius,
                s.y + (air.y - t.y) * radius,
            );
            // Quarter arc e -> s about o: ccw iff s bears left of the
            // start radius. The chord error at p.tolerance only appears
            // when something downstream flattens the move.
            let ccw = ((e.x - o.x) * (s.y - o.y) - (e.y - o.y) * (s.x - o.x)) > 0.0;
            (e, vec![Seg::Arc { center: o, ccw, to: s }])
        }
        Lead::Tangential { radius } => {
            // Semicircle e -> s about o = s + air*radius, with the plunge
            // point e = s + 2*radius*air: e - o and s - o are antiparallel,
            // so the quarter arm's radius-vector cross would vanish. The
            // surviving law is its limit: arrive at s moving along +t means
            // turning ccw iff air bears left of t - cross(air, t) < 0.
            // air_side puts climb's air left of t: ccw; conventional cw
            // (arcs.rs and machining2d.rs derive this from the law, never
            // hardcode it).
            let o = Point2::new(s.x + air.x * radius, s.y + air.y * radius);
            let e = Point2::new(s.x + air.x * 2.0 * radius, s.y + air.y * 2.0 * radius);
            let ccw = (air.x * t.y - air.y * t.x) < 0.0;
            (e, vec![Seg::Arc { center: o, ccw, to: s }])
        }
    }
}

/// Segments carrying the tool off the path end (not including it).
pub(super) fn lead_out(e: Point2, t: Point2, air: Point2, lead: Lead) -> Vec<Seg> {
    match lead {
        Lead::None => Vec::new(),
        Lead::Perpendicular { length } => {
            vec![Seg::Line(vec![Point2::new(e.x + air.x * length, e.y + air.y * length)])]
        }
        Lead::Linear { length } => vec![Seg::Line(vec![Point2::new(
            e.x + t.x * length,
            e.y + t.y * length,
        )])],
        Lead::Arc { radius } => {
            let o = Point2::new(e.x + air.x * radius, e.y + air.y * radius);
            let q = Point2::new(
                e.x + (air.x + t.x) * radius,
                e.y + (air.y + t.y) * radius,
            );
            let ccw = ((e.x - o.x) * (q.y - o.y) - (e.y - o.y) * (q.x - o.x)) > 0.0;
            vec![Seg::Arc { center: o, ccw, to: q }]
        }
        Lead::Tangential { radius } => {
            // The mirror of the lead-in half arc: leave the path end
            // overruning tangentially, sweep the semicircle out through
            // air, and stand off the wall 2*radius on the normal, moving
            // parallel to where the cut was going. Same sign law at the
            // tangent point as the lead-in: climb ccw, conventional cw.
            let o = Point2::new(e.x + air.x * radius, e.y + air.y * radius);
            let q = Point2::new(e.x + air.x * 2.0 * radius, e.y + air.y * 2.0 * radius);
            let ccw = (air.x * t.y - air.y * t.x) < 0.0;
            vec![Seg::Arc { center: o, ccw, to: q }]
        }
    }
}
