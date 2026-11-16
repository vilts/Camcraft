//! #263 in the surface: a ball nose's corner is its ball. The tool
//! editor shows half the diameter as text with no field to edit, Apply
//! saves that number, and the library summary reads it; a bull nose
//! keeps its editable corner.
//! Provenance: our own tests over our own tool editor page.

use egui::Vec2;
use egui::accesskit::Role;
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;

use camcraft_app::model;
use camcraft_app::tools::{Tool, catalog};

use crate::actions;
use crate::state::{CommandKind, State, UiTarget};

/// A 6 mm ball nose from the catalog, saved with a corner field of 0 -
/// what a document holds after a draft is retyped from an end mill.
const BALL: &str = "BN-0600";
/// A 10 mm bull nose with a 1 mm corner: the one kind that reads its field.
const BULL: &str = "BB-1000-R1";

fn catalog_tool(name: &str) -> Tool {
    catalog().into_iter().find(|(n, _)| *n == name).map(|(_, t)| t).expect("catalog tool")
}

/// A document holding `tool` as `name`, with nothing else in it.
fn with_tool(name: &str, tool: Tool) -> State {
    let mut state = State::new();
    state.pending.push_back(CommandKind::AddTool { name: name.to_owned(), tool });
    state.drain_actions();
    state
}

fn saved(state: &State, name: &str) -> Tool {
    model::find_by_name(&state.world, name).and_then(|e| model::tool_of(&state.world, &e)).expect("the tool")
}

/// The editor window open on `name`, drawn alone.
fn editor(mut state: State, name: &str) -> Harness<'static, State> {
    actions::open_tool_editor(&mut state, name);
    let mut h = Harness::builder()
        .with_size(Vec2::new(900.0, 1000.0))
        .build_ui_state(|ui, st: &mut State| super::pages::tool_editor_window(ui.ctx(), st), state);
    h.run();
    h
}

/// How many number fields sit between the "Corner radius" label and the
/// next field's label: the editor stacks each value under its label.
fn corner_fields(h: &Harness<'static, State>) -> usize {
    let top = h.get_by_label("Corner radius").rect().bottom();
    let bottom = h.get_by_label("Flute length").rect().top();
    h.query_all_by(|n| n.role() == Role::SpinButton)
        .filter(|n| (top..bottom).contains(&n.rect().center().y))
        .count()
}

#[test]
fn a_ball_noses_corner_is_shown_as_its_ball_with_no_field_to_edit() {
    let ball = Tool { corner_r: 0.0, ..catalog_tool(BALL) };
    let h = editor(with_tool(BALL, ball), BALL);
    assert_eq!(corner_fields(&h), 0, "a ball's corner is not an input");
    assert!(h.query_by_label("3.000 mm").is_some(), "the corner shows half the 6 mm diameter");
}

#[test]
fn applying_a_ball_nose_saves_its_corner_as_half_the_diameter() {
    let ball = Tool { corner_r: 0.0, ..catalog_tool(BALL) };
    let mut h = editor(with_tool(BALL, ball), BALL);
    h.get_by_label("Apply").click();
    h.run();
    h.state_mut().drain_actions();
    assert_eq!(saved(h.state(), BALL).corner_r, 3.0);
}

#[test]
fn properties_reads_a_ball_noses_corner_as_its_ball() {
    let mut state = with_tool(BALL, Tool { corner_r: 0.0, ..catalog_tool(BALL) });
    state.ui_target = UiTarget::Tool(BALL.to_owned());
    let mut h = Harness::builder()
        .with_size(Vec2::new(620.0, 900.0))
        .build_ui_state(|ui, st: &mut State| super::pages::properties(ui, st, &super::theme::current()), state);
    h.run();
    assert!(h.query_by_label("3.000 mm").is_some(), "Properties names the ball's corner");
    assert_eq!(corner_fields(&h), 0, "Properties keeps the ball's corner read-only");
}

#[test]
fn a_bull_nose_keeps_its_corner_field() {
    let h = editor(with_tool(BULL, catalog_tool(BULL)), BULL);
    assert_eq!(corner_fields(&h), 1, "the bull nose reads its corner, so it is edited");
}
