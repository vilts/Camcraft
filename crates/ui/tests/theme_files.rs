//! The `camcraft-theme/1` format, end to end (M6.1 B): the shipped
//! worked example, the overlay law (every override lands clamped), the
//! hostile-input refusals (each must name its rule), the byte budget,
//! the listing walk, and the parse cache's hit identity. The switcher
//! that consumes all this arrives with stage C; this file pins the
//! doors it will open.
//! Provenance: our own tests, specced from the ROADMAP's M6.1 notes.
//!
//! Gate map (M4.4): covers ui::egui_app::theme_files, ui::egui_app::theme,
//! ui::egui_app::contrast.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use camcraft_ui::egui_app::contrast;
use camcraft_ui::egui_app::theme::Tokens;
use camcraft_ui::egui_app::theme_files as tf;
use egui::Color32;

#[path = "theme_files/dropdown.rs"]
mod dropdown;

fn tmp_dir(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("camcraft_ui_theme_{tag}_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).expect("make the fixture dir");
    d
}

fn write(dir: &Path, name: &str, text: &str) -> PathBuf {
    let p = dir.join(name);
    std::fs::write(&p, text).expect("write the fixture file");
    p
}

/// The repo's own example theme: parsed, applied, and every untouched
/// field still DARK's number.
#[test]
fn the_shipped_example_parses() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/themes/teal-night.json");
    let theme = tf::load(&path).unwrap_or_else(|e| panic!("the shipped example must load: {e}"));
    assert_eq!(theme.name, "Teal Night");
    assert_eq!(theme.creator, "camcraft");
    // The worked example spells accent, hover, selection_bg and stroke,
    // and leaves field_active to M6.1 D's law - so it carries exactly
    // that one honest derivation note, and no reports.
    assert_eq!(theme.notes.len(), 1, "{:?}", theme.notes);
    assert!(theme.notes[0].contains("field_active"), "{:?}", theme.notes);
    assert_eq!(theme.tokens.accent, Color32::from_rgb(0x2A, 0x7F, 0x79));
    assert_eq!(theme.tokens.accent_hover, Color32::from_rgb(0x3C, 0xA0, 0x99));
    assert_eq!(theme.tokens.selection_stroke, theme.tokens.accent_hover, "the ride law, spelled to match");
    // The one derived state: dark's field_active step translated onto
    // teal. In linear light the dark step is (r -2.5%, g -12.9%,
    // b -35.0%) off the accent; teal's blue is already below that
    // sink, so red and blue clamp out and green carries the step -
    // #005200. The exact number IS the law: pinned so a changed
    // arithmetic is a changed test, never a silent restyle.
    assert_eq!(theme.tokens.field_active, Color32::from_rgb(0x00, 0x52, 0x00));
    assert_eq!(theme.tokens.radius_widget, 3);
    assert_eq!(theme.tokens.glyph_px, 22.0);
    // Untouched fields keep the dark column's numbers.
    assert_eq!(theme.tokens.ink, Color32::from_rgb(0x23, 0x26, 0x2B));
    assert_eq!(theme.tokens.cell_h, Tokens::DARK.cell_h);
    assert_eq!(theme.tokens.font_row_suffix, Tokens::DARK.font_row_suffix);
}

/// The ugly-but-legal law: a file can make the shell odd, never
/// impossible - every hostile geometry value lands inside its clamp
/// window instead of refusing (the refusals are for SHAPE, the clamps
/// for size).
#[test]
fn hostile_geometry_lands_clamped() {
    let t = tf::parse_text(
        r#"{"format":"camcraft-theme/1","descriptor":{"name":"Pushy"},"base":"dark",
            "geometry":{"radius_widget":400,"radius_window":1000,"cell_h":0,
            "font_row":999,"item_spacing_x":-50,"control_height":7,"glyph_px":5}}"#,
        "pushy",
    )
    .expect("a big number is ugly, not corrupt");
    assert_eq!(t.tokens.radius_widget, 12); // clamp::RADIUS
    assert_eq!(t.tokens.radius_window, 24); // the window/menu cap
    assert_eq!(t.tokens.cell_h, 24.0); // CELL floor
    assert_eq!(t.tokens.font_row, 32.0); // FONT ceiling
    assert_eq!(t.tokens.item_spacing_x, 0.0); // SPACING floor
    assert_eq!(t.tokens.control_height, 12.0); // CONTROL_H floor
    assert_eq!(t.tokens.glyph_px, 8.0); // GLYPH floor
}

