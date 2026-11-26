//! Waterjet tabs (#1883): short uncut gaps in a closed contour's cut
//! that hold the part in the sheet. Each tab is laid on the jet-centre
//! ring nearest its stated position, as a gap of its width plus the
//! kerf; the ring between gaps is cut as open stretches, each pierced
//! and led in on its own.
//!
//! Provenance: general waterjet and laser practice (tabs or
//! micro-joints hold a part in the skeleton) over plain arc-length
//! geometry on the ring ([`crate::sheet::arclen`]). Our own code.

use crate::geo2d::Point2;
use crate::machining2d::GenError;
use crate::sheet::arclen::ArcRing;
use crate::sheet::laws::pt_seg;
use crate::sheet::ring::{dist, nearest, unit};
use crate::sheet::Voice;

use super::{JetTabs, MAX_TABS};

/// The shortest stretch (mm) of cut between two tabs.
const MIN_RUN: f64 = 1e-3;

fn invalid(why: String) -> GenError {
    Voice::WATERJET.invalid(why)
}

/// The door: count, width and positions, by name.
pub(super) fn door(t: &JetTabs) -> Result<(), GenError> {
    if t.count == 0 || t.count > MAX_TABS {
        return Err(invalid(format!("the tab count must be 1 to {MAX_TABS} (it states {}) - for no tabs, turn tabs off", t.count)));
    }
    if !(t.width.is_finite() && t.width > 0.0) {
        return Err(invalid(format!("the tab width must be a positive number of mm (it states {})", t.width)));
    }
    if t.at.len() != t.count as usize {
        return Err(invalid(format!(
            "the tab count is {} but {} tab position(s) are stated - each tab needs exactly one position",
            t.count,
            t.at.len()
        )));
    }
    if t.at.iter().any(|p| !(p.x.is_finite() && p.y.is_finite())) {
        return Err(invalid("a tab position is not a finite point".into()));
    }
    Ok(())
}

/// One tab's gap on a ring, by arc length on the counter-clockwise ring
/// (`lo < hi`, `lo` may be negative or `hi` past the length: wrapped).
#[derive(Clone, Copy, Debug)]
pub(super) struct Gap {
    pub no: usize,
    pub lo: f64,
    pub hi: f64,
    /// The gap's middle on the ring (the cover law's exemption centre).
    pub mid: Point2,
    /// Half the gap: half the width plus the kerf radius.
    pub half: f64,
}

/// Lay every tab on the ring nearest it. `rings[k]` is the jet-centre
/// ring of contour `k` (counter-clockwise), `None` for an open run.
/// Returns the gaps per contour, sorted along the ring.
pub(super) fn lay(t: &JetTabs, rings: &[Option<Vec<Point2>>], r: f64) -> Result<Vec<Vec<Gap>>, GenError> {
    let mut out: Vec<Vec<Gap>> = vec![Vec::new(); rings.len()];
    let h = t.width * 0.5 + r;
    for (i, at) in t.at.iter().enumerate() {
        let no = i + 1;
        let best = rings
            .iter()
            .enumerate()
            .filter_map(|(k, ring)| ring.as_ref().filter(|g| g.len() >= 3).map(|g| (k, nearest(g, *at))))
            .map(|(k, (seg, foot))| (k, seg, foot, dist(*at, foot)))
            .min_by(|a, b| a.3.total_cmp(&b.3));
        let Some((k, seg, foot, off)) = best else {
            return Err(invalid(format!("tab {no} has no closed contour to lie on - tabs hold closed contours only")));
        };
        if !(off.is_finite() && off <= r + t.width + 1e-9) {
            return Err(invalid(format!(
                "tab {no} at X{:.3} Y{:.3} stands {off:.3} mm from the nearest cut - place it on a contour's edge",
                at.x, at.y
            )));
        }
        let Some(ring) = rings.get(k).and_then(Option::as_ref) else { continue };
        let a = ArcRing::new(ring);
        let c = a.cum.get(seg).copied().unwrap_or(0.0) + dist(ring[seg], foot);
        out[k].push(Gap { no, lo: c - h, hi: c + h, mid: foot, half: h });
    }
    for (k, gaps) in out.iter_mut().enumerate() {
        if gaps.is_empty() {
            continue;
        }
        let Some(ring) = rings.get(k).and_then(Option::as_ref) else { continue };
        let len = ArcRing::new(ring).len;
        gaps.sort_by(|a, b| a.lo.total_cmp(&b.lo));
        let n = gaps.len();
        for i in 0..n {
            let (g, next) = (gaps[i], gaps[(i + 1) % n]);
            let next_lo = if i + 1 == n { next.lo + len } else { next.lo };
            if next_lo - g.hi < MIN_RUN {
                return Err(invalid(if n == 1 {
                    format!("tab {} leaves no contour to cut - its gap ({:.3} mm) takes the whole cut", g.no, 2.0 * g.half)
                } else {
                    format!("tabs {} and {} overlap or leave no cut between them - spread them apart", g.no.min(next.no), g.no.max(next.no))
                }));
            }
        }
    }
    Ok(out)
}

