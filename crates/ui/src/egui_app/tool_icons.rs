//! Which glyph each tool kind wears - in the browser's tool library rows,
//! on the Tool library catalog rows and in the operation page's tool
//! picker (#661 section 4).
//! Provenance: our own mapping; the glyphs are original SVG masters in
//! assets/ui/svg/ (`tool-*`), drawn from each cutter's silhouette.

use camcraft_app::model;
use camcraft_app::tools::{Tool, ToolKind};
use egui::{ComboBox, Image, Ui, Vec2};
use flecs_ecs::prelude::World;

use super::icons::{Icon, IconSet};
use super::layout::CTRL_W;

/// The tool kind's glyph. Exhaustive on purpose: a new `ToolKind` without
/// a glyph stops at the compiler, and the structural test pins that no two
/// kinds share one.
pub(crate) fn tool_icon(kind: ToolKind) -> Icon {
    match kind {
        ToolKind::EndMill => Icon::ToolEndMill,
        ToolKind::BallNose => Icon::ToolBallNose,
        ToolKind::BullNose => Icon::ToolBullNose,
        ToolKind::Drill => Icon::ToolDrill,
        ToolKind::Tap => Icon::ToolTap,
        ToolKind::Bore => Icon::ToolBoringBar,
        ToolKind::Turning => Icon::ToolTurningInsert,
        ToolKind::Groove => Icon::ToolGroovingBlade,
        ToolKind::Thread => Icon::ToolThreadingInsert,
        ToolKind::Form => Icon::ToolForm,
        ToolKind::Taper => Icon::ToolChamferMill,
        ToolKind::Probe => Icon::ToolProbe,
        ToolKind::Torch => Icon::ToolTorch,
        ToolKind::Electrode => Icon::ToolElectrode,
        ToolKind::ThreadMill => Icon::ToolThreadMill,
        ToolKind::ThreadMillSingle => Icon::ToolThreadMillSingle,
    }
}

/// The glyph of one Tool library catalog row: its kind's glyph (#661
/// section 4). The row's label still names the tool.
pub(crate) fn catalog_row_icon(t: &Tool) -> Icon {
    tool_icon(t.kind)
}

/// The glyph of one picker item: the kind of the document tool it names,
/// or nothing when there is no such tool (the text still names it).
pub(crate) fn picker_item_icon(tool: Option<&Tool>) -> Option<Icon> {
    tool.map(|t| tool_icon(t.kind))
}

/// Paints one glyph at 16 px, tinted with the body ink like the ribbon and
/// the Debug info door; nothing (no space taken) when the icon set is not
/// up - the text beside it still names the tool.
pub(crate) fn glyph(ui: &mut Ui, icon: Icon) {
    let Some(set) = IconSet::from_ctx(ui.ctx()) else { return };
    if let Some(tex) = set.texture(icon) {
        let ink = ui.visuals().text_color();
        ui.add(Image::new(tex).fit_to_exact_size(Vec2::splat(16.0)).tint(ink));
    }
}

/// The glyph of the document tool called `name` (nothing when there is no
/// such tool): the setup's tool list and the operation page's picker show
/// the kind beside the name.
pub(crate) fn named_glyph(ui: &mut Ui, world: &World, name: &str) {
    let tool = model::find_by_name(world, name).and_then(|e| model::tool_of(world, &e));
    if let Some(icon) = picker_item_icon(tool.as_ref()) {
        glyph(ui, icon);
    }
}

/// The tool picker: `combo_owned`'s dropdown with each document tool's
/// glyph in front of its name. The items keep their names (the accessible
/// labels are unchanged); the glyph is decoration.
pub(crate) fn tool_combo(ui: &mut Ui, world: &World, id: &str, idx: &mut usize, names: &[String]) {
    let cur: String = names.get((*idx).min(names.len().saturating_sub(1))).cloned().unwrap_or_default();
    ComboBox::from_id_salt(id).width(CTRL_W).selected_text(cur).show_ui(ui, |ui| {
        for (i, n) in names.iter().enumerate() {
            ui.horizontal(|ui| {
                let tool = model::find_by_name(world, n).and_then(|e| model::tool_of(world, &e));
                if let Some(icon) = picker_item_icon(tool.as_ref()) {
                    glyph(ui, icon);
                }
                ui.selectable_value(idx, i, n.as_str());
            });
        }
    });
}
