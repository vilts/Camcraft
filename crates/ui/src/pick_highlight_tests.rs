//! Pins for the input key of `pick_highlight` (#2201): a still frame
//! builds nothing, and each changed input (pose, facets, display-off,
//! the pick itself) builds again. crates/ui/AGENTS.md, "Frame cost".
//! Provenance: our own unit tests; no vendor material consulted.

use std::sync::Arc;

use camcraft_app::geometry::{Point3, cube};
use camcraft_app::model;
use camcraft_app::operation::{FeatureSel, GeoSel};
use camcraft_app::schema::Location;

use super::{hover_builds, resolved_hover, resolved_row_hover, row_hover_builds};
use crate::state::State;

/// A state with one cube model "Part" and its first face row-hovered.
fn hovered_cube() -> State {
    let mut st = State::new();
    let part = model::spawn(&st.world, "Part");
    model::set_facets(&st.world, &part, cube(Point3::new(0.0, 0.0, 0.0), Point3::new(10.0, 10.0, 10.0)).facets);
    let features = model::features_of(&st.world, &part).expect("cube features");
    let fid = features.faces.first().expect("a face").id;
    st.geo_hover_drawn = Some(GeoSel { entity: "Part".into(), feature: FeatureSel::Face(fid) });
    st
}

fn max_x(r: &super::ResolvedHighlights) -> f64 {
    r.polylines.iter().flat_map(|p| p.pts.iter()).map(|p| p[0]).fold(f64::NEG_INFINITY, f64::max)
}

#[test]
fn highlight_still_frames_build_the_outline_once() {
    let st = hovered_cube();
    let first = resolved_row_hover(&st);
    assert!(!first.polylines.is_empty(), "the hovered face draws an outline");
    for _ in 0..16 {
        let again = resolved_row_hover(&st);
        assert!(Arc::ptr_eq(&first, &again), "a still frame reads the cached outline");
    }
    assert_eq!(row_hover_builds(&st), 1, "a still frame builds nothing");
    assert_eq!(hover_builds(&st), 0, "the row hover does not count as a viewport hover");
}

/// The viewport hover slot (`state.hover`) has its own pin: a still
/// frame builds once, a new hovered feature builds again, and the
/// row-hover count does not move.
#[test]
fn viewport_hover_still_frames_build_once_and_a_new_feature_builds_again() {
    let mut st = hovered_cube();
    st.geo_hover_drawn = None;
    let part = model::find_by_name(&st.world, "Part").expect("Part");
    let features = model::features_of(&st.world, &part).expect("cube features");
    let first = features.faces.first().expect("a face").id;
    let second = features.faces.get(1).expect("a second face").id;
    st.hover = Some(crate::state::HoverHit { entity: "Part".into(), facet: 0, feature: Some(FeatureSel::Face(first)) });
    let a = resolved_hover(&st);
    assert!(!a.polylines.is_empty(), "the hovered face draws an outline");
    for _ in 0..16 {
        assert!(Arc::ptr_eq(&a, &resolved_hover(&st)), "a still frame reads the cached outline");
    }
    assert_eq!(hover_builds(&st), 1, "a still frame builds nothing");
    st.hover = Some(crate::state::HoverHit { entity: "Part".into(), facet: 1, feature: Some(FeatureSel::Face(second)) });
    let b = resolved_hover(&st);
    assert_eq!(hover_builds(&st), 2, "a new hovered feature builds again");
    assert!(!Arc::ptr_eq(&a, &b));
    assert_eq!(row_hover_builds(&st), 0, "the viewport hover does not count as a row hover");
}

#[test]
fn highlight_rebuilds_when_the_pose_moves() {
    let st = hovered_cube();
    let before = resolved_row_hover(&st);
    let part = model::find_by_name(&st.world, "Part").expect("Part");
    let loc = Location { pos: Point3::new(50.0, 0.0, 0.0), ..Location::default() };
    model::set_transform(&st.world, &part, loc);
    let after = resolved_row_hover(&st);
    assert_eq!(row_hover_builds(&st), 2, "a moved body builds again");
    assert!((max_x(&after) - max_x(&before) - 50.0).abs() < 1e-9, "the outline moves with the body");
}

#[test]
fn highlight_rebuilds_when_the_facets_are_replaced() {
    let st = hovered_cube();
    let before = resolved_row_hover(&st);
    assert!(!before.polylines.is_empty());
    let part = model::find_by_name(&st.world, "Part").expect("Part");
    model::set_facets(&st.world, &part, cube(Point3::new(0.0, 0.0, 0.0), Point3::new(30.0, 7.0, 3.0)).facets);
    let after = resolved_row_hover(&st);
    assert_eq!(row_hover_builds(&st), 2, "a re-meshed body builds again");
    assert!(!Arc::ptr_eq(&before, &after), "the old outline is never served for the new mesh");
}