/// The stretches of a ring between its gaps, each as points in the
/// direction of travel (`ccw`: along the ring, else against it).
pub(super) fn stretches(ring: &[Point2], gaps: &[Gap], ccw: bool) -> Vec<Vec<Point2>> {
    let a = ArcRing::new(ring);
    let n = gaps.len();
    let mut out = Vec::with_capacity(n);
    for i in 0..n {
        let from = gaps[i].hi;
        let to = if i + 1 == n { gaps[0].lo + a.len } else { gaps[i + 1].lo };
        let mut pts = vec![a.at(from)];
        for q in a.walk(from, to) {
            if pts.last().is_none_or(|l| dist(*l, q) > 1e-9) {
                pts.push(q);
            }
        }
        if !ccw {
            pts.reverse();
        }
        if pts.len() >= 2 {
            out.push(pts);
        }
    }
    out
}

/// Is `q` (a point of the part edge) under one of the gaps' bridges?
pub(super) fn bridged(gaps: &[Gap], q: Point2) -> bool {
    gaps.iter().any(|g| dist(g.mid, q) <= g.half)
}

/// How far (mm) from the part edge the bridge law walks out: past the
/// far kerf of the longest lead, straight or a quarter arc (whose far
/// point stands 4 / pi of its length off the ring), with slack.
pub(super) fn bridge_reach(kerf: f64, lead_in: f64, lead_out: f64) -> f64 {
    2.0 * kerf + lead_in.max(lead_out) * 4.0 / std::f64::consts::PI + 0.1
}

/// The bridge law: each tab must join the part to the sheet. From the
/// part edge under the gap's middle, a line runs out along the waste
/// normal for `reach` mm; no lit chord may come within the kerf radius
/// `r` of it. Such a line is a strip of uncut material from the part to
/// beyond every lead, so the tab holds. `edge` is the contour's part
/// edge, `chords` every lit move as chords. Returns the first tab whose
/// bridge is cut, with the point where the kerf crosses it.
pub(super) fn bridge_law(gaps: &[Gap], edge: &[Point2], chords: &[(Point2, Point2)], r: f64, reach: f64) -> Result<(), (usize, Point2)> {
    if edge.len() < 3 || !(reach.is_finite() && reach > 0.0) {
        return Ok(());
    }
    let step = (r * 0.25).clamp(1e-3, 0.05);
    let n_steps = (reach / step).ceil().min(1e6) as u32;
    for g in gaps {
        let (_, foot) = nearest(edge, g.mid);
        let n = unit(foot, g.mid);
        for k in 0..=n_steps {
            let v = f64::from(k) * step;
            let q = Point2::new(foot.x + n.x * v, foot.y + n.y * v);
            let d = chords.iter().map(|(a, b)| pt_seg(q, *a, *b)).fold(f64::INFINITY, f64::min);
            if d <= r {
                return Err((g.no, q));
            }
        }
    }
    Ok(())
}
