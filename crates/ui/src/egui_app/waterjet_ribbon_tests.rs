//! #1883: the Waterjet Profile appears on a waterjet's ribbon - its own
//! Waterjet section, with its own glyph - and on no other workspace's,
//! where its cell is dark and the tip names the waterjet to mount. The
//! op page shows the leads tab. The ribbon and the command door read
//! the one registry word (`KindInfo::admits`).
//! Provenance: our own tests over our own ribbon.

use super::op_icons::op_icon;
use super::op_tabs::{has_section, SEC_LEADS};
use super::panes::{self, ribbon_plan, Cmd};
use crate::state::{CommandKind, State};
use camcraft_app::machine::Kind;
use camcraft_app::operation::{OpType, Section};
use egui::Vec2;
use egui_kittest::{Harness, kittest::Queryable};

fn sections(kind: Kind) -> Vec<Section> {
    ribbon_plan(kind, &[]).into_iter().filter_map(|g| g.section).collect()
}

#[test]
fn the_waterjet_profile_shows_on_a_waterjets_ribbon_only() {
    let jet = OpType::WaterjetProfile.ordinal();
    assert_eq!(sections(Kind::Waterjet), vec![Section::Waterjet], "a waterjet shows its own section alone");
    assert_eq!(Section::Waterjet.kinds_for(Kind::Waterjet), vec![jet]);
    assert!(ribbon_plan(Kind::Waterjet, &[]).iter().any(|g| g.cells.contains(&Cmd::Create(jet))), "the cell stands on a waterjet");
    for kind in [Kind::Mill, Kind::Lathe, Kind::MillTurn, Kind::Plasma, Kind::WireEdm, Kind::SinkerEdm] {
        assert!(!sections(kind).contains(&Section::Waterjet), "{kind:?} shows no waterjet section");
        assert!(ribbon_plan(kind, &[jet]).iter().all(|g| !g.cells.contains(&Cmd::Create(jet))), "{kind:?}: a waterjet pin stands nowhere");
    }

    let mut mill = State::new();
    mill.pending.push_back(CommandKind::SetStock {
        shape: camcraft_app::stock::StockShape::Box { dx: 40.0, dy: 40.0, dz: 6.0 },
        pos: [0.0, 0.0, 0.0],
        yaw_deg: 0.0,
    });
    mill.pending.push_back(CommandKind::LoadBuiltinMachine("VMC 3-axis".into()));
    mill.pending.push_back(CommandKind::AddTool { name: "Nozzle".into(), tool: camcraft_app::waterjet_tools::waterjet_tool(0.8) });
    mill.drain_actions();
    assert!(!panes::enabled(Cmd::Create(jet), &mill), "a mill carries no jet");
    let tip = panes::tip_of(Cmd::Create(jet), &mill);
    assert!(tip.contains("waterjet") && !tip.contains("mill-turn"), "{tip}");
}

#[test]
fn the_waterjet_profile_has_its_own_glyph_and_a_leads_tab() {
    let mine = op_icon(OpType::WaterjetProfile);
    assert_ne!(mine, op_icon(OpType::PlasmaProfile), "not the plasma's glyph");
    assert!(has_section(OpType::WaterjetProfile, SEC_LEADS), "the leads live on Lead in / Lead out");
}

fn waterjet_fields(tab: usize) -> Harness<'static, State> {
    let mut state = State::new();
    state.op_ui.params = crate::operations::quick_params(OpType::WaterjetProfile.ordinal());
    let camcraft_app::operation::OpParams::WaterjetProfile(params) = &mut state.op_ui.params else {
        panic!("the waterjet draft must have waterjet parameters");
    };
    params.control_comp = true;
    params.lead_in = 0.0;
    params.tabs = Some(camcraft_app::operation::JetTabsOp { count: 2, width: 1.0, at: vec![[0.0, 0.0], [10.0, 0.0]] });
    Harness::builder().with_size(Vec2::new(900.0, 900.0))
        .build_ui_state(move |ui, state: &mut State| super::params_page::params_fields(ui, state, tab), state)
}

#[test]
fn waterjet_compensation_uses_a_plain_control_label() {
    let mut h = waterjet_fields(super::op_tabs::SEC_GEO);
    h.run();
    let values: Vec<_> = h.query_all_by_role(egui::accesskit::Role::ComboBox).filter_map(|node| node.value()).collect();
    assert!(values.iter().any(|value| value == "In control"), "the selected compensation uses plain words: {values:?}");
    assert!(values.iter().all(|value| !value.contains("G41/G42")), "{values:?}");
}

#[test]
fn waterjet_tab_position_rows_have_unique_names() {
    let mut h = waterjet_fields(super::op_tabs::SEC_PASSES);
    h.run();
    assert!(h.query_by_label("Tab 1 position").is_some());
    assert!(h.query_by_label("Tab 2 position").is_some());
    assert!(h.query_by_label("Tab position").is_none());
}

#[test]
fn waterjet_zero_lead_in_names_the_part_edge_pierce() {
    let mut h = waterjet_fields(SEC_LEADS);
    h.run();
    assert!(h.query_by_label_contains("zero lead-in").is_some());
    assert!(h.query_by_label_contains("part edge").is_some());
}
