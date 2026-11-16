//! Uncut stock under overhangs (M13.7 B): the solid-law backstop of the reach laws (c), (c3, over its swept cells since #1316) and rapid law (d) - stock a plane section cannot show because part material hangs over it, read from the model's down-facing shadow (the projection identity of a closed solid's boundary, `slice::shadow_above`) on our own conservative rest model; no vendor source.
//!
//! The plane-section laws ask "is the part's section at height h over
//! this column, or was the column never cut down to h?" - and they only
//! ask it at the section walls and along the rapid (#140 marches its
//! whole length; for a flat-bottomed tool on a straight lateral rapid the
//! march already sees what this module would - it still matters for
//! shanks and for ball/bull tips near the rim). Stock under
//! a ledge stands OUTSIDE the section at h (the ledge is higher), so its
//! columns are never sampled: a shank or a rapid sliding under the ledge
//! through that stock was silent. This module reads it as a solid:
//!
//! (q, h) is hidden stock iff q is inside the stock footprint, h is
//! under the stock top, part material hangs over (q, h)
//! (`shadow_above(h)` holds q), the part itself is not at (q, h) (the
//! section at h does not hold q - that is the solid law b2's steel), and
//! no cutting chord of a move BEFORE the judged one, at a plane at or
//! below h, passed within the tool radius of q on a plane whose section
//! left q open (the column clear law of [`Rest::surface_before`], read
//! at h instead of as a surface height; cut order per M13.7).
//!
//! The stock solid is therefore "stock minus what earlier cuts of the
//! same path swept", with the sweep read the way `surface` reads it: a
//! chord clears its disc's column from its plane up. Exact in Z, sampled
//! in XY - the hidden region's boundary every [`STEP`] (nudged a hair
//! inside) and rings in the band annulus / lines along the rapid's disc.
//! A hidden sliver thinner than the sampling can be missed; a slice that
//! fails convicts (fail closed). It only ever ADDS convictions: every
//! caller asks it only where its plane-section law found nothing.
//!
//! One plane per ask is sound only for a caller that stands at one
//! height. The law is not monotone in `h` (the shadow shrinks as the
//! plane rises, the section comes and goes: a cavity over a floor is
//! hidden at its own planes and part material below them), so the swept
//! backstop ([`Rest::hidden_in_cover`], #1316) also reads every column
//! it asks over the whole range of heights the band's bottom takes
//! there, exactly in Z ([`super::hidden_span`]).

use super::laws::dist2_seg;
use super::rest::Rest;

use crate::geo2d::{Point2, Region};
use std::rc::Rc;

/// XY sample spacing of the hidden region's boundary and the rapid's
/// disc lines, mm.
const STEP: f64 = 0.5;
/// Inward nudge of a boundary sample, mm (membership unambiguous).
const NUDGE: f64 = 0.05;
/// Most boundary samples one plane keeps; past it the spacing widens.
const MAX_SAMPLES: usize = 50_000;
/// Ring sample count per radius in a band annulus.
const RING: usize = 24;

/// Most lattice columns one swept ask reads; past it the pitch widens.
const MAX_LATTICE: usize = 20_000;

/// The ranges of heights a swept footprint's bottom takes over a point
/// (`turn_hidden`'s `Ask::heights`): asks `hit(lo, hi)` once per closed
/// range and returns at the first `true`; no range, no cover.
pub(super) type Heights<'a> = dyn Fn(Point2, &mut dyn FnMut(f64, f64) -> bool) -> bool + 'a;

/// One plane's hidden stock: the region and its inside boundary samples.
pub(super) struct Hidden {
    region: Region,
    bounds: Option<(Point2, Point2)>,
    /// The box of everything part material hangs over above the plane,
    /// inside the stock footprint. The shadow only shrinks as the plane
    /// rises, so it holds the hidden stock of every higher plane too.
    over: Option<(Point2, Point2)>,
    samples: Vec<Point2>,
}

