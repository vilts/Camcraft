//! Issue #191: a dark Create cell that is clicked anyway says why AND
//! takes the user to the fix - the tool library when no tool fits, the
//! machine door's workspace list when the machine cannot run the kind.
//! Provenance: our own tests over our own ribbon gate.

use super::panes::{Cmd, GateRoute, follow_gate, gate_route};
use crate::state::{CommandKind, State, UiTarget};

fn kind(ty: camcraft_app::operation::OpType) -> usize {
    camcraft_app::operation::KindInfo::ALL
        .iter()
        .position(|k| k.ty == ty)
        .unwrap_or_else(|| panic!("{ty:?} is in the kind table"))
}

#[test]
fn no_tool_routes_to_the_tool_library() {
    let mut state = State::new();
    assert!(state.tool_names().is_empty(), "a fresh document racks no tool");
    let face = Cmd::Create(kind(camcraft_app::operation::OpType::Face));
    assert_eq!(gate_route(face, &state), Some(GateRoute::Tools));
    follow_gate(face, &mut state);
    assert_eq!(state.ui_target, UiTarget::Tool(String::new()));
    assert!(state.status.contains("no tool in the document") && state.status.ends_with("opened the tool library"), "{}", state.status);
}

#[test]
fn a_kind_the_machine_cannot_run_opens_the_machine_page() {
    let mut state = State::new();
    state.pending.push_back(CommandKind::LoadBuiltinMachine("Lathe".into()));
    state.drain_actions();
    let catalog = camcraft_app::tools::catalog();
    let (name, tool) = catalog.into_iter().find(|(n, _)| *n == "EM-0400-2FL").expect("catalog end mill");
    state.pending.push_back(CommandKind::AddTool { name: name.to_owned(), tool });
    state.drain_actions();
    assert!(!state.tool_names().is_empty(), "a tool is racked");
    let pocket = Cmd::Create(kind(camcraft_app::operation::OpType::Pocket));
    assert_eq!(gate_route(pocket, &state), Some(GateRoute::Machine));
    follow_gate(pocket, &mut state);
    assert!(state.status.ends_with("opened the Machine page"), "{}", state.status);
}

#[test]
fn a_live_cell_has_no_route_and_changes_nothing() {
    let mut state = State::new();
    let before = state.status.clone();
    assert_eq!(gate_route(Cmd::Frame, &state), None);
    follow_gate(Cmd::Frame, &mut state);
    assert_eq!(state.status, before);
}

/// #216: with a tool but no stock every Create cell darkens - every
/// ribbon-made operation machines the setup's stock - its tip and its
/// click say "set up stock first", and the click opens the Stock page.
/// Setting stock lights the cell again.
#[test]
fn no_stock_darkens_the_cell_and_routes_to_the_stock_page() {
    use super::panes::{enabled, tip_of};
    let mut state = State::new();
    let catalog = camcraft_app::tools::catalog();
    let (name, tool) = catalog.into_iter().find(|(n, _)| *n == "EM-0400-2FL").expect("catalog end mill");
    state.pending.push_back(CommandKind::AddTool { name: name.to_owned(), tool });
    state.drain_actions();
    let face = Cmd::Create(kind(camcraft_app::operation::OpType::Face));
    assert!(!enabled(face, &state), "no stock: the cell is dark");
    assert!(tip_of(face, &state).contains("set up stock first"), "{}", tip_of(face, &state));
    assert_eq!(gate_route(face, &state), Some(GateRoute::Stock));
    follow_gate(face, &mut state);
    assert_eq!(state.ui_target, UiTarget::Stock);
    assert!(state.status.contains("set up stock first") && state.status.ends_with("opened the Stock page"), "{}", state.status);
    state.pending.push_back(CommandKind::SetStock {
        shape: camcraft_app::stock::StockShape::Box { dx: 60.0, dy: 60.0, dz: 20.0 },
        pos: [0.0, 0.0, 0.0],
        yaw_deg: 0.0,
    });
    state.drain_actions();
    assert!(enabled(face, &state), "stock set: the cell lights");
    assert_eq!(gate_route(face, &state), None);
}
