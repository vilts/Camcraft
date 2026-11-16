//! Contour along an open run on its air side (#1777): an open edge picked
//! from a part is cut with the tool beside it, one tool radius plus the
//! stock to leave off the edge on the side that holds no part material,
//! never with the tool centre on the edge (which cuts the part on the
//! other side). The side is read from the part's sections at the cut
//! levels; a run with part material on both sides, or whose sides change
//! along it or between levels, is refused by name.
//!
//! Provenance: open-profile contouring of published CAM practice (cutter
//! radius compensation to the air side of an open profile, RS-274
//! G41/G42 semantics) and the textbook offset-curve-as-Minkowski-boundary
//! construction; the laws are ours. No vendor material consulted.

use crate::cl::Toolpath;
use crate::geo2d::{Point2, Region};

use super::contour::ContourParams;
use super::contour_chain::contour_along_chain;
use super::face::GenError;

/// How far (mm) beside the run the side probes read the section: well
/// above the slicer's lattice, well below any real feature.
const PROBE: f64 = 0.05;
/// How far (mm) above each cut level the section is read, so a floor
/// that lies exactly on the level counts as the floor it is, not as
/// material beside the run.
pub(super) const LIFT: f64 = 0.01;
/// Points closer than this (mm) are one point.
const SAME: f64 = 1e-9;
/// Slack (mm) of the cap classification of the offset tube.
pub(super) const TOL: f64 = 1e-3;
/// How far (mm) the tool clearance check stays inside the tool edge, so
/// the edge that just grazes the part wall is not read as a cut.
const GRAZE: f64 = 0.01;
/// The smallest overlap (mm^2) of tool and part that counts as a cut.
const CUT_AREA: f64 = 1e-4;

/// The part's section at a height: `Ok(None)` where the part has no
/// material at that height.
pub type SectionAt<'a> = dyn Fn(f64) -> Result<Option<Region>, GenError> + 'a;

/// Contour along the open run `pts`, the tool on the run's air side read
/// from `section`. A run with no part material beside it at any level is
/// ridden on its own line (nothing is there to cut), and the path says so.
pub fn contour_open_run(pts: &[Point2], p: &ContourParams, section: &SectionAt) -> Result<Toolpath, GenError> {
    p.validate()?;
    let run = simple(pts)?;
    let zs = p.levels()?;
    let mut levels: Vec<(f64, Region)> = Vec::with_capacity(zs.len());
    for &z in &zs {
        if let Some(r) = section(z + LIFT)?
            && !r.is_empty()
        {
            levels.push((z, r));
        }
    }
    let Some(material_left) = material_side(&run, &levels, &PART_WORDS)? else {
        let mut path = contour_along_chain(&run, p)?;
        path.notes.push("contour: no part material beside the open run at any level - the tool centre rides its line".into());
        return Ok(path);
    };
    let d = p.tool_diameter * 0.5 + p.stock_to_leave;
    if !(d.is_finite() && d > TOL) {
        return Err(GenError::Invalid(format!(
            "the open run's offset (tool radius plus stock to leave) is {d:.3} mm - it must be positive"
        )));
    }
    // The tool stands on the side without material. The offset comes back
    // with the run (the material's side) on its left; climb travels with
    // the material on its RIGHT (#1555), so it turns round.
    let mut ride = offset_run(&run, d, !material_left)?;
    ride.reverse();
    clear_of_part(&ride, p.tool_diameter * 0.5, &levels)?;
    let mut path = contour_along_chain(&ride, p)?;
    path.notes.push(format!(
        "contour: the open run is cut on its air side - the tool centre {d:.3} mm off it (tool radius plus stock to leave); Side applies to closed rings only"
    ));
    Ok(path)
}

/// What a side read calls the body it reads, and what it advises when
/// that body lies on both sides of the run (#1928: the stock reads too).
pub(super) struct Words {
    pub(super) body: &'static str,
    pub(super) both: &'static str,
}

