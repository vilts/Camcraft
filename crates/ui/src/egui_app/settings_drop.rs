//! Top-bar Settings dropdown (#2428 Part B): hosts per-user immediate
//! preferences (`prefs.json`) such as the startup update check toggle.
//!
//! Provenance: our own top-band button and foreground `Area` dropdown
//! following `home.rs`, `theme_drop.rs`, and `layout::view_drop`; no vendor
//! material consulted.

use egui::{Button, Rect, RichText, Sense, Ui, Vec2};

use super::theme::Tokens;
use crate::state::State;

/// Accessible button label in the ribbon's top band.
pub(super) const SETTINGS: &str = "Settings";
/// Accessible menu row label for the startup update check preference.
pub(super) const CHECK_FOR_UPDATES: &str = "Check for updates";

/// Draw the `"Settings"` button right-aligned to `right` inside `band`.
/// Returns `(rect.left(), rect)` so items to its left and its dropdown align.
pub(super) fn button(ui: &mut Ui, state: &mut State, pal: &Tokens, band: Rect, right: f32) -> (f32, Rect) {
    let label = RichText::new(SETTINGS).size(pal.font_label).color(pal.text);
    let text_w = ui
        .painter()
        .layout_no_wrap(SETTINGS.into(), egui::FontId::proportional(pal.font_label), pal.text)
        .size()
        .x;
    let width = text_w + 2.0 * pal.button_padding_x + 8.0;
    let rect = Rect::from_min_max(
        egui::pos2(right - width, band.top() + 2.0),
        egui::pos2(right, band.bottom() - 2.0),
    );
    let resp = ui
        .put(rect, Button::new(label).selected(state.ribbon_settings_open))
        .on_hover_text("Preferences");
    if resp.clicked() {
        state.ribbon_settings_open = !state.ribbon_settings_open;
        if state.ribbon_settings_open {
            state.ribbon_more = None;
            state.ribbon_mode_open = false;
            state.ribbon_view_open = false;
            state.ribbon_theme_open = false;
            state.ribbon_recent_open = None;
        }
    }
    (rect.left(), rect)
}

/// Draw the `"Settings"` dropdown when open, using the theme's dropdown metrics.
pub(super) fn show(ui: &mut Ui, state: &mut State, pal: &Tokens, id: egui::Id, button: Rect, strip: Rect) {
    let g = pal.dropdown();
    let w = g.fit_width(g.menu_width, strip.width());
    let left = g.fit_left(button.left(), strip, w);
    let row = Rect::from_min_size(
        egui::pos2(left, button.bottom() + g.gap),
        Vec2::new(w, pal.row_height),
    );
    let panel = row;
    let mut toggled = false;
    egui::Area::new(id)
        .fixed_pos(panel.min)
        .order(egui::Order::Foreground)
        .movable(false)
        .show(ui.ctx(), |ui| {
            ui.expand_to_include_rect(panel);
            let p = ui.painter();
            p.rect_filled(panel, pal.radius_menu, pal.ink);
            p.rect_stroke(
                panel,
                pal.radius_menu,
                egui::Stroke::new(1.0, pal.border),
                egui::StrokeKind::Outside,
            );
            let resp = ui.interact(row, id.with("check-updates"), Sense::click());
            if resp.hovered() {
                p.rect_filled(row, 0.0, pal.field_hover);
            }
            super::painted::menu_item(&resp, CHECK_FOR_UPDATES, true, Some(state.check_for_updates));
            let b = Rect::from_center_size(
                row.left_center() + Vec2::new(g.box_center_x, 0.0),
                Vec2::splat(g.box_px),
            );
            p.rect_stroke(
                b,
                2.0,
                egui::Stroke::new(1.0, pal.text_dim),
                egui::StrokeKind::Inside,
            );
            if state.check_for_updates {
                p.rect_filled(b.shrink(2.0), 1.0, pal.accent);
            }
            p.text(
                row.left_center() + Vec2::new(g.icon_text_x, 0.0),
                egui::Align2::LEFT_CENTER,
                CHECK_FOR_UPDATES,
                egui::FontId::proportional(pal.font_row),
                pal.text,
            );
            let clicked = resp.clicked();
            resp.on_hover_text("Check GitHub for new releases once per day on startup");
            if clicked {
                toggled = true;
            }
        });
    if toggled {
        state.check_for_updates = !state.check_for_updates;
        if !state.check_for_updates {
            state.update_available = None;
        }
        state.persist_prefs();
        state.ribbon_settings_open = false;
    } else {
        let outside = ui.input(|i| {
            i.pointer.button_pressed(egui::PointerButton::Primary)
                && i.pointer
                    .interact_pos()
                    .is_none_or(|p| !button.expand(g.gap).contains(p) && !row.contains(p))
        });
        if outside {
            state.ribbon_settings_open = false;
        }
    }
}
