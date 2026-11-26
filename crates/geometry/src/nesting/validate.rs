//! Input validation helpers for sheet dimensions, cut parameters, loops, and open chains.
//! Provenance: Prime Directive validation rules for CamCraft 2D nesting (#2158).

use super::decompose::IPoint;
use super::types::{ClearanceMode, NestError, PartInput, RotationMode, SheetInput};
use crate::geo2d::{COORD_LIMIT, Loop, Point2, Region};

pub(crate) const MIN_AREA: f64 = 1e-6;
const MIN_OPEN_LEN: f64 = 1e-6;
const MAX_ROTATION_STEPS: usize = 720;

#[derive(Clone, Debug)]
pub(crate) struct SheetSpec {
    pub area: f64,
    pub sheet_region: Region,
    pub usable_sheet: Region,
    pub usable_bbox: (IPoint, IPoint),
}

pub(crate) fn part_err(part_id: &str, reason: impl Into<String>) -> NestError {
    NestError::InvalidPart {
        part_id: part_id.to_string(),
        reason: reason.into(),
    }
}

fn check_non_negative(name: &str, val: f64, strictly_positive: bool) -> Result<(), NestError> {
    if !val.is_finite() {
        return Err(NestError::InvalidInput(format!("{name} must be finite")));
    }
    if val >= COORD_LIMIT {
        return Err(NestError::OutOfRange(format!(
            "{name} exceeds machinable range"
        )));
    }
    if strictly_positive && val <= 0.0 {
        return Err(NestError::InvalidInput(format!("{name} must be positive")));
    }
    if !strictly_positive && val < 0.0 {
        return Err(NestError::InvalidInput(format!(
            "{name} must be non-negative"
        )));
    }
    Ok(())
}

pub(crate) fn validate_sheet(sheet: &SheetInput) -> Result<SheetSpec, NestError> {
    check_non_negative("sheet width", sheet.width, true)?;
    check_non_negative("sheet height", sheet.height, true)?;
    check_non_negative("edge_margin", sheet.edge_margin, false)?;
    check_non_negative("cut_width", sheet.cut_width, false)?;
    check_non_negative("part_gap", sheet.part_gap, false)?;

    if sheet.clearance_mode == ClearanceMode::RouterSingleSlot && sheet.part_gap < sheet.cut_width {
        return Err(NestError::InvalidInput(format!(
            "router single-slot clearance requires part_gap ({}) >= cut_width ({})",
            sheet.part_gap, sheet.cut_width
        )));
    }
    if sheet.edge_margin * 2.0 >= sheet.width || sheet.edge_margin * 2.0 >= sheet.height {
        return Err(NestError::InvalidInput(
            "usable sheet is empty after applying edge_margin".into(),
        ));
    }

    let sheet_region = Region::new(vec![Loop::new(vec![
        Point2::new(0.0, 0.0),
        Point2::new(sheet.width, 0.0),
        Point2::new(sheet.width, sheet.height),
        Point2::new(0.0, sheet.height),
    ])?])?;
    let usable_sheet = if sheet.edge_margin > 0.0 {
        sheet_region.offset_sharp(-sheet.edge_margin)?
    } else {
        sheet_region.clone()
    };
    if usable_sheet.is_empty() || usable_sheet.area() <= MIN_AREA {
        return Err(NestError::InvalidInput(
            "usable sheet is empty after applying edge_margin".into(),
        ));
    }
    let Some(usable_bbox) = super::nfp::region_ibbox(&usable_sheet)? else {
        return Err(NestError::InvalidInput("usable sheet has no bounds".into()));
    };
    Ok(SheetSpec {
        area: sheet.width * sheet.height,
        sheet_region,
        usable_sheet,
        usable_bbox,
    })
}

