//! The Setups page's work-offset picker (#196, #342, #931): which work
//! offset the post writes for the setup's one WCS. It lists offsets
//! `1..=max` of the setup's OWN post (`setup::work_offset_names`), each
//! worded "Work offset n (NAME)" - "Work offset n" when the post gives
//! it no name. It is only the label - the WCS origin, and every height
//! measured from it, stay put.
//! An unset setup IS offset 1 - one entry, stored unset, so the file
//! format is unchanged (#329). A pick queues the undoable
//! `SetSetupWorkOffset`; nothing here writes the world.
//!
//! Provenance: our own UI over RS-274 / ISO 6983 work-offset selection;
//! no vendor material consulted.

use egui::{ComboBox, Ui};

use camcraft_app::post::work_offset::WorkOffsets;
use camcraft_app::setup_offsets;

use crate::state::{CommandKind, State};

/// The 1-based offset of a stored pick (unset = offset 1).
fn offset_of(slot: Option<usize>) -> u32 {
    u32::try_from(slot.unwrap_or(0)).map_or(u32::MAX, |s| s.saturating_add(1))
}

/// The combo text for a stored pick within the post's table - unset
/// reads like an explicit offset 1. A pick beyond what the post can
/// express says so; the post gate refuses it by setup name.
fn entry_text(t: &WorkOffsets, slot: Option<usize>) -> String {
    let n = offset_of(slot);
    let label = setup_offsets::entry_label(t, n);
    if n > t.max {
        format!("{label} - not on this post")
    } else {
        label
    }
}

/// [`entry_text`] for one setup, read from its own post's table (the
/// tests' view of what the combo shows).
#[cfg(test)]
#[must_use]
pub(super) fn pick_text(world: &flecs_ecs::prelude::World, setup: &str, slot: Option<usize>) -> String {
    entry_text(&camcraft_app::setup::work_offset_names(world, Some(setup)), slot)
}

/// The stored picks the combo lists: the post's offsets `1..=max`,
/// plus a stored pick beyond them so it stays visible as itself (never
/// shown as offset 1, and the only entry past the post's range).
fn entries(t: &WorkOffsets, current: Option<usize>) -> Vec<usize> {
    let max = usize::try_from(t.max).unwrap_or(0).min(setup_offsets::MAX_SLOT.saturating_add(1));
    let mut out: Vec<usize> = (0..max).collect();
    if let Some(c) = current.filter(|c| *c >= max) {
        out.push(c);
    }
    out
}

/// A stored pick in the combo's terms: offset 1 (slot 0) and unset are
/// one entry, so both read `None` and choosing offset 1 on either queues
/// nothing.
fn normalized(slot: Option<usize>) -> Option<usize> {
    slot.filter(|i| *i != 0)
}

/// One setup row's picker: label, combo, and the queued command when the
/// user picks a different entry.
pub(super) fn picker(ui: &mut Ui, state: &mut State, setup: &str) {
    let current = normalized(camcraft_app::setup::work_offset(&state.world, setup));
    let table = camcraft_app::setup::work_offset_names(&state.world, Some(setup));
    let mut chosen = current;
    ui.horizontal(|ui| {
        ui.label("    Work offset");
        ComboBox::from_id_salt(("setup_wcs", setup))
            .selected_text(entry_text(&table, current))
            .show_ui(ui, |ui| {
                for i in entries(&table, current) {
                    let w = entry_text(&table, Some(i));
                    ui.selectable_value(&mut chosen, normalized(Some(i)), w);
                }
            })
            .response
            .on_hover_text(
                "Which work offset the posted program uses for this setup, as this setup's post \
                 names it. It only names the controller's register - the origin stays where it is placed.",
            );
    });
    if chosen != current {
        state.pending.push_back(CommandKind::SetSetupWorkOffset {
            setup: setup.to_owned(),
            slot: chosen,
        });
    }
}
