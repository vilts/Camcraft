//! Bounded ASCII DXF writer; emits HEADER, TABLES, and ENTITIES sections
//! conforming to Autodesk's public ASCII DXF group-code specification.

use std::collections::BTreeSet;
use std::fmt::Write as _;

use crate::geo2d::{COORD_LIMIT, Point2};

use super::types::{DxfError, TextAnnotation, error};
use super::units::Unit;

/// An entity to serialize into the DXF ENTITIES section.
#[derive(Clone, Debug, PartialEq)]
pub enum ExportEntity {
    Line {
        layer: String,
        start: Point2,
        end: Point2,
    },
    Arc {
        layer: String,
        center: Point2,
        radius: f64,
        start_deg: f64,
        end_deg: f64,
    },
    Circle {
        layer: String,
        center: Point2,
        radius: f64,
    },
    /// Polyline vertices `(point, bulge)`; `closed` writes flag `70 = 1`.
    LwPolyline {
        layer: String,
        closed: bool,
        vertices: Vec<(Point2, f64)>,
    },
}

fn check_layer(name: &str, ctx: &str) -> Result<(), DxfError> {
    if name.is_empty() || name.len() > 255 || name.contains(['\n', '\r']) {
        return Err(error(ctx, "invalid DXF layer name"));
    }
    Ok(())
}

fn check_coord(v: f64, ctx: &str) -> Result<f64, DxfError> {
    if !v.is_finite() || v.abs() >= COORD_LIMIT {
        return Err(error(ctx, "non-finite or out-of-range coordinate"));
    }
    Ok(v)
}

fn check_point(p: Point2, ctx: &str) -> Result<Point2, DxfError> {
    Ok(Point2::new(check_coord(p.x, ctx)?, check_coord(p.y, ctx)?))
}

fn write_pair(out: &mut String, code: u16, val: &str) {
    let _ = writeln!(out, "{code}\n{val}");
}

fn write_num(out: &mut String, code: u16, val: f64) {
    let clean = if val == 0.0 { 0.0 } else { val };
    let _ = writeln!(out, "{code}\n{clean}");
}

/// Serialize DXF layers, entities, and optional text annotations into ASCII DXF text.
pub fn to_text(
    units: Option<Unit>,
    extra_layers: &[&str],
    entities: &[ExportEntity],
    annotations: &[TextAnnotation],
) -> Result<String, DxfError> {
    let mut layers: BTreeSet<String> = BTreeSet::new();
    for &l in extra_layers {
        check_layer(l, "LAYER")?;
        layers.insert(l.to_owned());
    }
    for e in entities {
        let l = match e {
            ExportEntity::Line { layer, .. }
            | ExportEntity::Arc { layer, .. }
            | ExportEntity::Circle { layer, .. }
            | ExportEntity::LwPolyline { layer, .. } => layer.as_str(),
        };
        check_layer(l, "ENTITIES")?;
        layers.insert(l.to_owned());
    }
    for a in annotations {
        check_layer(&a.layer, "TEXT")?;
        if a.text.contains(['\n', '\r']) {
            return Err(error("TEXT", "annotation text cannot contain newlines"));
        }
        layers.insert(a.layer.clone());
    }

    let mut out = String::new();
    write_pair(&mut out, 0, "SECTION");
    write_pair(&mut out, 2, "HEADER");
    if let Some(u) = units {
        write_pair(&mut out, 9, "$INSUNITS");
        write_pair(&mut out, 70, &u.insunits_code().to_string());
    }
    write_pair(&mut out, 0, "ENDSEC");

    write_pair(&mut out, 0, "SECTION");
    write_pair(&mut out, 2, "TABLES");
    if !layers.is_empty() {
        write_pair(&mut out, 0, "TABLE");
        write_pair(&mut out, 2, "LAYER");
        write_pair(&mut out, 70, &layers.len().to_string());
        for (i, l) in layers.iter().enumerate() {
            write_pair(&mut out, 0, "LAYER");
            write_pair(&mut out, 2, l);
            write_pair(&mut out, 70, "0");
            write_pair(&mut out, 62, if i == 0 { "7" } else { "3" });
            write_pair(&mut out, 6, "CONTINUOUS");
        }
        write_pair(&mut out, 0, "ENDTAB");
    }
    write_pair(&mut out, 0, "ENDSEC");

    write_pair(&mut out, 0, "SECTION");
    write_pair(&mut out, 2, "ENTITIES");
    for e in entities {
        write_entity(&mut out, e)?;
    }
    for a in annotations {
        let pos = check_point(a.pos, "TEXT")?;
        write_pair(&mut out, 0, "TEXT");
        write_pair(&mut out, 8, &a.layer);
        write_num(&mut out, 10, pos.x);
        write_num(&mut out, 20, pos.y);
        write_num(&mut out, 40, 2.5);
        write_pair(&mut out, 1, &a.text);
    }
    write_pair(&mut out, 0, "ENDSEC");
    write_pair(&mut out, 0, "EOF");
    Ok(out)
}

