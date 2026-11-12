//! The draw-list port, graduated from the spike: ribbon cells and browser
//! rows as Shape/Painter custom paint, not stock widgets.
//!
//! In the imgui shell these strips are one `get_window_draw_list()` call
//! per mark (ribbon.rs's `cell`, browser.rs's row loop): background rects,
//! glyphs and captions added as ink over invisible buttons. egui's answer
//! is the same division of labor - `Painter` + `Shape` for the ink,
//! `ui.interact` for the input - and this module is that port: what is
//! painted could not be done with a widget table, while every click is
//! still a real egui response with real hover/active state.
//!
//! M-b graduation: the tree rows move to browser.rs's COLUMN MODEL
//! (go/no-go report, divergence 1 - the spike's Contour1 row let the
//! warning suffix slide under the trailing eye). Each row is now laid out
//! right-to-left: the eye column is reserved (rows that own an eye get a
//! 24 px band at the right edge, browser.rs's `avail - INK - 6.0` rule in
//! points), and label + suffix form ONE galley truncated into the
//! remaining column - egui's own `TextWrapping::truncate_at_width`, so an
//! over-long name elides with "…" instead of colliding. The spike's extra
//! right-anchored marks (the green path dot and a second STALE text) are
//! gone: imgui's law is that the suffix column carries the whole status
//! ("Contour  ok  2 warnings"), and two channels overlapped at narrow
//! widths.
//!
//! The shapes are original marks (rectangle outlines, triangles, a
//! two-stroke eye), drawn from the geometry of what they name - not
//! adapted from any icon set, per AGENTS.md. Colours AND sizes are the
//! shared `Tokens` table (M6.1 A), so the painted strips and the widget
//! panes cannot drift apart on a retheme - or on a resize.
//!
//! Provenance: our own port, specced from imgui browser.rs/ribbon.rs
//! (our code) and the egui vendored sources.

use egui::text::{LayoutJob, TextWrapping};
use egui::{
    Color32, FontId, Pos2, Rect, Response, Sense, Shape, Stroke, TextureHandle, TextFormat,
    Ui, Vec2,
};

use super::icons::{Icon, IconSet};
use super::theme::Tokens;

/// The full-texel uv of an uploaded image.
const UV_FULL: Rect = Rect::from_min_max(Pos2::new(0.0, 0.0), Pos2::new(1.0, 1.0));

/// One painted ribbon cell: fixed-size, the bundled glyph centered over a
/// short caption, tinted background by state, dimmed ink when disabled
/// (the imgui rule: a disabled cell stays hoverable and greys, it never
/// vanishes). The label row sits at `icon + 2` below the cell top WHETHER
/// OR NOT a texture is available - headless frames lay out exactly like
/// GPU frames, the M1.4.2 icon law (ribbon.rs: "layout never depends on
/// the GPU").
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
    let resp = ui.interact(rect, id, Sense::click());
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
        painter.add(Shape::rect_filled(rect, pal.radius_widget, bg));
    }
    if let Some(tex) = icon {
        // The line art rides the theme's on-fill ink, not a hand-typed
        // white (M6.1 D): over the accent cell that is white by the
        // arithmetic - over a pale user accent, or the light column's
        // grounds, it is black, which is the whole point. The
        // enabled/disabled rule still rides the alpha (imgui's
        // x1.0 / x0.45), now on the chosen hue.
        let r = Rect::from_min_size(
            Pos2::new(rect.left() + (rect.width() - pal.glyph_px) * 0.5, rect.top()),
            Vec2::splat(pal.glyph_px),
        );
        let ink = super::contrast::on_fill(bg.unwrap_or(pal.strip_fill()));
        let tint = if enabled { ink } else { Color32::from_rgba_unmultiplied(ink.r(), ink.g(), ink.b(), 115) };
        painter.image(tex.id(), r, UV_FULL, tint);
    }
    let ink = if enabled { pal.text } else { pal.text_faint };
    // The label shrinks to fit its cell (down to MIN_LABEL_PX) so a short
    // word reads whole at the narrow 1280 px cell (#948); only a label
    // that still does not fit at the floor truncates with an ellipsis, so
    // it can never collide with the neighbour's.
    let max_w = (rect.width() - 2.0).max(0.0);
    let galley = fit_label(ui, label, pal.font_label, max_w, ink);
    painter.galley(
        Pos2::new(rect.center().x - galley.size().x * 0.5, rect.top() + pal.glyph_px + 2.0),
        galley,
        ink,
    );
    resp
}

