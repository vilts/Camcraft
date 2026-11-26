//! One painted ribbon cell (moved out of painted.rs for #2752): the glyph
//! over its label, a state fill behind both, and the click target.
//! Provenance: our own port of ribbon.rs's cell (our code), egui sources.

use egui::text::{LayoutJob, TextWrapping};
use egui::{Color32, FontId, Galley, Pos2, Rect, Response, Sense, Shape, TextureHandle, TextFormat, Ui, Vec2};

use super::contrast;
use super::painted::UV_FULL;
use super::theme::{Tokens, clamp};

/// Clear space between the label and the edge of the cell box.
const LABEL_PAD: f32 = 1.0;
/// Clear space that the cell box keeps above the group caption row.
const CAPTION_CLEAR: f32 = 2.0;

/// The box that a cell fills and hit-tests (#2752). The layout gives the
/// cell `rect` (`Tokens::cell_h` tall), but the label sits under a glyph
/// of `glyph_px`. A theme with a large glyph puts the label partly below
/// `rect`: the fill then covered only part of the label. The box extends
/// down to the label bottom, and never into the caption row.
#[must_use]
pub(super) fn cell_box(rect: Rect, label_bottom: f32, pal: &Tokens) -> Rect {
    let floor = rect.bottom() + (pal.ribbon_caption_gap - CAPTION_CLEAR).max(0.0);
    let bottom = (label_bottom + LABEL_PAD).clamp(rect.bottom(), floor);
    Rect::from_min_max(rect.min, Pos2::new(rect.right(), bottom))
}

/// One painted ribbon cell: the bundled glyph centred over a short
/// caption, a fill by state, dimmed ink when disabled (a disabled cell
/// stays hoverable and greys, it never vanishes). The label row sits at
/// `icon + 2` below the cell top WHETHER OR NOT a texture is available:
/// headless frames lay out exactly like GPU frames (the M1.4.2 icon law).
/// A filled cell paints its glyph AND its label in the fill's on-fill ink,
/// so the two never disagree on one fill (#2752).
#[allow(clippy::too_many_arguments)] // flat paint args, same shape as push_row
pub fn cell(
    ui: &mut Ui,
    id: egui::Id,
    rect: Rect,
    label: &str,
    pal: &Tokens,
    enabled: bool,
    toggled: bool,
    icon: Option<&TextureHandle>,
) -> Response {
    // The label shrinks to fit its cell (down to the theme's font_label_min)
    // so a short word reads whole at the narrow 1280 px cell (#948); only a
    // label that still does not fit at the floor truncates with an
    // ellipsis. Its ink is the placeholder: the fill decides it below.
    let max_w = (rect.width() - 2.0).max(0.0);
    let start_px = clamp::range(pal.font_label, clamp::FONT);
    let min_px = clamp::range(pal.font_label_min, clamp::FONT).min(start_px);
    let galley = fit_label(ui, label, start_px, min_px, max_w, Color32::PLACEHOLDER);
    let label_top = rect.top() + pal.glyph_px + 2.0;
    let bx = cell_box(rect, label_top + galley.size().y, pal);
    let resp = ui.interact(bx, id, Sense::click_and_drag());
    let bg = if !enabled {
        None
    } else if resp.clicked() || resp.is_pointer_button_down_on() {
        Some(pal.accent)
    } else if toggled {
        Some(pal.selection_bg)
    } else if resp.hovered() {
        Some(pal.field_hover)
    } else {
        None
    };
    let painter = ui.painter();
    if let Some(bg) = bg {
        painter.add(Shape::rect_filled(bx, pal.radius_widget, bg));
    }
    if let Some(tex) = icon {
        // The line art rides the theme's on-fill ink, not a hand-typed
        // white (M6.1 D). The enabled/disabled rule rides the alpha
        // (imgui's x1.0 / x0.45), on the chosen hue.
        let r = Rect::from_min_size(
            Pos2::new(rect.left() + (rect.width() - pal.glyph_px) * 0.5, rect.top()),
            Vec2::splat(pal.glyph_px),
        );
        // A filled cell wears the on-fill ink; a plain glyph wears the
        // theme's own ribbon ink (a token, #2021).
        let ink = bg.map_or(pal.ribbon_ink, contrast::on_fill);
        let tint = if enabled { ink } else { Color32::from_rgba_unmultiplied(ink.r(), ink.g(), ink.b(), 115) };
        painter.image(tex.id(), r, UV_FULL, tint);
    }
    let ink = match bg {
        Some(fill) => contrast::on_fill(fill),
        None if enabled => pal.text,
        None => pal.text_faint,
    };
    painter.galley(Pos2::new(rect.center().x - galley.size().x * 0.5, label_top), galley, ink);
    resp.widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::Button, enabled, toggled, label));
    resp
}

/// Lay `label` out on one line at the largest whole size from `start_px`
/// down to `min_px` whose width fits `max_w`; at the floor it truncates
/// with an ellipsis (never overflows).
fn fit_label(ui: &Ui, label: &str, start_px: f32, min_px: f32, max_w: f32, ink: Color32) -> std::sync::Arc<Galley> {
    let floor_px = min_px.min(start_px);
    let mut px = start_px.max(floor_px);
    loop {
        let floor = px <= floor_px;
        let mut job = LayoutJob {
            wrap: if floor {
                TextWrapping::truncate_at_width(max_w)
            } else {
                TextWrapping::no_max_width()
            },
            ..LayoutJob::default()
        };
        job.append(label, 0.0, TextFormat::simple(FontId::proportional(px), ink));
        let galley = ui.painter().layout_job(job);
        if floor || galley.size().x <= max_w {
            return galley;
        }
        px = (px - 1.0).max(floor_px);
    }
}