fn write_entity(out: &mut String, e: &ExportEntity) -> Result<(), DxfError> {
    match e {
        ExportEntity::Line { layer, start, end } => {
            let s = check_point(*start, "LINE")?;
            let e = check_point(*end, "LINE")?;
            if s == e {
                return Err(error("LINE", "zero-length line"));
            }
            write_pair(out, 0, "LINE");
            write_pair(out, 8, layer);
            write_num(out, 10, s.x);
            write_num(out, 20, s.y);
            write_num(out, 11, e.x);
            write_num(out, 21, e.y);
        }
        ExportEntity::Arc { layer, center, radius, start_deg, end_deg } => {
            let c = check_point(*center, "ARC")?;
            if !radius.is_finite() || *radius <= 0.0 {
                return Err(error("ARC", "radius must be finite and positive"));
            }
            check_coord(c.x.abs() + radius, "ARC")?;
            check_coord(c.y.abs() + radius, "ARC")?;
            if !start_deg.is_finite() || !end_deg.is_finite() {
                return Err(error("ARC", "angles must be finite"));
            }
            write_pair(out, 0, "ARC");
            write_pair(out, 8, layer);
            write_num(out, 10, c.x);
            write_num(out, 20, c.y);
            write_num(out, 40, *radius);
            write_num(out, 50, start_deg.rem_euclid(360.0));
            write_num(out, 51, end_deg.rem_euclid(360.0));
        }
        ExportEntity::Circle { layer, center, radius } => {
            let c = check_point(*center, "CIRCLE")?;
            if !radius.is_finite() || *radius <= 0.0 {
                return Err(error("CIRCLE", "radius must be finite and positive"));
            }
            check_coord(c.x.abs() + radius, "CIRCLE")?;
            check_coord(c.y.abs() + radius, "CIRCLE")?;
            write_pair(out, 0, "CIRCLE");
            write_pair(out, 8, layer);
            write_num(out, 10, c.x);
            write_num(out, 20, c.y);
            write_num(out, 40, *radius);
        }
        ExportEntity::LwPolyline { layer, closed, vertices } => {
            if vertices.len() < 2 {
                return Err(error("LWPOLYLINE", "at least two vertices required"));
            }
            write_pair(out, 0, "LWPOLYLINE");
            write_pair(out, 8, layer);
            write_pair(out, 90, &vertices.len().to_string());
            write_pair(out, 70, if *closed { "1" } else { "0" });
            for (pt, bulge) in vertices {
                let p = check_point(*pt, "LWPOLYLINE")?;
                if !bulge.is_finite() {
                    return Err(error("LWPOLYLINE", "non-finite bulge"));
                }
                write_num(out, 10, p.x);
                write_num(out, 20, p.y);
                if *bulge != 0.0 {
                    write_num(out, 42, *bulge);
                }
            }
        }
    }
    Ok(())
}
