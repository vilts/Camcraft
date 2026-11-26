//! Hurco WinMax tool library (.TCM) XML reader and writer (#2566);
//! specced from Hurco public WinMax documentation (#30001 ToolType table)
//! and user-recorded .TCM structure.

use std::collections::HashSet;

use camcraft_geometry::geometry::units::Unit;

use crate::catalog_fields::MAX_SHORT;
use crate::tool_table::{ToolTableEntry, base_tool, to_mm};
pub(crate) use crate::tool_table_hurco_export::export;
use crate::tool_table_xml::{XmlNode, parse_xml};
use crate::tools::{Tool, ToolKind};

/// Strip optional `Tool.` / `FileInfo.` / `SpeedFeed.` prefixes so both
/// `<Name>` and `<Tool.Name>` shapes read identically.
fn field_name(tag: &str) -> &str {
    tag.split_once('.').map_or(tag, |(_, rest)| rest)
}

fn find_field<'a>(node: &'a XmlNode, wanted: &str) -> Option<&'a XmlNode> {
    node.children.iter().find(|c| field_name(&c.tag) == wanted)
}

fn text_of(node: &XmlNode, wanted: &str) -> Option<String> {
    find_field(node, wanted)
        .map(|n| n.text.trim().to_owned())
        .filter(|s| !s.is_empty())
}

pub(crate) fn parse(text: &str, unit: Unit) -> Result<Vec<ToolTableEntry>, String> {
    let root = parse_xml(text).map_err(|e| format!("not a Hurco WinMax .TCM file: {e}"))?;
    if root.tag != "Hurco_Document" {
        return Err(format!(
            "not a Hurco WinMax .TCM file: root element is '<{}>', expected 'Hurco_Document'",
            root.tag
        ));
    }
    let tools_node = root
        .child("Tools")
        .ok_or("not a Hurco WinMax .TCM file: missing <Tools> element")?;
    let mut seen_numbers: HashSet<u32> = HashSet::new();
    let mut entries: Vec<ToolTableEntry> = Vec::new();
    for child in &tools_node.children {
        if child.tag != "Tool" {
            continue;
        }
        entries.push(parse_tool_node(child, unit, &mut seen_numbers)?);
    }
    if entries.is_empty() {
        return Err("the Hurco .TCM file holds no tools".into());
    }
    Ok(entries)
}

pub(crate) fn parse_tool_node(
    node: &XmlNode,
    unit: Unit,
    seen_numbers: &mut HashSet<u32>,
) -> Result<ToolTableEntry, String> {
    let num_str = text_of(node, "Number")
        .ok_or("Hurco .TCM tool entry is missing <Number>")?;
    let number: u32 = num_str
        .parse()
        .map_err(|_| format!("Hurco .TCM tool entry has invalid <Number> '{num_str}'"))?;
    if number == 0 || number > 99_999 {
        return Err(format!(
            "Hurco .TCM tool T{number}: number is outside 1-99999"
        ));
    }
    if !seen_numbers.insert(number) {
        return Err(format!("Hurco .TCM tool number T{number} appears twice"));
    }

    let name = text_of(node, "Name")
        .ok_or_else(|| format!("Hurco .TCM tool T{number} has no <Name>"))?;

    let type_str = text_of(node, "ToolType")
        .ok_or_else(|| format!("tool '{name}' (T{number}): missing Hurco <ToolType>"))?;
    let type_code: i32 = type_str.parse().map_err(|_| {
        format!("tool '{name}' (T{number}): invalid Hurco ToolType '{type_str}'")
    })?;
    let (kind, center_cutting, default_spot_angle) = map_hurco_type(type_code, &name, number)?;

    let dia_val = req_f64(node, "Diameter", &name, number)?;
    if dia_val <= 0.0 {
        return Err(format!(
            "tool '{name}' (T{number}): diameter {dia_val} must be over 0"
        ));
    }
    let dia_mm = to_mm(dia_val, unit);
    let mut tool = base_tool(kind, dia_mm);
    tool.center_cutting = center_cutting;
    if let Some(angle) = default_spot_angle {
        tool.tip_angle = angle;
    }

    if let Some(shank_val) = opt_f64(node, "ShankDiameter", &name, number)?
        && shank_val > 0.0
    {
        tool.shank_d = to_mm(shank_val, unit);
    }
    let tool_len_mm = opt_f64(node, "ToolLength", &name, number)?
        .filter(|&v| v > 0.0)
        .map(|v| to_mm(v, unit));
    let flute_len_mm = opt_f64(node, "FluteLength", &name, number)?
        .filter(|&v| v > 0.0)
        .map(|v| to_mm(v, unit));

    if let Some(fl) = flute_len_mm {
        tool.flute_len = fl;
        tool.shank_len = tool_len_mm
            .map_or((fl * 1.5).max(15.0), |tl| (tl - fl).max(0.0));
    } else if let Some(tl) = tool_len_mm {
        tool.flute_len = tl;
        tool.shank_len = 0.0;
    }
    if kind == ToolKind::Probe && tool.flute_len < dia_mm {
        tool.flute_len = dia_mm;
    }

    if let Some(abs_gauge) = opt_f64(node, "AbsoluteGaugeLength", &name, number)?
        && abs_gauge > 0.0
    {
        tool.holder_len = to_mm(abs_gauge, unit);
        tool.holder_d = (tool.shank_d * 2.0).max(20.0);
    } else {
        tool.holder_len = 0.0;
        tool.holder_d = 0.0;
    }

    if let Some(flutes_str) = text_of(node, "NumberOfFlutes") {
        let z: u32 = flutes_str.parse().map_err(|_| {
            format!("tool '{name}' (T{number}): invalid NumberOfFlutes '{flutes_str}'")
        })?;
        if (1..=32).contains(&z) {
            tool.flutes = Some(z);
        }
    }
    if kind == ToolKind::BullNose
        && let Some(r_val) = opt_f64(node, "TipRadius", &name, number)?
        && r_val >= 0.0
    {
        tool.corner_r = to_mm(r_val, unit);
    }
    if let Some(angle_val) = opt_f64(node, "Angle", &name, number)?
        && angle_val > 0.0
        && matches!(
            kind,
            ToolKind::Taper
                | ToolKind::Drill
                | ToolKind::ThreadMill
                | ToolKind::ThreadMillSingle
        )
    {
        tool.tip_angle = angle_val;
    }
    if let Some(pitch_val) = opt_f64(node, "ThreadPitch", &name, number)?
        && pitch_val > 0.0
        && matches!(
            kind,
            ToolKind::Tap | ToolKind::ThreadMill | ToolKind::ThreadMillSingle
        )
    {
        let pitch_mm = to_mm(pitch_val, unit);
        tool.pitch = pitch_mm;
        if kind == ToolKind::ThreadMillSingle {
            tool.pitch_min = pitch_mm;
        }
        if kind == ToolKind::ThreadMill && tool.flute_len < pitch_mm {
            tool.flute_len = pitch_mm;
        }
    }
    if let Some(vendor) = text_of(node, "Manufacturer") {
        let trimmed: String = vendor.trim().chars().take(MAX_SHORT).collect();
        if !trimmed.is_empty() {
            tool.catalog.vendor = Some(trimmed);
        }
    }
    apply_speed_feeds(node, &mut tool, unit);

    if let Some(why) = tool.why_not_sane() {
        return Err(format!("tool '{name}' (T{number}): {why}"));
    }
    Ok(ToolTableEntry { number, name, tool })
}

