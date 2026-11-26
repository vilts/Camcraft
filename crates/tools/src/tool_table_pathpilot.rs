//! Tormach PathPilot mill tool-table CSV reader and writer (#2566);
//! specced from Tormach 770MX manual UM10587 §7.6.4 and recorded PathPilot
//! export column layout.

use std::collections::HashSet;

use camcraft_geometry::geometry::units::Unit;

use crate::tool_table::{ToolTableEntry, ToolTableExportRow, from_mm, to_mm};
use crate::tool_table_pathpilot_desc::tool_from_description;
use crate::tools::ToolKind;

/// PathPilot mill tool table accepts tool numbers 1-1000.
pub const MAX_TOOL_NUMBER: u32 = 1000;

const ROW1_PREFIX: &str = "Mill Tool Table";
const ROW1_MARKER: &str = "This must be the first row - do not edit";
const EXPORT_ROW1: &str =
    "Mill Tool Table,2026-11-18 00:00:00 - v1,This must be the first row - do not edit";
const HEADER_ROW: &str =
    "Tool number,Description,Z offset,Diameter,Z offset wear,Diameter wear,Max RPM";

struct Columns {
    count: usize,
    number: usize,
    description: usize,
    diameter: usize,
    z_offset: Option<usize>,
    z_wear: Option<usize>,
    d_wear: Option<usize>,
    max_rpm: Option<usize>,
}

pub(crate) fn parse(text: &str, unit: Unit) -> Result<Vec<ToolTableEntry>, String> {
    let clean = text.strip_prefix('\u{FEFF}').unwrap_or(text);
    let mut raw_lines: Vec<(usize, &str)> = Vec::new();
    for (idx, line) in clean.lines().enumerate() {
        let trimmed = line.trim_end_matches('\r');
        if !trimmed.is_empty() {
            raw_lines.push((idx + 1, trimmed));
        }
    }
    if raw_lines.len() < 2 {
        return Err(
            "not a PathPilot mill tool table: expected the marker row and column header row".into(),
        );
    }
    let row1 = parse_csv_row(raw_lines[0].1).map_err(|e| format!("PathPilot row 1: {e}"))?;
    if row1.first().map(|s| s.trim()) != Some(ROW1_PREFIX)
        || row1.get(2).map(|s| s.trim()) != Some(ROW1_MARKER)
    {
        return Err(
            "not a PathPilot mill tool table: row 1 must start with 'Mill Tool Table' and end with 'This must be the first row - do not edit'".into(),
        );
    }
    let header = parse_csv_row(raw_lines[1].1)
        .map_err(|e| format!("PathPilot row {}: {e}", raw_lines[1].0))?;
    let cols = locate_columns(&header)?;

    let mut seen_numbers: HashSet<u32> = HashSet::new();
    let mut entries: Vec<ToolTableEntry> = Vec::new();
    for &(line_no, raw) in &raw_lines[2..] {
        let fields = parse_csv_row(raw).map_err(|e| format!("PathPilot row {line_no}: {e}"))?;
        if fields.len() < cols.count {
            return Err(format!(
                "PathPilot row {line_no}: expected {expected} columns, got {got}",
                expected = cols.count,
                got = fields.len()
            ));
        }
        if let Some(entry) = parse_data_row(line_no, &fields, &cols, unit, &mut seen_numbers)? {
            entries.push(entry);
        }
    }
    if entries.is_empty() {
        return Err("the PathPilot tool table holds no tools".into());
    }
    Ok(entries)
}

