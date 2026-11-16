//! Law (c3)'s swept enclosures (#1313): the band stack's law-(c) column
//! bounded over a whole interval of the turn and a whole chord of the
//! tip's path, where the law used to read five poses at four stations.
//!
//! Provenance: the convex hull of two discs is the union of the discs
//! interpolated between them, and a twice-differentiable curve strays
//! from its chord by at most `max|f''| * w^2 / 8` (the linear
//! interpolation error bound of any numerical-analysis text); segment
//! against convex polygon is Cyrus-Beck clipping (public textbook
//! material). The column is law (c)'s own ([`super::reach`]). Our
//! design; no vendor source consulted.
//!
//! One [`Cell`] is one band, one tip chord `t0 -> t0 + d` and one
//! interval `[g0, g1]` of the turn. Law (c)'s column at turn fraction
//! `g` is a disc about `tip + o(g)` of radius `rho(g)` standing from
//! `tip.z + b(g)` up; `o`, `rho` and `b` are `axis(g)` times lengths no
//! longer than the band's `reach` above the tip. The letters lerp, each
//! turns the axis at most one radian per radian and bends it no faster
//! (every second partial of the letter-to-axis map is a unit vector or
//! shorter), so over the interval the axis strays from the chord of its
//! two end axes by at most `kappa = (letters' sweep in the cell)^2 / 8`.
//! Hence every column the cell holds lies inside the hull of its two
//! end discs grown by `reach * kappa`, and stands no lower than the
//! lower end bottom less `reach * kappa`. The tip's own travel is read
//! exactly: the hull is swept along the chord, never sampled.

use super::midpose::pose;
use super::upright::{UPRIGHT_TILT_RAD, is_upright};
use crate::geo2d::Point2;
use crate::geometry::Point3;

/// The most a cell's enclosure may overstate its columns, mm: a tenth
/// of the tightest gouge tolerance a setup states (0.01 mm). The cell
/// count follows from it ([`cell_count`]).
pub(super) const SAG: f64 = 0.001;
/// Cells per band cap. Past it the cells stay conservative and the
/// overstatement grows instead (90 degrees at 150 mm reach: 0.011 mm).
pub(super) const MAX_CELLS: usize = 64;
/// Ring points per radius in a cell's footprint probes ([`Cell::probes`]).
const PROBE_RING: usize = 24;

/// One band's column swept along one tip chord under every pose of one
/// interval of the turn. Offsets are from the tip; radii carry the
/// model growth and the cell's sag; `z0`/`z1` are the column's bottom
/// with the tip at the chord's start/end.
#[derive(Clone, Copy, Debug)]
pub(super) struct Cell {
    t0: Point2,
    d: (f64, f64),
    o: [(f64, f64); 2],
    r: [f64; 2],
    z0: f64,
    z1: f64,
    /// How far above its lowest (`z0`/`z1`) the column's bottom can
    /// stand in this cell at one tip position: the bottom's whole range
    /// over the cell's poses ([`Cell::heights_over`], #1316).
    rise: f64,
    /// The hull's flat flanks between the two end discs, as half-planes
    /// `n . x <= bound` in tip-relative XY; `None` when one disc holds
    /// the other (the hull is that disc).
    flanks: Option<[(f64, f64, f64); 4]>,
    /// The interval of the turn this cell holds (`0..n`), for the
    /// overhang backstop's plane grouping ([`super::turn_hidden`]).
    pub interval: usize,
    pub lo: Point2,
    pub hi: Point2,
}

/// The intervals a band of `reach` needs for a turn of `rate` radians
/// (the letters' summed sweep) to keep its sag within [`SAG`].
#[cfg(test)]
pub(super) fn cell_count(reach: f64, rate: f64) -> usize {
    cell_count_for(reach, rate, SAG, MAX_CELLS)
}

/// [`cell_count`] for a stated sag budget `sag` and cap `cap`.
pub(super) fn cell_count_for(reach: f64, rate: f64, sag: f64, cap: usize) -> usize {
    let cap = cap.max(1);
    let want = rate * (reach / (8.0 * sag)).sqrt();
    if want.is_finite() { (want.ceil() as usize).clamp(1, cap) } else { cap }
}

