//! Where a cross hole's drill first meets the raw stock (issue #85):
//! along the hole's own line, against the stock mesh, rather than at
//! the stock's circumscribed radius - a box counts at its face, not its
//! corner. The body is read as the square tube of half-width `half`
//! about the line (across it in XY, and in Z): a superset of the round
//! drill, so the outermost stock point inside it is no nearer the axis
//! than the first point the real body touches - the entry stays
//! conservative.
//!
//! A tessellated round's facets are chords lying INSIDE the true
//! surface, so each facet is read as the slab its true surface can
//! occupy: thickened by its sagitta allowance either side (from the
//! dihedral angle to its edge neighbours and its width across that
//! edge, `cross_sag::sag_of`, edge neighbours found within a weld
//! tolerance so a non-conforming STL reads as a welded one). A facet meeting its neighbours only at sharp edges
//! (a box face, a hex flat) or flat is read exactly.
//!
//! Issue #119: the same reading, over a rectangular tube, is the cross
//! drill's holder footprint on one station (`tube_reach`).
//!
//! Provenance: Sutherland-Hodgman polygon clipping against slab
//! half-spaces, the first-contact point of a prism swept along its
//! axis, and the chord sagitta s = (w/2) tan(theta/4) of an inscribed
//! polygon are textbook computational geometry. Our own design notes;
//! no vendor material consulted.

use crate::geometry::{Point3, Triangle};

use super::cross_sag::{sag_of, unit_normal};

/// Stock facets in the spindle frame, each with the sagitta allowance
/// its true surface may stand beyond the facet plane (0: read exactly).
#[derive(Default)]
pub(super) struct StockMesh {
    tris: Vec<Triangle>,
    sag: Vec<f64>,
}

impl StockMesh {
    pub fn new(tris: Vec<Triangle>) -> Self {
        let sag = sag_of(&tris);
        StockMesh { tris, sag }
    }

    /// Add another solid's facets (their allowances read on their own:
    /// two solids' coincident edges are no crease of either).
    pub fn extend(&mut self, other: StockMesh) {
        self.tris.extend(other.tris);
        self.sag.extend(other.sag);
    }
}

/// A hole line in the spindle frame: `a` the XY axis it plunges on
/// (0 X, 1 Y), `out` the outward sign along it (+1: outward is +a), and
/// the line's transverse XY coordinate `t` and its Z.
#[derive(Clone, Copy)]
pub(super) struct Line {
    pub a: usize,
    pub out: f64,
    pub t: f64,
    pub z: f64,
}

impl Line {
    /// The outward coordinate of a point along this line.
    pub fn u(&self, p: Point3) -> f64 {
        self.out * if self.a == 0 { p.x } else { p.y }
    }
    fn w(&self, p: Point3) -> f64 {
        if self.a == 0 { p.y } else { p.x }
    }
}

/// A rectangular tube along a hole line, unbounded along it: `hw`
/// either side of the line's transverse coordinate `line.t` across it
/// in XY, `hz` either side of `line.z` in Z. A body's square tube is
/// `hw == hz`, its half-width; a holder sliding across its line or in
/// Z on one station (issue #119) widens the side it sweeps.
#[derive(Clone, Copy)]
pub(super) struct Tube {
    pub line: Line,
    pub hw: f64,
    pub hz: f64,
}

/// The outermost outward coordinate of the planar polygon `pts` inside
/// `tube`.
fn poly_max(pts: &[Point3], tube: Tube) -> Option<f64> {
    let line = tube.line;
    let mut poly: Vec<(f64, f64, f64)> =
        pts.iter().filter(|p| p.is_finite()).map(|&p| (line.u(p), line.w(p) - line.t, p.z - line.z)).collect();
    if poly.len() < 3 {
        return None;
    }
    // Four slabs: |w| <= hw and |z| <= hz, each as two halves
    // (`k` picks w or z, `sg` the side).
    for (k, sg) in [(1, 1.0), (1, -1.0), (2, 1.0), (2, -1.0)] {
        poly = clip(&poly, |p| if k == 1 { tube.hw - sg * p.1 } else { tube.hz - sg * p.2 });
        if poly.is_empty() {
            return None;
        }
    }
    poly.iter().map(|p| p.0).reduce(f64::max)
}

