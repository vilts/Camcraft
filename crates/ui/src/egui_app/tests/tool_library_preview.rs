//! #3894 (U3 of #3663): the preview of the tool library window's editor -
//! the orbitable 3D model is the default view (the imported body, or a
//! model revolved from the profile), the side view paints its dimension
//! labels, a landed drawn cutter shows in the two views and in the
//! Properties preview, and the model is built only when its input
//! changes (also the Properties cutter outline, #3914 review). Also the open U2 gap: "Edit tool shape" in the window opens
//! the shape window on the window's tool while Properties shows another.
//! Gate map: covers ui::egui_app::tool_library_dialog, ui::egui_app::tool_model_view, ui::egui_app::tool_preview, ui::egui_app::tool_profile_page, ui::egui_app::tool_preview_inputs, ui::state.
//! Provenance: our own structural tests; the test holder and the neck are
//! our own geometry. No HSMWorks material involved.

use camcraft_app::cutter_fit::{self, Neck};
use camcraft_app::units;
use egui::accesskit::Role;

use super::tool_body::{TOOL, prism_stl, shank_top, temp_stl};
use super::tool_library_editor::{H, click_window_button, draft, library_window, tool, two_tools};
use super::*;
use crate::egui_app::tool_library_dialog::{model as preview_model, preview};
use crate::egui_app::tool_preview_inputs as inputs;
use crate::state::PreviewMode;

/// Open `name` in the window's editor (Properties keeps its subject).
fn open(h: &mut H, name: &str) {
    h.state_mut().state.open_tool_in_library(name);
    h.run_steps(3);
}

fn face(h: &H) -> crate::state::EditorFace {
    match h.state().state.tool_library.as_ref().and_then(|v| v.editor.as_ref()) {
        Some(e) => e.face,
        None => panic!("no tool is open in the tool library window"),
    }
}

fn last() -> std::rc::Rc<preview_model::ToolModel> {
    preview_model::last().unwrap_or_else(|| panic!("the preview built no model"))
}

fn has_label(h: &H, label: &str) -> bool {
    library_window(h).query_all_by_label(label).next().is_some()
}

/// The largest radius of the mesh vertices below height `z`.
fn r_below(m: &preview_model::ToolModel, z: f64) -> f64 {
    m.mesh
        .iter()
        .flat_map(|t| [t.a, t.b, t.c])
        .filter(|p| p.z < z)
        .map(|p| p.x.hypot(p.y))
        .fold(0.0, f64::max)
}

/// Land the necked drawn cutter on `TOOL`; gives its landed outline.
fn land_neck(h: &mut H) -> Vec<(f64, f64)> {
    let t = tool(h, TOOL);
    let points = cutter_fit::necked(&t, Neck { d: 5.0, len: 10.0 }).unwrap_or_else(|e| panic!("the neck fits: {e:?}"));
    let cmd = CommandKind::SetToolCutter { name: TOOL.into(), points: Some(points) };
    h.state_mut().state.pending.push_back(cmd);
    h.run_steps(2);
    let st = &h.state().state;
    let e = model::find_by_name(&st.world, TOOL).expect("tool");
    match camcraft_app::tool_cutter::cutter_of(&st.world, &e) {
        Some(c) => c.outline(),
        None => panic!("the cutter landed: {}", st.status),
    }
}

#[test]
fn the_3d_view_is_the_default_and_shows_an_imported_body() {
    let mut h = super::tool_body::editor();
    let path = temp_stl("u3-body");
    prism_stl(&path, 15.0, 0.0, 30.0, 24);
    let top = shank_top(&h.state().state);
    {
        let st = &mut h.state_mut().state;
        st.import_tool_body_from(&path);
        st.tool_body_ui.offset = top + 5.0;
        st.queue_apply_tool_body();
    }
    h.run_steps(2);
    let body = {
        let st = &h.state().state;
        let e = model::find_by_name(&st.world, TOOL).expect("tool");
        camcraft_app::tool_body::body_of(&st.world, &e).unwrap_or_else(|| panic!("the body landed: {}", st.status))
    };
    open(&mut h, TOOL);
    assert_eq!(face(&h).preview, PreviewMode::Model, "the 3D view is the default");
    assert!(has_label(&h, "3D") && has_label(&h, "Side"), "the toggle offers the two views");
    assert!(has_label(&h, "Model: the tool with its body holder.stl"), "the caption names the body");
    let m = last();
    let n = body.facets.len();
    assert!(m.mesh.len() > n, "the steel is revolved under the body");
    assert_eq!(&m.mesh[m.mesh.len() - n..], &body.facets[..], "the 3D view shows the body's own facets");
    assert!(preview::projects() > 0, "the 3D view is painted");
    let _ = std::fs::remove_dir_all(path.parent().unwrap());
}