/// The region's boundary every `STEP`, each point nudged to whichever
/// side of its edge the region holds (loop orientation not assumed).
fn inside_samples(region: &Region) -> Vec<Point2> {
    let total: f64 = region
        .loops()
        .iter()
        .map(|l| {
            let p = l.points();
            (0..p.len())
                .map(|k| dist2_seg(p[k], p[(k + 1) % p.len()], p[(k + 1) % p.len()]).sqrt())
                .sum::<f64>()
        })
        .sum();
    let step = if total.is_finite() && total / STEP > MAX_SAMPLES as f64 { total / MAX_SAMPLES as f64 } else { STEP };
    let mut out = Vec::new();
    for l in region.loops() {
        let p = l.points();
        for k in 0..p.len() {
            let (a, b) = (p[k], p[(k + 1) % p.len()]);
            let (dx, dy) = (b.x - a.x, b.y - a.y);
            let len = (dx * dx + dy * dy).sqrt();
            if !(len.is_finite() && len > 0.0) {
                continue;
            }
            let (nx, ny) = (-dy / len, dx / len);
            let n = ((len / step).ceil() as usize).clamp(1, MAX_SAMPLES);
            for i in 0..n {
                let t = (i as f64 + 0.5) / n as f64;
                let q = Point2::new(a.x + dx * t, a.y + dy * t);
                for s in [1.0, -1.0] {
                    let c = Point2::new(q.x + s * NUDGE * nx, q.y + s * NUDGE * ny);
                    if region.contains(c) {
                        out.push(c);
                        break;
                    }
                }
            }
        }
    }
    out
}

