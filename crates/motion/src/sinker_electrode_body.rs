//! The sinker EDM BODY electrode a non-rectangular cavity calls for
//! (#1152): the cavity's outline offset inward by the inset, kept as
//! the electrode's section - the outline atom of the stock sim
//! (`stock_sim::simulate_outline`) burns it as it is.
//!
//! Provenance: general die-sink EDM practice from public machine-tool
//! handbooks (the electrode is the cavity's section made smaller by a
//! per-side undersize). The inward offset is the geometry crate's
//! polygon offset (`geo2d::Region::offset_sharp`, Clipper2's published
//! algorithm, mitred joins), cross-checked here by the distance from
//! each offset point to the cavity's boundary; the obstruction test is
//! the same crate's polygon intersection (Vatti clipping). The refusals
//! are our own design (#290 D7). No vendor material.
//!
//! A mitred inward offset is exact at a cavity's convex corners. At a
//! reflex corner (the inside corner of an L) it stands further from
//! the corner than the inset: the electrode is smaller there, never
//! larger, so it leaves stock at that corner and never burns past it.

use crate::geo2d::{Loop, Point2, Region};
use crate::geometry::{Point3, Triangle};
use crate::machining2d::GenError;
use crate::sinker_cavity::Cavity;
use crate::sinker_electrode::{ElectrodeBox, ElectrodeSpec};
use camcraft_tools::electrode_outline::MAX_POINTS;

/// Largest electrode extent, mm - the bound `Tool::is_sane` puts on
/// the record's sizes.
const MAX_SIDE: f64 = 500.0;
/// Smallest electrode extent worth making, mm.
const MIN_SIDE: f64 = 1e-3;
/// Slack between the offset outline and the inset it must keep, mm: a
/// few units of the offset's own lattice.
const OFFSET_SLACK: f64 = 1e-5;
/// How far inside the cavity's column a facet must reach to stand in
/// it, mm - the cavity's own walls and floor lie ON the column's faces.
const CLEAR: f64 = 1e-4;
/// The least overlap that is a facet in the column, mm².
const MIN_OVERLAP: f64 = 1e-9;

fn refuse<T>(why: String) -> Result<T, GenError> {
    Err(GenError::Invalid(format!("electrode from faces: {why}")))
}

/// The distance from `p` to the closed ring's boundary.
fn ring_dist(ring: &[Point2], p: Point2) -> f64 {
    let n = ring.len();
    (0..n)
        .filter_map(|i| Some((ring.get(i)?, ring.get((i + 1) % n)?)))
        .map(|(a, b)| {
            let (ex, ey, rx, ry) = (b.x - a.x, b.y - a.y, p.x - a.x, p.y - a.y);
            let ee = ex * ex + ey * ey;
            let t = if ee > 0.0 { ((rx * ex + ry * ey) / ee).clamp(0.0, 1.0) } else { 0.0 };
            (rx - t * ex).hypot(ry - t * ey)
        })
        .fold(f64::INFINITY, f64::min)
}

fn bounds(ring: &[Point2]) -> (Point2, Point2) {
    ring.iter().fold(
        (Point2::new(f64::INFINITY, f64::INFINITY), Point2::new(f64::NEG_INFINITY, f64::NEG_INFINITY)),
        |(lo, hi), p| (Point2::new(lo.x.min(p.x), lo.y.min(p.y)), Point2::new(hi.x.max(p.x), hi.y.max(p.y))),
    )
}

/// The body electrode `spec` asks for under a cavity that is no
/// axis-aligned rectangle. `region` is the cavity's outline as one
/// simple ring; `orbit` the checked orbit radius.
///
/// # Errors
/// By name: an inset that consumes the cavity or splits it; an offset
/// that stands nearer the cavity's wall than the inset; an outline with
/// more points than the stock sim sweeps; an extent beyond the tool
/// record's bounds; a section the tool record's own law refuses.
pub(crate) fn derive_body(cavity: &Cavity, spec: &ElectrodeSpec, orbit: f64, region: &Region) -> Result<ElectrodeBox, GenError> {
    let inset = spec.undersize + orbit;
    let (clo, chi) = bounds(&cavity.outline);
    let (cx, cy) = (chi.x - clo.x, chi.y - clo.y);
    let consumed = || {
        refuse(format!(
            "an inset of {inset:.3} per side (undersize {:.3} + orbit {orbit:.3}) leaves no electrode in the {cx:.3} x {cy:.3} cavity",
            spec.undersize
        ))
    };
    let shrunk = if inset > 0.0 {
        region
            .offset_sharp(-inset)
            .map_err(|e| GenError::Invalid(format!("electrode from faces: the inward offset refused - {e}")))?
    } else {
        region.clone()
    };
    match shrunk.loops() {
        [one] => body_of(cavity, spec, orbit, region, one.points().to_vec()),
        [] => consumed(),
        many => refuse(format!(
            "an inset of {inset:.3} per side splits the electrode into {} pieces - one electrode burns one cavity",
            many.len()
        )),
    }
}

