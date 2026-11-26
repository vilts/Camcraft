//! Integer grid primitives and Hertel-Mehlhorn convex polygon decomposition.
//! Provenance: Hertel-Mehlhorn convex merging over triangulated regions (`Region::triangles`).

use super::types::NestError;
use crate::geo2d::{Point2, Region, SCALE};
use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct IPoint {
    pub x: i64,
    pub y: i64,
}

impl IPoint {
    pub fn from_point2(p: Point2) -> Result<Self, NestError> {
        if !(p.x.is_finite() && p.y.is_finite()) {
            return Err(NestError::InvalidInput("non-finite coordinate".into()));
        }
        let sx = (p.x * SCALE).round();
        let sy = (p.y * SCALE).round();
        if sx.abs() >= (i64::MAX / 4) as f64 || sy.abs() >= (i64::MAX / 4) as f64 {
            return Err(NestError::OutOfRange(
                "coordinate exceeds integer grid range".into(),
            ));
        }
        Ok(IPoint {
            x: sx as i64,
            y: sy as i64,
        })
    }

    pub fn to_point2(self) -> Point2 {
        Point2::new(self.x as f64 / SCALE, self.y as f64 / SCALE)
    }

    pub fn add(self, o: Self) -> Result<Self, NestError> {
        let x = self
            .x
            .checked_add(o.x)
            .ok_or_else(|| NestError::OutOfRange("x overflow".into()))?;
        let y = self
            .y
            .checked_add(o.y)
            .ok_or_else(|| NestError::OutOfRange("y overflow".into()))?;
        Ok(IPoint { x, y })
    }

    pub fn neg(self) -> Result<Self, NestError> {
        let x = self
            .x
            .checked_neg()
            .ok_or_else(|| NestError::OutOfRange("x overflow".into()))?;
        let y = self
            .y
            .checked_neg()
            .ok_or_else(|| NestError::OutOfRange("y overflow".into()))?;
        Ok(IPoint { x, y })
    }
}

pub(crate) fn cross_i128(o: IPoint, a: IPoint, b: IPoint) -> i128 {
    let ax = (a.x as i128) - (o.x as i128);
    let ay = (a.y as i128) - (o.y as i128);
    let bx = (b.x as i128) - (o.x as i128);
    let by = (b.y as i128) - (o.y as i128);
    ax * by - ay * bx
}

fn upper_half(dx: i64, dy: i64) -> bool {
    dy > 0 || (dy == 0 && dx > 0)
}

pub(crate) fn cmp_edge_angle(e1: (i64, i64), e2: (i64, i64)) -> Ordering {
    let u1 = upper_half(e1.0, e1.1);
    let u2 = upper_half(e2.0, e2.1);
    if u1 != u2 {
        return if u1 {
            Ordering::Less
        } else {
            Ordering::Greater
        };
    }
    let c = (e1.0 as i128) * (e2.1 as i128) - (e1.1 as i128) * (e2.0 as i128);
    if c > 0 {
        Ordering::Less
    } else if c < 0 {
        Ordering::Greater
    } else {
        Ordering::Equal
    }
}

pub(crate) fn clean_ccw_ring(raw: &[IPoint]) -> Vec<IPoint> {
    let mut pts: Vec<IPoint> = Vec::with_capacity(raw.len());
    for &p in raw {
        if pts.last() != Some(&p) {
            pts.push(p);
        }
    }
    if pts.len() > 1 && pts.first() == pts.last() {
        pts.pop();
    }
    let mut changed = true;
    while changed && pts.len() >= 3 {
        changed = false;
        let mut next_pts = Vec::with_capacity(pts.len());
        let n = pts.len();
        for i in 0..n {
            let a = pts[(i + n - 1) % n];
            let b = pts[i];
            let c = pts[(i + 1) % n];
            let dot = (b.x as i128 - a.x as i128) * (c.x as i128 - b.x as i128)
                + (b.y as i128 - a.y as i128) * (c.y as i128 - b.y as i128);
            if cross_i128(a, b, c) == 0 && dot >= 0 {
                changed = true;
            } else {
                next_pts.push(b);
            }
        }
        pts = next_pts;
    }
    if pts.len() < 3 {
        return Vec::new();
    }
    let area2: i128 = (0..pts.len())
        .map(|i| {
            let a = pts[i];
            let b = pts[(i + 1) % pts.len()];
            (a.x as i128) * (b.y as i128) - (b.x as i128) * (a.y as i128)
        })
        .sum();
    if area2 == 0 {
        return Vec::new();
    }
    if area2 < 0 {
        pts.reverse();
    }
    pts
}

