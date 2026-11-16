//! The theme's single token table (M6.1 A): every colour, radius, spacing
//! and size the shell shows is a named field here - widget path and
//! painted path read the SAME struct, so a retheme is one table and the
//! two render paths cannot drift. The law is adopted from a sibling
//! project's proven design at the user's direction (ROADMAP M6.1); the
//! values are ours - the dark column carries today's `Palette::DARK`
//! numbers verbatim so the snapshots stay meaningful through the change.
//!
//! The drift guard is structural, not a checklist: the const block below
//! destructures EVERY `Tokens` field without a `..` arm, so adding a
//! field without wiring it (or explicitly resting it in the painted
//! path) fails to build - the sibling repo's exhaustiveness pattern,
//! restated for a table that paints as well as widgets.
//! Provenance: our own module (graduated from the spike and the M6.1 A
//! rework), specced from the imgui theme we already ship, the egui
//! Visuals docs, and the sibling architecture at the user's direction;
//! no HSMWorks material.

use egui::{Color32, CornerRadius, Margin, Stroke, Style, Visuals, vec2};

use super::contrast;

/// One theme's whole truth: colours, then geometry.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Tokens {
    // -- colours: the old Palette, names unchanged (M6.1 B keys on them).
    /// Window/panel ground.
    pub ink: Color32,
    /// Ribbon / browser / header ground (one step above ink).
    pub ink_alt: Color32,
    /// Input fields and empty recesses (one step below ink).
    pub well: Color32,
    pub text: Color32,
    pub text_dim: Color32,
    pub text_faint: Color32,
    pub border: Color32,
    /// The selection/hover accent (steel blue).
    pub accent: Color32,
    pub accent_hover: Color32,
    /// Widget-field ground, by interaction state.
    pub field_bg: Color32,
    pub field_hover: Color32,
    pub field_active: Color32,
    /// Text selection and combo-open highlight.
    pub selection_bg: Color32,
    pub selection_stroke: Color32,
    /// The viewport's 3D ground (the render pass clear colour).
    pub viewport: Color32,
    // -- geometry: the Phase-6 complaint made themeable - ribbon size,
    // input fields and button size are tokens, not literals.
    /// Corner radius of widget rects and painted cells/rows.
    pub radius_widget: u8,
    /// Corner radius of egui windows.
    pub radius_window: u8,
    /// Corner radius of menus and combo dropdowns.
    pub radius_menu: u8,
    /// egui item spacing, x/y.
    pub item_spacing_x: f32,
    pub item_spacing_y: f32,
    /// egui button padding, x/y.
    pub button_padding_x: f32,
    pub button_padding_y: f32,
    /// Input-field/combo minimum height (egui's `interact_size.y`).
    pub control_height: f32,
    /// Browser tree row height (painted::tree).
    pub row_height: f32,
    /// Ribbon cell height (painted cell box; RIBBON_H follows it).
    pub cell_h: f32,
    /// Ribbon cell width cap (panes' flow clamp; floor is geometry).
    pub cell_w: f32,
    /// Ribbon glyph edge (the PNG and the painted fallback ride it).
    pub glyph_px: f32,
    /// Cell caption size under the ribbon glyph.
    pub font_label: f32,
    /// Group caption size.
    pub font_caption: f32,
    /// Browser row label size.
    pub font_row: f32,
    /// Browser row suffix size.
    pub font_row_suffix: f32,
}

/// The painted path's clamps: geometry a theme file supplies can make
/// the shell ugly, never impossible - each value is clamped to the
/// window it must live in at the draw site (M6.1 law: a 400-point
/// radius cannot blow out a checkbox, a 0-height cell cannot eat the
/// viewport).
pub mod clamp {
    /// Widget/cell corner radius.
    pub const RADIUS: u8 = 12;
    /// Item spacing and button padding (px per axis): a window, so a
    /// file can space the shell out or in, never apart.
    pub const SPACING: (f32, f32) = (0.0, 32.0);
    /// Input-field height, browser row height (px).
    pub const CONTROL_H: (f32, f32) = (12.0, 64.0);
    /// Ribbon cell height/width (px).
    pub const CELL: (f32, f32) = (24.0, 96.0);
    /// Ribbon glyph edge (px).
    pub const GLYPH: (f32, f32) = (8.0, 64.0);
    /// Caption/row font sizes (px).
    pub const FONT: (f32, f32) = (6.0, 32.0);

