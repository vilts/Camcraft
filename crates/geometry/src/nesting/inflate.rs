//! Orientation transforms and kerf/gap inflation for 2D nesting parts.
//! Provenance: kerf/gap inflation rules from CamCraft plasma/router geometry specs (#2156, #2158).

use super::decompose::{IPoint, clean_ccw_ring};
use super::nfp::region_ibbox;
use super::types::{ClearanceMode, CutSide, NestError, PartInput, SheetInput};
use super::validate::{
    MIN_AREA, candidate_angles, part_err, validate_closed_loops, validate_open_chains,
};
use crate::geo2d::{Loop, Point2, Region};
use std::collections::BTreeSet;
use std::f64::consts::PI;

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct ShapeKey(Vec<Vec<IPoint>>);

#[derive(Clone, Debug)]
pub(crate) struct PreparedOrientation {
    pub ori_index: usize,
    pub angle_deg: f64,
    pub mirrored: bool,
    pub outer: Option<Loop>,
    pub holes: Vec<Loop>,
    pub open_chains: Vec<Vec<Point2>>,
    pub net_area: f64,
    pub cut_footprint: Region,
    pub sheet_fit_bbox: (IPoint, IPoint),
    pub inflated_footprint: Region,
    pub obstacle_footprint: Region,
    pub hole_usables: Vec<Region>,
    pub shape_key: ShapeKey,
}

#[derive(Clone, Debug)]
pub(crate) struct PreparedPart {
    pub id: String,
    pub quantity: usize,
    pub sort_area: f64,
    pub orientations: Vec<PreparedOrientation>,
}

fn cos_sin_deg(angle_deg: f64) -> (f64, f64) {
    let rem = angle_deg.rem_euclid(360.0);
    if (rem - 0.0).abs() < 1e-12 || (rem - 360.0).abs() < 1e-12 {
        (1.0, 0.0)
    } else if (rem - 90.0).abs() < 1e-12 {
        (0.0, 1.0)
    } else if (rem - 180.0).abs() < 1e-12 {
        (-1.0, 0.0)
    } else if (rem - 270.0).abs() < 1e-12 {
        (0.0, -1.0)
    } else {
        let rad = rem * PI / 180.0;
        (rad.cos(), rad.sin())
    }
}

pub(crate) fn transform_point(p: Point2, mirrored: bool, angle_deg: f64) -> Point2 {
    let mx = if mirrored { -p.x } else { p.x };
    let (c, s) = cos_sin_deg(angle_deg);
    Point2::new(mx * c - p.y * s, mx * s + p.y * c)
}

fn transform_loop(l: &Loop, mirrored: bool, angle_deg: f64) -> Result<Loop, NestError> {
    let mut pts: Vec<Point2> = l
        .points()
        .iter()
        .map(|&p| transform_point(p, mirrored, angle_deg))
        .collect();
    if mirrored {
        pts.reverse();
    }
    Ok(Loop::new(pts)?)
}

fn distances_for_mode(sheet: &SheetInput, cut_side: CutSide) -> (f64, f64, f64, f64, f64) {
    let kw = sheet.cut_width;
    let gap = sheet.part_gap;
    let outer_cut = match cut_side {
        CutSide::Outside => kw,
        CutSide::OnLine => kw * 0.5,
        CutSide::Inside => 0.0,
    };
    let open_cut = match cut_side {
        CutSide::OnLine => kw * 0.5,
        CutSide::Outside | CutSide::Inside => kw,
    };
    match sheet.clearance_mode {
        ClearanceMode::SeparateCuts => {
            let outer_inf = outer_cut + gap * 0.5;
            let hole_inf = match cut_side {
                CutSide::OnLine => kw * 0.5 + gap * 0.5,
                CutSide::Outside | CutSide::Inside => kw + gap * 0.5,
            };
            let open_inf = open_cut + gap * 0.5;
            (outer_cut, outer_inf, hole_inf, open_cut, open_inf)
        }
        ClearanceMode::RouterSingleSlot => {
            let half_gap = gap * 0.5;
            let open_inf = match cut_side {
                CutSide::OnLine => half_gap,
                CutSide::Outside | CutSide::Inside => kw.max(half_gap),
            };
            (outer_cut, half_gap, half_gap, open_cut, open_inf)
        }
    }
}

