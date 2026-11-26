//! Numeric domain checks: every length, point and caller option is finite and inside the machine envelope.
//! Provenance: AGENTS.md Prime Directive (refuse, never emit changed geometry); IEEE 754 binary64 precision bounds for the micron CL lattice.

use camcraft_geometry::geometry::Point3;

use crate::types::{GcodeError, ParseOptions};

/// Largest coordinate or length magnitude in mm (1 km). The micron lattice of
/// `cl::lattice` is exact below about 9e9 mm, the radius square of an arc stays
/// far below the f64 range, and no milling machine has a 1 km axis.
pub const MAX_ABS_MM: f64 = 1.0e6;

/// Largest feed rate in mm/min (100 m/min is far above a milling machine).
pub const MAX_FEED_MM_MIN: f64 = 1.0e7;

/// Largest dwell in seconds (one day).
pub const MAX_DWELL_S: f64 = 86_400.0;

/// Refuse a length (a converted word value or a computed level) outside the domain.
pub fn check_len(line: usize, what: &str, v: f64) -> Result<f64, GcodeError> {
    if v.is_finite() && v.abs() <= MAX_ABS_MM {
        Ok(v)
    } else {
        Err(GcodeError::OutOfRange {
            line,
            message: format!("{what} ({v}) is not finite or is outside +/-{MAX_ABS_MM} mm"),
        })
    }
}

/// Refuse a point with any coordinate outside the domain.
pub fn check_point(line: usize, what: &str, p: Point3) -> Result<Point3, GcodeError> {
    check_len(line, what, p.x)?;
    check_len(line, what, p.y)?;
    check_len(line, what, p.z)?;
    Ok(p)
}

/// Refuse a feed rate that is not finite, not positive or too large after unit conversion.
pub fn check_feed(line: usize, f_mm_min: f64) -> Result<f64, GcodeError> {
    if f_mm_min.is_finite() && f_mm_min > 0.0 && f_mm_min <= MAX_FEED_MM_MIN {
        Ok(f_mm_min)
    } else {
        Err(GcodeError::OutOfRange {
            line,
            message: format!("feed rate ({f_mm_min} mm/min) must be in (0, {MAX_FEED_MM_MIN}]"),
        })
    }
}

/// Refuse a dwell that is not finite, negative or longer than one day.
pub fn check_dwell(line: usize, s: f64) -> Result<f64, GcodeError> {
    if s.is_finite() && (0.0..=MAX_DWELL_S).contains(&s) {
        Ok(s)
    } else {
        Err(GcodeError::OutOfRange {
            line,
            message: format!("dwell ({s} s) must be in [0, {MAX_DWELL_S}] seconds"),
        })
    }
}

/// Validate the caller positions and tolerances before the first block runs.
pub fn check_options(options: &ParseOptions) -> Result<(), GcodeError> {
    let d = &options.dialect;
    point_option("dialect.initial_position", Some(d.initial_position))?;
    tolerance_option("dialect.arc_radius_tolerance_mm", d.arc_radius_tolerance_mm)?;
    tolerance_option("dialect.chip_break_retract_mm", d.chip_break_retract_mm)?;
    let r = &options.references;
    point_option("references.g28", r.g28)?;
    point_option("references.g30", r.g30)?;
    point_option("references.g53_origin", r.g53_origin)?;
    for (i, o) in r.wcs_origins.iter().enumerate() {
        point_option(&format!("references.wcs_origins[{i}]"), *o)?;
    }
    Ok(())
}

fn point_option(field: &str, p: Option<Point3>) -> Result<(), GcodeError> {
    let Some(p) = p else { return Ok(()) };
    let ok = [p.x, p.y, p.z]
        .iter()
        .all(|v| v.is_finite() && v.abs() <= MAX_ABS_MM);
    if ok {
        Ok(())
    } else {
        Err(GcodeError::InvalidOptions {
            field: field.to_string(),
            message: format!("({}, {}, {}) is not finite or is outside +/-{MAX_ABS_MM} mm", p.x, p.y, p.z),
        })
    }
}

fn tolerance_option(field: &str, v: f64) -> Result<(), GcodeError> {
    if v.is_finite() && (0.0..=1.0e3).contains(&v) {
        Ok(())
    } else {
        Err(GcodeError::InvalidOptions {
            field: field.to_string(),
            message: format!("({v}) must be finite and in [0, 1000] mm"),
        })
    }
}
