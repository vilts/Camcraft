//! #554's frame test: with the Setups page's plane row armed, a real
//! viewport click a few pixels inside a face, right beside its edge (well
//! within the edge-snap radius), takes the FACE - the row picks faces
//! only. The same click with an axis row armed (#557) takes the edge,
//! because an edge does set a direction.
//! Provenance: our own pick law test over our own shell; no vendor
//! material consulted.

use camcraft_app::pick::InPlaneAxis;

use super::*;

/// A pixel 4 px inside the most camera-facing side face of the seed's
/// 60 x 60 x 30 block, toward its centre from its top edge's midpoint.
fn beside_an_edge(sh: &ShellUi) -> egui::Pos2 {
    let ([x0, y0], [x1, y1]) = sh.state.viewport_rect;
    let (w, hgt) = (x1 - x0, y1 - y0);
    let mvp = sh.state.cam.mvp(f64::from(w / hgt));
    let eye = sh.state.cam.eye_f32();
    let sides = [
        ([60.0, 30.0, 15.0], [1.0, 0.0, 0.0]),
        ([0.0, 30.0, 15.0], [-1.0, 0.0, 0.0]),
        ([30.0, 60.0, 15.0], [0.0, 1.0, 0.0]),
        ([30.0, 0.0, 15.0], [0.0, -1.0, 0.0]),
    ];
    let px = |p: [f64; 3]| interact::project_uv(&mvp, p).map(|uv| egui::pos2(x0 + uv[0] * w, y0 + uv[1] * hgt));
    let mut best: Option<(f32, egui::Pos2)> = None;
    for (c, n) in sides {
        let facing = ((f64::from(eye[0]) - c[0]) * n[0]
            + (f64::from(eye[1]) - c[1]) * n[1]
            + (f64::from(eye[2]) - c[2]) * n[2]) as f32;
        let (Some(pc), Some(pm)) = (px(c), px([c[0], c[1], 30.0])) else { continue };
        let d = pc - pm;
        if facing <= 0.0 || d.length() < 20.0 {
            continue;
        }
        let p = pm + d.normalized() * 4.0;
        if best.is_none_or(|(bf, _)| facing > bf) {
            best = Some((facing, p));
        }
    }
    best.expect("the block shows a side face to the default camera").1
}

#[test]
fn a_click_beside_an_edge_picks_the_face_for_the_plane_and_the_edge_for_an_axis() {
    let mut harness = shell_harness();
    // #664: a pick collects only while its setup page is shown.
    harness.state_mut().state.pending.push_back(CommandKind::CreateSetup { name: None, datum_from: None });
    harness.run();
    crate::actions::target(
        &mut harness.state_mut().state,
        UiTarget::SetupNode("Setup1".into(), crate::state::SetupPage::Settings),
    );
    harness.run();
    let p = beside_an_edge(harness.state());
    assert!(
        crate::feature_click::feature_at(
            &harness.state().state,
            [p.x, p.y],
            crate::feature_click::Ask {
                kinds: camcraft_app::pick::PickKinds::EDGES,
                unit: camcraft_app::pick::EdgeUnit::Chain
            },
        )
        .is_some(),
        "the pixel is inside the edge-snap radius - an Edge-mode click there would take the edge"
    );
    crate::plane_pick::arm(&mut harness.state_mut().state, "Setup1");
    assert_eq!(harness.state().state.geo_pick, GeoPick::Face, "the plane row arms faces only");
    click_viewport(&mut harness, p);
    let picks = harness.state().state.plane_pick.as_ref().map(|d| d.picks.clone()).unwrap_or_default();
    assert_eq!(picks.len(), 1, "one pick drafted: {picks:?} ({})", harness.state().state.status);
    assert!(matches!(picks[0].feature, FeatureSel::Face(_)), "the face, not the edge: {picks:?}");
    assert!(harness.state().state.op_ui.geo.len() <= 1, "nothing joins an operation");

    // The axis row takes the edge at the same pixel.
    crate::axis_pick::arm(&mut harness.state_mut().state, "Setup1", InPlaneAxis::Y);
    assert!(harness.state().state.plane_pick.is_none(), "one draft collects at a time");
    assert_eq!(harness.state().state.geo_pick, GeoPick::Edge, "an axis by edge picks edges");
    click_viewport(&mut harness, p);
    let d = harness.state().state.axis_pick.clone().expect("the axis draft");
    let y = d.wanted().y.map(|a| a.picks).unwrap_or_default();
    assert_eq!(y.len(), 1, "one edge for +Y: {y:?} ({})", harness.state().state.status);
    assert!(matches!(y[0].feature, FeatureSel::Chain(_)), "the edge: {y:?}");
}

/// #573 hand check: on a setup page with the axes rows open and no axis
/// armed, a real Edge-mode viewport click on the part says what to do -
/// never "open an operation first" - and nothing is picked anywhere.
/// Choosing "+Y by Two points" then arms vertex picking at once.
#[test]
fn a_stray_click_on_the_setup_page_names_the_axis_pick_and_a_mode_choice_arms() {
    let mut harness = shell_harness();
    harness.state_mut().state.pending.push_back(CommandKind::CreateSetup { name: None, datum_from: None });
    harness.run();
    crate::actions::target(
        &mut harness.state_mut().state,
        UiTarget::SetupNode("Setup1".into(), crate::state::SetupPage::Settings),
    );
    harness.run();
    assert!(harness.state().state.axis_rows_shown, "the setup page draws the axes rows");
    harness.state_mut().state.geo_pick = GeoPick::Edge;
    let geo_before = harness.state().state.op_ui.geo.clone();
    let p = beside_an_edge(harness.state());
    click_viewport(&mut harness, p);
    let status = harness.state().state.status.clone();
    assert!(status.contains("click Pick +Y edge or Pick +X edge first"), "{status}");
    assert!(!status.contains("open an operation"), "{status}");
    assert_eq!(harness.state().state.op_ui.geo, geo_before, "nothing joins an operation");
    assert!(!crate::axis_pick::collecting(&harness.state().state));

    harness.get_by_label("+Y by Two points").click();
    harness.run();
    assert!(crate::axis_pick::collecting(&harness.state().state), "choosing the mode arms it");
    assert_eq!(harness.state().state.geo_pick, GeoPick::Vertex);
    assert!(harness.query_by_label("Stop picking +Y").is_some());
}

