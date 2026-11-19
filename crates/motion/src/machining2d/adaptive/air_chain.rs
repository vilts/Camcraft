//! Issue #1950: the open fronts of the air-side stock ring (`air_rings`)
//! cut region by region. The runs of consecutive fronts in one open
//! region form a chain, and each run is reached from the last one of its
//! chain in the material plane: back along the run just cut (on its own
//! path, so on cleared area and across no kept pass), then one chord to
//! the next lead-in start, read on the stock within the load bound. Only
//! the move between regions is a retract hop. The return pass of
//! contour-parallel pocketing (Held 1991, "On the Computational Geometry
//! of Pocket Machining", LNCS 500), first principles; ours.

use crate::geo2d::{Point2, Region};
use crate::machining2d::slots::chord_clears;

use super::air_uncross::Kept;
use super::front::{densify, dist};
use super::raster::Raster;

/// The runs of each open front, grouped into chains: a run joins the
/// chain whose last run starts and ends within `reach` of its own start
/// and end (the nearest such), else it starts a new chain. Chains are in
/// the order they start, and each keeps the front order.
pub(super) fn chains(fronts: Vec<Vec<Vec<Point2>>>, reach: f64) -> Vec<Vec<Vec<Point2>>> {
    let mut out: Vec<Vec<Vec<Point2>>> = Vec::new();
    for runs in fronts {
        let mut taken = vec![false; out.len()];
        for run in runs {
            let (Some(&a), Some(&b)) = (run.first(), run.last()) else { continue };
            let near = |c: &Vec<Vec<Point2>>| c.last().and_then(|l| Some(dist(*l.first()?, a).max(dist(*l.last()?, b))));
            let best = out
                .iter()
                .enumerate()
                .filter(|(i, _)| !taken.get(*i).copied().unwrap_or(true))
                .filter_map(|(i, c)| near(c).map(|d| (i, d)))
                .filter(|&(_, d)| d <= reach)
                .min_by(|x, y| x.1.total_cmp(&y.1));
            match best.and_then(|(i, _)| out.get_mut(i).map(|c| (i, c))) {
                Some((i, c)) => {
                    c.push(run);
                    if let Some(t) = taken.get_mut(i) {
                        *t = true;
                    }
                }
                None => out.push(vec![run]),
            }
        }
    }
    out
}

/// The in-material link from the end of `prev` (the run just cut, the
/// last piece of its chain) to `to` (the next lead-in start): `prev`
/// walked back to its start, then a chord to `to`. `None` where the chord
/// leaves `center`, crosses a kept pass or reads over `limit` (degrees of
/// rim) on the stock `ras`: the next run is then reached by the joins.
/// The link holds neither end point.
pub(super) fn link(ras: &Raster, kept: &Kept, center: &Region, prev: &[Point2], to: Point2, (r, rim, limit): (f64, f64, f64)) -> Option<Vec<Point2>> {
    let &from = prev.first()?;
    let chord = chord(ras, kept, center, from, to, (r, rim, limit))?;
    let back = prev.iter().rev().skip(1);
    let mut out: Vec<Point2> = back.copied().collect();
    out.extend(chord.iter().skip(1).take(chord.len().saturating_sub(2)).copied());
    Some(out)
}

/// The straight feed chord `from` -> `to` in the material plane, both
/// ends held, or `None` where it leaves `center`, crosses a kept pass or
/// reads over `limit` (degrees of rim) on the stock `ras`.
pub(super) fn chord(ras: &Raster, kept: &Kept, center: &Region, from: Point2, to: Point2, (r, rim, limit): (f64, f64, f64)) -> Option<Vec<Point2>> {
    if !(center.contains(to) && chord_clears(center, from, to)) {
        return None;
    }
    let chord = densify(&[from, to], r / 8.0, false);
    (!kept.crosses_any(&chord) && ras.peek(&chord, rim, 0.1 * r) <= limit).then_some(chord)
}

/// Each marked sample with its two neighbours marked too (a bite run
/// starts and ends one sample in air).
pub(super) fn grow_marks(m: &[bool]) -> Vec<bool> {
    let n = m.len();
    (0..n).map(|i| m[i] || (i > 0 && m[i - 1]) || (i + 1 < n && m[i + 1])).collect()
}

/// The runs of a closed walk (first point repeated) where `inside` holds.
pub(super) fn split_runs(ring: &[Point2], inside: &[bool]) -> Vec<Vec<Point2>> {
    let n = ring.len().saturating_sub(1);
    if n == 0 {
        return Vec::new();
    }
    let Some(out0) = (0..n).find(|&i| !inside[i]) else {
        return vec![ring.to_vec()];
    };
    let mut runs = Vec::new();
    let mut cur: Vec<Point2> = Vec::new();
    for k in 1..=n {
        let i = (out0 + k) % n;
        if inside[i] {
            cur.push(ring[i]);
        } else if !cur.is_empty() {
            runs.push(std::mem::take(&mut cur));
        }
    }
    if !cur.is_empty() {
        runs.push(cur);
    }
    runs
}

/// The vertex of `prev` nearest where the ray from `p0` along `n` first
/// meets it (within `max`, and ahead of `p0`), or `None`. A lead-in that
/// starts on a vertex of the path just cut only touches that path.
pub(super) fn onto(prev: &[Point2], p0: Point2, n: Point2, max: f64) -> Option<usize> {
    let mut best: Option<(f64, usize)> = None;
    for (i, s) in prev.windows(2).enumerate() {
        let (a, b) = (s[0], s[1]);
        let (ex, ey) = (b.x - a.x, b.y - a.y);
        let den = n.x * ey - n.y * ex;
        if den.abs() <= 1e-12 {
            continue;
        }
        let (dx, dy) = (a.x - p0.x, a.y - p0.y);
        let t = (dx * ey - dy * ex) / den;
        let u = (dx * n.y - dy * n.x) / den;
        if t.is_finite() && t > 1e-9 && t <= max && (0.0..=1.0).contains(&u) && best.is_none_or(|(bt, _)| t < bt) {
            best = Some((t, if u < 0.5 { i } else { i + 1 }));
        }
    }
    let (_, j) = best?;
    let q = prev.get(j)?;
    ((q.x - p0.x) * n.x + (q.y - p0.y) * n.y > 1e-9).then_some(j)
}

/// How deep the disc at `p` stands in the stock (mm, to a fiftieth of
/// the radius): `r` less the widest clear disc there.
pub(super) fn plunge_depth(ras: &Raster, p: Point2, r: f64) -> f64 {
    (0..=50).map(|k| r * (1.0 - k as f64 / 50.0)).find(|&rho| ras.disc_clear(p, rho)).map_or(r, |rho| r - rho)
}