/// #2066: every new op-tab, browser-tree, min-label and stroke token
/// fits inside MAX_MAP_ENTRIES and clamps at both ends of its window.
#[test]
fn op_tab_and_tree_tokens_fit_cap_and_clamp() {
    assert!(
        tf::GEOM_NAMES.len() <= tf::MAX_MAP_ENTRIES,
        "{} geometry tokens must fit MAX_MAP_ENTRIES ({})",
        tf::GEOM_NAMES.len(),
        tf::MAX_MAP_ENTRIES,
    );
    let low = tf::parse_text(
        r#"{"format":"camcraft-theme/1","descriptor":{"name":"Lo2066"},"base":"dark",
            "geometry":{"font_label_min":-9,"op_tab_h":-9,"op_tab_pad_x":-9,"op_tab_gap":-9,
            "op_tab_bottom_gap":-9,"op_tab_icon_px":-9,"op_tab_icon_gap":-9,"op_tab_radius":-9,
            "font_op_tab":-9,"tree_glyph_px":-9,"tree_inset_x":-9,"tree_indent":-9,
            "tree_eye_reserve":-9,"tree_pad_x":-9,"tree_pad_y":-9,"tree_suffix_gap":-9,
            "tree_header_boost":-9,"highlight_stroke_px":-9}}"#,
        "lo2066",
    )
    .expect("low extreme parses clamped");
    assert_eq!(low.tokens.font_label_min, 6.0);
    assert_eq!(low.tokens.op_tab_h, 12.0);
    assert_eq!(low.tokens.op_tab_pad_x, 0.0);
    assert_eq!(low.tokens.op_tab_gap, 0.0);
    assert_eq!(low.tokens.op_tab_bottom_gap, 0.0);
    assert_eq!(low.tokens.op_tab_icon_px, 8.0);
    assert_eq!(low.tokens.op_tab_icon_gap, 0.0);
    assert_eq!(low.tokens.op_tab_radius, 0);
    assert_eq!(low.tokens.font_op_tab, 6.0);
    assert_eq!(low.tokens.tree_glyph_px, 8.0);
    assert_eq!(low.tokens.tree_inset_x, 0.0);
    assert_eq!(low.tokens.tree_indent, 0.0);
    assert_eq!(low.tokens.tree_eye_reserve, 12.0);
    assert_eq!(low.tokens.tree_pad_x, 0.0);
    assert_eq!(low.tokens.tree_pad_y, 0.0);
    assert_eq!(low.tokens.tree_suffix_gap, 0.0);
    assert_eq!(low.tokens.tree_header_boost, 0.0);
    assert_eq!(low.tokens.highlight_stroke_px, 0.5);

    let hi = tf::parse_text(
        r#"{"format":"camcraft-theme/1","descriptor":{"name":"Hi2066"},"base":"dark",
            "geometry":{"font_label":15.0,"font_label_min":999,"op_tab_h":999,"op_tab_pad_x":999,
            "op_tab_gap":999,"op_tab_bottom_gap":999,"op_tab_icon_px":999,"op_tab_icon_gap":999,
            "op_tab_radius":999,"font_op_tab":999,"tree_glyph_px":999,"tree_inset_x":999,
            "tree_indent":999,"tree_eye_reserve":999,"tree_pad_x":999,"tree_pad_y":999,
            "tree_suffix_gap":999,"tree_header_boost":999,"highlight_stroke_px":999}}"#,
        "hi2066",
    )
    .expect("high extreme parses clamped");
    assert_eq!(hi.tokens.font_label_min, 15.0);
    assert_eq!(hi.tokens.op_tab_h, 64.0);
    assert_eq!(hi.tokens.op_tab_pad_x, 32.0);
    assert_eq!(hi.tokens.op_tab_gap, 32.0);
    assert_eq!(hi.tokens.op_tab_bottom_gap, 24.0);
    assert_eq!(hi.tokens.op_tab_icon_px, 64.0);
    assert_eq!(hi.tokens.op_tab_icon_gap, 32.0);
    assert_eq!(hi.tokens.op_tab_radius, 12);
    assert_eq!(hi.tokens.font_op_tab, 32.0);
    assert_eq!(hi.tokens.tree_glyph_px, 64.0);
    assert_eq!(hi.tokens.tree_inset_x, 32.0);
    assert_eq!(hi.tokens.tree_indent, 32.0);
    assert_eq!(hi.tokens.tree_eye_reserve, 64.0);
    assert_eq!(hi.tokens.tree_pad_x, 32.0);
    assert_eq!(hi.tokens.tree_pad_y, 32.0);
    assert_eq!(hi.tokens.tree_suffix_gap, 32.0);
    assert_eq!(hi.tokens.tree_header_boost, 12.0);
    assert_eq!(hi.tokens.highlight_stroke_px, 8.0);
}