/// The smallest ribbon label size (px); below it a label elides instead.
pub const MIN_LABEL_PX: f32 = 10.0;

/// Lay `label` out on one line at the largest whole size from `start_px`
/// down to MIN_LABEL_PX whose width fits `max_w`; at the floor it truncates
/// with an ellipsis (never overflows).
fn fit_label(
    ui: &Ui,
    label: &str,
    start_px: f32,
    max_w: f32,
    ink: Color32,
) -> std::sync::Arc<egui::Galley> {
    let mut px = start_px.max(MIN_LABEL_PX);
    loop {
        let floor = px <= MIN_LABEL_PX;
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
        px = (px - 1.0).max(MIN_LABEL_PX);
    }
}

/// A group caption under its cells, centred on `mid_x` in the strip's
/// bottom band, faint ink. Painted only when it fits inside `width` (the
/// group's own span): a caption wider than its group would run under the
/// neighbour's, so single-door groups - whose cell already names the
/// section - skip it (#948). Returns whether it was painted.
pub fn caption(
    ui: &Ui,
    mid_x: f32,
    width: f32,
    title: &str,
    strip: Rect,
    pal: &Tokens,
    cell_bottom: f32,
) -> bool {
    let ink = pal.text_faint;
    let mut job = LayoutJob {
        wrap: TextWrapping::no_max_width(),
        ..LayoutJob::default()
    };
    job.append(title, 0.0, TextFormat::simple(FontId::proportional(MIN_LABEL_PX), ink));
    let galley = ui.painter().layout_job(job);
    if galley.size().x > width {
        return false;
    }
    let y = (cell_bottom + 3.0).min(strip.bottom() - galley.size().y);
    ui.painter()
        .galley(Pos2::new(mid_x - galley.size().x * 0.5, y), galley, ink);
    true
}

/// A group divider: one vertical hairline, ink only.
pub fn divider(painter: &egui::Painter, x: f32, rect: Rect, pal: &Tokens) {
    painter.line_segment(
        [Pos2::new(x, rect.top() + 4.0), Pos2::new(x, rect.bottom() - 4.0)],
        Stroke::new(1.0, pal.border),
    );
}

/// One browser row: what it says, what it is, how it must read. The
/// suffix is the row's whole status column (browser.rs's law) - kind,
/// cache tag, warnings - so there is exactly one text channel to lay out.
pub struct Row {
    pub label: String,
    /// Faint tail text after the label - the browser's suffix column
    /// (tool sizes, op kind/status), mirrored from browser.rs.
    pub suffix: String,
    pub kind: Kind,
    /// The bundled glyph for this row (browser.rs's `spec.icon`: Stock,
    /// WorkOffset, Model, Tools, the op's operation icon, `Generate` as
    /// the fallback the eye-list refuses). `None` only where imgui drew
    /// no PNG - section headers - and for the placeholder rows; the
    /// painted `glyph` is the no-GPU twin of every real icon.
    pub icon: Option<Icon>,
    /// An op row's status badge (#661), painted small over `icon`.
    pub badge: Option<Icon>,
    /// Selection band - the WORLD's selection mirrored, never a local flag.
    pub selected: bool,
    pub hidden: bool,
    /// Indent level (sections 0, children 1).
    pub depth: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Section,
    /// A browser parent; true points down, false points right.
    Parent(bool),
    Model,
    /// A model wearing the fixture marker (M3.2 B): the vise row, not
    /// the part row - same hideability, different noun.
    Fixture,
    Stock,
    Wcs,
    Tool,
    Op,
    /// A setup node (#343): its radio, filled when it is the active setup.
    Setup(bool),
}

/// Tree icon edge (browser.rs: 32 px sources render at 16). The row's
/// height is NOT here: it is `Tokens::row_height` (M6.1 A), and the
/// tree's column model and row-click hit-tests all read that one number,
/// so a theme that grows the rows grows the tree with them.
const TREE_INK: f32 = 16.0;