    #[must_use]
    pub fn range(v: f32, lo_hi: (f32, f32)) -> f32 {
        if v.is_finite() { v.clamp(lo_hi.0, lo_hi.1) } else { lo_hi.0 }
    }
}

/// The ribbon strip's height: the cell box plus the caption row and the
/// pad, so a theme that grows the cells grows the ribbon with them
/// (66 = 16 + 30 + 20 is today's arithmetic, kept exact at the defaults).
#[must_use]
pub const fn ribbon_h(t: &Tokens) -> f32 {
    RIBBON_TAB_H + t.cell_h + 20.0
}

/// The ribbon's workspace tab strip height (above the cell row).
pub const RIBBON_TAB_H: f32 = 16.0;

impl Tokens {
    /// The imgui shell's numbers, as tokens - unchanged values, now the
    /// dark COLUMN of a table that will carry a second one (M6.1 C).
    pub const DARK: Tokens = Tokens {
        ink: Color32::from_rgb(0x23, 0x26, 0x2B),
        ink_alt: Color32::from_rgb(0x2C, 0x30, 0x37),
        well: Color32::from_rgb(0x1B, 0x1E, 0x22),
        text: Color32::from_rgb(0xD6, 0xDA, 0xE0),
        text_dim: Color32::from_rgb(0x9A, 0xA1, 0xAB),
        text_faint: Color32::from_rgb(0x6E, 0x75, 0x7E),
        border: Color32::from_rgb(0x3D, 0x43, 0x4C),
        accent: Color32::from_rgb(0x35, 0x6D, 0xA8),
        accent_hover: Color32::from_rgb(0x4A, 0x86, 0xC2),
        field_bg: Color32::from_rgb(0x1E, 0x21, 0x26),
        field_hover: Color32::from_rgb(0x2A, 0x2E, 0x35),
        field_active: Color32::from_rgb(0x1A, 0x2B, 0x3A),
        selection_bg: Color32::from_rgb(0x26, 0x4A, 0x6E),
        selection_stroke: Color32::from_rgb(0x4A, 0x86, 0xC2),
        viewport: Color32::from_rgb(0x17, 0x1A, 0x1F),
        radius_widget: 2,
        radius_window: 6,
        radius_menu: 6,
        item_spacing_x: 8.0,
        item_spacing_y: 3.0,
        button_padding_x: 4.0,
        button_padding_y: 1.0,
        control_height: 18.0,
        row_height: 20.0,
        cell_h: 30.0,
        cell_w: 76.0,
        glyph_px: 16.0,
        font_label: 11.0,
        font_caption: 12.5,
        font_row: 13.5,
        font_row_suffix: 11.5,
    };

