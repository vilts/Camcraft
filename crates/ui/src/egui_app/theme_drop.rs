//! The ribbon's Theme door (#441): the user's look moved off the
//! Stock page onto its own ribbon cell, right of the View door. Its list
//! offers the switcher rows `State::theme_options` builds (the two
//! built-ins, the discovered `assets/themes/*.json` files by name, and
//! Browse for any other file), then the colour-vision modes. A pick
//! queues `SelectTheme` / `SelectThemeDialog` / `SelectColorVision` -
//! the same actions the old page combo queued. The pick is a user
//! preference (#877): kept per user across restarts, never on the undo
//! history; opening and closing the list is view state.
//! Provenance: our own code, the hand-drawn dropdown law of the View
//! and machine doors (panes.rs, layout/mod.rs); no HSMWorks material.

use camcraft_app::commands::{color_vision_of, theme_of};
use camcraft_app::schema::{ColorVision, ThemeRef};
use egui::{Rect, Sense, Ui, Vec2};

use super::panes::{DROP_GAP, DROP_W};
use super::theme::Tokens;
use crate::state::{CommandKind, State};

/// One row of the Theme door's list.
#[derive(Clone, Debug, PartialEq)]
pub(super) enum ThemeRow {
    /// A theme switcher row: its label and the reference it selects
    /// (`None`: the Browse row, which opens the file pick).
    Theme(String, Option<ThemeRef>),
    /// The inert caption over the colour-vision rows.
    VisionCaption,
    /// One colour-vision mode.
    Vision(ColorVision),
}

/// The command a theme row queues.
fn command_of(pick: Option<&ThemeRef>) -> CommandKind {
    match pick {
        Some(r) => CommandKind::SelectTheme(r.clone()),
        None => CommandKind::SelectThemeDialog,
    }
}

/// The door's rows, pure geometry in the strip's global frame (the View
/// door's law): hung under the button, pulled left so the list never
/// runs off the strip's right edge. The tests click through these rects.
pub(super) fn rows_of(state: &State, button: Rect, strip: Rect, pal: &Tokens) -> Vec<(ThemeRow, Rect)> {
    let mut rows: Vec<ThemeRow> = state
        .theme_options()
        .into_iter()
        .filter_map(|(label, cmd)| match cmd {
            CommandKind::SelectTheme(r) => Some(ThemeRow::Theme(label, Some(r))),
            CommandKind::SelectThemeDialog => Some(ThemeRow::Theme(label, None)),
            _ => None,
        })
        .collect();
    rows.push(ThemeRow::VisionCaption);
    rows.extend(ColorVision::ALL.into_iter().map(ThemeRow::Vision));
    let left = button.left().min(strip.right() - DROP_W).max(strip.left());
    rows.into_iter()
        .enumerate()
        .map(|(k, r)| {
            let min = egui::Pos2::new(left, button.bottom() + DROP_GAP + k as f32 * pal.row_height);
            (r, Rect::from_min_size(min, Vec2::new(DROP_W, pal.row_height)))
        })
        .collect()
}

/// The open Theme door's dropdown: a foreground Area of rows, the
/// current theme and mode marked. A press on a live row queues its
/// command and closes the door; an outside press closes it.
pub(super) fn show(
    ui: &mut Ui,
    state: &mut State,
    pal: &Tokens,
    id: egui::Id,
    button: Rect,
    rows: &[(ThemeRow, Rect)],
) {
    let Some(top) = rows.first().map(|(_, r)| r.min) else {
        state.ribbon_theme_open = false;
        return;
    };
    let panel = Rect::from_min_size(top, Vec2::new(DROP_W, rows.len() as f32 * pal.row_height));
    let theme = theme_of(&state.world);
    let vision = color_vision_of(&state.world);
    let mut chose: Option<Option<CommandKind>> = None;
    egui::Area::new(id)
        .fixed_pos(panel.min)
        .order(egui::Order::Foreground)
        .movable(false)
        .show(ui.ctx(), |ui| {
            let p = ui.painter();
            p.rect_filled(panel, pal.radius_menu, pal.ink);
            p.rect_stroke(
                panel,
                pal.radius_menu,
                egui::Stroke::new(1.0, pal.border),
                egui::StrokeKind::Outside,
            );
            for (k, (row, r)) in rows.iter().enumerate() {
                let resp = ui.interact(*r, id.with(("theme-row", k)), Sense::click());
                let (label, current, live) = match row {
                    ThemeRow::Theme(label, pick) => (label.clone(), pick.as_ref() == Some(&theme), true),
                    ThemeRow::VisionCaption => ("Colour vision".to_owned(), false, false),
                    ThemeRow::Vision(mode) => (format!("  {}", mode.label()), *mode == vision, true),
                };
                if resp.hovered() && live {
                    p.rect_filled(*r, 0.0, pal.field_hover);
                }
                if matches!(row, ThemeRow::VisionCaption) {
                    p.hline(r.x_range(), r.top(), egui::Stroke::new(1.0, pal.border));
                }
                let text = if current { format!("{label}  (current)") } else { label };
                let checked = matches!(row, ThemeRow::Theme(_, Some(_)) | ThemeRow::Vision(_))
                    .then_some(current);
                super::painted::menu_item(&resp, text.trim(), live, checked);
                p.text(
                    r.left_center() + Vec2::new(8.0, 0.0),
                    egui::Align2::LEFT_CENTER,
                    &text,
                    egui::FontId::proportional(pal.font_row),
                    if live && !current { pal.text } else { pal.text_faint },
                );
                if resp.clicked() && live {
                    // The current row is a choice too: it closes the
                    // list and queues nothing.
                    chose = Some(match row {
                        ThemeRow::Theme(_, pick) if !current => Some(command_of(pick.as_ref())),
                        ThemeRow::Vision(mode) if !current => Some(CommandKind::SelectColorVision(*mode)),
                        _ => None,
                    });
                }
            }
        });
    match chose {
        Some(cmd) => {
            if let Some(cmd) = cmd {
                state.pending.push_back(cmd);
            }
            state.ribbon_theme_open = false;
        }
        None => {
            let outside = ui.input(|i| {
                i.pointer.button_pressed(egui::PointerButton::Primary)
                    && i.pointer.interact_pos().is_none_or(|p| {
                        !button.expand(DROP_GAP).contains(p) && !rows.iter().any(|(_, r)| r.contains(p))
                    })
            });
            if outside {
                state.ribbon_theme_open = false;
            }
        }
    }
}
