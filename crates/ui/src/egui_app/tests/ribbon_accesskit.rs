//! Pinning tests for #1988: painted ribbon cells, workspace tabs, and
//! dropdown rows expose AccessKit nodes (`Button`, `MenuItem`) by label.
//! Provenance: our own tests over our own egui ribbon; no HSMWorks material.

use camcraft_app::recent::RecentKind;
use egui::accesskit::{Role, Toggled};
use egui_kittest::kittest::NodeT;

use super::*;

#[test]
fn ribbon_cells_and_tabs_expose_button_accesskit_nodes() {
    let mut h = shell_harness();
    h.run();
    let expected_cells = [
        "New", "Model", "Open", "Save", "Save As", "Post", "Setups", "Stock", "Origin", "Tools",
        "2D", "3D", "Drill", "Multi", "Undo", "Redo", "Run", "Delete", "Frame", "Paths",
        "Simulate", "Deviate", "Panels", "Theme",
    ];
    for label in expected_cells {
        h.get_by_role_and_label(Role::Button, label);
    }
    assert!(
        h.get_by_role_and_label(Role::Button, "Redo").accesskit_node().is_disabled(),
        "Redo starts disabled with nothing undone"
    );
    assert!(
        !h.get_by_role_and_label(Role::Button, "Post").accesskit_node().is_disabled(),
        "Post starts enabled on seeded document"
    );
    let sim = h.get_by_role_and_label(Role::Button, "Simulate");
    assert_eq!(sim.accesskit_node().toggled(), Some(Toggled::False));
    sim.click();
    h.run_steps(2);
    assert_eq!(
        h.get_by_role_and_label(Role::Button, "Simulate").accesskit_node().toggled(),
        Some(Toggled::True)
    );
    for tab in ["Milling", "Turning", "Mill-turning", "Plasma cutting", "Wire EDM"] {
        h.get_by_role_and_label(Role::Tab, tab);
    }
    assert_eq!(
        h.get_by_role_and_label(Role::Tab, "Milling").accesskit_node().is_selected(),
        Some(true)
    );
    assert_eq!(
        h.get_by_role_and_label(Role::Tab, "Turning").accesskit_node().is_selected(),
        Some(false)
    );
}

#[test]
fn section_dropdown_rows_expose_menu_items() {
    let mut h = shell_harness();
    h.run();
    h.get_by_role_and_label(Role::Button, "2D").click();
    h.run_steps(2);
    let pocket = h.get_by_role_and_label(Role::MenuItem, "2D Pocket");
    assert!(!pocket.accesskit_node().is_disabled());
    pocket.click();
    h.run_steps(2);
    assert!(model::find_by_name(&h.state().state.world, "2D Pocket1").is_some());
}

#[test]
fn multiaxis_dropdown_lists_and_creates_rotary_flow() {
    let mut h = shell_harness();
    // The flow band cuts with a ball (#2826): seed one so the Create
    // cell is not dark - the tool law's own gate.
    let ball = camcraft_app::tools::catalog()
        .into_iter()
        .find(|(n, _)| *n == "BN-0600")
        .map(|(_, t)| t)
        .expect("the catalog's ball-nose entry");
    let st = h.state_mut();
    st.state
        .history
        .execute(&st.state.world, camcraft_app::commands::AddTool::new(&st.state.world, "BN-0600", ball).expect("tool"));
    h.run();
    h.get_by_role_and_label(Role::Button, "Multi").click();
    h.run_steps(2);
    let flow = h.get_by_role_and_label(Role::MenuItem, "Rotary Flow");
    assert!(!flow.accesskit_node().is_disabled(), "the Multi-axis group lists Rotary Flow enabled");
    flow.click();
    h.run_steps(2);
    assert!(model::find_by_name(&h.state().state.world, "Rotary Flow1").is_some());
}

#[test]
fn view_dropdown_rows_expose_menu_items() {
    let mut h = shell_harness();
    h.run();
    h.get_by_role_and_label(Role::Button, "Panels").click();
    h.run_steps(2);
    assert!(
        h.get_by_role_and_label(Role::MenuItem, "Viewport").accesskit_node().is_disabled(),
        "Viewport row is inert/disabled"
    );
    let browser = h.get_by_role_and_label(Role::MenuItem, "Browser");
    assert!(!browser.accesskit_node().is_disabled());
    assert_eq!(browser.accesskit_node().toggled(), Some(Toggled::True));
    h.get_by_role_and_label(Role::MenuItem, "Properties");
    h.get_by_role_and_label(Role::MenuItem, "Machines");
    h.get_by_role_and_label(Role::MenuItem, "AI Chat");
    h.get_by_role_and_label(Role::MenuItem, "Reset layout");
}

#[test]
fn theme_dropdown_rows_expose_menu_items() {
    let mut h = shell_harness();
    h.run();
    h.get_by_role_and_label(Role::Button, "Theme").click();
    h.run_steps(2);
    assert_eq!(
        h.get_by_role_and_label(Role::MenuItem, "Dark  (current)")
            .accesskit_node()
            .toggled(),
        Some(Toggled::True)
    );
    h.get_by_role_and_label(Role::MenuItem, "Light");
    h.get_by_role_and_label(Role::MenuItem, "Deuteranopia");
    h.get_by_role_and_label(Role::MenuItem, "Browse...");
}

#[test]
fn recent_dropdown_rows_expose_menu_items() {
    let d = std::env::temp_dir().join(format!("camcraft-ak-recent-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    let _ = std::fs::create_dir_all(&d);
    let p = d.join("kept.camproj");
    let mut other = State::new();
    world::seed(&mut other);
    other.save_project_path(p.clone());
    let mut h = shell_harness();
    h.state_mut().state.recent.add(RecentKind::Project, &p);
    h.run();
    h.get_by_role_and_label(Role::Button, "Open").click();
    h.run_steps(2);
    h.get_by_role_and_label(Role::MenuItem, "kept.camproj");
    h.get_by_role_and_label(Role::MenuItem, "Clear recent");
    h.get_by_role_and_label(Role::MenuItem, "Browse…");
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn workspace_tabs_expose_button_accesskit_nodes_and_switch() {
    let mut h = shell_harness();
    h.run();
    h.get_by_role_and_label(Role::Tab, "Turning").click();
    h.run_steps(2);
    assert_eq!(
        camcraft_app::machine::kind_of(&h.state().state.world),
        camcraft_app::machine::Kind::Lathe
    );
    assert_eq!(
        h.get_by_role_and_label(Role::Tab, "Turning")
            .accesskit_node()
            .is_selected(),
        Some(true)
    );
}
