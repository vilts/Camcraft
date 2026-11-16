//! Law (d) for `Mode::Radial` (#79): a cross-drill rapid's tool body against the raw stock solid, minus the kerf the program's own cuts swept before it - the shop practice of traversing only outside the stock (machine-tool handbooks), read as a solid instead of a height field; our own design, no vendor material consulted.
//!
//! The column law ([`super::rapids`]) reads a rapid as a vertical tool
//! over a height field, and a horizontal bore is a tunnel no height
//! field can see - so for radial programs it stood down (M5.2 F2) and
//! trusted the kernel's traverse law. That left the post gate blind:
//! a station change through uncut stock (#77) posted with no warning.
//! This reading needs no column:
//!
//! - The stock is the prism the verifier already holds - the stock
//!   footprint (`cx.stock`, or the region standing in for it, exactly
//!   as [`Rest::in_stock`] reads it) under the stock top, unbounded
//!   below (a chuck-held bar's foot is in the jaws: reading it as
//!   stock only adds convictions). A turned bar or a box about the
//!   spindle is a Z prism, so the footprint IS the solid measured
//!   about the spindle, wherever the work offset puts it.
//! - The tool is its flute body: discs of radius `r - gouge_tol`
//!   (the mill law's disc, [`Rest::hidden_along`]) square to the pose,
//!   at the tip and every [`BODY_STEP`] back along the pose up to the
//!   flute length, under the controller's modal pose rule
//!   ([`super::midpose::intervals`], 0.5 mm of travel at the far end
//!   between samples).
//! - The kerf is what the feeds BEFORE the rapid swept (cut order, as
//!   M13.7 reads the column): a cylinder of the tool radius about each
//!   cutting move, extended back along the pose by the flute length
//!   when the move plunges along its own axis (the body followed the
//!   tip in). A retract up a just-drilled hole - and the peck's chip
//!   break - stands wholly in that kerf, so it is acquitted by the
//!   stock reading, never by the path's description of itself.
//!
//! A sample of the body in stock and in no earlier kerf convicts the
//! rapid: [`Warning::RapidInStock`], the mill law's own blocking word.
//! Sampled, so bounded rather than exact: the discs stand
//! [`BODY_STEP`] (0.5 mm) apart along the body and the tip travels at
//! most 0.5 mm (midpose's step) between samples, so a sliver of stock
//! thinner than that between two samples can slip through. Round a
//! disc, the [`RIM`] = 16 rim samples are chords of the rim circle:
//! between two of them the circle bulges `r (1 - cos(pi / 16))`,
//! about `0.019 r`, past the chord - a graze deeper than the gouge
//! tolerance by less than that, between two rim samples, can pass.
//!
//! Fail closed, by name: a context the reading cannot stand on - a
//! non-finite stock top or Z tolerance, a tool with no finite positive
//! radius - convicts every rapid of the program, and a rapid whose
//! stated (or held) pose is no direction convicts itself: the law
//! cannot place the body, so it never vouches for it. A flute length
//! that is zero or non-finite reads as a body one tool diameter long,
//! never as the bare tip.
//!
//! Cost (#79 review): the kerf list is built once, in cut order, each
//! cylinder with its box; a rapid takes only the earlier kerfs whose
//! box meets its own swept box (padded by the body and the radius),
//! newest first - the hole a retract rides is the last one cut. A disc
//! whose box misses the stock prism's box is skipped whole before any
//! polygon test, and the shared interval ends are sampled once.

use super::midpose::{self, Pose};
use super::rest::Rest;
use super::types::{VerifyContext, Warning};

use crate::cl::{normalize_axis, Move, MoveKind, Toolpath};
use crate::geo2d::Point2;
use crate::geometry::Point3;

/// Axial spacing of the body discs behind the tip, mm.
const BODY_STEP: f64 = 0.5;
/// Rim samples of the outer disc ring (half as many on the inner).
const RIM: usize = 16;
/// Most body discs one sample carries.
const MAX_BODY: usize = 512;

/// One earlier cut's kerf: the tool-radius cylinder about `a..b`, the
/// index of the move that cut it, and its box `lo..hi` (padded by the
/// radius and the slack).
struct Kerf {
    at: usize,
    a: Point3,
    b: Point3,
    lo: [f64; 3],
    hi: [f64; 3],
}