/// The outermost outward coordinate of any stock facet - a curved one
/// as its sagitta slab, grown by `grow` along its normal on a coarse
/// model (#1053) - inside `tube`, or `None` when the tube meets no
/// facet. A slab's outermost point in the tube lies on one of its five
/// faces (the tube is unbounded along the line), so clipping them suffices.
fn tube_max(mesh: &StockMesh, tube: Tube, grow: f64) -> Option<f64> {
    let g = if grow.is_finite() { grow.max(0.0) } else { 0.0 };
    let mut best: Option<f64> = None;
    let mut take = |m: Option<f64>| {
        if let Some(m) = m {
            best = Some(best.map_or(m, |b: f64| b.max(m)));
        }
    };
    for (tri, &s) in mesh.tris.iter().zip(&mesh.sag) {
        let v = [tri.a, tri.b, tri.c];
        let s = if s.is_finite() { s.max(0.0) } else { 0.0 } + g;
        let n = if s > 0.0 { unit_normal(tri) } else { None };
        let Some(n) = n else {
            take(poly_max(&v, tube).map(|m| m + g));
            continue;
        };
        let off = |p: Point3, k: f64| Point3::new(p.x + k * n.x, p.y + k * n.y, p.z + k * n.z);
        let (lo, hi) = (v.map(|p| off(p, -s)), v.map(|p| off(p, s)));
        take(poly_max(&lo, tube));
        take(poly_max(&hi, tube));
        for k in 0..3 {
            let j = (k + 1) % 3;
            take(poly_max(&[lo[k], lo[j], hi[j], hi[k]], tube));
        }
    }
    best
}

/// Keep the part of polygon `poly` where `inside(p) >= 0`.
fn clip(poly: &[(f64, f64, f64)], inside: impl Fn(&(f64, f64, f64)) -> f64) -> Vec<(f64, f64, f64)> {
    let mut out = Vec::with_capacity(poly.len() + 2);
    for i in 0..poly.len() {
        let (p, q) = (poly[i], poly[(i + 1) % poly.len()]);
        let (dp, dq) = (inside(&p), inside(&q));
        if dp >= 0.0 {
            out.push(p);
        }
        if (dp >= 0.0) != (dq >= 0.0) {
            let s = dp / (dp - dq);
            if s.is_finite() {
                out.push((p.0 + s * (q.0 - p.0), p.1 + s * (q.1 - p.1), p.2 + s * (q.2 - p.2)));
            }
        }
    }
    out
}

/// The outward coordinate where a body of half-width `half` on `line`
/// first meets the raw stock: the stock mesh's own crossing, or - when
/// the mesh stands no further inside the stock's circumscribed radius
/// `raw` than a round bar's tessellation does - the circumscribed
/// reading (#77's, kept bit for bit for a round bar). The mesh is read
/// when the edge point nearest the axis at its crossing stands more
/// than `raw (1 - cos(pi/16))` inside `raw`: a flat face, not the
/// chords of a turned bar (whose facets lie inside the true round, so
/// trusting them there would put the mouth inside the real bar). A
/// tube that meets no facet falls back to the circumscribed reading, as
/// before #85. `None`: the body misses the stock's radius on this line.
/// A round well inside `raw` (a stepped bar's smaller section, an
/// off-axis cylinder, a boss) is read at its sagitta slabs, so the
/// entry is never nearer the axis than its true curved surface.
#[cfg(test)]
pub(super) fn entry(mesh: &StockMesh, raw: f64, line: Line, half: f64) -> Option<f64> {
    entry_grown(mesh, raw, line, half, 0.0)
}

/// [`entry`] on a coarse model whose facets are trusted only within
/// `grow` along their normals (#1053; `raw` already includes `grow`).
pub(super) fn entry_grown(mesh: &StockMesh, raw: f64, line: Line, half: f64, grow: f64) -> Option<f64> {
    let edge = (line.t.abs() - half).max(0.0);
    let circ = (edge < raw).then(|| (raw * raw - edge * edge).max(0.0).sqrt())?;
    let Some(m) = tube_max(mesh, Tube { line, hw: half, hz: half }, grow) else { return Some(circ) };
    Some(face_or_circ(m, circ, raw, edge))
}

/// The mesh's crossing `m` when it is a flat (more than a turned bar's
/// tessellation inside `raw` at the edge offset `edge`), else the
/// circumscribed reading `circ` - `entry`'s choice.
fn face_or_circ(m: f64, circ: f64, raw: f64, edge: f64) -> f64 {
    let band = raw * (1.0 - (std::f64::consts::PI / 16.0).cos());
    let flat = m.is_finite() && m < circ && raw - m.hypot(edge) > band;
    if flat { m } else { circ }
}

