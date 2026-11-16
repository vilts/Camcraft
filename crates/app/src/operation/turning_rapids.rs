//! Issue #103: the turned family's axial rapids judged in the meridian (turning) plane against the revolved stock - the shop rule that a rapid never enters uncut material (machine-tool handbooks) read about the spindle as a body of revolution; our own design, no vendor material consulted.

use crate::cl::{MoveKind, Toolpath};
use crate::geometry::{Point3, Triangle};
use crate::rest::DrillAhead;
use crate::turning::profile::Frame;
use crate::verify::Warning;

use super::drill_prior::{PriorCell, PriorRead};
use super::kinds::Section;
use super::turning_credit::pilots;
use super::turning_outline::Revolved;
use super::OpParams;

/// Most stations one rapid is sampled at: a longer rapid samples more
/// sparsely, never unboundedly.
const MAX_SAMPLES: f64 = 20_000.0;

/// What a turned path's axial rapids are judged against.
pub(super) struct Meridian<'a> {
    pub part: &'a [Triangle],
    /// The setup's stock; `None` reads the part as the stock.
    pub stock: Option<&'a [Triangle]>,
    /// The insert's nose radius: the moves carry its centre.
    pub nose: f64,
    /// The margin a reading must clear by (the gouge tolerance).
    pub margin: f64,
    pub z_tol: f64,
    /// The Drill ops ahead: an axial pilot is a removed cylinder with
    /// its point's cone, read on a lathe too.
    pub drills: &'a [DrillAhead],
    /// Half the blade's band along the axis (#103 review B2): a groove
    /// or parting blade's move carries the band's CENTRE, a thread's
    /// its crest - the edge spans this far either side. 0 for a point.
    pub band: f64,
    /// A boring bar's body (#103 review B1, B3, B4); the nose disc
    /// stays the cutting tip's, the body must fit the hole too.
    pub body: Option<BarBody>,
    /// The turning ops ahead in the setup: their cuts are this op's
    /// air (#159).
    pub ahead: &'a [crate::rest::TurnedAhead],
    /// A blade's edge outward of its tip (#159) - "outward" is LARGER
    /// radius, right for every OD blade kind (groove, part, thread). An
    /// internal (ID) groover or thread insert stands its body toward the
    /// axis instead: adding one must flip this side (or pass `None`), or
    /// the credit runs into the wall beyond its tip. A groove blade's band
    /// is cut everywhere outward of the tip (`Some(INFINITY)`), a
    /// thread's V widens by `Some(tan(half angle))` per mm outward up to
    /// the band. `None` credits no more than the tip's own line (a form
    /// gauge's crown, an insert's nose disc).
    pub outward: Option<f64>,
}

/// A boring bar read about its nose: the head (flute) over `flute_len`
/// up the axis from the nose, the shank over `shank_len` above it, each
/// as [`crate::turning::bar_spans`] says - the insert at the head's
/// edge, the head toward the axis, a shank wider than the head reaching
/// past the nose (B4). The same spans centre the kernel's rapids.
#[derive(Clone, Copy, Debug)]
pub(super) struct BarBody {
    pub head_d: f64,
    pub shank_d: f64,
    pub flute_len: f64,
    pub shank_len: f64,
}

