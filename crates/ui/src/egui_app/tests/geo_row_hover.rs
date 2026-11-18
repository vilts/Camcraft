//! #1507: hovering a Geometry-tab selection row - its label or its remove
//! button - lights that face / chain / vertex in the viewport, and leaving
//! clears it. Hover is ephemeral view state: no command, no undo.
//! Provenance: our own page law; no vendor material consulted.

use super::*;

fn hovered(h: &Harness<'static, ShellUi>) -> Option<camcraft_app::operation::GeoSel> {
    h.state().state.geo_hover.clone()
}

/// The seed's Contour1 carries the top face; add one chain and one vertex.
fn three_picks() -> Harness<'static, ShellUi> {
    // Wide enough that the pane's row and its remove button are on screen.
    let mut harness = Harness::builder().with_size(egui::vec2(2400.0, 1000.0)).build_ui_state(frame, ShellUi::seeded());
    harness.run_steps(2);
    let (chain, vert) = {
        let st = &harness.state().state;
        let block = model::find_by_name(&st.world, "Block").expect("seed has Block");
        let f = model::features_of(&st.world, &block).expect("features");
        let chain = camcraft_app::geometry::feature::chains(&f).into_iter().next().expect("an edge").id;
        let vid = camcraft_app::geometry::feature::vertex_ids(&f).into_iter().next();
        (chain, vid.expect("a vertex id"))
    };
    let ui = &mut harness.state_mut().state.op_ui;
    ui.geo.push(camcraft_app::operation::GeoSel { entity: "Block".into(), feature: FeatureSel::Chain(chain) });
    ui.geo.push(camcraft_app::operation::GeoSel { entity: "Block".into(), feature: FeatureSel::Vertex(vert) });
    harness.run_steps(2);
    harness
}

#[test]
fn hovering_a_selection_row_or_its_remove_button_sets_the_hovered_selection() {
    let mut harness = three_picks();
    let chips = harness.state().state.geo_chips();
    assert_eq!(chips.len(), 3, "face, chain and vertex rows");
    assert!(hovered(&harness).is_none(), "nothing hovered at rest");

    for (i, chip) in chips.iter().enumerate() {
        // The row's label.
        let at = harness.get_by_label(&chip.label).rect().center();
        harness.hover_at(at);
        harness.run_steps(2);
        assert_eq!(hovered(&harness), Some(chip.sel.clone()), "label of row {i}");
        // The viewport draw gets the segments (or the vertex marker).
        let (w, h) = {
            let ([x0, y0], [x1, y1]) = harness.state().state.viewport_rect;
            (x1 - x0, y1 - y0)
        };
        let segs = interact::row_hover_segments(&harness.state().state, (w / h) as f64);
        assert!(!segs.is_empty(), "row {i} reaches the draw list");

        // Away from the list: cleared.
        let away = super::anchors::chrome_point(&harness, &[]);
        harness.hover_at(away);
        harness.run_steps(2);
        assert!(hovered(&harness).is_none(), "leaving clears row {i}");
        assert!(
            interact::row_hover_segments(&harness.state().state, (w / h) as f64).is_empty(),
            "and the viewport stops drawing it ({i})"
        );
    }

    // The remove buttons, in row order (top to bottom).
    let mut xs: Vec<egui::Rect> = harness.get_all_by_label("×").map(|n| n.rect()).collect();
    xs.sort_by(|a, b| a.min.y.total_cmp(&b.min.y));
    assert_eq!(xs.len(), 3);
    for (i, r) in xs.iter().enumerate() {
        harness.hover_at(r.center());
        harness.run_steps(2);
        assert_eq!(hovered(&harness), Some(chips[i].sel.clone()), "remove button of row {i}");
        let away = super::anchors::chrome_point(&harness, &[]);
        harness.hover_at(away);
        harness.run_steps(2);
        assert!(hovered(&harness).is_none());
    }
    assert!(harness.state().state.pending.is_empty(), "hover files no command");
}

/// The row hover's style is a named constant set, strong and apart from the
/// thin viewport hover and the violet pick, in both themes (#1507).
#[test]
fn row_hover_style_is_strong_and_distinct_in_both_themes() {
    use super::super::ground_ink::{Overlay, overlay_ink};
    let (px, halo) = std::hint::black_box((interact::ROW_HOVER_PX, interact::ROW_HOVER_HALO_PX));
    assert!(px >= 3.0, "thicker than the 1 px viewport hover");
    assert!(halo > px, "a halo wider than the stroke");
    for light in [false, true] {
        let row = overlay_ink(Overlay::RowHover, light);
        for other in [Overlay::Hover, Overlay::Pick, Overlay::Cut, Overlay::Region, Overlay::Retract] {
            let o = overlay_ink(other, light);
            let d: f32 = (0..3).map(|i| (row[i] - o[i]).abs()).sum();
            assert!(d > 0.5, "row hover ink differs from {other:?} (light={light}): {d}");
        }
    }
}

/// #1519, the reverse of the above: the viewport pointer over a picked face
/// marks that pick's Geometry-tab row, and moving off clears the mark.
#[test]
fn viewport_hover_over_a_picked_face_marks_its_row() {
    let mut harness = three_picks();
    harness.state_mut().state.geo_pick = GeoPick::Face;
    harness.run_steps(2);
    let chips = harness.state().state.geo_chips();
    let face = chips[0].sel.clone();
    assert!(harness.state().state.geo_row_marked.is_empty(), "no row marked at rest");

    // The centre of the picked top face, projected to the pane.
    let ([x0, y0], [x1, y1]) = harness.state().state.viewport_rect;
    let (w, h) = (x1 - x0, y1 - y0);
    let mvp = harness.state().state.cam.mvp((w / h) as f64);
    let uv = interact::project_uv(&mvp, [30.0, 30.0, 30.0]).expect("the top face centre projects");
    set_pos(&mut harness, egui::pos2(x0 + uv[0] * w, y0 + uv[1] * h));
    harness.run_steps(2);
    let hit = harness.state().state.hover.clone().expect("the face lights under the pointer");
    assert_eq!(hit.feature, Some(face.feature), "the pointer is over the picked face");
    assert_eq!(harness.state().state.geo_row_marked, vec![face], "only the face's row is marked");
    assert!(harness.state().state.pending.is_empty(), "marking files no command");

    let away = super::anchors::chrome_point(&harness, &[]);
    set_pos(&mut harness, away);
    harness.run_steps(2);
    assert!(harness.state().state.geo_row_marked.is_empty(), "leaving the face clears the mark");
}