fn locate_columns(header: &[String]) -> Result<Columns, String> {
    let find = |want: &str| {
        header
            .iter()
            .position(|h| h.trim().eq_ignore_ascii_case(want))
    };
    let number = find("Tool number")
        .ok_or("not a PathPilot mill tool table: missing 'Tool number' column")?;
    let description = find("Description")
        .ok_or("not a PathPilot mill tool table: missing 'Description' column")?;
    let diameter = find("Diameter")
        .ok_or("not a PathPilot mill tool table: missing 'Diameter' column")?;
    Ok(Columns {
        count: header.len().max(number + 1).max(description + 1).max(diameter + 1),
        number,
        description,
        diameter,
        z_offset: find("Z offset"),
        z_wear: find("Z offset wear"),
        d_wear: find("Diameter wear"),
        max_rpm: find("Max RPM"),
    })
}

fn parse_data_row(
    line_no: usize,
    fields: &[String],
    cols: &Columns,
    unit: Unit,
    seen_numbers: &mut HashSet<u32>,
) -> Result<Option<ToolTableEntry>, String> {
    let num_raw = fields[cols.number].trim();
    let number: u32 = num_raw
        .parse()
        .map_err(|_| format!("PathPilot row {line_no}: invalid tool number '{num_raw}'"))?;
    if !(1..=MAX_TOOL_NUMBER).contains(&number) {
        return Err(format!(
            "PathPilot row {line_no}: tool number T{number} is outside 1-{MAX_TOOL_NUMBER}"
        ));
    }
    if !seen_numbers.insert(number) {
        return Err(format!("PathPilot row {line_no}: tool number T{number} appears twice"));
    }

    let desc = fields[cols.description].trim();
    let label = if desc.is_empty() { format!("T{number}") } else { format!("T{number} ('{desc}')") };
    let dia_raw = fields[cols.diameter].trim();
    let dia_val = parse_finite_f64(dia_raw, "Diameter", &label, line_no)?;
    if dia_val < 0.0 {
        return Err(format!("PathPilot row {line_no} tool {label}: negative diameter {dia_val}"));
    }
    for (name, idx) in [
        ("Z offset", cols.z_offset),
        ("Z offset wear", cols.z_wear),
        ("Diameter wear", cols.d_wear),
        ("Max RPM", cols.max_rpm),
    ] {
        if let Some(i) = idx {
            let v = fields[i].trim();
            if !v.is_empty() {
                parse_finite_f64(v, name, &label, line_no)?;
            }
        }
    }
    if desc.is_empty() && dia_val == 0.0 {
        return Ok(None);
    }
    if dia_val == 0.0 {
        return Err(format!("PathPilot row {line_no} tool {label}: diameter must be over 0"));
    }
    let dia_mm = to_mm(dia_val, unit);
    let max_rpm = cols
        .max_rpm
        .and_then(|i| fields[i].trim().parse::<f64>().ok())
        .unwrap_or(0.0);

    let name = if desc.is_empty() { format!("Tool T{number}") } else { desc.to_owned() };
    let tool = tool_from_description(&name, dia_mm, max_rpm, unit, line_no)?;
    if let Some(why) = tool.why_not_sane() {
        return Err(format!("PathPilot row {line_no} tool '{name}': {why}"));
    }
    Ok(Some(ToolTableEntry { number, name, tool }))
}

fn parse_finite_f64(raw: &str, col: &str, label: &str, line_no: usize) -> Result<f64, String> {
    let val: f64 = raw
        .parse()
        .map_err(|_| format!("PathPilot row {line_no} tool {label}: invalid {col} '{raw}'"))?;
    if !val.is_finite() {
        return Err(format!("PathPilot row {line_no} tool {label}: non-finite {col} '{raw}'"));
    }
    Ok(val)
}

