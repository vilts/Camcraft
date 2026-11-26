//! Deterministic 2D true-shape nesting kernel (NFP/IFP and bottom-left-fill, #2158).
//! Provenance: Bennell & Oliveira (EJOR 2008); Burke, Hellier, Kendall & Whitwell (OR 2006, EJOR 2007).

pub mod cut_order;
pub mod raster;

mod blf;
mod candidates;
mod decompose;
mod inflate;
mod nfp;
pub mod types;
mod validate;

pub use types::*;

use crate::geo2d::{Point2, Region};
use blf::{NfpCache, PlacementHit, find_best_placement};
use candidates::{ContainerKind, Obstacle, translate_loop, translate_region};
use decompose::IPoint;
use inflate::{PreparedOrientation, PreparedPart, prepare_part};
use nfp::region_ibbox;
use std::collections::{BTreeMap, BTreeSet};
use validate::{SheetSpec, validate_sheet};

struct HoleSlot {
    host_id: String,
    host_instance: usize,
    region: Region,
    obstacles: Vec<Obstacle>,
}

struct ActiveSheet {
    usable_bbox: (IPoint, IPoint),
    outer_obstacles: Vec<Obstacle>,
    holes: Vec<HoleSlot>,
    placements: Vec<PlacedPart>,
    placed_counts: BTreeMap<String, usize>,
    cut_footprints: Vec<Region>,
    net_area_sum: f64,
}

impl ActiveSheet {
    fn new(spec: &SheetSpec) -> Self {
        ActiveSheet {
            usable_bbox: spec.usable_bbox,
            outer_obstacles: Vec::new(),
            holes: Vec::new(),
            placements: Vec::new(),
            placed_counts: BTreeMap::new(),
            cut_footprints: Vec::new(),
            net_area_sum: 0.0,
        }
    }
}

fn record_placed(
    sheet: &mut ActiveSheet,
    part_id: &str,
    instance_index: usize,
    ori: &PreparedOrientation,
    offset: IPoint,
    container_part: Option<(String, usize)>,
) -> Result<(), NestError> {
    let shift = offset.to_point2();
    let outer = ori
        .outer
        .as_ref()
        .map(|l| translate_loop(l, offset))
        .transpose()?;
    let holes = ori
        .holes
        .iter()
        .map(|h| translate_loop(h, offset))
        .collect::<Result<Vec<_>, _>>()?;
    let open_chains: Vec<Vec<Point2>> = ori
        .open_chains
        .iter()
        .map(|c| {
            c.iter()
                .map(|p| Point2::new(p.x + shift.x, p.y + shift.y))
                .collect()
        })
        .collect();
    let cut_footprint = translate_region(&ori.cut_footprint, offset)?;
    let inflated_footprint = translate_region(&ori.inflated_footprint, offset)?;

    for hu in &ori.hole_usables {
        let shifted_hu = translate_region(hu, offset)?;
        if !shifted_hu.is_empty() {
            sheet.holes.push(HoleSlot {
                host_id: part_id.to_string(),
                host_instance: instance_index,
                region: shifted_hu,
                obstacles: Vec::new(),
            });
        }
    }

    if !cut_footprint.is_empty() {
        sheet.cut_footprints.push(cut_footprint.clone());
    }
    sheet.net_area_sum += ori.net_area;
    *sheet.placed_counts.entry(part_id.to_string()).or_insert(0) += 1;
    sheet.placements.push(PlacedPart {
        part_id: part_id.to_string(),
        instance_index,
        tx: shift.x,
        ty: shift.y,
        angle_deg: ori.angle_deg,
        mirrored: ori.mirrored,
        container_part,
        net_area: ori.net_area,
        outer,
        holes,
        open_chains,
        cut_footprint,
        inflated_footprint,
    });
    Ok(())
}

fn make_obstacle(ori: &PreparedOrientation, offset: IPoint) -> Result<Obstacle, NestError> {
    let base = &ori.obstacle_footprint;
    let placed_region = translate_region(base, offset)?;
    let placed_bbox = region_ibbox(&placed_region)?.unwrap_or((offset, offset));
    Ok(Obstacle {
        shape_key: ori.shape_key.clone(),
        base_region: base.clone(),
        placed_offset: offset,
        placed_region,
        placed_bbox,
    })
}

fn fits_empty_sheet(
    part: &PreparedPart,
    spec: &SheetSpec,
    cache: &mut NfpCache,
) -> Result<Option<PlacementHit>, NestError> {
    find_best_placement(
        &part.orientations,
        ContainerKind::RectSheet {
            usable_bbox: spec.usable_bbox,
        },
        &[],
        cache,
    )
}

fn oversized_reason(part: &PreparedPart, spec: &SheetSpec) -> String {
    let uw = (spec.usable_bbox.1.x - spec.usable_bbox.0.x) as f64 / crate::geo2d::SCALE;
    let uh = (spec.usable_bbox.1.y - spec.usable_bbox.0.y) as f64 / crate::geo2d::SCALE;
    if let Some(first) = part.orientations.first() {
        let pw = (first.sheet_fit_bbox.1.x - first.sheet_fit_bbox.0.x) as f64 / crate::geo2d::SCALE;
        let ph = (first.sheet_fit_bbox.1.y - first.sheet_fit_bbox.0.y) as f64 / crate::geo2d::SCALE;
        format!(
            "part '{}' ({pw:.3} x {ph:.3} mm with cut clearance) exceeds usable sheet ({uw:.3} x {uh:.3} mm) in all allowed orientations",
            part.id
        )
    } else {
        format!(
            "part '{}' has no feasible orientation on usable sheet ({uw:.3} x {uh:.3} mm)",
            part.id
        )
    }
}

