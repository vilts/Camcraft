//! Hurco WinMax tool library (.TCM) XML writer and round-trip guard (#2566);
//! specced from Hurco public WinMax documentation (#30001 ToolType table)
//! and user-recorded .TCM structure.

use std::collections::HashSet;

use camcraft_geometry::geometry::units::Unit;

use crate::tool_table::{ToolTableExportRow, from_mm};
use crate::tool_table_hurco::parse_tool_node;
use crate::tool_table_xml::{escape_xml, parse_xml};
use crate::tools::{Tool, ToolKind};

const ROUNDTRIP_TOL_MM: f64 = 1e-4;
const MAX_HURCO_TOOL_NUMBER: u32 = 99_999;

fn camcraft_to_hurco_type(tool: &Tool) -> Result<i32, String> {
    match tool.kind {
        ToolKind::BullNose | ToolKind::Taper | ToolKind::BallNose if !tool.center_cutting => {
            Err("non-center-cutting tool cannot be exported to a Hurco .TCM tool library".into())
        }
        ToolKind::BullNose => Ok(0),
        ToolKind::Taper => Ok(2),
        ToolKind::Drill => Ok(11),
        ToolKind::Tap if !tool.thread_hand.is_right() => {
            Err("left-hand tap cannot be exported to a Hurco .TCM tool library".into())
        }
        ToolKind::Tap => Ok(12),
        ToolKind::Bore if !tool.nose_radius.is_finite() || tool.nose_radius > 0.0 => Err(format!(
            "boring bar nose radius ({} mm) cannot be exported to a Hurco .TCM tool library; use .camtools",
            fmt_num(tool.nose_radius)
        )),
        ToolKind::Bore => Ok(13),
        ToolKind::EndMill => Ok(if tool.center_cutting { 14 } else { 15 }),
        ToolKind::BallNose => Ok(16),
        ToolKind::Probe => Ok(18),
        ToolKind::ThreadMill => Ok(22),
        ToolKind::ThreadMillSingle => Ok(26),
        other => Err(format!("{other} cannot be exported to a Hurco .TCM tool library")),
    }
}

