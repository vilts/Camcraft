//! The operation page's "Lead in / Lead out" tab for every level-ladder
//! kind (M11.2 C/D; regrouped on the user's 2026-11-06 review: "crammed,
//! hard to understand, hard to read"): concept groups on the property
//! grid - Entry (the type, then ONLY that type's fields), Linking (level
//! to level, between levels, between passes), Plunge, Entry feeds and
//! Engagement (the last two optional features switch on in their group
//! header). A field whose 0 means "derived" shows an explicit Auto toggle
//! (Auto writes the model's 0). Help rides the labels' hover; the one
//! visible line under an entry is its safety consequence. Draws into the
//! draft params; the page's Apply rides the command history.
//!
//! Provenance: our own UI over standard CAM link strategies (the motion
//! crate's `links.rs` names the sources); no vendor material or text.

use egui::Ui;

use camcraft_app::links::{Link, Links};
use camcraft_app::machining2d::{PassStart, entry_advice};
use camcraft_app::machining3d::Entry3D;
use camcraft_app::units::Unit;

use super::approach_page::{arm_fields, engagement_group, feeds_group, plunge_group};
use super::fields::{combo, num, num_unit};
use super::layout::{auto_value, check_row, consequence, group, hint, prop_row, prop_row_hint};

const ENTRY_KINDS: [&str; 4] = ["Plunge", "Ramp", "Helix", "Arc"];
const TRANSITIONS: [&str; 5] = ["Same as entry", "Plunge", "Ramp", "Helix", "Arc"];

fn entry_idx(e: Entry3D) -> usize {
    match e {
        Entry3D::Plunge => 0,
        Entry3D::Ramp => 1,
        Entry3D::Helix => 2,
        Entry3D::Arc => 3,
    }
}

fn entry_of(i: usize) -> Entry3D {
    match i {
        1 => Entry3D::Ramp,
        2 => Entry3D::Helix,
        3 => Entry3D::Arc,
        _ => Entry3D::Plunge,
    }
}

fn link_idx(l: Link) -> usize {
    match l {
        Link::Clearance => 0,
        Link::Retract => 1,
        Link::KeepDown => 2,
    }
}

fn link_of(i: usize) -> Link {
    match i {
        1 => Link::Retract,
        2 => Link::KeepDown,
        _ => Link::Clearance,
    }
}

/// The whole tab. `first` is the kind's first entry and its helix lead
/// radius, `None` for a kind whose first engagement is not a choice (the
/// bore's centre dive).
pub(super) fn leads_tab(ui: &mut Ui, u: Unit, tag: &str, first: Option<(&mut Entry3D, &mut f64)>, links: &mut Links) {
    leads_tab_with_retract(ui, u, tag, first, links, true, PassStart::Auto);
}

/// Roughing links use clearance while Heights has no retract hop (#3318).
/// Keep the stored Retract choice so enabling the hop restores its plane.
/// `start`: the op's "Pass starts at" (#3748) - for Inside or Outside a
/// Helix, Ramp or Plunge entry shows the advice under the Type selector;
/// Auto for a kind without the choice.
pub(super) fn leads_tab_with_retract(
    ui: &mut Ui,
    u: Unit,
    tag: &str,
    first: Option<(&mut Entry3D, &mut f64)>,
    links: &mut Links,
    has_retract: bool,
    start: PassStart,
) {
    let entry = first.as_ref().map(|(e, _)| **e);
    if let Some((entry, helix_radius)) = first {
        group(ui, "Entry", None, |ui| entry_group(ui, u, tag, entry, helix_radius, links, start));
    }
    group(ui, "Linking", None, |ui| linking_group(ui, u, tag, entry, links, has_retract));
    plunge_group(ui, u, &mut links.approach);
    feeds_group(ui, u, &mut links.approach);
    engagement_group(ui, &mut links.approach);
}