pub(crate) fn export(rows: &[ToolTableExportRow<'_>], unit: Unit) -> Result<String, String> {
    if rows.is_empty() {
        return Err("the document has no tools to export".into());
    }
    let mut out = String::new();
    out.push_str(EXPORT_ROW1);
    out.push_str("\r\n");
    out.push_str(HEADER_ROW);
    out.push_str("\r\n");
    for row in rows {
        if !is_mill_table_kind(row.tool.kind) {
            return Err(format!(
                "tool '{}': {} cannot be exported to a PathPilot mill tool table",
                row.name, row.tool.kind
            ));
        }
        if !(1..=MAX_TOOL_NUMBER).contains(&row.number) {
            return Err(format!(
                "tool '{}': tool number T{} exceeds PathPilot's 1-{MAX_TOOL_NUMBER} tool table",
                row.name, row.number
            ));
        }
        let dia = from_mm(row.tool.diameter, unit);
        // Refuse a lossy description: the reader uses it to reconstruct cutter geometry.
        let written_dia: f64 = format!("{dia:.7}")
            .parse()
            .map_err(|_| format!("tool '{}': invalid PathPilot diameter", row.name))?;
        let back = tool_from_description(row.name, to_mm(written_dia, unit), 0.0, unit, 0)?;
        if !same_geometry(row.tool, &back) {
            return Err(format!(
                "tool '{}': PathPilot description cannot preserve the cutter geometry; use .camtools",
                row.name
            ));
        }
        let desc = csv_escape(row.name);
        out.push_str(&format!(
            "{},{},0.0000000,{:.7},0.0000000,0.0000000,0.0\r\n",
            row.number, desc, dia
        ));
    }
    Ok(out)
}

fn same_geometry(a: &crate::tools::Tool, b: &crate::tools::Tool) -> bool {
    let lengths = [
        (a.diameter, b.diameter),
        (a.corner_r, b.corner_r),
        (a.tip_angle, b.tip_angle),
        (a.pitch, b.pitch),
        (a.pitch_min, b.pitch_min),
        (a.nose_radius, b.nose_radius),
        (a.width, b.width),
        (a.flute_len, b.flute_len),
        (a.shank_d, b.shank_d),
        (a.shank_len, b.shank_len),
        (a.holder_d, b.holder_d),
        (a.holder_len, b.holder_len),
    ];
    a.kind == b.kind
        && a.center_cutting == b.center_cutting
        && a.thread_hand == b.thread_hand
        && a.orientation == b.orientation
        && a.outline == b.outline
        && a.turning == b.turning
        && lengths.iter().all(|(a, b)| a.is_finite() && b.is_finite() && (a - b).abs() <= 1e-6)
}

fn is_mill_table_kind(kind: ToolKind) -> bool {
    matches!(
        kind,
        ToolKind::EndMill
            | ToolKind::BallNose
            | ToolKind::BullNose
            | ToolKind::Drill
            | ToolKind::Tap
            | ToolKind::Bore
            | ToolKind::Taper
            | ToolKind::Probe
            | ToolKind::ThreadMill
            | ToolKind::ThreadMillSingle
    )
}

fn csv_escape(field: &str) -> String {
    if field.contains([',', '"', '\r', '\n']) {
        let mut s = String::with_capacity(field.len() + 2);
        s.push('"');
        for ch in field.chars() {
            if ch == '"' {
                s.push_str("\"\"");
            } else {
                s.push(ch);
            }
        }
        s.push('"');
        s
    } else {
        field.to_owned()
    }
}

fn parse_csv_row(line: &str) -> Result<Vec<String>, String> {
    let mut fields = Vec::new();
    let mut cur = String::new();
    let mut chars = line.chars().peekable();
    let mut in_quotes = false;
    let mut field_start = true;
    while let Some(ch) = chars.next() {
        if in_quotes {
            if ch == '"' {
                if chars.peek() == Some(&'"') {
                    chars.next();
                    cur.push('"');
                } else {
                    in_quotes = false;
                }
            } else {
                cur.push(ch);
            }
            field_start = false;
        } else if ch == '"' && field_start {
            in_quotes = true;
            field_start = false;
        } else if ch == ',' {
            fields.push(std::mem::take(&mut cur));
            field_start = true;
        } else {
            cur.push(ch);
            field_start = false;
        }
    }
    if in_quotes {
        return Err("unclosed quote in CSV row".into());
    }
    fields.push(cur);
    Ok(fields)
}
