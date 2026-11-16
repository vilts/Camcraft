//! A sinker EDM cavity read from picked part faces (#547): one level
//! floor and the vertical walls standing on every edge of it, checked
//! face by face and edge by edge, and refused by name for anything
//! else: nothing is guessed from a face set that does not spell one
//! cavity.
//!
//! Provenance: our own reading over the planar-face boundary
//! representation this repo machines from, by the published
//! depression-feature rule (a pocket is a face whose boundary edges are
//! concave: Joshi & Chang, "Graph-based heuristics for recognition of
//! machined features from a 3D solid model", CAD 20(2), 1988), with
//! Newell's method for a planar ring's vector area (Sutherland, Sproull
//! & Schumacker, "A characterization of ten hidden-surface algorithms",
//! ACM Computing Surveys 6(1), 1974). General die-sink EDM practice from
//! public machine-tool handbooks: the electrode is the cavity's shape,
//! fed down Z. No vendor material.
//!
//! Robustness (Prime Directive): total on any input. A non-finite or
//! out-of-range coordinate, a face with no area, a boundary that does
//! not close, a curved or leaning wall, a second floor, a wall of
//! another cavity, or a floor edge no wall stands on is each a refusal
//! that names the face; no arithmetic runs on an unchecked number.

use crate::geo2d::Point2;
use crate::geometry::Point3;
use crate::machining2d::GenError;

/// Two points this close are the same point, mm (1 nm: the weld lattice
/// of `geometry::feature`).
pub const TOL: f64 = 1e-6;
/// A unit normal this close to +Z is level.
const LEVEL: f64 = 0.999_999;
/// A unit normal with |z| under this is vertical.
const VERTICAL: f64 = 1e-6;
/// A ring under this area bounds nothing, mm².
const MIN_AREA: f64 = 1e-9;
/// The least rise of a wall above its floor, mm.
pub const MIN_DEPTH: f64 = 1e-3;
/// Coordinates beyond this are no part on any machine, mm - and the
/// bound that keeps every product below finite.
const COORD_LIMIT: f64 = 1e6;

/// One picked face, in world coordinates.
#[derive(Clone, Debug, PartialEq)]
pub struct PickedFace {
    /// How a refusal names it ("face 0x12ab of 'part'").
    pub label: String,
    /// The outward unit normal by the facets' winding: it points into
    /// the air, so into the cavity for a cavity's own faces.
    pub normal: Point3,
    /// A welded smooth (curved) face rather than one plane.
    pub curved: bool,
    /// Its boundary refused to close into rings.
    pub open: bool,
    /// The outer boundary ring, in order (either winding).
    pub outer: Vec<Point3>,
}

/// One cavity: its floor's outline and the heights its walls span.
#[derive(Clone, Debug, PartialEq)]
pub struct Cavity {
    /// The floor's outer ring in world XY, counter-clockwise from above.
    pub outline: Vec<Point2>,
    pub floor_z: f64,
    /// The highest point of the picked walls: the cavity's mouth.
    pub top_z: f64,
    /// How many walls stand on the floor.
    pub walls: usize,
}

fn refuse<T>(why: String) -> Result<T, GenError> {
    Err(GenError::Invalid(format!("electrode from faces: {why}")))
}

fn in_range(p: Point3) -> bool {
    p.is_finite() && p.x.abs() < COORD_LIMIT && p.y.abs() < COORD_LIMIT && p.z.abs() < COORD_LIMIT
}

/// Newell's vector area of a ring: its length is the ring's area, its
/// direction the ring's plane normal.
fn vector_area(ring: &[Point3]) -> Point3 {
    let Some(&o) = ring.first() else { return Point3::ZERO };
    let mut s = Point3::ZERO;
    for w in ring.windows(2) {
        if let [a, b] = w {
            let c = (*a - o).cross(*b - o);
            s = Point3::new(s.x + c.x, s.y + c.y, s.z + c.z);
        }
    }
    Point3::new(0.5 * s.x, 0.5 * s.y, 0.5 * s.z)
}

