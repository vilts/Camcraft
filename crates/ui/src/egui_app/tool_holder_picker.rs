//! The tool editor's holder library picker (#793, #792): choose a
//! built-in holder (BT30 / BT40 / CAT40 or Tormach TTS, ER chuck or
//! Weldon, or a standalone ER chuck), set its gauge length and projection, and Apply
//! holder - which queues the one undoable
//! `SetToolBody::library`. #2018: one Weldon row for each taper, its
//! bore the library's standard bore for the tool's shank. The holder lands as a drawn profile, so the
//! Profile table above edits it like any other. Provenance: our own
//! pane over `app::holder_library`; no vendor material consulted.

use egui::{Color32, Ui};

use camcraft_app::holder_library::{self, Clamp, HolderEntry, PickerRow};
use camcraft_app::units::{self, Unit};

use crate::state::State;

const NOTE: Color32 = Color32::from_rgb(0x9E, 0xA8, 0xB8);
const BAD: Color32 = Color32::from_rgb(230, 90, 80);

/// The picker under the Profile section.
pub(super) fn holder_picker(ui: &mut Ui, state: &mut State, u: Unit) {
    ui.separator();
    ui.strong("Holder library");
    let picked = state.tool_profile_ui.library.clone();
    let mut choose = None;
    egui::ComboBox::from_label("Library holder")
        .selected_text(picked.as_deref().unwrap_or("choose a holder"))
        .width(170.0)
        .show_ui(ui, |ui| {
            for r in holder_library::picker_rows() {
                let n = r.name();
                if ui.selectable_label(picked.as_deref() == Some(n.as_str()), &n).on_hover_text(describe_row(&r, u)).clicked() {
                    choose = Some(n);
                }
            }
        });
    if let Some(n) = choose {
        state.pick_library_holder(&n);
    }
    // The stored tool's shank: the one Apply holder judges, and (#2018)
    // the one a Weldon row takes its bore from.
    let shank = state.editor_stored_tool().map(|t| t.shank_d);
    let pick = state.tool_profile_ui.library.clone();
    if let Some(n) = &pick {
        ui.horizontal(|ui| {
            ui.label(format!("gauge length {}", u.suffix()));
            super::tool_profile_page::len_cell(ui, u, &mut state.tool_profile_ui.gauge);
        })
        .response
        .on_hover_text("from the gauge line (the spindle face) to the holder's nose face");
        ui.horizontal(|ui| {
            ui.label(format!("projection {}", u.suffix()));
            super::tool_profile_page::len_cell(ui, u, &mut state.tool_profile_ui.projection);
        })
        .response
        .on_hover_text("from the tool tip to the holder's nose face (the tool's stickout)");
        // What the pick is: a Weldon row, or the entry it names.
        let row = holder_library::picker_rows().into_iter().find(|r| r.name() == *n);
        let what = row.map(|r| describe_row(&r, u)).or_else(|| holder_library::find(n).ok().map(|e| describe(&e, u)));
        match shank.map(|s| holder_library::resolve(n, s).and_then(|e| e.fits_shank(s).map(|()| e))) {
            // The resolved holder, bore and body as Apply lands them.
            Some(Ok(e)) => {
                ui.colored_label(NOTE, describe(&e, u));
            }
            Some(Err(err)) => {
                if let Some(w) = what {
                    ui.colored_label(NOTE, w);
                }
                ui.colored_label(BAD, err.to_string());
            }
            None => {
                if let Some(w) = what {
                    ui.colored_label(NOTE, w);
                }
            }
        }
    }
    if ui
        .add_enabled(pick.is_some(), egui::Button::new("Apply holder"))
        .on_hover_text("drop the library holder into the tool's holder (one undo step); it then edits as a Custom profile")
        .clicked()
    {
        state.queue_apply_tool_holder();
    }
}

/// One line on what a picker row is: an entry as [`describe`] says, or
/// a taper's Weldon row with the bores it takes the tool's shank into.
fn describe_row(r: &PickerRow, u: Unit) -> String {
    match r {
        PickerRow::Entry(e) => describe(e, u),
        PickerRow::Weldon { taper: t, bores } => {
            let l = |v: f64| units::len(v, u, 2);
            let b: Vec<String> = bores.iter().map(|b| format!("Ø{}", l(*b))).collect();
            format!(
                "{} {} Ø{} × {} {}; Weldon, bore for the tool's shank: {}",
                t.name,
                t.ring,
                l(t.flange_d),
                l(t.flange_t),
                u.suffix(),
                b.join(", ")
            )
        }
    }
}

/// One line on what the entry is: flange, then nut or body.
fn describe(e: &HolderEntry, u: Unit) -> String {
    let l = |v: f64| units::len(v, u, 2);
    let nose = match e.clamp {
        Clamp::Er(er) => format!(
            "{} nut Ø{} × {}, collets Ø{}-{}",
            er.name,
            l(er.nut_d),
            l(er.nut_len),
            l(er.min_bore),
            l(er.max_bore)
        ),
        Clamp::Weldon { bore, body_d } => format!("Weldon bore Ø{}, body Ø{}", l(bore), l(body_d)),
    };
    match e.taper {
        Some(t) => format!("{} {} Ø{} × {} {}; {nose}", t.name, t.ring, l(t.flange_d), l(t.flange_t), u.suffix()),
        None => format!("straight-shank chuck {}; {nose}", u.suffix()),
    }
}