/// The outermost outward coordinate of material inside `tube` (issue
/// #119: a holder's footprint on one station), read as `entry` reads a
/// body's: a flat at its face, a turned round at its circumscribed
/// reading at the tube's edge nearest the axis. `None` when the tube
/// meets no facet - the tube is unbounded along its line, so a solid
/// it entered would cross its wall: nothing is there. A non-finite
/// reading is `NaN`, which no holder clears.
pub(super) fn tube_reach(mesh: &StockMesh, raw: f64, tube: Tube, grow: f64) -> Option<f64> {
    let m = tube_max(mesh, tube, grow)?;
    if !(m.is_finite() && raw.is_finite()) {
        return Some(f64::NAN);
    }
    let edge = (tube.line.t.abs() - tube.hw).max(0.0);
    // A crossing outside the circumscribed circle (only by rounding)
    // stands as read.
    if edge >= raw {
        return Some(m);
    }
    Some(face_or_circ(m, (raw * raw - edge * edge).max(0.0).sqrt(), raw, edge))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::cube;

    fn line(t: f64) -> Line {
        Line { a: 0, out: 1.0, t, z: 0.0 }
    }

    #[test]
    fn a_box_face_is_the_entry_and_a_miss_falls_back_to_the_corner_circle() {
        let b = StockMesh::new(cube(Point3::new(-30.0, -30.0, -20.0), Point3::new(30.0, 30.0, 20.0)).facets.to_vec());
        let raw = 1800.0_f64.sqrt();
        assert_eq!(entry(&b, raw, line(0.0), 2.0), Some(30.0));
        assert_eq!(entry(&b, raw, line(7.0), 3.0), Some(30.0));
        // Above the box: no crossing, the circumscribed reading stands.
        let above = Line { z: 50.0, ..line(0.0) };
        assert_eq!(entry(&b, raw, above, 2.0), Some(raw));
        assert_eq!(entry(&b, raw, line(raw + 3.0), 2.0), None);
    }

    /// A closed `n`-gon prism of circumradius `r` about `(cx, cy)`, z in
    /// `[z0, z1]`, its vertices at `phase + k 2pi/n`.
    fn prism(n: usize, r: f64, (cx, cy): (f64, f64), (z0, z1): (f64, f64), phase: f64) -> Vec<Triangle> {
        let v = |k: usize, z: f64| {
            let th = phase + std::f64::consts::TAU * k as f64 / n as f64;
            Point3::new(cx + r * th.cos(), cy + r * th.sin(), z)
        };
        let mut f = Vec::new();
        for k in 0..n {
            let (a1, a2, b1, b2) = (v(k, z0), v(k + 1, z0), v(k, z1), v(k + 1, z1));
            f.push(Triangle::new(a1, a2, b2));
            f.push(Triangle::new(a1, b2, b1));
            f.push(Triangle::new(Point3::new(cx, cy, z0), a2, a1));
            f.push(Triangle::new(Point3::new(cx, cy, z1), b1, b2));
        }
        f
    }

    /// A d40 section (z -20..0) stepping up to d60 (z 0..20): the
    /// global raw radius is the d60's r30, but a hole on the d40 section
    /// meets the d40's true round. Drill d4 with its edge at w 19.5: the
    /// true entry is sqrt(20^2 - 19.5^2) = 4.44 - the bare chords read
    /// ~3.99 (32 facets) and ~2.13 (16), the mouth inside metal.
    #[test]
    fn a_stepped_bars_small_section_is_entered_at_or_outside_its_true_round() {
        let truth = (400.0_f64 - 19.5 * 19.5).sqrt();
        for n in [32, 16] {
            for phase in [0.0, std::f64::consts::PI / n as f64] {
                let mut tris = prism(n, 20.0, (0.0, 0.0), (-20.0, 0.0), phase);
                tris.extend(prism(n, 30.0, (0.0, 0.0), (0.0, 20.0), phase));
                let st = StockMesh::new(tris);
                let raw = 30.0;
                let at = |t: f64| entry(&st, raw, Line { a: 0, out: 1.0, t, z: -10.0 }, 2.0);
                let got = at(21.5).unwrap_or(f64::NAN);
                assert!(got >= truth - 1e-9, "{n} facets, phase {phase}: entry {got:.3} inside the true d40 ({truth:.3})");
                assert!(got < 22.0, "{n} facets: the d40 section is read, not the d60 corner circle: {got:.3}");
                let mid = at(0.0).unwrap_or(f64::NAN);
                assert!((20.0 - 1e-9..21.0).contains(&mid), "{n} facets: on the centre line {mid:.3}");
            }
        }
    }

    /// A hex bar's flat is a flat: its edges are 60 degree corners, not a
    /// round's creases, so the entry is the flat exactly.
    #[test]
    fn a_hex_bar_flat_is_read_at_the_flat() {
        let st = StockMesh::new(prism(6, 25.0, (0.0, 0.0), (-20.0, 20.0), std::f64::consts::PI / 6.0));
        let flat = 25.0 * (std::f64::consts::PI / 6.0).cos();
        for t in [0.0, 5.0] {
            let got = entry(&st, 25.0, line(t), 2.0).unwrap_or(f64::NAN);
            assert!((got - flat).abs() < 1e-9, "t {t}: the hex flat r{flat:.3} read as {got:.3}");
        }
    }

    /// A d50 cylinder stock 5 mm off the spindle axis (raw r30): on the
    /// line whose drill edge is at w 20 the true round is at
    /// u = 5 + sqrt(25^2 - 20^2) = 20; the entry is no nearer the axis.
    #[test]
    fn an_off_axis_cylinder_stock_is_entered_at_or_outside_its_true_round() {
        for n in [32, 16] {
            let st = StockMesh::new(prism(n, 25.0, (5.0, 0.0), (-20.0, 20.0), 0.0));
            let raw = 30.0;
            let got = entry(&st, raw, line(22.0), 2.0).unwrap_or(f64::NAN);
            assert!(got >= 20.0 - 1e-9, "{n} facets: entry {got:.3} inside the true round (20)");
            let back = entry(&st, raw, Line { out: -1.0, ..line(22.0) }, 2.0).unwrap_or(f64::NAN);
            assert!(back >= 10.0 - 1e-9, "{n} facets, -X side: entry {back:.3} inside the true round (10)");
        }
    }

    /// The same round as a non-conforming STL (#121): every side quad
    /// carries its own copies of its corners, moved 5e-6 mm along Z
    /// (alternately up and down, so neighbours' copies never coincide -
    /// the facet planes, and so the true surface, unchanged). Its
    /// entry and its shank reach (#84, the wider body) are the welded
    /// mesh's, not its bare chords'.
    #[test]
    fn an_unwelded_round_is_entered_and_reached_as_the_welded_one() {
        let unweld = |tris: Vec<Triangle>| -> Vec<Triangle> {
            let dz = |i: usize| if (i / 4).is_multiple_of(2) { 5e-6 } else { -5e-6 };
            let up = |p: Point3, d: f64| Point3::new(p.x, p.y, p.z + d);
            let side = |i: usize| i % 4 < 2;
            tris.iter()
                .enumerate()
                .map(|(i, t)| if side(i) { Triangle::new(up(t.a, dz(i)), up(t.b, dz(i)), up(t.c, dz(i))) } else { *t })
                .collect()
        };
        let truth = (400.0_f64 - 19.5 * 19.5).sqrt();
        for n in [32, 16] {
            let mut tris = prism(n, 20.0, (0.0, 0.0), (-20.0, 0.0), 0.0);
            tris.extend(prism(n, 30.0, (0.0, 0.0), (0.0, 20.0), 0.0));
            let (welded, loose) = (StockMesh::new(tris.clone()), StockMesh::new(unweld(tris)));
            let off = StockMesh::new(prism(n, 25.0, (5.0, 0.0), (-20.0, 20.0), 0.0));
            let off_loose = StockMesh::new(unweld(prism(n, 25.0, (5.0, 0.0), (-20.0, 20.0), 0.0)));
            for half in [2.0, 3.0] {
                let l = Line { a: 0, out: 1.0, t: 19.5 + half, z: -10.0 };
                let (w, u) = (entry(&welded, 30.0, l, half), entry(&loose, 30.0, l, half));
                let (w, u) = (w.unwrap_or(f64::NAN), u.unwrap_or(f64::NAN));
                assert!((w - u).abs() < 1e-9, "{n} facets, half {half}: unwelded {u:.6} vs welded {w:.6}");
                assert!(u >= truth - 1e-9, "{n} facets, half {half}: unwelded {u:.3} inside the true d40 ({truth:.3})");
                let l = line(20.0 + half);
                let (w, u) = (entry(&off, 30.0, l, half), entry(&off_loose, 30.0, l, half));
                let (w, u) = (w.unwrap_or(f64::NAN), u.unwrap_or(f64::NAN));
                assert!((w - u).abs() < 1e-9, "{n} facets off-axis, half {half}: unwelded {u:.6} vs welded {w:.6}");
                assert!(u >= 20.0 - 1e-9, "{n} facets off-axis, half {half}: unwelded {u:.3} inside the true round (20)");
            }
        }
    }
}