/// Every refusal one face can earn on its own, by name.
fn check_face(f: &PickedFace) -> Result<(), GenError> {
    let l = &f.label;
    if f.curved {
        return refuse(format!(
            "{l} is a curved wall - an electrode is generated from planar walls only (a non-planar wall is not modelled)"
        ));
    }
    if f.open || f.outer.len() < 3 {
        return refuse(format!("{l} has an open loop - its boundary does not close, so it bounds no face"));
    }
    if !f.normal.is_finite() || !f.outer.iter().all(|p| p.is_finite()) {
        return refuse(format!("{l} has a coordinate that is not a finite number"));
    }
    if !f.outer.iter().copied().all(in_range) {
        return refuse(format!("{l} lies outside the machinable range ({COORD_LIMIT:e} mm)"));
    }
    let len = f.normal.dot(f.normal).sqrt();
    if (len - 1.0).abs() > 1e-6 {
        return refuse(format!("{l} has no usable normal - re-pick it"));
    }
    let Some(&o) = f.outer.first() else {
        return refuse(format!("{l} has an open loop - its boundary does not close, so it bounds no face"));
    };
    if let Some(off) = f.outer.iter().map(|p| (*p - o).dot(f.normal).abs()).find(|d| *d > 10.0 * TOL) {
        return refuse(format!(
            "{l} is not planar - its boundary leaves its plane by {off:.6} mm (a non-planar wall is not modelled)"
        ));
    }
    let a = vector_area(&f.outer);
    if a.dot(a).sqrt() < MIN_AREA {
        return refuse(format!("{l} has zero area - it bounds nothing"));
    }
    Ok(())
}

fn xy(p: Point3) -> Point2 {
    Point2::new(p.x, p.y)
}

/// The floor ring in world XY, counter-clockwise from above, with
/// repeated points dropped.
fn floor_ring(f: &PickedFace) -> Vec<Point2> {
    let mut pts: Vec<Point2> = Vec::with_capacity(f.outer.len());
    for p in f.outer.iter().copied().map(xy) {
        if pts.last().is_none_or(|q| (q.x - p.x).hypot(q.y - p.y) > TOL) {
            pts.push(p);
        }
    }
    if pts.len() > 1 && pts.first().zip(pts.last()).is_some_and(|(a, b)| (a.x - b.x).hypot(a.y - b.y) <= TOL) {
        pts.pop();
    }
    let n = pts.len();
    let signed: f64 = (0..n)
        .filter_map(|i| Some((pts.get(i)?, pts.get((i + 1) % n)?)))
        .map(|(a, b)| a.x * b.y - b.x * a.y)
        .sum();
    if signed < 0.0 {
        pts.reverse();
    }
    pts
}

/// One wall's stand on the floor ring: the spans `(edge, from, to)` in
/// mm along each floor edge its bottom edge runs on.
fn stand_of(w: &PickedFace, ring: &[Point2], floor_z: f64) -> Result<Vec<(usize, f64, f64)>, GenError> {
    let l = &w.label;
    let (lo, hi) = w.outer.iter().fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), p| (lo.min(p.z), hi.max(p.z)));
    if lo < floor_z - TOL {
        return refuse(format!(
            "{l} reaches Z {lo:.3}, below the picked floor at Z {floor_z:.3} - it is not a wall of this cavity (faces from different cavities?)"
        ));
    }
    if hi < floor_z + MIN_DEPTH {
        return refuse(format!("{l} does not rise above the picked floor at Z {floor_z:.3} - it is not a wall of this cavity"));
    }
    let m = w.outer.len();
    let bottoms: Vec<(Point2, Point2)> = (0..m)
        .filter_map(|i| Some((*w.outer.get(i)?, *w.outer.get((i + 1) % m)?)))
        .filter(|(p, q)| (p.z - floor_z).abs() <= TOL && (q.z - floor_z).abs() <= TOL)
        .map(|(p, q)| (xy(p), xy(q)))
        .collect();
    let n = ring.len();
    let mut spans = Vec::new();
    for i in 0..n {
        let (Some(&a), Some(&b)) = (ring.get(i), ring.get((i + 1) % n)) else { continue };
        let len = (b.x - a.x).hypot(b.y - a.y);
        if len <= TOL {
            continue;
        }
        let u = ((b.x - a.x) / len, (b.y - a.y) / len);
        for (p, q) in &bottoms {
            let off = |r: &Point2| (u.0 * (r.y - a.y) - u.1 * (r.x - a.x)).abs();
            if off(p) > TOL || off(q) > TOL {
                continue;
            }
            let along = |r: &Point2| u.0 * (r.x - a.x) + u.1 * (r.y - a.y);
            let (tp, tq) = (along(p), along(q));
            let (from, to) = (tp.min(tq).max(0.0), tp.max(tq).min(len));
            if to - from <= TOL {
                continue;
            }
            // Into the cavity is the left of a counter-clockwise ring.
            if w.normal.x * -u.1 + w.normal.y * u.0 < LEVEL {
                return refuse(format!(
                    "{l} stands on the picked floor but faces away from it - it is the outside of a boss or of another cavity, not a wall of this one"
                ));
            }
            spans.push((i, from, to));
        }
    }
    if spans.is_empty() {
        return refuse(format!(
            "{l} does not stand on the picked floor - it is a face of a different cavity (or no wall of this one)"
        ));
    }
    Ok(spans)
}

