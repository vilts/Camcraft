//! Machine tool-table import and export dispatcher (#2566): one
//! shared model over CamCraft native libraries and machine controller
//! tables (Tormach PathPilot `.csv`, Hurco WinMax `.TCM`); clean-room
//! implementation from public documentation and user-recorded files.

use std::collections::HashMap;
use std::path::Path;

use camcraft_geometry::geometry::units::{MM_PER_INCH, Unit};

use crate::catalog_fields::ToolCatalog;
use crate::cutting::ToolMaterial;
use crate::thread_hand::ThreadHand;
use crate::tools::{Tool, ToolKind};

/// Supported tool-library / controller tool-table formats.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ToolTableFormat {
    /// CamCraft's native JSON envelope (`.camtools` / `.json`).
    CamTools,
    /// Tormach PathPilot mill tool table (`.csv`).
    PathPilot,
    /// Hurco WinMax tool library XML (`.TCM`).
    Hurco,
}

impl ToolTableFormat {
    /// Parse `"auto"` (returns `None`) or an explicit format name.
    pub fn parse_opt(text: &str) -> Result<Option<Self>, String> {
        match text.trim().to_ascii_lowercase().as_str() {
            "" | "auto" => Ok(None),
            "camtools" | "camcraft" | "json" => Ok(Some(Self::CamTools)),
            "pathpilot" | "tormach" | "csv" => Ok(Some(Self::PathPilot)),
            "hurco" | "winmax" | "tcm" => Ok(Some(Self::Hurco)),
            other => Err(format!(
                "unknown tool library format '{other}' (use auto, camtools, pathpilot or hurco)"
            )),
        }
    }

    /// Canonical wire name for reports, MCP, and scripts.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::CamTools => "camtools",
            Self::PathPilot => "pathpilot",
            Self::Hurco => "hurco",
        }
    }
}

/// Detect format from path extension first, then optionally from file content.
#[must_use]
pub fn detect_format(path: &Path, content: Option<&str>) -> Option<ToolTableFormat> {
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        match ext.to_ascii_lowercase().as_str() {
            "csv" => return Some(ToolTableFormat::PathPilot),
            "tcm" => return Some(ToolTableFormat::Hurco),
            "camtools" | "json" => return Some(ToolTableFormat::CamTools),
            _ => {}
        }
    }
    let text = content?.trim_start_matches('\u{FEFF}').trim_start();
    if text.starts_with("Mill Tool Table,") {
        Some(ToolTableFormat::PathPilot)
    } else if text.contains("<Hurco_Document") {
        Some(ToolTableFormat::Hurco)
    } else if text.starts_with('{') || text.starts_with('[') {
        Some(ToolTableFormat::CamTools)
    } else {
        None
    }
}

/// Parse an explicit length unit string (`"mm"` or `"inch"`).
pub fn parse_unit(text: &str) -> Result<Unit, String> {
    match text.trim().to_ascii_lowercase().as_str() {
        "mm" | "millimeter" | "millimeters" | "millimetre" | "millimetres" => Ok(Unit::Millimeters),
        "in" | "inch" | "inches" => Ok(Unit::Inches),
        other => Err(format!("unknown units '{other}' (use mm or inch)")),
    }
}

/// One imported tool row from a controller tool table.
#[derive(Clone, Debug, PartialEq)]
pub struct ToolTableEntry {
    pub number: u32,
    pub name: String,
    pub tool: Tool,
}

/// One tool to write into a controller tool table.
#[derive(Clone, Copy, Debug)]
pub struct ToolTableExportRow<'a> {
    pub number: u32,
    pub name: &'a str,
    pub tool: &'a Tool,
    /// Optional measured holder gauge length (mm) from a modelled body/holder.
    pub gauge_len: Option<f64>,
}

/// Convert a length value from `unit` into millimetres.
#[must_use]
pub(crate) fn to_mm(value: f64, unit: Unit) -> f64 {
    match unit {
        Unit::Millimeters => value,
        Unit::Inches => value * MM_PER_INCH,
    }
}

/// Convert a length value from millimetres into `unit`.
#[must_use]
pub(crate) fn from_mm(value: f64, unit: Unit) -> f64 {
    match unit {
        Unit::Millimeters => value,
        Unit::Inches => value / MM_PER_INCH,
    }
}

