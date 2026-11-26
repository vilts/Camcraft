//! Candidate point extraction and exact clearance verification for BLF placement.
//! Provenance: boundary-intersection & exact-fit candidate scanning (Burke et al., OR 2006).

use super::decompose::{IPoint, cross_i128};
use super::inflate::PreparedOrientation;
use super::nfp::{IfpResult, NfpWithSeams, region_ibbox, segment_intersections};
use super::types::NestError;
use crate::geo2d::{Loop, Point2, Region};

const MAX_GRAZE_AREA: f64 = 0.05;

#[derive(Clone, Debug)]
pub(crate) struct Obstacle {
    pub shape_key: super::inflate::ShapeKey,
    pub base_region: Region,
    pub placed_offset: IPoint,
    pub placed_region: Region,
    pub placed_bbox: (IPoint, IPoint),
}

#[derive(Clone, Copy)]
pub(crate) enum ContainerKind<'a> {
    RectSheet { usable_bbox: (IPoint, IPoint) },
    Hole { region: &'a Region },
}

pub(crate) fn translate_loop(l: &Loop, delta: IPoint) -> Result<Loop, NestError> {
    let shift = delta.to_point2();
    let pts = l
        .points()
        .iter()
        .map(|p| Point2::new(p.x + shift.x, p.y + shift.y))
        .collect();
    Ok(Loop::new(pts)?)
}

pub(crate) fn translate_region(region: &Region, delta: IPoint) -> Result<Region, NestError> {
    if region.is_empty() || (delta.x == 0 && delta.y == 0) {
        return Ok(region.clone());
    }
    let loops = region
        .loops()
        .iter()
        .map(|l| translate_loop(l, delta))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(Region::new(loops)?)
}

fn region_edges(region: &Region) -> Result<Vec<(IPoint, IPoint)>, NestError> {
    let mut edges = Vec::new();
    for l in region.loops() {
        let pts = l.points();
        let n = pts.len();
        for i in 0..n {
            let a = IPoint::from_point2(pts[i])?;
            let b = IPoint::from_point2(pts[(i + 1) % n])?;
            if a != b {
                edges.push((a, b));
            }
        }
    }
    Ok(edges)
}

fn bbox_edges(lo: IPoint, hi: IPoint) -> Vec<(IPoint, IPoint)> {
    let p0 = lo;
    let p1 = IPoint { x: hi.x, y: lo.y };
    let p2 = hi;
    let p3 = IPoint { x: lo.x, y: hi.y };
    [(p0, p1), (p1, p2), (p2, p3), (p3, p0)]
        .into_iter()
        .filter(|(a, b)| a != b)
        .collect()
}

fn near_boundary(edges: &[(IPoint, IPoint)], p: IPoint) -> bool {
    for &(a, b) in edges {
        if p.x < a.x.min(b.x) - 1
            || p.x > a.x.max(b.x) + 1
            || p.y < a.y.min(b.y) - 1
            || p.y > a.y.max(b.y) + 1
        {
            continue;
        }
        let cross = cross_i128(a, b, p).abs() as f64;
        let len = ((b.x - a.x) as f64).hypot((b.y - a.y) as f64);
        if cross <= len * 1.5 {
            return true;
        }
    }
    false
}

fn in_bbox(p: IPoint, lo: IPoint, hi: IPoint) -> bool {
    p.x >= lo.x && p.x <= hi.x && p.y >= lo.y && p.y <= hi.y
}

fn bbox_overlap(a: (IPoint, IPoint), b: (IPoint, IPoint)) -> bool {
    a.1.x > b.0.x && b.1.x > a.0.x && a.1.y > b.0.y && b.1.y > a.0.y
}