impl<'a> Rest<'_, 'a> {
    /// The hidden stock at plane `h` (cached by its bits): the shadow of
    /// what hangs over `h`, inside the stock footprint, less the section
    /// at `h`. `None` is a slice or clip that failed.
    fn hidden_at(&self, h: f64) -> Option<Rc<Hidden>> {
        let key = h.to_bits();
        if let Some(hit) = self.hidden_cache.borrow().get(&key) {
            return hit.clone();
        }
        let cx = self.cx();
        let fresh = cx.model.and_then(|f| {
            let hang = crate::geometry::slice::shadow_above(f, h).ok()?;
            let (region, over) = if hang.is_empty() {
                (hang, None)
            } else {
                let foot = cx.stock.unwrap_or(cx.region);
                let sec = self.islands_at(f, h)?;
                let shade = hang.intersect(foot).ok()?;
                let over = shade.bounds();
                (shade.difference(&sec).ok()?, over)
            };
            let samples = inside_samples(&region);
            Some(Rc::new(Hidden { bounds: region.bounds(), over, region, samples }))
        });
        self.hidden_cache.borrow_mut().entry(key).or_insert_with(|| fresh.clone());
        fresh
    }

    /// Whether (q, h) is uncut stock before move `upto`: hidden at `h`
    /// and in no cleared column of a cutting chord at or below `h` whose
    /// move comes before `upto` (M13.7 cut order - stock a LATER move
    /// removes still stands when this move passes).
    fn uncut(&self, hid: &Hidden, q: Point2, h: f64, upto: usize) -> bool {
        let cx = self.cx();
        if !(hid.region.contains(q) && self.in_stock(q)) {
            return false;
        }
        let top = h + cx.z_tol;
        // Candidates walk deepest first, so the planes at or below h are
        // a prefix of the walk.
        !self.near(q, 0).take_while(|c| c.2 <= top).any(|c| c.3 < upto && self.clears(c, q))
    }

    /// The plane's hidden stock when it can stand within `reach` of the
    /// box [lo, hi]: `Err(())` fails closed, `Ok(None)` is nothing there.
    fn hidden_near(&self, h: f64, lo: Point2, hi: Point2, reach: f64) -> Result<Option<Rc<Hidden>>, ()> {
        let cx = self.cx();
        if cx.model.is_none() || !h.is_finite() || h >= cx.stock_top_z {
            return Ok(None);
        }
        let hid = self.hidden_at(h).ok_or(())?;
        match hid.bounds {
            Some((b0, b1))
                if b0.x <= hi.x + reach && b1.x >= lo.x - reach && b0.y <= hi.y + reach && b1.y >= lo.y - reach =>
            {
                Ok(Some(hid))
            }
            _ => Ok(None),
        }
    }

    /// Law (c)/(c3) backstop: uncut hidden stock inside the band's disc
    /// `|q - pc| <= r_out` but outside the cutter's own column
    /// `|q - tip| <= r_in` at the band's bottom `h`, against the cuts of
    /// the moves before `upto`. Upright, `tip == pc` and this is the
    /// annulus it always was; leaning (#903), the band's disc moves off
    /// the tip and the flutes' kerf stays where the tip cut - the disc
    /// under a leaning band is not material the cutter cleared.
    pub(super) fn hidden_in_annulus(
        &self,
        pc: Point2,
        tip: Point2,
        r_in: f64,
        r_out: f64,
        h: f64,
        upto: usize,
    ) -> bool {
        if !(r_out > 0.0 && pc.x.is_finite() && pc.y.is_finite() && tip.x.is_finite() && tip.y.is_finite()) {
            return false;
        }
        let upright = pc == tip;
        if upright && r_out <= r_in {
            return false;
        }
        let hid = match self.hidden_near(h, pc, pc, r_out) {
            Err(()) => return true,
            Ok(None) => return false,
            Ok(Some(hid)) => hid,
        };
        let (i2, o2) = (r_in * r_in, (r_out + 1e-9) * (r_out + 1e-9));
        let in_ring = |q: &Point2| dist2_seg(*q, pc, pc) <= o2 && dist2_seg(*q, tip, tip) > i2;
        // Upright, the three rings stand in the annulus as they always
        // did; leaning, the excluded disc is off-centre, so the probe
        // rings cover the whole band disc (the filter drops the kerf).
        let radii: Vec<f64> = if upright {
            vec![r_in + (r_out - r_in) * 0.02, (r_in + r_out) * 0.5, r_out]
        } else {
            vec![r_out * 0.25, r_out * 0.5, r_out * 0.75, r_out]
        };
        let centre = (!upright).then_some(pc);
        let rings = centre.into_iter().chain(radii.into_iter().flat_map(|rad| {
            (0..RING).map(move |k| {
                let t = std::f64::consts::TAU * k as f64 / RING as f64;
                Point2::new(pc.x + rad * t.cos(), pc.y + rad * t.sin())
            })
        }));
        rings
            .chain(hid.samples.iter().copied())
            .filter(in_ring)
            .any(|q| self.uncut(&hid, q, h, upto))
    }

    /// Law (c3) backstop (#1316): uncut hidden stock under a swept
    /// footprint inside the box [lo, hi], against the cuts of the moves
    /// before `upto`. `heights` is the footprint's exact membership AND
    /// the heights its bottom takes there (the swept cells' own test,
    /// the flutes' kerf excluded where the tip stands); `h` is the
    /// lowest of them all. Two readings, either convicts:
    ///
    /// - the plane `h`, as a region: its boundary samples in the box and
    ///   `probes` (points of the footprint itself, built on demand) -
    ///   the samples catch hidden stock that clips the footprint, the
    ///   probes stock that swallows it;
    /// - every column asked, over the whole of its ranges
    ///   ([`super::hidden_span`], exact in Z): the hidden law is not
    ///   monotone in the plane, so stock hidden only ABOVE `h` (a cavity
    ///   over a floor that `h` stands in) is in no reading of `h`. The
    ///   columns are the probes, the plane's samples and a lattice of
    ///   pitch [`STEP`] over the box, clipped to where part material
    ///   hangs over anything from `h` up - so hidden stock [`STEP`]·√2
    ///   across or wider is met at whatever height the band passes it,
    ///   however thin in Z.
    ///
    /// A slice or a vertical that cannot be read convicts (fail closed).
    pub(super) fn hidden_in_cover(
        &self,
        (lo, hi): (Point2, Point2),
        h: f64,
        probes: &dyn Fn() -> Vec<Point2>,
        heights: &Heights<'_>,
        upto: usize,
        columns: &super::hidden_span::Columns<'a>,
    ) -> bool {
        if !(lo.x.is_finite() && lo.y.is_finite() && hi.x.is_finite() && hi.y.is_finite()) {
            return false; // the NonFinite law names it
        }
        let cx = self.cx();
        if cx.model.is_none() || !h.is_finite() || h >= cx.stock_top_z {
            return false; // nothing hidden at `h` or anywhere above it
        }
        let Some(hid) = self.hidden_at(h) else {
            return true;
        };
        let near = |b: Option<(Point2, Point2)>| {
            b.filter(|(b0, b1)| b0.x <= hi.x && b1.x >= lo.x && b0.y <= hi.y && b1.y >= lo.y)
        };
        // Nothing hangs over the box from `h` up: nothing hidden in it.
        let Some((o0, o1)) = near(hid.over) else {
            return false;
        };
        let in_box = |q: &Point2| q.x >= lo.x && q.x <= hi.x && q.y >= lo.y && q.y <= hi.y;
        let covers = |q: &Point2| heights(*q, &mut |_, _| true);
        let asked: Vec<Point2> = probes().into_iter().chain(hid.samples.iter().copied().filter(in_box)).collect();
        if near(hid.bounds).is_some() && asked.iter().filter(|q| covers(q)).any(|q| self.uncut(&hid, *q, h, upto)) {
            return true;
        }
        let (c0, c1) = (Point2::new(lo.x.max(o0.x), lo.y.max(o0.y)), Point2::new(hi.x.min(o1.x), hi.y.min(o1.y)));
        let shaded = |q: &Point2| q.x >= c0.x && q.x <= c1.x && q.y >= c0.y && q.y <= c1.y;
        asked.into_iter().filter(shaded).chain(lattice(c0, c1)).any(|q| match self.hidden_column(columns, q, upto) {
            Ok(None) => false,
            Ok(Some(col)) => heights(q, &mut |a, b| col.hidden_in(a, b)),
            Err(()) => covers(&q),
        })
    }

    /// Law (d) backstop: uncut hidden stock inside the tool disc of
    /// half-width `w` swept from `a` to `b` at height `h`, against the
    /// cuts of the moves before `upto`.
    pub(super) fn hidden_along(&self, a: Point2, b: Point2, h: f64, w: f64, upto: usize) -> bool {
        let w = if w.is_finite() { w.max(0.0) } else { 0.0 };
        let lo = Point2::new(a.x.min(b.x), a.y.min(b.y));
        let hi = Point2::new(a.x.max(b.x), a.y.max(b.y));
        let hid = match self.hidden_near(h, lo, hi, w) {
            Err(()) => return true,
            Ok(None) => return false,
            Ok(Some(hid)) => hid,
        };
        let (dx, dy) = (b.x - a.x, b.y - a.y);
        let len = (dx * dx + dy * dy).sqrt();
        if !len.is_finite() {
            return true; // the NonFinite law names it; never a silent pass
        }
        let (nx, ny) = if len > 0.0 { (-dy / len, dx / len) } else { (0.0, 0.0) };
        let n = ((len / STEP).ceil() as usize).clamp(1, 4096);
        let w2 = (w + 1e-9) * (w + 1e-9);
        let lines = (0..=n).flat_map(|k| {
            let t = k as f64 / n as f64;
            let c = Point2::new(a.x + dx * t, a.y + dy * t);
            [0.0, 0.5, 1.0, -0.5, -1.0].map(|s| Point2::new(c.x + s * w * nx, c.y + s * w * ny))
        });
        lines
            .chain(hid.samples.iter().copied().filter(|q| dist2_seg(*q, a, b) <= w2))
            .any(|q| self.uncut(&hid, q, h, upto))
    }
}

/// A lattice over the box [lo, hi]: pitch [`STEP`], widened so the box
/// holds at most [`MAX_LATTICE`] points; both ends of each axis are on
/// it, so a box thinner than the pitch is still read.
fn lattice(lo: Point2, hi: Point2) -> impl Iterator<Item = Point2> {
    let (w, d) = ((hi.x - lo.x).max(0.0), (hi.y - lo.y).max(0.0));
    let fits = |step: f64| ((w / step).ceil() + 1.0) * ((d / step).ceil() + 1.0) <= MAX_LATTICE as f64;
    let mut step = STEP;
    while step.is_finite() && !fits(step) {
        step *= 1.5;
    }
    let count = |len: f64| if step.is_finite() { ((len / step).ceil() as usize).min(MAX_LATTICE) } else { 0 };
    let (nx, ny) = (count(w), count(d));
    let at = |a: f64, len: f64, k: usize, n: usize| if n == 0 { a } else { a + len * k as f64 / n as f64 };
    (0..=ny).flat_map(move |j| (0..=nx).map(move |i| Point2::new(at(lo.x, w, i, nx), at(lo.y, d, j, ny))))
}
