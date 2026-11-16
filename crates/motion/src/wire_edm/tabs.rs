//! Wire EDM tabs (#539, part of #290 D6): bridges of material the main
//! cut leaves standing so the slug (a die's) or the punch stays held
//! when the wire's cut would otherwise close, and the later tab
//! cut-off that severs them.
//!
//! Off by default. When on, the tab count, the tab width and one
//! position per tab are all stated - no value is invented, since no
//! manufacturer-validated default exists (D6) - and every impossible
//! layout is refused by name: a tab whose gap (its width plus the kerf
//! width) does not fit on one straight side of the wire path (a width
//! at or past the side, a tab on the short facets of a round), two tabs
//! that overlap or leave no cut between them, a stretch between tabs
//! the start hole cannot reach without crossing the product, a cut-off
//! condition that would cut on another offset.
//!
//! The main cut: the ring between consecutive tabs is a run. Each run
//! is entered from the threading point by its own straight lead through
//! the waste, cut both ways from its foot to the tab gaps at its ends,
//! and left by retracing the wire's own kerf back to the threading
//! point - the wire passes without cutting where it has already cut, so
//! it is never cut or rethreaded mid-program. The tab cut-off follows:
//! for each tab, the wire travels in the kerf from the threading point
//! to the tab's gap, cuts across it on the main path's own line, and
//! retraces home. The path returns to the threading point exactly once
//! per run and once per tab; `cutoff_split` reads the cut-off's first
//! move from that law.
//!
//! Provenance: general wire EDM practice as the public literature the
//! umbrella #290 cites states it - tabs (bridges) hold the slug until a
//! separate cut-off pass, and a slot the wire already burnt is free to
//! travel in - over plain 2D arc-length geometry on the wire-centre
//! ring. Our own code; no vendor post, control manual or CAM product
//! behaviour was consulted.

use crate::cl::Toolpath;
use crate::geo2d::Point2;
use crate::geometry::Point3;
use crate::machining2d::GenError;

use super::ring::{dist, pt_seg};
use super::{WireParams, MIN_LEAD, PATH_TOL};

/// The most tabs one cut states.
pub const MAX_TABS: u32 = 64;
/// How far (mm) the cut-off condition's kerf offset may differ from
/// the main cut's: 1 um.
pub const OFFSET_TOL: f64 = 1e-3;
/// The shortest stretch (mm) of cut between two tabs.
const MIN_RUN: f64 = 1e-3;
/// How near (mm) a move's end must come to the threading point to read
/// as a return to it.
const HOME_TOL: f64 = 1e-6;

/// The tabs as stated, resolved to plain numbers (mm). `None` / empty
/// is "not stated" and is refused by name.
#[derive(Clone, Debug, PartialEq)]
pub struct TabParams {
    pub count: Option<u32>,
    /// The bridge's width along the profile.
    pub width: Option<f64>,
    /// One position per tab, near the profile (world XY).
    pub at: Vec<Point2>,
    /// The cut-off condition's id and its kerf offset (wire radius plus
    /// its gap); `None` is no cut-off condition named.
    pub cutoff: Option<(String, f64)>,
}

fn invalid(why: String) -> GenError {
    GenError::Invalid(format!("wire contour: {why}"))
}

