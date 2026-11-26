//! The rest of the "Lead in / Lead out" tab (M11.3 B/C/D; regrouped on
//! the user's 2026-11-06 review): the entry type's own numbers - only
//! the chosen type's, never another's - the Plunge, Entry feeds and
//! Engagement groups, and on the contour the Lead out, Extensions and
//! Axis-aligned start groups. Every "0 means derived / off" number is an
//! explicit toggle (Auto / No limit) or a group-header switch that writes
//! the model's 0, so the params and the commands are unchanged. Draws
//! into the draft params; the page's Apply rides the command history.
//!
//! Provenance: our own UI over the standard entry/lead parameter
//! vocabulary of published CAM practice (the motion crate's
//! `approach.rs` names the sources); no vendor material or text.

use egui::Ui;

use camcraft_app::links::{Approach, Compensation};
use camcraft_app::machining2d::{Lead, PassStart};
use camcraft_app::machining3d::Entry3D;
use camcraft_app::units::Unit;

use super::fields::{combo, num, num_feed, num_unit};
use super::layout::{auto_value, check_row, consequence, group, hint, prop_row, prop_row_hint};

const LEAD_OUTS: [&str; 6] = ["Same as lead-in", "None", "Linear", "Arc", "Tangential", "Perpendicular"];
const COMPS: [&str; 3] = ["In computer", "In control (G41/G42)", "Wear (G41/G42)"];

/// The entry arm's own numbers (`arm` - the first entry, or the level
/// descent a kind without a first entry uses), inside the caller's group.
pub(super) fn arm_fields(ui: &mut Ui, u: Unit, arm: Entry3D, a: &mut Approach) {
    match arm {
        Entry3D::Ramp => {
            auto_value(ui, "Ramp angle", "Auto", &mut a.ramp_angle, "one straight chord", 3.0, |ui, v| {
                num_unit(ui, v, "°");
            });
            hint(ui, "set an angle and the ramp zig-zags along the first cut, never steeper");
            auto_value(ui, "Max ramp leg", "No limit", &mut a.ramp_max_len, "no limit", 10.0, |ui, v| num(ui, u, v));
            check_row(ui, "Ramp from the top every level", &mut a.ramp_to_depth);
        }
        Entry3D::Helix => {
            auto_value(ui, "Minimum radius", "Auto", &mut a.helix_min_radius, "set radius only", 0.5, |ui, v| {
                num(ui, u, v);
            });
            auto_value(ui, "Helix pitch", "Auto", &mut a.helix_pitch, "1 turn / helix Ø", 1.0, |ui, v| num(ui, u, v));
            auto_value(ui, "Helix angle", "Auto", &mut a.helix_angle, "by pitch", 2.0, |ui, v| num_unit(ui, v, "°"));
            check_row(ui, "Clockwise (G2)", &mut a.helix_cw);
            consequence(ui, "where the helix cannot fit, a ramp and then a plunge stand in");
        }
        Entry3D::Arc => {
            auto_value(ui, "Arc radius", "Auto", &mut a.arc_radius, "tool Ø", 3.0, |ui, v| num(ui, u, v));
            auto_value(ui, "Arc sweep", "Auto", &mut a.arc_sweep, "covers the drop", 90.0, |ui, v| num_unit(ui, v, "°"));
            hint(ui, "a shorter sweep covers less of the drop; the rest feeds straight down at the arc's start");
            consequence(ui, "where the arc cannot fit, a plunge stands in");
        }
        Entry3D::Plunge => {}
    }
}

/// The pocket's horizontal arc entry (M11.3): an optional feature,
/// switched in the group header - the entry descends at the arc's start
/// and a G17 arc runs onto the first pass. The vertical Arc entry is its
/// own lead, so the group hides under it.
pub(super) fn pocket_arc_group(ui: &mut Ui, u: Unit, entry: Entry3D, a: &mut Approach) {
    if entry == Entry3D::Arc {
        return;
    }
    group(ui, "Horizontal arc entry", Some(&mut a.arc_xy), |ui| {
        auto_value(ui, "Arc radius", "Auto", &mut a.arc_radius, "tool Ø", 3.0, |ui, v| num(ui, u, v));
        auto_value(ui, "Arc sweep", "Auto", &mut a.arc_sweep, "90°", 90.0, |ui, v| num_unit(ui, v, "°"));
        consequence(ui, "the arc stays inside the pocket - shrunk where it cannot fit, then left out, named in the status");
    });
}