    /// The same shell, inverted: paper grounds, dark ink, the steel
    /// accent stepped down for contrast against light (a hover still
    /// lifts, a well still sinks - the RELATIVE laws of the dark column
    /// are what carry over; the numbers are ours, chosen so body text
    /// clears 4.5:1 against every ground it sits on - the check itself
    /// rides `contrast::readable` and the light golden, M6.1 D). Geometry is
    /// identical in both columns - sizes are the shell's body, not its
    /// weather; a theme file is what restyles them, per document.
    pub const LIGHT: Tokens = Tokens {
        ink: Color32::from_rgb(0xEC, 0xEE, 0xF2),
        ink_alt: Color32::from_rgb(0xF6, 0xF7, 0xF9),
        well: Color32::from_rgb(0xDF, 0xE2, 0xE8),
        text: Color32::from_rgb(0x1F, 0x24, 0x2A),
        text_dim: Color32::from_rgb(0x53, 0x5B, 0x66),
        text_faint: Color32::from_rgb(0x84, 0x8C, 0x97),
        border: Color32::from_rgb(0xC4, 0xCA, 0xD3),
        accent: Color32::from_rgb(0x2F, 0x66, 0x9E),
        accent_hover: Color32::from_rgb(0x47, 0x7D, 0xB8),
        field_bg: Color32::from_rgb(0xFF, 0xFF, 0xFF),
        field_hover: Color32::from_rgb(0xDD, 0xE1, 0xE7),
        field_active: Color32::from_rgb(0xE2, 0xEB, 0xF5),
        selection_bg: Color32::from_rgb(0xC6, 0xDC, 0xF0),
        selection_stroke: Color32::from_rgb(0x47, 0x7D, 0xB8),
        viewport: Color32::from_rgb(0xDD, 0xE1, 0xE7),
        radius_widget: 2,
        radius_window: 6,
        radius_menu: 6,
        item_spacing_x: 8.0,
        item_spacing_y: 3.0,
        button_padding_x: 4.0,
        button_padding_y: 1.0,
        control_height: 18.0,
        row_height: 20.0,
        cell_h: 30.0,
        cell_w: 76.0,
        glyph_px: 16.0,
        font_label: 11.0,
        font_caption: 12.5,
        font_row: 13.5,
        font_row_suffix: 11.5,
    };

    /// The panel ground behind the ribbon/browser strips (painted.rs).
    #[must_use]
    pub const fn strip_fill(&self) -> Color32 {
        self.ink_alt
    }

