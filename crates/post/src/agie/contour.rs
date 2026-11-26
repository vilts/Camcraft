//! Quantize the nominal polygon of a punch or a die opening, check the swept
//! rough pass and find a perpendicular entry on the waste side (planar geometry).
use super::{AgieJob, AgieSide};
use crate::agie_setup::{PunchSetup20, Quality};

pub(super) type Point = [i64; 2];
const UNITS: f64 = 100_000.0; // Five decimal places in the native ISO geometry file.
const MAX_VERTICES: usize = 4096;
mod entry;

pub(super) fn quantize(v: f64) -> Result<i64, String> {
    if !v.is_finite() || v.abs() > 9999.9999 {
        return Err("Job coordinates must be finite and within ±9999.9999 mm".into());
    }
    Ok((v * UNITS).round() as i64)
}

pub(super) fn cross(a: Point, b: Point, p: Point) -> i128 {
    i128::from(b[0] - a[0]) * i128::from(p[1] - a[1])
        - i128::from(b[1] - a[1]) * i128::from(p[0] - a[0])
}

fn between(a: Point, b: Point, p: Point) -> bool {
    (0..2).all(|d| (a[d].min(b[d])..=a[d].max(b[d])).contains(&p[d]))
}

fn intersects(a: Point, b: Point, c: Point, d: Point) -> bool {
    let (u, v, w, z) = (
        cross(a, b, c),
        cross(a, b, d),
        cross(c, d, a),
        cross(c, d, b),
    );
    (u.signum() * v.signum() == -1 && w.signum() * z.signum() == -1)
        || (u == 0 && between(a, b, c))
        || (v == 0 && between(a, b, d))
        || (w == 0 && between(c, d, a))
        || (z == 0 && between(c, d, b))
}

pub(super) fn micrometres(value: f64) -> Result<i32, String> {
    // quantize rejects nonfinite or out-of-range geometry before conversion.
    let hundredth_microns = quantize(value)?;
    let microns = i32::try_from(hundredth_microns / 100)
        .map_err(|_| "The setup coordinate exceeds the micrometre range")?;
    if (value - f64::from(microns) / 1000.0).abs() > 1e-9 {
        return Err(
            "The stock and start-hole coordinates must be representable to 0.001 mm".into(),
        );
    }
    Ok(microns)
}

pub(super) fn punch_setup<'a>(
    job: &AgieJob,
    program: &'a str,
    quality: Quality,
) -> Result<PunchSetup20<'a>, String> {
    let [width, depth, height] = std::array::from_fn(|i| job.stock_max[i] - job.stock_min[i]);
    if !(width > 0.0 && width <= 350.0 && depth > 0.0 && depth <= 250.0) {
        return Err("The stock must fit the stated 350 × 250 mm working span".into());
    }
    // The CCB030 texts are those of #2984 and #2954, unchanged.
    let (profile, recipe) = match quality {
        Quality::Ccb030 => ("The Berco Fine Ra0.30 profile", "The QN2 recipe"),
        Quality::Ccb035 => ("The Berco Fine+ Ra0.35 profile", "The CCB035 quality"),
    };
    if job.stock_max[2] != 0.0 {
        return Err(format!("{profile} requires stock top Z0"));
    }
    // Only a measured thickness has known offsets. No height is interpolated.
    let table = quality.thicknesses_mm();
    let Some(&mm) = table.iter().find(|mm| height == f64::from(**mm)) else {
        let supported = table
            .iter()
            .map(u16::to_string)
            .collect::<Vec<_>>()
            .join(", ");
        return Err(format!(
            "{recipe} has no measured offset for stock thickness {height} mm. Supported thicknesses in mm are {supported}"
        ));
    };
    // The control checked the die cycle on a 20 mm plate only (#2957).
    if job.side == AgieSide::Die && i32::from(mm) * 1000 != crate::agie_setup::DIE_HEIGHT_UM {
        return Err("The die opening requires the qualified 20 mm stock height".into());
    }
    // An open cut has two air endpoints and no hole; `cutoff` prepares it.
    let start = match (job.side, job.start) {
        (AgieSide::Punch | AgieSide::Die, Some(start)) => start,
        (AgieSide::Punch | AgieSide::Die, None) => {
            return Err("The closed contour requires a predrilled 2 mm start hole".into());
        }
        (AgieSide::Left | AgieSide::Right, _) => {
            return Err("An open cut has no closed contour or start hole".into());
        }
    };
    if !(0..2).all(|i| start[i] - 1.0 >= job.stock_min[i] && start[i] + 1.0 <= job.stock_max[i]) {
        return Err("The complete 2 mm start hole must lie inside the stock".into());
    }
    Ok(PunchSetup20 {
        stem: program,
        stock_min_um: [
            micrometres(job.stock_min[0])?,
            micrometres(job.stock_min[1])?,
        ],
        stock_max_um: [
            micrometres(job.stock_max[0])?,
            micrometres(job.stock_max[1])?,
        ],
        start_hole_um: [micrometres(start[0])?, micrometres(start[1])?],
        height_um: i32::from(mm) * 1000,
    })
}