fn req_f64(node: &XmlNode, field: &str, name: &str, number: u32) -> Result<f64, String> {
    let raw = text_of(node, field)
        .ok_or_else(|| format!("tool '{name}' (T{number}): missing <{field}>"))?;
    let v: f64 = raw.parse().map_err(|_| {
        format!("tool '{name}' (T{number}): invalid <{field}> '{raw}'")
    })?;
    if !v.is_finite() {
        return Err(format!("tool '{name}' (T{number}): non-finite <{field}>"));
    }
    Ok(v)
}

fn opt_f64(node: &XmlNode, field: &str, name: &str, number: u32) -> Result<Option<f64>, String> {
    let Some(raw) = text_of(node, field) else {
        return Ok(None);
    };
    let v: f64 = raw.parse().map_err(|_| {
        format!("tool '{name}' (T{number}): invalid <{field}> '{raw}'")
    })?;
    if !v.is_finite() {
        return Err(format!("tool '{name}' (T{number}): non-finite <{field}>"));
    }
    Ok(Some(v))
}

fn apply_speed_feeds(node: &XmlNode, tool: &mut Tool, unit: Unit) {
    let Some(sfs) = find_field(node, "SpeedFeeds") else { return };
    let (mut got_rpm, mut got_feed) = (false, false);
    for sf in &sfs.children {
        if !got_rpm
            && let Some(rpm) = text_of(sf, "Speed").and_then(|s| s.parse::<f64>().ok())
            && rpm.is_finite()
            && rpm > 0.0
        {
            tool.rpm = rpm;
            got_rpm = true;
        }
        if !got_feed {
            for key in ["FeedRate", "FeedPerMinute", "Feed"] {
                if let Some(f) = text_of(sf, key).and_then(|s| s.parse::<f64>().ok())
                    && f.is_finite()
                    && f > 0.0
                {
                    tool.feed = to_mm(f, unit);
                    got_feed = true;
                    break;
                }
            }
        }
        if got_rpm && got_feed {
            break;
        }
    }
}

/// Map Hurco WinMax `#30001` ToolType codes to CamCraft `ToolKind`; refuse
/// every Hurco type that CamCraft has no kind for by name, never guessing.
fn map_hurco_type(
    code: i32,
    name: &str,
    number: u32,
) -> Result<(ToolKind, bool, Option<f64>), String> {
    match code {
        0 => Ok((ToolKind::BullNose, true, None)),
        2 => Ok((ToolKind::Taper, true, None)),
        7 => Ok((ToolKind::Taper, true, Some(60.0))),
        11 => Ok((ToolKind::Drill, true, None)),
        12 => Ok((ToolKind::Tap, true, None)),
        13 => Ok((ToolKind::Bore, true, None)),
        14 => Ok((ToolKind::EndMill, true, None)),
        15 => Ok((ToolKind::EndMill, false, None)),
        16 => Ok((ToolKind::BallNose, true, None)),
        18 => Ok((ToolKind::Probe, false, None)),
        20 => Ok((ToolKind::Drill, true, Some(90.0))),
        21 => Ok((ToolKind::Tap, true, None)),
        22 => Ok((ToolKind::ThreadMill, true, None)),
        26 => Ok((ToolKind::ThreadMillSingle, true, None)),
        other => {
            let label = hurco_unsupported_label(other);
            Err(format!(
                "tool '{name}' (T{number}): Hurco tool type {other} ({label}) has no CamCraft tool kind"
            ))
        }
    }
}

fn hurco_unsupported_label(code: i32) -> &'static str {
    match code {
        1 => "Center Drill",
        3 => "Corner Round Mill",
        4 => "Counter Bore",
        5 => "Counter Sink",
        6 => "Dove Tail Mill",
        8 => "Keyseat Mill",
        9 => "Ream",
        10 => "Custom Tool",
        17 => "Back Spotface",
        19 => "Gun Drill",
        23 => "Taper Radius End Mill",
        _ => "unsupported Hurco tool type",
    }
}