/// Law (d) for the turned family: the column law's word on a turned
/// path's AXIAL rapids (no XY travel) is dropped - the spindle stands
/// on world Z, the column cannot see the meridian (#96) - and each such
/// rapid is judged in the meridian plane instead. At its radius from the
/// spindle the rapid's nose disc must not meet material between its two
/// stations: the stock and the part revolved about the axis (the spindle
/// carries every angle past the insert), less what the program ahead
/// faced off (the prior grid read revolved, asked only when needed) and
/// less the nose sweep of this path's own earlier feeds - a retract
/// along a turned diameter or an approach down a bored hole stands in
/// air this op made. A body the axis fit refuses keeps the column's
/// convictions (unknown is stock).
pub(super) fn judge_axial_rapids(
    params: &OpParams,
    path: &Toolpath,
    cx: &Meridian,
    prior: &mut PriorCell,
    warns: &mut Vec<Warning>,
) {
    if params.kind().info().section != Section::Turning {
        return;
    }
    let Ok(frame) = Frame::about(cx.part) else { return };
    let sm = SignedMeridian::of(&frame, path);
    // Judged here (#103, #159): EVERY rapid of the turned family that
    // stands in its turning plane - axial, radial or both. The column
    // cannot see the meridian, and its footprint disc of the tool's
    // radius about the nose (#106) is a mill's reading: a lathe's
    // radial retract along its own kerf is not a disc's dive. The
    // column's word on these is dropped and the meridian's stands. A
    // rapid off the turning plane (none of today's kernels makes one)
    // keeps the column's word.
    let axial = |at: usize| {
        path.moves.get(at).is_some_and(|m| m.kind == MoveKind::Rapid && sm.in_plane(m.from, cx.z_tol) && sm.in_plane(m.to, cx.z_tol))
    };
    let Some(part) = Revolved::of(cx.part, &frame) else { return };
    let stock = match cx.stock {
        Some(s) => match Revolved::of(s, &frame) {
            Some(r) => Some(r),
            None => return, // an unreadable stock: the column's word stands
        },
        None => None,
    };
    warns.retain(|w| !matches!(*w, Warning::RapidInStock { at } if axial(at)));
    let mut cap: Option<Option<f64>> = None; // the faced end, read once
    for (i, m) in path.moves.iter().enumerate() {
        if !axial(i) {
            continue;
        }
        let (a, b) = (sm.sr(m.from), sm.sr(m.to));
        let hit = |cap: Option<f64>| {
            enters(a, b, i, path, &sm, &part, stock.as_ref(), cap, cx)
        };
        if !hit(None) {
            continue;
        }
        // Only a conviction pays for the program ahead's simulation.
        let cap = *cap.get_or_insert_with(|| faced_end(prior, &frame, &sm, cx));
        if cap.is_some() && !hit(cap) {
            continue;
        }
        warns.push(Warning::RapidInStock { at: i });
    }
}

/// The frame read with a SIGNED radius: the facing passes cross the
/// axis, and their sweep reaches the other side of it.
pub(super) struct SignedMeridian {
    origin: Point3,
    dir: Point3,
    radial: Point3,
}

impl SignedMeridian {
    /// The sign's half plane is the path's own: the first point off
    /// the axis (the kernels walk one plane through it).
    pub(super) fn of(f: &Frame, path: &Toolpath) -> Self {
        let origin = f.point(0.0, 0.0);
        let dir = f.point(0.0, 1.0) - origin;
        let off = |q: Point3| {
            let d = q - origin;
            let s = d.dot(dir);
            Point3::new(d.x - dir.x * s, d.y - dir.y * s, d.z - dir.z * s)
        };
        let radial = path
            .moves
            .iter()
            .map(|m| off(m.from))
            .find(|w| w.dot(*w) > 1e-12)
            .unwrap_or_else(|| f.point(1.0, 0.0) - origin);
        let n = radial.dot(radial).sqrt();
        let radial = if n > 0.0 && n.is_finite() { Point3::new(radial.x / n, radial.y / n, radial.z / n) } else { f.point(1.0, 0.0) - origin };
        Self { origin, dir, radial }
    }

    /// Does `q` stand in the path's own meridian half planes (either
    /// side of the axis), within `tol`?
    pub(super) fn in_plane(&self, q: Point3, tol: f64) -> bool {
        let d = q - self.origin;
        let s = d.dot(self.dir);
        let w = Point3::new(d.x - self.dir.x * s, d.y - self.dir.y * s, d.z - self.dir.z * s);
        let r = w.dot(self.radial);
        let off = Point3::new(w.x - self.radial.x * r, w.y - self.radial.y * r, w.z - self.radial.z * r);
        off.dot(off).sqrt() <= tol.max(1e-9) * (1.0 + r.abs())
    }

    /// `q` in the frame the tool's plane is fixed in (#1037): station,
    /// along the path's radial, and across the plane (`dir x radial`).
    pub(super) fn suv(&self, q: Point3) -> (f64, f64, f64) {
        let d = q - self.origin;
        let s = d.dot(self.dir);
        let w = Point3::new(d.x - self.dir.x * s, d.y - self.dir.y * s, d.z - self.dir.z * s);
        (s, w.dot(self.radial), w.dot(self.dir.cross(self.radial)))
    }

