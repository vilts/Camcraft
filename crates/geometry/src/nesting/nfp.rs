//! Exact NFP/IFP computation on the geo2d::SCALE integer grid via convex Minkowski sums.
//! Provenance: Minkowski sum over convex decompositions (Bennell & Oliveira, EJOR 2008; Burke et al., EJOR 2007).

use super::decompose::{IPoint, clean_ccw_ring, cmp_edge_angle, convex_decompose, cross_i128};
use super::types::NestError;
use crate::geo2d::{Loop, Point2, Region};
use std::cmp::Ordering;

#[derive(Clone, Debug)]
pub(crate) struct NfpWithSeams {
    pub region: Region,
    /// Exact zero-slack fit vertices/segments swallowed inside merged Minkowski pieces.
    pub seam_points: Vec<IPoint>,
    pub seam_segments: Vec<(IPoint, IPoint)>,
}

#[derive(Clone, Debug)]
pub(crate) struct IfpResult {
    /// Positive-area feasible interior (empty when exact-fit collapses to 1D/0D).
    pub region: Region,
    /// Bounding-box IFP `[rx0..=rx1] x [ry0..=ry1]` on the micron grid.
    pub bbox: (IPoint, IPoint),
    pub seam_points: Vec<IPoint>,
    pub seam_segments: Vec<(IPoint, IPoint)>,
}

fn rotate_min_yx(pts: &[IPoint]) -> Vec<IPoint> {
    let min_idx = pts
        .iter()
        .enumerate()
        .min_by_key(|&(_, p)| (p.y, p.x))
        .map_or(0, |(idx, _)| idx);
    let mut out = Vec::with_capacity(pts.len());
    out.extend_from_slice(&pts[min_idx..]);
    out.extend_from_slice(&pts[..min_idx]);
    out
}

/// Exact Minkowski sum `P + Q` of two CCW convex integer polygons.
fn minkowski_convex(p_raw: &[IPoint], q_raw: &[IPoint]) -> Result<Vec<IPoint>, NestError> {
    let p = rotate_min_yx(p_raw);
    let q = rotate_min_yx(q_raw);
    let (m, n) = (p.len(), q.len());
    let mut out = Vec::with_capacity(m + n);
    let (mut i, mut j) = (0usize, 0usize);
    while i < m || j < n {
        out.push(p[i % m].add(q[j % n])?);
        if i == m {
            j += 1;
        } else if j == n {
            i += 1;
        } else {
            let ep = (p[(i + 1) % m].x - p[i % m].x, p[(i + 1) % m].y - p[i % m].y);
            let eq = (q[(j + 1) % n].x - q[j % n].x, q[(j + 1) % n].y - q[j % n].y);
            match cmp_edge_angle(ep, eq) {
                Ordering::Less => i += 1,
                Ordering::Greater => j += 1,
                Ordering::Equal => {
                    i += 1;
                    j += 1;
                }
            }
        }
    }
    Ok(clean_ccw_ring(&out))
}

fn ring_to_loop(ring: &[IPoint]) -> Result<Loop, NestError> {
    Ok(Loop::new(ring.iter().map(|p| p.to_point2()).collect())?)
}

fn ring_bbox(ring: &[IPoint]) -> (IPoint, IPoint) {
    let mut lo = ring[0];
    let mut hi = lo;
    for &p in &ring[1..] {
        lo.x = lo.x.min(p.x);
        lo.y = lo.y.min(p.y);
        hi.x = hi.x.max(p.x);
        hi.y = hi.y.max(p.y);
    }
    (lo, hi)
}

fn boxes_touch_or_overlap(a: (IPoint, IPoint), b: (IPoint, IPoint)) -> bool {
    a.1.x >= b.0.x && b.1.x >= a.0.x && a.1.y >= b.0.y && b.1.y >= a.0.y
}

fn strictly_inside_convex_ccw(ring: &[IPoint], p: IPoint) -> bool {
    let n = ring.len();
    for i in 0..n {
        if cross_i128(ring[i], ring[(i + 1) % n], p) <= 0 {
            return false;
        }
    }
    true
}