fn union_all_cuts(cuts: &[Region]) -> Result<Region, NestError> {
    match cuts.len() {
        0 => Ok(Region::empty()),
        1 => Ok(cuts[0].clone()),
        n => {
            let (l, r) = cuts.split_at(n / 2);
            let ul = union_all_cuts(l)?;
            let ur = union_all_cuts(r)?;
            Ok(ul.union(&ur)?)
        }
    }
}

/// Nest 2D parts onto rectangular sheets using exact NFP/IFP and deterministic bottom-left-fill.
/// Refuses requests above 10,000 total instances to bound placement work and report allocation.
pub fn nest(sheet: &SheetInput, parts: &[PartInput]) -> Result<NestReport, NestError> {
    let spec = validate_sheet(sheet)?;
    let mut seen_ids = BTreeSet::new();
    let mut prepared = Vec::with_capacity(parts.len());
    let mut requested_counts = BTreeMap::new();
    let mut total_requested = 0usize;

    for p in parts {
        if !seen_ids.insert(p.id.clone()) {
            return Err(NestError::InvalidInput(format!(
                "duplicate part id '{}'",
                p.id
            )));
        }
        total_requested = total_requested.checked_add(p.quantity).ok_or_else(|| {
            NestError::InvalidInput(format!(
                "part '{}' quantity overflows requested total",
                p.id
            ))
        })?;
        if total_requested > 10_000 {
            return Err(NestError::InvalidInput(format!(
                "part '{}' quantity exceeds the nesting limit of 10000 requested instances",
                p.id
            )));
        }
        let prep = prepare_part(p, sheet)?;
        requested_counts.insert(prep.id.clone(), prep.quantity);
        prepared.push(prep);
    }

    // Sort decreasing by outer footprint area, then by part id for deterministic ordering.
    prepared.sort_by(|a, b| {
        b.sort_area
            .total_cmp(&a.sort_area)
            .then_with(|| a.id.cmp(&b.id))
    });

    let mut cache = NfpCache::default();
    let mut sheets: Vec<ActiveSheet> = Vec::new();
    let mut placed_counts = BTreeMap::new();
    let mut unplaced = Vec::new();
    let mut total_placed = 0usize;

    for part in &prepared {
        if part.quantity == 0 {
            continue;
        }
        let Some(empty_hit) = fits_empty_sheet(part, &spec, &mut cache)? else {
            unplaced.push(UnplacedPart {
                part_id: part.id.clone(),
                quantity: part.quantity,
                reason: oversized_reason(part, &spec),
            });
            continue;
        };

        for inst in 0..part.quantity {
            let mut placed = false;

            if sheet.allow_parts_in_holes {
                'hole_search: for s in &mut sheets {
                    for hole in &mut s.holes {
                        if let Some(hit) = find_best_placement(
                            &part.orientations,
                            ContainerKind::Hole {
                                region: &hole.region,
                            },
                            &hole.obstacles,
                            &mut cache,
                        )? {
                            let ori = &part.orientations[hit.ori_index];
                            hole.obstacles.push(make_obstacle(ori, hit.offset)?);
                            let host = (hole.host_id.clone(), hole.host_instance);
                            record_placed(s, &part.id, inst, ori, hit.offset, Some(host))?;
                            placed = true;
                            break 'hole_search;
                        }
                    }
                }
            }

            if !placed {
                for s in &mut sheets {
                    if let Some(hit) = find_best_placement(
                        &part.orientations,
                        ContainerKind::RectSheet {
                            usable_bbox: s.usable_bbox,
                        },
                        &s.outer_obstacles,
                        &mut cache,
                    )? {
                        let ori = &part.orientations[hit.ori_index];
                        s.outer_obstacles.push(make_obstacle(ori, hit.offset)?);
                        record_placed(s, &part.id, inst, ori, hit.offset, None)?;
                        placed = true;
                        break;
                    }
                }
            }

            if !placed {
                let mut s = ActiveSheet::new(&spec);
                let ori = &part.orientations[empty_hit.ori_index];
                s.outer_obstacles
                    .push(make_obstacle(ori, empty_hit.offset)?);
                record_placed(&mut s, &part.id, inst, ori, empty_hit.offset, None)?;
                sheets.push(s);
            }

            *placed_counts.entry(part.id.clone()).or_insert(0) += 1;
            total_placed += 1;
        }
    }

    let mut out_sheets = Vec::with_capacity(sheets.len());
    let mut total_net_area = 0.0f64;
    for (idx, s) in sheets.into_iter().enumerate() {
        let cut_union = union_all_cuts(&s.cut_footprints)?;
        let used_bounds = cut_union.bounds();
        let waste_region = if cut_union.is_empty() {
            spec.sheet_region.clone()
        } else {
            spec.sheet_region.difference(&cut_union)?
        };
        let utilization_pct = (s.net_area_sum / spec.area) * 100.0;
        total_net_area += s.net_area_sum;
        out_sheets.push(SheetPlacement {
            sheet_index: idx,
            placements: s.placements,
            placed_counts: s.placed_counts,
            utilization_pct,
            waste_region,
            used_bounds,
            usable_sheet: spec.usable_sheet.clone(),
        });
    }

    let sheet_count = out_sheets.len();
    let overall_utilization_pct = if sheet_count > 0 {
        (total_net_area / (sheet_count as f64 * spec.area)) * 100.0
    } else {
        0.0
    };

    Ok(NestReport {
        sheet_count,
        sheets: out_sheets,
        total_requested,
        total_placed,
        requested_counts,
        placed_counts,
        overall_utilization_pct,
        unplaced,
    })
}
