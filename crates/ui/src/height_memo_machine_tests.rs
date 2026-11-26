//! Gate map: covers ui::height_memo.
//! The document machine changes drilling advice for setups that inherit it.
use camcraft_app::commands::{CreateSetup, LoadMachine, SetStock};
use camcraft_app::drilling::CycleChoice;
use camcraft_app::machine::{Kind, Machine};
use camcraft_app::operation::{Height, OpParams, SrcRef};
use camcraft_app::schema::Location;
use camcraft_app::stock::StockShape;

use crate::state::State;

fn machine(state: &mut State, kind: Kind) {
    let name = match kind {
        Kind::Mill => "mill",
        Kind::Lathe => "lathe",
        _ => unreachable!("this pin covers mill and lathe"),
    };
    let definition = Machine {
        kind: Some(name.into()),
        ..Default::default()
    };
    let load = LoadMachine::new(&state.world, format!("{name}.mch"), Some(definition))
        .expect("document machine");
    state.history.execute(&state.world, load);
}

fn inherited_setup() -> State {
    let mut state = State::new();
    state.history.execute(
        &state.world,
        CreateSetup::new(&state.world, Some("Setup1"), None).expect("setup"),
    );
    state.history.execute(
        &state.world,
        SetStock::new(
            &state.world,
            StockShape::Box {
                dx: 40.0,
                dy: 40.0,
                dz: 10.0,
            },
            Location::default(),
        )
        .expect("stock"),
    );
    assert!(
        camcraft_app::setup::def_of(&state.world, "Setup1")
            .unwrap()
            .machine
            .is_none()
    );
    state
}

fn advice(state: &State) -> Option<String> {
    super::clearance_advice(
        &state.world,
        Some("Setup1"),
        &SrcRef::Stock,
        &OpParams::Drill {
            clearance_z: Height::fixed(2.0),
            retract_z: None,
            top_z: Height::fixed(0.0),
            bottom_z: None,
            break_through: None,
            break_through_from: camcraft_app::drilling::BreakThroughFrom::Tip,
            cycle: CycleChoice::Drill,
            peck: 2.0,
            dwell: 0.0,
            feed: 200.0,
            plunge_feed: 100.0,
            holes: Vec::new(),
            cross: Vec::new(),
            picks: Default::default(),
        },
        None,
    )
}

fn assert_advice(state: &State, kind: Kind) {
    let result = advice(state);
    assert_eq!(
        result.is_some(),
        kind == Kind::Mill,
        "drilling advice follows {kind:?}"
    );
    if let Some(note) = &result {
        assert!(note.contains("is inside the stock"), "{note}");
    }
    let builds = super::advice_builds();
    for _ in 0..3 {
        assert_eq!(advice(state), result);
        assert_eq!(
            super::advice_builds(),
            builds,
            "unchanged machine reuses advice"
        );
    }
}

fn machine_transitions(first: Kind, second: Kind) {
    let mut state = inherited_setup();
    machine(&mut state, first);
    assert_advice(&state, first);
    let builds = super::advice_builds();
    machine(&mut state, second);
    assert_advice(&state, second);
    assert_eq!(
        super::advice_builds(),
        builds + 1,
        "machine kind invalidates advice"
    );
    machine(&mut state, first);
    assert_advice(&state, first);
    assert_eq!(
        super::advice_builds(),
        builds + 2,
        "returning machine kind invalidates advice"
    );
}

#[test]
fn drill_advice_without_document_machine_uses_mill_default() {
    let state = inherited_setup();
    assert_advice(&state, Kind::Mill);
}

#[test]
fn drill_advice_follows_document_machine_from_mill_to_lathe_and_back() {
    machine_transitions(Kind::Mill, Kind::Lathe);
}

#[test]
fn drill_advice_follows_document_machine_from_lathe_to_mill_and_back() {
    machine_transitions(Kind::Lathe, Kind::Mill);
}
