//! The entry of a start-side pass (#3746, 2D Offset "Pass starts at"
//! Inside or Outside round a part): the tool feeds down in air at a point
//! outside the stock box - by the tool radius plus the approach distance
//! (#3745) - then moves in the cut plane onto the first ring: a straight
//! lead in air where one is needed, then one G17 arc of the Arc radius and
//! Arc sweep, tangent to the first cut at the pass start. The lead is
//! proved against the cutter-centre area joined with the air outside the
//! stock box, and across none of the level's planned passes (the
//! verifier's Crossing law), so it touches no part and crosses no ring.
//! Where no lead fits, the asked Arc stands down to a named entry (the
//! vertical arc, a helix, the ramp along the first cut) or the operation
//! is refused by name - never a silent plunge.
//!
//! Provenance: the tangential arc lead-in of general machining practice
//! (RS-274 / ISO 6983 G17 G2/G3), entered from outside the stock as
//! public machining handbooks describe it; the arc proof is
//! `descent_xy::arc_at` (inscribed fan and circumscribed hull). No vendor
//! material consulted.

use crate::approach::AIR_ENTRY_REFUSAL;
use crate::cl::{MoveKind, PathBuilder, lattice};
use crate::entry::{Entry3D, chord_hits_plan};
use crate::entry_arc::{pts_dir, seg_dir};
use crate::geo2d::{Loop, Point2, Region};
use crate::geometry::Point3;

use super::descent::{Cuts, Descent, Lvl, plan_arm, ramp_fallback};
use super::pass_start::{PassStart, StartSide};
use super::pocket_strategy::PocketStrategy;
use super::descent_xy::{XyArc, arc_at};
use super::levels::{Cert, Motion};

/// Radii tried for the arc: the nominal, then halving - at most this many.
const RADIUS_TRIES: usize = 5;

/// What a start-side ladder knows of the air round the stock (#3746).
pub(super) struct AirRoom {
    /// The stock box in XY: the footprint's bounds.
    lo: Point2,
    hi: Point2,
    /// How far the plunge stands off the box: tool radius + approach
    /// distance.
    grow: f64,
    /// The legal room of a lead: the cutter-centre area joined with the
    /// air outside the stock box.
    legal: Region,
    /// The side the pass starts on, for the notes.
    side: &'static str,
}

impl AirRoom {
    /// The room of `center` (the cutter-centre area) round the stock
    /// footprint `stock`, for a tool of radius `tool_r` and the approach
    /// distance `approach`. `None` when the stock has no bounds or the
    /// numbers are not lengths (the ladder then keeps its old entry).
    pub(super) fn new(stock: &Region, center: &Region, tool_r: f64, approach: f64, inside: bool) -> Option<Self> {
        let (lo, hi) = stock.bounds()?;
        let grow = tool_r + approach.max(0.0);
        if !(grow.is_finite() && tool_r > 0.0 && lo.x < hi.x && lo.y < hi.y) {
            return None;
        }
        // The air reaches well past the plunge circle: room for a lead of
        // a few tool diameters beyond it.
        let far = grow + 8.0 * tool_r + 10.0;
        let outer = rect(lo.x - far, lo.y - far, hi.x + far, hi.y + far)?;
        // The box shrunk by a micron, so the air overlaps the area along
        // the box edge and the union has no seam there.
        let shrink = 1e-3;
        let inner = rect(lo.x + shrink, lo.y + shrink, hi.x - shrink, hi.y - shrink)?;
        let legal = outer.difference(&inner).ok()?.union(center).ok()?;
        Some(Self { lo, hi, grow, legal, side: if inside { "Inside" } else { "Outside" } })
    }

    /// The room of a resolved start `side` (Inside or Outside round a
    /// part, with the stock known) for a ring `strategy`; `None` for every
    /// other ladder.
    pub(super) fn for_side(side: &StartSide, strategy: PocketStrategy, center: &Region, tool_r: f64, approach: f64) -> Option<Self> {
        let rings = matches!(strategy, PocketStrategy::Waterline | PocketStrategy::OffsetRings);
        let started = matches!(side.start, PassStart::Inside | PassStart::Outside);
        let stock = side.stock.as_ref().filter(|_| side.around_part && rings && started)?;
        Self::new(stock, center, tool_r, approach, side.start == PassStart::Inside)
    }

    /// #3810: the room less `keepout` - the fixtures and clamps of an
    /// avoid level, in cutter-centre terms, over the whole descent - so
    /// that no plunge or lead in the air touches them. `None` when the
    /// difference fails (the ladder then keeps its old entry).
    pub(super) fn without(mut self, keepout: &Region) -> Option<Self> {
        if !keepout.is_empty() {
            self.legal = self.legal.difference(keepout).ok()?;
        }
        Some(self)
    }

    /// Whether the pass starts at Outside (#3884: its Arc entry is in air
    /// or refused).
    pub(super) fn outside(&self) -> bool {
        self.side == "Outside"
    }

