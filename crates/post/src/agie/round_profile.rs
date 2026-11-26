//! Native ISO arcs for one selected DXF circle, racetrack or convex tangent loop punch on a
//! 20 mm plate, from classical circle geometry and the offline control checks of #2022 and #2983.

use std::fmt::Write;
use std::path::Path;

use camcraft_geometry::wire_round_profile::{RoundProfile, Spine, WireProfile};

use super::contour::{self, Point};
use super::{AgieJob, AgieSide, ExportedJob, publish, valid_name};
use crate::agie_setup::{PunchSetup20, qn2_offsets_um};

const UNITS: f64 = 100_000.0; // Five decimal places in the native ISO geometry file.
pub(super) const LEAD: i64 = 300_000; // 3 mm lead and 3 mm of straight on each side of the seam.
/// The plate height of the control-checked arc jobs (#2022).
pub(super) const HEIGHT_UM: i32 = 20_000;

/// One ISO block: its end point, and the centre of the counter-clockwise arc
/// that ends there. `None` is a straight move.
#[derive(Debug, PartialEq, Eq)]
pub(super) struct ArcBlock {
    pub end: Point,
    pub center: Option<Point>,
}

/// A coordinate that the five-decimal grid holds exactly (within 1e-9 mm).
/// No arc endpoint, centre or radius is rounded, so each I/J is exact.
pub(super) fn exact(value: f64) -> Result<i64, String> {
    let units = contour::quantize(value)?;
    if (value - units as f64 / UNITS).abs() > 1e-9 {
        return Err("The DXF punch must fit the five-decimal native grid exactly".into());
    }
    Ok(units)
}

pub(super) fn line(end: Point) -> ArcBlock {
    ArcBlock { end, center: None }
}

pub(super) fn arc(end: Point, center: Point) -> ArcBlock {
    ArcBlock {
        end,
        center: Some(center),
    }
}

/// The ISO blocks of the nominal profile and the zero-step punch setup.
///
/// The control offsets a convex arc to a concentric arc. So the swept rough
/// pass and wire of the whole profile stay inside its bounding box, enlarged
/// by the rough offset and the wire radius (no miter as at a polygon corner).
pub(super) fn prepare<'a>(
    job: &AgieJob,
    shape: &RoundProfile,
    program: &'a str,
) -> Result<(Vec<ArcBlock>, PunchSetup20<'a>), String> {
    if job.side != AgieSide::Punch || !job.contour.is_empty() {
        return Err("The native DXF circle or racetrack is a punch only".into());
    }
    let setup = contour::punch_setup(job, program, crate::agie_setup::Quality::Ccb030)?;
    if setup.height_um != HEIGHT_UM {
        return Err("The native DXF circle or racetrack punch requires the qualified 20 mm stock height".into());
    }
    let [rough_um, ..] = qn2_offsets_um(HEIGHT_UM)
        .ok_or("The QN2 recipe has no measured offset for this stock thickness")?;
    let margin = contour::stock_margin(rough_um);
    let point = |x: f64, y: f64| -> Result<Point, String> { Ok([exact(x)?, exact(y)?]) };
    let [a, b] = shape.centers().map(|c| point(c.x, c.y));
    let (a, b) = (a?, b?);
    let r = exact(shape.radius)?;
    if !(50_000..=2_000_000).contains(&r) {
        return Err("The native DXF radius must be from 0.5 to 20 mm".into());
    }
    let start = job
        .start
        .ok_or("The DXF punch requires a predrilled 2 mm threading hole")?;
    let hole = point(start[0], start[1])?;
    let lo = [contour::quantize(job.stock_min[0])?, contour::quantize(job.stock_min[1])?];
    let hi = [contour::quantize(job.stock_max[0])?, contour::quantize(job.stock_max[1])?];
    // The arc extrema: the box of the two cap circles.
    if (0..2).any(|i| {
        a[i].min(b[i]) - r - margin < lo[i] || a[i].max(b[i]) + r + margin > hi[i]
    }) {
        return Err("The native DXF punch, its rough pass and the wire need at least 0.4 mm of stock beyond the arc extrema".into());
    }
    let blocks = match shape.spine {
        Spine::Circle(_) => circle(a, r, hole)?,
        Spine::Stadium(_) => racetrack(a, b, r, hole)?,
    };
    for (i, block) in blocks.iter().enumerate() {
        let Some(c) = block.center else { continue };
        let from = i.checked_sub(1).map(|j| blocks[j].end).ok_or("The ISO cannot start with an arc")?;
        let r2 = |p: Point| i128::from(p[0] - c[0]).pow(2) + i128::from(p[1] - c[1]).pow(2);
        if r2(from) != r2(block.end) || r2(from) != i128::from(r).pow(2) {
            return Err("The native arc radii must be exact at both ends".into());
        }
    }
    Ok((blocks, setup))
}