fn canonical_shape_key(region: &Region) -> Result<ShapeKey, NestError> {
    let Some((lo, _)) = region_ibbox(region)? else {
        return Ok(ShapeKey(Vec::new()));
    };
    let mut loops = Vec::with_capacity(region.loops().len());
    for l in region.loops() {
        let mut pts = Vec::with_capacity(l.points().len());
        for &p in l.points() {
            let ip = IPoint::from_point2(p)?;
            pts.push(IPoint {
                x: ip.x - lo.x,
                y: ip.y - lo.y,
            });
        }
        let ring = clean_ccw_ring(&pts);
        if !ring.is_empty() {
            let min_pos = ring
                .iter()
                .enumerate()
                .min_by_key(|&(_, q)| *q)
                .map_or(0, |(i, _)| i);
            let mut rot = Vec::with_capacity(ring.len());
            rot.extend_from_slice(&ring[min_pos..]);
            rot.extend_from_slice(&ring[..min_pos]);
            loops.push(rot);
        }
    }
    loops.sort();
    Ok(ShapeKey(loops))
}

fn open_chains_ibbox(chains: &[Vec<Point2>]) -> Result<Option<(IPoint, IPoint)>, NestError> {
    let mut it = chains.iter().flat_map(|c| c.iter());
    let Some(&first) = it.next() else {
        return Ok(None);
    };
    let mut lo = IPoint::from_point2(first)?;
    let mut hi = lo;
    for &p in it {
        let ip = IPoint::from_point2(p)?;
        lo.x = lo.x.min(ip.x);
        lo.y = lo.y.min(ip.y);
        hi.x = hi.x.max(ip.x);
        hi.y = hi.y.max(ip.y);
    }
    Ok(Some((lo, hi)))
}