/// The door: count, width, positions and cut-off condition, by name.
pub(super) fn door(t: &TabParams, k: f64) -> Result<(u32, f64, String), GenError> {
    let Some(count) = t.count else {
        return Err(invalid("tabs are on but the tab count is not stated - state how many tabs hold the slug (there is no default)".into()));
    };
    if count == 0 || count > MAX_TABS {
        return Err(invalid(format!("the tab count must be 1 to {MAX_TABS} (it states {count}) - for no tabs, turn tabs off")));
    }
    let Some(width) = t.width else {
        return Err(invalid("tabs are on but the tab width is not stated - state the bridge width in mm (there is no default)".into()));
    };
    if !(width.is_finite() && width > 0.0) {
        return Err(invalid(format!("the tab width must be a positive number of mm (it states {width})")));
    }
    if t.at.is_empty() {
        return Err(invalid("tabs are on but no tab position is stated - give each tab its place on the profile".into()));
    }
    if t.at.len() != count as usize {
        return Err(invalid(format!(
            "the tab count is {count} but {} tab position(s) are stated - each tab needs exactly one position",
            t.at.len()
        )));
    }
    if t.at.iter().any(|p| !(p.x.is_finite() && p.y.is_finite())) {
        return Err(invalid("a tab position is not a finite point".into()));
    }
    let Some((id, k2)) = &t.cutoff else {
        return Err(invalid("tabs are on but no tab cut-off condition is named - the cut-off that frees the slug burns with a condition of its own".into()));
    };
    if !(k2.is_finite() && (k2 - k).abs() <= OFFSET_TOL) {
        return Err(invalid(format!(
            "the tab cut-off's condition {id} cuts at a kerf offset of {k2:.4} mm and the main cut at {k:.4} mm - the cut-off runs on the main cut's path, so its wire and gap must give the same offset"
        )));
    }
    Ok((count, width, id.clone()))
}

/// The wire-centre ring by arc length.
struct Ring<'a> {
    pts: &'a [Point2],
    cum: Vec<f64>,
    len: f64,
}

impl<'a> Ring<'a> {
    fn new(pts: &'a [Point2]) -> Self {
        let n = pts.len();
        let mut cum = Vec::with_capacity(n + 1);
        let mut s = 0.0;
        cum.push(0.0);
        for i in 0..n {
            s += dist(pts[i], pts[(i + 1) % n]);
            cum.push(s);
        }
        Ring { pts, cum, len: s }
    }

    /// The point at arc length `s` (any real; wrapped).
    fn at(&self, s: f64) -> Point2 {
        let n = self.pts.len();
        let s = s.rem_euclid(self.len);
        let i = self.cum.partition_point(|c| *c <= s).saturating_sub(1).min(n - 1);
        let (a, b) = (self.pts[i], self.pts[(i + 1) % n]);
        let l = self.cum[i + 1] - self.cum[i];
        let t = if l > 0.0 { ((s - self.cum[i]) / l).clamp(0.0, 1.0) } else { 0.0 };
        Point2::new(a.x + t * (b.x - a.x), a.y + t * (b.y - a.y))
    }

    /// The vertices strictly between `a` and `b` (`a <= b`, unwrapped),
    /// counter-clockwise, with their arc lengths.
    fn between(&self, a: f64, b: f64) -> Vec<(f64, Point2)> {
        let n = self.pts.len();
        let mut out = Vec::new();
        let mut lap = (a / self.len).floor() * self.len;
        while lap < b {
            for i in 0..n {
                let s = lap + self.cum[i];
                if s > a + 1e-9 && s < b - 1e-9 {
                    out.push((s, self.pts[i]));
                }
            }
            lap += self.len;
        }
        out
    }

    /// The nearest point of the stretch `a..b` to `p`: its arc length.
    fn nearest_on(&self, a: f64, b: f64, p: Point2) -> f64 {
        let mut pts = vec![(a, self.at(a))];
        pts.extend(self.between(a, b));
        pts.push((b, self.at(b)));
        let mut best = (a, f64::INFINITY);
        for w in pts.windows(2) {
            let ((sa, pa), (sb, pb)) = (w[0], w[1]);
            let (dx, dy) = (pb.x - pa.x, pb.y - pa.y);
            let dd = dx * dx + dy * dy;
            let t = if dd > 0.0 { (((p.x - pa.x) * dx + (p.y - pa.y) * dy) / dd).clamp(0.0, 1.0) } else { 0.0 };
            let d = dist(p, Point2::new(pa.x + t * dx, pa.y + t * dy));
            if d < best.1 {
                best = (sa + t * (sb - sa), d);
            }
        }
        best.0
    }

    /// Walk from `a` to `b` along the ring (either direction): the
    /// points after `a`, ending at `b`.
    fn walk(&self, a: f64, b: f64) -> Vec<Point2> {
        let mut v: Vec<Point2> = if a <= b {
            self.between(a, b).into_iter().map(|(_, p)| p).collect()
        } else {
            self.between(b, a).into_iter().rev().map(|(_, p)| p).collect()
        };
        v.push(self.at(b));
        v
    }
}

