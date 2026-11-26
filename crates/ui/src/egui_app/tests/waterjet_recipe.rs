//! PR #2001: structural pins for the nozzle editor's recipe form.
//! Exercises the shipping editor through the same AccessKit harness as
//! the shell structural tests. No snapshot PNGs are changed. #3930: the
//! recipe form is the tool library window's Recipes tab (Properties shows
//! a summary).
//! Gate map: covers ui::egui_app::tool_library_dialog, ui::egui_app::tool_cutting_rows.

use egui_kittest::kittest::NodeT;

use super::tool_library_editor::{click_window_button, draft, library_window, open_in_window};
use super::*;
use camcraft_app::waterjet_tools::{PierceMethod, waterjet_tool};

const NOZZLE: &str = "Recipe nozzle";

fn editor() -> Harness<'static, ShellUi> {
    editor_in(Unit::Millimeters)
}

/// The same editor with the document's length unit set first.
fn editor_in(unit: Unit) -> Harness<'static, ShellUi> {
    let mut h = Harness::builder()
        .with_size(Vec2::new(1600.0, 1600.0))
        .build_ui_state(frame, ShellUi::seeded());
    let st = &mut h.state_mut().state;
    if unit != Unit::Millimeters {
        st.pending.push_back(CommandKind::SetUnits(unit));
    }
    st.pending.push_back(CommandKind::AddTool {
        name: NOZZLE.into(), tool: waterjet_tool(0.8),
    });
    h.run_steps(2);
    // #3930: the recipe is a tool setting, so it is edited on the tool
    // library window's Recipes tab; the shape window does not carry it.
    open_in_window(&mut h, NOZZLE);
    click_window_button(&mut h, crate::egui_app::tool_library_dialog::recipes_tab::TAB);
    super::super::tool_cutting_rows::add_row(draft(&mut h));
    h.run_steps(3);
    h
}

/// The window's Apply button.
fn apply<'h>(h: &'h Harness<'static, ShellUi>) -> egui_kittest::Node<'h> {
    library_window(h).get_by_role_and_label(egui::accesskit::Role::Button, "Apply")
}

#[test]
fn waterjet_recipe_controls_render_without_spindle_rpm() {
    let _g = snapshot_guard();
    let mut h = editor();
    {
        let row = &mut draft(&mut h).cutting[0];
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
    let win = library_window(&h);
    assert!(win.query_by_label(crate::egui_app::tool_library_dialog::recipes_tab::TAB).is_some());
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
        // #2014: the caption is left of its value on the same row.
        assert!(spins.iter().any(|n| n.value().as_deref() == Some(value)
            && (n.rect().center().y - caption.rect().center().y).abs() < 4.0),
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
    assert_eq!(draft(&mut h).cutting[0].waterjet.unwrap().pierce_method, PierceMethod::Wiggle);
}

/// #2663 review: the recipe's feed row is a feed rate, not a length. It
/// shows the `mm/min` / `in/min` suffix in both document unit modes.
#[test]
fn waterjet_recipe_feed_row_reads_as_a_feed_rate_in_both_unit_modes() {
    let _g = snapshot_guard();
    for (unit, name, expect) in [
        (Unit::Millimeters, "mm", "731 mm/min"),
        (Unit::Inches, "inch", "28.78 in/min"),
    ] {
        let mut h = editor_in(unit);
        draft(&mut h).cutting[0].feed = 731.0;
        h.run_steps(3);
        let win = library_window(&h);
        let spins: Vec<String> = win
            .query_all_by_role(egui::accesskit::Role::SpinButton)
            .filter_map(|n| n.value())
            .collect();
        assert!(
            spins.iter().any(|v| v.ends_with(&format!(" {}", unit.feed_suffix()))
                && v.starts_with(expect.split(' ').next().unwrap())),
            "{name} units: the recipe feed reads as a feed rate {expect}; values {spins:?}"
        );
        assert!(
            spins.iter().all(|v| !v.starts_with("731")
                || v.ends_with("mm/min") || v.ends_with("in/min")),
            "{name} units: no recipe field shows the feed 731 as a plain length; values {spins:?}"
        );
    }
}

#[test]
fn waterjet_recipe_apply_refuses_invalid_and_accepts_valid_recipe() {
    let _g = snapshot_guard();
    let mut h = editor();
    assert!(draft(&mut h).to_tool().torch_refusal().is_some());
    assert!(apply(&h).accesskit_node().is_disabled(), "zero thickness keeps Apply disabled");
    draft(&mut h).cutting[0].waterjet.as_mut().unwrap().thickness = 5.0;
    h.run_steps(2);
    assert!(!apply(&h).accesskit_node().is_disabled(), "a valid recipe enables Apply");
    let valid = draft(&mut h).cutting[0].waterjet.unwrap();
    for field in ["thickness", "kerf", "pierce time", "abrasive delay", "re-pierce time"] {
        let recipe = draft(&mut h).cutting[0].waterjet.as_mut().unwrap();
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
        assert!(apply(&h).accesskit_node().is_disabled(), "invalid {field} disables Apply");
    }
    *draft(&mut h).cutting[0].waterjet.as_mut().unwrap() = valid;
    h.run_steps(2);
    apply(&h).click();
    h.run_steps(2);
    let st = &h.state().state;
    let tool = model::find_by_name(&st.world, NOZZLE).and_then(|e| model::tool_of(&st.world, &e)).unwrap();
    assert_eq!(tool.cutting.len(), 1);
    assert_eq!(tool.cutting[0].waterjet.unwrap().thickness, 5.0);
}

/// #3930: a browser pick of the nozzle shows its Properties summary (no
/// Apply there); the window's draft keeps the invalid recipe, and its
/// Apply keeps the refusal until the recipe is valid. Before:
/// `waterjet_properties_apply_preserves_recipe_refusal`.
#[test]
fn waterjet_window_apply_preserves_recipe_refusal_across_a_browser_pick() {
    let _g = snapshot_guard();
    let mut h = editor();
    super::super::browser_pane::click(
        &mut h.state_mut().state, panes::Leaf::Tool(NOZZLE.into()),
    );
    h.run_steps(3);
    assert!(h.state().state.tool_shape_editor.is_none());
    assert!(
        h.get_by_role_and_label(egui::accesskit::Role::Pane, "Properties").query_by_label("Apply").is_none(),
        "the Properties summary has no Apply"
    );
    assert!(draft(&mut h).to_tool().torch_refusal().is_some(), "the window keeps its draft");
    assert!(apply(&h).accesskit_node().is_disabled(), "the window preserves the invalid waterjet recipe refusal");
    draft(&mut h).cutting[0].waterjet.as_mut().unwrap().thickness = 5.0;
    h.run_steps(3);
    assert!(!apply(&h).accesskit_node().is_disabled(), "the window accepts a valid waterjet recipe");
}