fn is_simple_convex_ccw(pts: &[IPoint]) -> bool {
    let n = pts.len();
    if n < 3 {
        return false;
    }
    let uniq: BTreeSet<IPoint> = pts.iter().copied().collect();
    if uniq.len() != n {
        return false;
    }
    let mut wraps = 0usize;
    for i in 0..n {
        let a = pts[i];
        let b = pts[(i + 1) % n];
        let c = pts[(i + 2) % n];
        if cross_i128(a, b, c) < 0 {
            return false;
        }
        let e1 = (b.x - a.x, b.y - a.y);
        let e2 = (c.x - b.x, c.y - b.y);
        if e1 == (0, 0) || e2 == (0, 0) {
            return false;
        }
        if cmp_edge_angle(e1, e2) == Ordering::Greater {
            wraps += 1;
        }
    }
    wraps == 1
}

/// Decompose a `Region` into non-overlapping CCW convex polygons on the micron grid.
pub(crate) fn convex_decompose(region: &Region) -> Result<Vec<Vec<IPoint>>, NestError> {
    if region.is_empty() {
        return Ok(Vec::new());
    }
    if region.loops().len() == 1 {
        let raw: Vec<IPoint> = region.loops()[0]
            .points()
            .iter()
            .map(|&p| IPoint::from_point2(p))
            .collect::<Result<_, _>>()?;
        let ring = clean_ccw_ring(&raw);
        if is_simple_convex_ccw(&ring) {
            return Ok(vec![ring]);
        }
    }

    let tris = match region.triangles() {
        Ok(tris) => {
            let mut out = Vec::with_capacity(tris.len());
            for tri in tris {
                let raw = [
                    IPoint::from_point2(tri[0])?,
                    IPoint::from_point2(tri[1])?,
                    IPoint::from_point2(tri[2])?,
                ];
                let cleaned = clean_ccw_ring(&raw);
                if is_simple_convex_ccw(&cleaned) {
                    out.push(Some(cleaned));
                }
            }
            out
        }
        Err(_) => slab_decompose(region)?,
    };
    let mut polys: Vec<Option<Vec<IPoint>>> = tris;

    let mut edge_owner: BTreeMap<(IPoint, IPoint), usize> = BTreeMap::new();
    for (idx, poly) in polys.iter().enumerate() {
        if let Some(pts) = poly {
            for k in 0..pts.len() {
                edge_owner.insert((pts[k], pts[(k + 1) % pts.len()]), idx);
            }
        }
    }

    let mut i = 0usize;
    while i < polys.len() {
        let Some(pi) = polys[i].clone() else {
            i += 1;
            continue;
        };
        let mut merged_any = false;
        for k in 0..pi.len() {
            let u = pi[k];
            let v = pi[(k + 1) % pi.len()];
            let Some(&j) = edge_owner.get(&(v, u)) else {
                continue;
            };
            if j == i {
                continue;
            }
            let Some(pj) = polys[j].as_ref() else {
                continue;
            };
            let Some(pos_j) = pj
                .windows(2)
                .position(|w| w[0] == v && w[1] == u)
                .or_else(|| {
                    if pj.last() == Some(&v) && pj.first() == Some(&u) {
                        Some(pj.len() - 1)
                    } else {
                        None
                    }
                })
            else {
                continue;
            };
            let mut combined = Vec::with_capacity(pi.len() + pj.len());
            for step in 1..pi.len() {
                combined.push(pi[(k + 1 + step) % pi.len()]);
            }
            for step in 1..pj.len() {
                combined.push(pj[(pos_j + 1 + step) % pj.len()]);
            }
            let cleaned = clean_ccw_ring(&combined);
            if !is_simple_convex_ccw(&cleaned) {
                continue;
            }
            for e in 0..pi.len() {
                edge_owner.remove(&(pi[e], pi[(e + 1) % pi.len()]));
            }
            for e in 0..pj.len() {
                edge_owner.remove(&(pj[e], pj[(e + 1) % pj.len()]));
            }
            for e in 0..cleaned.len() {
                edge_owner.insert((cleaned[e], cleaned[(e + 1) % cleaned.len()]), i);
            }
            polys[i] = Some(cleaned);
            polys[j] = None;
            merged_any = true;
            break;
        }
        if !merged_any {
            i += 1;
        }
    }

    Ok(polys.into_iter().flatten().collect())
}