pub(super) fn prepare<'a>(
    job: &AgieJob,
    program: &'a str,
) -> Result<(Vec<[f64; 2]>, PunchSetup20<'a>), String> {
    prepare_with_reach(job, program, Quality::Ccb030).map(|(points, setup, _)| (points, setup))
}

/// [`prepare`], and the largest distance in mm from a nominal vertex to its
/// rough miter. A punch group bounds each work with it (#2961).
pub(super) fn prepare_with_reach<'a>(
    job: &AgieJob,
    program: &'a str,
    quality: Quality,
) -> Result<(Vec<[f64; 2]>, PunchSetup20<'a>, f64), String> {
    let setup = punch_setup(job, program, quality)?;
    // The clearance and miter checks use the rough offset of this quality.
    let rough_um = quality
        .rough_um(setup.height_um)
        .ok_or("The selected quality has no measured offset for this stock thickness")?;
    let input = &job.contour;
    if input.len() < 3 || input.len() > MAX_VERTICES + 1 {
        return Err(format!(
            "The job needs one closed nominal polygon with 3–{MAX_VERTICES} vertices"
        ));
    }
    let mut ring = Vec::with_capacity(input.len());
    for &[x, y] in input {
        let p = [quantize(x)?, quantize(y)?];
        if !(0..2).all(|i| {
            let value = p[i] as f64 / UNITS;
            value >= job.stock_min[i] && value <= job.stock_max[i]
        }) {
            return Err("The nominal contour extends outside the stock".into());
        }
        ring.push(p);
    }
    if input.first() == input.last() {
        ring.pop();
    }
    let n = ring.len();
    if !(3..=MAX_VERTICES).contains(&n) {
        return Err("The contour vertex count exceeds the supported range".into());
    }
    // Validate quantized segments, including the closing edge, before choosing a start point.
    // Keep collinear intermediate vertices out of the search for the perpendicular entry edge.
    let mut outline = Vec::with_capacity(n);
    for i in 0..n {
        let a = ring[(i + n - 1) % n];
        let b = ring[i];
        let c = ring[(i + 1) % n];
        if a == b || (cross(a, b, c) == 0 && !between(a, c, b)) {
            return Err("The nominal contour contains a collapsed or reversed edge".into());
        }
        if cross(a, b, c) != 0 {
            outline.push(b);
        }
    }
    let n = outline.len();
    let mut area = 0_i128;
    for i in 0..n {
        let (a, b) = (outline[i], outline[(i + 1) % n]);
        area += cross([0, 0], a, b);
        for j in (i + 2)..n {
            if i == 0 && j + 1 == n {
                continue;
            }
            if intersects(a, b, outline[j], outline[(j + 1) % n]) {
                return Err("The nominal contour crosses or touches itself".into());
            }
        }
    }
    if area == 0 {
        return Err("The nominal contour has no enclosed area".into());
    }
    if area < 0 {
        outline.reverse();
    }
    if job.side == AgieSide::Die {
        // Convexity proves that every straight lead from the inside hole to
        // the chosen wall stays in the opening throughout its length.
        if (0..n).any(|i| cross(outline[i], outline[(i + 1) % n], outline[(i + 2) % n]) <= 0) {
            return Err("The job supports a convex die opening only".into());
        }
        // Clockwise, the retained plate is on the left and the waste on the
        // right, as outside a counter-clockwise punch. The offset direction
        // and the convex/reentrant sense of each corner flip with it: every
        // corner of the opening is an inside corner of the plate.
        outline.reverse();
    }
    // The hole (radius 1 mm) and the swept rough pass both fit in the convex
    // stock. The straight perpendicular entry between them fits too.
    let mut reach = check_cut_envelope(&outline, job, rough_um)?;
    let start = job
        .start
        .ok_or("The closed contour requires a predrilled 2 mm start hole")?;
    let hole = [quantize(start[0])?, quantize(start[1])?];
    let (edge, foot) = entry::entry_face(&outline, hole, job.side, stock_margin(rough_um))?;
    // Begin the nominal program at the middle of the chosen edge, facing along that edge. The
    // hole and the full three-millimetre lead share its waste half-plane. Compensation is
    // deliberately left to the control technology. The closing move returns to the inserted point.
    let exact_face = cross(outline[edge], outline[(edge + 1) % n], foot) == 0;
    outline.rotate_left((edge + 1) % n);
    let mut vertices = vec![foot];
    vertices.extend(outline);
    // A rounded seam on the waste side adds a small corner and slightly
    // changes its neighbours. Check the envelope of the actual ISO lines.
    if !exact_face {
        reach = reach.max(check_cut_envelope(&vertices, job, rough_um)?);
    }
    if job.side == AgieSide::Die {
        // The control read the die opening as counter-clockwise nominal ISO
        // geometry with c_die (#2957). Keep the seam first, so the separation
        // stays on the chosen wall.
        vertices[1..].reverse();
    }
    let vertices = vertices
        .into_iter()
        .map(|p| [p[0] as f64 / UNITS, p[1] as f64 / UNITS])
        .collect();
    Ok((vertices, setup, reach))
}