/// Law (c)'s column for one band at one pose, tip-relative: (offset,
/// radius, bottom). The leaning form always, its bottom dipping by the
/// rim's R·sin θ (#1303; at an upright pose, by nothing).
fn column(a: [f64; 3], above: f64, band_r: f64, band_len: f64) -> ((f64, f64), f64, f64) {
    let mid = above + band_len * 0.5;
    let spread = (band_len * 0.5) * a[0].hypot(a[1]);
    let (za, zb) = (above * a[2], (above + band_len) * a[2]);
    ((mid * a[0], mid * a[1]), band_r + spread, za.min(zb) - super::tilt_band::rim_dip(&a, band_r))
}

/// The lowest end station over the tip and the rim sine at one pose.
fn station_and_sine(a: [f64; 3], above: f64, band_len: f64) -> (f64, f64) {
    ((above * a[2]).min((above + band_len) * a[2]), super::tilt_band::rim_sine(&a))
}

/// Push the cells of one band `(above, radius, length)` over the tip
/// `pieces` of a move turning from the letters `from` to `to` (degrees).
/// `r` is the cutter's radius (a band within it is the flutes' kerf
/// while the whole cell stays upright, law (c)'s skip) and `grow` the
/// band growth on a coarse model (#904). `floors` are #1303's span
/// floors `(turn fraction, bottom over the tip)`: a cell holding one of
/// those fractions stands no higher than it, so a span whose lerp leans
/// past both its samples is read at least as low as the sampled sweep
/// read it (the two laws compose by `min`).
#[allow(clippy::too_many_arguments)]
pub(super) fn push_cells(
    out: &mut Vec<Cell>,
    pieces: &[(Point3, Point3)],
    from: [f64; 3],
    to: [f64; 3],
    (above, band_r, band_len): (f64, f64, f64),
    r: f64,
    grow: f64,
    floors: &[(f64, f64)],
) {
    push_cells_with(out, pieces, from, to, (above, band_r, band_len), r, grow, (SAG, MAX_CELLS), floors);
}

/// [`push_cells`] under a stated `(sag, cap)` cell budget, the span
/// `floors` read the same way; returns the interval count `n` it split
/// the turn into (0 for a dead band).
#[allow(clippy::too_many_arguments)]
pub(super) fn push_cells_with(
    out: &mut Vec<Cell>,
    pieces: &[(Point3, Point3)],
    from: [f64; 3],
    to: [f64; 3],
    (above, band_r, band_len): (f64, f64, f64),
    r: f64,
    grow: f64,
    (sag_budget, cap): (f64, usize),
    floors: &[(f64, f64)],
) -> usize {
    if band_len <= 0.0 || band_r <= 0.0 {
        return 0;
    }
    let reach = above.abs().max((above + band_len).abs());
    let rate: f64 = (0..3).map(|k| (to[k] - from[k]).abs()).sum::<f64>().to_radians();
    let n = cell_count_for(reach, rate, sag_budget, cap);
    let w = rate / n as f64;
    // Non-finite letters cannot be bounded: the widest sag there is
    // (the axis is a unit vector, so no chord is off by more than 2).
    let kappa = if w.is_finite() { (w * w / 8.0).min(2.0) } else { 2.0 };
    let sag = reach * kappa;
    for j in 0..n {
        let (g0, g1) = (j as f64 / n as f64, (j + 1) as f64 / n as f64);
        let (a0, a1) = (pose(from, to, g0), pose(from, to, g1));
        let upright = w <= UPRIGHT_TILT_RAD && is_upright(&a0) && is_upright(&a1);
        if upright && band_r <= r {
            continue;
        }
        let (o0, r0, _) = column(a0, above, band_r, band_len);
        let (o1, r1, _) = column(a1, above, band_r, band_len);
        // An all-upright cell keeps law (c)'s upright bottom.
        // The rim dips by R·sin θ (#1303); over the cell the axis's
        // horizontal part strays from its chord by kappa at most, so
        // sin θ stays under the larger end's sine plus kappa.
        let ((l0, s0), (l1, s1)) = (station_and_sine(a0, above, band_len), station_and_sine(a1, above, band_len));
        let dip = band_r * (s0.max(s1) + kappa).min(1.0);
        let b = if upright { above } else { l0.min(l1) - dip } - sag;
        // #1303's span floor at every sampled pose this cell holds.
        let b = floors.iter().filter(|&&(g, _)| g0 <= g && g <= g1).fold(b, |b, &(_, f)| b.min(f));
        // The highest the bottom stands at any pose of the cell: each
        // end station is `axis.z` times a length within `reach`, so it
        // strays from the chord of its end values by `sag` at most, and
        // the rim's dip only lowers it. Unbounded when not a number.
        let ends = |len: f64| (len * a0[2]).max(len * a1[2]);
        let b_hi = if upright { above } else { ends(above).min(ends(above + band_len)) } + sag;
        let rise = if b_hi - b >= 0.0 { b_hi - b } else { f64::INFINITY };
        let rad = [r0 + grow + sag + 1e-9, r1 + grow + sag + 1e-9];
        let flanks = flanks(o0, o1, rad);
        for (p, q) in pieces {
            let d = (q.x - p.x, q.y - p.y);
            let (xs, ys) = ([0.0, d.0], [0.0, d.1]);
            let x_lo = xs[0].min(xs[1]) + (o0.0 - rad[0]).min(o1.0 - rad[1]);
            let x_hi = xs[0].max(xs[1]) + (o0.0 + rad[0]).max(o1.0 + rad[1]);
            let y_lo = ys[0].min(ys[1]) + (o0.1 - rad[0]).min(o1.1 - rad[1]);
            let y_hi = ys[0].max(ys[1]) + (o0.1 + rad[0]).max(o1.1 + rad[1]);
            out.push(Cell {
                t0: Point2::new(p.x, p.y),
                d,
                o: [o0, o1],
                r: rad,
                z0: p.z + b,
                z1: q.z + b,
                rise,
                flanks,
                interval: j,
                lo: Point2::new(p.x + x_lo, p.y + y_lo),
                hi: Point2::new(p.x + x_hi, p.y + y_hi),
            });
        }
    }
    n
}

