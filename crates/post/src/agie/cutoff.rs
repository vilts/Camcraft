//! Straight open QN2 cut from our own implementation of the open-work format and CamCraft's controller trials.
use super::{AgieJob, AgieSide, contour::micrometres};
use crate::agie_setup::{CUTOFF_HEIGHT_UM, CutoffSetup20, CutoffSide, qn2_offsets_um};

/// Air tails and perpendicular approach are the geometry measured on the qualified 20 mm cycle.
const TAIL: f64 = 4.0;
const APPROACH: f64 = 3.0;
const STROKES: [f64; 2] = [350.0, 250.0];

/// The nominal ISO line, extended into air at both ends.
pub(super) struct Cutoff {
    pub line: [[f64; 2]; 2],
}

pub(super) fn prepare<'a>(
    job: &AgieJob,
    program: &'a str,
) -> Result<(Cutoff, CutoffSetup20<'a>), String> {
    if job.stock_max[2] != 0.0 {
        return Err("The Berco Fine Ra0.30 profile requires stock top Z0".into());
    }
    if job.stock_min[2] != -20.0 || job.start.is_some() {
        return Err("Open cutoffs require 20 mm stock and two outside air endpoints; clear the drilled start hole".into());
    }
    let [rough_um, ..] = qn2_offsets_um(CUTOFF_HEIGHT_UM)
        .ok_or("The QN2 recipe has no measured offset for this stock thickness")?;
    let margin = f64::from(rough_um) / 1000.0 + 0.125; // furthest wire centre plus wire radius
    let &[a, b] = job.contour.as_slice() else {
        return Err("Select one straight axis-aligned edge across the stock".into());
    };
    // SBL start points have three decimal places. Require the ISO and SBL
    // geometries to share that grid so rounding cannot shift the nominal cut.
    if [a, b]
        .into_iter()
        .flatten()
        .any(|v| !v.is_finite() || (v - (v * 1000.).round() / 1000.).abs() > 1e-9)
    {
        return Err("Open cutoff coordinates must be representable to 0.001 mm".into());
    }
    let along = if a[0] != b[0] && a[1] == b[1] {
        0
    } else if a[0] == b[0] && a[1] != b[1] {
        1
    } else {
        return Err("Open cutoffs require a straight edge parallel to X or Y".into());
    };
    let across = 1 - along;
    if a[along].min(b[along]) != job.stock_min[along]
        || a[along].max(b[along]) != job.stock_max[along]
    {
        return Err("Both ends of the selected edge must meet opposite stock faces".into());
    }
    let sign = (b[along] - a[along]).signum();
    let left = if along == 0 { sign } else { -sign };
    let waste = match job.side {
        AgieSide::Left => left,
        AgieSide::Right => -left,
        _ => return Err("An open cut needs an explicit left or right kerf side".into()),
    };
    let mut line = [a, b];
    line[0][along] -= sign * TAIL;
    line[1][along] += sign * TAIL;
    let [mut entry, mut exit] = line;
    entry[across] += waste * APPROACH;
    exit[across] += waste * APPROACH;
    // Keep the cut away from either stock corner. Include the approach and
    // all smaller trim offsets. Each change of side happens at an air tail.
    for point in [a, entry] {
        if point[across] - margin < job.stock_min[across]
            || point[across] + margin > job.stock_max[across]
        {
            return Err(
                "The open cut and its approach need clearance from the stock corners".into(),
            );
        }
    }
    // The operation explicitly confirms that stock centre is the installed
    // XY travel midpoint. Include the work origin used for non-cutting moves.
    for (i, span) in STROKES.iter().enumerate() {
        let mid = (job.stock_min[i] + job.stock_max[i]) / 2.;
        for p in [line[0], line[1], entry, exit, [0., 0.]] {
            if (p[i] - mid).abs() + margin > span / 2. {
                return Err(
                    "The air tails, approach or work origin exceed the installed XY travel".into(),
                );
            }
        }
    }
    let side = match job.side {
        AgieSide::Left => CutoffSide::Left,
        AgieSide::Right => CutoffSide::Right,
        AgieSide::Punch | AgieSide::Die => {
            return Err("An open cut needs an explicit left or right kerf side".into());
        }
    };
    let um =
        |p: [f64; 2]| -> Result<[i32; 2], String> { Ok([micrometres(p[0])?, micrometres(p[1])?]) };
    let setup = CutoffSetup20 {
        stem: program,
        stock_min_um: um([job.stock_min[0], job.stock_min[1]])?,
        stock_max_um: um([job.stock_max[0], job.stock_max[1]])?,
        side,
        entry_um: um(entry)?,
        exit_um: um(exit)?,
        height_um: CUTOFF_HEIGHT_UM,
    };
    Ok((Cutoff { line }, setup))
}
