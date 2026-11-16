//! #2021: each theme supplies the ribbon's row heights, gaps and glyph ink.
//! Structural checks only: tokens of each shipped theme file and the painted
//! text positions. The tab text and the caption text fit in their rows, the
//! rows stack without overlap, and the glyph ink reads on the ribbon ground.
//! Provenance: our own tests over our own ribbon and theme files.

use super::structural::painted;
use super::*;
use crate::egui_app::{contrast, theme_files};
use crate::egui_app::theme::{Tokens, ribbon_h};

/// Text height of one line: the egui line box is about 1.2 to 1.3 times
/// the font size, so 1.3 is the estimate here.
fn line_h(px: f32) -> f32 {
    px * 1.3
}

fn theme_tokens(file: &str) -> Tokens {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/themes").join(file);
    match theme_files::load(&path) {
        Ok(t) => t.tokens,
        Err(why) => panic!("shipped theme {file} must load: {why}"),
    }
}

fn each_theme() -> [(&'static str, Tokens); 2] {
    [("horizon-day", theme_tokens("horizon-day.json")), ("teal-night", theme_tokens("teal-night.json"))]
}

#[test]
fn each_theme_has_the_ribbon_rows_of_the_table_or_larger() {
    for (name, t) in each_theme() {
        assert!(t.ribbon_tab_h >= 24.0, "{name}: tab row {}", t.ribbon_tab_h);
        assert!(t.ribbon_cell_top_gap >= 4.0, "{name}: cell top gap");
        assert!(t.ribbon_caption_gap >= 8.0, "{name}: caption gap");
        assert!(t.ribbon_caption_row_h >= 18.0, "{name}: caption row");
        assert!(t.ribbon_bottom_gap >= 4.0, "{name}: bottom gap");
        assert!(ribbon_h(&t) >= 88.0, "{name}: ribbon {} px", ribbon_h(&t));
    }
}

#[test]
fn tab_text_and_caption_text_fit_their_rows_in_each_theme() {
    for (name, t) in each_theme() {
        // One tab line with 4 px clear above and below.
        assert!(t.ribbon_tab_h >= line_h(t.font_label) + 8.0, "{name}: tab text does not fit its row");
        // One caption line with 2 px clear above and below.
        assert!(t.ribbon_caption_row_h >= line_h(t.font_caption) + 4.0, "{name}: caption text does not fit its row");
        // The caption row starts clear of the button label under the glyph.
        let label_bottom = t.glyph_px + 2.0 + line_h(t.font_label);
        assert!(t.cell_h + t.ribbon_caption_gap >= label_bottom + 2.0, "{name}: caption touches the button label");
        // The rows stack inside the strip: the sum is the ribbon height.
        let rows_end = t.ribbon_tab_h + t.ribbon_cell_top_gap + t.cell_h + t.ribbon_caption_gap + t.ribbon_caption_row_h;
        assert!((ribbon_h(&t) - (rows_end + t.ribbon_bottom_gap)).abs() < f32::EPSILON, "{name}: stray ribbon space");
    }
}

#[test]
fn ribbon_ink_reads_on_the_ribbon_ground_in_each_theme() {
    for (name, t) in each_theme() {
        let r = contrast::ratio(t.ribbon_ink, t.strip_fill());
        assert!(r >= 4.5, "{name}: ribbon ink reads {r:.1}:1 on the ribbon ground");
    }
    for (name, t) in [("DARK", Tokens::DARK), ("LIGHT", Tokens::LIGHT)] {
        let r = contrast::ratio(t.ribbon_ink, t.strip_fill());
        assert!(r >= 4.5, "{name}: ribbon ink reads {r:.1}:1");
    }
    assert_eq!(theme_tokens("teal-night.json").ribbon_ink, egui::Color32::WHITE, "dark theme: white glyphs");
}

#[test]
fn a_theme_with_unreadable_ribbon_ink_is_refused() {
    let text = r##"{"format":"camcraft-theme/1","descriptor":{"name":"x"},"base":"dark","colors":{"ribbon_ink":"#2C3037"}}"##;
    let why = theme_files::parse_text(text, "t.json").err().unwrap_or_default();
    assert!(why.contains("ribbon_ink"), "refusal names the token: {why}");
}

#[test]
fn ribbon_text_rows_stack_top_to_bottom() {
    let mut h = shell_harness();
    h.run_steps(3);
    let texts = painted(&h);
    let find = |want: &str| texts.iter().find(|(t, _)| t == want).map(|(_, p)| *p);
    let tab_caption = crate::machine_modes::MACHINE_MODES[0].1.to_uppercase();
    let tab = find(&tab_caption).expect("the first machine tab paints its caption");
    let new = find("New").expect("the New cell paints its label");
    let caption = find("FILE").expect("the FILE caption paints");
    let pal = theme::current();
    assert!(new.y - tab.y >= line_h(pal.font_label), "tab text runs into the button labels");
    assert!(caption.y - new.y >= line_h(pal.font_label) + 2.0, "caption touches the button label");
}

#[derive(Clone, Copy)]
struct CaptionProbe {
    font_caption: f32,
    painted: bool,
}

fn caption_job(shape: &egui::Shape) -> Option<std::sync::Arc<egui::text::LayoutJob>> {
    match shape {
        egui::Shape::Text(t) if t.galley.text() == "GROUP" => Some(t.galley.job.clone()),
        egui::Shape::Vec(v) => v.iter().find_map(caption_job),
        _ => None,
    }
}

fn painted_caption_size(font_caption: f32) -> f32 {
    let strip = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::Vec2::new(400.0, 120.0));
    let mut h = Harness::builder()
        .with_size(strip.size())
        .build_ui_state(
            move |ui, st: &mut CaptionProbe| {
                let mut t = Tokens::DARK;
                t.font_caption = st.font_caption;
                st.painted = super::super::painted::caption(ui, 200.0, 360.0, "Group", strip, &t, 12.0);
            },
            CaptionProbe { font_caption, painted: false },
        );
    h.run();
    assert!(h.state().painted, "caption must fit inside 360 px span");
    let job = h
        .output()
        .shapes
        .iter()
        .find_map(|c| caption_job(&c.shape))
        .expect("GROUP caption galley painted");
    job.sections[0].format.font_id.size
}

#[test]
fn builtin_caption_font_fits_caption_row() {
    for (name, t) in [("DARK", Tokens::DARK), ("LIGHT", Tokens::LIGHT)] {
        assert!(
            t.ribbon_caption_row_h >= line_h(t.font_caption) + 4.0,
            "{name}: font_caption {} px exceeds ribbon_caption_row_h {} px",
            t.font_caption,
            t.ribbon_caption_row_h
        );
    }
}

#[test]
fn caption_uses_font_caption_token() {
    assert_eq!(painted_caption_size(16.0), 16.0);
    assert_eq!(painted_caption_size(7.0), 7.0);
}

#[test]
fn caption_clamps_out_of_range_font_caption() {
    let lo = crate::egui_app::theme::clamp::FONT.0;
    let hi = crate::egui_app::theme::clamp::FONT.1;
    assert_eq!(painted_caption_size(1.0), lo);
    assert_eq!(painted_caption_size(100.0), hi);
    assert_eq!(painted_caption_size(f32::NAN), lo);
}