#[test]
fn a_tool_without_a_body_shows_a_revolved_model() {
    let mut h = shell_harness();
    h.run_steps(2);
    let has_body = {
        let st = &h.state().state;
        model::find_by_name(&st.world, TOOL).and_then(|e| camcraft_app::tool_body::body_of(&st.world, &e)).is_some()
    };
    if has_body {
        h.state_mut().state.pending.push_back(CommandKind::SetToolBody { name: TOOL.into(), body: None });
        h.run_steps(2);
    }
    open(&mut h, TOOL);
    assert_eq!(face(&h).preview, PreviewMode::Model);
    assert!(has_label(&h, "Model: revolved from the tool profile"), "the caption says the model is revolved");
    let (t, m) = (tool(&h, TOOL), last());
    assert!(m.body.is_none() && !m.mesh.is_empty(), "the 3D view is never empty");
    let r = [t.diameter, t.shank_d, t.holder_d].into_iter().fold(0.0, f64::max) * 0.5;
    assert!((r_below(&m, f64::INFINITY) - r).abs() < 1e-6, "the model is as wide as the tool and its holder");
    assert!((r_below(&m, t.flute_len * 0.5) - t.diameter * 0.5).abs() < 1e-6, "the cutter is the tool's diameter");
    let z = m.mesh.iter().flat_map(|q| [q.a.z, q.b.z, q.c.z]).fold(0.0, f64::max);
    assert!((z - (t.flute_len + t.shank_len + t.holder_len)).abs() < 1e-6, "the holder cylinder stands on the shank");
}

#[test]
fn a_landed_drawn_cutter_shows_in_both_views() {
    let mut h = shell_harness();
    h.run_steps(2);
    let landed = land_neck(&mut h);
    open(&mut h, TOOL);
    assert_eq!(last().steel, landed, "the 3D model revolves the landed cutter");
    let neck = last().mesh.iter().flat_map(|q| [q.a, q.b, q.c]).any(|p| (p.x.hypot(p.y) - 2.5).abs() < 1e-6);
    assert!(neck, "the 3D model has the Ø 5 neck");
    click_window_button(&mut h, "Side");
    assert_eq!(face(&h).preview, PreviewMode::Side);
    assert_eq!(last().steel, landed, "the side view draws the landed cutter");
    // The Properties preview shows it too, outside "Draw cutter" (#93).
    crate::actions::target(&mut h.state_mut().state, UiTarget::Tool(TOOL.into()));
    h.run_steps(3);
    let (_, steel) = super::super::tool_preview_inputs::preview_inputs(&mut h.state_mut().state, true);
    assert_eq!(steel, Some(landed), "the Properties preview draws the landed cutter");
}

#[test]
fn the_side_view_paints_the_dimension_labels() {
    let mut h = shell_harness();
    h.run_steps(2);
    open(&mut h, TOOL);
    click_window_button(&mut h, "Side");
    h.run_steps(1);
    let texts: Vec<String> = super::structural::painted(&h).into_iter().map(|(t, _)| t).collect();
    let (t, u, d) = (tool(&h, TOOL), h.state().state.units(), last().dims);
    let len = |v: f64| units::len(v, u, 2);
    let mut want = vec![
        format!("Ø {}", len(t.diameter)),
        format!("flute {}", len(t.flute_len)),
        format!("shank Ø {} × {}", len(t.shank_d), len(t.shank_len)),
        format!("stick-out {}", len(d.stick_out)),
        format!("dimensions in {}", u.suffix()),
    ];
    if let Some((hd, hl)) = d.holder {
        want.push(format!("gauge {}", len(d.gauge)));
        want.push(format!("holder Ø {} × {}", len(hd), len(hl)));
    }
    for w in &want {
        assert!(texts.contains(w), "the side view paints {w:?}: {texts:?}");
    }
    assert!(d.holder.is_some(), "the seeded tool has a holder: {d:?}");
}

