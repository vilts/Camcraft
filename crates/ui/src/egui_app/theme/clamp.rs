//! Clamped theme geometry ranges shared by `theme.rs` and every draw site (#2066, extracted from `theme.rs` for the 400-line law).
//! Provenance: our own bounds for Camcraft's shell layout; no HSMWorks material.

/// Widget/cell/tab corner radius (px).
pub const RADIUS: u8 = 12;
/// Item spacing, button padding, tab padding and tree inset/indent (px).
pub const SPACING: (f32, f32) = (0.0, 32.0);
/// Input-field, op-tab and browser-row height (px).
pub const CONTROL_H: (f32, f32) = (12.0, 64.0);
/// Ribbon cell height/width (px).
pub const CELL: (f32, f32) = (24.0, 96.0);
/// Ribbon, tab and browser-tree glyph edge (px).
pub const GLYPH: (f32, f32) = (8.0, 64.0);
/// Caption, tab, label and row font sizes (px).
pub const FONT: (f32, f32) = (6.0, 32.0);
/// Ribbon tab row and caption row heights (px).
pub const RIBBON_ROW: (f32, f32) = (12.0, 48.0);
/// Ribbon and op-tab bottom gaps (px).
pub const RIBBON_GAP: (f32, f32) = (0.0, 24.0);
/// Right-edge reserve for the browser eye toggle (px).
pub const EYE_RESERVE: (f32, f32) = (12.0, 64.0);
/// Extra point size added to section header labels in the browser tree (px).
pub const HEADER_BOOST: (f32, f32) = (0.0, 12.0);
/// Outline and insertion/bottom-rule stroke thickness (px).
pub const STROKE: (f32, f32) = (0.5, 8.0);

/// Window and menu corner radius upper bound (#2064).
pub const RADIUS_WINDOW: u8 = 24;
/// Floating window inner margin (px).
pub const WINDOW_MARGIN: (f32, f32) = (0.0, 24.0);
/// Bottom status strip height (px).
pub const STATUS_H: (f32, f32) = (16.0, 64.0);
/// Ribbon tab per-side horizontal padding (px).
pub const RIBBON_TAB_PAD_X: (f32, f32) = (2.0, 32.0);
/// Ribbon active tab underline thickness (px).
pub const RIBBON_STROKE: (f32, f32) = (1.0, 8.0);

#[must_use]
pub fn range(v: f32, lo_hi: (f32, f32)) -> f32 {
    if v.is_finite() { v.clamp(lo_hi.0, lo_hi.1) } else { lo_hi.0 }
}
