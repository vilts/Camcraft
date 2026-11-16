//! PR #2001: structural pins for the nozzle editor's recipe form.
//! Exercises the shipping editor through the same AccessKit harness as
//! the shell structural tests. No snapshot PNGs are changed.

use egui_kittest::kittest::NodeT;

use super::*;
use camcraft_app::waterjet_tools::{PierceMethod, waterjet_tool};

const NOZZLE: &str = "Recipe nozzle";

fn editor() -> Harness<'static, ShellUi> {
    let mut h = Harness::builder()
        .with_size(Vec2::new(1600.0, 1600.0))
        .build_ui_state(frame, ShellUi::seeded());
    h.state_mut().state.pending.push_back(CommandKind::AddTool {
        name: NOZZLE.into(), tool: waterjet_tool(0.8),
    });
    h.run_steps(2);
    crate::actions::open_tool_editor(&mut h.state_mut().state, NOZZLE);
    super::super::tool_cutting_rows::add_row(&mut h.state_mut().state.tool_ui);
    h.run_steps(3);
    h
}

#[test]
fn waterjet_recipe_controls_render_without_spindle_rpm() {
    let _g = snapshot_guard();
    let mut h = editor();
    {
        let row = &mut h.state_mut().state.tool_ui.cutting[0];
        row.feed = 731.0;
        let w = row.waterjet.as_mut().unwrap();
        w.thickness = 5.1;
        w.kerf = 0.91;
        w.pierce_time = 2.3;
        w.abrasive_delay = 0.4;
        w.re_pierce_time = 1.2;
        w.pierce_method = PierceMethod::Dynamic;
    }
    h.run_steps(3);
    let win = h.get_by_label("Tool: Recipe nozzle");
    assert!(win.query_by_label("Cutting data by material and thickness").is_some());
    let fields = [
        ("Plate thickness", "5.1 mm"),
        ("Kerf width", "0.91 mm"),
        ("Pierce time (s)", "2.3"),
        ("Abrasive delay (s)", "0.4"),
        ("Re-pierce time (s)", "1.2"),
    ];
    let spins: Vec<_> = win.query_all_by_role(egui::accesskit::Role::SpinButton).collect();
    let mut last_y = f32::MIN;
    for (label, value) in fields {
        // Kerf also exists as the nozzle's general width; the recipe
        // caption is the copy below Plate thickness.
        let caption = win.query_all_by_label(label)
            .find(|n| n.rect().top() > last_y).expect(label);
        last_y = caption.rect().top();
        assert!(spins.iter().any(|n| n.value().as_deref() == Some(value)
            && n.rect().top() > caption.rect().top()
            && n.rect().top() < caption.rect().bottom() + 15.0),
            "{label} has its editable recipe value {value}");
    }
    assert!(win.get_by_label("Pierce method").rect().top() > last_y);
    let method = win.query_all_by_role(egui::accesskit::Role::ComboBox)
        .find(|n| n.value().as_deref() == Some("Dynamic")).expect("pierce method combo reads the recipe");
    assert!(win.query_by_label("Spindle rpm").is_none(), "no nozzle or recipe spindle control");
    method.click();
    h.run_steps(2);
    h.get_by_label("Wiggle").click();
    h.run_steps(2);
    assert_eq!(h.state().state.tool_ui.cutting[0].waterjet.unwrap().pierce_method, PierceMethod::Wiggle);
}

#[test]
fn waterjet_recipe_apply_refuses_invalid_and_accepts_valid_recipe() {
    let _g = snapshot_guard();
    let mut h = editor();
    assert!(h.state().state.tool_ui.to_tool().torch_refusal().is_some());
    assert!(h.get_by_label("Tool: Recipe nozzle").get_by_label("Apply").accesskit_node().is_disabled(),
        "zero thickness keeps Apply disabled");
    h.state_mut().state.tool_ui.cutting[0].waterjet.as_mut().unwrap().thickness = 5.0;
    h.run_steps(2);
    assert!(!h.get_by_label("Tool: Recipe nozzle").get_by_label("Apply").accesskit_node().is_disabled(),
        "a valid recipe enables Apply");
    let valid = h.state().state.tool_ui.cutting[0].waterjet.unwrap();
    for field in ["thickness", "kerf", "pierce time", "abrasive delay", "re-pierce time"] {
        let recipe = h.state_mut().state.tool_ui.cutting[0].waterjet.as_mut().unwrap();
        *recipe = valid;
        match field {
            "thickness" => recipe.thickness = 0.0,
            "kerf" => recipe.kerf = 0.0,
            "pierce time" => recipe.pierce_time = -1.0,
            "abrasive delay" => recipe.abrasive_delay = -1.0,
            "re-pierce time" => recipe.re_pierce_time = -1.0,
            _ => unreachable!(),
        }
        h.run_steps(2);
        assert!(h.get_by_label("Tool: Recipe nozzle").get_by_label("Apply").accesskit_node().is_disabled(),
            "invalid {field} disables Apply");
    }
    *h.state_mut().state.tool_ui.cutting[0].waterjet.as_mut().unwrap() = valid;
    h.run_steps(2);
    h.get_by_label("Tool: Recipe nozzle").get_by_label("Apply").click();
    h.run_steps(2);
    let st = &h.state().state;
    let tool = model::find_by_name(&st.world, NOZZLE).and_then(|e| model::tool_of(&st.world, &e)).unwrap();
    assert_eq!(tool.cutting.len(), 1);
    assert_eq!(tool.cutting[0].waterjet.unwrap().thickness, 5.0);
}

#[test]
fn waterjet_properties_apply_preserves_recipe_refusal() {
    let _g = snapshot_guard();
    let mut h = editor();
    super::super::browser_pane::click(
        &mut h.state_mut().state, panes::Leaf::Tool(NOZZLE.into()),
    );
    h.run_steps(3);
    super::super::tool_cutting_rows::add_row(&mut h.state_mut().state.tool_ui);
    h.run_steps(3);
    assert!(h.state().state.tool_editor.is_none());
    assert!(h.state().state.tool_ui.to_tool().torch_refusal().is_some());
    h.get_by_label("Apply").scroll_to_me();
    h.run_steps(3);
    assert!(h.get_by_label("Apply").accesskit_node().is_disabled(),
        "Properties preserves the invalid waterjet recipe refusal");
    h.state_mut().state.tool_ui.cutting[0].waterjet.as_mut().unwrap().thickness = 5.0;
    h.run_steps(3);
    assert!(!h.get_by_label("Apply").accesskit_node().is_disabled(),
        "Properties accepts a valid waterjet recipe");
}
