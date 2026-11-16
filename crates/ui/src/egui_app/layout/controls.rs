//! Basic form controls routed through the layout seam (#1122): buttons,
//! read-only list rows and radio choices, so pages do not call raw
//! `ui.button` / `ui.small_button` / `ui.label` / `ui.radio_value`
//! directly and a future look or kit can restyle them in one place.
//!
//! Provenance: our own thin wrappers over egui's `Ui` controls (vendored
//! egui 0.36 docs); no vendor material involved.

use egui::{Response, Ui, WidgetText};

/// A form action button (e.g. "Detect holes").
pub fn button(ui: &mut Ui, label: impl Into<WidgetText>) -> Response {
    ui.button(label)
}

/// A compact inline button inside a list or row (e.g. "Remove 1").
pub fn small_button(ui: &mut Ui, label: impl Into<WidgetText>) -> Response {
    ui.small_button(label)
}

/// One read-only text row in a form list (e.g. a detected hole entry).
pub fn list_row(ui: &mut Ui, text: impl Into<WidgetText>) -> Response {
    ui.label(text)
}

/// One radio choice inside a `prop_row` or stacked group.
pub fn radio_value<V: PartialEq>(
    ui: &mut Ui,
    current: &mut V,
    value: V,
    label: impl Into<WidgetText>,
) -> Response {
    ui.radio_value(current, value, label)
}

/// Two mutually exclusive radio choices laid out in order (inside a
/// right-to-left `prop_row`, `first` sits to the right of `second`).
pub fn radio_pair<V: PartialEq>(ui: &mut Ui, current: &mut V, first: (V, &str), second: (V, &str)) {
    radio_value(ui, current, first.0, first.1);
    radio_value(ui, current, second.0, second.1);
}