pub(crate) fn collect_candidates(
    ifp: &IfpResult,
    obs_union: &Region,
    obs_nfps: &[NfpWithSeams],
    is_rect: bool,
) -> Result<Vec<IPoint>, NestError> {
    let feasible = if !ifp.region.is_empty() {
        if obs_union.is_empty() {
            ifp.region.clone()
        } else {
            ifp.region.difference(obs_union)?
        }
    } else {
        Region::empty()
    };

    let mut cands: Vec<IPoint> = Vec::new();
    for l in feasible.loops() {
        for &p in l.points() {
            cands.push(IPoint::from_point2(p)?);
        }
    }
    if !ifp.region.is_empty() {
        for l in ifp.region.loops() {
            for &p in l.points() {
                cands.push(IPoint::from_point2(p)?);
            }
        }
    } else {
        let (lo, hi) = ifp.bbox;
        cands.extend([
            lo,
            IPoint { x: hi.x, y: lo.y },
            hi,
            IPoint { x: lo.x, y: hi.y },
            IPoint {
                x: lo.x + (hi.x - lo.x) / 2,
                y: lo.y + (hi.y - lo.y) / 2,
            },
        ]);
    }

    let ifp_boundary_edges = if !ifp.region.is_empty() {
        region_edges(&ifp.region)?
    } else {
        bbox_edges(ifp.bbox.0, ifp.bbox.1)
    };

    // Include zero-width pinch-seam points/intersections swallowed inside unioned NFP/IFP.
    let mut seam_pts: Vec<IPoint> = ifp.seam_points.clone();
    for &(s0, s1) in &ifp.seam_segments {
        seam_pts.push(s0);
        seam_pts.push(s1);
    }
    for nfp in obs_nfps {
        seam_pts.extend(nfp.seam_points.iter().copied());
        for &(s0, s1) in &nfp.seam_segments {
            seam_pts.push(s0);
            seam_pts.push(s1);
            for &(a0, a1) in &ifp_boundary_edges {
                segment_intersections(a0, a1, s0, s1, &mut seam_pts);
            }
        }
    }
    cands.extend(seam_pts.iter().copied());

    let obs_edges = region_edges(obs_union)?;
    if !obs_union.is_empty() {
        for l in obs_union.loops() {
            for &p in l.points() {
                let ip = IPoint::from_point2(p)?;
                if in_bbox(ip, ifp.bbox.0, ifp.bbox.1) {
                    cands.push(ip);
                }
            }
        }
        for &(a0, a1) in &ifp_boundary_edges {
            for &(b0, b1) in &obs_edges {
                segment_intersections(a0, a1, b0, b1, &mut cands);
            }
        }
    }

    let ifp_edges = if !is_rect && !ifp.region.is_empty() {
        region_edges(&ifp.region)?
    } else {
        Vec::new()
    };

    cands.sort_unstable();
    cands.dedup();
    cands.retain(|&ip| {
        if !in_bbox(ip, ifp.bbox.0, ifp.bbox.1) {
            return false;
        }
        let pt = ip.to_point2();
        if !is_rect
            && !ifp.region.is_empty()
            && !ifp.region.contains(pt)
            && !near_boundary(&ifp_edges, ip)
        {
            return false;
        }
        // Preserve zero-width seam points (handled by exact verification).
        if seam_pts.contains(&ip) {
            return true;
        }
        if !obs_union.is_empty() && obs_union.contains(pt) && !near_boundary(&obs_edges, ip) {
            return false;
        }
        true
    });
    cands.sort_by_key(|p| (p.y, p.x));
    Ok(cands)
}

fn check_candidate_exact(
    ori: &PreparedOrientation,
    offset: IPoint,
    container: ContainerKind<'_>,
    obstacles: &[Obstacle],
    use_inflated: bool,
) -> Result<bool, NestError> {
    match container {
        ContainerKind::RectSheet { usable_bbox } => {
            let (lo, hi) = ori.sheet_fit_bbox;
            if lo.x + offset.x < usable_bbox.0.x
                || hi.x + offset.x > usable_bbox.1.x
                || lo.y + offset.y < usable_bbox.0.y
                || hi.y + offset.y > usable_bbox.1.y
            {
                return Ok(false);
            }
        }
        ContainerKind::Hole { region } => {
            let shifted_inf = translate_region(&ori.inflated_footprint, offset)?;
            if !shifted_inf.difference(region)?.is_empty() {
                return Ok(false);
            }
        }
    }

    let check_base = if use_inflated {
        &ori.inflated_footprint
    } else {
        &ori.obstacle_footprint
    };
    let Some(base_box) = region_ibbox(check_base)? else {
        return Ok(true);
    };
    let cand_box = (
        IPoint {
            x: base_box.0.x + offset.x,
            y: base_box.0.y + offset.y,
        },
        IPoint {
            x: base_box.1.x + offset.x,
            y: base_box.1.y + offset.y,
        },
    );
    let cand_shifted = translate_region(check_base, offset)?;
    for obs in obstacles {
        if !bbox_overlap(cand_box, obs.placed_bbox) {
            continue;
        }
        if !cand_shifted.intersect(&obs.placed_region)?.is_empty() {
            return Ok(false);
        }
    }
    Ok(true)
}