/// The four half-planes of the quadrilateral between the outer tangent
/// points of two discs: the two tangent lines, and the chord joining
/// each disc's tangent points. With the discs themselves it is their
/// convex hull.
fn flanks(o0: (f64, f64), o1: (f64, f64), r: [f64; 2]) -> Option<[(f64, f64, f64); 4]> {
    let (vx, vy) = (o1.0 - o0.0, o1.1 - o0.1);
    let len = vx.hypot(vy);
    if !(len.is_finite() && len > (r[0] - r[1]).abs() + 1e-12) {
        return None; // one disc holds the other
    }
    let (ux, uy) = (vx / len, vy / len);
    // A tangent line's outward normal n has n . u = c (it touches both).
    let c = (r[0] - r[1]) / len;
    let s = (1.0 - c * c).max(0.0).sqrt();
    let side = |k: f64| {
        let (nx, ny) = (c * ux - k * s * uy, c * uy + k * s * ux);
        (nx, ny, r[0] + nx * o0.0 + ny * o0.1)
    };
    Some([
        side(1.0),
        side(-1.0),
        (-ux, -uy, -(r[0] * c + ux * o0.0 + uy * o0.1)),
        (ux, uy, r[1] * c + ux * o1.0 + uy * o1.1),
    ])
}

/// The `s` in [0, 1] with `|e + s * d|^2 <= rad2`.
fn span(e: (f64, f64), d: (f64, f64), rad2: f64) -> Option<(f64, f64)> {
    let aa = d.0 * d.0 + d.1 * d.1;
    let c = e.0 * e.0 + e.1 * e.1 - rad2;
    if aa <= f64::EPSILON {
        return (c <= 0.0).then_some((0.0, 1.0));
    }
    let b = e.0 * d.0 + e.1 * d.1;
    let disc = b * b - aa * c;
    if disc.is_nan() || disc < 0.0 {
        return None;
    }
    let root = disc.sqrt();
    let (lo, hi) = (((-b - root) / aa).max(0.0), ((-b + root) / aa).min(1.0));
    (lo <= hi).then_some((lo, hi))
}

/// The `s` in [0, 1] with `e + s * d` inside every half-plane.
fn clip(e: (f64, f64), d: (f64, f64), planes: &[(f64, f64, f64); 4]) -> Option<(f64, f64)> {
    let (mut lo, mut hi) = (0.0f64, 1.0f64);
    for &(nx, ny, bound) in planes {
        let (at, rate) = (nx * e.0 + ny * e.1 - bound, nx * d.0 + ny * d.1);
        if rate.abs() <= f64::EPSILON {
            if at > 0.0 {
                return None;
            }
        } else if rate > 0.0 {
            hi = hi.min(-at / rate);
        } else {
            lo = lo.max(-at / rate);
        }
    }
    (lo <= hi).then_some((lo, hi))
}

impl Cell {
    /// The lowest the band's bottom stands anywhere in this cell.
    pub(super) fn floor(&self) -> f64 {
        self.z0.min(self.z1)
    }

