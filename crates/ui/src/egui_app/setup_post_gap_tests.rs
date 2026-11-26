//! #3390: the Setup -> Post page keeps a gap for the floating scroll bar, so a
//! click on a post option checkbox is not taken by the bar.
//!
//! Provenance: our own page law; the pattern of #3384 (Post dialog).

use camcraft_app::post::options::OptionValue;
use camcraft_app::setup_post_options;
use egui::accesskit::Role;
use egui::Vec2;
use egui_kittest::kittest::Queryable;
use egui_kittest::Harness;

use crate::state::{CommandKind, SetupPage, State, UiTarget};

fn frame(ui: &mut egui::Ui, st: &mut State) {
    st.drain_actions();
    super::pages::properties(ui, st, &super::theme::current());
}

/// A real click on "Write tool list" of the Setup -> Post page, in a window
/// that is too low for the page, toggles the option through the command.
#[test]
fn setup_post_page_option_checkbox_is_clickable_when_the_page_scrolls() {
    let mut state = State::new();
    state.pending.push_back(CommandKind::CreateSetup { name: None, datum_from: None });
    state.drain_actions();
    state.ui_target = UiTarget::SetupNode("Setup1".into(), SetupPage::Post);
    let mut h = Harness::builder().with_size(Vec2::new(500.0, 420.0)).build_ui_state(frame, state);
    h.run();
    h.get_by_label_contains("Post script options").scroll_to_me();
    h.run_steps(3);
    h.get_by_label_contains("Post script options").click();
    h.run_steps(3);

    let label = "Write tool list";
    let cb = h.get_by(|n| n.role() == Role::CheckBox && n.label().as_deref() == Some(label));
    cb.scroll_to_me();
    h.run_steps(3);
    let cb = h.get_by(|n| n.role() == Role::CheckBox && n.label().as_deref() == Some(label));
    let rect = cb.rect();
    cb.click();
    h.run_steps(3);

    let key = setup_post_options::post_key(&camcraft_app::posting::post_of(&h.state().world));
    let doc = setup_post_options::script_document(&h.state().world);
    assert_eq!(
        doc.for_post(&key).get("writeToolList"),
        Some(&OptionValue::Bool(true)),
        "a click on {label} at {rect:?} must toggle the option"
    );
}