pub(crate) fn export(rows: &[ToolTableExportRow<'_>], unit: Unit) -> Result<String, String> {
    if rows.is_empty() {
        return Err("the document has no tools to export".into());
    }
    let mut out = String::from("\u{FEFF}<?xml version=\"1.0\" encoding=\"utf-8\"?>\r\n");
    out.push_str("<Hurco_Document Version=\"1.1\">\r\n");
    out.push_str("  <FileInfo>\r\n");
    out.push_str("    <Language>English</Language>\r\n");
    out.push_str("  </FileInfo>\r\n");
    out.push_str("  <Tools>\r\n");
    for row in rows {
        let tool_xml = build_tool_xml(row, unit)?;
        out.push_str(&tool_xml);
    }
    out.push_str("  </Tools>\r\n");
    out.push_str("</Hurco_Document>\r\n");
    Ok(out)
}

fn build_tool_xml(row: &ToolTableExportRow<'_>, unit: Unit) -> Result<String, String> {
    if !(1..=MAX_HURCO_TOOL_NUMBER).contains(&row.number) {
        return Err(format!(
            "tool '{}': tool number T{} is outside Hurco's 1-{MAX_HURCO_TOOL_NUMBER} range",
            row.name, row.number
        ));
    }
    if row.name.trim().is_empty() {
        return Err(format!("tool T{}: Hurco .TCM requires a non-empty tool name", row.number));
    }
    if !row.tool.holder_d.is_finite() || row.tool.holder_d < 0.0 {
        return Err(format!("tool '{}': invalid holder diameter", row.name));
    }
    if !row.tool.holder_len.is_finite() || row.tool.holder_len < 0.0 {
        return Err(format!("tool '{}': invalid holder length", row.name));
    }
    if row.gauge_len.is_some_and(|g| !g.is_finite() || g < 0.0) {
        return Err(format!("tool '{}': invalid gauge length", row.name));
    }
    let type_code = camcraft_to_hurco_type(row.tool)
        .map_err(|why| format!("tool '{}': {why}", row.name))?;
    let id = deterministic_guid(row.number, row.name);
    let dia = from_mm(row.tool.diameter, unit);
    let flute_len = from_mm(row.tool.flute_len, unit);
    let tool_len = from_mm(row.tool.flute_len + row.tool.shank_len, unit);
    let shank_d = from_mm(row.tool.shank_d, unit);
    let gauge_len = row.gauge_len.unwrap_or(row.tool.holder_len).max(0.0);

    let mut tool_xml = format!("    <Tool Id=\"{id}\">\r\n");
    let mut elem = |tag: &str, val: &str| {
        tool_xml.push_str(&format!("      <{tag}>{val}</{tag}>\r\n"));
    };
    elem("Name", &escape_xml(row.name));
    elem("Number", &row.number.to_string());
    elem("Diameter", &fmt_num(dia));
    elem("ToolLength", &fmt_num(tool_len));
    if shank_d > 0.0 {
        elem("ShankDiameter", &fmt_num(shank_d));
    }
    elem("FluteLength", &fmt_num(flute_len));
    if gauge_len > 0.0 {
        elem("AbsoluteGaugeLength", &fmt_num(from_mm(gauge_len, unit)));
    }
    elem("ToolType", &type_code.to_string());
    if let Some(z) = row.tool.flutes {
        elem("NumberOfFlutes", &z.to_string());
    }
    if matches!(row.tool.kind, ToolKind::Tap | ToolKind::ThreadMill | ToolKind::ThreadMillSingle)
        && row.tool.pitch > 0.0
    {
        elem("ThreadPitch", &fmt_num(from_mm(row.tool.pitch, unit)));
    }
    if matches!(
        row.tool.kind,
        ToolKind::Taper | ToolKind::Drill | ToolKind::ThreadMill | ToolKind::ThreadMillSingle
    ) && row.tool.tip_angle > 0.0
    {
        elem("Angle", &fmt_num(row.tool.tip_angle));
    }
    if row.tool.kind == ToolKind::BullNose && row.tool.corner_r > 0.0 {
        elem("TipRadius", &fmt_num(from_mm(row.tool.corner_r, unit)));
    }
    if let Some(vendor) = row.tool.catalog.vendor.as_deref() {
        elem("Manufacturer", &escape_xml(vendor));
    }
    tool_xml.push_str("    </Tool>\r\n");

    let node = parse_xml(&tool_xml).map_err(|e| format!("tool '{}': {e}", row.name))?;
    let mut seen = HashSet::new();
    let back = parse_tool_node(&node, unit, &mut seen)?;
    verify_roundtrip(row, gauge_len, &back.tool)?;
    Ok(tool_xml)
}

fn verify_roundtrip(row: &ToolTableExportRow<'_>, gauge_len: f64, back: &Tool) -> Result<(), String> {
    if (back.holder_d - row.tool.holder_d).abs() > ROUNDTRIP_TOL_MM {
        return Err(format!(
            "tool '{}': Hurco .TCM cannot preserve the {} mm holder diameter (import infers {} mm); use .camtools",
            row.name,
            fmt_num(row.tool.holder_d),
            fmt_num(back.holder_d),
        ));
    }
    if (back.holder_len - gauge_len).abs() > ROUNDTRIP_TOL_MM
        || (row.gauge_len.is_some_and(|g| g <= 0.0) && row.tool.holder_len > ROUNDTRIP_TOL_MM)
    {
        return Err(format!(
            "tool '{}': Hurco .TCM cannot preserve the {} mm holder length; use .camtools",
            row.name,
            fmt_num(row.tool.holder_len),
        ));
    }
    if (back.nose_radius - row.tool.nose_radius).abs() > ROUNDTRIP_TOL_MM {
        return Err(format!(
            "tool '{}': Hurco .TCM cannot preserve the {} mm nose radius; use .camtools",
            row.name,
            fmt_num(row.tool.nose_radius),
        ));
    }
    let tip_ok = match row.tool.kind {
        ToolKind::BullNose => (back.corner_r - row.tool.corner_r).abs() <= ROUNDTRIP_TOL_MM,
        ToolKind::Taper | ToolKind::Drill | ToolKind::ThreadMill | ToolKind::ThreadMillSingle => {
            (back.tip_angle - row.tool.tip_angle).abs() <= ROUNDTRIP_TOL_MM
        }
        _ => true,
    };
    let pitch_ok = match row.tool.kind {
        ToolKind::Tap | ToolKind::ThreadMill => {
            (back.pitch - row.tool.pitch).abs() <= ROUNDTRIP_TOL_MM
        }
        ToolKind::ThreadMillSingle => {
            (back.pitch - row.tool.pitch).abs() <= ROUNDTRIP_TOL_MM
                && (back.pitch_min - row.tool.pitch_min).abs() <= ROUNDTRIP_TOL_MM
        }
        _ => true,
    };
    let cc_ok = !matches!(
        row.tool.kind,
        ToolKind::EndMill | ToolKind::BullNose | ToolKind::Taper | ToolKind::BallNose | ToolKind::Probe
    ) || back.center_cutting == row.tool.center_cutting;
    let shank_d_ok = row.tool.shank_d <= 0.0 || (back.shank_d - row.tool.shank_d).abs() <= ROUNDTRIP_TOL_MM;
    if back.kind != row.tool.kind
        || !cc_ok
        || (row.tool.kind == ToolKind::Tap && back.thread_hand != row.tool.thread_hand)
        || !tip_ok
        || !pitch_ok
        || !shank_d_ok
        || (back.diameter - row.tool.diameter).abs() > ROUNDTRIP_TOL_MM
        || (back.flute_len - row.tool.flute_len).abs() > ROUNDTRIP_TOL_MM
        || (back.shank_len - row.tool.shank_len).abs() > ROUNDTRIP_TOL_MM
    {
        return Err(format!(
            "tool '{}': Hurco .TCM cannot preserve the cutter geometry; use .camtools",
            row.name
        ));
    }
    Ok(())
}

fn fmt_num(v: f64) -> String {
    let rounded = (v * 1_000_000.0).round() / 1_000_000.0;
    let s = format!("{rounded}");
    if s == "-0" { "0".into() } else { s }
}

fn deterministic_guid(number: u32, name: &str) -> String {
    let mut a: u64 = 0xcbf2_9ce4_8422_2325;
    let mut b: u64 = 0xaf63_bd4c_8601_b7df;
    for byte in number.to_le_bytes().iter().chain(name.as_bytes()) {
        a ^= u64::from(*byte);
        a = a.wrapping_mul(0x100_0000_01b3);
        b ^= u64::from(*byte).wrapping_add(0x9e);
        b = b.wrapping_mul(0x100_0000_01b3);
    }
    let b1 = (a >> 32) as u32;
    let b2 = ((a >> 16) & 0xffff) as u16;
    let b3 = (((a & 0xffff) as u16) & 0x0fff) | 0x4000;
    let b4 = (((b >> 48) as u16) & 0x3fff) | 0x8000;
    let b5 = b & 0x0000_ffff_ffff_ffff;
    format!("{b1:08x}-{b2:04x}-{b3:04x}-{b4:04x}-{b5:012x}")
}
