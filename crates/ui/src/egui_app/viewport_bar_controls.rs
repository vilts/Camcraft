//! The Move / Rotate bar controls (#1198 slice d): axis selector (X/Y/Z),
//! typed distance or angle entry (applied via the same `MoveModelBy` /
//! `TurnModelBy` commands as a handle drag, and synced live during drags),
//! and the Shift-snap step parameter with its defaults (1.0 mm / 90.0 deg).
//!
//! Provenance: our own viewport transform controls over egui widgets; no
//! vendor material consulted.

use camcraft_app::units::{self, Unit};

use crate::state::{CommandKind, ErrorScope, State};

/// Default grid step for Shift-snapped Move drags, mm.
pub(crate) const DEFAULT_MOVE_SNAP_MM: f64 = 1.0;
/// Default angle step for Shift-snapped Rotate drags, degrees (#1198).
pub(crate) const DEFAULT_ROTATE_SNAP_DEG: f64 = 90.0;
/// Lower and upper bounds on the bar's Move snap parameter, mm.
pub(crate) const MIN_SNAP_MM: f64 = 0.001;
pub(crate) const MAX_SNAP_MM: f64 = 10_000.0;
/// Lower and upper bounds on the bar's Rotate snap parameter, degrees.
pub(crate) const MIN_SNAP_DEG: f64 = 0.1;
pub(crate) const MAX_SNAP_DEG: f64 = 360.0;

const AXIS_NAMES: [&str; 3] = ["X", "Y", "Z"];

/// Ephemeral view state for the Move / Rotate strip of the bottom bar.
#[derive(Clone, Debug, PartialEq)]
pub struct HandleBarState {
    /// Choose the typed angle editor without hiding either handle set.
    pub turn_value: bool,
    /// Active axis for typed entry (0 = X, 1 = Y, 2 = Z); updated when a
    /// handle is grabbed or an axis button is clicked.
    pub axis: usize,
    /// Distance field text in the active display unit (updated live during
    /// a handle drag, committed on Enter or Set).
    pub move_draft: String,
    /// Angle field text in degrees (updated live during a ring drag,
    /// committed on Enter or Set).
    pub rotate_draft: String,
    /// Grid snap step used when holding Shift during a Move drag, mm.
    pub move_snap_mm: f64,
    /// Angle snap step used when holding Shift during a Rotate drag, deg.
    pub rotate_snap_deg: f64,
}

impl Default for HandleBarState {
    fn default() -> Self {
        Self {
            turn_value: false,
            axis: 0,
            move_draft: String::new(),
            rotate_draft: String::new(),
            move_snap_mm: DEFAULT_MOVE_SNAP_MM,
            rotate_snap_deg: DEFAULT_ROTATE_SNAP_DEG,
        }
    }
}

/// Snap `raw` to the nearest multiple of `step`; non-finite or non-positive
/// steps return `raw` unchanged.
#[must_use]
pub(crate) fn snap_step(raw: f64, step: f64) -> f64 {
    if !raw.is_finite() || !step.is_finite() || step <= 0.0 {
        return raw;
    }
    let out = (raw / step).round() * step;
    if !out.is_finite() {
        raw
    } else if out == 0.0 {
        0.0
    } else {
        out
    }
}

/// Mirror a live Move drag's axis and signed distance into the bar.
pub(crate) fn sync_move_drag(state: &mut State, axis: usize, delta_mm: f64) {
    state.handle_bar.turn_value = false;
    state.handle_bar.axis = axis.min(2);
    state.handle_bar.move_draft = units::len_signed(delta_mm, state.units(), 3);
}

/// Mirror a live Rotate drag's axis and signed angle into the bar.
pub(crate) fn sync_turn_drag(state: &mut State, axis: usize, angle_rad: f64) {
    state.handle_bar.turn_value = true;
    state.handle_bar.axis = axis.min(2);
    state.handle_bar.rotate_draft = format!("{:+.1}", angle_rad.to_degrees());
}