pub(crate) fn segment_intersections(
    a0: IPoint,
    a1: IPoint,
    b0: IPoint,
    b1: IPoint,
    out: &mut Vec<IPoint>,
) {
    if a0.x.max(a1.x) < b0.x.min(b1.x)
        || b0.x.max(b1.x) < a0.x.min(a1.x)
        || a0.y.max(a1.y) < b0.y.min(b1.y)
        || b0.y.max(b1.y) < a0.y.min(a1.y)
    {
        return;
    }
    let rx = (a1.x as i128) - (a0.x as i128);
    let ry = (a1.y as i128) - (a0.y as i128);
    let sx = (b1.x as i128) - (b0.x as i128);
    let sy = (b1.y as i128) - (b0.y as i128);
    let denom = rx * sy - ry * sx;
    let qpx = (b0.x as i128) - (a0.x as i128);
    let qpy = (b0.y as i128) - (a0.y as i128);
    let t_num = qpx * sy - qpy * sx;
    let u_num = qpx * ry - qpy * rx;

    if denom == 0 {
        if u_num != 0 {
            return;
        }
        let on_box = |p: IPoint, u: IPoint, v: IPoint| {
            p.x >= u.x.min(v.x) && p.x <= u.x.max(v.x) && p.y >= u.y.min(v.y) && p.y <= u.y.max(v.y)
        };
        for p in [a0, a1] {
            if on_box(p, b0, b1) {
                out.push(p);
            }
        }
        for p in [b0, b1] {
            if on_box(p, a0, a1) {
                out.push(p);
            }
        }
        return;
    }

    let in_01 = |num: i128| {
        if denom > 0 {
            num >= 0 && num <= denom
        } else {
            num <= 0 && num >= denom
        }
    };
    if !in_01(t_num) || !in_01(u_num) {
        return;
    }
    if t_num == 0 {
        out.push(a0);
    } else if t_num == denom {
        out.push(a1);
    } else if u_num == 0 {
        out.push(b0);
    } else if u_num == denom {
        out.push(b1);
    } else {
        let t = (t_num as f64) / (denom as f64);
        let x = a0.x + (t * (rx as f64)).round() as i64;
        let y = a0.y + (t * (ry as f64)).round() as i64;
        out.push(IPoint { x, y });
    }
}

fn find_internal_seams(
    rings: &[Vec<IPoint>],
    nfp: &Region,
) -> (Vec<IPoint>, Vec<(IPoint, IPoint)>) {
    if rings.len() <= 1 || nfp.is_empty() {
        return (Vec::new(), Vec::new());
    }
    let boxes: Vec<(IPoint, IPoint)> = rings.iter().map(|r| ring_bbox(r)).collect();
    let mut raw_pts = Vec::new();
    let mut seam_segs = Vec::new();

    for i in 0..rings.len() {
        for &p in &rings[i] {
            raw_pts.push(p);
        }
        for j in (i + 1)..rings.len() {
            if !boxes_touch_or_overlap(boxes[i], boxes[j]) {
                continue;
            }
            let ri = &rings[i];
            let rj = &rings[j];
            for ei in 0..ri.len() {
                let (a0, a1) = (ri[ei], ri[(ei + 1) % ri.len()]);
                for ej in 0..rj.len() {
                    let (b0, b1) = (rj[ej], rj[(ej + 1) % rj.len()]);
                    let before = raw_pts.len();
                    segment_intersections(a0, a1, b0, b1, &mut raw_pts);
                    // Opposite-direction collinear overlap creates a potential zero-width seam.
                    let ax = (a1.x as i128) - (a0.x as i128);
                    let ay = (a1.y as i128) - (a0.y as i128);
                    let bx = (b1.x as i128) - (b0.x as i128);
                    let by = (b1.y as i128) - (b0.y as i128);
                    if ax * by - ay * bx == 0
                        && ax * bx + ay * by < 0
                        && raw_pts.len() >= before + 2
                    {
                        let mut seg_pts = raw_pts[before..].to_vec();
                        seg_pts.sort_unstable();
                        seg_pts.dedup();
                        if seg_pts.len() >= 2 {
                            let p_lo = seg_pts[0];
                            let p_hi = seg_pts[seg_pts.len() - 1];
                            let mid = IPoint {
                                x: p_lo.x + (p_hi.x - p_lo.x) / 2,
                                y: p_lo.y + (p_hi.y - p_lo.y) / 2,
                            };
                            if !rings.iter().enumerate().any(|(k, r)| {
                                boxes[k].0.x <= mid.x
                                    && boxes[k].1.x >= mid.x
                                    && boxes[k].0.y <= mid.y
                                    && boxes[k].1.y >= mid.y
                                    && strictly_inside_convex_ccw(r, mid)
                            }) {
                                seam_segs.push((p_lo, p_hi));
                            }
                        }
                    }
                }
            }
        }
    }

    raw_pts.sort_unstable();
    raw_pts.dedup();
    raw_pts.retain(|&p| {
        if !nfp.contains(p.to_point2()) {
            return false;
        }
        !rings.iter().enumerate().any(|(k, r)| {
            boxes[k].0.x < p.x
                && boxes[k].1.x > p.x
                && boxes[k].0.y < p.y
                && boxes[k].1.y > p.y
                && strictly_inside_convex_ccw(r, p)
        })
    });
    (raw_pts, seam_segs)
}

