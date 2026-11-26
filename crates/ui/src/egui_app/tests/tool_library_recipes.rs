//! #3896 (U5 of #3663): the Recipes tab of the tool library window.
//! "Suggest" and "Copy from tool" fill the window's draft and queue no
//! command; the one Apply lands the rows as one `ChangeTool`; the row of
//! the document's material is marked; Properties' draft never changes.
//! Gate map: covers ui::egui_app::tool_library_dialog, ui::egui_app::tool_cutting_rows, ui::state.
//! Provenance: our own structural tests over the AccessKit tree and the
//! command history; no HSMWorks material involved.

use super::tool_library_editor::{H, click_window_button, draft, library_window, open_in_window, tool};
use super::*;
use crate::egui_app::tool_library_dialog::recipes_tab::{COPY, MARK, SUGGEST, TAB};
use crate::state::CommandKind;
use camcraft_app::tool_cutting_rows::CuttingRow;
use camcraft_app::tools::{Tool, ToolMaterial, WorkMaterial};

/// A document tool that the cutting table speaks for (an end, ball, bull
/// nose or chamfer mill), with a carbide edge, three flutes and the
/// given material rows. Lands through `ChangeTool`.
fn mill(h: &mut H, rows: Vec<CuttingRow>) -> String {
    let st = &h.state().state;
    let found = st.tool_names().into_iter().find(|n| {
        model::find_by_name(&st.world, n)
            .and_then(|e| model::tool_of(&st.world, &e))
            .is_some_and(|t| camcraft_app::cutting::tabulated(t.kind))
    });
    let name = match found {
        Some(n) => n,
        None => {
            let Some((n, t)) = camcraft_app::tools::catalog().into_iter().find(|(n, t)| {
                model::find_by_name(&st.world, n).is_none()
                    && camcraft_app::cutting::tabulated(t.kind)
                    && crate::machine_modes::tool_kind_offered(&st.world, t.kind)
            }) else {
                panic!("the catalog has a mill for this machine");
            };
            h.state_mut().state.pending.push_back(CommandKind::AddTool { name: n.to_owned(), tool: t });
            h.run_steps(2);
            n.to_owned()
        }
    };
    let t = Tool { tool_material: Some(ToolMaterial::Carbide), flutes: Some(3), cutting: rows, ..tool(h, &name) };
    h.state_mut().state.pending.push_back(CommandKind::ChangeTool { name: name.clone(), tool: t });
    h.run_steps(2);
    assert_eq!(tool(h, &name).flutes, Some(3), "the mill lands: {}", h.state().state.status);
    name
}

/// A second mill from the catalog, added (`AddTool`) with `rows`.
fn other_mill(h: &mut H, rows: Vec<CuttingRow>) -> String {
    let st = &h.state().state;
    let Some((n, t)) = camcraft_app::tools::catalog().into_iter().find(|(n, t)| {
        model::find_by_name(&st.world, n).is_none()
            && camcraft_app::cutting::tabulated(t.kind)
            && crate::machine_modes::tool_kind_offered(&st.world, t.kind)
    }) else {
        panic!("the catalog has a second mill for this machine");
    };
    let tool = Tool { cutting: rows, ..t };
    h.state_mut().state.pending.push_back(CommandKind::AddTool { name: n.to_owned(), tool });
    h.run_steps(2);
    assert!(model::find_by_name(&h.state().state.world, n).is_some(), "the second mill lands: {}", h.state().state.status);
    n.to_owned()
}

fn set_material(h: &mut H, m: Option<WorkMaterial>) {
    h.state_mut().state.pending.push_back(CommandKind::SetWorkMaterial(m));
    h.run_steps(3);
}

fn row_of(rows: &[CuttingRow], m: WorkMaterial) -> Option<CuttingRow> {
    rows.iter().find(|r| r.material == Some(m) && !r.is_recipe()).copied()
}

/// Suggest fills the window's draft from the cutting table and queues no
/// command; Properties' draft of the same tool does not change. Apply is
/// one `ChangeTool` undo step.
#[test]
fn suggest_fills_the_draft_and_apply_is_one_undo_step() {
    let mut h = shell_harness();
    h.run_steps(2);
    let name = mill(&mut h, Vec::new());
    set_material(&mut h, Some(WorkMaterial::Aluminium6061));
    crate::actions::target(&mut h.state_mut().state, UiTarget::Tool(name.clone()));
    h.run_steps(3);
    open_in_window(&mut h, &name);
    click_window_button(&mut h, TAB);
    let before = tool(&h, &name);
    let props = h.state().state.tool_ui.cutting.clone();
    let done = h.state().state.history.done_len();
    click_window_button(&mut h, SUGGEST);
    let st = &h.state().state;
    let machine = camcraft_app::machine::of_setup(&st.world, None).1;
    let want = camcraft_app::cutting::compute(
        &name, &before, 3, ToolMaterial::Carbide, WorkMaterial::Aluminium6061, None, machine.as_ref(),
    );
    let Some(row) = row_of(&draft(&mut h).cutting, WorkMaterial::Aluminium6061) else {
        panic!("Suggest adds the document material's row: {}", h.state().state.status);
    };
    assert_eq!((row.rpm, row.feed), (want.rpm, want.feed), "the row holds the table's numbers");
    assert_eq!(h.state().state.history.done_len(), done, "Suggest queues no command");
    assert_eq!(tool(&h, &name), before, "Suggest writes the draft, not the document");
    assert_eq!(h.state().state.tool_ui.cutting, props, "Properties' draft of the same tool does not change");
    click_window_button(&mut h, "Apply");
    assert_eq!(h.state().state.history.done_len(), done + 1, "one Apply is one undo step");
    let landed = row_of(&tool(&h, &name).cutting, WorkMaterial::Aluminium6061);
    assert_eq!(landed.map(|r| (r.rpm, r.feed)), Some((want.rpm, want.feed)), "Apply lands the row: {}", h.state().state.status);
    crate::actions::undo(&mut h.state_mut().state);
    h.run_steps(2);
    assert_eq!(tool(&h, &name), before, "one undo restores the tool");
}

