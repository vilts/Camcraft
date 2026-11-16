//! The viewport's bottom furniture layout (#1198 fix round): where the
//! control ribbon's neighbours sit - the legend at the bottom-right and the
//! empty-document line - so that nothing overlaps anything at any pane width
//! down to the hud's minimum.
//!
//! Provenance: our own viewport layout, written for the #1198 maintainer
//! review; no vendor material consulted.
//!
//! `place` is pure on the pane, the cube and measured sizes. `furniture`
//! measures the text with the frame's fonts (the same call the paint makes)
//! and places it, so the shell and the tests read one set of rects.

use super::viewport_bar::bar_rect;

/// The control legend the shell paints at the pane's bottom-right while the
/// pointer is over the viewport (or a drag runs). Its text lives here so the
/// layout and the tests measure the same string the shell draws.
pub(crate) const LEGEND: &str = "LMB pick / drag-orbit · MMB orbit · RMB pan · wheel · F frame";
/// The empty-document line the shell paints until a model exists.
pub(crate) const EMPTY_LINE: &str = "No model yet - File > Model imports an STL, STEP or OBJ file";
const LEGEND_INSET: f32 = 12.0;
/// The empty-document line's base centre: this far above the pane's bottom
/// edge, and never above its top plus `EMPTY_TOP`.
const EMPTY_BASE: f32 = 56.0;
const EMPTY_TOP: f32 = 24.0;
/// The clear space kept between two pieces of furniture (and from the cube).
pub(crate) const GAP: f32 = 8.0;

/// Where the legend's bottom-right anchor sits: inset from the pane's corner.
#[must_use]
pub(crate) fn legend_anchor(pane: egui::Rect) -> egui::Pos2 {
    pane.max - egui::vec2(LEGEND_INSET, LEGEND_INSET)
}

/// The text wrap width of the bottom furniture: the pane less the inset on
/// each side, so a line never runs past the pane at the hud's minimum width.
#[must_use]
pub(crate) fn wrap_width(pane: egui::Rect) -> f32 {
    (pane.width() - 2.0 * LEGEND_INSET).max(1.0)
}

/// The empty-document line's font: the shell's small style at 1.2x.
#[must_use]
pub(crate) fn empty_font(small: &egui::FontId) -> egui::FontId {
    egui::FontId::proportional(small.size * 1.2)
}

/// `text` laid out with the frame's fonts - the same call the paint uses, so
/// a measured size is the painted size.
#[must_use]
pub(crate) fn galley(
    ctx: &egui::Context,
    text: &str,
    font: egui::FontId,
    wrap: f32,
) -> std::sync::Arc<egui::Galley> {
    ctx.fonts_mut(|f| f.layout(text.to_owned(), font, egui::Color32::WHITE, wrap))
}

/// Where the bottom furniture sits, in screen space. `legend` is None when it
/// would meet the view cube (it is a hover hint, so it is then not drawn);
/// `empty` is None when the line would meet the cube or leave the pane.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Placed {
    pub bar: egui::Rect,
    pub legend: Option<egui::Rect>,
    pub empty: Option<egui::Rect>,
}

/// Lift `r` so its bottom sits just above `obstacle`, when it meets it.
fn lift_above(r: egui::Rect, obstacle: egui::Rect) -> egui::Rect {
    if r.expand(GAP).intersects(obstacle) {
        r.translate(egui::vec2(0.0, obstacle.min.y - GAP - r.max.y))
    } else {
        r
    }
}

/// The placement rule, pure on the pane, the cube and the measured sizes:
/// 1. The ribbon is centred on the bottom edge and has priority.
/// 2. The legend stays at the bottom-right corner while it clears the ribbon
///    by `GAP`; where it would meet the ribbon it sits on its own line
///    directly above the ribbon, right-aligned as before.
/// 3. The empty-document line is centred at its base height. It lifts above
///    the ribbon if it meets it, then above the legend if the legend is drawn
///    and it meets that too - so the line moves up by the legend's lift.
/// 4. A piece that would meet the view cube, or leave the pane, is not drawn.
#[must_use]
pub(crate) fn place(
    pane: egui::Rect,
    cube: Option<egui::Rect>,
    legend: egui::Vec2,
    empty: Option<egui::Vec2>,
) -> Placed {
    let bar = bar_rect(pane);
    let anchor = legend_anchor(pane);
    let bottom_line = egui::Rect::from_min_size(
        egui::pos2(anchor.x - legend.x, anchor.y - legend.y),
        legend,
    );
    let fits = |r: egui::Rect| {
        r.min.y >= pane.min.y && cube.is_none_or(|c| !r.expand(GAP).intersects(c))
    };
    let legend_rect = Some(lift_above(bottom_line, bar)).filter(|r| fits(*r));
    let empty_rect = empty.and_then(|size| {
        let centre = egui::pos2(
            pane.center().x,
            (pane.max.y - EMPTY_BASE).max(pane.min.y + EMPTY_TOP),
        );
        let r = lift_above(egui::Rect::from_center_size(centre, size), bar);
        let r = match legend_rect {
            Some(l) => lift_above(r, l),
            None => r,
        };
        Some(r).filter(|r| fits(*r))
    });
    Placed {
        bar,
        legend: legend_rect,
        empty: empty_rect,
    }
}