/// Law (d) for radial programs: one [`Warning::RapidInStock`] per rapid
/// whose flute body enters stock no earlier cut removed.
pub(super) fn radial_rapids(path: &Toolpath, cx: &VerifyContext, rest: &Rest, warns: &mut Vec<Warning>) {
    let r = cx.tool.radius;
    let rapids = path.moves.iter().enumerate().filter(|(_, m)| m.kind == MoveKind::Rapid);
    if !(r.is_finite() && r > 0.0 && cx.stock_top_z.is_finite() && cx.z_tol.is_finite()) {
        // No body, or no stock to read it against: never a silent pass.
        warns.extend(rapids.map(|(at, _)| Warning::RapidInStock { at }));
        return;
    }
    let flute = cx.tool.flute_len;
    let body = if flute.is_finite() && flute > 0.0 { flute } else { 2.0 * r };
    let tol = cx.z_tol.max(0.0);
    let top = cx.stock_top_z + cx.band_growth() - tol;
    let disc = (r - cx.tightened(cx.gouge_tol.max(0.0))).max(0.5 * r);
    let poses = midpose::poses(path);
    let kerfs = kerfs(path, &poses, body, r + tol, cx.gouge_tol.max(1e-3));
    let solid = cx.stock.unwrap_or(cx.region).bounds();
    let ring = rings(disc);
    let k = ((body / BODY_STEP).ceil() as usize).clamp(1, MAX_BODY);
    for (i, m) in rapids {
        if !(m.from.is_finite() && m.to.is_finite()) {
            continue; // non-finite moves are NonFinite's word
        }
        let Some((Some(a0), Some(a1))) = poses.get(i).map(|p| (unit(p.0), unit(p.1))) else {
            warns.push(Warning::RapidInStock { at: i }); // no direction: no body to clear
            continue;
        };
        // The rapid's swept box: the tip's line, padded by the body
        // every way (the pose may lean anywhere in between).
        let (lo, hi) = boxed(m.from, m.to, body + r + tol);
        let upto = kerfs.partition_point(|c| c.at < i);
        let near: Vec<&Kerf> = kerfs[..upto].iter().rev().filter(|c| overlaps(c.lo, c.hi, lo, hi)).collect();
        let ivs = midpose::intervals(m, a0, a1, body, cx.gouge_tol);
        let first = ivs.first().map(|iv| (iv.p0, iv.a0));
        let mut samples = first.into_iter().chain(ivs.iter().flat_map(|iv| [(iv.pm, iv.am), (iv.p1, iv.a1)]));
        let bad = samples.any(|(tip, a)| {
            let (u, v) = basis(a);
            (0..=k).any(|s| {
                let h = body * s as f64 / k as f64;
                let c = Point3::new(tip.x + a[0] * h, tip.y + a[1] * h, tip.z + a[2] * h);
                if !disc_meets_stock(c, disc, top, solid) || near.iter().any(|kf| disc_in_kerf(c, a, disc, kf, r)) {
                    return false;
                }
                ring.iter().any(|&(du, dv)| {
                    let p = Point3::new(
                        c.x + u[0] * du + v[0] * dv,
                        c.y + u[1] * du + v[1] * dv,
                        c.z + u[2] * du + v[2] * dv,
                    );
                    p.z < top && rest.in_stock(Point2::new(p.x, p.y)) && !near.iter().any(|kf| in_kerf(p, kf, r, tol))
                })
            })
        });
        if bad {
            warns.push(Warning::RapidInStock { at: i });
        }
    }
}

/// A pose as a unit axis: the upright when none is stated, `None` when
/// the stated vector is no direction (zero, non-finite).
fn unit(p: Pose) -> Option<[f64; 3]> {
    p.map_or(Some([0.0, 0.0, 1.0]), normalize_axis)
}

/// The kerf of every cutting move, in cut order: a cylinder about each
/// chord, extended back along the pose by the body when the move runs
/// along its own axis. A cut whose pose is no direction keeps only its
/// tip's chord - less kerf, so only more convictions.
fn kerfs(path: &Toolpath, poses: &[(Pose, Pose)], body: f64, pad: f64, chord_tol: f64) -> Vec<Kerf> {
    let mut out = Vec::new();
    for (j, m) in path.moves.iter().enumerate() {
        if m.kind == MoveKind::Rapid || !(m.from.is_finite() && m.to.is_finite()) {
            continue;
        }
        let tail = tail_of(m, poses.get(j).and_then(|p| unit(p.1)), body);
        for (p, q) in m.segments3(chord_tol) {
            let b = if (p.x, p.y, p.z) == (m.from.x, m.from.y, m.from.z) { tail } else { p };
            let (lo, hi) = boxed(q, b, pad);
            out.push(Kerf { at: j, a: q, b, lo, hi });
        }
    }
    out
}

/// Where the body's far end stood as the cut began: `body` back along
/// the pose for a cut along its own axis, the start otherwise.
fn tail_of(m: &Move, a: Option<[f64; 3]>, body: f64) -> Point3 {
    match a {
        Some(a) if along_axis(m.from, m.to, a) => {
            Point3::new(m.from.x + a[0] * body, m.from.y + a[1] * body, m.from.z + a[2] * body)
        }
        _ => m.from,
    }
}

/// Whether a disc of radius `d` about `c` can touch the stock prism:
/// some of it under the top, its XY box meeting the footprint's.
fn disc_meets_stock(c: Point3, d: f64, top: f64, solid: Option<(Point2, Point2)>) -> bool {
    let Some((lo, hi)) = solid else {
        return false; // an empty footprint holds no stock
    };
    c.z - d < top && c.x + d > lo.x && c.x - d < hi.x && c.y + d > lo.y && c.y - d < hi.y
}

