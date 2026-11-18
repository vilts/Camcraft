//! M7.2 B3b + C: the traced stock's field follows the cells that ask
//! for it - nothing walked while neither Deviate nor the transport is
//! up, a refusal kept and said once (never retried every frame), and
//! a mode change retiring whatever was walked. Provenance: our own
//! tests; nothing from vendor binaries.
//!
//! Gate map (M4.4): covers ui::actions::ensure_field_view, ui::state (ExportStock*).

use crate::actions::ensure_field_view;
use crate::state::{Sim, State};

#[test]
fn nothing_is_walked_while_no_cell_asks() {
    let mut state = State::new();
    ensure_field_view(&mut state);
    assert!(state.field_view.is_none() && state.field_play.is_none() && state.field_mode.is_none());
}

#[test]
fn a_refused_playback_is_said_once_and_retired_with_the_transport() {
    let mut state = State::new();
    state.sim = Some(Sim::new()); // an empty document: no stock to cut
    ensure_field_view(&mut state);
    assert!(matches!(state.field_view, Some(Err(_))), "no stock is a named refusal");
    assert!(state.status.contains("simulation shows the uncut stock"), "{}", state.status);
    let said = state.status.clone();
    ensure_field_view(&mut state);
    assert_eq!(state.status, said, "the same state is not re-walked or re-said");
    state.sim = None;
    ensure_field_view(&mut state);
    assert!(state.field_view.is_none() && state.field_mode.is_none(), "the transport down retires it");
}

#[test]
fn the_stock_export_refuses_by_name_and_writes_nothing_without_stock() {
    let mut state = State::new();
    let path = std::env::temp_dir().join(format!("camcraft-no-stock-{}.stl", std::process::id()));
    state.pending.push_back(crate::state::CommandKind::ExportStockFile(path.clone()));
    state.drain_actions();
    assert!(state.status.starts_with("stock export refused:"), "{}", state.status);
    assert!(!path.exists(), "a refusal writes no file");
}

/// #2177: a refusal that each document edit asks again is said once,
/// and the uncut stock still stands in the scene while it is refused.
#[test]
fn a_refusal_asked_again_is_said_once_and_the_stock_stays_drawn() {
    use camcraft_app::commands::SetStock;
    use camcraft_app::geometry::{quat_from_euler, Point3};
    use camcraft_app::schema::Location;
    use camcraft_app::stock::StockShape;
    let mut state = State::new();
    let turned = Location { pos: Point3::ZERO, rot: quat_from_euler(0.0, 0.0, 0.7) };
    let stock = SetStock::new(&state.world, StockShape::Box { dx: 20.0, dy: 10.0, dz: 8.0 }, turned).expect("box stock");
    state.history.execute(&state.world, stock);
    state.sim = Some(Sim::new());
    ensure_field_view(&mut state);
    assert!(matches!(state.field_view, Some(Err(_))), "nothing to cut is a named refusal");
    state.doc_version += 1; // an edit: the playback is asked again
    ensure_field_view(&mut state);
    assert_eq!(state.status.matches("simulation shows the uncut stock").count(), 1, "{}", state.status);
    assert!(!state.status.contains("sheet stands") && !state.status.contains("field view"), "{}", state.status);
    let scene = crate::egui_app::world::collect(&state);
    let drawn = scene.parts.iter().find(|p| p.stock).expect("the refused stock is still drawn");
    assert_eq!(drawn.verts.len(), 36, "the uncut box's twelve facets");
}