/// Rapid down to a height over the material, feed only the rest - an
/// optional feature, switched in the group header (off writes 0).
pub(super) fn plunge_group(ui: &mut Ui, u: Unit, a: &mut Approach) {
    let mut on = a.plunge_height > 0.0;
    let g = group(ui, "Plunge", Some(&mut on), |ui| {
        prop_row_hint(ui, "Height above material", "rapid down to this height over the material, feed only the rest", |ui| {
            num(ui, u, &mut a.plunge_height)
        });
    });
    if g.toggled {
        a.plunge_height = if on { 2.0 } else { 0.0 };
    }
}

/// The row's label (#3745).
pub(crate) const APPROACH_DISTANCE_LABEL: &str = "Approach distance";

/// The row's hover text (#3745).
pub(crate) const APPROACH_DISTANCE_HINT: &str =
    "how far outside the stock edge the tool goes down to the cut depth, in air, before it moves in to the first cut";

/// Where the entry of an Inside or Outside pass starts (#3745): a
/// distance outside the stock edge. Auto keeps the kind's own entry, so
/// the group shows only for Inside and Outside.
pub(super) fn approach_distance_group(ui: &mut Ui, u: Unit, start: PassStart, a: &mut Approach) {
    if start == PassStart::Auto {
        return;
    }
    group(ui, "Approach", None, |ui| {
        prop_row_hint(ui, APPROACH_DISTANCE_LABEL, APPROACH_DISTANCE_HINT, |ui| num(ui, u, &mut a.approach_distance));
    });
}

/// The entry moves' own feeds.
pub(super) fn feeds_group(ui: &mut Ui, u: Unit, a: &mut Approach) {
    group(ui, "Entry feeds", None, |ui| {
        auto_value(ui, "Ramp / helix / arc feed", "Auto", &mut a.ramp_feed, "cutting feed", 300.0, |ui, v| {
            num_feed(ui, u, v);
        });
        auto_value(ui, "Entry plunge feed", "Auto", &mut a.plunge_feed, "plunge feed", 150.0, |ui, v| num_feed(ui, u, v));
        prop_row(ui, "Lead-in feed", |ui| num_unit(ui, &mut a.lead_feed_pct, "%"));
        hint(ui, "a share of the cutting feed");
    });
}

/// The descent engagement limit - optional (off writes 0): over it the
/// descent's feeds scale down to it, named in the status.
pub(super) fn engagement_group(ui: &mut Ui, a: &mut Approach) {
    let mut on = a.max_engagement > 0.0;
    let g = group(ui, "Engagement", Some(&mut on), |ui| {
        let mut pct = a.max_engagement * 100.0;
        let before = pct;
        prop_row_hint(
            ui,
            "Limit (of tool Ø)",
            "a descent cuts the full diameter: over the limit its feeds scale down to it, named in the status",
            |ui| num_unit(ui, &mut pct, "%"),
        );
        if pct != before {
            a.max_engagement = pct / 100.0;
        }
    });
    if g.toggled {
        a.max_engagement = if on { 0.5 } else { 0.0 };
    }
    // #1512: said whether the group is open or not - this limit slows the
    // way in, it never moves a pass.
    hint(ui, ENGAGEMENT_SCOPE);
}

/// What the entry Engagement limit does and does not change (#1512).
pub(super) const ENGAGEMENT_SCOPE: &str =
    "entry feeds only: the path keeps its shape - the side load of the passes is Stepover or Optimal load on Passes";

