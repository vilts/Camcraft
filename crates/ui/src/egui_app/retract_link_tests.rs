//! Gate map: covers ui::egui_app::lead_page, ui::egui_app::kind_editors,
//! ui::egui_app::heights_page, ui::interact.
//! Provenance: our own structural regression of issue #3318.

use super::*;
use camcraft_app::links::Link;
use camcraft_app::operation::{KindInfo, OpType, seed_params};

fn roughing(ty: OpType, retract: Option<Height>) -> OpParams {
    let kind = KindInfo::ALL.iter().position(|kind| kind.ty == ty).expect("roughing kind");
    let mut params = seed_params(kind);
    match &mut params {
        OpParams::Offset2D { clearance_z, retract_z, top_z, bottom_z, links, .. }
        | OpParams::Adaptive2D { clearance_z, retract_z, top_z, bottom_z, links, .. } => {
            *clearance_z = Height::fixed(30.0);
            *retract_z = retract;
            *top_z = Height::fixed(20.0);
            *bottom_z = Height::fixed(5.0);
            links.level = Link::Retract;
            links.pass = Link::Retract;
        }
        other => panic!("roughing kind expected, got {other:?}"),
    }
    params
}

fn assert_link_planes(ty: OpType, hop: bool) {
    let params = roughing(ty, hop.then(|| Height::fixed(25.0)));
    let mut heights = heights_tab(opened(params.clone()));
    heights.run();
    assert!(heights.query_by_label("Clearance Z").is_some());
    assert_eq!(heights.query_by_label("Retract Z").is_some(), hop);
    let labels: Vec<_> = crate::interact::overlay_labels(heights.state(), 4.0 / 3.0)
        .into_iter().map(|(_, _, label)| label).collect();
    assert!(labels.iter().any(|label| label.contains("Clearance 30.00")), "{labels:?}");
    assert_eq!(labels.iter().any(|label| label.contains("Retract 25.00")), hop, "{labels:?}");
    assert_eq!(labels.iter().any(|label| label.contains("Retract")), hop, "{labels:?}");

    let mut leads = Harness::builder()
        .with_size(Vec2::new(600.0, 1200.0))
        .build_ui_state(
            |ui, state: &mut State| super::super::params_page::params_fields(ui, state, super::super::op_tabs::SEC_LEADS),
            opened(params.clone()),
        );
    leads.run();
    let expected = if hop { "Retract plane" } else { "Clearance plane (hop off)" };
    assert_eq!(combos_showing(&leads, expected), 2, "both links name the effective height plane");
    assert_eq!(combos_showing(&leads, "Retract plane"), if hop { 2 } else { 0 });
    assert_eq!(leads.state().op_ui.params, params, "drawing must preserve the stored link choice");
    assert!(leads.state().pending.is_empty());
    match &mut leads.state_mut().op_ui.params {
        OpParams::Offset2D { links, .. } | OpParams::Adaptive2D { links, .. } => links.pass = Link::KeepDown,
        other => panic!("roughing kind expected, got {other:?}"),
    }
    leads.run();
    leads.ctx.all_styles_mut(|style| {
        style.interaction.tooltip_delay = 0.0;
        style.interaction.show_tooltips_only_when_still = false;
    });
    leads.get_by_label("Max stay-down distance").hover();
    leads.run_steps(2);
    let fallback = if hop {
        "every other link lifts to the retract plane"
    } else {
        "every other link lifts to Clearance Z because Hop below clearance is off"
    };
    assert!(leads.query_by_label_contains(fallback).is_some(), "the Keep down fallback names its plane");
}

#[test]
fn offset_retract_links_name_the_visible_plane() {
    assert_link_planes(OpType::Offset2D, false);
    assert_link_planes(OpType::Offset2D, true);
}

#[test]
fn adaptive_retract_links_name_the_visible_plane() {
    assert_link_planes(OpType::Adaptive2D, false);
    assert_link_planes(OpType::Adaptive2D, true);
}