    /// The XY distance of `p` from the stock box (0 inside it).
    fn off_box(&self, p: Point2) -> f64 {
        let dx = (self.lo.x - p.x).max(p.x - self.hi.x).max(0.0);
        let dy = (self.lo.y - p.y).max(p.y - self.hi.y).max(0.0);
        dx.hypot(dy)
    }
}

fn rect(x0: f64, y0: f64, x1: f64, y1: f64) -> Option<Region> {
    let pts = vec![Point2::new(x0, y0), Point2::new(x1, y0), Point2::new(x1, y1), Point2::new(x0, y1)];
    Region::new(vec![Loop::new(pts).ok()?]).ok()
}

impl Motion {
    /// #3316: prove keep-down pass links through cut material too (the
    /// start-side plan's link law).
    pub(super) fn with_cleared_links(mut self, on: bool) -> Self {
        self.cleared_links = on;
        self
    }

    /// #3746: the start side's air room; `None` keeps the old entry.
    pub(super) fn with_air(mut self, air: Option<AirRoom>) -> Self {
        self.air = air;
        self
    }
}

/// A proved air entry: the plunge in air, then the lead and the arc.
pub(super) struct AirLead {
    pub plunge: Point2,
    pub arc: XyArc,
}

/// The start-side plan of the asked Arc entry (`None`: not a start-side
/// ladder, or another entry was asked).
#[allow(clippy::too_many_arguments)]
pub(super) fn plan(arm: Entry3D, _zig: bool, l: &Lvl, leg: Point2, z: f64, prev_z: f64, m: &Motion) -> Option<Descent> {
    let air = m.air.as_ref()?;
    if arm != Entry3D::Arc {
        return None;
    }
    Some(match lead(air, l, m) {
        Ok(a) => Descent::Air(Box::new(a)),
        Err(why) => stand_in(air, why, l, leg, z, prev_z, m),
    })
}

/// #3746: the air entry onto a pass start in material after a lift (a
/// start-side ladder only); `None` where no lead reaches it.
pub(super) fn restart(l: &Lvl, m: &Motion) -> Option<Descent> {
    let air = m.air.as_ref()?;
    lead(air, l, m).ok().map(|a| Descent::Air(Box::new(a)))
}

/// The lead onto `l.pass[0]`: both sides of the first cut at each radius
/// from the nominal down, the first one whose arc, straight lead and
/// plunge are all proved.
fn lead(air: &AirRoom, l: &Lvl, m: &Motion) -> Result<AirLead, &'static str> {
    let a = &m.links.approach;
    let (&e, rest) = l.pass.split_first().ok_or("the pass has no start")?;
    let dir = l.walk.and_then(|w| w.first()).map_or_else(|| pts_dir(e, rest), |s| seg_dir(e, s));
    let u = dir.and_then(unit).ok_or("the first cut has no direction")?;
    let sweep = if a.arc_sweep > 0.0 { a.arc_sweep_rad() } else { std::f64::consts::FRAC_PI_2 };
    let r0 = a.arc_nominal(m.arc_radius);
    if !(r0.is_finite() && r0 > 0.0 && sweep.is_finite() && sweep > 0.0) {
        return Err("the arc has no radius");
    }
    let cert = Cert::Region(&air.legal);
    let la = Lvl { cen: &cert, passes: l.passes, pass: l.pass, walk: None, mat_top: l.mat_top };
    let tool_r = m.tool_r();
    let mut r = r0;
    let mut arced = false;
    // A lead whose arc starts with the disc still in the air is taken
    // first: then the straight lead cuts nothing and the arc is the first
    // cut. Else the first proved lead.
    let mut touching: Option<AirLead> = None;
    for _ in 0..RADIUS_TRIES {
        for ccw in [true, false] {
            let Some(x) = arc_at(&la, e, u, r, sweep, ccw) else { continue };
            arced = true;
            if let Some(p) = plunge_point(air, &x)
                && (p == x.start || (cert.chord_ok(p, x.start) && !chord_hits_plan(p, x.start, l.passes)))
            {
                if air.off_box(x.start) >= tool_r - 1e-9 {
                    return Ok(AirLead { plunge: p, arc: x });
                }
                touching.get_or_insert(AirLead { plunge: p, arc: x });
            }
        }
        if touching.is_some() {
            break;
        }
        r *= 0.5;
    }
    if let Some(t) = touching {
        return Ok(t);
    }
    Err(if arced {
        "no straight lead from the air outside the stock box reaches the arc without a ring or the part in its way"
    } else {
        "no arc onto the first ring stays clear of the part and of the other rings"
    })
}

fn unit(d: Point2) -> Option<Point2> {
    let len = d.x.hypot(d.y);
    (len.is_finite() && len > 0.0).then(|| Point2::new(d.x / len, d.y / len))
}