/// The cavity a face set spells: exactly one level floor facing up, and
/// vertical planar walls standing on every edge of its outer ring.
///
/// # Errors
/// By name: no faces; a face that is curved, open, non-finite, out of
/// range, non-planar, zero-area, leaning or facing down; no floor or
/// more than one; no walls; a wall of another cavity; a floor edge with
/// no wall (the walls do not close).
pub fn cavity_of(faces: &[PickedFace]) -> Result<Cavity, GenError> {
    if faces.is_empty() {
        return refuse("no faces are picked - pick the cavity's floor and the walls standing on it".into());
    }
    let (mut floors, mut walls) = (Vec::new(), Vec::new());
    for f in faces {
        check_face(f)?;
        let nz = f.normal.z;
        if nz >= LEVEL {
            floors.push(f);
        } else if nz.abs() <= VERTICAL {
            walls.push(f);
        } else if nz <= -LEVEL {
            return refuse(format!("{} faces down - a cavity's floor faces up and its walls are vertical", f.label));
        } else {
            return refuse(format!(
                "{} leans {:.3} degrees off vertical - a drafted wall or a tilted floor is not modelled",
                f.label,
                nz.abs().min(1.0).asin().to_degrees()
            ));
        }
    }
    let floor = match floors.as_slice() {
        [one] => *one,
        [] => return refuse("no floor is picked - pick the cavity's level floor with its walls".into()),
        many => {
            let names: Vec<&str> = many.iter().map(|f| f.label.as_str()).collect();
            return refuse(format!(
                "{} floors are picked ({}) - one cavity has one level floor; these are faces from different cavities, or a stepped cavity, which is not modelled",
                many.len(),
                names.join(", ")
            ));
        }
    };
    if walls.is_empty() {
        return refuse(format!(
            "only the floor ({}) is picked - pick the walls standing on it too (they give the cavity its depth)",
            floor.label
        ));
    }
    let ring = floor_ring(floor);
    if ring.len() < 3 {
        return refuse(format!("{} has zero area - it bounds nothing", floor.label));
    }
    let floor_z = floor.outer.iter().map(|p| p.z).sum::<f64>() / floor.outer.len() as f64;
    if let Some(off) = floor.outer.iter().map(|p| (p.z - floor_z).abs()).find(|d| *d > 10.0 * TOL) {
        return refuse(format!(
            "{} is not level - its Z leaves the floor's by {off:.6} mm (a tilted floor is not modelled)",
            floor.label
        ));
    }
    let n = ring.len();
    let mut cover: Vec<Vec<(f64, f64)>> = vec![Vec::new(); n];
    let mut top_z = f64::NEG_INFINITY;
    for w in &walls {
        for (i, from, to) in stand_of(w, &ring, floor_z)? {
            if let Some(c) = cover.get_mut(i) {
                c.push((from, to));
            }
        }
        top_z = w.outer.iter().fold(top_z, |t, p| t.max(p.z));
    }
    for (i, spans) in cover.iter_mut().enumerate() {
        let (Some(&a), Some(&b)) = (ring.get(i), ring.get((i + 1) % n)) else { continue };
        let len = (b.x - a.x).hypot(b.y - a.y);
        if len <= TOL {
            continue;
        }
        spans.sort_by(|p, q| p.0.total_cmp(&q.0));
        let mut reach = 0.0_f64;
        for (from, to) in spans.iter() {
            if *from > reach + TOL {
                break;
            }
            reach = reach.max(*to);
        }
        if len - reach > TOL {
            return refuse(format!(
                "the picked walls do not close around the floor: its edge from ({:.3}, {:.3}) to ({:.3}, {:.3}) has no wall for {:.3} mm - pick every wall of the cavity (an open-sided cavity is not modelled)",
                a.x,
                a.y,
                b.x,
                b.y,
                len - reach
            ));
        }
    }
    if !(top_z.is_finite() && top_z - floor_z >= MIN_DEPTH) {
        return refuse(format!("the walls do not rise above the floor at Z {floor_z:.3} - the cavity has no depth"));
    }
    Ok(Cavity { outline: ring, floor_z, top_z, walls: walls.len() })
}