/// Build a baseline `Tool` in mm. Required threading pitch stays unknown
/// until the controller tool table supplies it.
#[must_use]
pub(crate) fn base_tool(kind: ToolKind, diameter_mm: f64) -> Tool {
    let flute_len = (diameter_mm * 3.0).max(10.0);
    let shank_len = (flute_len * 1.5).max(15.0);
    let corner_r = match kind {
        ToolKind::BallNose | ToolKind::Probe => diameter_mm * 0.5,
        _ => 0.0,
    };
    let tip_angle = match kind {
        ToolKind::Drill => 118.0,
        ToolKind::Taper => 90.0,
        ToolKind::ThreadMill | ToolKind::ThreadMillSingle => 60.0,
        _ => 0.0,
    };
    let rpm = if kind == ToolKind::Probe { 1.0 } else { 3000.0 };
    let feed = if kind == ToolKind::Probe { 300.0 } else { 500.0 };
    let center_cutting = kind != ToolKind::Probe;
    Tool {
        kind,
        diameter: diameter_mm,
        corner_r,
        tip_angle,
        pitch: 0.0,
        pitch_min: 0.0,
        thread_hand: ThreadHand::Right,
        nose_radius: 0.0,
        width: 0.0,
        flute_len,
        shank_d: diameter_mm,
        shank_len,
        holder_d: (diameter_mm * 2.0).max(20.0),
        holder_len: 35.0,
        material_group: if kind == ToolKind::Probe { String::new() } else { "aluminum".into() },
        rpm,
        feed,
        tool_material: None::<ToolMaterial>,
        flutes: None,
        center_cutting,
        orientation: None,
        catalog: ToolCatalog::default(),
        cutting: Vec::new(),
        outline: Vec::new(),
        turning: None,
        tip_form: Default::default(),
        tip_diameter: 0.0,
        form: None,
    }
}

/// Ensure imported entries have unique names (disambiguating clashes with ` (T<number>)`).
pub(crate) fn disambiguate_names(entries: &mut [ToolTableEntry]) {
    let mut counts: HashMap<String, usize> = HashMap::new();
    for e in entries.iter() {
        *counts.entry(e.name.clone()).or_insert(0) += 1;
    }
    let mut used: HashMap<String, usize> = HashMap::new();
    for e in entries.iter_mut() {
        let base = e.name.clone();
        if counts.get(&base).copied().unwrap_or(0) > 1 {
            let candidate = format!("{base} (T{})", e.number);
            if !counts.contains_key(&candidate) && !used.contains_key(&candidate) {
                e.name = candidate;
            } else {
                let n = used.entry(base.clone()).or_insert(1);
                *n += 1;
                e.name = format!("{base} ({n})");
            }
        }
        used.entry(e.name.clone()).or_insert(1);
    }
}

/// Parse a controller tool-table file (`PathPilot` or `Hurco`).
pub fn parse_controller_table(
    format: ToolTableFormat,
    text: &str,
    unit: Unit,
) -> Result<Vec<ToolTableEntry>, String> {
    let mut entries = match format {
        ToolTableFormat::PathPilot => crate::tool_table_pathpilot::parse(text, unit)?,
        ToolTableFormat::Hurco => crate::tool_table_hurco::parse(text, unit)?,
        ToolTableFormat::CamTools => {
            return Err("internal error: camtools JSON is parsed by app::tool_library".into());
        }
    };
    disambiguate_names(&mut entries);
    Ok(entries)
}

/// Format document tools into a controller tool-table file (`PathPilot` or `Hurco`).
pub fn export_controller_table(
    format: ToolTableFormat,
    rows: &[ToolTableExportRow<'_>],
    unit: Unit,
) -> Result<String, String> {
    match format {
        ToolTableFormat::PathPilot => crate::tool_table_pathpilot::export(rows, unit),
        ToolTableFormat::Hurco => crate::tool_table_hurco::export(rows, unit),
        ToolTableFormat::CamTools => {
            Err("internal error: camtools JSON is exported by app::tool_library".into())
        }
    }
}
