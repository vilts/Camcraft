//! #3930 (U4b of #3663): a saved tool's Properties page is a summary. It
//! shows what the tool is - the type, the sizes, the holder, the speeds,
//! the material group and the tool number - as read-only rows, and one
//! door, "Edit in tool library", that opens the tool library window on
//! the tool. The window edits each of these (its Tool, Recipes and
//! Details tabs and its Body section), so Properties no longer has a
//! second draft of them to Apply.
//! Provenance: our own read-only page over the tool draft; no HSMWorks
//! material involved.

use egui::Ui;

use camcraft_app::tools::ToolKind;
use camcraft_app::units::{self, Unit};

use super::layout::prop_row;
use crate::state::State;

/// The button that opens the tool library window on the tool.
pub(crate) const EDIT_IN_LIBRARY: &str = "Edit in tool library";

/// The door to the window: opens it (or keeps it open) on `name`. The
/// Properties subject and the document selection do not change.
pub(super) fn edit_button(ui: &mut Ui, state: &mut State, name: &str) {
    if ui
        .button(EDIT_IN_LIBRARY)
        .on_hover_text("open this tool in the tool library window: its geometry, holder, body, recipes and details")
        .clicked()
    {
        state.open_tool_in_library(name);
    }
}

/// The read-only rows of the tool draft (synced from the document).
pub(super) fn rows(ui: &mut Ui, state: &State) {
    let u = state.units();
    let t = &state.tool_ui;
    let kind = t.tool_kind();
    let l = |v: f64| format!("{} {}", units::len(v, u, 3), u.suffix());
    row(ui, "Type", super::tool_settings_grid::kind_name(t.kind));
    match kind {
        ToolKind::Electrode => {
            row(ui, "Electrode X size", &l(t.diameter));
            row(ui, "Electrode Y size", &l(t.width));
        }
        ToolKind::Torch | ToolKind::Waterjet => row(ui, "Kerf width", &l(t.diameter)),
        ToolKind::Wire => row(ui, "Wire diameter", &l(t.diameter)),
        _ => row(ui, "Diameter", &l(t.diameter)),
    }
    if kind == ToolKind::BallNose {
        // #263: a ball's corner is its ball, half the diameter.
        row(ui, "Corner radius", &l(t.diameter * 0.5));
    } else if reads_corner(kind) {
        row(ui, "Corner radius", &l(t.corner_r));
    }
    row(ui, "Flute length", &l(t.flute_len));
    row(ui, "Shank diameter", &l(t.shank_d));
    row(ui, "Shank length", &l(t.shank_len));
    row(ui, "Holder", &holder(state, u));
    if !matches!(kind, ToolKind::Torch | ToolKind::Waterjet | ToolKind::Electrode | ToolKind::Wire) {
        row(ui, "Spindle rpm", &format!("{:.0}", t.rpm));
    }
    row(ui, "Feed", &format!("{} {}", units::len(t.feed, u, 1), u.feed_suffix()));
    if !t.material_group.is_empty() {
        row(ui, "Material group", &t.material_group);
    }
    row(ui, "Tool number", &t.number.to_string());
}

fn row(ui: &mut Ui, label: &str, value: &str) {
    prop_row(ui, label, |ui| ui.label(value));
}

/// The kinds that read a typed corner radius (the grid's rule).
fn reads_corner(kind: ToolKind) -> bool {
    !matches!(
        kind,
        ToolKind::Tap
            | ToolKind::Torch
            | ToolKind::Waterjet
            | ToolKind::Electrode
            | ToolKind::Wire
            | ToolKind::ThreadMill
            | ToolKind::ThreadMillSingle
            | ToolKind::TSlot
            | ToolKind::Keyseat
            | ToolKind::Dovetail
            | ToolKind::Lollipop
            | ToolKind::FormMill
    )
}

/// One line on the holder the collision checks read.
fn holder(state: &State, u: Unit) -> String {
    let l = |v: f64| units::len(v, u, 1);
    if let Some((entry, ..)) = state.editor_library_holder() {
        return format!("library holder {entry}");
    }
    match state.editor_tool_body() {
        Some(b) if b.machine_default => "the machine's default holder".to_owned(),
        Some(b) if b.spun.is_some() => "a drawn holder outline".to_owned(),
        Some(b) => format!("body {}", b.source),
        None => {
            let t = &state.tool_ui;
            format!("holder cylinder Ø{} × {} {}", l(t.holder_d), l(t.holder_len), u.suffix())
        }
    }
}
