//! Provenance: Autodesk DXF SECTION, HEADER, TABLES and ENTITIES structure.
use super::Units;
use super::entities::{layer_name, simple};
use super::pairs::{Pairs, Record, integer};
use super::polyline::{legacy, lightweight};
use super::types::{Document, DxfError, Layer, Limits, State, TextAnnotation, error};
use crate::geo2d::Point2;
use std::collections::BTreeSet;

pub fn parse(bytes: &[u8]) -> Result<Document, DxfError> {
    parse_with_limits(bytes, Limits::default())
}

/// Parse an in-memory ASCII DXF. Callers must also cap file reads before calling this API.
pub fn parse_with_limits(bytes: &[u8], limits: Limits) -> Result<Document, DxfError> {
    if bytes.starts_with(b"AutoCAD Binary DXF") {
        return Err(error("Binary DXF", "unsupported; use ASCII DXF"));
    }
    if bytes.len() > limits.max_bytes {
        return Err(error("file", "size exceeds limit"));
    }
    let text = std::str::from_utf8(bytes).map_err(|_| error("text", "invalid UTF-8 ASCII DXF"))?;
    let mut input = Pairs { rest: text };
    let mut state = State {
        doc: Document::default(),
        limits,
    };
    let mut sections = BTreeSet::new();
    loop {
        let record = input
            .record()?
            .ok_or_else(|| error("stream", "missing EOF"))?;
        match record.kind {
            "EOF" => {
                record.empty()?;
                if input.next()?.is_some() {
                    return Err(error("EOF", "data after EOF"));
                }
                return Ok(state.doc);
            }
            "SECTION" => {
                let mut fields = record.pairs();
                let name = fields
                    .next()?
                    .ok_or_else(|| error("SECTION", "missing name"))?;
                if name.code != 2 {
                    return Err(error("SECTION", "expected name in group 2"));
                }
                let name = name.value.trim();
                if !sections.insert(name) {
                    return Err(error(name, "duplicate SECTION"));
                }
                if name == "HEADER" {
                    header(fields, &mut state.doc)?;
                    end_section(&mut input, "HEADER")?;
                } else {
                    if fields.next()?.is_some() {
                        return Err(error(name, "unexpected SECTION fields"));
                    }
                    match name {
                        "ENTITIES" => entities(&mut input, &mut state)?,
                        "TABLES" => tables(&mut input, &mut state)?,
                        "BLOCKS" => end_section(&mut input, "BLOCK")?,
                        _ => return Err(error(name, "unsupported SECTION")),
                    }
                }
            }
            _ => return Err(error(record.kind, "expected SECTION or EOF")),
        }
    }
}

fn end_section(input: &mut Pairs<'_>, context: &str) -> Result<(), DxfError> {
    let record = input
        .record()?
        .ok_or_else(|| error(context, "missing ENDSEC"))?;
    if record.kind != "ENDSEC" {
        return Err(error(
            context,
            "expected ENDSEC; section content is unsupported",
        ));
    }
    record.empty()
}

fn header(mut fields: Pairs<'_>, doc: &mut Document) -> Result<(), DxfError> {
    let mut seen_units = false;
    let mut variable = None;
    let mut units_value = false;
    while let Some(pair) = fields.next()? {
        if pair.code == 9 {
            if variable == Some("$INSUNITS") && !units_value {
                return Err(error("$INSUNITS", "missing group 70 value"));
            }
            variable = Some(pair.value.trim());
            if variable == Some("$INSUNITS") {
                if seen_units {
                    return Err(error("$INSUNITS", "duplicate header variable"));
                }
                seen_units = true;
            }
        } else if variable == Some("$INSUNITS") {
            if pair.code != 70 {
                return Err(error("$INSUNITS", "expected group 70"));
            }
            if units_value {
                return Err(error("$INSUNITS", "duplicate group 70 value"));
            }
            doc.units = Units::from_code(integer(pair.value, "$INSUNITS")?);
            units_value = true;
        } else if variable.is_none() {
            return Err(error("HEADER", "value without a header variable"));
        }
    }
    if seen_units && !units_value {
        return Err(error("$INSUNITS", "missing group 70 value"));
    }
    Ok(())
}

fn entities(input: &mut Pairs<'_>, state: &mut State) -> Result<(), DxfError> {
    loop {
        let record = input
            .record()?
            .ok_or_else(|| error("ENTITIES", "missing ENDSEC"))?;
        if record.kind == "ENDSEC" {
            return record.empty();
        }
        state.entity(record.kind)?;
        match record.kind {
            "LINE" | "ARC" | "CIRCLE" => simple(record, state)?,
            "LWPOLYLINE" => lightweight(record, state)?,
            "POLYLINE" => legacy(record, input, state)?,
            "TEXT" | "MTEXT" | "DIMENSION" | "HATCH" => {
                record.validate()?;
                if matches!(record.kind, "TEXT" | "MTEXT") {
                    let layer = record.field(8)?.unwrap_or("0");
                    layer_name(layer, record.kind)?;
                    if let Some(text) = record.field(1)? {
                        let x = record.number(10, Some(0.0))?;
                        let y = record.number(20, Some(0.0))?;
                        if x.is_finite() && y.is_finite() {
                            state.doc.annotations.push(TextAnnotation {
                                layer: layer.into(),
                                text: text.trim().to_owned(),
                                pos: Point2::new(x, y),
                            });
                        }
                    }
                }
                *state
                    .doc
                    .summary
                    .skipped
                    .entry(record.kind.into())
                    .or_default() += 1;
            }
            "EOF" | "SECTION" => return Err(error("ENTITIES", "missing ENDSEC")),
            _ => return Err(error(record.kind, "unsupported entity")),
        }
    }
}

fn tables(input: &mut Pairs<'_>, state: &mut State) -> Result<(), DxfError> {
    let mut table = None;
    let mut names = BTreeSet::new();
    loop {
        let record = input
            .record()?
            .ok_or_else(|| error("TABLES", "missing ENDSEC"))?;
        match record.kind {
            "ENDSEC" => {
                if table.is_some() {
                    return Err(error("TABLES", "missing ENDTAB"));
                }
                return record.empty();
            }
            "TABLE" => {
                if table.is_some() {
                    return Err(error("TABLES", "nested TABLE"));
                }
                table = Some(record.required(2)?);
            }
            "ENDTAB" => {
                if table.take().is_none() {
                    return Err(error("TABLES", "unexpected ENDTAB"));
                }
                record.empty()?;
            }
            "EOF" | "SECTION" => return Err(error("TABLES", "missing ENDSEC")),
            kind => {
                if table != Some(kind) {
                    return Err(error(kind, "entry does not match TABLE"));
                }
                if kind == "LAYER" {
                    if state.doc.layers.len() >= state.limits.max_entities {
                        return Err(error("LAYER", "entity count exceeds limit"));
                    }
                    let name = record.required(2)?;
                    layer_name(name, "LAYER")?;
                    if !names.insert(name) {
                        return Err(error("LAYER", "duplicate layer name"));
                    }
                    state.doc.layers.push(read_layer(record, name)?);
                }
            }
        }
    }
}

fn read_layer(record: Record<'_>, name: &str) -> Result<Layer, DxfError> {
    Ok(Layer {
        name: name.into(),
        flags: record.integer(70, 0)?,
        color: record.integer(62, 7)?,
        line_type: record.field(6)?.unwrap_or("CONTINUOUS").into(),
    })
}
