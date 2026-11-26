//! Deterministic Bottom-Left-Fill (BLF) placement engine and NFP cache.
//! Provenance: BLF placement heuristic over exact NFPs (Burke et al., Operations Research 2006).

use super::candidates::{
    ContainerKind, Obstacle, collect_candidates, translate_region, verify_with_nudge,
};
use super::decompose::IPoint;
use super::inflate::{PreparedOrientation, ShapeKey};
use super::nfp::{
    IfpResult, NfpWithSeams, compute_ifp, compute_nfp, rect_region_from_ipoints, region_ibbox,
};
use super::types::NestError;
use crate::geo2d::Region;
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct PlacementScore {
    pub max_y: i64,
    pub min_y: i64,
    pub min_x: i64,
    pub max_x: i64,
    pub ori_index: usize,
}

#[derive(Clone, Debug)]
pub(crate) struct PlacementHit {
    pub ori_index: usize,
    pub offset: IPoint,
    pub score: PlacementScore,
}

#[derive(Default)]
pub(crate) struct NfpCache {
    entries: BTreeMap<(ShapeKey, ShapeKey), NfpWithSeams>,
}

impl NfpCache {
    fn get_translated(
        &mut self,
        obs: &Obstacle,
        part_base: &Region,
        part_key: &ShapeKey,
    ) -> Result<NfpWithSeams, NestError> {
        let Some((obs_lo, _)) = region_ibbox(&obs.base_region)? else {
            return Ok(NfpWithSeams {
                region: Region::empty(),
                seam_points: Vec::new(),
                seam_segments: Vec::new(),
            });
        };
        let Some((part_lo, _)) = region_ibbox(part_base)? else {
            return Ok(NfpWithSeams {
                region: Region::empty(),
                seam_points: Vec::new(),
                seam_segments: Vec::new(),
            });
        };
        let key = (obs.shape_key.clone(), part_key.clone());
        let nfp_zero = if let Some(cached) = self.entries.get(&key) {
            cached.clone()
        } else {
            let obs_zero = translate_region(
                &obs.base_region,
                IPoint {
                    x: -obs_lo.x,
                    y: -obs_lo.y,
                },
            )?;
            let part_zero = translate_region(
                part_base,
                IPoint {
                    x: -part_lo.x,
                    y: -part_lo.y,
                },
            )?;
            let computed = compute_nfp(&obs_zero, &part_zero)?;
            self.entries.insert(key, computed.clone());
            computed
        };
        let shift = IPoint {
            x: obs.placed_offset.x + obs_lo.x - part_lo.x,
            y: obs.placed_offset.y + obs_lo.y - part_lo.y,
        };
        let region = translate_region(&nfp_zero.region, shift)?;
        let seam_points = nfp_zero
            .seam_points
            .iter()
            .map(|p| p.add(shift))
            .collect::<Result<Vec<_>, _>>()?;
        let seam_segments = nfp_zero
            .seam_segments
            .iter()
            .map(|&(a, b)| Ok((a.add(shift)?, b.add(shift)?)))
            .collect::<Result<Vec<_>, NestError>>()?;
        Ok(NfpWithSeams {
            region,
            seam_points,
            seam_segments,
        })
    }
}

fn union_regions(regions: &[&Region]) -> Result<Region, NestError> {
    match regions.len() {
        0 => Ok(Region::empty()),
        1 => Ok(regions[0].clone()),
        n => {
            let (left, right) = regions.split_at(n / 2);
            let ul = union_regions(left)?;
            let ur = union_regions(right)?;
            if ul.is_empty() {
                Ok(ur)
            } else if ur.is_empty() {
                Ok(ul)
            } else {
                Ok(ul.union(&ur)?)
            }
        }
    }
}

/// Find the deterministic best BLF placement across all allowed orientations inside a container.
pub(crate) fn find_best_placement(
    orientations: &[PreparedOrientation],
    container: ContainerKind<'_>,
    obstacles: &[Obstacle],
    cache: &mut NfpCache,
) -> Result<Option<PlacementHit>, NestError> {
    let (container_reg, is_rect) = match container {
        ContainerKind::RectSheet { usable_bbox } => {
            let r = rect_region_from_ipoints(usable_bbox.0, usable_bbox.1)?;
            (r, true)
        }
        ContainerKind::Hole { region } => (region.clone(), false),
    };

    let mut best: Option<PlacementHit> = None;
    for ori in orientations {
        let ifp = if is_rect && ori.cut_footprint.is_empty() {
            let (lo, hi) = match container {
                ContainerKind::RectSheet { usable_bbox } => usable_bbox,
                ContainerKind::Hole { .. } => continue,
            };
            let rx0 = lo.x - ori.sheet_fit_bbox.0.x;
            let rx1 = hi.x - ori.sheet_fit_bbox.1.x;
            let ry0 = lo.y - ori.sheet_fit_bbox.0.y;
            let ry1 = hi.y - ori.sheet_fit_bbox.1.y;
            if rx0 > rx1 || ry0 > ry1 {
                continue;
            }
            let bbox = (IPoint { x: rx0, y: ry0 }, IPoint { x: rx1, y: ry1 });
            let region = if rx0 < rx1 && ry0 < ry1 {
                rect_region_from_ipoints(bbox.0, bbox.1)?
            } else {
                Region::empty()
            };
            IfpResult {
                region,
                bbox,
                seam_points: Vec::new(),
                seam_segments: Vec::new(),
            }
        } else {
            let fit_fp = if is_rect {
                &ori.cut_footprint
            } else {
                &ori.inflated_footprint
            };
            let Some(ifp) = compute_ifp(&container_reg, fit_fp, is_rect)? else {
                continue;
            };
            ifp
        };

        let part_fp = &ori.obstacle_footprint;
        let mut nfp_list = Vec::with_capacity(obstacles.len());
        for obs in obstacles {
            let nfp = cache.get_translated(obs, part_fp, &ori.shape_key)?;
            if !nfp.region.is_empty() {
                nfp_list.push(nfp);
            }
        }
        let nfp_refs: Vec<&Region> = nfp_list.iter().map(|n| &n.region).collect();
        let obs_union = union_regions(&nfp_refs)?;
        let cands = collect_candidates(&ifp, &obs_union, &nfp_list, is_rect)?;
        let part_box = ori.sheet_fit_bbox;

        for cand in cands {
            let cand_max_y = part_box.1.y + cand.y;
            if let Some(ref b) = best
                && cand_max_y > b.score.max_y + 1000
            {
                break;
            }
            let Some(offset) = verify_with_nudge(ori, cand, container, obstacles, false)? else {
                continue;
            };
            let score = PlacementScore {
                max_y: part_box.1.y + offset.y,
                min_y: part_box.0.y + offset.y,
                min_x: part_box.0.x + offset.x,
                max_x: part_box.1.x + offset.x,
                ori_index: ori.ori_index,
            };
            if best.as_ref().is_none_or(|b| score < b.score) {
                best = Some(PlacementHit {
                    ori_index: ori.ori_index,
                    offset,
                    score,
                });
            }
            break;
        }
    }
    Ok(best)
}
