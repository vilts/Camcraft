//! The Heights tab's height editor (M1.5.4's From / Offset law, regrouped
//! for the user's 2026-11-06 review: "a flat stack ... no grouping and
//! ragged alignment"): each height is its own titled group - "Clearance
//! Z", "Top Z", ... - holding the From reference, the setup's work
//! offset (read-only, #329: program zero is the setup's, picked on the
//! Setups page) when the reference is one, and the Offset with the resolved Z as a
//! dimmed suffix, all on the shared property grid. Plus the two height
//! relationship notes (retract vs clearance, the corner), which stay
//! visible: they are consequences, not help.
//!
//! Provenance: our own port of the M-c height rows (the wording is
//! `crate::operations`' law); no vendor material consulted.

use egui::{Label, RichText, Ui};

use camcraft_app::operation::{Height, HeightReference, SrcRef};
use camcraft_app::units;

use crate::operations::{height_echo, offset_label};

use super::fields::{combo, num};
use super::layout::{check_row, consequence, group, hint, prop_row};

/// The frame-temp key `params_fields` stores the edited op's source under,
/// for the height editors' From: Selection echo (M11.3 A).
pub(super) const HEIGHT_SRC_ID: &str = "camcraft.height_src";

fn op_src(ui: &Ui) -> Option<SrcRef> {
    ui.data(|d| d.get_temp(egui::Id::new(HEIGHT_SRC_ID)))
}

/// One height as a titled group. `tag` scopes the widgets' egui ids: the
/// combos salt through `make_persistent_id`, which ignores position, so
/// five height editors in one panel would otherwise share one id per
/// combo (the duplicate-id badges the first S2 dump caught). Each call
/// site passes its section name.
pub(super) fn height_editor(
    ui: &mut Ui,
    title: &str,
    world: &flecs_ecs::prelude::World,
    setup: Option<&str>,
    h: &mut Height,
    tag: &str,
) {
    ui.push_id(tag, |ui| {
        group(ui, title, None, |ui| height_rows(ui, world, setup, h, height_tag_is_bottom(tag)));
    });
}

/// The retract-vs-clearance note under the Heights rows, resolved through
/// the edited op's own source (the same frame-temp the echo reads), so a
/// Selection-referenced height speaks too (M11.3 A owed item).
pub(super) fn retract_note(
    ui: &mut Ui,
    world: &flecs_ecs::prelude::World,
    setup: Option<&str>,
    retract: &Height,
    clearance: &Height,
) {
    let src = op_src(ui);
    if let Some(note) = crate::operations::retract_note(world, setup, src.as_ref(), retract, clearance) {
        consequence(ui, &note);
    }
}

pub(super) fn corner_note(ui: &mut Ui, world: &flecs_ecs::prelude::World, setup: Option<&str>, top: &Height, drop: f64) {
    let src = op_src(ui);
    consequence(ui, &crate::operations::corner_note(world, setup, src.as_ref(), top, drop));
}

/// The Heights tab's group for the cut's levels - the retract plane's
/// hop, the depth staging, the stepdown (user review 2026-11-06: the
/// loose rows under the heights get a titled home).
pub(super) const LEVELS: &str = "Levels";

/// The optional retract plane, inside the Levels group: off, every link
/// climbs to Clearance Z; on, links hop at a Retract Z (seeded 1 above
/// Top Z) with the retract-vs-clearance note under it.
pub(super) fn retract_hop(
    ui: &mut Ui,
    world: &flecs_ecs::prelude::World,
    setup: Option<&str>,
    retract_z: &mut Option<Height>,
    top_z: &Height,
    clearance_z: &Height,
    tag: &str,
) {
    let mut hop = retract_z.is_some();
    if check_row(ui, "Hop below clearance", &mut hop).changed() {
        *retract_z = if hop { Some(Height::of(top_z.reference, top_z.offset + 1.0)) } else { None };
    }
    hint(ui, "the retract plane: on, links hop at Retract Z; off, every link climbs to Clearance Z");
    if let Some(r) = retract_z {
        height_editor(ui, "Retract Z", world, setup, r, tag);
        retract_note(ui, world, setup, r, clearance_z);
    }
}