/// Clear the draft readout when a live drag cancels with no move/turn.
pub(crate) fn clear_live_draft(state: &mut State, move_mode: bool) {
    if move_mode {
        state.handle_bar.move_draft.clear();
    } else {
        state.handle_bar.rotate_draft.clear();
    }
}

/// Parse a bar field, check finiteness, verify a single selected shown part
/// exists, and reject zero values with a clear status line.
fn checked_input(
    state: &mut State,
    raw: &str,
    verb: &str,
    noun: &str,
    min_abs: f64,
    to_world: impl FnOnce(f64) -> f64,
) -> Option<(String, f64)> {
    if raw.is_empty() {
        state.status = format!("part not {verb}: {noun} is zero");
        return None;
    }
    let Ok(parsed) = raw.parse::<f64>() else {
        state.refuse(
            ErrorScope::Setup,
            format!("part not {verb}: '{raw}' is not a finite number"),
        );
        return None;
    };
    if !parsed.is_finite() {
        state.refuse(
            ErrorScope::Setup,
            format!("part not {verb}: '{raw}' is not a finite number"),
        );
        return None;
    }
    let world = to_world(parsed);
    if !world.is_finite() {
        state.refuse(ErrorScope::Setup, format!("part not {verb}: the {noun} must be finite"));
        return None;
    }
    let Some((name, _)) = crate::move_drag::one_part(state) else {
        state.refuse(ErrorScope::Setup, format!("part not {verb}: select one shown part"));
        return None;
    };
    if world.abs() <= min_abs {
        state.status = format!("part not {verb}: {noun} is zero");
        return None;
    }
    Some((name, world))
}

/// Apply the typed Move distance along `state.handle_bar.axis` via the
/// same `MoveModelBy` command a ball drag releases.
pub(crate) fn apply_move_text(state: &mut State) {
    let raw = state.handle_bar.move_draft.trim().to_owned();
    let u = state.units();
    let Some((name, mm)) = checked_input(state, &raw, "moved", "distance", crate::move_drag::MIN_MOVE_MM, |v| {
        if u == Unit::Millimeters { v } else { units::from_display(v, u) }
    }) else {
        return;
    };
    let axis = state.handle_bar.axis;
    let mut delta = [0.0; 3];
    let Some(slot) = delta.get_mut(axis) else {
        state.refuse(ErrorScope::Setup, format!("part not moved: axis {axis} is not X, Y or Z"));
        return;
    };
    *slot = mm;
    state.pending.push_back(CommandKind::MoveModelBy { delta, name });
}

/// Apply the typed Rotate angle about `state.handle_bar.axis` via the same
/// `TurnModelBy` command a ring drag releases.
pub(crate) fn apply_turn_text(state: &mut State) {
    let raw = state.handle_bar.rotate_draft.trim().to_owned();
    let Some((name, rad)) = checked_input(state, &raw, "turned", "angle", crate::turn_drag::MIN_TURN_RAD, f64::to_radians) else {
        return;
    };
    let axis = state.handle_bar.axis;
    if axis > 2 {
        state.refuse(ErrorScope::Setup, format!("part not turned: axis {axis} is not X, Y or Z"));
        return;
    }
    state.pending.push_back(CommandKind::TurnModelBy { name, axis, angle_rad: rad });
}