/// The whole vocabulary lands: colours by name, alpha included.
#[test]
fn colours_override_by_name() {
    let t = tf::parse_text(
        r##"{"format":"camcraft-theme/1","descriptor":{"name":"Tinted"},"base":"dark",
            "colors":{"ink":"#101112","well":"#0A0B0CFF","accent_hover":"#AABBCC"}}"##,
        "tinted",
    )
    .expect("three overrides must parse");
    assert_eq!(t.tokens.ink, Color32::from_rgb(0x10, 0x11, 0x12));
    assert_eq!(t.tokens.well, Color32::from_rgba_unmultiplied(0x0A, 0x0B, 0x0C, 0xFF));
    assert_eq!(t.tokens.accent_hover, Color32::from_rgb(0xAA, 0xBB, 0xCC));
    assert_eq!(t.tokens.accent, Color32::from_rgb(0x35, 0x6D, 0xA8), "untouched stays dark");
}

/// Unknown schema keys are reported, unknown TOKENS are refusals -
/// the roadmap's two sentences, kept distinct.
#[test]
fn unknown_keys_report_unknown_tokens_refuse() {
    let t = tf::parse_text(
        r#"{"format":"camcraft-theme/1","descriptor":{"name":"Odd","colourist":"nobody"},"base":"dark","extra":1}"#,
        "odd",
    )
    .expect("unknown keys only report");
    assert!(t.notes.iter().any(|n| n.contains("\"extra\"")), "{:?}", t.notes);
    assert!(t.notes.iter().any(|n| n.contains("\"colourist\"")), "{:?}", t.notes);
    let err = tf::parse_text(
        r##"{"format":"camcraft-theme/1","descriptor":{"name":"Bad"},"base":"dark","colors":{"hover":"#FFF"}}"##,
        "bad",
    )
    .expect_err("\"hover\" is not a token - a theme that sets nothing must not pretend");
    assert!(err.contains("unknown token \"hover\""), "{err}");
    assert!(err.contains("selection_stroke"), "{err}"); // names what IS legal
}

/// The misplaced-token hint: the format has two maps, and saying so
/// turns a paste mistake into a sentence.
#[test]
fn misplaced_tokens_say_where_they_belong() {
    let err = tf::parse_text(
        r##"{"format":"camcraft-theme/1","descriptor":{"name":"Swap"},"base":"dark","colors":{"cell_h":"#FFFFFF"}}"##,
        "swap",
    )
    .expect_err("cell_h is geometry");
    assert!(err.contains("belongs under \"geometry\""), "{err}");
    let err = tf::parse_text(
        r#"{"format":"camcraft-theme/1","descriptor":{"name":"Swap"},"base":"dark","geometry":{"ink":3}}"#,
        "swap",
    )
    .expect_err("ink is a colour");
    assert!(err.contains("belongs under \"colors\""), "{err}");
}