fn lead_idx(l: Option<Lead>) -> usize {
    match l {
        None => 0,
        Some(Lead::None) => 1,
        Some(Lead::Linear { .. }) => 2,
        Some(Lead::Arc { .. }) => 3,
        Some(Lead::Tangential { .. }) => 4,
        Some(Lead::Perpendicular { .. }) => 5,
    }
}

/// The contour's lead geometry beyond the lead-in's shape.
pub(super) fn contour_leads_editor(
    ui: &mut Ui,
    u: Unit,
    a: &mut Approach,
    picks: &mut camcraft_app::path_picks::PathPicks,
) {
    group(ui, "Lead out", None, |ui| {
        let mut i = lead_idx(a.lead_out);
        let before = i;
        prop_row_hint(ui, "Type", "every lead and extension stays in the legal room - shrunk, then refused by name", |ui| {
            combo(ui, "c_lead_out", &mut i, &LEAD_OUTS)
        });
        if i != before {
            a.lead_out = match i {
                1 => Some(Lead::None),
                2 => Some(Lead::Linear { length: 8.0 }),
                3 => Some(Lead::Arc { radius: 5.0 }),
                4 => Some(Lead::Tangential { radius: 5.0 }),
                5 => Some(Lead::Perpendicular { length: 5.0 }),
                _ => None,
            };
        }
        match &mut a.lead_out {
            Some(Lead::Linear { length } | Lead::Perpendicular { length }) => {
                prop_row(ui, "Lead-out length", |ui| num(ui, u, length));
            }
            Some(Lead::Arc { radius } | Lead::Tangential { radius }) => {
                prop_row(ui, "Lead-out arc radius", |ui| num(ui, u, radius));
            }
            Some(Lead::None) | None => {}
        }
    });
    group(ui, "Extensions", None, |ui| {
        prop_row(ui, "Before lead-in", |ui| num(ui, u, &mut a.ext_in));
        prop_row(ui, "After lead-out", |ui| num(ui, u, &mut a.ext_out));
        prop_row(ui, "Vertical lead-in height", |ui| num(ui, u, &mut a.lead_height));
        prop_row(ui, "Overlap past the start", |ui| num(ui, u, &mut a.overlap));
    });
    group(ui, "Axis-aligned start", Some(&mut a.axis_start), |ui| {
        prop_row_hint(
            ui,
            "Axis tolerance",
            "starts each ring on an X- or Y-aligned run, so an Arc entry posts as one G18/G19 arc",
            |ui| num_unit(ui, &mut a.axis_tol, "°"),
        );
    });
    // #1448: the user's start and exit points, typed in model X/Y.
    super::start_exit_rows::start_exit_groups(ui, u, picks);
    // M13.11: where the cutter radius comes off - the computer walks
    // the centre (the simulation and every file before the option), the
    // control offsets the part contour by its register.
    group(ui, "Cutter compensation", None, |ui| {
        let mut i = match a.compensation {
            Compensation::Computer => 0,
            Compensation::Control => 1,
            Compensation::Wear => 2,
        };
        let before = i;
        prop_row_hint(
            ui,
            "Compensation",
            "In computer: the cutter-centre path is posted. In control: the part contour is posted under G41/G42 with the tool's offset register (D = tool number) holding the full radius. Wear: the cutter-centre path is posted under G41/G42 and the register holds only the wear delta (zero on a new cutter)",
            |ui| combo(ui, "c_comp", &mut i, &COMPS),
        );
        if i != before {
            a.compensation = match i {
                1 => Compensation::Control,
                2 => Compensation::Wear,
                _ => Compensation::Computer,
            };
        }
        if a.compensation == Compensation::Wear {
            consequence(ui, "set the D register to the wear delta only - a full radius there cuts the part a radius undersize");
        }
        if a.compensation.in_control() {
            consequence(ui, "needs a Perpendicular lead at least a tool radius long; a Linear lead, tabs, corner blends, overlap, keep-down links and non-Fanuc posts are refused by name");
        }
    });
}