/// The eye column's reserved width at the row's right edge: the 18 px hit
/// band plus margin (browser.rs's `eye_x = avail - INK - 6.0`).
const EYE_RESERVE: f32 = 24.0;

/// Rows whose viewport visibility the user can toggle get the eye.
pub(crate) fn has_eye(kind: Kind) -> bool {
    matches!(kind, Kind::Model | Kind::Fixture | Kind::Stock | Kind::Wcs | Kind::Op)
}

/// Hit result for one frame of the tree.
pub struct TreeHit {
    pub row: Option<usize>,
    pub eye: Option<usize>,
    /// The row a secondary click landed on this frame (M9.1): the
    /// retype menu's channel, a hit like the others - never state.
    pub menu: Option<usize>,
    pub drag_started: Option<usize>,
    /// A setup row's radio was clicked (#343).
    pub radio: Option<usize>,
}

/// A tree row's (label, suffix) ink (#506); the row's icon takes the
/// label ink and its eye the suffix ink (#524). The label is body text; the
/// suffix and a hidden row step down to the dim tier - never the faint
/// (disabled) one, which reads ~3:1 on the light strip. A selected row is
/// all body text: the dim tier does not clear the selection fill. Every
/// ink here clears `contrast::BODY` on the ground it is painted on,
/// pinned by tree_ink_tests.
#[must_use]
pub fn row_inks(pal: &Tokens, hidden: bool, selected: bool) -> (Color32, Color32) {
    if selected {
        (pal.text, pal.text)
    } else if hidden {
        (pal.text_dim, pal.text_dim)
    } else {
        (pal.text, pal.text_dim)
    }
}

