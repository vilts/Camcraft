//! Law (g), the undercut cutters (#1444): no Z move with the head in or under material, and no neck or holder band through material above the head - the shop rule for T-slot, keyseat and dovetail cutters (enter and leave sideways at depth: general shop practice; cutter forms per Machinery's Handbook) on our own column reading. No vendor source consulted.
//!
//! An undercut head cuts sideways under a roof, so the column over it
//! still holds material after the pass. Two facts follow, and each is a
//! conviction:
//!
//! - **The lift.** A move that changes Z while material stands above the
//!   lowest tip height anywhere under the head's disc drives the head
//!   into that material: up into the roof it just cut under, or down
//!   into stock a side-cutting head cannot plunge
//!   ([`Warning::UndercutLift`]). Feeds and rapids alike: the head does
//!   not cut along Z.
//! - **The bands.** The neck (and the holder above it) cuts nothing. At
//!   every station of every move, material above a band's bottom under
//!   that band's disc is a collision ([`Warning::ShankHit`]): the neck
//!   against the mouth of a slot too narrow for it, the holder against
//!   the stock top.
//!
//! "Material" is read through one question, `top(centre, radius)`: the
//! highest material under a disc. The verifier answers with the stock
//! prism (this path clears no column, `Rest::new`); the operation layer
//! asks again with the stock the program ahead left
//! ([`acquit_undercut_by_prior`]), which is what makes a slot cut earlier
//! for the neck, or a pocket cut earlier for the entry, air. An unknown
//! height is material: the conviction stands.
//!
//! A disc meets the stock prism by an exact disc-polygon test (centre
//! inside, or an edge within the radius). A move is read at stations no
//! further apart than a quarter of the narrowest disc. A move too long to
//! march at that step fails closed (convicted).

use super::laws::dist2_seg;
use super::probe::Cutter;
use super::rest::Rest;
use super::types::{VerifyContext, Warning};

use crate::cl::{Move, Toolpath};
use crate::geo2d::{Point2, Region};
use crate::stock_sim::Columns;

/// The most stations one move is marched at; past it the move is convicted.
const MAX_STATIONS: usize = 1 << 16;
/// The smallest march step, mm.
const MIN_STEP: f64 = 0.01;

/// The highest material under the disc `(centre, radius)`.
type Top<'a> = &'a dyn Fn(Point2, f64) -> f64;

/// The stations of `m`: `(xy, tip z)` along its chords, no further apart
/// than `step`. `None` when the move cannot be marched (fail closed).
fn stations(m: &Move, step: f64, chord_tol: f64) -> Option<Vec<(Point2, f64)>> {
    let chords = m.chords(chord_tol);
    let total: f64 = chords.iter().map(|(a, b)| (b.x - a.x).hypot(b.y - a.y)).sum();
    if !total.is_finite() {
        return None;
    }
    if total <= 0.0 {
        return Some(vec![(Point2::new(m.from.x, m.from.y), m.from.z.min(m.to.z))]);
    }
    if total / step > MAX_STATIONS as f64 {
        return None;
    }
    let dz = m.to.z - m.from.z;
    let mut out = Vec::new();
    let mut run = 0.0;
    for (a, b) in chords {
        let len = (b.x - a.x).hypot(b.y - a.y);
        let n = ((len / step).ceil() as usize).max(1);
        for k in 0..=n {
            let t = k as f64 / n as f64;
            let z = m.from.z + dz * ((run + len * t) / total);
            out.push((Point2::new(a.x + (b.x - a.x) * t, a.y + (b.y - a.y) * t), z));
        }
        run += len;
    }
    Some(out)
}

/// Every conviction of the module law on `path`, read through `top`.
fn convictions(path: &Toolpath, cutter: &Cutter, z_tol: f64, chord_tol: f64, top: Top<'_>) -> Vec<Warning> {
    let ok = |v: f64| v.is_finite() && v > 0.0;
    let bands: Vec<(f64, f64, f64)> = cutter
        .shank_bands()
        .into_iter()
        .chain(cutter.holder_bands())
        .filter(|&(above, r, len)| above.is_finite() && ok(r) && ok(len))
        .collect();
    let head_r = cutter.radius;
    let narrowest = bands.iter().map(|b| b.1).fold(head_r, f64::min);
    let step = (narrowest * 0.25).max(MIN_STEP);
    let mut out = Vec::new();
    for (i, m) in path.moves.iter().enumerate() {
        if !m.is_finite() {
            continue; // the NonFinite law owns it
        }
        let Some(st) = stations(m, step, chord_tol) else {
            out.push(Warning::UndercutLift { at: i });
            continue;
        };
        let low = m.from.z.min(m.to.z);
        let z_moves = (m.to.z - m.from.z).abs() > z_tol;
        // An unknown height (NaN) is material: it convicts.
        let over = |t: f64, limit: f64| t.is_nan() || t > limit;
        if z_moves && st.iter().any(|&(c, _)| over(top(c, head_r), low + z_tol)) {
            out.push(Warning::UndercutLift { at: i });
        }
        let band_hit = st.iter().any(|&(c, z)| bands.iter().any(|&(above, r, _)| over(top(c, r), z + above + z_tol)));
        if band_hit {
            out.push(Warning::ShankHit { at: i, part: false });
        }
    }
    out
}

