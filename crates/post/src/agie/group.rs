//! Native job export of two to nine captured punches or two die openings on one plate, from the
//! work format of our own implementation and CamCraft's offline control checks of #2961, #2977 and #2980.

use std::path::Path;

use super::{
    AgieJob, AgieSide, ExportedJob, contour, export_with_review, iso, publish, valid_name,
};
use crate::agie_setup::{GROUP_HEIGHT_UM, GroupPunch, MAX_PUNCH_WORKS, PunchGroup20, Quality};

/// The 3 mm lead from the threading hole to the contour (`ID_COMMPOINTENTRY`).
const LEAD_MM: f64 = 3.0;
/// The radius of the 0.25 mm Berco Cut wire.
const WIRE_RADIUS_MM: f64 = 0.125;

/// The nominal ISO of one work and the XY box that its complete cutting area
/// cannot leave.
struct Work {
    geometry: String,
    points: Vec<[f64; 2]>,
    start_um: [i32; 2],
    min: [f64; 2],
    max: [f64; 2],
}

/// Publish one work as [`export_with_review`] does, or a group of two to
/// [`MAX_PUNCH_WORKS`] independently captured punches (#2980) or two
/// through-openings of one retained die plate (#2977), on one 20 mm plate.
///
/// A group writes one ISO for each work (`<program>1.ISO`, `<program>2.ISO`
/// and so on), one SBR with a resource line for each ISO, and one SBL with
/// the works `CUT1`, `CUT2` and so on in list order. A single work keeps its
/// file layout.
///
/// # Errors
/// Refuses unsupported groups, and any pair of works whose cutting areas are
/// not separated by a clear band along X or Y, before it publishes the
/// directory.
pub fn export_group_with_review(
    jobs: &[AgieJob],
    destination: &Path,
    program: &str,
    separation_length_mm: f64,
) -> Result<ExportedJob, String> {
    let first = match jobs {
        [] => return Err("The job requires an active Wire Contour".into()),
        [job] => return export_with_review(job, destination, program, separation_length_mm),
        [first, ..] => first,
    };
    if separation_length_mm != 1.0 {
        return Err("Set the separation cut length (separationLengthMm) to 1.0 mm for this closed contour. A zero or unspecified separation cannot pass the control formal check".into());
    }
    if !valid_name(program) {
        return Err("The job program name must be a non-reserved 1–8 character DOS name starting with a letter and using uppercase letters or digits".into());
    }
    let height = f64::from(GROUP_HEIGHT_UM) / 1000.0;
    // All works cut the same side. A mixed group would need checks of each
    // punch against the whole retained die plate, which no control run covers.
    // The control checked up to nine punches (#2980) but only two openings.
    let most = if first.side == AgieSide::Die {
        2
    } else {
        MAX_PUNCH_WORKS
    };
    if jobs.len() > most
        || !matches!(first.side, AgieSide::Punch | AgieSide::Die)
        || jobs.iter().any(|job| {
            job.side != first.side
                || job.stock_min != first.stock_min
                || job.stock_max != first.stock_max
                || job.stock_max[2] != 0.0
                || job.stock_min[2] != -height
        })
    {
        return Err(format!(
            "The work group requires 2 to {MAX_PUNCH_WORKS} independent closed punches, or two die openings of one plate, on the same 20 mm stock"
        ));
    }
    if program.len() > 7 {
        return Err("Choose a work group name of up to seven characters, so that each work has a distinct geometry file name".into());
    }
    let names: Vec<String> = (1..=jobs.len()).map(|n| format!("{program}{n}")).collect();
    if !names.iter().all(|name| valid_name(name)) {
        return Err("Choose a work group name that gives non-reserved geometry file names".into());
    }
    let mut works = Vec::with_capacity(jobs.len());
    let mut stock = None;
    for (job, geometry) in jobs.iter().zip(&names) {
        let (mut points, setup, reach) =
            contour::prepare_with_reach(job, geometry, Quality::Ccb030)?;
        let seam = *points.first().ok_or("The job requires a closed contour")?;
        points.push(seam);
        let entry = job
            .start
            .ok_or("The closed contour requires a predrilled 2 mm start hole")?;
        // The box holds the retained punch or the die opening, its hole and
        // lead, the longest rough miter (at least the rough offset of this
        // thickness) and the wire radius. Every pass of the work stays inside
        // it. A die offsets inward, so the box of its opening is conservative.
        let margin = reach + LEAD_MM + WIRE_RADIUS_MM;
        let min = std::array::from_fn(|axis| {
            points.iter().fold(entry[axis], |v, p| v.min(p[axis])) - margin
        });
        let max = std::array::from_fn(|axis| {
            points.iter().fold(entry[axis], |v, p| v.max(p[axis])) + margin
        });
        stock = Some((setup.stock_min_um, setup.stock_max_um, setup.height_um));
        works.push(Work {
            geometry: geometry.clone(),
            points,
            start_um: setup.start_hole_um,
            min,
            max,
        });
    }
    // Every pair, not only neighbours in program order: the control can cut
    // all profiles before any separation, so any two works can meet (#2980).
    for (i, a) in works.iter().enumerate() {
        for b in &works[i + 1..] {
            if !(0..2).any(|axis| a.max[axis] < b.min[axis] || b.max[axis] < a.min[axis]) {
                return Err(format!(
                    "The works {} and {} share a cutting area. The work group requires a clear band along X or Y between the complete cutting areas of every pair of works: each punch or opening, threading hole, lead, rough corner and wire",
                    a.geometry, b.geometry
                ));
            }
        }
    }
    let (stock_min_um, stock_max_um, height_um) =
        stock.ok_or("The job requires an active Wire Contour")?;
    let punches: Vec<GroupPunch<'_>> = works
        .iter()
        .map(|work| GroupPunch {
            iso_stem: &work.geometry,
            start_hole_um: work.start_um,
        })
        .collect();
    let group = PunchGroup20 {
        stem: program,
        stock_min_um,
        stock_max_um,
        height_um,
        punches: &punches,
    };
    let setup = match first.side {
        AgieSide::Die => group.encode_die(),
        _ => group.encode(),
    }
    .map_err(|error| error.to_string())?;
    let mut files: Vec<(String, String)> = works
        .iter()
        .map(|work| {
            let name = format!("{}.ISO", work.geometry);
            let text = iso(&name, &work.points);
            (name, text)
        })
        .collect();
    files.push((format!("{program}.SBR"), setup.sbr));
    files.push((format!("{program}.SBL"), setup.sbl));
    Ok(ExportedJob {
        files: publish(destination, &files)?,
        controller_review: setup.controller_review,
    })
}
