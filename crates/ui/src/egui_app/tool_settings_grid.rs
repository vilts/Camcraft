//! The tool settings' property grid (#2014): every field of the selected
//! tool as a label-left row of `layout::form`, in four collapsible
//! groups - Geometry, Holder, Cutting data and Catalog. Draft fields
//! only: Apply / Add carry them through `ChangeTool` / `AddTool`, as
//! before. #3930: Properties draws the grid for a new tool only; a saved
//! tool's groups are drawn by the tool library window (the Tool tab, its
//! Body section, the Recipes tab and the Details tab).
//!
//! Provenance: our own layout over the tool draft, on the form law in
//! `layout/form.rs` (the generic pattern of desktop property inspectors).
//! No vendor material involved.

use camcraft_app::tools;
use egui::Ui;

use crate::state::State;

use super::fields::{check_row, combo, int_range_row, num_feed, num_plain_row, num_row};
use super::layout::{CTRL_W, group_open, prop_row, prop_row_hint, text_row};

/// Tool kind labels, same order as `ToolKind` (mirrors `ToolUi::from_tool`).
const TOOL_KINDS: [(tools::ToolKind, &str); 23] = [
    (tools::ToolKind::EndMill, "End mill"),
    (tools::ToolKind::BallNose, "Ball nose"),
    (tools::ToolKind::BullNose, "Bull nose"),
    (tools::ToolKind::Drill, "Drill"),
    (tools::ToolKind::Tap, "Tap"),
    (tools::ToolKind::Bore, "Bore"),
    (tools::ToolKind::Turning, "Turning insert"),
    (tools::ToolKind::Groove, "Grooving blade"),
    (tools::ToolKind::Thread, "Threading insert"),
    (tools::ToolKind::Form, "Form gauge"),
    (tools::ToolKind::Taper, "Chamfer mill"),
    (tools::ToolKind::Probe, "Touch probe"),
    // #670: the kerf is its diameter; it turns no spindle.
    (tools::ToolKind::Torch, "Plasma torch"),
    // #541: diameter = X size, width = Y size; it turns no spindle.
    (tools::ToolKind::Electrode, "Sinker electrode"),
    // #894: rows of teeth at one pitch / one row over a pitch range.
    (tools::ToolKind::ThreadMill, "Thread mill"),
    (tools::ToolKind::ThreadMillSingle, "Single-form thread mill"),
    (tools::ToolKind::Waterjet, "Waterjet nozzle"),
    // #2011: diameter = wire diameter in mm; it turns no spindle.
    (tools::ToolKind::Wire, "EDM wire"),
    // #1444: the undercut cutters - a head on a narrower neck (the
    // shank fields); the dovetail's tip angle is its flank angle.
    (tools::ToolKind::TSlot, "T-slot cutter"),
    (tools::ToolKind::Keyseat, "Keyseat cutter"),
    (tools::ToolKind::Dovetail, "Dovetail cutter"),
    (tools::ToolKind::Lollipop, "Lollipop cutter"),
    // #3369: a router-style cutter whose edge is a drawn or preset form.
    (tools::ToolKind::FormMill, "Form mill"),
];

/// The settings grid: the groups, in the order the tool reads.
pub(super) fn grid(ui: &mut Ui, state: &mut State) {
    if !state.tool_ui.exists {
        // Typing the name is not an event; it rides the Add command.
        text_row(ui, "Name", &mut state.tool_ui.name, "tool_name", CTRL_W * 1.3);
    }
    group_open(ui, "Geometry", None, true, |ui| geometry(ui, state));
    group_open(ui, "Holder", None, true, |ui| holder(ui, state));
    group_open(ui, "Cutting data", None, true, |ui| cutting(ui, state));
    group_open(ui, "Catalog", None, true, |ui| catalog(ui, state));
}

/// #3930: the name of the tool kind at `index` (the draft's `kind`), for
/// the Properties summary.
pub(super) fn kind_name(index: usize) -> &'static str {
    TOOL_KINDS.get(index).map_or("Tool", |k| k.1)
}