fn entry_group(ui: &mut Ui, u: Unit, tag: &str, entry: &mut Entry3D, helix_radius: &mut f64, links: &mut Links, start: PassStart) {
    let mut e_idx = entry_idx(*entry);
    let before = e_idx;
    prop_row_hint(ui, "Type", "how the tool first reaches the cut depth", |ui| {
        combo(ui, &format!("{tag}_entry"), &mut e_idx, &ENTRY_KINDS)
    });
    if e_idx != before {
        *entry = entry_of(e_idx);
    }
    // #3748: every type stays selectable; a type that is not the
    // recommended Arc on an Inside or Outside pass says why, next to the
    // selector (the same text as the op's warning).
    if let Some(advice) = entry_advice(start, *entry) {
        consequence(ui, advice);
    }
    if *entry == Entry3D::Helix {
        prop_row(ui, "Helix radius", |ui| num(ui, u, helix_radius));
    }
    arm_fields(ui, u, *entry, &mut links.approach);
}

fn linking_group(ui: &mut Ui, u: Unit, tag: &str, entry: Option<Entry3D>, links: &mut Links, has_retract: bool) {
    let (retract_label, keep_down_hint) = if has_retract {
        (
            "Retract plane",
            "the tool stays at depth only where the link is proved clear of the part; every other link lifts to the retract plane, named in the status line",
        )
    } else {
        (
            "Clearance plane (hop off)",
            "the tool stays at depth only where the link is proved clear of the part; every other link lifts to Clearance Z because Hop below clearance is off",
        )
    };
    let link_names = ["Clearance plane", retract_label, "Keep down"];
    let mut t_idx = links.transition.map_or(0, |e| entry_idx(e) + 1);
    prop_row(ui, "Level to level", |ui| combo(ui, &format!("{tag}_transition"), &mut t_idx, &TRANSITIONS));
    links.transition = t_idx.checked_sub(1).map(entry_of);
    match links.transition {
        None => hint(ui, "each level is entered the way the first one is"),
        Some(Entry3D::Plunge) => hint(ui, "an axial dive where the floor has room"),
        Some(Entry3D::Arc) => hint(ui, "a vertical-plane arc onto the first cut; a plunge stands in where it cannot fit"),
        Some(Entry3D::Ramp) => {}
        Some(Entry3D::Helix) => {
            if entry != Some(Entry3D::Helix) {
                hint(ui, "the helix descends on a 1 mm radius");
            }
            auto_value(ui, "Descent helix pitch", "Auto", &mut links.helix_pitch, "1 turn / tool Ø", 1.0, |ui, v| {
                num(ui, u, v);
            });
        }
    }
    if links.reads_ramp_angle() {
        prop_row(ui, "Max ramp angle", |ui| num_unit(ui, &mut links.ramp_angle, "°"));
    }
    // A kind without a chosen first entry (the bore) shapes its descent
    // with the entry numbers here, under the transition they serve.
    if entry.is_none()
        && let Some(arm) = links.transition
    {
        arm_fields(ui, u, arm, &mut links.approach);
    }
    // #2012: the depth ladder's mode - off keeps the remainder last step.
    check_row(ui, "Equal depth steps", &mut links.approach.equal_steps);
    hint(ui, "the same number of levels, each step the same depth, none over the stepdown");
    let mut l_idx = link_idx(links.level);
    prop_row(ui, "Between levels", |ui| combo(ui, &format!("{tag}_level_link"), &mut l_idx, &link_names));
    links.level = link_of(l_idx);
    let mut p_idx = link_idx(links.pass);
    prop_row(ui, "Between passes", |ui| combo(ui, &format!("{tag}_pass_link"), &mut p_idx, &link_names));
    links.pass = link_of(p_idx);
    if links.level == Link::KeepDown || links.pass == Link::KeepDown {
        auto_value(ui, "Max stay-down distance", "No limit", &mut links.max_stay_down, "no limit", 50.0, |ui, v| {
            num(ui, u, v);
        });
        hint(ui, keep_down_hint);
    }
}