    /// Build the egui Visuals from the tokens: every default-egui colour
    /// and corner the shell shows is named here, so the widget path and
    /// the painted path cannot disagree about what the theme is.
    #[must_use]
    pub fn visuals(&self) -> Visuals {
        // The destructuring arm: every COLOUR is named here and nowhere
        // else; a new colour field without a wiring line is a build
        // error, not default-egui grey. (Geometry routes through
        // `style()`; the const guard below covers those names.)
        let Tokens {
            ink, ink_alt, well, text, text_dim, text_faint, border, accent,
            accent_hover, field_bg, field_hover, field_active, selection_bg,
            selection_stroke, viewport: _,
            radius_widget, radius_window, radius_menu,
            item_spacing_x: _, item_spacing_y: _,
            button_padding_x: _, button_padding_y: _,
            control_height: _, row_height: _, cell_h: _, cell_w: _,
            glyph_px: _, font_label: _, font_caption: _,
            font_row: _, font_row_suffix: _,
        } = *self;
        let mut v = Visuals::dark();
        v.panel_fill = ink;
        v.window_fill = ink_alt;
        v.faint_bg_color = well;
        v.extreme_bg_color = well;
        v.code_bg_color = well;
        v.override_text_color = Some(text);
        v.weak_text_color = Some(text_dim);
        v.hyperlink_color = accent_hover;
        v.window_corner_radius = CornerRadius::same(radius_window);
        v.menu_corner_radius = CornerRadius::same(radius_menu);
        v.selection = egui::style::Selection {
            bg_fill: selection_bg,
            stroke: Stroke::new(v.selection.stroke.width, selection_stroke),
        };
        let stroke_1px = Stroke::new(1.0, border);
        let r = CornerRadius::same(radius_widget);
        v.widgets.noninteractive.bg_fill = ink_alt;
        v.widgets.noninteractive.weak_bg_fill = Color32::TRANSPARENT;
        v.widgets.noninteractive.bg_stroke = stroke_1px;
        v.widgets.noninteractive.fg_stroke = Stroke::new(1.0, text);
        v.widgets.noninteractive.corner_radius = r;
        v.widgets.inactive.bg_fill = field_bg;
        v.widgets.inactive.weak_bg_fill = ink_alt;
        v.widgets.inactive.bg_stroke = stroke_1px;
        v.widgets.inactive.fg_stroke = Stroke::new(1.0, text);
        v.widgets.inactive.corner_radius = r;
        v.widgets.hovered.bg_fill = field_hover;
        // #698: light hovers field_hover, not the accent (egui's bare title
        // glyphs took white-on-accent ink), and its title bars wear ink_alt.
        let light = super::ground_ink::light_ground(ink);
        let hover = if light { field_hover } else { accent };
        v.widgets.hovered.weak_bg_fill = hover;
        v.widgets.hovered.bg_stroke = Stroke::new(1.0, accent_hover);
        // Glyphs ride the hover fill (M6.1 D): WHITE on steel blue, black on pale.
        v.widgets.hovered.fg_stroke = Stroke::new(1.0, contrast::on_fill(hover));
        v.widgets.hovered.corner_radius = r;
        v.widgets.active.bg_fill = field_active;
        v.widgets.active.weak_bg_fill = accent_hover;
        v.widgets.active.bg_stroke = Stroke::new(1.0, accent_hover);
        // The active (pressed) fill is the HOVER accent, one step
        // lighter - and there white only reads 3.8:1 on dark's blue.
        // The derivation picks black (5.5:1): pressed chrome is
        // momentary, readable chrome is the point. The first tables
        // hand-typed white here; the arithmetic now overrules the
        // habit, which is exactly what M6.1 D exists to do.
        v.widgets.active.fg_stroke = Stroke::new(1.0, contrast::on_fill(accent_hover));
        v.widgets.active.corner_radius = r;
        v.widgets.open.bg_fill = field_bg;
        if light {
            (v.widgets.open.weak_bg_fill, v.widgets.open.fg_stroke) = (ink_alt, Stroke::new(1.0, text));
        }
        v.widgets.open.bg_stroke = stroke_1px;
        v.widgets.open.corner_radius = r;
        for exp in [
            &mut v.widgets.noninteractive.expansion,
            &mut v.widgets.inactive.expansion,
            &mut v.widgets.hovered.expansion,
            &mut v.widgets.active.expansion,
            &mut v.widgets.open.expansion,
        ] {
            *exp = 0.0; // flat chrome: hovered widgets must not bleed past their bounds
        }
        let _ = text_faint; // read by the painted path's disabled ink, not Visuals
        v
    }

    /// The style spacing the table owns: item gaps, button padding, the
    /// field height. Defaults equal egui's own numbers, so wiring the
    /// tokens in changes no pixels until a theme file overrides them.
    #[must_use]
    pub fn style(&self, base: &Style) -> Style {
        let mut s = base.clone();
        // Clamp at the seam (the M6.1 law's other half): the const guard
        // proves the built-in tables are in range; a file-supplied
        // overlay is checked here, where it can no longer hurt.
        s.spacing.item_spacing =
            vec2(clamp::range(self.item_spacing_x, clamp::SPACING), clamp::range(self.item_spacing_y, clamp::SPACING));
        s.spacing.button_padding =
            vec2(clamp::range(self.button_padding_x, clamp::SPACING), clamp::range(self.button_padding_y, clamp::SPACING));
        s.spacing.interact_size =
            vec2(s.spacing.interact_size.x, clamp::range(self.control_height, clamp::CONTROL_H));
        s.spacing.window_margin = Margin::same(self.item_spacing_y.max(6.0) as i8);
        s
    }
}