/// Type, the cutting shape and the tool body's lengths. #3893: the tool
/// library window's Tool tab draws these same rows.
pub(super) fn geometry(ui: &mut Ui, state: &mut State) {
    let u = state.units();
    let exists = state.tool_ui.exists;
    let before = state.tool_ui.kind;
    let offered: Vec<usize> = TOOL_KINDS
        .iter()
        .enumerate()
        .filter(|&(i, (kind, _))| {
            crate::machine_modes::tool_kind_offered(&state.world, *kind) || (exists && i == before)
        })
        .map(|(i, _)| i)
        .collect();
    let labels: Vec<&str> = offered.iter().map(|&i| TOOL_KINDS[i].1).collect();
    let mut sel = offered.iter().position(|&i| i == before).unwrap_or(0);
    prop_row(ui, "Type", |ui| combo(ui, "tool_kind", &mut sel, &labels));
    if let Some(&chosen) = offered.get(sel) {
        state.tool_ui.kind = chosen;
    }
    if state.tool_ui.kind != before {
        super::tool_settings::seed_kind(state, before);
    }
    match state.tool_ui.tool_kind() {
        tools::ToolKind::Electrode => {
            // #541: the burning face's two sides.
            prop_row_hint(ui, "Electrode X size", "the burning face's width across X", |ui| {
                num_row(ui, u, &mut state.tool_ui.diameter)
            });
            prop_row_hint(ui, "Electrode Y size", "the burning face's height across Y", |ui| {
                num_row(ui, u, &mut state.tool_ui.width)
            });
        }
        // #669 / #670: the kerf width IS the torch's diameter.
        tools::ToolKind::Torch | tools::ToolKind::Waterjet => {
            prop_row_hint(ui, "Kerf width", "the cut's width; confirm it with a test cut", |ui| {
                num_row(ui, u, &mut state.tool_ui.diameter)
            });
        }
        tools::ToolKind::Wire => {
            prop_row_hint(
                ui,
                "Wire diameter",
                "the wire diameter in mm; matched against the machine's edm.wire_diameters",
                |ui| num_row(ui, u, &mut state.tool_ui.diameter),
            );
        }
        // #3369: a form mill's diameter is its form's widest cut.
        tools::ToolKind::FormMill => form_length_row(ui, state, "Diameter", state.tool_ui.diameter),
        _ => {
            prop_row(ui, "Diameter", |ui| num_row(ui, u, &mut state.tool_ui.diameter));
        }
    }
    // Kind-read fields (the inert-unless-the-kind-reads-them law): a point
    // angle means nothing on an end mill, a pitch means nothing on a drill.
    kind_rows(ui, state);
    // #630: which point of the nose the lathe posts write.
    super::tool_orientation_row::orientation_row(ui, state);
    // #894: a thread mill's pitch (range) and form angle.
    super::tool_thread_mill_row::thread_mill_rows(ui, state);
    // #1444: a dovetail cutter's flank angle.
    super::tool_undercut_row::dovetail_row(ui, state);
    if state.tool_ui.tool_kind() == tools::ToolKind::BallNose {
        // #263: a ball's corner IS its ball, half the diameter - shown,
        // never edited, since no reader takes any other number.
        prop_row_hint(ui, "Corner radius", "half the diameter - a ball nose's whole tip is its ball", |ui| {
            ui.label(format!("{} {}", camcraft_app::units::len(state.tool_ui.diameter * 0.5, u, 3), u.suffix()));
        });
    } else if !matches!(
        state.tool_ui.tool_kind(),
        tools::ToolKind::Tap
            | tools::ToolKind::Torch
            | tools::ToolKind::Waterjet
            | tools::ToolKind::Electrode
            | tools::ToolKind::Wire
            | tools::ToolKind::ThreadMill
            | tools::ToolKind::ThreadMillSingle
            | tools::ToolKind::TSlot
            | tools::ToolKind::Keyseat
            | tools::ToolKind::Dovetail
            | tools::ToolKind::Lollipop
            | tools::ToolKind::FormMill
    ) {
        prop_row(ui, "Corner radius", |ui| num_row(ui, u, &mut state.tool_ui.corner_r));
    }
    if state.tool_ui.tool_kind() == tools::ToolKind::FormMill {
        form_length_row(ui, state, "Flute length", state.tool_ui.flute_len);
    } else {
        prop_row(ui, "Flute length", |ui| num_row(ui, u, &mut state.tool_ui.flute_len));
    }
    prop_row(ui, "Shank diameter", |ui| num_row(ui, u, &mut state.tool_ui.shank_d));
    prop_row(ui, "Shank length", |ui| num_row(ui, u, &mut state.tool_ui.shank_len));
}

