//! #258 in the surface: the Post page's Program number and Program
//! comment fields queue `SetProgramNumber` / `SetProgramComment` on
//! Enter (never a world poke), the drain lands each as one undo unit,
//! Ctrl+Z / Ctrl+Y walk it, and a refused value changes nothing and
//! says why.
//! Provenance: our own page law; no vendor material consulted.

use camcraft_app::posting;
use camcraft_app::schema::ProgramInfo;
use egui::accesskit::Role;
use egui::{Event, Key, Modifiers, Vec2};
use egui_kittest::kittest::Queryable;
use egui_kittest::Harness;

use crate::state::{CommandKind, State};

fn run(state: &mut State, k: CommandKind) {
    state.pending.push_back(k);
    state.drain_actions();
}

fn frame(ui: &mut egui::Ui, st: &mut State) {
    super::panes::shortcuts(ui, st);
    st.drain_actions();
    super::program_info::program_rows(ui, st);
}

fn key(h: &mut Harness<'static, State>, k: Key, modifiers: Modifiers) {
    h.input_mut().events.push(Event::ModifiersChanged(modifiers));
    h.input_mut().events.push(Event::Key { key: k, physical_key: None, pressed: true, repeat: false, modifiers });
    h.run_steps(2);
}

/// Type into the `nth` text field and press Enter.
fn enter(h: &mut Harness<'static, State>, nth: usize, text: &str) {
    h.query_all_by(|n| n.role() == Role::TextInput).nth(nth).expect("the field").click();
    h.run();
    h.query_all_by(|n| n.role() == Role::TextInput).nth(nth).expect("focused").type_text(text);
    h.run();
    key(h, Key::Enter, Modifiers::NONE);
}

#[test]
fn the_fields_set_the_program_through_the_command_and_undo() {
    let mut h = Harness::builder().with_size(Vec2::new(500.0, 200.0)).build_ui_state(frame, State::new());
    h.run();
    assert!(h.state().pending.is_empty(), "drawing the rows queues nothing");
    assert_eq!(h.query_all_by(|n| n.role() == Role::TextInput).count(), 2, "number and comment");
    enter(&mut h, 0, "1234");
    assert_eq!(posting::program_info(&h.state().world).number, Some(1234), "{}", h.state().status);
    enter(&mut h, 1, "Bracket A");
    let both = ProgramInfo { number: Some(1234), comment: Some("Bracket A".into()) };
    assert_eq!(posting::program_info(&h.state().world), both, "{}", h.state().status);
    assert!(
        h.query_all_by(|n| n.role() == Role::TextInput).any(|n| n.value().as_deref() == Some("1234")),
        "the unfocused field shows the document's number"
    );
    key(&mut h, Key::Z, Modifiers::COMMAND);
    assert_eq!(posting::program_info(&h.state().world).comment, None, "undo clears the comment");
    assert_eq!(posting::program_info(&h.state().world).number, Some(1234), "and keeps the number");
    key(&mut h, Key::Y, Modifiers::COMMAND);
    assert_eq!(posting::program_info(&h.state().world), both, "redo reapplies it");
}

#[test]
fn a_refused_value_changes_nothing_and_says_why() {
    let mut state = State::new();
    run(&mut state, CommandKind::SetProgramNumber("12a".into()));
    assert!(state.status.contains("program number not set") && state.status.contains("not a whole number"), "{}", state.status);
    run(&mut state, CommandKind::SetProgramNumber("10000".into()));
    assert!(state.status.contains("outside 1 to 9999"), "{}", state.status);
    run(&mut state, CommandKind::SetProgramComment("A (B)".into()));
    assert!(state.status.contains("program comment not set"), "{}", state.status);
    assert_eq!(posting::program_info(&state.world), ProgramInfo::default());
    assert!(!state.history.can_undo(), "a refusal pushes no undo unit");
}