pub(crate) fn prepare_part(
    part: &PartInput,
    sheet: &SheetInput,
) -> Result<PreparedPart, NestError> {
    if part.id.trim().is_empty() {
        return Err(NestError::InvalidInput("part id cannot be empty".into()));
    }
    if part.outer.is_none() && part.open_chains.is_empty() {
        return Err(part_err(
            &part.id,
            "part has neither an outer loop nor open chains",
        ));
    }
    if part.outer.is_none() && !part.holes.is_empty() {
        return Err(part_err(&part.id, "part has holes without an outer loop"));
    }
    let clean_chains = validate_open_chains(&part.id, &part.open_chains)?;
    let (outer_cut, outer_inf, hole_inf, open_cut, open_inf) =
        distances_for_mode(sheet, part.cut_side);
    if !clean_chains.is_empty() && open_inf <= 0.0 {
        return Err(part_err(
            &part.id,
            "open chains require positive cut_width or part_gap",
        ));
    }

    let (base_outer_area, has_closed) = if part.outer.is_some() {
        let (outer_reg, _) = validate_closed_loops(part)?;
        (outer_reg.area().abs(), true)
    } else {
        (0.0, false)
    };

    let angles = candidate_angles(&part.id, part.rotation_mode)?;
    let mirrors: &[bool] = if part.allow_mirror {
        &[false, true]
    } else {
        &[false]
    };
    let mut seen_keys: BTreeSet<(ShapeKey, ShapeKey)> = BTreeSet::new();
    let mut orientations = Vec::new();
    let mut sort_area = base_outer_area;

    for &mirrored in mirrors {
        for &angle_deg in &angles {
            let t_chains: Vec<Vec<Point2>> = clean_chains
                .iter()
                .map(|c| {
                    c.iter()
                        .map(|&p| transform_point(p, mirrored, angle_deg))
                        .collect()
                })
                .collect();
            let swept_cut = if !t_chains.is_empty() && open_cut > 0.0 {
                Region::sweep(&t_chains, open_cut)?
            } else {
                Region::empty()
            };
            let swept_inf = if !t_chains.is_empty() {
                Region::sweep(&t_chains, open_inf)?
            } else {
                Region::empty()
            };

            let (t_outer, t_holes, net_area, cut_fp, inf_fp, obs_fp, hole_usables) = if has_closed {
                let outer_ref = part
                    .outer
                    .as_ref()
                    .ok_or_else(|| part_err(&part.id, "missing outer"))?;
                let lo = transform_loop(outer_ref, mirrored, angle_deg)?;
                let lh: Vec<Loop> = part
                    .holes
                    .iter()
                    .map(|h| transform_loop(h, mirrored, angle_deg))
                    .collect::<Result<_, _>>()?;
                let mut all_loops = Vec::with_capacity(1 + lh.len());
                all_loops.push(lo.clone());
                all_loops.extend(lh.iter().cloned());
                let nominal = Region::new(all_loops)?;
                let outer_only = Region::new(vec![lo.clone()])?;
                let net_area = nominal.area().abs();

                let closed_cut = if outer_cut > 0.0 {
                    nominal.offset_sharp(outer_cut)?
                } else {
                    nominal.clone()
                };
                let cut_fp = if swept_cut.is_empty() {
                    closed_cut
                } else {
                    closed_cut.union(&swept_cut)?
                };

                let outer_filled_inf = if outer_inf > 0.0 {
                    outer_only.offset_sharp(outer_inf)?
                } else {
                    outer_only
                };
                let obs_fp = if swept_inf.is_empty() {
                    outer_filled_inf.clone()
                } else {
                    outer_filled_inf.union(&swept_inf)?
                };

                let mut usable_holes = Vec::new();
                let mut all_holes_usable = Region::empty();
                for h in &lh {
                    let hr = Region::new(vec![h.clone()])?;
                    let shrunk = if hole_inf > 0.0 {
                        hr.offset_sharp(-hole_inf)?
                    } else {
                        hr
                    };
                    let hu = if swept_inf.is_empty() || shrunk.is_empty() {
                        shrunk
                    } else {
                        shrunk.difference(&swept_inf)?
                    };
                    if !hu.is_empty() && hu.area() > MIN_AREA {
                        all_holes_usable = all_holes_usable.union(&hu)?;
                        if sheet.allow_parts_in_holes {
                            usable_holes.push(hu);
                        }
                    }
                }
                let closed_inf = if all_holes_usable.is_empty() {
                    outer_filled_inf
                } else {
                    outer_filled_inf.difference(&all_holes_usable)?
                };
                let inf_fp = if swept_inf.is_empty() {
                    closed_inf
                } else {
                    closed_inf.union(&swept_inf)?
                };
                (Some(lo), lh, net_area, cut_fp, inf_fp, obs_fp, usable_holes)
            } else {
                sort_area = sort_area.max(swept_inf.area());
                (
                    None,
                    Vec::new(),
                    0.0,
                    swept_cut,
                    swept_inf.clone(),
                    swept_inf,
                    Vec::new(),
                )
            };

            let sheet_fit_bbox = if let Some(b) = region_ibbox(&cut_fp)? {
                b
            } else if let Some(b) = open_chains_ibbox(&t_chains)? {
                b
            } else {
                (IPoint { x: 0, y: 0 }, IPoint { x: 0, y: 0 })
            };
            let key_pair = (canonical_shape_key(&obs_fp)?, canonical_shape_key(&cut_fp)?);
            if !seen_keys.insert(key_pair.clone()) {
                continue;
            }
            orientations.push(PreparedOrientation {
                ori_index: orientations.len(),
                angle_deg,
                mirrored,
                outer: t_outer,
                holes: t_holes,
                open_chains: t_chains,
                net_area,
                cut_footprint: cut_fp,
                sheet_fit_bbox,
                inflated_footprint: inf_fp,
                obstacle_footprint: obs_fp,
                hole_usables,
                shape_key: key_pair.0,
            });
        }
    }

    Ok(PreparedPart {
        id: part.id.clone(),
        quantity: part.quantity,
        sort_area,
        orientations,
    })
}