/// The bottom furniture measured with the frame's fonts, and placed by
/// `place`. The shell and the tests both call this, so they read one set of
/// rects.
pub(crate) struct Furniture {
    pub placed: Placed,
    /// The legend text at `wrap_width(pane)`; painted at the legend rect.
    pub legend_text: std::sync::Arc<egui::Galley>,
    /// The empty-document text, when the document is empty.
    pub empty_text: Option<std::sync::Arc<egui::Galley>>,
}

#[must_use]
pub(crate) fn furniture(
    ctx: &egui::Context,
    pane: egui::Rect,
    has_model: bool,
    cube: Option<egui::Rect>,
    small: &egui::FontId,
) -> Furniture {
    let wrap = wrap_width(pane);
    let legend_text = galley(ctx, LEGEND, small.clone(), wrap);
    let empty_text = (!has_model).then(|| galley(ctx, EMPTY_LINE, empty_font(small), wrap));
    let placed = place(
        pane,
        cube,
        legend_text.size(),
        empty_text.as_ref().map(|g| g.size()),
    );
    Furniture {
        placed,
        legend_text,
        empty_text,
    }
}


#[cfg(test)]
mod tests {
    use super::*;
    use crate::egui_app::viewcube;

    /// The shell's small style, read the same way the shell reads it.
    fn small_font() -> egui::FontId {
        egui::Style::default()
            .text_styles
            .get(&egui::TextStyle::Small)
            .cloned()
            .unwrap_or_else(|| egui::FontId::proportional(11.0))
    }

    /// A context with its fonts up, the way a frame has them. The texture
    /// delta is cleared: epaint asserts on drop of unapplied uploads.
    fn frame_ctx() -> egui::Context {
        let ctx = egui::Context::default();
        let mut out = ctx.run_ui(egui::RawInput::default(), |_| {});
        out.textures_delta.clear();
        ctx
    }

    #[test]
    fn legend_clears_the_ribbon() {
        // Three panes: a wide one (the legend keeps the bottom line), an
        // ordinary one, and the hud's minimum width (the legend wraps and
        // lifts). With and without a model, no pair of the bottom furniture
        // and the view cube may share pixels, and every piece stays in the pane.
        let ctx = frame_ctx();
        let font = small_font();
        let min_w = viewcube::min_width();
        let panes = [(1600.0_f32, 900.0_f32), (700.0, 600.0), (min_w, 260.0)];
        for (w, h) in panes {
            for has_model in [true, false] {
                let pane = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(w, h));
                let cube = viewcube::Hud::new(pane).map(|hud| hud.cube);
                let f = furniture(&ctx, pane, has_model, cube, &font);
                let mut drawn: Vec<(&str, egui::Rect)> = vec![("ribbon", f.placed.bar)];
                if let Some(r) = f.placed.legend {
                    drawn.push(("legend", r));
                }
                if let Some(r) = f.placed.empty {
                    drawn.push(("empty-document line", r));
                }
                if let Some(c) = cube {
                    drawn.push(("view cube", c));
                }
                for (i, (a, ra)) in drawn.iter().enumerate() {
                    for (b, rb) in &drawn[i + 1..] {
                        assert!(
                            !ra.intersects(*rb),
                            "{a} {ra:?} meets {b} {rb:?} on a {w} x {h} pane (model: {has_model})"
                        );
                    }
                    assert!(
                        pane.contains(ra.min) && pane.contains(ra.max),
                        "{a} {ra:?} leaves the {w} x {h} pane"
                    );
                }
                if w == 1600.0 {
                    assert_eq!(
                        f.placed.legend.map(|r| r.max),
                        Some(legend_anchor(pane)),
                        "a wide pane keeps the legend on the bottom line"
                    );
                }
                if w == min_w
                    && let Some(r) = f.placed.legend
                {
                    assert!(
                        (r.max.y - (f.placed.bar.min.y - GAP)).abs() < 1e-3,
                        "at the minimum the legend lifts onto its own line above the ribbon"
                    );
                }
            }
        }
    }
}
