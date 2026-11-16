//! The sinker EDM electrode a cavity calls for (#547): the cavity's
//! outline offset inward by the electrode's undersize (and an orbit
//! radius, when one is stated), extended above the cavity's mouth to a
//! holder stub - as the box electrode of #541, or a refusal by name.
//!
//! Provenance: general die-sink EDM practice from public machine-tool
//! handbooks - an electrode is the cavity's shape made smaller by a
//! per-side undersize, and smaller again by the radius of any orbit it
//! is to run, so its burn reaches the finished size; it stands proud of
//! the work so its holder never enters the cavity. The inward offset is
//! the polygon offset of the geometry crate (`geo2d::Region`, Clipper2's
//! published algorithm: Vatti clipping with mitred joins), and the
//! obstruction test is Sutherland & Hodgman's polygon clipping
//! ("Reentrant polygon clipping", CACM 17(1), 1974) of each facet
//! against the cavity's column. The refusals are our own design (#290
//! D1-D10). No vendor material.
//!
//! What it makes: a cavity whose outline IS an axis-aligned rectangle
//! yields the box electrode the stock sim burns with the box atom
//! (`stock_sim::simulate_box`, #541). Every other outline yields a body
//! electrode (#1152, `sinker_electrode_body`): the offset outline is
//! the electrode's section, burned by the outline atom
//! (`stock_sim::simulate_outline`).

use crate::geo2d::{Loop, Point2, Region};
use crate::geometry::{Point3, Triangle};
use crate::machining2d::GenError;
use crate::sinker_cavity::{Cavity, MIN_DEPTH, TOL};
use crate::tools::Tool;

/// Largest electrode face side, mm - the bound `Tool::is_sane` puts on
/// the record's sizes.
const MAX_SIDE: f64 = 500.0;
/// Smallest electrode face side - and holder stub - worth making, mm.
const MIN_SIDE: f64 = 1e-3;
/// Slack between the offset polygon and the arithmetic it must agree
/// with, mm: a few units of the offset's own lattice.
const OFFSET_SLACK: f64 = 1e-5;
/// How far inside the cavity's column a facet must reach to stand in
/// it, mm - the cavity's own walls and floor lie ON the column's faces.
const CLEAR: f64 = 1e-4;

/// What the user states about the electrode, mm.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ElectrodeSpec {
    /// The undersize per side: how far the electrode stands inside the
    /// finished wall (the condition's, #543).
    pub undersize: f64,
    /// The radius of the orbit the electrode will run, when one is
    /// stated (#544): the electrode is made that much smaller again, so
    /// the orbit's sweep - not the electrode at rest - reaches the size.
    pub orbit: Option<f64>,
    /// How far the electrode stands above the cavity's mouth: the stub
    /// its holder grips, kept out of the work.
    pub stub: f64,
    /// The pickup offset: from the electrode's top face up to the
    /// reference the control measures it from.
    pub pickup: f64,
}

/// The electrode a cavity calls for, world mm.
#[derive(Clone, Debug, PartialEq)]
pub struct ElectrodeBox {
    /// The electrode face's X and Y size.
    pub face: (f64, f64),
    /// The face centre: where the burn is made.
    pub center: (f64, f64),
    /// The finished cavity's X and Y size.
    pub cavity: (f64, f64),
    pub floor_z: f64,
    pub top_z: f64,
    /// The cavity's depth, mouth to floor.
    pub depth: f64,
    /// The electrode's length below its holder: the depth plus the stub.
    pub length: f64,
    pub pickup: f64,
    /// The inward offset per side: the undersize plus the orbit radius.
    pub inset: f64,
    /// The orbit radius the electrode was made for (0 for none).
    pub orbit: f64,
    /// The offset outline, counter-clockwise: the electrode's section.
    pub outline: Vec<Point2>,
    /// A body electrode (#1152): the section is `outline`, not the
    /// rectangle `face` (which is then the outline's X and Y extents).
    pub body: bool,
    /// The cavity's own outline, counter-clockwise: its column.
    pub ring: Vec<Point2>,
}

impl ElectrodeBox {
    /// The electrode as #541's tool record; a body electrode's record
    /// carries its section about the burn point (#1152).
    #[must_use]
    pub fn tool(&self) -> Tool {
        if self.body {
            let section = crate::sinker_electrode_body::section(self);
            return camcraft_tools::electrode::electrode_outline_tool(&section, self.length, self.pickup);
        }
        camcraft_tools::electrode::electrode_tool(self.face.0, self.face.1, self.length, self.pickup)
    }
}

fn refuse<T>(why: String) -> Result<T, GenError> {
    Err(GenError::Invalid(format!("electrode from faces: {why}")))
}