    /// The spindle direction and the path's signed radial (unit,
    /// world): the turning tool frame's +z and +x as a tip's (station,
    /// signed radius) is read here (#1038: the drawn lathe body).
    pub(super) fn axes(&self) -> (Point3, Point3) {
        (self.dir, self.radial)
    }

    pub(super) fn sr(&self, q: Point3) -> (f64, f64) {
        let d = q - self.origin;
        let s = d.dot(self.dir);
        let w = Point3::new(d.x - self.dir.x * s, d.y - self.dir.y * s, d.z - self.dir.z * s);
        let r = w.dot(w).sqrt();
        (s, if w.dot(self.radial) < 0.0 { -r } else { r })
    }
}

/// The station the program ahead faced the stock down to, or `None`
/// when nothing ahead cut or it cannot be read (the raw stock stands).
pub(super) fn faced_end(prior: &mut PriorCell, f: &Frame, sm: &SignedMeridian, cx: &Meridian) -> Option<f64> {
    let dex = prior.get()?;
    let c = sm.origin;
    // The whole face out to the stock's reach: the highest ring stands.
    let reach = cx.stock.unwrap_or(cx.part).iter().flat_map(|t| [t.a, t.b, t.c]).map(|q| f.sr(q).1).fold(0.0f64, f64::max);
    let top = PriorRead { dex, revolved: true }.surface(crate::geo2d::Point2::new(c.x, c.y), reach + cx.nose)?;
    top.is_finite().then(|| f.station_of(top))
}

