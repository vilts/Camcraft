//! #342 in the browser: a setup shows ONE work coordinate system - a
//! single row labelled plain "WCS" (the word its post writes beside it),
//! never G54..G59 slot rows. Clicking it targets the WCS page, which
//! edits that setup's one origin through the undoable `SetWcs`; the
//! Setups page's word only relabels it.
//! Provenance: our own tests over our own shell; no HSMWorks material.

use super::*;

fn wcs_rows(state: &State) -> Vec<(String, String)> {
    let (rows, leaves) = panes::browser_rows(state);
    leaves
        .iter()
        .enumerate()
        .filter(|(_, l)| matches!(l, panes::Leaf::Wcs(_)))
        .map(|(i, _)| (rows[i].label.clone(), rows[i].suffix.clone()))
        .collect()
}

fn run(state: &mut State, k: CommandKind) {
    state.pending.push_back(k);
    state.drain_actions();
}

#[test]
fn the_browser_shows_one_wcs_per_setup_and_no_slot_rows() {
    let mut state = State::new();
    assert_eq!(wcs_rows(&state), [("WCS".to_owned(), "G54".to_owned())], "a document without setups");
    run(&mut state, CommandKind::CreateSetup { name: None, datum_from: None });
    run(&mut state, CommandKind::CreateSetup { name: None, datum_from: None });
    run(&mut state, CommandKind::SetSetupWorkOffset { setup: "Setup2".into(), slot: Some(1) });
    // #343: every setup node shows its own one WCS.
    let both = [("WCS".to_owned(), "G54".to_owned()), ("WCS".to_owned(), "G55".to_owned())];
    assert_eq!(wcs_rows(&state), both, "Setup1's and Setup2's one WCS each");
    let (rows, _) = panes::browser_rows(&state);
    for slot in ["G54", "G55", "G56", "G57", "G58", "G59"] {
        assert!(!rows.iter().any(|r| r.label == slot), "no {slot} slot row");
    }
    assert!(!rows.iter().any(|r| r.suffix == "not placed"), "no unplaced slot rows");
    run(&mut state, CommandKind::ActivateSetup(Some("Setup1".into())));
    assert_eq!(wcs_rows(&state), both, "activation moves no row");
}

#[test]
fn the_wcs_row_edits_the_active_setups_origin_undoably() {
    let mut state = State::new();
    run(&mut state, CommandKind::CreateSetup { name: None, datum_from: None });
    run(&mut state, CommandKind::CreateSetup { name: None, datum_from: None });
    run(&mut state, CommandKind::SetWcs { pos: [7.0, 0.0, 0.0], roll_deg: 0.0, pitch_deg: 0.0, yaw_deg: 0.0 });
    assert_eq!(state.status, "WCS (G54) origin set");
    let x = |s: &State, setup: &str| {
        camcraft_app::setup::wcs_entity(&s.world, Some(setup))
            .and_then(|e| model::transform_of(&s.world, &e))
            .map(|l| l.pos.x)
    };
    assert_eq!(x(&state, "Setup2"), Some(7.0));
    assert_eq!(x(&state, "Setup1"), None, "the other setup's WCS is untouched");
    // Relabelling it moves nothing.
    run(&mut state, CommandKind::SetSetupWorkOffset { setup: "Setup2".into(), slot: Some(3) });
    assert_eq!(x(&state, "Setup2"), Some(7.0));
    assert!(UiTarget::Wcs.resolves_in(&state.world));
    run(&mut state, CommandKind::Undo);
    run(&mut state, CommandKind::Undo);
    assert_eq!(x(&state, "Setup2"), None, "undo removes the placement");
    run(&mut state, CommandKind::Redo);
    assert_eq!(x(&state, "Setup2"), Some(7.0), "redo puts it back");
}

/// #876: the WCS page shows its work-offset dropdown (drawn under the page
/// text, #343) and no label on that page points at "the Setups page" - the
/// stale hint that sent the user looking for a control that was right there.
#[test]
fn the_wcs_page_shows_the_work_offset_dropdown_and_no_setups_page_pointer() {
    let _g = snapshot_guard();
    let mut h = Harness::builder()
        .with_size(Vec2::new(1280.0, 720.0))
        .build_ui_state(frame, ShellUi::blank());
    {
        let st = &mut h.state_mut().state;
        st.pending.push_back(CommandKind::CreateSetup { name: None, datum_from: None });
        st.drain_actions();
        crate::actions::target(st, UiTarget::Wcs);
    }
    h.run_steps(3);
    for _ in 0..6 {
        if super::structural::painted(&h)
            .iter()
            .any(|(t, _)| t == "Work offset: the G54-G59 slot this setup posts. It does not move the origin.")
        {
            break;
        }
        wheel(&mut h, egui::pos2(1100.0, 500.0), -220.0);
    }
    let texts = super::structural::painted(&h);
    assert!(
        texts.iter().any(|(t, _)| t == "Work offset: the offset this setup posts, as its post names it. It does not move the origin."),
        "the WCS page names the work offset in plain words: {texts:?}"
    );
    assert!(
        !texts.iter().any(|(t, _)| t.contains("Setups page")),
        "no WCS-page label points at the Setups page: {texts:?}"
    );
    // The dropdown itself: an unset setup reads offset 1 in the combo,
    // named by its post (#931).
    h.get_by(|n| n.role() == egui::accesskit::Role::ComboBox && n.value().as_deref() == Some("Work offset 1 (G54)"));
}