/// The axis-aligned rectangle a simple ring IS, within `tol`: every
/// vertex on its bounds, all four corners present, and the ring's area
/// the bounds' own. `None` for any other shape - a rotated rectangle, a
/// rounded corner, a notch.
#[must_use]
pub fn rect_of(ring: &[Point2], tol: f64) -> Option<(Point2, Point2)> {
    if ring.len() < 4 || !ring.iter().all(|p| p.x.is_finite() && p.y.is_finite()) {
        return None;
    }
    let (mut lo, mut hi) = (Point2::new(f64::INFINITY, f64::INFINITY), Point2::new(f64::NEG_INFINITY, f64::NEG_INFINITY));
    for p in ring {
        lo = Point2::new(lo.x.min(p.x), lo.y.min(p.y));
        hi = Point2::new(hi.x.max(p.x), hi.y.max(p.y));
    }
    let (w, h) = (hi.x - lo.x, hi.y - lo.y);
    if !(w > tol && h > tol) {
        return None;
    }
    let near = |a: f64, b: f64| (a - b).abs() <= tol;
    if !ring.iter().all(|p| near(p.x, lo.x) || near(p.x, hi.x) || near(p.y, lo.y) || near(p.y, hi.y)) {
        return None;
    }
    let corners = [(lo.x, lo.y), (hi.x, lo.y), (hi.x, hi.y), (lo.x, hi.y)];
    if !corners.iter().all(|c| ring.iter().any(|p| near(p.x, c.0) && near(p.y, c.1))) {
        return None;
    }
    let n = ring.len();
    let area: f64 = (0..n)
        .filter_map(|i| Some((ring.get(i)?, ring.get((i + 1) % n)?)))
        .map(|(a, b)| a.x * b.y - b.x * a.y)
        .sum::<f64>()
        .abs()
        * 0.5;
    (area >= w * h - tol * 2.0 * (w + h)).then_some((lo, hi))
}

fn check_spec(s: &ElectrodeSpec) -> Result<f64, GenError> {
    if !(s.undersize.is_finite() && s.undersize >= 0.0) {
        return refuse(format!("the undersize {} is not a finite, non-negative number of mm", s.undersize));
    }
    let orbit = s.orbit.unwrap_or(0.0);
    if !(orbit.is_finite() && orbit >= 0.0) {
        return refuse(format!("the orbit radius {orbit} is not a finite, non-negative number of mm"));
    }
    if !(s.stub.is_finite() && s.stub >= MIN_SIDE) {
        return refuse(format!(
            "the holder stub {} must be a positive length (at least {MIN_SIDE} mm) - the electrode stands that far above the cavity so its holder stays out of the work",
            s.stub
        ));
    }
    if !(s.pickup.is_finite() && s.pickup >= 0.0) {
        return refuse(format!("the pickup offset {} is not a finite, non-negative number of mm", s.pickup));
    }
    Ok(orbit)
}

/// The electrode `spec` asks for under `cavity`.
///
/// # Errors
/// By name: a garbled spec; an outline that crosses itself; an inset
/// that consumes the cavity or splits it; a face beyond the tool
/// record's bounds; an offset that disagrees with its own arithmetic;
/// and, for a cavity that is no axis-aligned rectangle, the refusals of
/// `sinker_electrode_body::derive_body`.
pub fn derive(cavity: &Cavity, spec: &ElectrodeSpec) -> Result<ElectrodeBox, GenError> {
    let orbit = check_spec(spec)?;
    let depth = cavity.top_z - cavity.floor_z;
    if !(cavity.floor_z.is_finite() && cavity.top_z.is_finite() && depth >= MIN_DEPTH) {
        return refuse(format!("the cavity has no depth (floor Z {}, mouth Z {})", cavity.floor_z, cavity.top_z));
    }
    let ring = Loop::new(cavity.outline.clone())
        .map_err(|e| GenError::Invalid(format!("electrode from faces: the floor's outline is not a boundary - {e}")))?;
    let region = Region::new(vec![ring]).map_err(|e| {
        GenError::Invalid(format!(
            "electrode from faces: the floor's outline is self-intersecting ({e}) - it bounds no cavity"
        ))
    })?;
    let [outer] = region.loops() else {
        return refuse(format!(
            "the floor's outline is self-intersecting - it falls into {} pieces, not one cavity",
            region.loops().len()
        ));
    };
    // The rectangle is read off the picked ring itself (exact corners);
    // the region's ring - the same shape on the offset's lattice - must
    // read as one too.
    let rect = rect_of(&cavity.outline, TOL).filter(|_| rect_of(outer.points(), 2.0 * TOL).is_some());
    let Some((lo, hi)) = rect else {
        // #1152: any other outline is a body electrode's section.
        return crate::sinker_electrode_body::derive_body(cavity, spec, orbit, &region);
    };
    let (cx, cy) = (hi.x - lo.x, hi.y - lo.y);
    let inset = spec.undersize + orbit;
    let face = (cx - 2.0 * inset, cy - 2.0 * inset);
    let consumed = || {
        refuse(format!(
            "an inset of {inset:.3} per side (undersize {:.3} + orbit {orbit:.3}) leaves no electrode in the {cx:.3} x {cy:.3} cavity",
            spec.undersize
        ))
    };
    if !(face.0 >= MIN_SIDE && face.1 >= MIN_SIDE) {
        return consumed();
    }
    // The published offset, and the arithmetic it must agree with: a
    // rectangle's inward mitred offset is the rectangle shrunk per side.
    let shrunk = if inset > 0.0 {
        region
            .offset_sharp(-inset)
            .map_err(|e| GenError::Invalid(format!("electrode from faces: the inward offset refused - {e}")))?
    } else {
        region.clone()
    };
    let outline = match shrunk.loops() {
        [one] => one.points().to_vec(),
        [] => return consumed(),
        many => {
            return refuse(format!(
                "an inset of {inset:.3} per side splits the electrode into {} pieces - one electrode burns one cavity",
                many.len()
            ));
        }
    };
    let agrees = rect_of(&outline, OFFSET_SLACK).is_some_and(|(a, b)| {
        let near = |p: f64, q: f64| (p - q).abs() <= OFFSET_SLACK;
        near(a.x, lo.x + inset) && near(a.y, lo.y + inset) && near(b.x, hi.x - inset) && near(b.y, hi.y - inset)
    });
    if !agrees {
        return refuse(
            "the offset outline disagrees with the cavity's bounds shrunk by the inset - refused rather than guessed (a bug; please report it)".into(),
        );
    }
    if face.0 > MAX_SIDE || face.1 > MAX_SIDE {
        return refuse(format!(
            "the electrode face {:.3} x {:.3} is larger than the {MAX_SIDE} mm a tool record holds",
            face.0, face.1
        ));
    }
    let length = depth + spec.stub;
    if !(length.is_finite() && length > depth) {
        return refuse(format!(
            "the electrode's length {length} does not stand above the {depth:.3} deep cavity - its holder would enter the work"
        ));
    }
    Ok(ElectrodeBox {
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
        body: false,
        ring: cavity.outline.clone(),
    })
}

