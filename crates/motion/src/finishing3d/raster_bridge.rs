//! Same-line gap bridging for the Z-following raster (#2431): a hole the
//! ball cannot enter is crossed by one straight feed, certified by the
//! swept-sphere (segment-to-triangle distance) clearance law of the
//! drop-cutter literature. Source: our own design notes.

use crate::geo2d::Point2;
use crate::geometry::{Point3, Triangle};
use crate::stock_sim::dist_seg_tri;

use super::params::SPLIT_MZ;
use super::grid::SupSet;

/// One marched piece: XY samples with their ball-center heights.
pub(super) type Piece = Vec<(Point2, f64)>;

/// Join the pieces of ONE raster line where the gap between them is a
/// hole the ball cannot enter. The march breaks a line wherever no
/// shallow facet answers under the center, and a hole narrower than
/// the tool is such a stretch: the ball would rest on the rim, which
/// no candidate of `support_at` answers, so the line hopped to the
/// retract and plunged again on the far rim - a stop and a pick for
/// every bolt hole (#2431). Here the two pieces become one run, the
/// gap one straight feed, when all of these hold:
///
/// - the gap's XY span is at most the tool diameter (`2r`). A wider
///   opening is one the ball fits into: its walls and floor are the
///   model's own surface, and the land model (`fill_openings`) is the
///   user's switch to fly over it. There the clean hop stays.
/// - the feed climbs or falls no steeper than the shallow region's
///   own slope (`SPLIT_MZ`, 45 degrees) plus `tol` - the march's
///   discontinuity law, so a shelf edge still breaks;
/// - the ball swept along the feed stays `r - chord` clear of every
///   facet: the exact certifier `column_clear` uses, with the march's
///   own chord budget as the only slack.
///
/// Anything else (non-finite input included) keeps the break.
pub(super) fn bridge_line(pieces: Vec<Piece>, r: f64, tol: f64, chord: f64, sups: &SupSet) -> Vec<Piece> {
    let mut out: Vec<Piece> = Vec::with_capacity(pieces.len());
    for pc in pieces {
        let (Some(&(b, zb)), Some(prev)) = (pc.first(), out.last_mut()) else {
            if !pc.is_empty() {
                out.push(pc);
            }
            continue;
        };
        match prev.last() {
            Some(&(a, za)) if bridge_ok((a, za), (b, zb), r, tol, chord, sups) => prev.extend(pc),
            _ => out.push(pc),
        }
    }
    out
}

/// The certificate for one straight feed between two ball centers
/// (XY, center Z). See [`bridge_line`] for the three laws.
pub(super) fn bridge_ok(
    (a, za): (Point2, f64),
    (b, zb): (Point2, f64),
    r: f64,
    tol: f64,
    chord: f64,
    sups: &SupSet,
) -> bool {
    let d = (b.x - a.x).hypot(b.y - a.y);
    // NaN fails every compare below: refused, never a silent pass.
    let span_ok = d <= 2.0 * r;
    if !(span_ok && za.is_finite() && zb.is_finite()) {
        return false;
    }
    let slope = (1.0 - SPLIT_MZ * SPLIT_MZ).max(0.0).sqrt() / SPLIT_MZ;
    let climb_ok = (zb - za).abs() <= d * slope + tol;
    if !climb_ok {
        return false;
    }
    let limit = r - chord;
    let (pa, pb) = (Point3::new(a.x, a.y, za), Point3::new(b.x, b.y, zb));
    let (x0, x1) = (a.x.min(b.x), a.x.max(b.x));
    let (y0, y1) = (a.y.min(b.y), a.y.max(b.y));
    for s in sups {
        if x1 < s.xmin || x0 > s.xmax || y1 < s.ymin || y0 > s.ymax {
            continue; // outside the padded footprint: the swept ball cannot reach it
        }
        let t = Triangle::new(s.tri[0], s.tri[1], s.tri[2]);
        let clear = dist_seg_tri(pa, pb, &t) >= limit;
        if !clear {
            return false;
        }
    }
    true
}
