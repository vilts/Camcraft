//! Which glyph each tool kind wears - in the browser's tool library rows
//! and beside the operation page's tool picker (#661 section 4).
//! Provenance: our own mapping; the glyphs are original SVG masters in
//! assets/ui/svg/ (`tool-*`), drawn from each cutter's silhouette.

use camcraft_app::model;
use camcraft_app::tools::ToolKind;
use egui::{Image, Ui, Vec2};
use flecs_ecs::prelude::World;

use super::icons::{Icon, IconSet};

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
    }
}

/// The kind's glyph at 16 px, tinted with the body ink like the ribbon and
/// the Debug info door; nothing (no space taken) when the icon set is not
/// up - the text beside it still names the tool.
pub(crate) fn kind_glyph(ui: &mut Ui, kind: ToolKind) {
    let Some(set) = IconSet::from_ctx(ui.ctx()) else { return };
    if let Some(tex) = set.texture(tool_icon(kind)) {
        let ink = ui.visuals().text_color();
        ui.add(Image::new(tex).fit_to_exact_size(Vec2::splat(16.0)).tint(ink));
    }
}

/// The glyph of the document tool called `name` (nothing when there is no
/// such tool): the picker and the setup's tool list show the kind beside
/// the name.
pub(crate) fn named_glyph(ui: &mut Ui, world: &World, name: &str) {
    if let Some(t) = model::find_by_name(world, name).and_then(|e| model::tool_of(world, &e)) {
        kind_glyph(ui, t.kind);
    }
}
