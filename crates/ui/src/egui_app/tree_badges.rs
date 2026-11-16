//! A browser row's glyph and its operation status badge (#661 section 7):
//! the badge is a small solid overlay at the glyph's lower right, so the
//! status reads at a glance beside the row's text status column.
//! Provenance: our own; the badge glyphs are original SVG masters in
//! assets/ui/svg/ (`badge-*`).

use camcraft_app::operation::OpStatus;
use egui::{Color32, Painter, Pos2, Rect, Vec2};

use super::icons::{Icon, IconSet};

/// The badge an operation row wears. It shows what the row's text already
/// says - no new state: refused by the tool law ("BLOCKED") or generated
/// with warnings wears the warning badge; otherwise the cache status
/// (`never` / `ok` / `STALE`). A stale path outranks its old warnings: they
/// belong to the path that no longer matches.
pub(crate) fn status_badge(status: OpStatus, refused: bool, warnings: usize) -> Icon {
    match status {
        _ if refused => Icon::BadgeWarning,
        OpStatus::Never => Icon::BadgeNever,
        OpStatus::Stale => Icon::BadgeStale,
        OpStatus::Ok if warnings > 0 => Icon::BadgeWarning,
        OpStatus::Ok => Icon::BadgeOk,
    }
}

/// Glyph edge on a tree row, and the badge's.
const GLYPH: f32 = 16.0;
const BADGE: f32 = 9.0;

/// Paint a row's glyph centred on `at`, tinted `ink`, and its badge (if
/// any) over the lower right on a disc of `ground`, so the glyph's strokes
/// do not run into it. Returns false when there is no texture to paint
/// (headless, or a failed decode): the caller draws its vector stand-in.
pub(crate) fn row_glyph(
    painter: &Painter,
    icons: Option<&IconSet>,
    icon: Option<Icon>,
    badge: Option<Icon>,
    at: Pos2,
    ink: Color32,
    ground: Color32,
) -> bool {
    let uv = Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0));
    let Some(tex) = icons.and_then(|s| icon.and_then(|i| s.texture(i))) else { return false };
    painter.image(tex.id(), Rect::from_center_size(at, Vec2::splat(GLYPH)), uv, ink);
    if let Some(b) = icons.and_then(|s| badge.and_then(|i| s.texture(i))) {
        let c = at + Vec2::splat(GLYPH * 0.5 - BADGE * 0.5 + 1.0);
        painter.circle_filled(c, BADGE * 0.5 + 1.0, ground);
        painter.image(b.id(), Rect::from_center_size(c, Vec2::splat(BADGE)), uv, ink);
    }
    true
}
