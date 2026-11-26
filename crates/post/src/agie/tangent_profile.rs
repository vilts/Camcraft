//! Native ISO lines and arcs for one selected convex tangent DXF loop punch on a 20 mm plate
//! (#2983). Provenance: classical tangent and circle geometry, checked in integer grid units,
//! and the offline control checks of #2022 and #2983.

use camcraft_geometry::wire_round_profile::{TangentProfile, TangentSegment};

use super::contour::{self, Point};
use super::round_profile::{ArcBlock, HEIGHT_UM, LEAD, arc, exact, line};
use super::{AgieJob, AgieSide};
use crate::agie_setup::{PunchSetup20, qn2_offsets_um};

const UNITS: f64 = 100_000.0; // Five decimal places in the native ISO geometry file.
/// The arc radius range of the native jobs, 0.5 to 20 mm, in grid units.
const RADIUS: std::ops::RangeInclusive<i64> = 50_000..=2_000_000;

fn delta(p: Point, q: Point) -> Point {
    [q[0] - p[0], q[1] - p[1]]
}

fn dot(a: Point, b: Point) -> i128 {
    i128::from(a[0]) * i128::from(b[0]) + i128::from(a[1]) * i128::from(b[1])
}

/// The z component of `a` × `b`.
fn cross(a: Point, b: Point) -> i128 {
    i128::from(a[0]) * i128::from(b[1]) - i128::from(a[1]) * i128::from(b[0])
}

/// One segment on the five-decimal grid.
struct Segment {
    from: Point,
    to: Point,
    center: Option<Point>,
}

impl Segment {
    fn new(s: &TangentSegment) -> Result<Self, String> {
        let point = |p: camcraft_geometry::geo2d::Point2| -> Result<Point, String> {
            Ok([exact(p.x)?, exact(p.y)?])
        };
        let from = point(s.start)?;
        let to = point(s.end)?;
        let center = s.center.map(point).transpose()?;
        if from == to {
            return Err("The native grid collapses a segment of the DXF punch".into());
        }
        if let Some(c) = center {
            let (a, b) = (delta(c, from), delta(c, to));
            // The source sweeps counter-clockwise by at most a half turn. The
            // emitted integers keep the radius and the side of the sweep.
            if dot(a, a) != dot(b, b) || cross(a, b) < 0 {
                return Err(
                    "The native arc ends must keep the same radius and sweep on the ISO grid"
                        .into(),
                );
            }
            let r2 = dot(a, a);
            if r2 < i128::from(*RADIUS.start()).pow(2) || r2 > i128::from(*RADIUS.end()).pow(2) {
                return Err("The native DXF radius must be from 0.5 to 20 mm".into());
            }
        }
        Ok(Self { from, to, center })
    }

    /// The direction of travel at `at` (not normalised).
    fn tangent(&self, at: Point) -> Point {
        match self.center {
            Some(c) => {
                let r = delta(c, at);
                [-r[1], r[0]]
            }
            None => delta(self.from, self.to),
        }
    }

    fn block(&self) -> ArcBlock {
        match self.center {
            Some(c) => arc(self.to, c),
            None => line(self.to),
        }
    }
}