#[test]
fn a_still_frame_computes_no_mesh() {
    let mut h = shell_harness();
    h.run_steps(2);
    open(&mut h, TOOL);
    let (b, p) = (preview_model::builds(), preview::projects());
    h.run_steps(3);
    assert_eq!(preview_model::builds() - b, 0, "a still frame builds no model");
    assert_eq!(preview::projects() - p, 0, "a still frame projects no mesh");
}

#[test]
fn a_changed_tool_rebuilds_the_mesh() {
    let mut h = shell_harness();
    h.run_steps(2);
    open(&mut h, TOOL);
    let (m0, b) = (last(), preview_model::builds());
    let d0 = draft(&mut h).diameter;
    draft(&mut h).diameter = d0 + 1.0;
    h.run_steps(2);
    assert!(preview_model::builds() > b, "a draft edit builds the model again");
    let m1 = last();
    assert!((r_below(&m1, m1.tool.flute_len * 0.5) - (d0 + 1.0) * 0.5).abs() < 1e-6, "the mesh follows the draft");
    let b1 = preview_model::builds();
    h.run_steps(2);
    assert_eq!(preview_model::builds(), b1, "and then holds");
    draft(&mut h).diameter = d0;
    h.run_steps(2);
    let m2 = last();
    assert_eq!((m2.steel.clone(), m2.mesh.len(), m2.dims), (m0.steel.clone(), m0.mesh.len(), m0.dims), "the old input gives the old model");
}

/// Properties on `TOOL` with its landed neck cutter, settled.
fn properties_on_neck(h: &mut H) -> Vec<(f64, f64)> {
    let landed = land_neck(h);
    crate::actions::target(&mut h.state_mut().state, UiTarget::Tool(TOOL.into()));
    h.run_steps(3);
    landed
}

/// #3914 review (Frame cost): with a landed cutter, a still Properties
/// frame checks no cutter and flattens no outline.
#[test]
fn properties_still_frame_computes_no_cutter_outline() {
    let mut h = shell_harness();
    h.run_steps(2);
    properties_on_neck(&mut h);
    let c = inputs::checks();
    h.run_steps(3);
    assert_eq!(inputs::checks() - c, 0, "a still Properties frame computes no cutter outline");
}

/// #3914 review (Frame cost): a changed draft tool value computes the
/// cutter outline once, and the old outline no longer shows.
#[test]
fn properties_changed_tool_computes_the_cutter_outline_once() {
    let mut h = shell_harness();
    h.run_steps(2);
    let landed = properties_on_neck(&mut h);
    let c = inputs::checks();
    let d0 = h.state().state.tool_ui.diameter;
    h.state_mut().state.tool_ui.diameter = d0 + 1.0;
    h.run_steps(3);
    assert_eq!(inputs::checks() - c, 1, "a changed draft computes the cutter outline once");
    let (_, steel) = inputs::preview_inputs(&mut h.state_mut().state, true);
    assert_ne!(steel, Some(landed), "the outline follows the draft, not the landed numbers");
}

/// The U2 gap: Properties shows tool A and the window tool B. "Edit tool
/// shape" in the window opens the shape window on B, and Properties
/// follows B (the shape window edits through Properties' draft).
#[test]
fn edit_tool_shape_in_the_window_opens_its_tool_while_properties_shows_another() {
    let mut h = shell_harness();
    h.run_steps(2);
    let (a, b) = two_tools(&mut h);
    crate::actions::target(&mut h.state_mut().state, UiTarget::Tool(a.clone()));
    h.run_steps(3);
    open(&mut h, &b);
    library_window(&h).get_by_role_and_label(Role::Button, "Edit tool shape").click_accesskit();
    h.run_steps(3);
    let st = &h.state().state;
    assert_eq!(st.tool_shape_editor.as_deref(), Some(b.as_str()), "the shape window opens on the window's tool");
    assert_eq!(st.ui_target, UiTarget::Tool(b.clone()), "Properties follows the window's tool");
    assert_eq!(st.tool_ui.name, b, "the shape window's draft is tool {b}");
    h.run_steps(3);
    assert_eq!(h.state().state.tool_shape_editor.as_deref(), Some(b.as_str()), "and stays there");
    assert_eq!(draft(&mut h).name, b, "the window keeps its tool");
}

/// #3908: open `TOOL` with `n` flutes and the helix `helix` in the
/// window's draft; gives the model built from it.
fn fluted_draft(h: &mut H, n: Option<u32>, helix: &str) -> std::rc::Rc<preview_model::ToolModel> {
    let d = draft(h);
    d.flutes = n;
    d.helix_deg = helix.to_owned();
    h.run_steps(2);
    last()
}