/// The body electrode of section `outline` (world mm) under `cavity`:
/// every law the offset's answer must keep, so a wrong offset is
/// refused by name and never handed on.
pub(crate) fn body_of(cavity: &Cavity, spec: &ElectrodeSpec, orbit: f64, region: &Region, outline: Vec<Point2>) -> Result<ElectrodeBox, GenError> {
    let depth = cavity.top_z - cavity.floor_z;
    let inset = spec.undersize + orbit;
    let (clo, chi) = bounds(&cavity.outline);
    let (cx, cy) = (chi.x - clo.x, chi.y - clo.y);
    // The published offset, and the law it must keep: no point of the
    // electrode outside the cavity, or nearer its wall than the inset.
    let near_wall = outline
        .iter()
        .find(|p| (!region.contains(**p) && ring_dist(&cavity.outline, **p) > OFFSET_SLACK) || ring_dist(&cavity.outline, **p) < inset - OFFSET_SLACK);
    if let Some(p) = near_wall {
        return refuse(format!(
            "the offset outline stands {:.6} from the cavity's wall at ({:.3}, {:.3}), nearer than the inset {inset:.6} - refused rather than guessed (a bug; please report it)",
            ring_dist(&cavity.outline, *p),
            p.x,
            p.y
        ));
    }
    let (lo, hi) = bounds(&outline);
    let face = (hi.x - lo.x, hi.y - lo.y);
    if !(face.0 >= MIN_SIDE && face.1 >= MIN_SIDE) {
        return refuse(format!(
            "an inset of {inset:.3} per side leaves an electrode of {:.6} x {:.6} in the {cx:.3} x {cy:.3} cavity - under the {MIN_SIDE} mm worth making",
            face.0, face.1
        ));
    }
    if outline.len() > MAX_POINTS {
        return refuse(format!(
            "the cavity's outline gives an electrode of {} points, more than the {MAX_POINTS} the stock sim sweeps - an outline that fine is not modelled",
            outline.len()
        ));
    }
    if face.0 > MAX_SIDE || face.1 > MAX_SIDE {
        return refuse(format!(
            "the electrode's extent {:.3} x {:.3} is larger than the {MAX_SIDE} mm a tool record holds",
            face.0, face.1
        ));
    }
    let length = depth + spec.stub;
    if !(length.is_finite() && length > depth) {
        return refuse(format!(
            "the electrode's length {length} does not stand above the {depth:.3} deep cavity - its holder would enter the work"
        ));
    }
    let e = ElectrodeBox {
        face,
        center: (0.5 * (lo.x + hi.x), 0.5 * (lo.y + hi.y)),
        cavity: (cx, cy),
        floor_z: cavity.floor_z,
        top_z: cavity.top_z,
        depth,
        length,
        pickup: spec.pickup,
        inset,
        orbit,
        outline,
        body: true,
        ring: cavity.outline.clone(),
    };
    // The record's own door: a section it refuses is never handed on.
    if let Some(why) = e.tool().torch_refusal() {
        return refuse(format!("the offset outline is no electrode section - {why}"));
    }
    Ok(e)
}

/// The electrode's section about its burn point: the tool record's
/// outline.
pub(crate) fn section(e: &ElectrodeBox) -> Vec<[f64; 2]> {
    e.outline.iter().map(|p| [p.x - e.center.0, p.y - e.center.1]).collect()
}

/// Keep the part of a convex polygon above `z` (Sutherland-Hodgman,
/// one clip plane).
fn above(poly: &[Point3], z: f64) -> Vec<Point3> {
    let n = poly.len();
    let mut out = Vec::with_capacity(n + 1);
    for i in 0..n {
        let (Some(&a), Some(&b)) = (poly.get(i), poly.get((i + 1) % n)) else { continue };
        let (da, db) = (a.z - z, b.z - z);
        if da > 0.0 {
            out.push(a);
        }
        if (da > 0.0) != (db > 0.0) && (da - db).abs() > 0.0 {
            let t = da / (da - db);
            out.push(Point3::new(a.x + (b.x - a.x) * t, a.y + (b.y - a.y) * t, a.z + (b.z - a.z) * t));
        }
    }
    out
}