/// The exhaustiveness guard, compiled: every field is named in this
/// arm, so the table can never gain a member that no code path reads.
/// Geometry gets its sanity window here (the same clamps the draw sites
/// re-apply to file-supplied values); colours have no range to check.
/// Both built-in columns wear the check (M6.1 C).
const fn geom_legal(t: &Tokens) -> bool {
    let Tokens {
        ink: _, ink_alt: _, well: _, text: _, text_dim: _, text_faint: _,
        border: _, accent: _, accent_hover: _, field_bg: _, field_hover: _,
        field_active: _, selection_bg: _, selection_stroke: _, viewport: _,
        radius_widget, radius_window, radius_menu,
        item_spacing_x, item_spacing_y, button_padding_x, button_padding_y,
        control_height, row_height, cell_h, cell_w, glyph_px,
        font_label, font_caption, font_row, font_row_suffix,
    } = *t;
    radius_widget <= clamp::RADIUS
        && radius_window <= 24
        && radius_menu <= 24
        && item_spacing_x >= 0.0
        && item_spacing_y >= 0.0
        && button_padding_x >= 0.0
        && button_padding_y >= 0.0
        && control_height >= clamp::CONTROL_H.0
        && control_height <= clamp::CONTROL_H.1
        && row_height >= clamp::CONTROL_H.0
        && row_height <= clamp::CONTROL_H.1
        && cell_h >= clamp::CELL.0
        && cell_h <= clamp::CELL.1
        && cell_w >= clamp::CELL.0
        && cell_w <= clamp::CELL.1
        && glyph_px >= clamp::GLYPH.0
        && glyph_px <= clamp::GLYPH.1
        && font_label >= clamp::FONT.0
        && font_label <= clamp::FONT.1
        && font_caption >= clamp::FONT.0
        && font_caption <= clamp::FONT.1
        && font_row >= clamp::FONT.0
        && font_row <= clamp::FONT.1
        && font_row_suffix >= clamp::FONT.0
        && font_row_suffix <= clamp::FONT.1
}

const _: () = assert!(geom_legal(&Tokens::DARK));
const _: () = assert!(geom_legal(&Tokens::LIGHT));

/// The live theme in force, one cell per process. Every consumer reads
/// it through `current()`; the ONLY writer is `set_current`, and the
/// only caller of that is the shell's per-frame theme sync (M6.1 C) -
/// no pane, command, or system pokes a table in. The default is DARK,
/// so a process that never syncs (the kittest frames, the headless
/// proof) renders exactly the pre-C shell.
static LIVE: std::sync::OnceLock<std::sync::RwLock<Tokens>> = std::sync::OnceLock::new();

fn live() -> &'static std::sync::RwLock<Tokens> {
    LIVE.get_or_init(|| std::sync::RwLock::new(Tokens::DARK))
}

/// The theme in force. A poisoned cell still answers (Tokens is Copy,
/// so a panicked writer left either the old or the new table - both
/// legal); the read never crashes the frame.
#[must_use]
pub fn current() -> Tokens {
    *live().read().unwrap_or_else(|p| p.into_inner())
}

/// Install a token table as the live theme WITHOUT touching any
/// context - the half of the swap the sync chooses not to take (a
/// refused file theme keeps the previous table live; only the status
/// line says so). `apply` is the paired install; nothing else writes.
pub fn set_current(t: &Tokens) {
    *live().write().unwrap_or_else(|p| p.into_inner()) = *t;
}

/// Install a token table into a context: visuals and spacing together,
/// so no theme can half-apply (Prime Directive, chrome edition). BOTH of
/// egui's theme slots get the table (#506): egui follows the OS
/// appearance, and writing only the slot active at startup (dark, before
/// the first input names the system theme) left a light-mode Mac on
/// egui's stock light visuals while the painted tree drew this table's
/// ink on it. The document's theme is the only theme; the OS never picks.
pub fn apply_to(ctx: &egui::Context, t: &Tokens) {
    let visuals = t.visuals();
    for slot in [egui::Theme::Dark, egui::Theme::Light] {
        let mut style = t.style(&ctx.style_of(slot));
        style.visuals = visuals.clone();
        ctx.set_style_of(slot, style);
    }
}

/// Install the live theme. Called per Context (windowed); the switcher
/// re-calls it on change (M6.1 C).
pub fn apply(ctx: &egui::Context) {
    apply_to(ctx, &current());
}
