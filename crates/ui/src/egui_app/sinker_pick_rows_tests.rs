//! #3333: the Start point / Exit point groups of the Sinker Burn and the
//! Sinker Side Burn on the Geometry tab. Labels, roles and the draft
//! only (no position, no PNG).
//! Provenance: our own page law; no vendor material consulted.

use egui::accesskit::Role;
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;

use camcraft_app::geo2d::Point2;
use camcraft_app::operation::{KindInfo, OpParams, SinkerOrbit, SinkerOrbitKind};
use camcraft_app::path_picks::PickRole;

use super::super::dialog_tests::{face, opened, page};
use super::super::op_tabs::SEC_GEO;
use super::super::start_exit_rows::{EXIT_GROUP, NOT_TAKEN, START_GROUP};
use super::NO_ORBIT;
use crate::state::State;

/// The Geometry tab of a draft of `label`, after `edit` changed the draft.
fn geometry(label: &str, edit: impl FnOnce(&mut OpParams)) -> Harness<'static, State> {
    let k = KindInfo::ALL.iter().position(|k| k.label == label).expect("the kind");
    let mut state = opened(face());
    state.op_ui.params = crate::operations::retype(&face(), k);
    edit(&mut state.op_ui.params);
    state.op_tab = SEC_GEO;
    let mut harness = page(state);
    harness.run();
    harness
}

fn draft(h: &Harness<'static, State>) -> (Option<Point2>, Option<Point2>) {
    let picks = h.state().op_ui.params.path_picks();
    (picks.first(PickRole::Start).map(|k| k.xy), picks.first(PickRole::Exit).map(|k| k.xy))
}

/// Click the checkbox on the row of `label`.
fn switch(h: &mut Harness<'static, State>, label: &str) {
    let lab = h.get_by_label(label).rect();
    h.query_all_by(|n| n.role() == Role::CheckBox)
        .find(|n| {
            let c = n.rect().center().y;
            c >= lab.top() - 2.0 && c <= lab.bottom() + 2.0
        })
        .expect("a checkbox on the label's row")
        .click();
    h.run();
}

fn same(a: Option<Point2>, x: f64, y: f64) -> bool {
    a.is_some_and(|q| (q.x - x).abs() < 1e-9 && (q.y - y).abs() < 1e-9)
}

fn burn_at(p: &mut OpParams, orbit: Option<SinkerOrbit>) {
    if let OpParams::SinkerBurn(b) = p {
        (b.x, b.y, b.orbit) = (12.0, -4.0, orbit);
    }
}

#[test]
fn a_sinker_burn_without_an_orbit_says_by_name_that_no_point_is_taken() {
    let harness = geometry("Sinker Burn", |p| burn_at(p, None));
    for g in [START_GROUP, EXIT_GROUP] {
        assert!(harness.query_by_label(g).is_some(), "the {g} group is drawn");
    }
    assert_eq!(harness.query_all_by_label_contains(NOT_TAKEN).count(), 2, "the start and the exit say they are not taken");
    assert_eq!(harness.query_all_by_label_contains(NO_ORBIT).count(), 2, "each says why: no orbit");
    for r in ["Orbit start stated", "Orbit exit stated", "Orbit start X", "Orbit exit X"] {
        assert!(harness.query_by_label(r).is_none(), "no {r} row without an orbit");
    }
    assert_eq!(draft(&harness), (None, None), "drawing the tab writes no point");
}

#[test]
fn a_sinker_burn_with_an_orbit_types_its_orbit_start_and_exit() {
    for (kind, exit) in [(SinkerOrbitKind::Circular, (12.25, -4.0)), (SinkerOrbitKind::Vector, (12.0, -4.25))] {
        let orbit = SinkerOrbit { kind, radius: 0.25, vectors: 4, turns: 1 };
        let mut harness = geometry("Sinker Burn", |p| burn_at(p, Some(orbit)));
        assert_eq!(harness.query_all_by_label_contains(NOT_TAKEN).count(), 0, "{kind:?}: an orbit takes both points");
        for r in ["Orbit start X", "Orbit start Y", "Orbit exit X", "Orbit exit Y"] {
            assert!(harness.query_by_label(r).is_none(), "{kind:?}: {r} hides while its group is off");
        }
        assert_eq!(draft(&harness), (None, None), "{kind:?}: drawing the tab writes no point");
        switch(&mut harness, "Orbit start stated");
        switch(&mut harness, "Orbit exit stated");
        let (start, end) = draft(&harness);
        assert!(same(start, 12.25, -4.0), "{kind:?}: the start seeds the automatic start at +X: {start:?}");
        assert!(same(end, exit.0, exit.1), "{kind:?}: the exit seeds the automatic end: {end:?}");
        assert!(harness.state().op_ui.params.named_refusal().is_none(), "{kind:?}: the seeds are points that the generator takes");
        for r in ["Orbit start X", "Orbit start Y", "Orbit exit X", "Orbit exit Y"] {
            assert!(harness.query_by_label(r).is_some(), "{kind:?}: {r} shows while its group is on");
        }
        assert!(harness.query_all_by_label_contains("is refused by name").count() >= 2, "{kind:?}: each group says the law");
        switch(&mut harness, "Orbit start stated");
        assert!(draft(&harness).0.is_none(), "{kind:?}: off clears the start");
    }
}

#[test]
fn a_sinker_side_burn_types_the_ends_of_its_side_move() {
    let mut harness = geometry("Sinker Side Burn", |p| {
        if let OpParams::SinkerSideBurn(s) = p {
            (s.burn.x, s.burn.y) = (3.0, 7.0);
        }
    });
    for g in [START_GROUP, EXIT_GROUP] {
        assert!(harness.query_by_label(g).is_some(), "the {g} group is drawn");
    }
    assert_eq!(harness.query_all_by_label_contains(NOT_TAKEN).count(), 0, "the side burn takes both points");
    assert_eq!(draft(&harness), (None, None), "drawing the tab writes no point");
    switch(&mut harness, "Side move start stated");
    switch(&mut harness, "Side move exit stated");
    let (start, exit) = draft(&harness);
    assert!(same(start, 3.0, 7.0) && same(exit, 3.0, 7.0), "both seed the burn position, the automatic ends: {start:?} {exit:?}");
    assert!(harness.state().op_ui.params.named_refusal().is_none(), "the seeds are points that the generator takes");
    for r in ["Side move start X", "Side move start Y", "Side move exit X", "Side move exit Y"] {
        assert!(harness.query_by_label(r).is_some(), "{r} shows while its group is on");
    }
    assert!(harness.query_all_by_label_contains("is refused by name").count() >= 2, "each group says the law");
}