/// Where the segment `a`-`b` runs inside `column`, as a point of it;
/// `None` when it never does. The segment is cut at every crossing of
/// the column's boundary and each piece is judged at its middle.
fn segment_in(column: &Region, a: Point2, b: Point2) -> Option<Point2> {
    let mut cuts = vec![0.0, 1.0];
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    for l in column.loops() {
        let pts = l.points();
        let n = pts.len();
        for i in 0..n {
            let (Some(p), Some(q)) = (pts.get(i), pts.get((i + 1) % n)) else { continue };
            let (ex, ey) = (q.x - p.x, q.y - p.y);
            let den = dx * ey - dy * ex;
            if den.abs() <= f64::EPSILON {
                continue;
            }
            let t = ((p.x - a.x) * ey - (p.y - a.y) * ex) / den;
            let s = ((p.x - a.x) * dy - (p.y - a.y) * dx) / den;
            if (0.0..=1.0).contains(&t) && (0.0..=1.0).contains(&s) {
                cuts.push(t);
            }
        }
    }
    cuts.sort_by(f64::total_cmp);
    cuts.windows(2).find_map(|w| {
        let [t0, t1] = w else { return None };
        if t1 - t0 <= 1e-12 {
            return None;
        }
        let t = 0.5 * (t0 + t1);
        let m = Point2::new(a.x + dx * t, a.y + dy * t);
        column.contains(m).then_some(m)
    })
}

/// Where the model stands inside a body electrode's cavity column
/// above its floor - a boss, a rib, an overhang across the mouth - or
/// `None` when the column is clear. The column is the cavity's own
/// outline, not its bounds: the part round a rounded pocket's corner is
/// not in the electrode's way. `Err` when the column cannot be built.
pub(crate) fn standing_in_ring(e: &ElectrodeBox, facets: &[Triangle]) -> Result<Option<Point3>, GenError> {
    let built = Loop::new(e.ring.clone())
        .and_then(|l| Region::new(vec![l]))
        .and_then(|r| r.offset_sharp(-CLEAR))
        .map_err(|err| GenError::Invalid(format!("electrode from faces: the cavity's column could not be built ({err}) - no electrode is made")))?;
    let Some((lo, hi)) = built.bounds() else { return Ok(None) };
    let floor = e.floor_z + CLEAR;
    for t in facets {
        if !(t.a.is_finite() && t.b.is_finite() && t.c.is_finite()) {
            continue;
        }
        let poly = above(&[t.a, t.b, t.c], floor);
        if poly.len() < 3 {
            continue;
        }
        let (plo, phi) = bounds(&poly.iter().map(|p| Point2::new(p.x, p.y)).collect::<Vec<_>>());
        if phi.x < lo.x || plo.x > hi.x || phi.y < lo.y || plo.y > hi.y {
            continue;
        }
        let z = poly.iter().map(|p| p.z).fold(f64::NEG_INFINITY, f64::max);
        let flat: Vec<Point2> = poly.iter().map(|p| Point2::new(p.x, p.y)).collect();
        // A facet with a footprint: its overlap with the column.
        let overlap = Loop::new(flat.clone())
            .ok()
            .filter(|l| l.area().abs() > MIN_OVERLAP)
            .and_then(|l| Region::new(vec![l]).ok())
            .and_then(|r| r.intersect(&built).ok());
        if let Some(o) = &overlap {
            if o.area() > MIN_OVERLAP
                && let Some(l) = o.loops().first()
            {
                let k = l.points().len().max(1) as f64;
                let c = l.points().iter().fold((0.0, 0.0), |s, p| (s.0 + p.x / k, s.1 + p.y / k));
                return Ok(Some(Point3::new(c.0, c.1, z)));
            }
            continue;
        }
        // An upright facet has no footprint: its edges are judged.
        let n = flat.len();
        for i in 0..n {
            let (Some(&a), Some(&b)) = (flat.get(i), flat.get((i + 1) % n)) else { continue };
            if let Some(m) = segment_in(&built, a, b) {
                return Ok(Some(Point3::new(m.x, m.y, z)));
            }
        }
    }
    Ok(None)
}