/// Copy from tool puts the other tool's material rows into the draft: a
/// row of the same material is replaced, the others join, the draft's own
/// rows stay. Nothing lands before Apply.
#[test]
fn copy_from_tool_copies_the_rows() {
    let mut h = shell_harness();
    h.run_steps(2);
    let steel = CuttingRow::plain(Some(WorkMaterial::MildSteel), 3000.0, 500.0);
    let brass = CuttingRow::plain(Some(WorkMaterial::Brass), 8000.0, 1200.0);
    let plastics = CuttingRow::plain(Some(WorkMaterial::Plastics), 12000.0, 2000.0);
    let a = mill(&mut h, vec![CuttingRow::plain(Some(WorkMaterial::MildSteel), 1000.0, 100.0), plastics]);
    let b = other_mill(&mut h, vec![steel, brass]);
    open_in_window(&mut h, &a);
    click_window_button(&mut h, TAB);
    h.ctx.data_mut(|d| d.insert_temp(egui::Id::new("tool_recipes_copy_from"), b.clone()));
    h.run_steps(2);
    let before = tool(&h, &a);
    let done = h.state().state.history.done_len();
    click_window_button(&mut h, COPY);
    let rows = draft(&mut h).cutting.clone();
    assert_eq!(row_of(&rows, WorkMaterial::MildSteel), Some(steel), "the same material's row is replaced");
    assert_eq!(row_of(&rows, WorkMaterial::Brass), Some(brass), "a new material's row joins");
    assert_eq!(row_of(&rows, WorkMaterial::Plastics), Some(plastics), "the draft's own row stays");
    assert_eq!(h.state().state.history.done_len(), done, "Copy queues no command");
    assert_eq!(tool(&h, &a), before, "Copy writes the draft, not the document");
    click_window_button(&mut h, "Apply");
    assert_eq!(h.state().state.history.done_len(), done + 1, "one Apply is one undo step");
    let landed = tool(&h, &a).cutting;
    assert_eq!(row_of(&landed, WorkMaterial::Brass), Some(brass), "Apply lands the copied rows: {}", h.state().state.status);
}

/// The row of the document's material carries the mark; with no row for
/// it, the default row does; with no material stated, no row does. The
/// table also shows the surface speed of each row.
#[test]
fn the_document_material_row_is_marked() {
    let mut h = shell_harness();
    h.run_steps(2);
    let rows = vec![
        CuttingRow::plain(Some(WorkMaterial::MildSteel), 3000.0, 500.0),
        CuttingRow::plain(Some(WorkMaterial::Brass), 8000.0, 1200.0),
    ];
    let name = mill(&mut h, rows);
    set_material(&mut h, Some(WorkMaterial::Brass));
    open_in_window(&mut h, &name);
    click_window_button(&mut h, TAB);
    let plain: Vec<CuttingRow> = draft(&mut h).cutting.iter().filter(|r| r.material.is_some() && !r.is_recipe()).copied().collect();
    let Some(brass) = plain.iter().position(|r| r.material == Some(WorkMaterial::Brass)) else {
        panic!("the draft has the Brass row");
    };
    let marks = |h: &H| -> Vec<egui::Rect> { library_window(h).query_all_by_label(MARK).map(|n| n.rect()).collect() };
    let removes = |h: &H| -> Vec<egui::Rect> { library_window(h).query_all_by_label("Remove").map(|n| n.rect()).collect() };
    let on_row = |m: egui::Rect, r: egui::Rect| m.center().y >= r.top() - 4.0 && m.center().y <= r.bottom() + 4.0;
    let (m, r) = (marks(&h), removes(&h));
    assert_eq!(m.len(), 1, "one row is marked");
    assert_eq!(r.len(), plain.len(), "each material row has its Remove button");
    assert!(on_row(m[0], r[brass]), "the mark {:?} stands on the Brass row {:?}", m[0], r[brass]);
    if h.state().state.units() == camcraft_app::units::Unit::Millimeters {
        let d = tool(&h, &name).diameter;
        let vc = format!("{:.0}", std::f64::consts::PI * d * 8000.0 / 1000.0);
        assert!(library_window(&h).query_all_by_label("Vc (m/min)").next().is_some(), "the table has the Vc column");
        assert!(library_window(&h).query_all_by_label(&vc).next().is_some(), "the Brass row shows Vc {vc}");
    }
    set_material(&mut h, Some(WorkMaterial::Hdpe));
    let m = marks(&h);
    assert_eq!(m.len(), 1, "with no HDPE row, the default row is marked");
    assert!(removes(&h).iter().all(|r| !on_row(m[0], *r)), "the default row has no Remove button");
    set_material(&mut h, None);
    assert!(marks(&h).is_empty(), "no material stated, no mark");
}