pub(crate) fn verify_with_nudge(
    ori: &PreparedOrientation,
    cand: IPoint,
    container: ContainerKind<'_>,
    obstacles: &[Obstacle],
    use_inflated: bool,
) -> Result<Option<IPoint>, NestError> {
    if check_candidate_exact(ori, cand, container, obstacles, use_inflated)? {
        return Ok(Some(cand));
    }
    let check_base = if use_inflated {
        &ori.inflated_footprint
    } else {
        &ori.obstacle_footprint
    };
    let cand_shifted = translate_region(check_base, cand)?;
    let mut max_overlap = 0.0f64;
    for obs in obstacles {
        let area = cand_shifted.intersect(&obs.placed_region)?.area().abs();
        max_overlap = max_overlap.max(area);
    }
    if max_overlap > MAX_GRAZE_AREA {
        return Ok(None);
    }
    for step in [1i64, 10, 100, 1000] {
        for (dx, dy) in [
            (0, step),
            (step, 0),
            (step, step),
            (-step, 0),
            (0, -step),
            (step, -step),
            (-step, step),
            (-step, -step),
        ] {
            let nudged = IPoint {
                x: cand.x + dx,
                y: cand.y + dy,
            };
            if check_candidate_exact(ori, nudged, container, obstacles, use_inflated)? {
                return Ok(Some(nudged));
            }
        }
    }
    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nesting::{PartInput, RotationMode, SheetInput};

    fn rectangle(width: f64, height: f64) -> PreparedOrientation {
        let outer = Loop::new(vec![
            Point2::new(0.0, 0.0),
            Point2::new(width, 0.0),
            Point2::new(width, height),
            Point2::new(0.0, height),
        ])
        .unwrap();
        let part = PartInput::closed("tiny", 1, outer, vec![]).with_rotation(RotationMode::Locked);
        super::super::inflate::prepare_part(&part, &SheetInput::default())
            .unwrap()
            .orientations
            .remove(0)
    }

    #[test]
    fn exact_check_refuses_micron_escape_from_hole() {
        let ori = rectangle(1.0, 1.0);
        let container = ContainerKind::Hole {
            region: &ori.inflated_footprint,
        };
        assert!(!check_candidate_exact(&ori, IPoint { x: 1, y: 0 }, container, &[], true).unwrap());
    }

    #[test]
    fn exact_check_refuses_sub_epsilon_overlap() {
        let ori = rectangle(0.01, 0.01);
        let obstacle = Obstacle {
            shape_key: ori.shape_key.clone(),
            base_region: ori.obstacle_footprint.clone(),
            placed_offset: IPoint { x: 0, y: 0 },
            placed_region: ori.obstacle_footprint.clone(),
            placed_bbox: region_ibbox(&ori.obstacle_footprint).unwrap().unwrap(),
        };
        let container = ContainerKind::RectSheet {
            usable_bbox: (
                IPoint { x: 0, y: 0 },
                IPoint {
                    x: 100_000,
                    y: 100_000,
                },
            ),
        };
        assert!(
            !check_candidate_exact(
                &ori,
                IPoint { x: 9_999, y: 0 },
                container,
                &[obstacle],
                true
            )
            .unwrap()
        );
    }
}