/// Does the nose disc riding `a` -> `b` (signed meridian points) meet
/// material the cuts before move `i` left standing?
#[allow(clippy::too_many_arguments)]
fn enters(
    a: (f64, f64),
    b: (f64, f64),
    i: usize,
    path: &Toolpath,
    sm: &SignedMeridian,
    part: &Revolved,
    stock: Option<&Revolved>,
    cap: Option<f64>,
    cx: &Meridian,
) -> bool {
    let rn = if cx.nose.is_finite() && cx.nose > 0.0 { cx.nose } else { 0.0 };
    let rd = (rn - cx.margin).max(0.0);
    let band = if cx.band.is_finite() { (cx.band - cx.margin).max(0.0) } else { 0.0 };
    // The pieces swept, each a station segment and the offsets sampled
    // about it: the nose disc and blade band on the rapid itself; a
    // bar's head carried `flute_len` up the axis from it, its shank the
    // `shank_len` above that (an axial rapid keeps its radius).
    let toward = if a.1 < 0.0 { -1.0 } else { 1.0 };
    let ring: Vec<(f64, f64)> = (0..8)
        .map(|k| f64::from(k) * std::f64::consts::FRAC_PI_4)
        .flat_map(|t| [(rd * t.cos(), rd * t.sin()), (0.5 * rd * t.cos(), 0.5 * rd * t.sin())])
        .chain(std::iter::once((0.0, 0.0)))
        // The blade's edge across its band: flat for a groove or a form
        // gauge's widest reading, a thread's V rising |u| / tan(half
        // angle) outward of its tip.
        .chain(spread(band).map(|ds| (ds, match cx.outward {
            Some(k) if k.is_finite() && k > 0.0 => ds.abs() / k,
            _ => 0.0,
        })))
        .collect();
    let (s_lo, s_hi) = (a.0.min(b.0), a.0.max(b.0));
    let mut pieces = vec![(s_lo, s_hi, ring)];
    if let Some(bar) = cx.body.filter(|q| [q.head_d, q.shank_d, q.flute_len, q.shank_len].iter().all(|v| v.is_finite())) {
        let (lf, ls) = (bar.flute_len.max(0.0), bar.shank_len.max(0.0));
        let [head, shank] = crate::turning::bar_spans(bar.head_d, bar.shank_d);
        // A span shrunk by the margin, sampled across, signed toward
        // the nose's own side (the insert points away from the axis).
        let across = |(lo, hi): (f64, f64)| -> Vec<(f64, f64)> {
            let (lo, hi) = (lo + cx.margin, hi - cx.margin);
            if hi < lo {
                return Vec::new();
            }
            let mid = (lo + hi) * 0.5;
            spread((hi - lo) * 0.5).chain(std::iter::once(0.0)).map(|k| (0.0, toward * (mid + k))).collect()
        };
        pieces.push((s_lo, s_hi + lf, across(head)));
        pieces.push((s_lo + lf, s_hi + lf + ls, across(shank)));
    }
    let reach_s = rd.max(band);
    let (lo, hi) = pieces.iter().fold((f64::INFINITY, f64::NEG_INFINITY), |(l, h), p| (l.min(p.0), h.max(p.1)));
    let (lo, hi) = (lo - reach_s, hi + reach_s);
    // This path's own earlier feeds that can reach the rapid's band.
    // ...and the turning ops ahead in the setup (#159: a finish's
    // retract across the band its rough cleared), each at its own nose.
    let own = path.moves[..i.min(path.moves.len())].iter().map(|m| (m, rn, true));
    let ahead = cx.ahead.iter().flat_map(|a| a.moves.iter().map(move |m| (m, a.nose, false)));
    let wide = rd.max(band);
    let mut feeds: Vec<Kerf> = own
        .chain(ahead)
        .filter(|(m, _, _)| m.kind != MoveKind::Rapid)
        .filter_map(|(m, nose, mine)| kerf_of(m, sm, cx.z_tol).map(|(p, q)| Kerf { p, q, reach: nose + cx.margin, mine }))
        .filter(|k| k.p.0.max(k.q.0) + k.reach.max(wide) >= lo && k.p.0.min(k.q.0) - k.reach.max(wide) <= hi)
        .collect();
    // This path's own first: the flank credit reads only those.
    feeds.sort_by_key(|k| !k.mine);
    let own_n = feeds.iter().filter(|k| k.mine).count();
    let pilots = pilots(cx.drills, sm);
    let solid = |s: f64, rho: f64| {
        let r = rho.abs();
        // A blade's feed sweeps its whole band along the axis, not its
        // centre's point: THIS path's kerf is the segment widened by the
        // band. An op ahead keeps its own reach (review B2): this blade's
        // band is no measure of what another tool cut.
        let kerf = |p: (f64, f64), q: (f64, f64), reach: f64, mine: bool| {
            let width = if mine { band } else { 0.0 };
            spread(width).chain(std::iter::once(0.0)).any(|u| {
                seg_dist((s + u, rho), p, q) <= reach || seg_dist((s + u, -rho), p, q) <= reach
            })
        };
        // The blade's edge outward of its tip, along this path's own
        // feeds (the threading V behind its tip, the groove's band).
        let flank = |p: (f64, f64), q: (f64, f64)| {
            let Some(k) = cx.outward.filter(|k| *k > 0.0) else { return false };
            let len = (q.0 - p.0).hypot(q.1 - p.1);
            let m = (len / 0.25).ceil().clamp(1.0, 4000.0) as usize;
            (0..=m).any(|j| {
                let t = j as f64 / m as f64;
                let (sc, rc) = (p.0 + (q.0 - p.0) * t, (p.1 + (q.1 - p.1) * t).abs());
                let h = r - rc;
                h >= -cx.margin && (s - sc).abs() <= (h.max(0.0) * k).min(band) + cx.margin
            })
        };
        if feeds.iter().take(own_n).any(|k| flank(k.p, k.q)) {
            return false; // the blade's own V or band
        }
        if feeds.iter().any(|k| kerf(k.p, k.q, k.reach, k.mine)) {
            return false; // this path's own kerf
        }
        if pilots.iter().any(|p| p.removed(s, r)) {
            return false; // a drill ahead's hole
        }
        // The cheap outer reading first: the bodies' ray parity is
        // asked only inside their skins.
        part.solid(s, r, cx.margin)
            || (cap.is_none_or(|c| s < c - cx.margin) && stock.is_some_and(|o| o.solid(s, r, cx.margin)))
    };
    if (a.1 - b.1).abs() > cx.z_tol {
        // A rapid across radii (a radial retract, a lead): its own
        // samples carry the nose and band, and the bar's head and shank
        // up the axis from each - the pieces above are the axial
        // shortcut of the same sweep.
        let len = (b.0 - a.0).hypot(b.1 - a.1);
        let n = (len / (rn * 0.5).max(0.05).max(len / MAX_SAMPLES)).ceil().max(1.0) as usize;
        let up: Vec<(f64, Vec<(f64, f64)>)> = pieces.iter().skip(1).map(|(p0, p1, offs)| (p1 - p0 - (s_hi - s_lo), offs.clone())).collect();
        let starts = [0.0, pieces.get(1).map_or(0.0, |p| p.1 - p.0 - (s_hi - s_lo))];
        return (0..=n).any(|k| {
            let t = k as f64 / n as f64;
            let (s, rho) = (a.0 + (b.0 - a.0) * t, a.1 + (b.1 - a.1) * t);
            let flip = if (rho < 0.0) == (toward < 0.0) { 1.0 } else { -1.0 };
            pieces[0].2.iter().any(|&(ds, dr)| solid(s + ds, rho + dr))
                || up.iter().zip(starts).any(|((h, offs), h0)| {
                    let m = (h / 0.5).ceil().max(1.0) as usize;
                    (0..=m).any(|j| {
                        let hs = s + h0 + h * j as f64 / m as f64;
                        offs.iter().any(|&(_, dr)| solid(hs, rho + flip * dr))
                    })
                })
        });
    }
    pieces.iter().any(|(p0, p1, offs)| {
        let len = p1 - p0;
        let step = (rn * 0.5).max(0.05).max(len / MAX_SAMPLES);
        let n = (len / step).ceil().max(1.0) as usize;
        (0..=n).any(|k| {
            let s = p0 + len * k as f64 / n as f64;
            offs.iter().any(|&(ds, dr)| solid(s + ds, a.1 + dr))
        })
    })
}