#[test]
fn highlight_rebuilds_when_the_body_is_hidden_or_the_pick_changes() {
    let mut st = hovered_cube();
    assert!(!resolved_row_hover(&st).polylines.is_empty());
    st.hidden_models.push("Part".into());
    assert!(resolved_row_hover(&st).polylines.is_empty(), "a hidden body draws nothing");
    assert_eq!(row_hover_builds(&st), 2);
    st.hidden_models.clear();
    let part = model::find_by_name(&st.world, "Part").expect("Part");
    let features = model::features_of(&st.world, &part).expect("cube features");
    let other = features.faces.get(1).expect("a second face").id;
    st.geo_hover_drawn = Some(GeoSel { entity: "Part".into(), feature: FeatureSel::Face(other) });
    assert!(!resolved_row_hover(&st).polylines.is_empty());
    assert_eq!(row_hover_builds(&st), 3, "a new pick builds again");
}

fn hovered_sketch() -> State {
    use camcraft_app::commands::{DxfOptions, ImportDxf, SetStock};
    use camcraft_app::stock::StockShape;

    let mut st = State::new();
    let stock = SetStock::new(
        &st.world,
        StockShape::Box {
            dx: 120.0,
            dy: 120.0,
            dz: 10.0,
        },
        Location::default(),
    )
    .unwrap();
    st.history.execute(&st.world, stock);
    let bytes = b"0\nSECTION\n2\nHEADER\n9\n$INSUNITS\n70\n4\n0\nENDSEC\n0\nSECTION\n2\nENTITIES\n0\nCIRCLE\n8\ncut\n10\n50\n20\n50\n40\n8\n0\nENDSEC\n0\nEOF\n";
    let import = ImportDxf::from_bytes(
        &st.world,
        None,
        "profile.dxf",
        bytes,
        &DxfOptions::default(),
    )
    .unwrap();
    st.history.execute(&st.world, import);
    st.geo_hover_drawn = Some(GeoSel {
        entity: "profile".into(),
        feature: FeatureSel::SketchLayer("cut".into()),
    });
    st
}

#[test]
fn sketch_highlight_follows_stock_pose_and_height_with_undo() {
    use camcraft_app::commands::SetStock;
    use camcraft_app::stock::StockShape;

    let mut st = hovered_sketch();
    let initial = resolved_row_hover(&st);
    assert!(!initial.polylines.is_empty());
    let initial_point = initial.polylines[0].pts[0];
    for (height, pos) in [
        (10.0, Point3::new(30.0, 4.0, 2.0)),
        (25.0, Point3::new(0.0, 0.0, 0.0)),
    ] {
        let command = SetStock::new(
            &st.world,
            StockShape::Box {
                dx: 120.0,
                dy: 120.0,
                dz: height,
            },
            Location {
                pos,
                ..Location::default()
            },
        )
        .unwrap();
        st.history.execute(&st.world, command);
        let moved = resolved_row_hover(&st);
        assert!(
            !Arc::ptr_eq(&initial, &moved),
            "changed stock must rebuild the sketch highlight"
        );
        let actual = moved.polylines[0].pts[0];
        let expected = [
            initial_point[0] + pos.x,
            initial_point[1] + pos.y,
            initial_point[2] + pos.z + height - 10.0,
        ];
        for axis in 0..3 {
            assert!(
                (actual[axis] - expected[axis]).abs() < 1e-9,
                "placed highlight axis {axis}: {actual:?}, expected {expected:?}"
            );
        }
        assert!(
            Arc::ptr_eq(&moved, &resolved_row_hover(&st)),
            "still frames reuse the changed highlight"
        );
        st.history.undo(&st.world);
        let restored = resolved_row_hover(&st);
        assert_eq!(
            restored.polylines[0].pts, initial.polylines[0].pts,
            "undo restores the placed highlight"
        );
        st.history.redo(&st.world);
        assert_eq!(
            resolved_row_hover(&st).polylines[0].pts,
            moved.polylines[0].pts,
            "redo restores the changed highlight"
        );
        st.history.undo(&st.world);
        let _ = resolved_row_hover(&st);
    }
    assert_eq!(
        row_hover_builds(&st),
        9,
        "each changed input builds once; still frames build nothing"
    );
}

#[test]
fn sketch_highlight_disappears_without_stock_and_returns_on_undo() {
    use camcraft_app::commands::DeleteEntity;

    let mut st = hovered_sketch();
    let before = resolved_row_hover(&st);
    assert!(!before.polylines.is_empty());
    let delete = DeleteEntity::new(&st.world, "Stock").unwrap();
    st.history.execute(&st.world, delete);
    let removed = resolved_row_hover(&st);
    assert!(
        removed.polylines.is_empty(),
        "an unplaced sketch has no highlight"
    );
    assert!(
        Arc::ptr_eq(&removed, &resolved_row_hover(&st)),
        "stock absence is a stable cache input"
    );
    st.history.undo(&st.world);
    let restored = resolved_row_hover(&st);
    assert_eq!(restored.polylines[0].pts, before.polylines[0].pts);
    st.history.redo(&st.world);
    assert!(resolved_row_hover(&st).polylines.is_empty());
    assert_eq!(row_hover_builds(&st), 4);
}