const PART_WORDS: Words = Words {
    body: "part",
    both: "a tool on it would cut the part; raise the Bottom height above that level, or pick an edge with air on one side",
};

/// Repeated points out; at least one edge.
pub(super) fn simple(pts: &[Point2]) -> Result<Vec<Point2>, GenError> {
    let mut v: Vec<Point2> = Vec::with_capacity(pts.len());
    for q in pts {
        if !(q.x.is_finite() && q.y.is_finite()) {
            return Err(GenError::Invalid("the open run holds a non-finite point - re-pick it".into()));
        }
        if v.last().is_none_or(|l: &Point2| dist(*l, *q) > SAME) {
            v.push(*q);
        }
    }
    if v.len() < 2 {
        return Err(GenError::Empty("a centerline run needs at least two points".into()));
    }
    Ok(v)
}

/// Which side of the run holds the part material: `Some(true)` left of
/// travel, `Some(false)` right, `None` neither at any level. Probes stand
/// `PROBE` mm off the middle of each edge at every level that has
/// material; any probe pair with material on both sides, or two pairs
/// that disagree, refuses the run by name.
pub(super) fn material_side(run: &[Point2], levels: &[(f64, Region)], words: &Words) -> Result<Option<bool>, GenError> {
    let mut side: Option<(bool, f64)> = None;
    for (z, region) in levels {
        for w in run.windows(2) {
            let (a, b) = (w[0], w[1]);
            let l = dist(a, b);
            if l <= 2.0 * PROBE {
                continue;
            }
            let (nx, ny) = (-(b.y - a.y) / l, (b.x - a.x) / l);
            let m = Point2::new((a.x + b.x) * 0.5, (a.y + b.y) * 0.5);
            let left = region.contains(Point2::new(m.x + nx * PROBE, m.y + ny * PROBE));
            let right = region.contains(Point2::new(m.x - nx * PROBE, m.y - ny * PROBE));
            let here = match (left, right) {
                (true, true) => {
                    return Err(GenError::invalid_at(
                        format!(
                            "the open run has {} material on both sides at Z {z:.3} near ({:.3}, {:.3}) - {}",
                            words.body, m.x, m.y, words.both
                        ),
                        crate::geometry::Point3::new(m.x, m.y, *z),
                    ));
                }
                (false, false) => continue,
                (l, _) => l,
            };
            match side {
                None => side = Some((here, *z)),
                Some((s, z0)) if s != here => {
                    return Err(GenError::invalid_at(
                        format!(
                            "the open run has the {} on its {} side at Z {z0:.3} and on its {} side at Z {z:.3} \
                             near ({:.3}, {:.3}) - its air side is not clear; pick an edge with the {} on one side only",
                            words.body,
                            if s { "left" } else { "right" },
                            if here { "left" } else { "right" },
                            m.x,
                            m.y,
                            words.body
                        ),
                        crate::geometry::Point3::new(m.x, m.y, *z),
                    ));
                }
                Some(_) => {}
            }
        }
    }
    Ok(side.map(|(s, _)| s))
}

/// The tool, swept along the ride at every level with material, must not
/// overlap the part: an edge that ends at a wall, or a part that comes
/// back within the tool's reach, is refused by name, never posted.
pub(super) fn clear_of_part(ride: &[Point2], r: f64, levels: &[(f64, Region)]) -> Result<(), GenError> {
    if r <= GRAZE {
        return Ok(());
    }
    let tool = Region::sweep(&[ride.to_vec()], r - GRAZE)?;
    for (z, region) in levels {
        let cut = tool.intersect(region)?;
        if cut.area() > CUT_AREA {
            let (x, y) = cut.bounds().map_or((ride[0].x, ride[0].y), |(lo, hi)| ((lo.x + hi.x) * 0.5, (lo.y + hi.y) * 0.5));
            return Err(GenError::invalid_at(
                format!(
                    "the tool beside the open run cuts the part at Z {z:.3} near ({x:.3}, {y:.3}) - the run ends at a \
                     wall or the part comes back within the tool's reach; pick the closed ring, or a smaller tool"
                ),
                crate::geometry::Point3::new(x, y, *z),
            ));
        }
    }
    Ok(())
}

