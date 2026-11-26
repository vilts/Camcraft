//! The Setups page's "Sheet job" checklist (#3070): the answers a flat-sheet
//! router job needs, in one group - sheet size and thickness, where Z zero
//! is (sheet top or spoilboard), the through-cut allowance into the
//! spoilboard (always shown), XY zero (front left by default) and the clamp
//! clearance. Apply queues `CommandKind::SetSheetJob`, which lands ONE
//! undoable `SetSheetJob` (stock, WCS and the record); Clear drops the
//! record. Nothing here writes the world. Shown on a milling setup whose
//! machine offers it (#2410: a router, or no machine), and on any setup
//! that already states a job.
//! Provenance: our own page law (issue #3070); no vendor material consulted.

use egui::{ComboBox, DragValue, RichText, Ui};

use camcraft_app::machine::Kind;
use camcraft_app::sheet_job::{self, SheetCorner, SheetJob, SheetZero, MAX_ALLOWANCE, MAX_CLAMP_CLEARANCE};
use camcraft_app::router_offer::{self, RouterOption};
use camcraft_app::{model, setup_mount};

use crate::state::{CommandKind, State};

/// The group title, pinned by the structural twin.
pub(crate) const GROUP: &str = "Sheet job";
/// The Apply and Clear words, pinned by the structural twin.
pub(crate) const APPLY: &str = "Apply sheet job";
pub(crate) const CLEAR: &str = "Clear sheet job";
/// The allowance row's words: the one answer never left implicit.
pub(crate) const ALLOWANCE_LABEL: &str = "    Through-cut allowance into spoilboard";
/// The allowance a fresh checklist offers, mm (shown, never hidden).
const DEFAULT_ALLOWANCE: f64 = 0.3;

/// Whether the group is drawn for `name`: a milling setup whose machine
/// offers the sheet job (a router, or no machine - #2410), or one that
/// already states a job (a stated job is never hidden).
pub(super) fn shown(state: &State, name: &str) -> bool {
    let offered = matches!(setup_mount::kind_of_setup(&state.world, Some(name)), Kind::Mill)
        && router_offer::offers(&state.world, Some(name), RouterOption::SheetJob);
    offered || sheet_job::of(&state.world, name).is_some()
}

/// The checklist a fresh group offers: the stated job, else the setup's
/// stock box as the sheet, Z zero on top, XY zero front left.
fn first_draft(state: &State, name: &str) -> SheetJob {
    if let Some(j) = sheet_job::of(&state.world, name) {
        return j;
    }
    let (length, width, thickness) = model::stock_world_aabb_for(&state.world, Some(name)).map_or((600.0, 400.0, 18.0), |b| {
        (b.max.x - b.min.x, b.max.y - b.min.y, b.max.z - b.min.z)
    });
    SheetJob {
        length,
        width,
        thickness,
        z_zero: SheetZero::Top,
        xy_zero: SheetCorner::FrontLeft,
        allowance: DEFAULT_ALLOWANCE,
        clamp_clearance: 0.0,
    }
}

fn mm_row(ui: &mut Ui, label: &str, v: &mut f64, max: f64) {
    ui.horizontal(|ui| {
        ui.label(label);
        ui.add(DragValue::new(v).speed(0.1).range(0.0..=max).suffix(" mm"));
    });
}

fn zero_words(z: SheetZero) -> &'static str {
    match z {
        SheetZero::Top => "Sheet top",
        SheetZero::Spoilboard => "Spoilboard",
    }
}

fn corner_words(c: SheetCorner) -> &'static str {
    match c {
        SheetCorner::FrontLeft => "Front left",
        SheetCorner::FrontRight => "Front right",
        SheetCorner::BackLeft => "Back left",
        SheetCorner::BackRight => "Back right",
        SheetCorner::Centre => "Centre",
    }
}

/// What a job means for the program, in one line.
pub(crate) fn summary(j: &SheetJob) -> String {
    let floor = match j.z_zero {
        SheetZero::Top => -(j.thickness + j.allowance),
        SheetZero::Spoilboard => -j.allowance,
    };
    format!(
        "Z0 on the {}; cuts may go {:.2} mm into the spoilboard (deepest Z{floor:.2}); rapids stay {:.2} mm over the sheet",
        zero_words(j.z_zero).to_lowercase(),
        j.allowance,
        j.clamp_clearance
    )
}

/// One setup's checklist.
pub(super) fn row(ui: &mut Ui, state: &mut State, name: &str) {
    let held = sheet_job::of(&state.world, name);
    let id = ui.id().with(("setup_sheet_job_draft", name));
    let mut d: SheetJob = ui.data_mut(|m| *m.get_temp_mut_or_insert_with(id, || first_draft(state, name)));
    let limit = camcraft_app::stock::MAX_STOCK_DIM;
    mm_row(ui, "    Sheet length (X)", &mut d.length, limit);
    mm_row(ui, "    Sheet width (Y)", &mut d.width, limit);
    mm_row(ui, "    Sheet thickness", &mut d.thickness, limit);
    ui.horizontal(|ui| {
        ui.label("    Z zero");
        ComboBox::from_id_salt(("sheet_z_zero", name)).selected_text(zero_words(d.z_zero)).show_ui(ui, |ui| {
            for z in [SheetZero::Top, SheetZero::Spoilboard] {
                ui.selectable_value(&mut d.z_zero, z, zero_words(z));
            }
        });
    });
    mm_row(ui, ALLOWANCE_LABEL, &mut d.allowance, MAX_ALLOWANCE);
    ui.horizontal(|ui| {
        ui.label("    XY zero");
        ComboBox::from_id_salt(("sheet_xy_zero", name)).selected_text(corner_words(d.xy_zero)).show_ui(ui, |ui| {
            for c in [SheetCorner::FrontLeft, SheetCorner::FrontRight, SheetCorner::BackLeft, SheetCorner::BackRight, SheetCorner::Centre] {
                ui.selectable_value(&mut d.xy_zero, c, corner_words(c));
            }
        });
    });
    mm_row(ui, "    Clamp / screw clearance", &mut d.clamp_clearance, MAX_CLAMP_CLEARANCE);
    ui.label(RichText::new(format!("      {}", summary(&d))).small().weak());
    let mut queued: Option<Option<SheetJob>> = None;
    ui.horizontal(|ui| {
        let hover = "Sets this setup's stock to the sheet and moves its WCS to the stated zero, as one undo step. \
                     The post refuses a cut deeper than the allowance into the spoilboard, or a rapid across \
                     below the clamp clearance.";
        if ui.button(APPLY).on_hover_text(hover).clicked() {
            queued = Some(Some(d));
        }
        if held.is_some() && ui.button(CLEAR).clicked() {
            queued = Some(None);
        }
    });
    let words = held.map_or_else(|| "No sheet job stated".to_owned(), |j| format!("Stated: {}", summary(&j)));
    ui.label(RichText::new(format!("      {words}")).weak());
    if let Some(job) = queued {
        state.pending.push_back(CommandKind::SetSheetJob { setup: name.to_owned(), job });
    }
    ui.data_mut(|m| m.insert_temp(id, d));
}
