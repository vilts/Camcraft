//! Wire EDM wire tool record and validation (#2011).
//! Provenance: public wire EDM wire diameters (0.02-1.0 mm, matching `edm_conditions`); our own `Tool` field mapping.

use crate::tools::{Tool, ToolKind};

/// Thinnest wire diameter an EDM wire tool accepts, mm.
pub const MIN_WIRE_MM: f64 = 0.02;

/// Thickest wire diameter an EDM wire tool accepts, mm.
pub const MAX_WIRE_MM: f64 = 1.0;

/// Build a standard brass EDM wire tool of `diameter` mm (`rpm = 0`, no corner radius).
#[must_use]
pub fn wire_tool(diameter: f64) -> Tool {
    Tool {
        kind: ToolKind::Wire,
        diameter,
        corner_r: 0.0,
        tip_angle: 0.0,
        pitch: 0.0,
        pitch_min: 0.0,
        thread_hand: Default::default(),
        nose_radius: 0.0,
        width: 0.0,
        flute_len: 100.0,
        shank_d: diameter,
        shank_len: 20.0,
        holder_d: 0.0,
        holder_len: 0.0,
        material_group: "brass".into(),
        rpm: 0.0,
        feed: 50.0,
        tool_material: None,
        flutes: None,
        center_cutting: true,
        orientation: None,
        turning: None,
        catalog: crate::catalog_fields::ToolCatalog::default(),
        cutting: Vec::new(),
        outline: Vec::new(),
        tip_form: Default::default(),
        tip_diameter: 0.0,
        form: None,
    }
}

/// Named refusal for a wire tool at command, catalog, and library doors.
#[must_use]
pub fn refusal(tool: &Tool) -> Option<String> {
    if tool.kind != ToolKind::Wire {
        return None;
    }
    if tool.rpm != 0.0 {
        return Some(format!(
            "an EDM wire turns no spindle - its rpm must be 0 (it states {})",
            tool.rpm
        ));
    }
    if !(tool.diameter.is_finite() && (MIN_WIRE_MM..=MAX_WIRE_MM).contains(&tool.diameter)) {
        return Some(format!(
            "wire diameter {} mm is outside [{MIN_WIRE_MM}, {MAX_WIRE_MM}] mm",
            tool.diameter
        ));
    }
    if tool.corner_r != 0.0 {
        return Some(format!(
            "an EDM wire has no corner radius (it states {}) - its diameter is the wire diameter",
            tool.corner_r
        ));
    }
    if !tool.cutting.is_empty() {
        return Some("an EDM wire uses machine EDM conditions, not per-material cutting data rows".into());
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cutting::WorkMaterial;
    use crate::cutting_rows::CuttingRow;

    #[test]
    fn wire_tool_is_sane_and_refuses_invalid_inputs_by_name() {
        let w = wire_tool(0.25);
        assert!(w.is_sane());
        assert_eq!(w.torch_refusal(), None);

        let spun = Tool { rpm: 100.0, ..w.clone() };
        assert!(!spun.is_sane());
        assert!(spun.torch_refusal().is_some_and(|s| s.contains("rpm must be 0")));

        let too_thin = Tool { diameter: 0.01, ..w.clone() };
        assert!(!too_thin.is_sane());
        assert!(too_thin.torch_refusal().is_some_and(|s| s.contains("wire diameter")));

        let too_thick = Tool { diameter: 1.5, ..w.clone() };
        assert!(!too_thick.is_sane());
        assert!(too_thick.torch_refusal().is_some_and(|s| s.contains("wire diameter")));

        let round = Tool { corner_r: 0.05, ..w.clone() };
        assert!(!round.is_sane());
        assert!(round.torch_refusal().is_some_and(|s| s.contains("corner radius")));

        let row = Tool {
            cutting: vec![CuttingRow::plain(Some(WorkMaterial::MildSteel), 0.0, 100.0)],
            ..w
        };
        assert!(!row.is_sane());
        assert!(row.torch_refusal().is_some_and(|s| s.contains("EDM conditions")));
    }
}