pub(crate) fn candidate_angles(part_id: &str, mode: RotationMode) -> Result<Vec<f64>, NestError> {
    match mode {
        RotationMode::Locked => Ok(vec![0.0]),
        RotationMode::Orthogonal => Ok(vec![0.0, 90.0, 180.0, 270.0]),
        RotationMode::FixedStep(deg) => {
            if !(deg.is_finite() && deg > 0.0 && deg <= 360.0) {
                return Err(part_err(
                    part_id,
                    format!("rotation step {deg} must be in (0, 360]"),
                ));
            }
            let count = (360.0 / deg).ceil() as usize;
            if count > MAX_ROTATION_STEPS {
                return Err(part_err(
                    part_id,
                    format!(
                        "rotation step {deg} exceeds {MAX_ROTATION_STEPS} candidate orientations"
                    ),
                ));
            }
            let mut angles = Vec::new();
            let mut k = 0usize;
            loop {
                let a = (k as f64) * deg;
                if a >= 360.0 - 1e-9 || angles.len() >= MAX_ROTATION_STEPS {
                    break;
                }
                angles.push(a);
                k += 1;
            }
            if angles.is_empty() {
                angles.push(0.0);
            }
            Ok(angles)
        }
    }
}

pub(crate) fn validate_open_chains(
    part_id: &str,
    chains: &[Vec<Point2>],
) -> Result<Vec<Vec<Point2>>, NestError> {
    let mut cleaned = Vec::with_capacity(chains.len());
    for (idx, chain) in chains.iter().enumerate() {
        let mut pts: Vec<Point2> = Vec::with_capacity(chain.len());
        for &p in chain {
            if !(p.x.is_finite() && p.y.is_finite()) {
                return Err(part_err(
                    part_id,
                    format!("open chain {idx} has non-finite point"),
                ));
            }
            if p.x.abs() >= COORD_LIMIT || p.y.abs() >= COORD_LIMIT {
                return Err(NestError::OutOfRange(format!(
                    "part '{part_id}' open chain {idx} exceeds machinable range"
                )));
            }
            if pts
                .last()
                .is_none_or(|q| (q.x - p.x).hypot(q.y - p.y) > 1e-9)
            {
                pts.push(p);
            }
        }
        let len: f64 = pts
            .windows(2)
            .map(|w| (w[1].x - w[0].x).hypot(w[1].y - w[0].y))
            .sum();
        if pts.len() < 2 || len <= MIN_OPEN_LEN {
            return Err(part_err(
                part_id,
                format!("open chain {idx} has no usable length"),
            ));
        }
        cleaned.push(pts);
    }
    Ok(cleaned)
}

pub(crate) fn validate_closed_loops(part: &PartInput) -> Result<(Region, Vec<Region>), NestError> {
    let Some(outer) = &part.outer else {
        return Err(part_err(&part.id, "missing outer loop"));
    };
    let outer_reg = Region::new(vec![outer.clone()])
        .map_err(|e| part_err(&part.id, format!("invalid outer loop: {e}")))?;
    if outer_reg.loops().len() != 1 || outer_reg.area().abs() <= MIN_AREA {
        return Err(part_err(&part.id, "outer loop encloses no area"));
    }
    let mut hole_regs = Vec::with_capacity(part.holes.len());
    for (idx, hole) in part.holes.iter().enumerate() {
        let hr = Region::new(vec![hole.clone()])
            .map_err(|e| part_err(&part.id, format!("invalid hole loop {idx}: {e}")))?;
        if hr.loops().len() != 1 || hr.area().abs() <= MIN_AREA {
            return Err(part_err(
                &part.id,
                format!("hole loop {idx} encloses no area"),
            ));
        }
        let outside = hr.difference(&outer_reg)?;
        if !outside.is_empty() || hr.area().abs() >= outer_reg.area().abs() - MIN_AREA {
            return Err(part_err(
                &part.id,
                format!("hole loop {idx} lies outside or intersects outer loop"),
            ));
        }
        for (prev_idx, prev) in hole_regs.iter().enumerate() {
            if !hr.intersect(prev)?.is_empty() {
                return Err(part_err(
                    &part.id,
                    format!("hole loop {idx} overlaps hole loop {prev_idx}"),
                ));
            }
        }
        hole_regs.push(hr);
    }
    Ok((outer_reg, hole_regs))
}
