//! #867: one shared widget for every error, refusal and diagnostic text
//! the UI shows, so the user can select it, right-click "Copy message",
//! and - for a long or multi-line message - press a small "Copy" button,
//! then paste it into an issue report. Clipboard copy is ephemeral UI
//! (no document state changes), so it needs no command.
//! Provenance: our own UI law (AGENTS.md: refusals are visible); egui's
//! public `Label::selectable` and `Context::copy_text` APIs.

use egui::{Color32, Label, Response, RichText, Ui};

/// A message longer than this (or with a line break) gets the visible
/// "Copy" button; shorter ones still copy through the context menu.
const COPY_BUTTON_MIN_CHARS: usize = 80;

/// The label of the visible copy control (the structural tests find it).
pub(crate) const COPY_LABEL: &str = "Copy";
/// The context-menu entry, the same wording the status strip uses.
pub(crate) const COPY_MENU_LABEL: &str = "Copy message";

/// Whether `msg` earns the visible Copy button.
pub(crate) fn wants_button(msg: &str) -> bool {
    msg.contains('\n') || msg.chars().count() > COPY_BUTTON_MIN_CHARS
}

/// Show `msg` in `ink` as selectable, wrapped text with copy affordances.
/// Returns the text's response.
pub(crate) fn error_text(ui: &mut Ui, ink: Color32, msg: &str) -> Response {
    error_rich(ui, RichText::new(msg).color(ink), msg, false)
}

/// As [`error_text`] for pre-styled text (e.g. the underlined refusal
/// link); `sense_click` keeps the label clickable for such a link.
pub(crate) fn error_rich(ui: &mut Ui, text: RichText, plain: &str, sense_click: bool) -> Response {
    let mut label = Label::new(text).wrap().selectable(true);
    if sense_click {
        label = label.sense(egui::Sense::click());
    }
    let resp = ui.add(label);
    resp.context_menu(|ui| {
        if ui.button(COPY_MENU_LABEL).clicked() {
            ui.ctx().copy_text(plain.to_owned());
            ui.close();
        }
    });
    if wants_button(plain) && ui.small_button(COPY_LABEL).clicked() {
        ui.ctx().copy_text(plain.to_owned());
    }
    resp
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui_kittest::Harness;
    use egui_kittest::kittest::Queryable;

    fn frame(msg: &'static str) -> Harness<'static> {
        let mut h = Harness::new_ui(move |ui| {
            error_text(ui, Color32::RED, msg);
        });
        h.run();
        h
    }

    #[test]
    fn long_or_multiline_error_gets_a_copy_button() {
        let h = frame("post refused: move 3 gouges\nmove 9 collides with the vise");
        assert!(h.query_by_label(COPY_LABEL).is_some(), "multi-line error shows Copy");
        let long = "x".repeat(COPY_BUTTON_MIN_CHARS + 1);
        assert!(wants_button(&long));
    }

    #[test]
    fn short_single_line_error_has_no_button_but_is_text() {
        let h = frame("tool not found");
        assert!(h.query_by_label(COPY_LABEL).is_none());
        assert!(h.query_by_label("tool not found").is_some(), "the message is a labelled node");
    }

    #[test]
    fn right_click_offers_copy_message() {
        let mut h = frame("tool not found");
        h.get_by_label("tool not found").click_secondary();
        h.run();
        assert!(h.query_by_label(COPY_MENU_LABEL).is_some(), "context menu carries Copy message");
    }
}