/// Every refusal shape, each named. Nothing here may panic; the
/// refusals ARE the behaviour.
#[test]
fn the_refusal_battery() {
    let long_name = format!(
        r#"{{"format":"camcraft-theme/1","descriptor":{{"name":"{}"}},"base":"dark"}}"#,
        "n".repeat(70)
    );
    let fat_keys = (0..65)
        .map(|i| format!("\"k{i}\":\"#112233\""))
        .collect::<Vec<_>>()
        .join(",");
    let fat_map = format!(
        r#"{{"format":"camcraft-theme/1","descriptor":{{"name":"n"}},"base":"dark","colors":{{{fat_keys}}}}}"#
    );
    let bad: Vec<(&str, &str)> = vec![
        ("not json at all", "not valid JSON"),
        ("[]", "one JSON object"),
        (r#"{"format":"camcraft-theme/2"}"#, "refused whole"),
        (r#"{"descriptor":{"name":"x"},"base":"dark"}"#, "missing \"format\""),
        (r#"{"format":"camcraft-theme/1","base":"dark"}"#, "missing \"descriptor\""),
        (r#"{"format":"camcraft-theme/1","descriptor":{},"base":"dark"}"#, "name is required"),
        (
            r#"{"format":"camcraft-theme/1","descriptor":{"name":"n"},"base":"solarpunk"}"#,
            "unknown theme base",
        ),
        (
            r#"{"format":"camcraft-theme/1","descriptor":{"name":"n"}}"#,
            "missing \"base\"",
        ),
        (
            r#"{"format":"camcraft-theme/1","descriptor":{"name":"n"},"base":"dark","colors":["x"]}"#,
            "must be an object",
        ),
        (
            r##"{"format":"camcraft-theme/1","descriptor":{"name":"n"},"base":"dark","colors":{"accent":["#112233","#445566"]}}"##,
            "ramp array",
        ),
        (
            r##"{"format":"camcraft-theme/1","descriptor":{"name":"n"},"base":"dark","colors":{"accent":"red"}}"##,
            "not \"#rrggbb\"",
        ),
        (
            r##"{"format":"camcraft-theme/1","descriptor":{"name":"n"},"base":"dark","colors":{"accent":"#11223"}}"##,
            "not \"#rrggbb\"",
        ),
        (
            r##"{"format":"camcraft-theme/1","descriptor":{"name":"n"},"base":"dark","colors":{"accent":"#1122334Z"}}"##,
            "not \"#rrggbb\"",
        ),
        (
            r#"{"format":"camcraft-theme/1","descriptor":{"name":"n"},"base":"dark","geometry":{"cell_h":"35"}}"#,
            "must be a number",
        ),
        (
            // serde_json refuses a magnitude past f64 at the parse step,
            // before the module's own finite check can see it; the
            // refusal is still by name, so the battery pins the text.
            r#"{"format":"camcraft-theme/1","descriptor":{"name":"n"},"base":"dark","geometry":{"cell_h":1e999}}"#,
            "number out of range",
        ),
        (
            r#"{"format":"camcraft-theme/1","descriptor":{"name":"n"},"base":"dark","geometry":3}"#,
            "must be an object",
        ),
        (long_name.as_str(), "past the 64-byte cap"),
        (fat_map.as_str(), "past the 64 cap"),
    ];
    for (text, want) in bad {
        let err = tf::parse_text(text, "battery").expect_err(&format!("must refuse: {want}"));
        assert!(err.contains(want), "wanted {want:?}, got {err}");
    }
}

/// The battery case that refused `base:"light"` "when M6.1 C lands"
/// flipped to a support check: the light column is a real table, so a
/// file may build on it, and an overlay on top of light lands on
/// light's numbers - not dark's.
#[test]
fn a_light_base_is_a_real_starting_point() {
    let t = tf::parse_text(
        r#"{"format":"camcraft-theme/1","descriptor":{"name":"L"},"base":"light"}"#,
        "light",
    )
    .expect("M6.1 C landed: light is a base now");
    assert_eq!(t.tokens.text, Tokens::LIGHT.text);
    let u = tf::parse_text(
        r##"{"format":"camcraft-theme/1","descriptor":{"name":"L2"},"base":"light",
            "colors":{"accent":"#FF0000"}}"##,
        "light",
    )
    .expect("an overlay rides the light base");
    assert_eq!(u.tokens.accent, Color32::from_rgb(0xFF, 0x00, 0x00));
    assert_eq!(u.tokens.text, Tokens::LIGHT.text); // untouched: light's
    assert_eq!(u.tokens.cell_h, Tokens::LIGHT.cell_h); // geometry: same in both
}

#[test]
fn the_byte_budget_refuses_before_reading_everything() {
    let dir = tmp_dir("budget");
    let path = write(&dir, "fat.json", &format!(r#"{{"pad":"{}"}}"#, " ".repeat(300 * 1024)));
    let err = tf::load(&path).expect_err("a fat file is not a theme");
    assert!(err.contains("budget"), "{err}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn discovery_lists_sorted_and_says_what_it_skipped() {
    let dir = tmp_dir("discover");
    write(&dir, "zeta.json", "{}"); // malformed content is still listed
    write(&dir, "alpha.json", "{}");
    write(&dir, "notes.txt", "not a theme");
    write(&dir, "sub.json", "{}");
    let (paths, notes) = tf::discover(&dir);
    let names: Vec<String> = paths
        .iter()
        .map(|p| p.file_name().and_then(|n| n.to_str()).unwrap_or_default().to_string())
        .collect();
    assert_eq!(names, vec!["alpha.json", "sub.json", "zeta.json"]);
    assert!(notes.is_empty(), "{notes:?}");
    // A listed file may still refuse at load time - naming, never panicking.
    assert!(tf::load(&paths[0]).is_err());
    let (paths, notes) = tf::discover(&dir.join("no-such-dir"));
    assert!(paths.is_empty() && notes.is_empty(), "a missing dir is a normal state");
    let _ = std::fs::remove_dir_all(&dir);
}

/// M6.1 D, the arithmetic itself: both built-in columns must clear
/// the gate they enforce on files - a no-op file lands its base
/// exactly, so its base had better be legal - and `on_fill`'s choices
/// on the four built-in fills are the snapshot's memory.
#[test]
fn both_builtin_columns_clear_their_own_gate() {
    assert_eq!(contrast::readable(&Tokens::DARK), None, "dark must read");
    assert_eq!(contrast::readable(&Tokens::LIGHT), None, "light must read");
    assert!((contrast::ratio(Color32::BLACK, Color32::WHITE) - 21.0).abs() < 0.01, "the formula's ceiling");
    assert!((contrast::ratio(Color32::WHITE, Color32::WHITE) - 1.0).abs() < 0.001, "and its floor");
    assert_eq!(contrast::on_fill(Tokens::DARK.accent), Color32::WHITE);
    assert_eq!(contrast::on_fill(Tokens::LIGHT.accent), Color32::WHITE);
    // The finding the derivation made for us: the pressed fill (the
    // hover accent, one step lighter) never read 4.5:1 with white -
    // 3.8 dark, 4.3 light - while black clears both. The first
    // tables' hand-typed white there was a bug wearing a habit;
    // visuals() now asks the arithmetic, and pressed chrome reads.
    assert_eq!(contrast::on_fill(Tokens::DARK.accent_hover), Color32::BLACK);
    assert_eq!(contrast::on_fill(Tokens::LIGHT.accent_hover), Color32::BLACK);
}

/// The follow law is an identity on its own base (the built-ins stay
/// byte-for-byte the hand-tuned tables, snapshots intact) and a
/// translation on a new one.
#[test]
fn the_follow_law_round_trips_and_translates() {
    for base in [Tokens::DARK, Tokens::LIGHT] {
        assert_eq!(contrast::follow(base.accent, &base, base.accent_hover), base.accent_hover);
        assert_eq!(contrast::follow(base.accent, &base, base.field_active), base.field_active);
        assert_eq!(contrast::follow(base.accent, &base, base.selection_bg), base.selection_bg);
    }
    // Hover's step on both columns is a lift; onto a rust accent it
    // stays a lift. The number is the law's - pinned.
    let rust = Color32::from_rgb(0xC0, 0x50, 0x4D);
    let hover = contrast::follow(rust, &Tokens::DARK, Tokens::DARK.accent_hover);
    assert_eq!(hover, Color32::from_rgb(0xC5, 0x71, 0x82));
    assert!(contrast::luma(hover) > contrast::luma(rust), "hover lifts");
}

/// An accent-only file gets the whole family for free - and says so,
/// one note per state that moved. This is the ROADMAP's sentence
/// rendered: hover/active/selection generated from the accent, not
/// trusted from a hand.
#[test]
fn an_accent_only_file_derives_its_family_and_says_so() {
    let t = tf::parse_text(
        r##"{"format":"camcraft-theme/1","descriptor":{"name":"Rust"},"base":"dark",
            "colors":{"accent":"#C0504D"}}"##,
        "rust",
    )
    .expect("a red accent is legal - the derivation carries the family with it");
    assert_eq!(t.tokens.accent_hover, Color32::from_rgb(0xC5, 0x71, 0x82)); // lifts
    assert_eq!(t.tokens.selection_stroke, t.tokens.accent_hover, "the stroke rides the hover");
    assert_eq!(t.tokens.field_active, Color32::from_rgb(0xBC, 0x00, 0x00));
    assert_eq!(t.tokens.selection_bg, Color32::from_rgb(0xBD, 0x00, 0x00));
    assert_eq!(t.notes.len(), 4, "{:?}", t.notes); // hover, active, bg, stroke
    for kw in ["accent_hover", "field_active", "selection_bg", "selection_stroke"] {
        assert!(t.notes.iter().any(|n| n.contains(kw)), "{:?}", t.notes);
    }
    assert_eq!(contrast::readable(&t.tokens), None, "and the derived table still reads");
}

/// The gate's refusals, each named with its pair and its ratio: an
/// unreadable theme is refused, not rendered (Prime Directive, chrome
/// edition) - and never half-lands.
#[test]
fn unreadable_files_refuse_by_name() {
    for (key, value, want) in [
        ("text", "#404040", "text on ink"),
        ("text_dim", "#6E757E", "text_dim on ink"),
        ("selection_stroke", "#2E3136", "selection_stroke on ink_alt"),
    ] {
        let text = format!(
            r#"{{"format":"camcraft-theme/1","descriptor":{{"name":"B"}},"base":"dark","colors":{{"{key}":"{value}"}}}}"#
        );
        let err = tf::parse_text(&text, "gate").expect_err(want);
        assert!(err.contains(want), "{err}");
    }
}

#[test]
fn the_cache_holds_one_parse_per_unchanged_file() {
    let dir = tmp_dir("cache");
    let path = write(
        &dir,
        "one.json",
        r#"{"format":"camcraft-theme/1","descriptor":{"name":"One"},"base":"dark"}"#,
    );
    let mut cache = tf::Cache::new();
    let first = cache.get(&path);
    let again = cache.get(&path);
    assert!(Arc::ptr_eq(&first, &again), "an unchanged file must not re-parse");
    let gone = dir.join("vanished.json");
    let miss = cache.get(&gone);
    assert!(miss.is_err(), "a vanished file refuses by name");
    let _ = std::fs::remove_dir_all(&dir);
}
