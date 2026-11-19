//! The token vocabulary shared by the JSON parser and `apply_overlay` (#2066, extracted from `theme_files.rs` for the 400-line law).
//! Provenance: our own token key names for Camcraft's theme schema; no HSMWorks material.

pub const INK: &str = "ink";
pub const INK_ALT: &str = "ink_alt";
pub const WELL: &str = "well";
pub const TEXT: &str = "text";
pub const TEXT_DIM: &str = "text_dim";
pub const TEXT_FAINT: &str = "text_faint";
pub const BORDER: &str = "border";
pub const ACCENT: &str = "accent";
pub const ACCENT_HOVER: &str = "accent_hover";
pub const FIELD_BG: &str = "field_bg";
pub const FIELD_HOVER: &str = "field_hover";
pub const FIELD_ACTIVE: &str = "field_active";
pub const SELECTION_BG: &str = "selection_bg";
pub const SELECTION_STROKE: &str = "selection_stroke";
pub const VIEWPORT: &str = "viewport";
pub const RADIUS_WIDGET: &str = "radius_widget";
pub const RADIUS_WINDOW: &str = "radius_window";
pub const RADIUS_MENU: &str = "radius_menu";
pub const ITEM_SPACING_X: &str = "item_spacing_x";
pub const ITEM_SPACING_Y: &str = "item_spacing_y";
pub const BUTTON_PADDING_X: &str = "button_padding_x";
pub const BUTTON_PADDING_Y: &str = "button_padding_y";
pub const CONTROL_HEIGHT: &str = "control_height";
pub const ROW_HEIGHT: &str = "row_height";
pub const CELL_H: &str = "cell_h";
pub const CELL_W: &str = "cell_w";
pub const GLYPH_PX: &str = "glyph_px";
pub const FONT_LABEL: &str = "font_label";
pub const FONT_LABEL_MIN: &str = "font_label_min";
pub const FONT_CAPTION: &str = "font_caption";
pub const FONT_ROW: &str = "font_row";
pub const FONT_ROW_SUFFIX: &str = "font_row_suffix";
pub const RIBBON_INK: &str = "ribbon_ink";
pub const RIBBON_TAB_H: &str = "ribbon_tab_h";
pub const RIBBON_CELL_TOP_GAP: &str = "ribbon_cell_top_gap";
pub const RIBBON_CAPTION_GAP: &str = "ribbon_caption_gap";
pub const RIBBON_CAPTION_ROW_H: &str = "ribbon_caption_row_h";
pub const RIBBON_BOTTOM_GAP: &str = "ribbon_bottom_gap";
pub const OP_TAB_H: &str = "op_tab_h";
pub const OP_TAB_PAD_X: &str = "op_tab_pad_x";
pub const OP_TAB_GAP: &str = "op_tab_gap";
pub const OP_TAB_BOTTOM_GAP: &str = "op_tab_bottom_gap";
pub const OP_TAB_ICON_PX: &str = "op_tab_icon_px";
pub const OP_TAB_ICON_GAP: &str = "op_tab_icon_gap";
pub const OP_TAB_RADIUS: &str = "op_tab_radius";
pub const FONT_OP_TAB: &str = "font_op_tab";
pub const TREE_GLYPH_PX: &str = "tree_glyph_px";
pub const TREE_INSET_X: &str = "tree_inset_x";
pub const TREE_INDENT: &str = "tree_indent";
pub const TREE_EYE_RESERVE: &str = "tree_eye_reserve";
pub const TREE_PAD_X: &str = "tree_pad_x";
pub const TREE_PAD_Y: &str = "tree_pad_y";
pub const TREE_SUFFIX_GAP: &str = "tree_suffix_gap";
pub const TREE_HEADER_BOOST: &str = "tree_header_boost";
pub const HIGHLIGHT_STROKE_PX: &str = "highlight_stroke_px";
pub const MENU_WIDTH: &str = "menu_width";
pub const RECENT_WIDTH: &str = "recent_width";
pub const MENU_GAP: &str = "menu_gap";
pub const MENU_PAD_X: &str = "menu_pad_x";

pub const WINDOW_MARGIN: &str = "window_margin";
pub const STATUS_H: &str = "status_h";
pub const RIBBON_TAB_PAD_X: &str = "ribbon_tab_pad_x";
pub const RIBBON_TAB_GAP: &str = "ribbon_tab_gap";
pub const RIBBON_TAB_STROKE: &str = "ribbon_tab_stroke";

/// The colour vocabulary, in table order (refusals quote it).
pub const COLOR_NAMES: &[&str] = &[
    INK, INK_ALT, WELL, TEXT, TEXT_DIM, TEXT_FAINT, BORDER, ACCENT,
    ACCENT_HOVER, FIELD_BG, FIELD_HOVER, FIELD_ACTIVE, SELECTION_BG,
    SELECTION_STROKE, VIEWPORT, RIBBON_INK,
];

/// The geometry vocabulary (every override is clamped through the
/// same windows the built-in const guard checks).
pub const GEOM_NAMES: &[&str] = &[
    RADIUS_WIDGET, RADIUS_WINDOW, RADIUS_MENU, ITEM_SPACING_X, ITEM_SPACING_Y,
    BUTTON_PADDING_X, BUTTON_PADDING_Y, CONTROL_HEIGHT, ROW_HEIGHT, CELL_H,
    CELL_W, GLYPH_PX, FONT_LABEL, FONT_LABEL_MIN, FONT_CAPTION, FONT_ROW,
    FONT_ROW_SUFFIX, RIBBON_TAB_H, RIBBON_CELL_TOP_GAP, RIBBON_CAPTION_GAP,
    RIBBON_CAPTION_ROW_H, RIBBON_BOTTOM_GAP, OP_TAB_H, OP_TAB_PAD_X,
    OP_TAB_GAP, OP_TAB_BOTTOM_GAP, OP_TAB_ICON_PX, OP_TAB_ICON_GAP,
    OP_TAB_RADIUS, FONT_OP_TAB, TREE_GLYPH_PX, TREE_INSET_X, TREE_INDENT,
    TREE_EYE_RESERVE, TREE_PAD_X, TREE_PAD_Y, TREE_SUFFIX_GAP,
    TREE_HEADER_BOOST, HIGHLIGHT_STROKE_PX,
    WINDOW_MARGIN, STATUS_H, RIBBON_TAB_PAD_X, RIBBON_TAB_GAP, RIBBON_TAB_STROKE,
    MENU_WIDTH, RECENT_WIDTH, MENU_GAP, MENU_PAD_X,
];