/// Paint the interactive Move / Rotate controls inside `rect` (the upper
/// strip of the expanded bottom bar).
pub(crate) fn draw(ui: &mut egui::Ui, state: &mut State, rect: egui::Rect) {
    if !state.move_handles && !state.rotate_handles {
        return;
    }
    let mut move_mode = state.move_handles;
    let u = state.units();
    let mut submit = false;
    ui.scope_builder(egui::UiBuilder::new().max_rect(rect), |ui| {
        ui.spacing_mut().item_spacing.x = 4.0;
        let h = ui.spacing().interact_size.y;
        ui.vertical(|ui| {
            if state.move_handles && state.rotate_handles {
                ui.horizontal(|ui| {
                    ui.selectable_value(&mut state.handle_bar.turn_value, false, "Move value")
                        .on_hover_text("Enter a distance; arrows and rings stay visible");
                    ui.selectable_value(&mut state.handle_bar.turn_value, true, "Rotate value")
                        .on_hover_text("Enter an angle; arrows and rings stay visible");
                });
            }
            move_mode = state.move_handles && (!state.rotate_handles || !state.handle_bar.turn_value);
            ui.horizontal_centered(|ui| {
                for (i, letter) in AXIS_NAMES.iter().enumerate() {
                    let tip = if move_mode {
                        format!("Move along {letter} axis")
                    } else {
                        format!("Turn about {letter} axis")
                    };
                    let btn = crate::egui_app::fields::named(
                        ui.selectable_label(state.handle_bar.axis == i, *letter),
                        &format!("Axis {letter}"),
                    )
                    .on_hover_text(tip);
                    if btn.clicked() {
                        state.handle_bar.axis = i;
                    }
                }
                ui.separator();
                let (draft, hint, field_name, suffix, field_tip, set_tip) = if move_mode {
                    (
                        &mut state.handle_bar.move_draft,
                        "0.000",
                        "Move distance",
                        u.suffix(),
                        "Signed distance to move along the chosen axis",
                        "Move the selected part along the chosen axis",
                    )
                } else {
                    (
                        &mut state.handle_bar.rotate_draft,
                        "0.0",
                        "Rotate angle",
                        "\u{b0}",
                        "Signed angle in degrees to turn about the chosen axis",
                        "Turn the selected part about the chosen axis",
                    )
                };
                let edit = crate::egui_app::fields::named(
                    ui.add_sized([58.0, h], egui::TextEdit::singleline(draft).hint_text(hint)),
                    field_name,
                )
                .on_hover_text(field_tip);
                ui.label(suffix);
                let set = ui.button("Set").on_hover_text(set_tip);
                let enter = edit.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                submit = set.clicked() || enter;
                ui.separator();
                ui.label("Snap").on_hover_text("Step size when holding Shift while dragging");
                if move_mode {
                    draw_move_snap(ui, state, u, h);
                } else {
                    let dv = ui.add_sized(
                        [56.0, h],
                        egui::DragValue::new(&mut state.handle_bar.rotate_snap_deg)
                            .speed(1.0)
                            .range(MIN_SNAP_DEG..=MAX_SNAP_DEG)
                            .suffix("\u{b0}"),
                    );
                    crate::egui_app::fields::named(dv, "Rotate snap step")
                        .on_hover_text("Angle step in degrees while holding Shift");
                }
                ui.label(egui::RichText::new("(Shift)").small().weak())
                    .on_hover_text("Hold Shift while dragging a handle to snap");
            });
        });
    });
    if submit {
        if move_mode {
            apply_move_text(state);
        } else {
            apply_turn_text(state);
        }
    }
}

fn draw_move_snap(ui: &mut egui::Ui, state: &mut State, u: Unit, h: f32) {
    if u == Unit::Millimeters {
        let dv = ui.add_sized(
            [62.0, h],
            egui::DragValue::new(&mut state.handle_bar.move_snap_mm)
                .speed(0.1)
                .range(MIN_SNAP_MM..=MAX_SNAP_MM)
                .suffix(" mm"),
        );
        crate::egui_app::fields::named(dv, "Move snap step")
            .on_hover_text("Grid step in mm while holding Shift");
        return;
    }
    let mut disp = units::to_display(state.handle_bar.move_snap_mm, u);
    let lo = units::to_display(MIN_SNAP_MM, u);
    let hi = units::to_display(MAX_SNAP_MM, u);
    let dv = ui.add_sized(
        [62.0, h],
        egui::DragValue::new(&mut disp)
            .speed(0.1 / 25.4)
            .range(lo..=hi)
            .custom_formatter(|n, _| format!("{n:.4}"))
            .suffix(" in"),
    );
    if dv.changed() {
        state.handle_bar.move_snap_mm = units::from_display(disp, u).clamp(MIN_SNAP_MM, MAX_SNAP_MM);
    }
    crate::egui_app::fields::named(dv, "Move snap step")
        .on_hover_text("Grid step in inches while holding Shift");
}