/// #3908: the 3D view shows the helical flutes by default; the Flutes
/// toggle (view state, no command) gives the plain revolved model back.
#[test]
fn the_flutes_toggle_shows_and_hides_the_helical_flutes() {
    let mut h = shell_harness();
    h.run_steps(2);
    open(&mut h, TOOL);
    let m = fluted_draft(&mut h, Some(4), "30");
    assert!(face(&h).flutes, "the Flutes toggle starts on");
    let (Some(all), Some(bare)) = (m.fluted.clone(), m.fluted_bare.clone()) else {
        panic!("a 4-flute end mill has a fluted model");
    };
    assert!(std::sync::Arc::ptr_eq(m.shown(true, true).0, &all), "the 3D view paints the fluted model");
    assert!(std::sync::Arc::ptr_eq(m.shown(false, true).0, &bare), "also without the holder");
    assert_eq!(&all[all.len() - (m.mesh.len() - m.bare.len())..], &m.mesh[m.bare.len()..], "the holder is the same");
    let r = |p: &camcraft_app::geometry::Point3| p.x.hypot(p.y);
    let z = m.tool.flute_len * 0.5;
    let deep = bare.iter().flat_map(|t| [t.a, t.b, t.c]).filter(|p| (p.z - z).abs() < m.tool.flute_len * 0.05).map(|p| r(&p));
    assert!(deep.fold(f64::INFINITY, f64::min) < m.tool.diameter * 0.5 * 0.7, "grooves cut into the fluted length");
    let (done, p) = (h.state().state.history.done_len(), preview::projects());
    library_window(&h).get_by_label(preview::FLUTES).click();
    h.run_steps(2);
    assert!(!face(&h).flutes, "the Flutes toggle is off");
    assert!(preview::projects() > p, "the toggle projects the plain model");
    let p1 = preview::projects();
    h.run_steps(3);
    assert_eq!(preview::projects(), p1, "and then projects nothing");
    assert!(std::sync::Arc::ptr_eq(m.shown(true, false).0, &m.mesh), "the plain revolved model");
    let st = &h.state().state;
    assert_eq!(st.history.done_len(), done, "a toggle lands no command");
    assert!(st.pending.is_empty(), "and queues none");
}

/// #3908 safety rule: the flutes are display only. The steel outline,
/// the holder bands the collision check sweeps, the plain mesh and the
/// tool's envelope (`Tool::geom`, `Tool::profile`) are the same with
/// and without flutes, whatever the count and the helix.
#[test]
fn the_flutes_do_not_change_what_the_checks_read() {
    let mut h = shell_harness();
    h.run_steps(2);
    open(&mut h, TOOL);
    let plain = fluted_draft(&mut h, None, "");
    let fluted = fluted_draft(&mut h, Some(3), "45");
    assert!(plain.fluted.is_none() && fluted.fluted.is_some(), "only the stated count draws flutes");
    assert_eq!(fluted.steel, plain.steel, "the steel outline");
    assert_eq!(fluted.bands, plain.bands, "the holder bands of the collision check");
    assert_eq!(&fluted.mesh[..], &plain.mesh[..], "the plain mesh");
    assert_eq!((fluted.bounds, fluted.dims), (plain.bounds, plain.dims), "the box and the dimensions");
    assert_eq!(fluted.tool.geom(), plain.tool.geom(), "the envelope the laws read");
    assert_eq!(fluted.tool.profile(), plain.tool.profile(), "the profile the laws read");
}

/// #3908 (Frame cost): a still frame with the flutes on builds and
/// projects nothing; a changed helix builds the model once.
#[test]
fn a_changed_helix_rebuilds_the_fluted_model_once() {
    let mut h = shell_harness();
    h.run_steps(2);
    open(&mut h, TOOL);
    let m0 = fluted_draft(&mut h, Some(4), "30");
    let (b, p) = (preview_model::builds(), preview::projects());
    h.run_steps(3);
    assert_eq!((preview_model::builds() - b, preview::projects() - p), (0, 0), "a still frame computes no mesh");
    let m1 = fluted_draft(&mut h, Some(4), "0");
    assert_eq!(preview_model::builds() - b, 1, "a changed helix builds the model once");
    assert_ne!(m1.fluted, m0.fluted, "the grooves follow the helix");
    let b1 = preview_model::builds();
    h.run_steps(3);
    assert_eq!(preview_model::builds(), b1, "and then holds");
}