/// The measured rough wire-centre offset plus the 125 µm wire radius, rounded
/// up to a tenth of a millimetre, in µm. All measured heights give 0.4 mm.
fn margin_um(rough_um: u16) -> u32 {
    (u32::from(rough_um) + 125).next_multiple_of(100)
}

/// The straight and round swept envelope of the rough pass and wire, in ISO lattice units.
pub(super) fn stock_margin(rough_um: u16) -> i64 {
    i64::from(margin_um(rough_um)) * 100
}

/// Check the swept wire against the stock before the control chooses its
/// corner joins. The ring has the retained material on its left and the waste
/// on its right. A round join lies within the rough offset of its nominal
/// vertex. A miter reaches the intersection of the two parallel face lines
/// offset toward the waste. The segments that connect adjacent joins stay
/// inside the convex stock bounds.
/// Returns the longest miter in mm.
fn check_cut_envelope(ring: &[Point], job: &AgieJob, rough_um: u16) -> Result<f64, String> {
    // The miter below uses the exact offset.
    let margin_um = margin_um(rough_um);
    let stock_margin = stock_margin(rough_um);
    let rough_offset = f64::from(rough_um) * 100.0;
    let wire_and_allowance = stock_margin as f64 - rough_offset;
    let lo = [quantize(job.stock_min[0])?, quantize(job.stock_min[1])?];
    let hi = [quantize(job.stock_max[0])?, quantize(job.stock_max[1])?];
    let n = ring.len();
    let outward = |a: Point, b: Point| {
        let dx = (b[0] - a[0]) as f64;
        let dy = (b[1] - a[1]) as f64;
        // Adjacent ring points are distinct (checked by the caller), so the length is positive.
        let length = dx.hypot(dy);
        [dy / length, -dx / length]
    };
    let mut reentrant = false;
    let mut max_miter: f64 = 0.0;
    for (i, &corner) in ring.iter().enumerate() {
        let previous = ring[(i + n - 1) % n];
        let next = ring[(i + 1) % n];
        let a = outward(previous, corner);
        let b = outward(corner, next);
        // Use the squared bisector length instead of 1 + dot(a, b);
        // the latter loses precision when the corner is very sharp.
        let bisector = [a[0] + b[0], a[1] + b[1]];
        let norm2 = bisector[0] * bisector[0] + bisector[1] * bisector[1];
        if norm2.is_nan() || norm2 <= f64::EPSILON {
            return Err(
                "The rough pass cannot be bounded at a nearly reversed contour corner".into(),
            );
        }
        let reach = 2.0 * rough_offset / norm2.sqrt();
        max_miter = max_miter.max(reach);
        if cross(previous, corner, next) < 0 {
            reentrant = true;
            // At an inside turn the two offset faces intersect outside the
            // drawing corner. The strict wire cover law permits 0.002 mm of
            // rest. Use the largest resident offset of all passes (the main cut).
            if reach - rough_offset > 0.002 * UNITS {
                return Err(
                    "The rough compensation cannot reach the reentrant corner within 0.002 mm"
                        .into(),
                );
            }
        }
        for axis in 0..2 {
            if corner[axis] - stock_margin < lo[axis] || corner[axis] + stock_margin > hi[axis] {
                return Err(format!(
                    "The rough pass and wire need at least {}.{} mm of stock beyond every contour face",
                    margin_um / 1000,
                    margin_um % 1000 / 100,
                ));
            }
            let miter = 2.0 * rough_offset * bisector[axis] / norm2;
            let left = (lo[axis] - corner[axis]) as f64;
            let right = (hi[axis] - corner[axis]) as f64;
            if !miter.is_finite()
                || miter - wire_and_allowance < left
                || miter + wire_and_allowance > right
            {
                return Err(
                    "The rough corner and wire can reach outside the stock at a contour vertex"
                        .into(),
                );
            }
        }
    }
    if reentrant {
        // The full segment between two joins stays inside the nominal edge's
        // capsule. Bound each join by the longest miter, plus a complete rough
        // kerf allowance. Disjoint capsules on unrelated faces exclude contact
        // at every smaller offset, including all trims.
        let clearance = (2.0 * (max_miter + rough_offset)).ceil() as i64;
        super::spacing::check_feature_spacing(ring, clearance)?;
    }
    Ok(max_miter / UNITS)
}

pub(super) fn vector(a: Point, b: Point) -> Point {
    [b[0] - a[0], b[1] - a[1]]
}

pub(super) fn dot(a: Point, b: Point) -> i128 {
    i128::from(a[0]) * i128::from(b[0]) + i128::from(a[1]) * i128::from(b[1])
}