fn dist(a: Point2, b: Point2) -> f64 {
    (a.x - b.x).hypot(a.y - b.y)
}

fn cross(o: Point2, a: Point2, b: Point2) -> f64 {
    (a.x - o.x) * (b.y - o.y) - (a.y - o.y) * (b.x - o.x)
}

/// The unclamped parameter of `p`'s foot on ab and the clamped distance.
fn foot(p: Point2, a: Point2, b: Point2) -> (f64, f64) {
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    let l2 = dx * dx + dy * dy;
    if l2 <= 0.0 {
        return (0.0, dist(p, a));
    }
    let t = ((p.x - a.x) * dx + (p.y - a.y) * dy) / l2;
    let tc = t.clamp(0.0, 1.0);
    (t, dist(p, Point2::new(a.x + dx * tc, a.y + dy * tc)))
}

/// One side of the run's offset at `d`: the boundary of the disc swept
/// along it (`Region::sweep`, the clipper2 Minkowski buffer), minus its
/// round end caps, travelled with the run on its left. A run whose offset
/// folds onto itself is refused by name, never trimmed.
pub(super) fn offset_run(run: &[Point2], d: f64, left: bool) -> Result<Vec<Point2>, GenError> {
    let folds = || {
        GenError::Invalid(format!(
            "the open run's offset of {d:.3} mm folds onto itself (the run comes back within twice the offset of \
             itself) - pick a gentler run or a smaller tool"
        ))
    };
    let tube = Region::sweep(&[run.to_vec()], d)?;
    let [ring] = tube.loops() else {
        return Err(folds());
    };
    if !ring.is_ccw() {
        return Err(folds());
    }
    let n = run.len();
    let keep: Vec<bool> = ring
        .points()
        .iter()
        .map(|&q| {
            let mut best = (f64::INFINITY, 0usize, 0.0);
            for i in 0..n - 1 {
                let (t, dd) = foot(q, run[i], run[i + 1]);
                if dd < best.0 {
                    best = (dd, i, t);
                }
            }
            let (_, i, t) = best;
            let len = dist(run[i], run[i + 1]).max(SAME);
            let cap = (i == 0 && t * len < -TOL) || (i == n - 2 && (t - 1.0) * len > TOL);
            !cap && (cross(run[i], run[i + 1], q) > 0.0) == left
        })
        .collect();
    let m = keep.len();
    let starts: Vec<usize> = (0..m).filter(|&k| keep[k] && !keep[(k + m - 1) % m]).collect();
    let [s0] = starts[..] else {
        return Err(folds());
    };
    let mut ride: Vec<Point2> =
        (0..m).map(|k| (s0 + k) % m).take_while(|&k| keep[k]).map(|k| ring.points()[k]).collect();
    // The exact ends: the perpendicular offset points at the run's ends,
    // in CCW order - the right side runs with the run, the left against it.
    let normal_at = |a: Point2, b: Point2, at: Point2| {
        let l = dist(a, b);
        let (nx, ny) = (-(b.y - a.y) / l, (b.x - a.x) / l);
        let s = if left { d } else { -d };
        Point2::new(at.x + nx * s, at.y + ny * s)
    };
    let p0 = normal_at(run[0], run[1], run[0]);
    let p1 = normal_at(run[n - 2], run[n - 1], run[n - 1]);
    let (first, last) = if left { (p1, p0) } else { (p0, p1) };
    ride.retain(|q| dist(*q, first) > TOL && dist(*q, last) > TOL);
    ride.insert(0, first);
    ride.push(last);
    Ok(ride)
}

#[cfg(test)]
#[path = "contour_run_side_tests.rs"]
mod tests;
