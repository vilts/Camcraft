//! Read the selected die rim of a Wire Contour for the native wire EDM job: one opening
//! (#2957), or one of the two openings of a die group (#2977). The plate and opening geometry
//! is in `camcraft_geometry::die_opening`.
//! Provenance: CAMCraft's Wire Contour selection and its closed vertical prism.

use super::job;
use crate::geo2d::{Loop, Region};
use camcraft_geometry::die_opening::{check_die_plate, matching_opening};

/// The through-opening of the retained die plate that the selected rim
/// matches. The plate must have exactly `openings` through-openings, one for
/// each work of the job.
pub(super) fn die_opening<'r>(
    region: &'r Region,
    job: &job::GenJob,
    lo: [f64; 2],
    hi: [f64; 2],
    openings: usize,
) -> Result<&'r Loop, String> {
    check_die_plate(region, lo, hi, openings)?;
    let (picked, open, _) = job
        .contour_sel
        .as_ref()
        .ok_or("Select the complete rim of the die opening")?
        .as_ref()
        .map_err(ToString::to_string)?;
    let [picked] = picked.as_slice() else {
        return Err("Select the edge chains of exactly one closed die opening rim".into());
    };
    matching_opening(region, picked, open.is_empty())
}