/// #1864 review: a placed DXF sketch rides the stock top. The hover and
/// selected highlights and the violet pick lines all key on the sketch's
/// stock-relative world pose, so a stock height change or a stock move
/// draws the picked chain where it now is - never at the old Z.
#[test]
fn sketch_highlights_follow_stock_height_and_stock_move() {
    use camcraft_app::commands::{DxfOptions, ImportDxf};
    use camcraft_app::operation::{OpType, SrcRef, seed_params};
    use camcraft_app::stock::StockShape;
    use camcraft_app::tools::catalog;

    use crate::state::CommandKind;

    const TOOL: &str = "EM-0400-2FL";
    const DXF: &str = concat!(
        "0\nSECTION\n2\nHEADER\n9\n$INSUNITS\n70\n4\n0\nENDSEC\n0\nSECTION\n2\nENTITIES\n",
        "0\nCIRCLE\n8\nholes\n10\n25\n20\n55\n40\n8\n0\nENDSEC\n0\nEOF\n",
    );

    let set_stock = |st: &mut State, dz: f64, pos: [f64; 3]| {
        st.pending.push_back(CommandKind::SetStock { shape: StockShape::Box { dx: 300.0, dy: 240.0, dz }, pos, yaw_deg: 0.0 });
        st.drain_actions();
    };
    let mut st = State::new();
    set_stock(&mut st, 10.0, [0.0, 0.0, 0.0]);
    let tool = catalog().into_iter().find(|(n, _)| *n == TOOL).map(|(_, t)| t).expect("catalog");
    st.pending.push_back(CommandKind::AddTool { name: TOOL.into(), tool });
    st.drain_actions();
    let cmd = ImportDxf::from_bytes(&st.world, None, "plate.dxf", DXF.as_bytes(), &DxfOptions::default()).expect("import dxf");
    st.history.execute(&st.world, cmd);
    let pick = GeoSel { entity: "plate".into(), feature: FeatureSel::SketchLayer("holes".into()) };
    st.pending.push_back(CommandKind::CreateOperation {
        name: "Contour1".into(),
        tool: TOOL.into(),
        src: SrcRef::Selection(vec![pick.clone()]),
        params: seed_params(OpType::Contour.ordinal()),
    });
    st.pending.push_back(CommandKind::Select(Some("Contour1".into())));
    st.drain_actions();
    st.sync_ops_ui();
    st.hover = Some(crate::state::HoverHit { entity: "plate".into(), facet: 0, feature: Some(pick.feature.clone()) });

    let zs = |r: &super::ResolvedHighlights| -> Vec<f64> { r.polylines.iter().flat_map(|p| p.pts.iter()).map(|p| p[2]).collect() };
    let line_zs = |st: &State| -> Vec<f64> {
        crate::interact::selection_feature_lines(st).iter().map(|v| f64::from(v[2])).collect()
    };
    let line_max_x = |st: &State| -> f64 {
        crate::interact::selection_feature_lines(st).iter().map(|v| f64::from(v[0])).fold(f64::NEG_INFINITY, f64::max)
    };
    let all_near = |v: &[f64], want: f64| !v.is_empty() && v.iter().all(|z| (z - want).abs() < 1e-3);

    // Warm all three caches at the first stock height.
    assert!(all_near(&zs(&resolved_hover(&st)), 10.0), "hover at Z=10");
    assert!(all_near(&zs(&super::resolved_selected(&st)), 10.0), "selected at Z=10");
    assert!(all_near(&line_zs(&st), 10.0), "pick lines at Z=10");
    let hover_x0 = max_x(&resolved_hover(&st));
    let lines_x0 = line_max_x(&st);

    // Stock height 10 -> 25: the sketch rides up, and so does each overlay.
    set_stock(&mut st, 25.0, [0.0, 0.0, 0.0]);
    assert!(all_near(&zs(&resolved_hover(&st)), 25.0), "hover follows the stock height: {:?}", zs(&resolved_hover(&st)));
    assert!(all_near(&zs(&super::resolved_selected(&st)), 25.0), "selected follows the stock height");
    assert!(all_near(&line_zs(&st), 25.0), "pick lines follow the stock height: {:?}", line_zs(&st));

    // Stock move +40 in X: the sketch moves with it.
    set_stock(&mut st, 25.0, [40.0, 0.0, 0.0]);
    assert!((max_x(&resolved_hover(&st)) - (hover_x0 + 40.0)).abs() < 1e-3, "hover follows the stock move");
    assert!((max_x(&super::resolved_selected(&st)) - (hover_x0 + 40.0)).abs() < 1e-3, "selected follows the stock move");
    let lines_x1 = line_max_x(&st);
    assert!((lines_x1 - (lines_x0 + 40.0)).abs() < 1e-3, "pick lines follow the stock move: {lines_x0} -> {lines_x1}");
}