/// Robust horizontal-slab trapezoidation fallback for multi-hole or pinched regions
/// where bridge ear-clipping cannot pick a bridge chord.
fn slab_decompose(region: &Region) -> Result<Vec<Option<Vec<IPoint>>>, NestError> {
    struct Edge {
        lo: IPoint,
        hi: IPoint,
    }
    let mut edges = Vec::new();
    let mut ys = Vec::new();
    for l in region.loops() {
        let pts = l.points();
        let n = pts.len();
        for i in 0..n {
            let a = IPoint::from_point2(pts[i])?;
            let b = IPoint::from_point2(pts[(i + 1) % n])?;
            ys.push(a.y);
            if a.y < b.y {
                edges.push(Edge { lo: a, hi: b });
            } else if b.y < a.y {
                edges.push(Edge { lo: b, hi: a });
            }
        }
    }
    ys.sort_unstable();
    ys.dedup();
    let x_at = |e: &Edge, y: f64| -> f64 {
        let dy = (e.hi.y - e.lo.y) as f64;
        (e.lo.x as f64) + ((y - (e.lo.y as f64)) / dy) * ((e.hi.x - e.lo.x) as f64)
    };
    let x_int = |e: &Edge, y: i64| -> i64 {
        if y == e.lo.y {
            e.lo.x
        } else if y == e.hi.y {
            e.hi.x
        } else {
            x_at(e, y as f64).round() as i64
        }
    };

    let mut out = Vec::new();
    for w in ys.windows(2) {
        let (y0, y1) = (w[0], w[1]);
        if y0 >= y1 {
            continue;
        }
        let y_mid = (y0 as f64 + y1 as f64) * 0.5;
        let mut active: Vec<&Edge> = edges
            .iter()
            .filter(|e| e.lo.y <= y0 && e.hi.y >= y1)
            .collect();
        active.sort_by(|a, b| x_at(a, y_mid).total_cmp(&x_at(b, y_mid)));
        for &[left, right] in active.as_chunks::<2>().0 {
            let mut bl_x = x_int(left, y0);
            let mut br_x = x_int(right, y0);
            let mut tl_x = x_int(left, y1);
            let mut tr_x = x_int(right, y1);
            if bl_x > br_x {
                let mid = bl_x + (br_x - bl_x) / 2;
                bl_x = mid;
                br_x = mid;
            }
            if tl_x > tr_x {
                let mid = tl_x + (tr_x - tl_x) / 2;
                tl_x = mid;
                tr_x = mid;
            }
            let quad = [
                IPoint { x: bl_x, y: y0 },
                IPoint { x: br_x, y: y0 },
                IPoint { x: tr_x, y: y1 },
                IPoint { x: tl_x, y: y1 },
            ];
            let cleaned = clean_ccw_ring(&quad);
            if is_simple_convex_ccw(&cleaned) {
                out.push(Some(cleaned));
            }
        }
    }
    Ok(out)
}