/// The ISO blocks of the nominal loop and the zero-step punch setup.
///
/// The control offsets each convex arc to a concentric arc, and each straight
/// to a parallel straight. So the swept rough pass and wire stay inside the
/// exact bounds of the loop, enlarged by the rough offset and the wire radius.
pub(super) fn prepare<'a>(
    job: &AgieJob,
    shape: &TangentProfile,
    program: &'a str,
) -> Result<(Vec<ArcBlock>, PunchSetup20<'a>), String> {
    if job.side != AgieSide::Punch || !job.contour.is_empty() {
        return Err("The native DXF tangent loop is a punch only".into());
    }
    let setup = contour::punch_setup(job, program, crate::agie_setup::Quality::Ccb030)?;
    if setup.height_um != HEIGHT_UM {
        return Err(
            "The native DXF tangent loop punch requires the qualified 20 mm stock height".into(),
        );
    }
    let [rough_um, ..] = qn2_offsets_um(HEIGHT_UM)
        .ok_or("The QN2 recipe has no measured offset for this stock thickness")?;
    let margin = contour::stock_margin(rough_um);
    let segments = shape
        .segments()
        .iter()
        .map(Segment::new)
        .collect::<Result<Vec<_>, _>>()?;
    for (a, b) in segments.iter().zip(segments.iter().cycle().skip(1)) {
        let (u, v) = (a.tangent(a.to), b.tangent(b.from));
        if a.to != b.from || cross(u, v) != 0 || dot(u, v) <= 0 {
            return Err("The native ISO grid must keep each tangent joint of the DXF punch".into());
        }
    }
    // The exact extrema, rounded outward to the grid (within 1e-9 mm, as `exact`).
    let [low, high] = shape.bounds();
    let outward = |v: f64, up: bool| -> Result<i64, String> {
        contour::quantize(v)?;
        let units = v * UNITS;
        Ok(if up {
            (units - 1e-4).ceil()
        } else {
            (units + 1e-4).floor()
        } as i64)
    };
    let lo = [
        contour::quantize(job.stock_min[0])?,
        contour::quantize(job.stock_min[1])?,
    ];
    let hi = [
        contour::quantize(job.stock_max[0])?,
        contour::quantize(job.stock_max[1])?,
    ];
    let (low, high) = (
        [outward(low.x, false)?, outward(low.y, false)?],
        [outward(high.x, true)?, outward(high.y, true)?],
    );
    if (0..2).any(|i| low[i] - margin < lo[i] || high[i] + margin > hi[i]) {
        return Err("The native DXF punch, its rough pass and the wire need at least 0.4 mm of stock beyond the arc extrema".into());
    }
    let start = job
        .start
        .ok_or("The DXF punch requires a predrilled 2 mm threading hole")?;
    let hole = [exact(start[0])?, exact(start[1])?];
    let blocks = entry(shape, &segments, hole)?;
    Ok((blocks, setup))
}

/// Enter one straight side parallel to X or Y perpendicular to it, from the
/// waste side, with 3 mm of straight on both sides of the seam. The other
/// sides can be oblique.
fn entry(
    shape: &TangentProfile,
    segments: &[Segment],
    hole: Point,
) -> Result<Vec<ArcBlock>, String> {
    for (i, s) in segments.iter().enumerate() {
        if s.center.is_some() {
            continue;
        }
        let axis = if s.from[1] == s.to[1] {
            0
        } else if s.from[0] == s.to[0] {
            1
        } else {
            continue;
        };
        let mut seam = hole;
        seam[1 - axis] = s.from[1 - axis];
        let direction = delta(s.from, s.to);
        let sign = direction[axis].signum();
        if (seam[axis] - s.from[axis]) * sign < LEAD || (s.to[axis] - seam[axis]) * sign < LEAD {
            continue;
        }
        // The waste side of a counter-clockwise loop is on the right.
        let normal = [direction[1].signum(), -direction[0].signum()];
        let outside = (hole[0] - seam[0]) * normal[0] + (hole[1] - seam[1]) * normal[1];
        let face = (seam[0] * normal[0] + seam[1] * normal[1]) as f64 / UNITS;
        // The lead is perpendicular to a supporting line of the convex loop,
        // so all of the punch is on the inner side of it.
        if outside < LEAD || shape.support(normal.map(|n| n as f64)) > face + 1e-9 {
            continue;
        }
        let mut blocks = vec![line(seam), line(s.to)];
        blocks.extend(
            segments[i + 1..]
                .iter()
                .chain(&segments[..i])
                .map(Segment::block),
        );
        blocks.push(line(seam));
        return Ok(blocks);
    }
    Err("The DXF punch needs the threading hole at least 3 mm outside a straight side parallel to X or Y, with at least 3 mm of that straight on both sides of the perpendicular entry".into())
}