/// Paint the rows and collect this frame's clicks. Input first (real
/// `ui.interact` responses), then the ink - so hover state and painted
/// background cannot disagree within a frame.
///
/// `icons` is the loaded glyph set (`None` headless). Where a row owns an
/// icon and the texture is present, the PNG rides the row (dimmed to the
/// imgui 0.55 when hidden); everywhere else the painted `glyph`/`eye_glyph`
/// vector marks stand in, so a GPU-less frame lays out identically.
pub fn tree(
    ui: &mut Ui,
    rect: Rect,
    rows: &[Row],
    pal: &Tokens,
    icons: Option<&IconSet>,
) -> TreeHit {
    let mut hit = TreeHit { row: None, eye: None, menu: None, drag_started: None, radio: None };
    let mut eye_resps: Vec<(usize, Response)> = Vec::new();
    let row_h = pal.row_height;
    for (ix, row) in rows.iter().enumerate() {
        let top = rect.top() + ix as f32 * row_h;
        if top >= rect.bottom() {
            break; // scrolled past the pane: no row, no input, no ink
        }
        let rr = Rect::from_min_size(Pos2::new(rect.left(), top), Vec2::new(rect.width(), row_h));
        // Row first, eye after: egui's last-wins interaction order plus the
        // clicked() guard below means an eye pixel is one action, never
        // row+eye (browser.rs reaches the same law by submitting the eye's
        // invisible button before the Selectable).
        let row_resp = ui.interact(rr, ui.id().with(("tree-row", ix)), Sense::click_and_drag());
        let eye_rect = if has_eye(row.kind) {
            let er = Rect::from_center_size(
                Pos2::new(rr.right() - 12.0, rr.center().y),
                Vec2::new(18.0, 16.0),
            );
            let r = ui.interact(er, ui.id().with(("tree-eye", ix)), Sense::click());
            if r.clicked() {
                hit.eye = Some(ix);
            }
            eye_resps.push((ix, r));
            Some(er)
        } else {
            None
        };
        let painter = ui.painter();
        if row.selected {
            painter.add(Shape::rect_filled(rr.shrink(1.0), pal.radius_widget, pal.selection_bg));
        } else if row_resp.hovered() {
            painter.add(Shape::rect_filled(rr.shrink(1.0), pal.radius_widget, pal.field_hover));
        }
        let gx = rr.left() + 10.0 + row.depth as f32 * 14.0;
        if matches!(row.kind, Kind::Setup(_)) {
            let rr = Rect::from_center_size(Pos2::new(gx, rr.center().y), Vec2::splat(TREE_INK));
            if ui.interact(rr, ui.id().with(("tree-radio", ix)), Sense::click()).clicked() {
                hit.radio = Some(ix);
            }
        }
        // PNG first (browser.rs: the tree icon rides the row, dimmed to
        // 0.55 when hidden), the painted glyph as the no-GPU twin.
        // The glyphs are white masks: tinted with the row's own inks
        // (#524), as the ribbon tints its icons, never a white wash
        // that vanishes on a light panel.
        let (icon_ink, eye_ink) = row_inks(pal, row.hidden, row.selected);
        let ground = if row.selected { pal.selection_bg } else if row_resp.hovered() { pal.field_hover } else { ui.visuals().panel_fill };
        let painted = super::tree_badges::row_glyph(painter, icons, row.icon, row.badge, Pos2::new(gx, rr.center().y), icon_ink, ground);
        let mut shapes = Vec::new();
        if !painted {
            super::tree_glyphs::glyph(&mut shapes, Pos2::new(gx, rr.center().y), row.kind, pal);
        }
        if let Some(er) = eye_rect {
            let eye_tex = icons
                .and_then(|s| {
                    s.texture(if row.hidden {
                        Icon::EyeOff
                    } else {
                        Icon::Toolpaths
                    })
                })
                .map(|t| t.id());
            match eye_tex {
                Some(id) => {
                    painter.image(
                        id,
                        er.shrink(2.0),
                        UV_FULL,
                        eye_ink,
                    );
                }
                None => super::tree_glyphs::eye_glyph(&mut shapes, er, row.hidden, pal),
            }
        }
        painter.add(Shape::Vec(shapes));
        // The column model: one galley, label + suffix, truncated into the
        // text column - everything right of it belongs to the eye, never
        // to overflowing ink (divergence 1's fix).
        let (ink, suffix_ink) = row_inks(pal, row.hidden, row.selected);
        let reserve = if has_eye(row.kind) { EYE_RESERVE } else { 6.0 };
        let max_w = (rr.right() - reserve - (gx + 12.0)).max(0.0);
        let mut job = LayoutJob {
            wrap: TextWrapping::truncate_at_width(max_w),
            ..LayoutJob::default()
        };
        job.append(
            &row.label,
            0.0,
            TextFormat::simple(FontId::proportional(pal.font_row), ink),
        );
        if !row.suffix.is_empty() {
            job.append(
                &row.suffix,
                6.0,
                TextFormat::simple(FontId::proportional(pal.font_row_suffix), suffix_ink),
            );
        }
        let galley = painter.layout_job(job);
        painter.galley(
            Pos2::new(gx + 12.0, rr.center().y - galley.size().y * 0.5),
            galley,
            ink,
        );
        if row_resp.clicked() && hit.eye != Some(ix) && hit.radio != Some(ix) {
            hit.row = Some(ix);
        }
        if row_resp.secondary_clicked() && hit.eye != Some(ix) {
            hit.menu = Some(ix);
        }
        if row_resp.drag_started() && hit.eye != Some(ix) {
            hit.drag_started = Some(ix);
        }
    }
    // The eye's tooltip (browser.rs: Hide/Show + the view-state promise),
    // after the loop so the response lives past its borrow of `ui`.
    for (ix, r) in eye_resps {
        let tip = if rows[ix].hidden { "Show" } else { "Hide" };
        r.on_hover_text(format!("{tip} in the viewport (view state; geometry still cuts)"));
    }
    hit
}

/// Ground and hairline under a painted strip (ribbon rows, the status
/// line): the caller sizes the panel, this makes it read as chrome.
pub fn strip_bg(painter: &egui::Painter, rect: Rect, pal: &Tokens) {
    painter.rect_filled(rect, 0.0, pal.strip_fill());
    let y = rect.bottom() - 0.5;
    painter.line_segment(
        [Pos2::new(rect.left(), y), Pos2::new(rect.right(), y)],
        Stroke::new(1.0, pal.border),
    );
}