/// The rows each tool kind reads, shown only where they feed a generator
/// or the collision profile.
pub(super) fn kind_rows(ui: &mut Ui, state: &mut State) {
    let u = state.units();
    let kind = state.tool_ui.kind;
    if kind == 3 {
        prop_row_hint(ui, "Point angle (deg)", "cone tip, 60 to 180; standard twist drill is 118", |ui| {
            num_plain_row(ui, &mut state.tool_ui.tip_angle)
        });
    }
    if kind == 4 {
        prop_row_hint(ui, "Thread pitch", "distance advanced per turn - M6 is 1.0, M8 is 1.25", |ui| {
            num_row(ui, u, &mut state.tool_ui.pitch)
        });
        let mut left = state.tool_ui.thread_hand == camcraft_app::tools::ThreadHand::Left;
        check_row(ui, "Left-hand tap (posting unavailable)", &mut left);
        state.tool_ui.thread_hand = if left {
            camcraft_app::tools::ThreadHand::Left
        } else {
            camcraft_app::tools::ThreadHand::Right
        };
    }
    if kind == 5 {
        prop_row_hint(ui, "Bar nose radius", "the ground tip circle the bar rides the hole wall on; inert on drilling cycles", |ui| {
            num_row(ui, u, &mut state.tool_ui.nose_radius)
        });
    }
    if kind == 6 {
        prop_row_hint(ui, "Insert nose radius", "the corner circle ground into the tip - it IS the cutting ball", |ui| {
            num_row(ui, u, &mut state.tool_ui.nose_radius)
        });
    }
    if kind == 7 {
        prop_row_hint(ui, "Blade width", "the cutting edge along the axis - the kerf's width IS the tool", |ui| {
            num_row(ui, u, &mut state.tool_ui.width)
        });
    }
    if kind == 8 {
        prop_row_hint(ui, "Blade width", "the V's crest width across the axis - 0.866 x the pitch it threads", |ui| {
            num_row(ui, u, &mut state.tool_ui.width)
        });
        prop_row_hint(ui, "Form angle (deg)", "the included angle of the V's flanks - 60 for the metric and unified forms", |ui| {
            num_plain_row(ui, &mut state.tool_ui.tip_angle)
        });
    }
    if kind == 9 {
        prop_row_hint(ui, "Band width", "the crown's ground edge across the axis - inside twice the crown radius", |ui| {
            num_row(ui, u, &mut state.tool_ui.width)
        });
        prop_row_hint(ui, "Crown radius", "the arc of the ground edge in the work's meridian", |ui| {
            num_row(ui, u, &mut state.tool_ui.nose_radius)
        });
    }
    if kind == 10 {
        prop_row_hint(ui, "Cone angle (deg)", "the included angle of the taper - 90 cuts a 45-degree face", |ui| {
            num_plain_row(ui, &mut state.tool_ui.tip_angle)
        });
        // #3368: the tip form, and a flat tip's diameter.
        super::tool_taper_tip_row::tip_rows(ui, state);
    }
    // #3369: a form mill's preset, its numbers and the pilot.
    super::tool_form_mill_row::rows(ui, state);
}

/// A length a form mill's outline sets: shown, never typed (#3369).
fn form_length_row(ui: &mut Ui, state: &State, label: &str, v: f64) {
    let u = state.units();
    prop_row_hint(ui, label, "set by the form outline", |ui| {
        ui.label(format!("{} {}", camcraft_app::units::len(v, u, 3), u.suffix()));
    });
}

