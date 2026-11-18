//! The key set of `assets/themes/theme.json.example` equals the key set of
//! the theme loader, and the file loads as a legal theme (#2025).
//! Provenance: our own tests of our own format.
//!
//! Gate map: covers ui::egui_app::theme_files.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use camcraft_ui::egui_app::theme::Tokens;
use camcraft_ui::egui_app::theme_files as tf;
use serde_json::Value;

fn example_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/themes/theme.json.example")
}

fn keys(root: &Value, field: &str) -> BTreeSet<String> {
    root.get(field)
        .and_then(Value::as_object)
        .map(|m| m.keys().cloned().collect())
        .unwrap_or_default()
}

fn names(list: &[&str]) -> BTreeSet<String> {
    list.iter().map(|s| (*s).to_string()).collect()
}

fn example_root() -> Value {
    let text = std::fs::read_to_string(example_path()).expect("read the example file");
    serde_json::from_str(&text).expect("the example is JSON")
}

/// The loader's vocabulary (`COLOR_NAMES`, `GEOM_NAMES`) is the code's key
/// set; the example must hold exactly it, no more and no less.
#[test]
fn example_has_exactly_the_loader_keys() {
    let root = example_root();
    let (c, g) = (keys(&root, "colors"), keys(&root, "geometry"));
    let (want_c, want_g) = (names(tf::COLOR_NAMES), names(tf::GEOM_NAMES));
    let none: Vec<&String> = Vec::new();
    assert_eq!(c.difference(&want_c).collect::<Vec<_>>(), none, "colour keys the loader does not know");
    assert_eq!(want_c.difference(&c).collect::<Vec<_>>(), none, "colour settings missing from the example");
    assert_eq!(g.difference(&want_g).collect::<Vec<_>>(), none, "geometry keys the loader does not know");
    assert_eq!(want_g.difference(&g).collect::<Vec<_>>(), none, "geometry settings missing from the example");
}

/// The example loads with no notes, and every value is the dark default.
#[test]
fn example_loads_and_holds_the_dark_defaults() {
    let theme = tf::load(&example_path()).unwrap_or_else(|e| panic!("example must load: {e}"));
    assert!(theme.notes.is_empty(), "notes: {:?}", theme.notes);
    assert_eq!(theme.tokens, Tokens::DARK);
}

/// The theme walk lists `.json` files only, so the example is not a theme.
#[test]
fn example_is_not_listed_as_a_theme() {
    let dir = example_path().parent().map(Path::to_path_buf).expect("dir");
    let (paths, _) = tf::discover(&dir);
    assert!(!paths.is_empty());
    assert!(paths.iter().all(|p| p.extension().is_some_and(|x| x == "json")));
}