    /// Points of the cell's footprint, appended to `out`: at
    /// `stations + 1` tip positions along the chord, each end disc's centre, the
    /// hull's middle and rings of [`PROBE_RING`] points at a quarter,
    /// half, three quarters and the whole of each disc's radius. The
    /// hidden region's own boundary samples catch a region that only
    /// clips the footprint; these catch one that swallows it.
    pub(super) fn probes(&self, stations: usize, out: &mut Vec<Point2>) {
        let stations = stations.max(1);
        for i in 0..=stations {
            let s = i as f64 / stations as f64;
            let t = (self.t0.x + self.d.0 * s, self.t0.y + self.d.1 * s);
            let mid = ((self.o[0].0 + self.o[1].0) * 0.5, (self.o[0].1 + self.o[1].1) * 0.5);
            out.push(Point2::new(t.0 + mid.0, t.1 + mid.1));
            for k in 0..2 {
                let (c, rad) = ((t.0 + self.o[k].0, t.1 + self.o[k].1), self.r[k]);
                out.push(Point2::new(c.0, c.1));
                for f in [0.25, 0.5, 0.75, 1.0] {
                    for j in 0..PROBE_RING {
                        let a = std::f64::consts::TAU * j as f64 / PROBE_RING as f64;
                        out.push(Point2::new(c.0 + f * rad * a.cos(), c.1 + f * rad * a.sin()));
                    }
                }
            }
        }
    }

    /// The lowest the band's column bottom stands over the sample
    /// column `q` anywhere in this cell - `None` when no position of
    /// the cell puts the column over `q` outside the flutes' kerf
    /// (`kerf2`: the squared kerf radius about the tip, law (c)'s inner
    /// edge; negative for no kerf). The kerf's own rim counts as
    /// covered: closed toward conviction.
    pub(super) fn bottom_over(&self, q: Point2, kerf2: f64) -> Option<f64> {
        let z = |s: f64| self.z0 + (self.z1 - self.z0) * s;
        let low = |(a, b): (f64, f64)| z(a).min(z(b));
        match self.stretches(q, kerf2) {
            [Some(a), Some(b)] => Some(low(a).min(low(b))),
            [a, b] => a.or(b).map(low),
        }
    }

    /// Every height the band's column bottom takes over `q` in this
    /// cell, as one closed range per stretch of the chord that stands
    /// the column over `q` outside the kerf: from the lowest bottom
    /// there ([`Self::bottom_over`]'s reading) to the highest any pose
    /// of the cell holds it (`rise` above). `hit` is asked each range;
    /// the first `true` ends the walk and is returned. The overhang
    /// backstop's reading (#1316): hidden stock is not monotone in the
    /// plane, so it is asked over the whole range, never at one end.
    pub(super) fn heights_over(&self, q: Point2, kerf2: f64, hit: &mut dyn FnMut(f64, f64) -> bool) -> bool {
        let z = |s: f64| self.z0 + (self.z1 - self.z0) * s;
        self.stretches(q, kerf2)
            .into_iter()
            .flatten()
            .any(|(a, b)| hit(z(a).min(z(b)), z(a).max(z(b)) + self.rise))
    }

    /// The stretches `(s0, s1)` of the chord (tip at `t0 + s * d`) that
    /// stand the column over `q` outside the kerf: the one before the
    /// tip's kerf passes over `q` and the one after it.
    fn stretches(&self, q: Point2, kerf2: f64) -> [Option<(f64, f64)>; 2] {
        // The tip stands at t0 + s*d; seen from it, q walks backwards.
        let p = (q.x - self.t0.x, q.y - self.t0.y);
        let back = (-self.d.0, -self.d.1);
        let mut over: Option<(f64, f64)> = None;
        let mut join = |x: Option<(f64, f64)>| {
            if let Some((a, b)) = x {
                over = Some(over.map_or((a, b), |(lo, hi): (f64, f64)| (lo.min(a), hi.max(b))));
            }
        };
        for k in 0..2 {
            join(span((p.0 - self.o[k].0, p.1 - self.o[k].1), back, self.r[k] * self.r[k]));
        }
        if let Some(planes) = &self.flanks {
            join(clip(p, back, planes));
        }
        let Some((lo, hi)) = over else {
            return [None, None];
        };
        match span(p, back, kerf2) {
            None => [Some((lo, hi)), None],
            Some((k0, k1)) => [(k0 > lo).then_some((lo, k0)), (k1 < hi).then_some((k1, hi))],
        }
    }
}

#[cfg(test)]
#[path = "turn_cells_tests.rs"]
mod tests;
