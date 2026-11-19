//! Provenance: our own theme vocabulary and dropdown dimension regression tests.
//! Gate map (M4.4): covers ui::egui_app::theme_files, ui::egui_app::theme.

use super::*;
use camcraft_ui::egui_app::theme::clamp;

#[test]
fn dropdown_tokens_parse_and_clamp() {
    for base in ["dark", "light"] {
        let text = format!(r#"{{"format":"camcraft-theme/1","descriptor":{{"name":"Default drops"}},"base":"{base}"}}"#);
        let default = tf::parse_text(&text, "defaults").expect("default dropdown geometry");
        assert_eq!(default.tokens.menu_width, 200.0);
        assert_eq!(default.tokens.recent_width, 300.0);
        assert_eq!(default.tokens.menu_gap, 2.0);
        assert_eq!(default.tokens.menu_pad_x, 8.0);
    }
    let custom = tf::parse_text(
        r#"{"format":"camcraft-theme/1","descriptor":{"name":"Drops"},"base":"dark",
            "geometry":{"menu_width":260,"recent_width":360,"menu_gap":10,"menu_pad_x":16}}"#,
        "drops",
    ).expect("dropdown tokens must be known geometry keys");
    assert_eq!(custom.tokens.menu_width, 260.0);
    assert_eq!(custom.tokens.recent_width, 360.0);
    assert_eq!(custom.tokens.menu_gap, 10.0);
    assert_eq!(custom.tokens.menu_pad_x, 16.0);
    let clamped = tf::parse_text(
        r#"{"format":"camcraft-theme/1","descriptor":{"name":"Hostile drops"},"base":"light",
            "geometry":{"menu_width":5,"recent_width":5000,"menu_gap":-9,"menu_pad_x":999}}"#,
        "hostile_drops",
    ).expect("hostile dropdown geometry clamps");
    assert_eq!(clamped.tokens.ink, Tokens::LIGHT.ink);
    assert_eq!(clamped.tokens.menu_width, clamp::MENU_W.0);
    assert_eq!(clamped.tokens.recent_width, clamp::RECENT_W.1);
    assert_eq!(clamped.tokens.menu_gap, clamp::MENU_GAP.0);
    assert_eq!(clamped.tokens.menu_pad_x, clamp::MENU_PAD_X.1);
    let reverse = tf::parse_text(
        r#"{"format":"camcraft-theme/1","descriptor":{"name":"Other bounds"},"base":"dark",
            "geometry":{"menu_width":5000,"recent_width":5,"menu_gap":999,"menu_pad_x":-9}}"#,
        "other_bounds",
    ).expect("both ends of every clamp apply");
    assert_eq!(reverse.tokens.menu_width, clamp::MENU_W.1);
    assert_eq!(reverse.tokens.recent_width, clamp::RECENT_W.0);
    assert_eq!(reverse.tokens.menu_gap, clamp::MENU_GAP.1);
    assert_eq!(reverse.tokens.menu_pad_x, clamp::MENU_PAD_X.0);
}

#[test]
fn the_theme_example_lists_every_supported_token() {
    let text = include_str!("../../../../assets/themes/theme.json.example");
    let example: serde_json::Value = serde_json::from_str(text).expect("valid JSON example");
    for (field, names) in [("colors", tf::COLOR_NAMES), ("geometry", tf::GEOM_NAMES)] {
        let keys: std::collections::BTreeSet<_> = example[field]
            .as_object().expect("example token map").keys().map(String::as_str).collect();
        let expected: std::collections::BTreeSet<_> = names.iter().copied().collect();
        assert_eq!(keys, expected, "{field} example must match the parser vocabulary");
    }
    tf::parse_text(text, "theme.json.example").expect("the complete example must parse");
}