/// A cutting move's meridian segment `p`-`q`, credited out to `reach`;
/// `mine` when it is this path's own (the blade-flank credit's).
struct Kerf {
    p: (f64, f64),
    q: (f64, f64),
    reach: f64,
    mine: bool,
}

/// A cutting move's meridian segment, credited as its nose's kerf. A
/// straight move is its chord. An arc credits only when it is a turn
/// about the spindle (an XY arc centred on the axis, both ends at one
/// radius - a thread's helix): a constant radius across its stations
/// (review S1's reading). Any other arc's chord is not its sweep, and
/// credits nothing.
///
/// *The helix approximation* (PR #135 review): a helix is a groove that
/// winds, not a tube; read as constant radius over its stations it
/// credits the crests between the turns as cut. On the thread's own path
/// the flank credit along that chord makes the V a continuous trough
/// over the threaded length. That is safe only for today's thread
/// kernel, which rapids axially outside the work and radially out of
/// its own groove at the helix's end station. A kernel that rapids
/// ACROSS the threaded length below the crests must not lean on it.
pub(super) fn kerf_of(m: &crate::cl::Move, sm: &SignedMeridian, tol: f64) -> Option<((f64, f64), (f64, f64))> {
    let (p, q) = (sm.sr(m.from), sm.sr(m.to));
    let Some(arc) = m.arc.as_ref() else { return Some((p, q)) };
    let on_axis = (arc.center.x - sm.origin.x).hypot(arc.center.y - sm.origin.y) <= tol.max(1e-9) * (1.0 + p.1.abs());
    let round = (p.1.abs() - q.1.abs()).abs() <= tol.max(1e-9) * (1.0 + p.1.abs());
    (arc.plane.is_xy() && on_axis && round).then(|| ((p.0, p.1.abs()), (q.0, q.1.abs())))
}

/// Offsets across `[-h, h]`, at most 0.25 mm apart, ends included (none
/// for `h` = 0).
pub(super) fn spread(h: f64) -> impl Iterator<Item = f64> {
    let n = if h > 0.0 { (2.0 * h / 0.25).ceil().min(4000.0) as usize } else { 0 };
    (0..=n).filter(move |_| n > 0).map(move |k| -h + 2.0 * h * k as f64 / n as f64)
}

/// Distance from `p` to the segment `a`-`b` in the meridian plane.
pub(super) fn seg_dist(p: (f64, f64), a: (f64, f64), b: (f64, f64)) -> f64 {
    let (vx, vy) = (b.0 - a.0, b.1 - a.1);
    let l2 = vx * vx + vy * vy;
    let t = if l2 > 0.0 { (((p.0 - a.0) * vx + (p.1 - a.1) * vy) / l2).clamp(0.0, 1.0) } else { 0.0 };
    (p.0 - a.0 - vx * t).hypot(p.1 - a.1 - vy * t)
}

#[cfg(test)]
#[path = "turning_rapids_tests.rs"]
mod tests;