/// Where the tool plunges for the arc `x`: back along the arc's tangent at
/// its start, the first point at least `grow` off the stock box (the arc
/// start itself when it is that far off already).
fn plunge_point(air: &AirRoom, x: &XyArc) -> Option<Point2> {
    let s = x.start;
    if air.off_box(s) >= air.grow {
        return Some(s);
    }
    // The arc's direction of travel at its start; the lead runs back
    // along it, so the lead and the arc are tangent.
    let rs = Point2::new(s.x - x.center.x, s.y - x.center.y);
    let t = unit(if x.ccw { Point2::new(-rs.y, rs.x) } else { Point2::new(rs.y, -rs.x) })?;
    let at = |len: f64| Point2::new(s.x - t.x * len, s.y - t.y * len);
    let span = (air.hi.x - air.lo.x).hypot(air.hi.y - air.lo.y) + 2.0 * air.grow + 2.0 * x.radius;
    if air.off_box(at(span)) < air.grow {
        return None;
    }
    // The distance off a convex box is convex along a line, so the points
    // short of `grow` form one stretch from the start: bisect its end.
    let (mut lo, mut hi) = (0.0, span);
    for _ in 0..64 {
        let mid = 0.5 * (lo + hi);
        if air.off_box(at(mid)) < air.grow { lo = mid } else { hi = mid }
    }
    let q = at(hi + 1e-3);
    let p = Point2::new(lattice(q.x), lattice(q.y));
    (air.off_box(p) >= air.grow).then_some(p)
}

/// The named stand-in of an Inside pass when no air entry fits: a helix,
/// else the ramp along the first cut - or the refusal. An Outside pass is
/// refused by name (#3884). Never the axial dive that the Arc entry
/// gave before (#3746), and never the vertical arc ramp: its first chords
/// come down within 30 degrees of the axis, which the plunge law reads as
/// a plunge into material.
#[allow(clippy::too_many_arguments)]
fn stand_in(air: &AirRoom, why: &str, l: &Lvl, leg: Point2, z: f64, prev_z: f64, m: &Motion) -> Descent {
    // #3884: an Outside pass has no stand-in: in air, or refused by name.
    if let Err(r) = super::entry_warn::outside_air_check(air.outside(), Entry3D::Arc, false, why) {
        return Descent::Refused(r);
    }
    let head = format!("entry: {} pass - no air entry outside the stock box fits ({why})", air.side);
    let h = plan_arm(Entry3D::Helix, false, l, leg, z, prev_z, m);
    if matches!(h, Descent::Helix(..)) {
        return Descent::Named(Box::new(h), format!("{head} - helix entry instead"));
    }
    if let Some(r) = ramp_fallback(l, z, prev_z, m) {
        return Descent::Named(Box::new(r), format!("{head} - ramp along the first cut instead"));
    }
    Descent::Refused(format!("{AIR_ENTRY_REFUSAL} - {head}, and no helix or ramp fits onto Z {z:.3}"))
}

/// Post the air entry: over to the plunge point, down in air, the
/// straight lead (when the plunge is not on the arc's start), then the G17
/// arc onto the pass start. Returns the planar cuts at the level's plane.
pub(super) fn push(b: &mut PathBuilder, a: &AirLead, l: &Lvl, z: f64, tz: f64, m: &Motion) -> Cuts {
    let Some(&e) = l.pass.first() else {
        return Vec::new();
    };
    let ap = &m.links.approach;
    let (p, x) = (a.plunge, &a.arc);
    let cur = b.at();
    if cur.x != p.x || cur.y != p.y {
        b.push(MoveKind::Rapid, Point3::new(p.x, p.y, tz), None);
    }
    // The plunge stands `grow` off the stock box: no material under the
    // disc, so no centre-cutting law applies. It keeps the plunge
    // height's shape (a rapid to just over the material, then the feed).
    let hz = (l.mat_top + ap.plunge_height).min(m.hop_z.max(l.mat_top));
    if ap.plunge_height > 0.0 && hz.is_finite() && hz < b.at().z && hz > z {
        b.push(MoveKind::Rapid, Point3::new(p.x, p.y, hz), None);
    }
    b.push(MoveKind::Plunge, Point3::new(p.x, p.y, z), Some(ap.plunge(m.plunge_feed)));
    let lf = ap.lead_feed(m.feed);
    let mut cuts: Cuts = Vec::new();
    if p != x.start {
        b.push(MoveKind::Feed, Point3::new(x.start.x, x.start.y, z), Some(lf));
        cuts.push(vec![p, x.start]);
    }
    if x.radius < ap.arc_nominal(m.arc_radius) {
        b.note_once(&format!("entry: the arc onto the first ring fits only under its radius - flown at {:.3} mm", x.radius));
    }
    b.push_arc(MoveKind::Feed, Point3::new(e.x, e.y, z), Some(lf), x.center, x.ccw);
    cuts.extend(x.cuts(l.pass.len()));
    cuts
}

#[cfg(test)]
#[path = "air_entry_tests.rs"]
mod tests;
