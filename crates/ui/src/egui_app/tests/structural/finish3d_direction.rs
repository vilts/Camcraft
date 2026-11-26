//! #2377: the live Parallel angle field and its Apply/undo command.
//! Provenance: our own operation page and the public AccessKit API.

use super::*;
use camcraft_app::finishing3d::FinishPattern;
use camcraft_app::operation::OpType;
use egui::accesskit::Role;

const ANGLE: &str = "Direction angle";

fn direction(params: &OpParams) -> f64 {
    let OpParams::Finish3D {
        direction_angle, ..
    } = params
    else {
        panic!("expected 3D Finish parameters");
    };
    *direction_angle
}

fn live(h: &Harness<'static, ShellUi>) -> f64 {
    let st = &h.state().state;
    let entity = model::find_by_name(&st.world, "3D Finish1").unwrap();
    direction(&model::operation_of(&st.world, &entity).unwrap().params)
}

#[test]
fn parallel_direction_control_applies_and_undoes() {
    let _g = snapshot_guard();
    let mut h = shell_harness();
    rack(&mut h.state_mut().state, &["BN-0600"]);
    crate::actions::add_operation(&mut h.state_mut().state, OpType::Finish3D.ordinal());
    h.run_steps(2);
    h.get_by_label("Passes").click();
    h.run_steps(2);

    let control = h.get_by_role_and_label(Role::SpinButton, ANGLE);
    assert!(!control.accesskit_node().is_disabled());
    assert_eq!(live(&h), 0.0);
    control.scroll_to_me();
    h.run();
    h.get_by_role_and_label(Role::SpinButton, ANGLE).click();
    h.run();
    h.input_mut().events.push(Event::Text("45".to_owned()));
    h.run_steps(2);
    assert_eq!(direction(&h.state().state.op_ui.params), 45.0);
    assert_eq!(live(&h), 0.0, "typing changes the draft only");

    h.get_by_label("Apply").scroll_to_me();
    h.run();
    h.get_by_label("Apply").click();
    h.run_steps(2);
    assert_eq!(live(&h), 45.0);
    {
        let st = &mut h.state_mut().state;
        assert!(st.history.undo(&st.world));
    }
    assert_eq!(live(&h), 0.0, "one undo restores the direction");

    let OpParams::Finish3D { pattern, .. } = &mut h.state_mut().state.op_ui.params else {
        panic!("expected the 3D Finish draft");
    };
    *pattern = FinishPattern::Scallop;
    h.run_steps(2);
    assert!(
        h.query_by_label(ANGLE).is_none(),
        "Scallop has no direction field"
    );
}