/// Whether the closed disc `(c, r)` meets `region`: its centre is inside,
/// or a boundary edge lies within `r` of it. Exact for a polygon region,
/// so a stock corner that enters the disc between samples is still read
/// (#1444 review). A disc that is not finite meets it (fail closed).
fn disc_meets(region: &Region, c: Point2, r: f64) -> bool {
    if !(c.x.is_finite() && c.y.is_finite() && r.is_finite()) {
        return true;
    }
    if region.contains(c) {
        return true;
    }
    let r2 = r.max(0.0) * r.max(0.0);
    region.loops().iter().any(|l| {
        let p = l.points();
        p.iter().zip(p.iter().cycle().skip(1)).any(|(&a, &b)| dist2_seg(c, a, b) <= r2)
    })
}

/// Law (g) against the stock prism: a disc that reaches the stock's
/// footprint has the stock top over it.
pub(super) fn undercut_law(path: &Toolpath, cx: &VerifyContext, rest: &Rest, cutter: &Cutter, warns: &mut Vec<Warning>) {
    let stock = rest.cx().stock.unwrap_or(rest.cx().region);
    let top = |c: Point2, r: f64| {
        if disc_meets(stock, c, r) { cx.stock_top_z } else { f64::NEG_INFINITY }
    };
    for w in convictions(path, cutter, cx.z_tol, cx.gouge_tol, &top) {
        if !warns.contains(&w) {
            warns.push(w);
        }
    }
}

/// `warns` without the undercut law's convictions that the stock the
/// program ahead left makes safe: the law is read again with `prior`'s
/// columns as the material (the highest column a disc covers), and a
/// lift or a band conviction stays only while that reading convicts the
/// same move. A disc that covers no column, or a height that is not a
/// number, is unknown - the conviction stands. Convictions of any other
/// law are untouched; the caller runs this for an undercut path only
/// (`Mode::Undercut`, where the band law is this law's alone).
pub fn acquit_undercut_by_prior(path: &Toolpath, cutter: &Cutter, prior: &Columns<'_>, z_tol: f64, chord_tol: f64, warns: &mut Vec<Warning>) {
    let ours = |w: &Warning| matches!(w, Warning::UndercutLift { .. } | Warning::ShankHit { part: false, .. });
    if !warns.iter().any(ours) {
        return;
    }
    let top = |c: Point2, r: f64| prior_top(prior, c, r);
    let still = convictions(path, cutter, z_tol, chord_tol, &top);
    warns.retain(|w| !ours(w) || still.contains(w));
}

/// The highest material `dex` holds under the disc `(q, r)`: every
/// column whose centre lies in the disc, and `q`'s own. Cells off the
/// grid are outside the stock - air ([`super::prior::acquit_by_prior`]'s
/// reading) - so a disc that covers no column has nothing over it
/// (`NEG_INFINITY`). A height that is not a number, or a disc that is
/// not finite, is unknown (`NAN`): the caller's conviction stands.
fn prior_top(dex: &Columns<'_>, q: Point2, r: f64) -> f64 {
    let cell = dex.cell();
    if !(r.is_finite() && r >= 0.0 && cell > 0.0 && q.x.is_finite() && q.y.is_finite()) {
        return f64::NAN;
    }
    let o = dex.origin();
    let rr = r + cell * 0.5;
    let lo = |v: f64, o: f64| ((v - rr - o) / cell).floor().max(0.0) as usize;
    let hi = |v: f64, o: f64, n: usize| (((v + rr - o) / cell).ceil().max(0.0) as usize).min(n);
    let mut top = f64::NEG_INFINITY;
    let mut read = |ix: usize, iy: usize| {
        let z = dex.top_at(ix, iy);
        top = if z.is_nan() || top.is_nan() { f64::NAN } else { top.max(z) };
    };
    if let Some((ix, iy)) = dex.col(q) {
        read(ix, iy);
    }
    for iy in lo(q.y, o.y)..hi(q.y, o.y, dex.ny()) {
        for ix in lo(q.x, o.x)..hi(q.x, o.x, dex.nx()) {
            let c = dex.center(ix, iy);
            if (c.x - q.x).hypot(c.y - q.y) <= r {
                read(ix, iy);
            }
        }
    }
    top
}
