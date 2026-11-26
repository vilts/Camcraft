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
pub(super) fn line_h(px: f32) -> f32 {
    px * 1.3
}

fn theme_tokens(file: &str) -> Tokens {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/themes").join(file);
    match theme_files::load(&path) {
        Ok(t) => t.tokens,
        Err(why) => panic!("shipped theme {file} must load: {why}"),
    }
}

pub(super) fn each_theme() -> [(&'static str, Tokens); 2] {
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


mod issue_2064_pins {
    use std::path::PathBuf;
    use camcraft_app::commands::theme_set;
    use camcraft_app::schema::ThemeRef;

    struct TempTheme(PathBuf);

    impl TempTheme {
        fn write(tag: &str, geom: &str) -> Self {
            let path = std::env::temp_dir().join(format!(
                "camcraft_issue2064_{}_{}_{}.json",
                std::process::id(),
                tag,
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map_or(0, |d| d.as_nanos())
            ));
            let json = format!(
                r#"{{
  "format": "camcraft-theme/1",
  "descriptor": {{
    "name": "Pin {tag}",
    "creator": "test",
    "description": "issue 2064 pin"
  }},
  "base": "dark",
  "geometry": {{ {geom} }}
}}"#
            );
            std::fs::write(&path, json).expect("write temp theme");
            Self(path)
        }
    }

    impl Drop for TempTheme {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(&self.0);
        }
    }

    fn apply_geom(h: &mut egui_kittest::Harness<'static, super::super::ShellUi>, tag: &str, geom: &str) -> TempTheme {
        let f = TempTheme::write(tag, geom);
        let p = f.0.to_string_lossy().into_owned();
        theme_set(&h.state_mut().state.world, &ThemeRef::File(p));
        h.run_steps(3);
        f
    }

    fn find_text_x(h: &egui_kittest::Harness<'static, super::super::ShellUi>, needle: &str) -> f32 {
        super::super::structural::painted(h)
            .into_iter()
            .find(|(text, _)| text == needle)
            .unwrap_or_else(|| panic!("missing painted text {needle:?}"))
            .1
            .x
    }

    fn walk_accent_lines(shape: &egui::Shape, accent: egui::Color32, out: &mut Vec<f32>) {
        match shape {
            egui::Shape::LineSegment { points, stroke }
                if stroke.color == accent && points[0].y < 60.0 && (points[0].y - points[1].y).abs() < 1.0 =>
            {
                out.push(stroke.width);
            }
            egui::Shape::Vec(v) => {
                for s in v {
                    walk_accent_lines(s, accent, out);
                }
            }
            _ => {}
        }
    }

    fn walk_status_rects(
        shape: &egui::Shape,
        center_y: f32,
        width: f32,
        strip_fill: egui::Color32,
        out: &mut Vec<f32>,
    ) {
        match shape {
            egui::Shape::Rect(r)
                if r.fill == strip_fill
                    && r.rect.top() > center_y
                    && r.rect.width() >= width * 0.5
                    && r.rect.height() >= 8.0
                    && r.rect.height() <= 120.0 =>
            {
                out.push(r.rect.height());
            }
            egui::Shape::Vec(v) => {
                for s in v {
                    walk_status_rects(s, center_y, width, strip_fill, out);
                }
            }
            _ => {}
        }
    }

    fn bottom_strip_height(h: &egui_kittest::Harness<'static, super::super::ShellUi>) -> f32 {
        let cr = h.ctx.content_rect();
        let strip_fill: egui::Color32 = super::super::theme::current().strip_fill();
        let mut heights = Vec::new();
        for c in &h.output().shapes {
            walk_status_rects(&c.shape, cr.center().y, cr.width(), strip_fill, &mut heights);
        }
        heights.into_iter().next().expect("status strip background rect must paint")
    }

    #[test]
    fn status_strip_height_tracks_theme_controls_and_status_h() {
        let _g = super::super::snapshot_guard();
        let mut h = super::super::shell_harness();
        h.run_steps(3);
        let base_strip = bottom_strip_height(&h);

        let _t1 = apply_geom(&mut h, "status_controls", r#""control_height": 34.0"#);
        let control_strip = bottom_strip_height(&h);
        assert!(
            control_strip >= base_strip + 12.0,
            "increasing control_height to 34 must grow the status bar (strip stayed {control_strip} vs {base_strip})"
        );

        let _t2 = apply_geom(&mut h, "status_font", r#""font_row": 26.0"#);
        let font_strip = bottom_strip_height(&h);
        assert!(
            font_strip >= base_strip + 10.0,
            "increasing font_row to 26 must grow the status bar (strip stayed {font_strip} vs {base_strip})"
        );

        let _t3 = apply_geom(&mut h, "status_h_direct", r#""status_h": 42.0"#);
        let explicit_strip = bottom_strip_height(&h);
        assert!(
            (explicit_strip - base_strip - 20.0).abs() <= 1.5,
            "status_h=42 should grow status bar by 20px from 22 (delta = {})",
            explicit_strip - base_strip
        );

        theme_set(&h.state_mut().state.world, &ThemeRef::Dark);
        h.run_steps(2);
    }

    #[test]
    fn ribbon_tabs_follow_theme_spacing_pad_gap_and_stroke() {
        let _g = super::super::snapshot_guard();
        let mut h = super::super::shell_harness();
        h.run_steps(3);
        let base_milling = find_text_x(&h, "MILLING");
        let base_turning = find_text_x(&h, "TURNING");

        let _t1 = apply_geom(&mut h, "tab_inset", r#""item_spacing_x": 20.0"#);
        let inset_milling = find_text_x(&h, "MILLING");
        assert!(
            (inset_milling - base_milling - 12.0).abs() <= 1.5,
            "increasing item_spacing_x from 8 to 20 must shift first tab right by 12px (got {})",
            inset_milling - base_milling
        );

        let _t2 = apply_geom(
            &mut h,
            "tab_metrics",
            r#""ribbon_tab_pad_x": 22.0, "ribbon_tab_gap": 10.0, "ribbon_tab_stroke": 5.0"#,
        );
        let pad_milling = find_text_x(&h, "MILLING");
        let pad_turning = find_text_x(&h, "TURNING");
        let pitch_delta = (pad_turning - pad_milling) - (base_turning - base_milling);
        assert!(
            (pad_milling - base_milling - 10.0).abs() <= 1.5,
            "increasing ribbon_tab_pad_x from 12 to 22 must shift centered MILLING right by 10px (got {})",
            pad_milling - base_milling
        );
        assert!(
            (pitch_delta - 28.0).abs() <= 1.5,
            "pad_x +10/side and gap +8 must widen tab pitch by 28px (got {pitch_delta})"
        );

        let accent = super::super::theme::Tokens::DARK.accent;
        let mut widths = Vec::new();
        for c in &h.output().shapes {
            walk_accent_lines(&c.shape, accent, &mut widths);
        }
        assert!(
            widths.iter().any(|w| (*w - 5.0).abs() <= 0.1),
            "active tab underline stroke must read ribbon_tab_stroke=5.0, got {widths:?}"
        );

        theme_set(&h.state_mut().state.world, &ThemeRef::Dark);
        h.run_steps(2);
    }
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
