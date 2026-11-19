//! Provenance: Autodesk's published ASCII DXF Reference (HEADER, TABLES, ENTITIES, LINE, ARC, CIRCLE); original implementation.

use std::collections::BTreeSet;
use std::f64::consts::TAU;
use std::fmt::{Display, Write as _};

use super::Units;
use super::entities::{circle_range, coordinate, layer_name};
use super::types::{Document, DxfError, Limits, Primitive, error};

/// Serialize a 2D [`Document`] to ASCII DXF text. Refuses invalid coordinates,
/// zero-length segments, invalid arcs/circles, unsupported unit codes, or
/// malformed layer/line-type strings by name.
pub fn write(doc: &Document) -> Result<String, DxfError> {
    write_with_limits(doc, Limits::default())
}

/// Serialize a 2D [`Document`] to UTF-8 ASCII DXF bytes.
pub fn write_bytes(doc: &Document) -> Result<Vec<u8>, DxfError> {
    write(doc).map(String::into_bytes)
}

fn write_with_limits(doc: &Document, limits: Limits) -> Result<String, DxfError> {
    if doc.layers.len() > limits.max_entities {
        return Err(error("LAYER", "layer count exceeds limit"));
    }
    if doc.primitives.len() > limits.max_entities {
        return Err(error("ENTITIES", "entity count exceeds limit"));
    }
    let unit_code = match doc.units {
        Units::Known(u) => u.code(),
        Units::MissingOrUnitless => 0,
        Units::Unsupported(code) => {
            return Err(error(
                "$INSUNITS",
                &format!("unsupported unit code {code} cannot be written"),
            ));
        }
    };
    let z = match doc.elevation {
        Some(v) => Some(coordinate(v, "elevation")?),
        None => None,
    };

    let mut out = String::new();
    pair(&mut out, 0, "SECTION");
    pair(&mut out, 2, "HEADER");
    pair(&mut out, 9, "$INSUNITS");
    pair(&mut out, 70, unit_code);
    pair(&mut out, 0, "ENDSEC");

    if !doc.layers.is_empty() {
        write_layers(&mut out, doc)?;
    }

    pair(&mut out, 0, "SECTION");
    pair(&mut out, 2, "ENTITIES");
    for item in &doc.primitives {
        write_primitive(&mut out, &item.layer, &item.primitive, z)?;
    }
    pair(&mut out, 0, "ENDSEC");
    pair(&mut out, 0, "EOF");

    if out.len() > limits.max_bytes {
        return Err(error("file", "output size exceeds limit"));
    }
    Ok(out)
}

fn write_layers(out: &mut String, doc: &Document) -> Result<(), DxfError> {
    pair(out, 0, "SECTION");
    pair(out, 2, "TABLES");
    pair(out, 0, "TABLE");
    pair(out, 2, "LAYER");
    let mut seen = BTreeSet::new();
    for layer in &doc.layers {
        layer_name(&layer.name, "LAYER")?;
        no_newlines(&layer.name, "LAYER", "layer name")?;
        if !seen.insert(layer.name.as_str()) {
            return Err(error("LAYER", "duplicate layer name"));
        }
        if layer.line_type.is_empty() || layer.line_type.len() > 255 {
            return Err(error("LAYER", "line type must contain 1 to 255 bytes"));
        }
        no_newlines(&layer.line_type, "LAYER", "line type")?;
        pair(out, 0, "LAYER");
        pair(out, 2, &layer.name);
        pair(out, 70, layer.flags);
        pair(out, 62, layer.color);
        pair(out, 6, &layer.line_type);
    }
    pair(out, 0, "ENDTAB");
    pair(out, 0, "ENDSEC");
    Ok(())
}

fn write_primitive(
    out: &mut String,
    layer: &str,
    primitive: &Primitive,
    z: Option<f64>,
) -> Result<(), DxfError> {
    let kind = match primitive {
        Primitive::Line { .. } => "LINE",
        Primitive::Arc { .. } => "ARC",
        Primitive::Circle { .. } => "CIRCLE",
    };
    layer_name(layer, kind)?;
    no_newlines(layer, kind, "layer name")?;
    match *primitive {
        Primitive::Line { start, end } => {
            let sx = coordinate(start.x, "LINE")?;
            let sy = coordinate(start.y, "LINE")?;
            let ex = coordinate(end.x, "LINE")?;
            let ey = coordinate(end.y, "LINE")?;
            if sx == ex && sy == ey {
                return Err(error("LINE", "zero-length entity"));
            }
            pair(out, 0, "LINE");
            pair(out, 8, layer);
            fpair(out, 10, sx);
            fpair(out, 20, sy);
            if let Some(zv) = z {
                fpair(out, 30, zv);
            }
            fpair(out, 11, ex);
            fpair(out, 21, ey);
            if let Some(zv) = z {
                fpair(out, 31, zv);
            }
        }
        Primitive::Arc {
            center,
            radius,
            start_angle,
            sweep_angle,
        } => {
            let cx = coordinate(center.x, "ARC")?;
            let cy = coordinate(center.y, "ARC")?;
            circle_range(center, radius, "ARC")?;
            if !start_angle.is_finite()
                || !sweep_angle.is_finite()
                || sweep_angle <= 0.0
                || sweep_angle >= TAU
            {
                return Err(error(
                    "ARC",
                    "sweep angle must be finite, strictly positive, and less than 2*pi",
                ));
            }
            let a_deg = start_angle.to_degrees().rem_euclid(360.0);
            let b_deg = (start_angle + sweep_angle).to_degrees().rem_euclid(360.0);
            let sweep_back = (b_deg.rem_euclid(360.0).to_radians()
                - a_deg.rem_euclid(360.0).to_radians())
            .rem_euclid(TAU);
            if sweep_back == 0.0 {
                return Err(error("ARC", "zero-length angular sweep"));
            }
            pair(out, 0, "ARC");
            pair(out, 8, layer);
            fpair(out, 10, cx);
            fpair(out, 20, cy);
            if let Some(zv) = z {
                fpair(out, 30, zv);
            }
            fpair(out, 40, radius);
            fpair(out, 50, a_deg);
            fpair(out, 51, b_deg);
        }
        Primitive::Circle { center, radius } => {
            let cx = coordinate(center.x, "CIRCLE")?;
            let cy = coordinate(center.y, "CIRCLE")?;
            circle_range(center, radius, "CIRCLE")?;
            pair(out, 0, "CIRCLE");
            pair(out, 8, layer);
            fpair(out, 10, cx);
            fpair(out, 20, cy);
            if let Some(zv) = z {
                fpair(out, 30, zv);
            }
            fpair(out, 40, radius);
        }
    }
    Ok(())
}

fn no_newlines(text: &str, kind: &str, field: &str) -> Result<(), DxfError> {
    if text.contains(['\n', '\r']) {
        return Err(error(kind, &format!("{field} cannot contain line breaks")));
    }
    Ok(())
}

fn pair(out: &mut String, code: u16, value: impl Display) {
    let _ = writeln!(out, "{code}\n{value}");
}

fn fpair(out: &mut String, code: u16, value: f64) {
    let clean = if value == 0.0 { 0.0 } else { value };
    let _ = writeln!(out, "{code}\n{clean}");
}
