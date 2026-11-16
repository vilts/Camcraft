//! Contour along an open run that is not a part edge (#1928): a run
//! picked on the stock (or in a selection that is not all part) is cut
//! with the tool beside it, one tool radius plus the stock to leave off
//! the run, never with the tool centre on the picked line. The Side
//! setting chooses the side: Outside stands the tool on the side without
//! stock, Inside on the stock's side, Auto on the side away from the
//! part. A side that cannot be read, and a tool that would enter the
//! part, are refused by name.
//!
//! Provenance: open-profile contouring of published CAM practice (cutter
//! radius compensation to a chosen side of an open profile, RS-274
//! G41/G42 semantics); the offset and side reads are
//! `contour_run_side`'s. The laws are ours. No vendor material consulted.

use crate::cl::Toolpath;
use crate::geo2d::{Point2, Region};

use super::contour::{ContourParams, Side};
use super::contour_chain::contour_along_chain;
use super::contour_run_side::{
    LIFT, SectionAt, TOL, Words, clear_of_part, material_side, offset_run, simple,
};
use super::face::GenError;

const STOCK_WORDS: Words = Words {
    body: "stock",
    both: "Inside and Outside cannot tell which side is the stock; choose Side Auto to cut on the side away from the part",
};

/// A near tie of the part's area on the two sides of the run (a share).
const TIE: f64 = 0.01;

/// Contour along the open run `pts`, one radius plus the stock to leave
/// to the side `side` names: Outside off the stock, Inside into it, Auto
/// away from the part. `stock` and `part` give the stock's and the
/// part's sections at a height (`Ok(None)`: none there).
pub fn contour_open_run_sided(
    pts: &[Point2],
    p: &ContourParams,
    side: Side,
    stock: &SectionAt,
    part: &SectionAt,
) -> Result<Toolpath, GenError> {
    p.validate()?;
    let run = simple(pts)?;
    let zs = p.levels()?;
    let read = |at: &SectionAt| -> Result<Vec<(f64, Region)>, GenError> {
        let mut v = Vec::with_capacity(zs.len());
        for &z in &zs {
            if let Some(r) = at(z + LIFT)?
                && !r.is_empty()
            {
                v.push((z, r));
            }
        }
        Ok(v)
    };
    let stock_levels = read(stock)?;
    let part_levels = read(part)?;
    let d = p.tool_diameter * 0.5 + p.stock_to_leave;
    if !(d.is_finite() && d > TOL) {
        return Err(GenError::Invalid(format!(
            "the open run's offset (tool radius plus stock to leave) is {d:.3} mm - it must be positive"
        )));
    }
    // Which side of travel the stock lies on, when it lies on one.
    let stock_left = || -> Result<bool, GenError> {
        material_side(&run, &stock_levels, &STOCK_WORDS)?.ok_or_else(|| {
            GenError::Invalid(
                "the open run has no stock beside it at any cut level - Inside and Outside cannot tell which side \
                 is the stock; choose Side Auto to cut on the side away from the part, or lower the Bottom height"
                    .into(),
            )
        })
    };
    let (tool_left, said) = match side {
        Side::Outside => (!stock_left()?, "Outside (off the stock)"),
        Side::Inside => (stock_left()?, "Inside (into the stock)"),
        Side::Auto => (!part_left(&run, &part_levels)?, "Auto (away from the part)"),
    };
    // Climb keeps the material the tool's edge finishes on its RIGHT
    // (#1555). Off the stock that is the run's side; into the stock it
    // is the far side. With no stock beside the run the part's side
    // stands in for it.
    let material_left = match material_side(&run, &stock_levels, &STOCK_WORDS) {
        Ok(Some(l)) => l,
        _ => !tool_left,
    };
    let mut ride = offset_run(&run, d, tool_left)?;
    // `offset_run` comes back with the run on its left.
    if tool_left != material_left {
        ride.reverse();
    }
    clear_of_part(&ride, p.tool_diameter * 0.5, &part_levels)?;
    let mut path = contour_along_chain(&ride, p)?;
    path.notes.push(format!(
        "contour: the open run on the stock is cut to its {said} side - the tool centre {d:.3} mm off it (tool radius plus stock to leave)"
    ));
    Ok(path)
}

/// Whether the part lies left of the run's travel, read at every level
/// where the part has material: the part's section split by the run
/// (run on straight past its ends, `sides_of_chain`). No part at any
/// level, the part on both sides, or sides that change between levels
/// are refused by name - Auto then has no side to take.
fn part_left(run: &[Point2], levels: &[(f64, Region)]) -> Result<bool, GenError> {
    let pick = "choose Side Inside (into the stock) or Outside (off the stock)";
    // `sides_of_chain` reads left walking from the lexicographically lower end.
    let flipped = match (run.first(), run.last()) {
        (Some(a), Some(b)) => (b.x, b.y) < (a.x, a.y),
        _ => false,
    };
    let mut side: Option<(bool, f64)> = None;
    for (z, region) in levels {
        let (l, r, _) = camcraft_geometry::boundary_chains::sides_of_chain(run, region)
            .map_err(|e| GenError::Invalid(format!("the open run cannot be read against the part: {e}")))?;
        let (in_l, in_r) = (l.area().abs(), r.area().abs());
        let most = in_l.max(in_r);
        if most <= 0.0 {
            continue;
        }
        if in_l.min(in_r) > TIE * most {
            return Err(GenError::Invalid(format!(
                "Side Auto cuts an open run on the stock away from the part, and the part lies on both sides of \
                 this run at Z {z:.3} - {pick}"
            )));
        }
        let here = (in_l > in_r) != flipped;
        match side {
            None => side = Some((here, *z)),
            Some((s, z0)) if s != here => {
                return Err(GenError::Invalid(format!(
                    "Side Auto cuts an open run on the stock away from the part, and the part lies on one side of \
                     this run at Z {z0:.3} and on the other at Z {z:.3} - {pick}"
                )));
            }
            Some(_) => {}
        }
    }
    side.map(|(s, _)| s).ok_or_else(|| {
        GenError::Invalid(format!(
            "Side Auto cuts an open run on the stock away from the part, and there is no part at the cut levels - {pick}"
        ))
    })
}

#[cfg(test)]
#[path = "contour_run_stock_tests.rs"]
mod tests;
