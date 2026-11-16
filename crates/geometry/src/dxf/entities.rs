//! Provenance: Autodesk DXF LINE/ARC/CIRCLE fields and OCS extrusion conventions.
use super::pairs::Record;
use super::types::{DxfError, Primitive, State, error};
use crate::geo2d::{COORD_LIMIT, Point2};
use std::f64::consts::TAU;

pub(super) fn coordinate(value: f64, kind: &str) -> Result<f64, DxfError> {
    if !value.is_finite() || value.abs() >= COORD_LIMIT {
        return Err(error(kind, "coordinate outside geo2d range"));
    }
    Ok(value)
}

pub(super) fn point(record: Record<'_>, x: u16, y: u16) -> Result<Point2, DxfError> {
    Ok(Point2::new(
        coordinate(record.number(x, None)?, record.kind)?,
        coordinate(record.number(y, None)?, record.kind)?,
    ))
}

pub(super) fn common(record: Record<'_>) -> Result<(), DxfError> {
    record.validate()?;
    if record.number(210, Some(0.0))? != 0.0
        || record.number(220, Some(0.0))? != 0.0
        || record.number(230, Some(1.0))? <= 0.0
    {
        return Err(error(record.kind, "extrusion must point along +Z"));
    }
    if record.number(39, Some(0.0))? != 0.0 {
        return Err(error(record.kind, "nonzero thickness is unsupported"));
    }
    if record.integer(67, 0)? != 0 {
        return Err(error(record.kind, "paper space geometry is unsupported"));
    }
    Ok(())
}

pub(super) fn layer(record: Record<'_>) -> Result<&str, DxfError> {
    let name = record.field(8)?.unwrap_or("0");
    layer_name(name, record.kind)?;
    Ok(name)
}

pub(super) fn layer_name(name: &str, kind: &str) -> Result<(), DxfError> {
    // DXF $EXTNAMES allows 255 characters. This ASCII reader caps bytes as well
    // so copying a layer name per segment cannot amplify an unbounded string.
    if name.is_empty() || name.len() > 255 {
        return Err(error(kind, "layer name must contain 1 to 255 bytes"));
    }
    Ok(())
}

/// Conservatively bound the entire support circle, including the center and radius.
pub(super) fn circle_range(center: Point2, radius: f64, kind: &str) -> Result<(), DxfError> {
    if !radius.is_finite() || radius <= 0.0 {
        return Err(error(kind, "radius must be finite and positive"));
    }
    coordinate(center.x.abs() + radius, kind)?;
    coordinate(center.y.abs() + radius, kind)?;
    Ok(())
}

pub(super) fn simple(record: Record<'_>, state: &mut State) -> Result<(), DxfError> {
    common(record)?;
    let layer = layer(record)?;
    let start = point(record, 10, 20)?;
    let z = coordinate(record.number(30, Some(0.0))?, record.kind)?;
    state.elevation(record.kind, z)?;
    if record.kind == "LINE" {
        let end = point(record, 11, 21)?;
        let end_z = coordinate(record.number(31, Some(0.0))?, record.kind)?;
        state.elevation(record.kind, end_z)?;
        if start == end {
            return Err(error("LINE", "zero-length entity"));
        }
        state.push(layer, Primitive::Line { start, end });
    } else {
        let radius = record.number(40, None)?;
        circle_range(start, radius, record.kind)?;
        let primitive = if record.kind == "CIRCLE" {
            Primitive::Circle {
                center: start,
                radius,
            }
        } else {
            // Normalize before subtraction so large finite angles cannot overflow.
            let a = record.number(50, None)?.rem_euclid(360.0).to_radians();
            let b = record.number(51, None)?.rem_euclid(360.0).to_radians();
            let sweep_angle = (b - a).rem_euclid(TAU);
            if sweep_angle == 0.0 {
                return Err(error("ARC", "zero-length angular sweep"));
            }
            Primitive::Arc {
                center: start,
                radius,
                start_angle: a,
                sweep_angle,
            }
        };
        state.push(layer, primitive);
    }
    Ok(())
}

pub(super) fn segment(
    start: Point2,
    end: Point2,
    bulge: f64,
    kind: &str,
) -> Result<Primitive, DxfError> {
    if start == end {
        return Err(error(kind, "zero-length segment"));
    }
    if bulge == 0.0 {
        return Ok(Primitive::Line { start, end });
    }
    let dx = end.x - start.x;
    let dy = end.y - start.y;
    let chord = dx.hypot(dy);
    let offset = chord * 0.25 * (1.0 / bulge - bulge);
    let center = Point2::new(
        (start.x + end.x) * 0.5 - dy / chord * offset,
        (start.y + end.y) * 0.5 + dx / chord * offset,
    );
    let radius = chord * 0.25 * (bulge.abs() + 1.0 / bulge.abs());
    circle_range(center, radius, kind)?;
    let sweep_angle = 4.0 * bulge.atan();
    if sweep_angle == 0.0 || sweep_angle.abs() >= TAU {
        return Err(error(kind, "bulge sweep cannot be represented safely"));
    }
    let start_angle = (start.y - center.y).atan2(start.x - center.x);
    // Drawing units are unresolved. Bound endpoint loss relative to the source
    // chord, not the potentially enormous support circle or an absolute unit.
    let endpoint_tolerance = chord * 1e-10;
    for (source, angle) in [(start, start_angle), (end, start_angle + sweep_angle)] {
        let x = center.x + radius * angle.cos();
        let y = center.y + radius * angle.sin();
        let endpoint_error = (x - source.x).hypot(y - source.y);
        if !endpoint_error.is_finite() || endpoint_error > endpoint_tolerance {
            return Err(error(kind, "bulge arc cannot preserve source endpoints"));
        }
    }
    Ok(Primitive::Arc {
        center,
        radius,
        start_angle,
        sweep_angle,
    })
}
