//! The Setups page's part-alignment control (#326): how the part sits on
//! the table in one setup. #738 made the page one flow: every move -
//! this row's preset and "Turn by", then the face, three-point and axis
//! picks below it - starts from where the part sits NOW and is one undo
//! step, so no row silently wipes what another did. "As modelled" is the
//! one reset, named as such. A preset turns about the part's centre;
//! typed angles turn about the setup's WCS origin by default (#606), or
//! the part's centre, by choice - the row says which. Every change queues
//! the undoable `AlignPartToSetup` (`CommandKind::TurnPart`, or
//! `CommandKind::AlignPart` for the reset); nothing here writes the
//! world. The work offset is a separate row (its own picker).
//!
//! Provenance: our own UI over plain shop practice (re-clamping a part
//! face-down or turned); no vendor material consulted.

use egui::{ComboBox, DragValue, Ui};

use camcraft_app::setup_align::{self, AlignPreset};
use camcraft_app::setup_turn::TurnPivot;

use crate::state::{CommandKind, State};

/// The words and angles for a stored alignment live in the app (the
/// setup sheet prints the same words); the page reads them from there.
pub(crate) use camcraft_app::setup_align::describe as align_text;

/// The page's flow, said once on the row's header (#738).
const FLOW_TIP: &str = "Align the part in steps, top to bottom: start from a preset or turn it by \
     angles (optional), lay it on a face or on three points, then set its axes in the plane. Each Apply \
     moves the part from where it sits now - nothing below undoes what came before - and is one undo \
     step (Ctrl+Z). \"As modelled\" returns it to its import pose.";

/// One setup row's control: the header with the flow, the preset combo,
/// then the typed turn, then the pivot and Apply on a line of their own
/// (the properties pane is narrow - #330's controls ran off its edge).
pub(super) fn picker(ui: &mut Ui, state: &mut State, setup: &str) {
    let current = setup_align::stored(&state.world, setup);
    let mut queued: Option<CommandKind> = None;
    ui.horizontal(|ui| {
        ui.label("    Part alignment").on_hover_text(FLOW_TIP);
        ComboBox::from_id_salt(("setup_align", setup))
            .selected_text(align_text(current))
            .show_ui(ui, |ui| {
                for p in AlignPreset::ALL {
                    let tip = match p {
                        AlignPreset::AsModelled => "Undo every move: the part back in its import pose.",
                        _ => "Turn the part this way from where it sits now, about its centre.",
                    };
                    if ui.selectable_label(false, p.label()).on_hover_text(tip).clicked() {
                        queued = Some(match p {
                            AlignPreset::AsModelled => CommandKind::AlignPart {
                                setup: setup.to_owned(),
                                angles: [0.0; 3],
                                pivot: TurnPivot::PartCentre,
                            },
                            _ => CommandKind::TurnPart { setup: setup.to_owned(), angles: p.angles(), pivot: TurnPivot::PartCentre },
                        });
                    }
                }
            })
            .response
            .on_hover_text(
                "Shows how the part sits now. A choice flips or turns it from there, about the part's \
                 centre - it adds to the face, point and axis moves, never replaces them; \"As modelled\" \
                 clears them all. The stock follows and is rebuilt square to the table; the work offset \
                 is set separately below.",
            );
    });
    let id = ui.id().with(("setup_align_draft", setup));
    let (mut draft, mut pivot): ([f64; 3], TurnPivot) =
        ui.data_mut(|d| *d.get_temp_mut_or_insert_with(id, || ([0.0; 3], TurnPivot::default())));
    ui.horizontal(|ui| {
        ui.label("      Turn");
        for (axis, v) in ["X", "Y", "Z"].into_iter().zip(draft.iter_mut()) {
            ui.label(axis);
            ui.add(DragValue::new(v).speed(1.0).range(-360.0..=360.0).suffix("°"));
        }
    });
    ui.horizontal(|ui| {
        ui.label("      about");
        ComboBox::from_id_salt(("setup_align_pivot", setup))
            .selected_text(pivot.label())
            .show_ui(ui, |ui| {
                for p in TurnPivot::ALL {
                    ui.selectable_value(&mut pivot, p, p.label());
                }
            })
            .response
            .on_hover_text(
                "What stays put as the part turns: the WCS origin keeps the part's point on \
                 the origin there; the part centre keeps the centre of its bounds put.",
            );
        let tip = match pivot {
            TurnPivot::WcsOrigin => {
                "Turn the part by these angles (X, then Y, then Z) from where it sits now, about the \
                 WCS origin: the part keeps its place relative to the origin."
            }
            TurnPivot::PartCentre => {
                "Turn the part by these angles (X, then Y, then Z) from where it sits now, about the \
                 centre of its bounds."
            }
        };
        if ui.button("Apply").on_hover_text(tip).clicked() {
            queued = Some(CommandKind::TurnPart { setup: setup.to_owned(), angles: draft, pivot });
        }
    });
    if let Some(k) = queued {
        state.pending.push_back(k);
    }
    ui.data_mut(|d| d.insert_temp(id, (draft, pivot)));
}