/// Keep the part of a convex polygon where `side` is positive
/// (Sutherland-Hodgman, one clip plane).
fn clip(poly: &[Point3], side: impl Fn(Point3) -> f64) -> Vec<Point3> {
    let n = poly.len();
    let mut out = Vec::with_capacity(n + 1);
    for i in 0..n {
        let (Some(&a), Some(&b)) = (poly.get(i), poly.get((i + 1) % n)) else { continue };
        let (da, db) = (side(a), side(b));
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

/// A planar polygon's area (Newell's vector area, its length).
fn area(poly: &[Point3]) -> f64 {
    let Some(&o) = poly.first() else { return 0.0 };
    let mut s = Point3::ZERO;
    for w in poly.windows(2) {
        if let [a, b] = w {
            let c = (*a - o).cross(*b - o);
            s = Point3::new(s.x + c.x, s.y + c.y, s.z + c.z);
        }
    }
    0.5 * s.dot(s).sqrt()
}

/// Where the model stands inside the cavity's column above its floor -
/// a boss on the floor, a rib, an overhang across the mouth - or `None`
/// when the column is clear. The electrode is fed down that column, so
/// anything in it is in the electrode's way. Facets with a non-finite
/// corner are skipped: they bound nothing a burn could meet. A body
/// electrode's column is the cavity's own outline (#1152).
///
/// # Errors
/// A body electrode's column that cannot be built, by name.
pub fn standing_in(e: &ElectrodeBox, facets: &[Triangle]) -> Result<Option<Point3>, GenError> {
    if e.body {
        return crate::sinker_electrode_body::standing_in_ring(e, facets);
    }
    Ok(standing_in_box(e, facets))
}

fn standing_in_box(e: &ElectrodeBox, facets: &[Triangle]) -> Option<Point3> {
    let (hx, hy) = (0.5 * e.cavity.0 - CLEAR, 0.5 * e.cavity.1 - CLEAR);
    if !(hx > 0.0 && hy > 0.0) {
        return None;
    }
    let (cx, cy, floor) = (e.center.0, e.center.1, e.floor_z + CLEAR);
    for t in facets {
        if !(t.a.is_finite() && t.b.is_finite() && t.c.is_finite()) {
            continue;
        }
        let mut poly = vec![t.a, t.b, t.c];
        poly = clip(&poly, |p| p.z - floor);
        poly = clip(&poly, |p| p.x - (cx - hx));
        poly = clip(&poly, |p| (cx + hx) - p.x);
        poly = clip(&poly, |p| p.y - (cy - hy));
        poly = clip(&poly, |p| (cy + hy) - p.y);
        if poly.len() < 3 || area(&poly) < 1e-12 {
            continue;
        }
        let k = poly.len() as f64;
        let c = poly.iter().fold(Point3::ZERO, |s, p| Point3::new(s.x + p.x / k, s.y + p.y / k, s.z + p.z / k));
        if c.is_finite() {
            return Some(c);
        }
    }
    None
}