/// Exact No-Fit Polygon `NFP(A, B) = A + (-B)` on the micron grid, preserving zero-width seams.
pub(crate) fn compute_nfp(a: &Region, b: &Region) -> Result<NfpWithSeams, NestError> {
    if a.is_empty() || b.is_empty() {
        return Ok(NfpWithSeams {
            region: Region::empty(),
            seam_points: Vec::new(),
            seam_segments: Vec::new(),
        });
    }
    let pa = convex_decompose(a)?;
    let pb = convex_decompose(b)?;
    let mut neg_b = Vec::with_capacity(pb.len());
    for piece in &pb {
        let mut neg = Vec::with_capacity(piece.len());
        for &pt in piece {
            neg.push(pt.neg()?);
        }
        neg_b.push(clean_ccw_ring(&neg));
    }
    let mut sum_rings = Vec::with_capacity(pa.len() * neg_b.len());
    let mut loops = Vec::with_capacity(pa.len() * neg_b.len());
    for ca in &pa {
        for cb in &neg_b {
            let sum = minkowski_convex(ca, cb)?;
            if sum.len() >= 3 {
                loops.push(ring_to_loop(&sum)?);
                sum_rings.push(sum);
            }
        }
    }
    if loops.is_empty() {
        return Ok(NfpWithSeams {
            region: Region::empty(),
            seam_points: Vec::new(),
            seam_segments: Vec::new(),
        });
    }
    let region = Region::union_all(loops)?;
    let (seam_points, seam_segments) = find_internal_seams(&sum_rings, &region);
    Ok(NfpWithSeams {
        region,
        seam_points,
        seam_segments,
    })
}

pub(crate) fn region_ibbox(r: &Region) -> Result<Option<(IPoint, IPoint)>, NestError> {
    let Some((lo, hi)) = r.bounds() else {
        return Ok(None);
    };
    Ok(Some((IPoint::from_point2(lo)?, IPoint::from_point2(hi)?)))
}

pub(crate) fn rect_region_from_ipoints(lo: IPoint, hi: IPoint) -> Result<Region, NestError> {
    let (a, b) = (lo.to_point2(), hi.to_point2());
    Ok(Region::new(vec![Loop::new(vec![
        Point2::new(a.x, a.y),
        Point2::new(b.x, a.y),
        Point2::new(b.x, b.y),
        Point2::new(a.x, b.y),
    ])?])?)
}

/// Inner-Fit Polygon `IFP(container, part)` returning feasible reference positions.
pub(crate) fn compute_ifp(
    container: &Region,
    part: &Region,
    is_rect_container: bool,
) -> Result<Option<IfpResult>, NestError> {
    let Some((c0, c1)) = region_ibbox(container)? else {
        return Ok(None);
    };
    let Some((p0, p1)) = region_ibbox(part)? else {
        return Ok(None);
    };
    let rx0 = c0.x - p0.x;
    let rx1 = c1.x - p1.x;
    let ry0 = c0.y - p0.y;
    let ry1 = c1.y - p1.y;
    if rx0 > rx1 || ry0 > ry1 {
        return Ok(None);
    }
    let bbox = (IPoint { x: rx0, y: ry0 }, IPoint { x: rx1, y: ry1 });
    if is_rect_container {
        let region = if rx0 < rx1 && ry0 < ry1 {
            rect_region_from_ipoints(bbox.0, bbox.1)?
        } else {
            Region::empty()
        };
        return Ok(Some(IfpResult {
            region,
            bbox,
            seam_points: Vec::new(),
            seam_segments: Vec::new(),
        }));
    }

    let pw = p1.x - p0.x;
    let ph = p1.y - p0.y;
    let pad = pw.max(ph).saturating_add(10_000_000);
    let frame = rect_region_from_ipoints(
        IPoint {
            x: c0.x.saturating_sub(pad),
            y: c0.y.saturating_sub(pad),
        },
        IPoint {
            x: c1.x.saturating_add(pad),
            y: c1.y.saturating_add(pad),
        },
    )?;
    let comp = frame.difference(container)?;
    let nfp_comp = compute_nfp(&comp, part)?;
    let region = if rx0 < rx1 && ry0 < ry1 {
        rect_region_from_ipoints(bbox.0, bbox.1)?.difference(&nfp_comp.region)?
    } else {
        Region::empty()
    };
    Ok(Some(IfpResult {
        region,
        bbox,
        seam_points: nfp_comp.seam_points,
        seam_segments: nfp_comp.seam_segments,
    }))
}