/// Whether a height editor's tag names a floor - the last level, the cut
/// plane, the facing or parting plane - so "Selection" there measures from
/// the selection's LOWEST Z; every other height from its highest.
fn height_tag_is_bottom(tag: &str) -> bool {
    [".bottom", ".cut", ".face", ".plane"].iter().any(|s| tag.ends_with(s))
}

/// Where the read-only work-offset row points the user (#329).
pub(crate) const SETUP_OFFSET_HINT: &str =
    "program zero is the setup's WCS: its work offset (named by the setup's post) is set on the WCS page or the Setups page";

/// The read-only work-offset row's text: the setup's resolved word, and
/// where it is set.
pub(crate) fn setup_offset_text(world: &flecs_ecs::prelude::World, setup: Option<&str>) -> String {
    let word = camcraft_app::setup::wcs_name(world, setup);
    match camcraft_app::setup::effective(world, setup) {
        Some(s) => format!("WCS ({word})  (from setup {s})"),
        None => format!("WCS ({word})"),
    }
}

fn height_rows(ui: &mut Ui, world: &flecs_ecs::prelude::World, setup: Option<&str>, h: &mut Height, bottom: bool) {
    let u = units::of(world);
    let src = op_src(ui);
    let mut idx = h.reference.kind_index().min(HeightReference::KIND_LABELS.len() - 1);
    let before = idx;
    prop_row(ui, "From", |ui| combo(ui, "h_kind", &mut idx, &HeightReference::KIND_LABELS));
    if idx != before {
        h.reference = match idx {
            0 => HeightReference::Fixed,
            1 => HeightReference::TopOfStock,
            2 => HeightReference::BottomOfStock,
            3 => HeightReference::TopOfModel,
            4 => HeightReference::BottomOfModel,
            5 => HeightReference::WorkOffset,
            _ => HeightReference::selection_for(bottom),
        };
        // The selection IS the plane (user hand test 2026-11-06): a floor
        // switched to Selection sits on the picked face. Carrying the old
        // reference's offset over (Bottom of stock +18.7 -> selection
        // bottom +18.7) put the floor 18.7 above a face at Z -5.
        if h.reference.is_selection() {
            h.offset = 0.0;
        }
    }
    if matches!(h.reference, HeightReference::WorkOffset) {
        // #329: no slot choice here - the setup's offset, read-only.
        prop_row(ui, "Work offset", |ui| {
            ui.add(Label::new(setup_offset_text(world, setup)).selectable(false))
                .on_hover_text(SETUP_OFFSET_HINT);
        });
    }
    // Both strings are operations.rs's law (the retired imgui height row
    // said the same words); the page only lays them out - the resolved Z
    // as the row's dimmed suffix (right-to-left: the echo goes first).
    let echo = height_echo(world, setup, src.as_ref(), h);
    prop_row(ui, offset_label(h.reference), |ui| {
        ui.add(Label::new(RichText::new(echo).weak()).selectable(false));
        num(ui, u, &mut h.offset);
    });
}

#[cfg(test)]
mod offset_hint_tests {
    use super::SETUP_OFFSET_HINT;

    /// #876: the hint names the two pages that really draw the work-offset
    /// dropdown (the WCS page, #343, and the Setups page), and uses plain
    /// words - no RS-274 "word" or "G-word" jargon.
    #[test]
    fn offset_hint_names_the_real_pages_in_plain_words() {
        assert!(SETUP_OFFSET_HINT.contains("WCS page"), "{SETUP_OFFSET_HINT}");
        assert!(SETUP_OFFSET_HINT.contains("Setups page"), "{SETUP_OFFSET_HINT}");
        assert!(SETUP_OFFSET_HINT.contains("work offset (named by the setup's post)"), "{SETUP_OFFSET_HINT}");
        // #931: the post names the offsets - no fixed G54-G59 range.
        assert!(!SETUP_OFFSET_HINT.contains("G54"), "{SETUP_OFFSET_HINT}");
        assert!(!SETUP_OFFSET_HINT.contains("word"), "{SETUP_OFFSET_HINT}");
    }
}