/// A cardinal radial lead and the two diameter ends are exact on the grid.
/// The control checked R6, R10 and R15 and the four cardinal leads at R10.
fn circle(c: Point, r: i64, hole: Point) -> Result<Vec<ArcBlock>, String> {
    if !(600_000..=1_500_000).contains(&r) {
        return Err("The native circle punch requires a radius from 6 to 15 mm".into());
    }
    let axis = if hole[1] == c[1] {
        0
    } else if hole[0] == c[0] {
        1
    } else {
        return Err("Place the threading hole on an exact horizontal or vertical radius of the circle".into());
    };
    let radial = hole[axis] - c[axis];
    if radial.abs() < r + LEAD {
        return Err("The threading hole must be at least 3 mm beyond the circle".into());
    }
    let mut seam = c;
    let mut opposite = c;
    seam[axis] += r * radial.signum();
    opposite[axis] -= r * radial.signum();
    Ok(vec![line(seam), arc(opposite, c), arc(seam, c)])
}

/// Enter one straight side perpendicular to it, from the waste side, with
/// 3 mm of straight on both sides of the seam. The straights are parallel to X or Y.
fn racetrack(a: Point, b: Point, r: i64, hole: Point) -> Result<Vec<ArcBlock>, String> {
    let d = [b[0] - a[0], b[1] - a[1]];
    if d == [0, 0] || (d[0] != 0 && d[1] != 0) {
        return Err("The native DXF racetrack needs straight sides parallel to X or Y".into());
    }
    // Waste side of the first straight, as in RoundProfile::tangencies.
    let right = [d[1].signum() * r, -d[0].signum() * r];
    let p = [a[0] + right[0], a[1] + right[1]];
    let q = [b[0] + right[0], b[1] + right[1]];
    let s = [b[0] - right[0], b[1] - right[1]];
    let t = [a[0] - right[0], a[1] - right[1]];
    for (first, last, around, other, far, near) in [(p, q, s, t, b, a), (s, t, p, q, a, b)] {
        let dx = last[0] - first[0];
        let dy = last[1] - first[1];
        let length = dx.abs() + dy.abs();
        let along = ((hole[0] - first[0]) * dx + (hole[1] - first[1]) * dy) / length;
        let outside = ((hole[0] - first[0]) * dy - (hole[1] - first[1]) * dx) / length;
        if along < LEAD || length - along < LEAD || outside < LEAD {
            continue;
        }
        let seam = if dx == 0 { [first[0], hole[1]] } else { [hole[0], first[1]] };
        // The lead is perpendicular to a supporting line of the convex
        // racetrack, so all of the punch is on the inner side of it.
        return Ok(vec![
            line(seam),
            line(last),
            arc(around, far),
            line(other),
            arc(first, near),
            line(seam),
        ]);
    }
    Err("The DXF racetrack needs the threading hole at least 3 mm outside a straight side, with at least 3 mm of that straight on both sides of the perpendicular entry".into())
}

/// The nominal ISO with counter-clockwise arcs (G03) and incremental I/J centres.
pub(super) fn iso(iso_name: &str, blocks: &[ArcBlock]) -> String {
    let mm = |v: i64| v as f64 / UNITS;
    let mut iso = format!("( {iso_name} )\r\n");
    let mut previous: Option<Point> = None;
    for (i, block) in blocks.iter().enumerate() {
        let [x, y] = block.end;
        let n = i + 1;
        let _ = match (previous, block.center) {
            (Some(from), Some(c)) => write!(
                iso,
                "N{n:04} G03 X{:.5} Y{:.5} I{:.5} J{:.5} ;\r\n",
                mm(x),
                mm(y),
                mm(c[0] - from[0]),
                mm(c[1] - from[1])
            ),
            (None, _) => write!(iso, "N{n:04} G00 X{:.5} Y{:.5} ;\r\n", mm(x), mm(y)),
            (Some(_), None) => write!(iso, "N{n:04} G01 X{:.5} Y{:.5} ;\r\n", mm(x), mm(y)),
        };
        previous = Some(block.end);
    }
    let _ = write!(iso, "N{:04} M02 ;\r\n", blocks.len() + 1);
    iso
}

/// Publish one zero-step punch whose nominal ISO has native arcs: a selected
/// DXF circle, racetrack or convex tangent loop on a 20 mm plate. The SBL and SBR are those of
/// the polygon punch ([`super::export_with_review`]).
///
/// # Errors
/// Refuses unsupported shapes, placements and settings before it publishes.
pub fn export_round_with_review(
    job: &AgieJob,
    shape: &WireProfile,
    destination: &Path,
    program: &str,
    separation_length_mm: f64,
) -> Result<ExportedJob, String> {
    if separation_length_mm != 1.0 {
        return Err("Set the separation cut length (separationLengthMm) to 1.0 mm for this closed contour. A zero or unspecified separation cannot pass the control formal check".into());
    }
    if !valid_name(program) {
        return Err("The job program name must be a non-reserved 1–8 character DOS name starting with a letter and using uppercase letters or digits".into());
    }
    let (blocks, setup) = match shape {
        WireProfile::Round(round) => prepare(job, round, program)?,
        WireProfile::Tangent(tangent) => super::tangent_profile::prepare(job, tangent, program)?,
    };
    let setup = setup.encode().map_err(|error| error.to_string())?;
    let iso_name = format!("{program}.ISO");
    let files = [
        (iso_name.clone(), iso(&iso_name, &blocks)),
        (format!("{program}.SBR"), setup.sbr),
        (format!("{program}.SBL"), setup.sbl),
    ];
    Ok(ExportedJob {
        files: publish(destination, &files)?,
        controller_review: setup.controller_review,
    })
}
