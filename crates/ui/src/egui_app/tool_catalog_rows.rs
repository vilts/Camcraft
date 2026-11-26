//! The tool editor's catalog rows (#898): part number, vendor, coating,
//! helix angle and notes - label-left rows of the settings grid (#2014).
//! Draft fields only; Apply / Add carry them through `ChangeTool` /
//! `AddTool` like every other tool number, so one edit is one undo step
//! and a bad value is refused by name there.
//!
//! Provenance: our own widget over the tool model's catalog fields.

use egui::{TextEdit, Ui};

use crate::state::ToolUi;

use super::layout::{CTRL_W, prop_row, row_label_id, text_row};

/// The rows, in the Catalog group of the settings grid.
pub(super) fn catalog_rows(ui: &mut Ui, t: &mut ToolUi) {
    text_row(ui, "Part number", &mut t.part_number, "tool_part_number", CTRL_W * 1.3);
    text_row(ui, "Vendor", &mut t.vendor, "tool_vendor", CTRL_W * 1.3);
    text_row(ui, "Coating", &mut t.coating, "tool_coating", CTRL_W * 1.3);
    text_row(ui, "Helix angle (deg)", &mut t.helix_deg, "tool_helix", CTRL_W);
    prop_row(ui, "Notes", |ui| {
        let mut r = ui
            .add(TextEdit::multiline(&mut t.notes).id_salt("tool_notes").desired_width(CTRL_W * 1.3).desired_rows(2));
        if let Some(l) = row_label_id(ui) {
            r = r.labelled_by(l);
        }
        r
    });
}

#[cfg(test)]
mod tests {
    use crate::state::ToolUi;

    #[test]
    fn the_draft_carries_the_facts_both_ways() {
        let mut d = ToolUi {
            part_number: "  P-1 ".into(),
            helix_deg: "38".into(),
            notes: "  ".into(),
            ..ToolUi::default()
        };
        let c = d.to_tool().catalog;
        assert_eq!(c.part_number.as_deref(), Some("P-1"));
        assert_eq!(c.helix_deg, Some(38.0));
        assert_eq!(c.notes, None);
        let back = ToolUi::from_tool("T", &d.to_tool(), true);
        assert_eq!((back.part_number.as_str(), back.helix_deg.as_str()), ("P-1", "38"));
        d.helix_deg = "3x".into();
        assert!(d.to_tool().catalog.refusal().is_some(), "half-typed helix is refused, not dropped");
    }
}