/// The entry fallbacks of a generated path, as lines for the op page's
/// status banner (M11.3, the user's "say it on the op page"): every entry
/// substitution, second dive, centre-cutting fallback and lifted link the
/// generator named, each with the fix a user can make. A stand-in entry
/// (an entry that is not the selected one, #3884) is a warning line.
#[must_use]
pub(crate) fn entry_status_lines(notes: &[String]) -> Vec<String> {
    let mut out = Vec::new();
    for n in notes {
        if n.starts_with(camcraft_app::operation::TOP_STOCK_WARNING) || n.starts_with(camcraft_app::machining2d::PASS_START_NOTE) {
            out.push(format!("warning: {n}"));
            continue;
        }
        if n.starts_with(camcraft_app::finishing3d::WATERLINES_OFF_NOTE) {
            out.push(format!("warning: {n}"));
            continue;
        }
        let stand_in = camcraft_app::machining2d::is_entry_stand_in(n);
        let fix = if n.contains("not centre-cutting") {
            "fix: a centre-cutting tool, or a Helix / Ramp entry that fits"
        } else if n.contains("second dive") {
            "fix: a helix radius that fits at the first cut's start, or Keep down links"
        } else if n.contains("axial dive") || n.contains("fits nowhere") || n.contains("stands down to a ramp") {
            "fix: a smaller helix radius (or a minimum radius), a pre-drilled start hole, or a smaller tool"
        } else if n.contains("horizontal arc") {
            "fix: a smaller arc radius or sweep, or a smaller stepover"
        } else if n.starts_with("link: the tool lifted") {
            "fix: Retract plane links, or a smaller stepover so the cuts it crosses are already cleared"
        } else if stand_in {
            "fix: set the stock so that an Arc entry can start in the air outside it, or a smaller stepover"
        } else {
            continue;
        };
        let body = n.strip_prefix("entry: ").unwrap_or(n);
        // #3884: an entry that is not the selected one is a warning line.
        let warn = if stand_in { "warning: " } else { "" };
        out.push(format!("{warn}Entry: {body} - {fix}"));
    }
    out
}

#[cfg(test)]
mod top_stock_tests {
    #[test]
    fn the_top_stock_note_is_a_visible_warning_not_an_entry_fallback() {
        let note = format!("{} top Z5.000, stock Z10.000 - raise Top or face first", camcraft_app::operation::TOP_STOCK_WARNING);
        assert_eq!(super::entry_status_lines(std::slice::from_ref(&note)), vec![format!("warning: {note}")]);
    }

    #[test]
    fn the_waterlines_off_note_is_a_visible_warning() {
        let note = format!("{} 6 steep flank facet(s) (>45 deg) left uncut", camcraft_app::finishing3d::WATERLINES_OFF_NOTE);
        assert_eq!(super::entry_status_lines(std::slice::from_ref(&note)), vec![format!("warning: {note}")]);
    }

    /// #3884 pin 3: each stand-in entry that the ladder names is a warning
    /// line; the marker notes add no second line (the op's warning says it).
    #[test]
    fn each_stand_in_entry_is_a_warning_line() {
        use camcraft_app::machining2d::{ENTRY_STAND_IN_NOTE, PLUNGE_INTO_STOCK_NOTE};
        let site = "the pass start (10.246, 16.000) at Z -3.000 is in material and the arc's lead room is blocked";
        let notes = vec![
            format!("entry: {site} - ramp along the pass instead"),
            format!("entry: {site}, and no ramp fits clear of prior cuts - plunged into material at the plunge feed"),
            "entry: Outside pass - no air entry outside the stock box fits (why) - helix entry instead".to_owned(),
            "entry: Outside pass - no air entry outside the stock box fits (why) - ramp along the first cut instead".to_owned(),
            ENTRY_STAND_IN_NOTE.to_owned(),
            PLUNGE_INTO_STOCK_NOTE.to_owned(),
        ];
        let lines = super::entry_status_lines(&notes);
        assert_eq!(lines.len(), 4, "{lines:?}");
        assert!(lines.iter().all(|l| l.starts_with("warning: Entry: ")), "{lines:?}");
        assert!(lines[0].contains("ramp along the pass instead"), "{lines:?}");
    }
}