/// One tab's gap in the wire path, by arc length (`lo < hi`).
#[derive(Clone, Copy)]
struct Gap {
    no: usize,
    lo: f64,
    hi: f64,
}

fn seg_seg(a: Point2, b: Point2, c: Point2, d: Point2) -> f64 {
    let cross = |o: Point2, p: Point2, q: Point2| (p.x - o.x) * (q.y - o.y) - (p.y - o.y) * (q.x - o.x);
    let (d1, d2, d3, d4) = (cross(c, d, a), cross(c, d, b), cross(a, b, c), cross(a, b, d));
    if ((d1 > 0.0) != (d2 > 0.0)) && ((d3 > 0.0) != (d4 > 0.0)) {
        return 0.0;
    }
    pt_seg(a, c, d).min(pt_seg(b, c, d)).min(pt_seg(c, a, b)).min(pt_seg(d, a, b))
}

/// The tabbed path's points after the threading point: the main cut's
/// and the cut-off's, and the notes that say them.
pub(super) struct Tabbed {
    pub main: Vec<Point2>,
    pub cutoff: Vec<Point2>,
    pub notes: Vec<String>,
}

/// Lay the tabs on the counter-clockwise wire ring and walk the main
/// cut and the cut-off. `given` is the profile as given (the product's
/// edge the leads must stay clear of).
pub(super) fn tabbed(ring: &[Point2], given: &[Point2], start: Point2, p: &WireParams, t: &TabParams) -> Result<Tabbed, GenError> {
    let k = p.kerf_offset();
    let (count, width, cutoff_id) = door(t, k)?;
    let r = Ring::new(ring);
    if !(r.len.is_finite() && r.len > 0.0) {
        return Err(invalid("the wire path has no length to lay tabs on".into()));
    }
    let h = width * 0.5 + k;
    let mut gaps = Vec::with_capacity(count as usize);
    for (i, at) in t.at.iter().enumerate() {
        let no = i + 1;
        let (seg, foot) = super::ring::nearest(ring, *at);
        let off = dist(*at, foot);
        if off > k + width + 1e-9 {
            return Err(invalid(format!(
                "tab {no} at X{:.3} Y{:.3} stands {off:.3} mm from the wire path - place it on the profile",
                at.x, at.y
            )));
        }
        let (s0, s1) = (r.cum[seg], r.cum[seg + 1]);
        let c = s0 + dist(ring[seg], foot);
        if c - h < s0 - 1e-9 || c + h > s1 + 1e-9 {
            let why = if s1 - s0 < 2.0 * h {
                format!("the side there is only {:.3} mm long (a tab is not laid across a corner or on the facets of a round)", s1 - s0)
            } else {
                "it runs past the side's end - move it along the side".to_owned()
            };
            return Err(invalid(format!(
                "tab {no} at X{:.3} Y{:.3} does not fit on one straight side of the wire path: its gap is {:.3} mm (width {width:.3} plus the kerf width {:.3}) and {why}",
                at.x,
                at.y,
                2.0 * h,
                2.0 * k
            )));
        }
        gaps.push(Gap { no, lo: c - h, hi: c + h });
    }
    gaps.sort_by(|a, b| a.lo.total_cmp(&b.lo));
    let n = gaps.len();
    for i in 0..n {
        let (g, next) = (gaps[i], gaps[(i + 1) % n]);
        let next_lo = if i + 1 == n { next.lo + r.len } else { next.lo };
        if next_lo - g.hi < MIN_RUN {
            return Err(invalid(if n == 1 {
                format!("tab {} leaves no profile to cut - its gap takes the whole path", g.no)
            } else {
                format!("tabs {} and {} overlap or leave no cut between them - spread them apart", g.no.min(next.no), g.no.max(next.no))
            }));
        }
    }
    let mut main = Vec::new();
    let mut cutoff = Vec::new();
    let push = |v: &mut Vec<Point2>, q: Point2| {
        if v.last().is_none_or(|l| dist(*l, q) > 1e-9) {
            v.push(q);
        }
    };
    // Each run: the stretch from one gap's end to the next gap's start.
    let mut feet: Vec<(f64, f64, f64)> = Vec::with_capacity(n);
    for i in 0..n {
        let a = gaps[i].hi;
        let b = if i + 1 == n { gaps[0].lo + r.len } else { gaps[i + 1].lo };
        // The foot is the run's nearest point to the threading point,
        // kept a kerf offset clear of the gaps where the run allows.
        let (lo, hi) = if b - a > 2.0 * k { (a + k, b - k) } else { (0.5 * (a + b), 0.5 * (a + b)) };
        let f = r.nearest_on(lo, hi, start);
        let foot = r.at(f);
        let lead = dist(start, foot);
        let clear = given.iter().enumerate().map(|(j, q)| seg_seg(start, foot, *q, given[(j + 1) % given.len()])).fold(f64::INFINITY, f64::min);
        if !(lead.is_finite() && lead > MIN_LEAD && clear >= k - PATH_TOL) {
            return Err(GenError::invalid_at(
                format!(
                    "wire contour: the stretch between tabs {} and {} cannot be reached from the start hole by a straight lead without the kerf entering the {} - place the tabs so every stretch between them faces the start hole",
                    gaps[i].no,
                    gaps[(i + 1) % n].no,
                    p.side.word()
                ),
                Point3::new(foot.x, foot.y, p.plane_z),
            ));
        }
        for q in std::iter::once(foot).chain(r.walk(f, b)).chain(r.walk(b, f)).chain(r.walk(f, a)).chain(r.walk(a, f)) {
            push(&mut main, q);
        }
        push(&mut main, start);
        feet.push((a, f, b));
    }
    // The cut-off: each tab reached along whichever run beside it has
    // its foot nearer, cut across on the main path's line, retraced.
    for i in 0..n {
        let g = gaps[i];
        let (_, fp, bp) = feet[(i + n - 1) % n];
        let (an, fn_, _) = feet[i];
        let (near, end, across) = if bp - fp <= fn_ - an {
            // From the run before: its end is this gap's start.
            (fp, bp, if i == 0 { g.hi + r.len } else { g.hi })
        } else {
            (fn_, an, g.lo)
        };
        for q in std::iter::once(r.at(near)).chain(r.walk(near, end)).chain([r.at(across), r.at(end)]).chain(r.walk(end, near)) {
            push(&mut cutoff, q);
        }
        push(&mut cutoff, start);
    }
    let notes = vec![
        format!(
            "wire EDM: {count} tab(s) of {width:.3} mm left in the main cut (each a gap of {:.3} mm in the wire path at {}) - the slug is held; each stretch between tabs is entered by its own straight lead from the threading point and left back along its own kerf",
            2.0 * h,
            t.at.iter().map(|q| format!("X{:.3} Y{:.3}", q.x, q.y)).collect::<Vec<_>>().join(", ")
        ),
        format!(
            "wire EDM: the tab cut-off follows the main cut with condition {cutoff_id} - the wire travels in the kerf to each tab, cuts across it on the main path's line and retraces; it frees the slug"
        ),
    ];
    Ok(Tabbed { main, cutoff, notes })
}

/// Where a tabbed path's cut-off begins: the move after the main cut's
/// last return to the threading point. The path returns there once per
/// run and once per tab (module head); any other count is not a tabbed
/// path of `tabs` tabs, and reads `None`.
#[must_use]
pub fn cutoff_split(path: &Toolpath, start: Point2, tabs: usize) -> Option<usize> {
    if tabs == 0 {
        return None;
    }
    let home: Vec<usize> = path
        .moves
        .iter()
        .enumerate()
        .filter(|(_, m)| (m.to.x - start.x).hypot(m.to.y - start.y) <= HOME_TOL)
        .map(|(i, _)| i)
        .collect();
    let last = path.moves.len().checked_sub(1)?;
    (home.len() == 2 * tabs && home.last() == Some(&last)).then(|| home.get(tabs - 1).map(|i| i + 1)).flatten()
}
