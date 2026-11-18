//! #2257: the progress overlay over the viewport while a toolpath
//! generates - the operation's name, its place in the batch, the elapsed
//! time, an activity bar (the generators report no fraction) and a Cancel
//! button. View state only: it reads `State::gen_progress` and starts no
//! work; the shell repaints on a timer only while a job runs.
//! Provenance: our own code over egui's painter; no HSMWorks material.

use egui::{Align2, Color32, FontId, Rect, Stroke, Ui, Vec2};

use crate::gen_worker::overlay_text;
use crate::state::State;

/// The Cancel button's label (also what the structural test reads).
pub(super) const CANCEL: &str = "Cancel generation";

/// Paint the overlay at the top centre of the viewport's `rect` (clear of
/// the navigation strip at the bottom), if a job runs, and return the
/// panel's rect: the viewport takes no press inside it.
pub(super) fn paint(ui: &mut Ui, rect: Rect, state: &mut State, pal: &super::theme::Tokens) -> Option<Rect> {
    let progress = state.gen_progress()?;
    let font = FontId::proportional(pal.font_label.max(10.0));
    let pad = pal.window_margin.clamp(4.0, 24.0);
    let text = overlay_text(&progress);
    let text_w = ui.fonts_mut(|f| f.layout_no_wrap(text.clone(), font.clone(), Color32::WHITE).size().x);
    let cancel_w = ui.fonts_mut(|f| f.layout_no_wrap(CANCEL.to_owned(), font.clone(), Color32::WHITE).size().x)
        + 2.0 * ui.spacing().button_padding.x
        + 8.0;
    let row_h = font.size + 2.0 * ui.spacing().button_padding.y + 2.0;
    let bar_h = (font.size * 0.4).max(3.0);
    let width = (text_w + cancel_w + 3.0 * pad).max(280.0).min(rect.width() - 2.0 * pad);
    let height = pad + row_h + pad * 0.5 + bar_h + pad;
    if width < cancel_w + 2.0 * pad || rect.height() < height + 2.0 * pad {
        // Too small a viewport for the panel: the status line still says it.
        return None;
    }
    let panel = Rect::from_min_size(egui::pos2(rect.center().x - width * 0.5, rect.min.y + pad), Vec2::new(width, height));
    let painter = ui.painter_at(rect);
    let radius = f32::from(pal.radius_window);
    painter.rect_filled(panel, radius, Color32::from_black_alpha(200));
    painter.rect_stroke(panel, radius, Stroke::new(1.0, Color32::from_white_alpha(60)), egui::StrokeKind::Inside);
    let row = Rect::from_min_size(egui::pos2(panel.min.x + pad, panel.min.y + pad), Vec2::new(width - 2.0 * pad, row_h));
    painter.text(row.left_center(), Align2::LEFT_CENTER, text, font, Color32::WHITE);
    // The activity bar: a segment that sweeps the track once each 1.6 s.
    let track = Rect::from_min_size(egui::pos2(row.min.x, row.max.y + pad * 0.5), Vec2::new(row.width(), bar_h));
    painter.rect_filled(track, bar_h * 0.5, Color32::from_white_alpha(40));
    let seg = track.width() * 0.25;
    let phase = (progress.secs / 1.6).fract() as f32;
    let x0 = track.min.x + phase * (track.width() + seg) - seg;
    let bar = Rect::from_min_max(egui::pos2(x0.max(track.min.x), track.min.y), egui::pos2((x0 + seg).min(track.max.x), track.max.y));
    if bar.width() > 0.0 {
        painter.rect_filled(bar, bar_h * 0.5, pal.accent);
    }
    let button = Rect::from_min_size(egui::pos2(row.max.x - cancel_w, row.min.y), Vec2::new(cancel_w, row_h));
    if ui.put(button, egui::Button::new(CANCEL).wrap_mode(egui::TextWrapMode::Extend)).clicked() {
        state.cancel_generation();
    }
    Some(panel)
}