/// The box of segment `p..q`, padded by `pad` on every side.
fn boxed(p: Point3, q: Point3, pad: f64) -> ([f64; 3], [f64; 3]) {
    (
        [p.x.min(q.x) - pad, p.y.min(q.y) - pad, p.z.min(q.z) - pad],
        [p.x.max(q.x) + pad, p.y.max(q.y) + pad, p.z.max(q.z) + pad],
    )
}

fn overlaps(alo: [f64; 3], ahi: [f64; 3], blo: [f64; 3], bhi: [f64; 3]) -> bool {
    (0..3).all(|j| alo[j] <= bhi[j] && blo[j] <= ahi[j])
}

/// Whether `p` is inside the tool-radius cylinder about the kerf's
/// segment (flat ends, `tol` of slack along it).
fn in_kerf(p: Point3, kf: &Kerf, r: f64, tol: f64) -> bool {
    let q = [p.x, p.y, p.z];
    if !overlaps(kf.lo, kf.hi, q, q) {
        return false;
    }
    let d = [kf.b.x - kf.a.x, kf.b.y - kf.a.y, kf.b.z - kf.a.z];
    let w = [p.x - kf.a.x, p.y - kf.a.y, p.z - kf.a.z];
    let len2 = dot(d, d);
    if !(len2.is_finite() && len2 > 1e-18) {
        return dot(w, w) <= r * r;
    }
    let t = dot(w, d) / len2;
    let slack = tol / len2.sqrt();
    if !(-slack..=1.0 + slack).contains(&t) {
        return false;
    }
    let t = t.clamp(0.0, 1.0);
    let e = [w[0] - d[0] * t, w[1] - d[1] * t, w[2] - d[2] * t];
    dot(e, e) <= r * r
}

/// Whether the whole disc of radius `d` about `c`, square to unit `a`,
/// stands inside the kerf: its points sit within `d` of `c` radially
/// and within `d * sin(angle(a, kerf))` of `c` along the kerf, so the
/// centre's place plus that spread inside the cylinder acquits every
/// ring sample at once - the retract riding its own hole's axis.
fn disc_in_kerf(c: Point3, a: [f64; 3], d: f64, kf: &Kerf, r: f64) -> bool {
    let e = [kf.b.x - kf.a.x, kf.b.y - kf.a.y, kf.b.z - kf.a.z];
    let len = dot(e, e).sqrt();
    if !(len.is_finite() && len > 1e-9) {
        return false; // a point kerf: the ring test reads it
    }
    let e = [e[0] / len, e[1] / len, e[2] / len];
    let w = [c.x - kf.a.x, c.y - kf.a.y, c.z - kf.a.z];
    let s = dot(w, e);
    let cos = dot(a, e).clamp(-1.0, 1.0);
    let spread = d * (1.0 - cos * cos).max(0.0).sqrt();
    let radial = [w[0] - e[0] * s, w[1] - e[1] * s, w[2] - e[2] * s];
    s - spread >= 0.0 && s + spread <= len && dot(radial, radial).sqrt() + d <= r
}

/// Whether the move from `p` to `q` runs along axis `a` (either way).
fn along_axis(p: Point3, q: Point3, a: [f64; 3]) -> bool {
    let d = [q.x - p.x, q.y - p.y, q.z - p.z];
    let l2 = dot(d, d);
    if !(l2.is_finite() && l2 > 1e-18) {
        return true; // no travel: the tip's own disc
    }
    let c = dot(d, a);
    l2 - c * c <= 1e-12 * l2.max(1.0)
}

/// The disc's sample offsets in its own (u, v) plane: the centre, an
/// inner ring at half radius, the rim.
fn rings(r: f64) -> Vec<(f64, f64)> {
    let mut out = vec![(0.0, 0.0)];
    for (n, rad) in [(RIM / 2, 0.5 * r), (RIM, r)] {
        for j in 0..n {
            let t = std::f64::consts::TAU * j as f64 / n as f64;
            out.push((rad * t.cos(), rad * t.sin()));
        }
    }
    out
}

/// Two unit vectors square to unit `a` and to each other.
fn basis(a: [f64; 3]) -> ([f64; 3], [f64; 3]) {
    let h = if a[0].abs() < 0.9 { [1.0, 0.0, 0.0] } else { [0.0, 1.0, 0.0] };
    let u = norm(cross(a, h));
    (u, cross(a, u))
}

fn dot(p: [f64; 3], q: [f64; 3]) -> f64 {
    p[0] * q[0] + p[1] * q[1] + p[2] * q[2]
}

fn cross(p: [f64; 3], q: [f64; 3]) -> [f64; 3] {
    [p[1] * q[2] - p[2] * q[1], p[2] * q[0] - p[0] * q[2], p[0] * q[1] - p[1] * q[0]]
}

fn norm(p: [f64; 3]) -> [f64; 3] {
    let l = dot(p, p).sqrt();
    if l > 0.0 && l.is_finite() { [p[0] / l, p[1] / l, p[2] / l] } else { [1.0, 0.0, 0.0] }
}