/// The holder's two cylinders. #2016: when the editor's holder is a
/// library entry, the entry sizes itself - shown, never typed. #3930: the
/// tool library window's Body section draws these same rows.
pub(super) fn holder(ui: &mut Ui, state: &mut State) {
    let u = state.units();
    if let Some((entry, d, len)) = state.editor_library_holder() {
        let from = format!("from the library holder {entry} (Holder and stick-out)");
        prop_row(ui, "Holder diameter", |ui| {
            ui.label(format!("{} {}", camcraft_app::units::len(d, u, 3), u.suffix()));
            ui.label(&from);
        });
        prop_row(ui, "Holder length", |ui| {
            ui.label(format!("{} {}", camcraft_app::units::len(len, u, 3), u.suffix()));
            ui.label(from);
        });
    } else {
        prop_row_hint(
            ui,
            "Holder diameter",
            "holder cylinder diameter used when no library holder or custom profile is set",
            |ui| num_row(ui, u, &mut state.tool_ui.holder_d),
        );
        prop_row_hint(
            ui,
            "Holder length",
            "holder cylinder length used when no library holder or custom profile is set",
            |ui| num_row(ui, u, &mut state.tool_ui.holder_len),
        );
    }
}

/// Speeds, feeds and the per-material table.
fn cutting(ui: &mut Ui, state: &mut State) {
    let u = state.units();
    let kind = state.tool_ui.tool_kind();
    let torch = matches!(kind, tools::ToolKind::Torch | tools::ToolKind::Waterjet);
    let wire = kind == tools::ToolKind::Wire;
    speeds(ui, state);
    if !wire {
        super::tool_cutting_rows::cutting_rows(ui, &mut state.tool_ui, u, torch);
    }
    lathe_and_entry(ui, state);
}

/// The tool's own spindle rpm and feed (the rows' fallback).
fn speeds(ui: &mut Ui, state: &mut State) {
    let u = state.units();
    let kind = state.tool_ui.tool_kind();
    if !matches!(
        kind,
        tools::ToolKind::Torch | tools::ToolKind::Waterjet | tools::ToolKind::Electrode | tools::ToolKind::Wire
    ) {
        prop_row(ui, "Spindle rpm", |ui| num_plain_row(ui, &mut state.tool_ui.rpm));
    }
    prop_row(ui, &format!("Feed ({})", u.feed_suffix()), |ui| num_feed(ui, u, &mut state.tool_ui.feed));
}

/// #3930: the Cutting data rows that are not the per-material table - the
/// tool's own rpm and feed, the lathe rows and centre cutting. The tool
/// library window's Recipes tab draws them under its table.
pub(super) fn cutting_base(ui: &mut Ui, state: &mut State) {
    speeds(ui, state);
    lathe_and_entry(ui, state);
}

/// The lathe steel's rows and the centre-cutting flag.
fn lathe_and_entry(ui: &mut Ui, state: &mut State) {
    // #891: a lathe steel's surface speed, feed per rev, clamp.
    super::tool_turning_row::turning_rows(ui, state);
    // M11.3: an entry may plunge into solid only with a centre-cutting
    // tool; otherwise it helixes, ramps, or the operation is refused.
    check_row(ui, "Centre cutting (may plunge into solid)", &mut state.tool_ui.center_cutting);
}

/// The catalog facts, the workpiece material and the tool-table number.
/// #3893: the tool library window's Details tab draws these same rows.
pub(super) fn catalog(ui: &mut Ui, state: &mut State) {
    text_row(ui, "Material group", &mut state.tool_ui.material_group, "tool_material_group", CTRL_W * 1.3);
    super::layout::hint(ui, "workpiece material these base speeds and feeds are set for");
    super::tool_catalog_rows::catalog_rows(ui, &mut state.tool_ui);
    // #221 / #2442: the machine's tool-table number - the post's T and H
    // words. New tools take the lowest unused number when 0.
    let min = if state.tool_ui.exists { 1 } else { 0 };
    let help = if state.tool_ui.exists {
        "controller tool number (T and H words)"
    } else {
        "controller tool number (T and H words); leave at 0 to assign the next unused number"
    };
    prop_row_hint(ui, "Tool number", help, |ui| {
        int_range_row(ui, &mut state.tool_ui.number, min..=camcraft_app::tool_numbers::MAX)
    });
}
