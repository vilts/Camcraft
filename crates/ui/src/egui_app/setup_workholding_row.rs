//! The Setups page's "Workholding max rpm" row (#1031): the rated maximum
//! rpm of the chuck or collet a turning setup holds the work in. Apply
//! queues `CommandKind::SetSetupWorkholdingRpm` with the typed value and
//! Clear queues it with none - nothing here writes the world, and undo
//! walks each change. Shown on a setup whose machine turns the work (a
//! lathe or a mill-turn), and on any setup that already states a limit.
//! Provenance: our own page law (issue #1031); no vendor material consulted.

use egui::{DragValue, Ui};

use camcraft_app::machine::Kind;
use camcraft_app::{setup_mount, setup_workholding};

use crate::state::{CommandKind, State};

/// The row's words, pinned by the structural twin.
pub(crate) const ROW_LABEL: &str = "    Workholding max rpm";

/// One setup's row.
pub(super) fn row(ui: &mut Ui, state: &mut State, name: &str) {
    let held = setup_workholding::of(&state.world, Some(name));
    let turns = matches!(setup_mount::kind_of_setup(&state.world, Some(name)), Kind::Lathe | Kind::MillTurn);
    if held.is_none() && !turns {
        return;
    }
    let id = ui.id().with(("setup_workholding_draft", name));
    let mut draft: f64 = ui.data_mut(|d| *d.get_temp_mut_or_insert_with(id, || held.unwrap_or(2000.0)));
    let mut queued: Option<Option<f64>> = None;
    ui.horizontal(|ui| {
        ui.label(ROW_LABEL);
        ui.add(DragValue::new(&mut draft).speed(10.0).range(1.0..=setup_workholding::MAX_STATED_RPM).suffix(" rpm"));
        let hover = "The chuck's or collet's rated maximum rpm. Constant surface speed is clamped to it \
                     (with the machine's and the tool's), and a constant-rpm turning operation over it is \
                     refused at the post.";
        if ui.button("Apply rpm").on_hover_text(hover).clicked() && held != Some(draft) {
            queued = Some(Some(draft));
        }
        if held.is_some() && ui.button("Clear rpm").clicked() {
            queued = Some(None);
        }
    });
    let words = held.map_or_else(|| "No workholding limit stated".to_owned(), |r| format!("Stated: {r:.0} rpm"));
    ui.label(egui::RichText::new(format!("      {words}")).weak());
    if let Some(max_rpm) = queued {
        state.pending.push_back(CommandKind::SetSetupWorkholdingRpm { setup: name.to_owned(), max_rpm });
    }
    ui.data_mut(|d| d.insert_temp(id, draft));
}