/// Every painted shape of the last frame, groups flattened.
fn shapes(h: &Harness<'static, ShellUi>) -> Vec<egui::Shape> {
    fn walk(s: &egui::Shape, out: &mut Vec<egui::Shape>) {
        match s {
            egui::Shape::Vec(v) => v.iter().for_each(|s| walk(s, out)),
            other => out.push(other.clone()),
        }
    }
    let mut out = Vec::new();
    for c in &h.output().shapes {
        walk(&c.shape, &mut out);
    }
    out
}

/// #738: the alignment picks are painted so they can be seen - a picked
/// face filled with the pick violet, semi-transparent (every facet of it,
/// not only the outline); picked points as round filled discs; a picked
/// edge as a bold stroke, well over the 1 px hover outline.
#[test]
fn picked_faces_fill_points_are_discs_and_edges_are_bold() {
    use crate::align_pick_viz::{EDGE_PX, FACE_ALPHA, PICK_RGB8, POINT_ALPHA, POINT_RADIUS_PX};
    let mut harness = shell_harness();
    harness.state_mut().state.pending.push_back(CommandKind::CreateSetup { name: None, datum_from: None });
    harness.run();
    crate::actions::target(
        &mut harness.state_mut().state,
        UiTarget::SetupNode("Setup1".into(), crate::state::SetupPage::Settings),
    );
    harness.run();
    let f = {
        let w = &harness.state().state.world;
        let e = model::find_by_name(w, "Block").expect("the seed's Block");
        model::features_of(w, &e).expect("features")
    };
    let top = f.faces.iter().find(|x| x.normal[2] > 0.99).expect("the top face");
    let sel = |feature| camcraft_app::operation::GeoSel { entity: "Block".into(), feature };
    // #771: each method's row waits behind the plane dropdown.
    let method = |h: &mut Harness<'static, ShellUi>, from: &str, to: &str| {
        h.get_by(|n| n.role() == egui::accesskit::Role::ComboBox && n.value().as_deref() == Some(from)).click();
        h.run();
        h.get_by_label(to).click();
        h.run();
    };
    let [r, g, b] = PICK_RGB8;

    method(&mut harness, "Axes", "Face");
    crate::plane_pick::arm(&mut harness.state_mut().state, "Setup1");
    crate::plane_pick::take(&mut harness.state_mut().state, sel(FeatureSel::Face(top.id)));
    harness.run();
    let fill = egui::Color32::from_rgba_unmultiplied(r, g, b, FACE_ALPHA);
    let filled: usize = shapes(&harness)
        .iter()
        .filter_map(|s| match s {
            egui::Shape::Mesh(m) if m.vertices.iter().all(|v| v.color == fill) => Some(m.indices.len() / 3),
            _ => None,
        })
        .sum();
    assert_eq!(filled, top.facets.len(), "every facet of the face is filled with the pick violet");
    assert!(fill.a() > 0 && fill.a() < 255, "semi-transparent");
    assert!(harness.query_by_label_contains("1 face selected").is_some(), "the row counts the face");

    method(&mut harness, "Face", "Points");
    crate::points_pick::arm(&mut harness.state_mut().state, "Setup1");
    for vi in camcraft_app::geometry::feature::corner_verts(&f).into_iter().take(2) {
        let id = camcraft_app::geometry::feature::vertex_id(&f, vi).expect("a corner id");
        crate::points_pick::take(&mut harness.state_mut().state, sel(FeatureSel::Vertex(id)));
    }
    harness.run();
    let disc = egui::Color32::from_rgba_unmultiplied(r, g, b, POINT_ALPHA);
    let discs = shapes(&harness)
        .iter()
        .filter(|s| matches!(s, egui::Shape::Circle(c) if c.fill == disc && c.radius >= POINT_RADIUS_PX))
        .count();
    assert_eq!(discs, 2, "two picked points, two filled discs");
    assert!(harness.query_by_label_contains("2 points selected").is_some(), "the row counts the points");

    method(&mut harness, "Points", "Axes");
    crate::axis_pick::arm(&mut harness.state_mut().state, "Setup1", camcraft_app::pick::InPlaneAxis::Y);
    let edge = camcraft_app::geometry::feature::chains(&f).into_iter().find(|c| !c.closed).expect("a straight edge");
    crate::axis_pick::take(&mut harness.state_mut().state, sel(FeatureSel::Chain(edge.id)));
    harness.run();
    let ink = egui::Color32::from_rgb(r, g, b);
    let bold = shapes(&harness)
        .iter()
        .filter(|s| matches!(s, egui::Shape::LineSegment { stroke, .. } if stroke.color == ink && stroke.width >= EDGE_PX))
        .count();
    assert!(bold >= 1, "the picked edge is drawn bold");
    const { assert!(EDGE_PX >= 3.0, "bold is well over the 1 px hover outline") };
    assert!(harness.query_by_label_contains("edge on ").is_some(), "the row names the edge");
    assert!(harness.query_all_by_label_contains("0x").next().is_none(), "no internal id on the page");
}
