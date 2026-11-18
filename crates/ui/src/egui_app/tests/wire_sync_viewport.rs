//! #1171's frame test: with the Wire Contour page's sync pick armed,
//! real viewport clicks on corners of the seed's 60 x 60 x 30 block state
//! a sync pair into the page draft (upper first, then a lower point below
//! it), a flat second click is refused by name, and the stored pair is
//! drawn as a ruling. The draft is not the command history: nothing is
//! applied by a click.
//! Provenance: our own pick law test over our own shell; no vendor
//! material consulted.

use camcraft_app::operation::{OpParams, WireFourAxis, WirePairingRule};

use super::*;

fn pixel(sh: &ShellUi, p: [f64; 3]) -> egui::Pos2 {
    let ([x0, y0], [x1, y1]) = sh.state.viewport_rect;
    let (w, hgt) = (x1 - x0, y1 - y0);
    let mvp = sh.state.cam.mvp(f64::from(w / hgt));
    let uv = interact::project_uv(&mvp, p).expect("the corner is in front of the camera");
    egui::pos2(x0 + uv[0] * w, y0 + uv[1] * hgt)
}

fn sync(h: &Harness<'static, ShellUi>) -> Vec<camcraft_app::operation::WireSyncPair> {
    match &h.state().state.op_ui.params {
        OpParams::WireContour(w) => w.four_axis.as_ref().map(|f| f.sync.clone()).unwrap_or_default(),
        _ => Vec::new(),
    }
}

#[test]
fn two_clicked_corners_state_a_sync_pair_and_a_flat_one_is_refused() {
    let mut harness = shell_harness();
    harness.run();
    {
        let st = &mut harness.state_mut().state;
        let machine = camcraft_app::machine::parse_mch(include_str!("../../../../app/tests/edm/wire-iso-fixture.mch")).expect("fixture");
        st.pending.push_back(CommandKind::EditMachineApply { path: "wire-test.mch".into(), machine: Box::new(machine) });
        st.pending.push_back(CommandKind::AddTool {
            name: "EDM-W025".into(),
            tool: camcraft_app::wire_tools::wire_tool(0.25),
        });
        st.drain_actions();
        let kind = camcraft_app::operation::KindInfo::ALL.iter().position(|k| k.label == "Wire Contour").expect("wire kind");
        crate::actions::add_operation(st, kind);
        st.drain_actions();
        st.sync_ops_ui();
        let OpParams::WireContour(w) = &mut st.op_ui.params else { panic!("a wire draft: {}", st.status) };
        w.four_axis = Some(WireFourAxis { pairing: WirePairingRule::SyncPoints, ..WireFourAxis::default() });
        crate::wire_sync_pick::arm(st);
        assert_eq!(st.geo_pick, GeoPick::Vertex, "arming sets Vertex mode");
    }
    let geo_before = harness.state().state.op_ui.geo.clone();
    let px = pixel(harness.state(), [0.0, 0.0, 30.0]);
    click_viewport(&mut harness, px);
    let s = sync(&harness);
    assert_eq!(s.len(), 1, "{}", harness.state().state.status);
    assert!(s[0].upper.is_some() && s[0].lower.is_none(), "{s:?}");
    assert_eq!(crate::wire_sync_pick::next_click(&s), "click the lower point of pair 1 (a vertex of the lower profile)");
    // A second corner at the same height is not below the first.
    let px = pixel(harness.state(), [60.0, 0.0, 30.0]);
    click_viewport(&mut harness, px);
    assert!(harness.state().state.status.contains("must lie below"), "{}", harness.state().state.status);
    assert!(sync(&harness)[0].lower.is_none(), "a refused click changes nothing");
    let px = pixel(harness.state(), [60.0, 0.0, 0.0]);
    click_viewport(&mut harness, px);
    let s = sync(&harness);
    assert_eq!(s[0].upper, Some([0.0, 0.0]), "{s:?}");
    assert_eq!(s[0].lower, Some([60.0, 0.0]), "{s:?} ({})", harness.state().state.status);
    let st = &harness.state().state;
    assert_eq!(st.op_ui.geo, geo_before, "nothing joins the open operation");
    assert!(!crate::wire_sync_pick::segments(st, 1.6).is_empty(), "the stored pair is drawn as a ruling");
}
