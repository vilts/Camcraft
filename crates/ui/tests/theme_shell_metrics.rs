//! Pinning tests for issue #2064: theme-driven status bar height, ribbon tab
//! horizontal metrics, and explicit window margin.

use camcraft_ui::egui_app::{theme, theme_files};

fn theme_json(geom: &str) -> String {
    format!(
        r#"{{
  "format": "{}",
  "descriptor": {{
    "name": "Pin 2064",
    "creator": "test",
    "description": "issue 2064 pin"
  }},
  "base": "dark",
  "geometry": {{ {geom} }}
}}"#,
        theme_files::FORMAT
    )
}

#[test]
fn window_margin_comes_from_own_token_not_item_spacing() {
    let base = egui::Style::default();
    assert_eq!(
        theme::Tokens::DARK.style(&base).spacing.window_margin,
        egui::Margin::same(6),
        "default dark window_margin stays 6"
    );
    assert_eq!(
        theme::Tokens::LIGHT.style(&base).spacing.window_margin,
        egui::Margin::same(6),
        "default light window_margin stays 6"
    );

    let mut spaced = theme::Tokens::DARK;
    spaced.item_spacing_y = 14.0;
    assert_eq!(
        spaced.style(&base).spacing.window_margin,
        egui::Margin::same(6),
        "item_spacing_y must not override window_margin"
    );

    let parsed = theme_files::parse_text(&theme_json(r#""window_margin": 11"#), "pin-window-margin")
        .expect("window_margin key must parse");
    assert!(parsed.notes.is_empty(), "unexpected notes: {:?}", parsed.notes);
    assert_eq!(
        parsed.tokens.style(&base).spacing.window_margin,
        egui::Margin::same(11),
        "style() reads explicit window_margin token"
    );
}

#[test]
fn status_h_and_ribbon_tab_keys_parse_and_clamp() {
    assert_eq!(theme::status_h(&theme::Tokens::DARK), 22.0);
    assert_eq!(theme::status_h(&theme::Tokens::LIGHT), 22.0);
    assert_eq!(theme::clamp::RADIUS_WINDOW, 24);

    let base = egui::Style::default();
    let valid = theme_files::parse_text(
        &theme_json(
            r#""status_h": 34.0, "window_margin": 10.0, "ribbon_tab_pad_x": 18.0, "ribbon_tab_gap": 6.0, "ribbon_tab_stroke": 4.0"#,
        ),
        "pin-valid-keys",
    )
    .expect("new geometry keys must parse cleanly");
    assert!(valid.notes.is_empty(), "unexpected notes: {:?}", valid.notes);
    assert_eq!(valid.tokens.window_margin, 10.0);
    assert_eq!(valid.tokens.status_h, 34.0);
    assert_eq!(valid.tokens.ribbon_tab_pad_x, 18.0);
    assert_eq!(valid.tokens.ribbon_tab_gap, 6.0);
    assert_eq!(valid.tokens.ribbon_tab_stroke, 4.0);
    assert_eq!(theme::status_h(&valid.tokens), 34.0);

    let low = theme_files::parse_text(
        &theme_json(
            r#""window_margin": -5.0, "status_h": 2.0, "ribbon_tab_pad_x": -10.0, "ribbon_tab_gap": -4.0, "ribbon_tab_stroke": 0.0"#,
        ),
        "pin-clamp-low",
    )
    .expect("out-of-range low geometry values must clamp without failing parse");
    assert_eq!(
        low.tokens.style(&base).spacing.window_margin,
        egui::Margin::same(0),
        "negative window_margin clamps to 0"
    );
    assert_eq!(low.tokens.status_h, theme::clamp::STATUS_H.0);
    assert_eq!(low.tokens.ribbon_tab_pad_x, theme::clamp::RIBBON_TAB_PAD_X.0);
    assert_eq!(low.tokens.ribbon_tab_gap, theme::clamp::RIBBON_GAP.0);
    assert_eq!(low.tokens.ribbon_tab_stroke, theme::clamp::RIBBON_STROKE.0);

    let high = theme_files::parse_text(
        &theme_json(
            r#""radius_window": 99.0, "window_margin": 999.0, "status_h": 999.0, "ribbon_tab_pad_x": 999.0, "ribbon_tab_gap": 999.0, "ribbon_tab_stroke": 999.0"#,
        ),
        "pin-clamp-high",
    )
    .expect("out-of-range high geometry values must clamp without failing parse");
    assert_eq!(high.tokens.radius_window, theme::clamp::RADIUS_WINDOW);
    assert_eq!(high.tokens.window_margin, theme::clamp::WINDOW_MARGIN.1);
    assert!(
        high.tokens.style(&base).spacing.window_margin.left <= 24,
        "huge window_margin clamps to sane upper bound: {:?}",
        high.tokens.style(&base).spacing.window_margin
    );
    assert_eq!(high.tokens.status_h, theme::clamp::STATUS_H.1);
    assert_eq!(high.tokens.ribbon_tab_pad_x, theme::clamp::RIBBON_TAB_PAD_X.1);
    assert_eq!(high.tokens.ribbon_tab_gap, theme::clamp::RIBBON_GAP.1);
    assert_eq!(high.tokens.ribbon_tab_stroke, theme::clamp::RIBBON_STROKE.1);

    for file in ["horizon-day.json", "teal-night.json"] {
        let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../assets/themes")
            .join(file);
        let parsed = theme_files::load(&path).unwrap_or_else(|e| panic!("{file}: {e}"));
        assert_eq!(theme::status_h(&parsed.tokens), 22.0, "{file}: default status_h");
        assert_eq!(parsed.tokens.window_margin, 6.0, "{file}: default window_margin");
    }
}

#[test]
fn merged_shell_and_main_geometry_keep_overrides_clamps_and_refusals() {
    let geometry = r#""window_margin": 11.0, "status_h": 35.0,
        "ribbon_tab_pad_x": 19.0, "ribbon_tab_gap": 7.0, "ribbon_tab_stroke": 4.0,
        "font_label": 9.0, "font_label_min": 30.0,
        "op_tab_h": 999.0, "tree_indent": -1.0"#;
    let parsed = theme_files::parse_text(&theme_json(geometry), "pin-merged-geometry")
        .expect("shell metrics and main geometry must parse together");
    assert!(parsed.notes.is_empty(), "unexpected notes: {:?}", parsed.notes);
    let t = parsed.tokens;
    assert_eq!(t.window_margin, 11.0);
    assert_eq!(t.status_h, 35.0);
    assert_eq!(t.ribbon_tab_pad_x, 19.0);
    assert_eq!(t.ribbon_tab_gap, 7.0);
    assert_eq!(t.ribbon_tab_stroke, 4.0);
    assert_eq!(t.font_label_min, 9.0, "minimum label font cannot exceed label font");
    assert_eq!(t.op_tab_h, theme::clamp::CONTROL_H.1);
    assert_eq!(t.tree_indent, theme::clamp::SPACING.0);

    let unknown = format!(r#"{geometry}, "unknown_shell_metric": 1.0"#);
    let why = theme_files::parse_text(&theme_json(&unknown), "pin-merged-unknown")
        .expect_err("an unknown token must still refuse the whole theme");
    assert!(why.contains("unknown_shell_metric"), "refusal must name the token: {why}");
}
