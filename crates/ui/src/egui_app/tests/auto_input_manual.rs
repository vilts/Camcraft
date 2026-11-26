//! #2178: an `Auto` input stays manual while the user types a value whose
//! first keys read as 0 ("0", "0.", "0.5"). The model's 0 is the Auto
//! sentinel, so the toggle must not read it back from the value mid-edit.
//! Provenance: our own page law; asserted structurally (no pixels).

use super::super::dialog_tests::{opened, page};
use super::*;
use camcraft_app::machining2d::PocketStrategy;
use camcraft_app::machining3d::Entry3D;
use camcraft_app::operation::Height;
use egui::accesskit::Role;

fn helix_pocket() -> OpParams {
    OpParams::Pocket { pass_start: Default::default(),
        rest: None,
        blend_radius: None,
        links: camcraft_app::links::Links::PLANAR,
        tilt: None,
        stepover: 1.0,
        stock_to_leave: 0.0,
        clearance_z: Height::fixed(25.0),
        retract_z: None,
        top_z: Height::fixed(12.0),
        bottom_z: Height::fixed(9.0),
        stepdown: 1.0,
        feed: 800.0,
        plunge_feed: 200.0,
        climb: true,
        strategy: PocketStrategy::Zigzag,
        entry: Entry3D::Helix,
        helix_radius: 1.0,
        optimal_load: camcraft_app::load::DEFAULT_OPTIMAL_LOAD,
        blend_turns: false,
        corner_relief: Default::default(),
        picks: Default::default(),
    }
}

fn pitch(h: &Harness<'static, State>) -> f64 {
    match &h.state().op_ui.params {
        OpParams::Pocket { links, .. } => links.approach.helix_pitch,
        _ => f64::NAN,
    }
}

fn row_node<'a>(h: &'a Harness<'static, State>, label: &str, role: Role) -> egui_kittest::Node<'a> {
    let lab = h.get_by_label(label).rect();
    h.query_all_by(move |n| n.role() == role)
        .find(|n| {
            let c = n.rect().center().y;
            c >= lab.top() - 2.0 && c <= lab.bottom() + 2.0
        })
        .expect("a control on the label's row")
}

#[test]
fn typing_over_an_auto_off_input_keeps_it_manual() {
    let mut state = opened(helix_pocket());
    state.op_tab = super::super::op_tabs::SEC_LEADS;
    let mut h = page(state);
    h.run();
    // Auto is on (0). Turn it off: the seed 1.0 appears as a number field.
    row_node(&h, "Helix pitch", Role::CheckBox).click();
    h.run();
    assert_eq!(pitch(&h), 1.0, "Auto off seeds a value");
    row_node(&h, "Helix pitch", Role::SpinButton).click();
    h.run();
    // Replace the text with "0.5" one key at a time, a frame each.
    for ch in ["0", ".", "5"] {
        row_node(&h, "Helix pitch", Role::SpinButton).type_text(ch);
        h.run();
        h.run();
        assert!(
            h.query_by_label("1 turn / helix Ø").is_none(),
            "Auto must stay off after typing {ch:?}"
        );
        assert!(
            h.query_all_by(|n| n.role() == Role::SpinButton).count() > 0,
            "the number field stays after typing {ch:?}"
        );
    }
    assert!((pitch(&h) - 0.5).abs() < 1e-9, "the draft holds the typed 0.5 ({})", pitch(&h));
    // Apply commits it through the command history; one undo restores Auto (0).
    h.get_by_label("Apply").click();
    h.run();
    h.state_mut().drain_actions();
    assert!((live_pitch(h.state()) - 0.5).abs() < 1e-9, "Apply wrote the manual value");
    let st = h.state_mut();
    st.history.undo(&st.world);
    assert_eq!(live_pitch(h.state()), 0.0, "one undo step restores Auto");
}

fn live_pitch(state: &State) -> f64 {
    model::find_by_name(&state.world, "Op")
        .and_then(|e| model::operation_of(&state.world, &e))
        .map(|o| match o.params {
            OpParams::Pocket { links, .. } => links.approach.helix_pitch,
            _ => f64::NAN,
        })
        .unwrap_or(f64::NAN)
}
