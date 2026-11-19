//! #870: the WCS page applies its typed origin from a fixed footer -
//! Set origin beside Close, apart from the per-field controls - flags
//! edits that are not applied yet, and says where the origin is in
//! words under its heading. Structural: painted text, AccessKit labels
//! and coarse placement; no pixels.
//! Provenance: our own page law, specced from the Makera Z1 user's
//! first-cut report on #870; no vendor material consulted.

use super::*;
use camcraft_app::stock::StockShape;

fn wcs_page_on_a_block() -> Harness<'static, ShellUi> {
    let mut h = Harness::builder()
        .with_size(Vec2::new(1280.0, 900.0))
        .build_ui_state(frame, ShellUi::blank());
    {
        let st = &mut h.state_mut().state;
        let shape = StockShape::Box { dx: 100.0, dy: 50.0, dz: 20.0 };
        st.pending.push_back(CommandKind::SetStock { shape, pos: [0.0, 0.0, 0.0], yaw_deg: 0.0 });
        st.pending.push_back(CommandKind::CreateSetup { name: None, datum_from: None });
        st.drain_actions();
        let b = model::stock_world_aabb_for(&st.world, camcraft_app::setup::active(&st.world).as_deref())
            .expect("stock box");
        st.pending.push_back(CommandKind::SetWcs {
            pos: [b.min.x, b.min.y, b.max.z],
            roll_deg: 0.0,
            pitch_deg: 0.0,
            yaw_deg: 0.0,
        });
        st.drain_actions();
        crate::actions::target(st, UiTarget::Wcs);
    }
    h.run_steps(3);
    h
}

fn has_text(h: &Harness<'static, ShellUi>, want: &str) -> bool {
    super::structural::painted(h).iter().any(|(t, _)| t == want)
}

#[test]
fn the_page_says_where_the_origin_is_in_words() {
    let _g = snapshot_guard();
    let h = wcs_page_on_a_block();
    let texts = super::structural::painted(&h);
    assert!(
        texts.iter().any(|(t, _)| t.ends_with(" origin: top of stock, front-left corner (X0 Y0 Z0)")),
        "the WCS page states where its origin is: {texts:?}"
    );
}

#[test]
fn set_origin_sits_in_the_footer_beside_close_and_nowhere_else() {
    let _g = snapshot_guard();
    let h = wcs_page_on_a_block();
    let set = h.get_by_label("Set origin").rect();
    let close = h.get_by_label("Close").rect();
    assert!((set.center().y - close.center().y).abs() < 2.0, "one row: {set:?} / {close:?}");
    assert!(set.max.x <= close.min.x, "Set origin comes first, then Close: {set:?} / {close:?}");
    // The footer is under every per-field control of the page.
    let level = h.get_by_label("Level").rect();
    assert!(set.min.y > level.max.y, "the footer is below the page body: {set:?} / {level:?}");
    // The body no longer carries its own apply button.
    assert!(h.query_all_by_label("Set Origin").count() == 0, "no second, body-level apply button");
    assert!(h.query_all_by_label("Set origin").count() == 1, "exactly one Set origin");
    assert!(!has_text(&h, super::super::wcs_footer::NOT_APPLIED), "nothing pending on a fresh page");
}

#[test]
fn a_typed_origin_is_flagged_until_set_origin_applies_it() {
    let _g = snapshot_guard();
    let mut h = wcs_page_on_a_block();
    h.state_mut().state.wcs_ui.x += 5.0;
    h.run_steps(2);
    assert!(has_text(&h, super::super::wcs_footer::NOT_APPLIED), "a typed, unapplied origin is flagged");
    h.get_by_label("Set origin").click();
    h.run_steps(3);
    assert!(!has_text(&h, super::super::wcs_footer::NOT_APPLIED), "applied: no flag");
    let st = &h.state().state;
    let placed = camcraft_app::setup::wcs_entity(&st.world, camcraft_app::setup::active(&st.world).as_deref())
        .and_then(|e| model::transform_of(&st.world, &e))
        .expect("placed");
    let b = model::stock_world_aabb_for(&st.world, camcraft_app::setup::active(&st.world).as_deref()).expect("box");
    assert!((placed.pos.x - (b.min.x + 5.0)).abs() < 1e-9, "Set origin placed the typed X: {placed:?}");
    // A turn typed as another angle triple of the same rotation is not pending.
    h.state_mut().state.wcs_ui.yaw_deg = 360.0;
    h.run_steps(2);
    assert!(!has_text(&h, super::super::wcs_footer::NOT_APPLIED), "yaw 360 is the placed rotation");
}
