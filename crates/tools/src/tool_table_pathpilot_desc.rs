//! PathPilot tool-description parser (#2566): extracts optional tool
//! kind, corner radius `R:`, length of cut `loc:`, flute count `<n>FL`,
//! tool material, and Tormach `[part]` number from a PathPilot CSV
//! Description string.

use camcraft_geometry::geometry::units::Unit;

use crate::catalog_fields::MAX_SHORT;
use crate::cutting::ToolMaterial;
use crate::tool_table::{base_tool, to_mm};
use crate::tools::{Tool, ToolKind};

pub(crate) fn tool_from_description(
    desc: &str,
    diameter_mm: f64,
    max_rpm: f64,
    unit: Unit,
    line_no: usize,
) -> Result<Tool, String> {
    let lower = desc.to_ascii_lowercase();
    if lower.contains("slitting saw") {
        return Err(format!("PathPilot row {line_no} tool '{desc}': unsupported slitting saw geometry"));
    }
    let (mut kind, center_cutting) = infer_kind(&lower);
    let mut tool = base_tool(kind, diameter_mm);
    tool.center_cutting = center_cutting;

    // Negative Max RPM (such as -2.0 on T99 probes) is a PathPilot marker, not a speed.
    if max_rpm.is_finite() && max_rpm > 0.0 {
        tool.rpm = max_rpm;
    }
    if let Some(flutes) = parse_flutes(&lower) {
        tool.flutes = Some(flutes);
    }
    if lower.contains("carbide") {
        tool.tool_material = Some(ToolMaterial::Carbide);
    } else if lower.contains("hss") {
        tool.tool_material = Some(ToolMaterial::Hss);
    }
    if let Some(part) = parse_part_number(desc) {
        tool.catalog.part_number = Some(part);
    }
    if let Some(loc_val) = parse_tagged_number(&lower, "loc:") {
        let loc_mm = to_mm(loc_val, unit);
        if loc_mm < 0.0 {
            return Err(format!("PathPilot row {line_no} tool '{desc}': negative loc:{loc_val}"));
        }
        if loc_mm > 0.0 {
            tool.flute_len = loc_mm;
            tool.shank_len = (loc_mm * 1.5).max(15.0);
        }
    }
    if let Some(r_val) = parse_tagged_number(&lower, "r:") {
        let r_mm = to_mm(r_val, unit);
        if r_mm < 0.0 {
            return Err(format!("PathPilot row {line_no} tool '{desc}': negative R:{r_val}"));
        }
        if r_mm > 0.0 {
            if kind == ToolKind::EndMill {
                kind = ToolKind::BullNose;
                tool.kind = ToolKind::BullNose;
            }
            if matches!(kind, ToolKind::BullNose | ToolKind::BallNose) {
                tool.corner_r = r_mm;
            }
        }
    }
    if kind == ToolKind::Probe && tool.flute_len < diameter_mm {
        tool.flute_len = diameter_mm;
    }
    Ok(tool)
}

fn infer_kind(lower: &str) -> (ToolKind, bool) {
    if lower.contains("probe") {
        (ToolKind::Probe, false)
    } else if lower.contains("ball") {
        (ToolKind::BallNose, true)
    } else if lower.contains("bull") {
        (ToolKind::BullNose, true)
    } else if lower.contains("chamfer")
        || lower.contains("engrav")
        || lower.contains("v-bit")
        || lower.contains("vbit")
    {
        (ToolKind::Taper, true)
    } else if lower.contains("thread mill") || lower.contains("threadmill") {
        if lower.contains("single") {
            (ToolKind::ThreadMillSingle, true)
        } else {
            (ToolKind::ThreadMill, true)
        }
    } else if has_word(lower, "tap") {
        (ToolKind::Tap, true)
    } else if lower.contains("boring") || has_word(lower, "bore") {
        (ToolKind::Bore, true)
    } else if lower.contains("drill") {
        (ToolKind::Drill, true)
    } else if lower.contains("face mill")
        || lower.contains("facemill")
        || lower.contains("shell mill")
    {
        (ToolKind::EndMill, false)
    } else {
        (ToolKind::EndMill, true)
    }
}

fn has_word(haystack: &str, word: &str) -> bool {
    haystack
        .split(|c: char| !c.is_ascii_alphanumeric())
        .any(|tok| tok == word)
}

fn parse_tagged_number(lower: &str, tag: &str) -> Option<f64> {
    let mut start = 0;
    while let Some(pos) = lower[start..].find(tag) {
        let abs = start + pos;
        let boundary_ok = abs == 0
            || lower[..abs]
                .chars()
                .next_back()
                .is_some_and(|c| c.is_whitespace() || c == ',');
        let after = &lower[abs + tag.len()..];
        if boundary_ok {
            let num_str: String = after
                .chars()
                .take_while(|c| c.is_ascii_digit() || matches!(c, '.' | '-' | '+'))
                .collect();
            if let Ok(v) = num_str.parse::<f64>()
                && v.is_finite()
            {
                return Some(v);
            }
        }
        start = abs + tag.len();
    }
    None
}

fn parse_flutes(lower: &str) -> Option<u32> {
    for tok in lower.split_whitespace() {
        let clean = tok.trim_matches(|c: char| !c.is_ascii_alphanumeric());
        if let Some(num) = clean.strip_suffix("fl")
            && !num.is_empty()
            && num.chars().all(|c| c.is_ascii_digit())
            && let Ok(n) = num.parse::<u32>()
            && (1..=32).contains(&n)
        {
            return Some(n);
        }
    }
    None
}

fn parse_part_number(desc: &str) -> Option<String> {
    let mut last: Option<String> = None;
    let mut rest = desc;
    while let Some(open) = rest.find('[') {
        let after = &rest[open + 1..];
        let Some(close) = after.find(']') else { break };
        let inner = after[..close].trim();
        if !inner.is_empty()
            && inner.len() <= MAX_SHORT
            && inner.chars().all(|c| c.is_ascii_digit())
        {
            last = Some(inner.to_owned());
        }
        rest = &after[close + 1..];
    }
    last
}
