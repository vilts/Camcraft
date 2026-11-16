//! The tool editor's catalog rows (#898): part number, vendor, coating,
//! helix angle and notes. Draft fields only; Apply / Add carry them
//! through `ChangeTool` / `AddTool` like every other tool number, so one
//! edit is one undo step and a bad value is refused by name there.
//!
//! Provenance: our own widget over the tool model's catalog fields.

use egui::{TextEdit, Ui};

use crate::state::ToolUi;

use super::fields::field;

/// The rows, in the editor's single column, under one collapsed header
/// (open when the tool states any fact) so the editor keeps its height.
pub(super) fn catalog_rows(ui: &mut Ui, t: &mut ToolUi) {
    let any = [&t.part_number, &t.vendor, &t.coating, &t.helix_deg, &t.notes].iter().any(|s| !s.trim().is_empty());
    egui::CollapsingHeader::new("Catalog")
        .id_salt("tool_catalog_header")
        .default_open(any)
        .show(ui, |ui| {
            field(ui, "Part number");
            ui.add(TextEdit::singleline(&mut t.part_number).id_salt("tool_part_number").desired_width(140.0));
            field(ui, "Vendor");
            ui.add(TextEdit::singleline(&mut t.vendor).id_salt("tool_vendor").desired_width(140.0));
            field(ui, "Coating");
            ui.add(TextEdit::singleline(&mut t.coating).id_salt("tool_coating").desired_width(140.0));
            field(ui, "Helix angle (deg)");
            ui.add(TextEdit::singleline(&mut t.helix_deg).id_salt("tool_helix").desired_width(60.0).hint_text("0-90"));
            field(ui, "Notes");
            ui.add(TextEdit::multiline(&mut t.notes).id_salt("tool_notes").desired_width(140.0).desired_rows(2));
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
