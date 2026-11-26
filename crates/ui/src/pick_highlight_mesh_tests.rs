//! Pins for the screen-mesh cache of `pick_highlight_mesh` (#2201): each
//! of the three slots (selected, viewport hover, row hover) has a build
//! counter. A still frame tessellates nothing again; an orbit, a pane
//! resize and an ink change each tessellate again, and the painted
//! vertices move or change colour. crates/ui/AGENTS.md, "Frame cost".
//! Provenance: our own unit tests; no vendor material consulted.

use camcraft_app::geometry::{Point3, cube};
use camcraft_app::model;
use camcraft_app::operation::{FeatureSel, GeoSel};
use egui::{Color32, Pos2, Rect, Shape, pos2};

use super::{paint_hover, paint_row_hover, paint_selected};
use crate::pick_highlight::lock;
use crate::points_pick::PointsPick;
use crate::state::{HoverHit, State};

#[derive(Clone, Copy, Debug, PartialEq)]
enum Slot {
    Selected,
    Hover,
    RowHover,
}

const SLOTS: [Slot; 3] = [Slot::Selected, Slot::Hover, Slot::RowHover];
const INK: Color32 = Color32::from_rgb(40, 160, 255);
const OTHER_INK: Color32 = Color32::from_rgb(255, 120, 20);
const HALO: Color32 = Color32::from_rgb(10, 10, 10);
const PANE: Rect = Rect { min: Pos2 { x: 0.0, y: 0.0 }, max: Pos2 { x: 800.0, y: 600.0 } };
const SMALL_PANE: Rect = Rect { min: Pos2 { x: 100.0, y: 50.0 }, max: Pos2 { x: 500.0, y: 350.0 } };

/// A state with one cube model "Part" and its first face in `slot`.
fn picked_cube(slot: Slot) -> State {
    let mut st = State::new();
    let part = model::spawn(&st.world, "Part");
    model::set_facets(&st.world, &part, cube(Point3::new(0.0, 0.0, 0.0), Point3::new(10.0, 10.0, 10.0)).facets);
    let features = model::features_of(&st.world, &part).expect("cube features");
    let fid = features.faces.first().expect("a face").id;
    let sel = GeoSel { entity: "Part".into(), feature: FeatureSel::Face(fid) };
    match slot {
        Slot::Selected => st.points_pick = Some(PointsPick { setup: "Setup1".into(), picks: vec![sel] }),
        Slot::Hover => st.hover = Some(HoverHit { entity: "Part".into(), facet: 0, feature: Some(sel.feature) }),
        Slot::RowHover => st.geo_hover_drawn = Some(sel),
    }
    st
}

fn builds(st: &State, slot: Slot) -> u64 {
    let inner = lock(st);
    match slot {
        Slot::Selected => inner.selected_mesh.builds,
        Slot::Hover => inner.hover_mesh.builds,
        Slot::RowHover => inner.row_hover_mesh.builds,
    }
}

/// Run one egui frame that paints `slot` into `pane` with `ink`, and
/// return the painted mesh vertices (position and colour).
fn frame(ctx: &egui::Context, st: &State, slot: Slot, pane: Rect, ink: Color32) -> Vec<(Pos2, Color32)> {
    let input = egui::RawInput {
        screen_rect: Some(Rect::from_min_max(pos2(0.0, 0.0), pos2(1000.0, 800.0))),
        ..Default::default()
    };
    let mut out = ctx.run_ui(input, |ui| {
        let painter = ui.ctx().layer_painter(egui::LayerId::background());
        let aspect = f64::from(pane.width() / pane.height());
        let to_px = move |uv: [f32; 2]| {
            pos2(
                pane.min.x + (uv[0] + 1.0) * 0.5 * pane.width(),
                pane.min.y + (1.0 - uv[1]) * 0.5 * pane.height(),
            )
        };
        match slot {
            Slot::Selected => paint_selected(&painter, st, aspect, &to_px, 2.0, 4.0, ink, HALO),
            Slot::Hover => paint_hover(&painter, st, aspect, &to_px, 2.0, 4.0, ink, HALO),
            Slot::RowHover => paint_row_hover(&painter, st, aspect, &to_px, 2.0, 4.0, ink, HALO),
        }
    });
    out.textures_delta.clear();
    fn walk(shape: &Shape, verts: &mut Vec<(Pos2, Color32)>) {
        match shape {
            Shape::Vec(v) => v.iter().for_each(|s| walk(s, verts)),
            Shape::Mesh(m) => verts.extend(m.vertices.iter().map(|v| (v.pos, v.color))),
            _ => {}
        }
    }
    let mut verts = Vec::new();
    for clipped in &out.shapes {
        walk(&clipped.shape, &mut verts);
    }
    verts
}

#[test]
fn highlight_mesh_still_frames_tessellate_once_in_each_slot() {
    for slot in SLOTS {
        let st = picked_cube(slot);
        let ctx = egui::Context::default();
        let first = frame(&ctx, &st, slot, PANE, INK);
        assert!(!first.is_empty(), "{slot:?}: the pick paints a mesh");
        for i in 0..8 {
            let again = frame(&ctx, &st, slot, PANE, INK);
            assert_eq!(again, first, "{slot:?}: frame {i} paints the same mesh");
        }
        assert_eq!(builds(&st, slot), 1, "{slot:?}: a still frame tessellates nothing again");
        for other in SLOTS.into_iter().filter(|o| *o != slot) {
            assert_eq!(builds(&st, other), 0, "{slot:?}: the {other:?} slot does not build");
        }
    }
}

#[test]
fn highlight_mesh_rebuilds_after_an_orbit_and_the_vertices_move() {
    for slot in SLOTS {
        let mut st = picked_cube(slot);
        let ctx = egui::Context::default();
        let before = frame(&ctx, &st, slot, PANE, INK);
        st.cam.orbit(12.0, -4.0);
        let after = frame(&ctx, &st, slot, PANE, INK);
        assert_eq!(builds(&st, slot), 2, "{slot:?}: an orbit tessellates again");
        assert!(!after.is_empty(), "{slot:?}: the orbited pick still paints");
        assert_ne!(after, before, "{slot:?}: the painted vertices follow the camera");
        frame(&ctx, &st, slot, PANE, INK);
        assert_eq!(builds(&st, slot), 2, "{slot:?}: the next still frame reads the cache");
    }
}

#[test]
fn highlight_mesh_rebuilds_after_a_pane_resize_and_the_vertices_move() {
    for slot in SLOTS {
        let st = picked_cube(slot);
        let ctx = egui::Context::default();
        let before = frame(&ctx, &st, slot, PANE, INK);
        let after = frame(&ctx, &st, slot, SMALL_PANE, INK);
        assert_eq!(builds(&st, slot), 2, "{slot:?}: a pane resize tessellates again");
        assert!(!after.is_empty(), "{slot:?}: the resized pane still paints");
        let max_x = |v: &[(Pos2, Color32)]| v.iter().map(|(p, _)| p.x).fold(f32::NEG_INFINITY, f32::max);
        assert!(max_x(&after) < max_x(&before), "{slot:?}: the painted vertices shrink into the smaller pane");
    }
}

#[test]
fn highlight_mesh_rebuilds_after_an_ink_change_and_the_colour_changes() {
    for slot in SLOTS {
        let st = picked_cube(slot);
        let ctx = egui::Context::default();
        let before = frame(&ctx, &st, slot, PANE, INK);
        assert!(before.iter().any(|(_, c)| *c == INK), "{slot:?}: the first frame paints the first ink");
        let after = frame(&ctx, &st, slot, PANE, OTHER_INK);
        assert_eq!(builds(&st, slot), 2, "{slot:?}: a theme (ink) change tessellates again");
        assert!(after.iter().any(|(_, c)| *c == OTHER_INK), "{slot:?}: the new ink is painted");
        assert!(after.iter().all(|(_, c)| *c != INK), "{slot:?}: the old ink is never served");
    }
}

// A slanted rectangle: its first corner is behind the eye. The near plane
// z_clip = z_world - 1 cuts its first and last edges at z_world = 1.
const CLIP_MVP: [[f32; 4]; 4] = [
    [1.0, 0.0, 0.0, 0.0],
    [0.0, 1.0, 0.0, 0.0],
    [0.0, 0.0, 1.0, 1.0],
    [0.0, 0.0, -1.0, 0.0],
];

fn clip_to_px(uv: [f32; 2]) -> Pos2 {
    pos2(uv[0] * 800.0, uv[1] * 600.0)
}

fn clipped_rectangle() -> crate::pick_highlight::WorldPolyline {
    crate::pick_highlight::WorldPolyline {
        pts: vec![[-1.0, -1.0, -1.0], [1.0, -1.0, 2.0], [1.0, 1.0, 5.0], [-1.0, 1.0, 2.0]],
        closed: true,
    }
}

fn expected_rectangle_outline() -> [Pos2; 5] {
    // These are the two analytic near-plane intersections and three
    // surviving corners, projected with x/z and y/z.
    [
        pos2(1600.0 / 3.0, 600.0),
        pos2(600.0, 450.0),
        pos2(480.0, 240.0),
        pos2(200.0, 150.0),
        pos2(0.0, 200.0),
    ]
}

fn distance_to_segment(p: Pos2, a: Pos2, b: Pos2) -> f32 {
    let ab = b - a;
    let t = ((p - a).dot(ab) / ab.length_sq()).clamp(0.0, 1.0);
    p.distance(a + t * ab)
}

#[test]
fn highlight_near_clip_keeps_every_rectangle_edge_without_a_bridge() {
    let mut paths = Vec::new();
    super::push_screen_polyline_runs(&clipped_rectangle(), &CLIP_MVP, &clip_to_px, &mut paths);
    let segments: Vec<_> = paths
        .iter()
        .flat_map(|(pts, closed)| {
            assert!(!closed, "a clipped rectangle must not close across the near plane");
            pts.windows(2).map(|p| (p[0], p[1]))
        })
        .collect();
    let expected = expected_rectangle_outline();
    assert_eq!(segments.len(), 4, "each surviving part of all four edges must remain");
    for edge in expected.windows(2) {
        assert!(
            segments.iter().any(|&(a, b)| {
                (a.distance(edge[0]) < 0.01 && b.distance(edge[1]) < 0.01)
                    || (b.distance(edge[0]) < 0.01 && a.distance(edge[1]) < 0.01)
            }),
            "missing clipped edge {edge:?}; got {segments:?}"
        );
    }
}

#[test]
fn highlight_near_clip_mesh_stays_within_the_rectangle_stroke() {
    let ctx = egui::Context::default();
    let resolved = crate::pick_highlight::ResolvedHighlights {
        polylines: vec![clipped_rectangle()],
        ..Default::default()
    };
    let painter = ctx.layer_painter(egui::LayerId::background()).with_clip_rect(PANE);
    let mesh = super::build_batched_mesh(
        &painter,
        &resolved,
        false,
        &CLIP_MVP,
        4.0 / 3.0,
        &clip_to_px,
        2.0,
        4.0,
        INK,
        HALO,
    )
    .expect("clipped outline mesh");
    let expected = expected_rectangle_outline();
    for vertex in &mesh.vertices {
        let distance = expected
            .windows(2)
            .map(|p| distance_to_segment(vertex.pos, p[0], p[1]))
            .fold(f32::INFINITY, f32::min);
        assert!(
            distance <= 4.0,
            "stroke vertex {:?} is {distance} points from the outline",
            vertex.pos
        );
    }
    for edge in expected.windows(2) {
        assert!(
            mesh.vertices.iter().any(|v| v.pos.distance(edge[0]) < 4.0),
            "clipped endpoint {:?} must have stroke vertices",
            edge[0]
        );
    }
}

#[test]
fn highlight_near_clip_stops_a_front_of_eye_point_before_projection() {
    let poly = crate::pick_highlight::WorldPolyline {
        pts: vec![[0.25, 0.0, 0.5], [0.25, 0.0, 2.0]],
        closed: false,
    };
    let mut paths = Vec::new();
    super::push_screen_polyline_runs(&poly, &CLIP_MVP, &clip_to_px, &mut paths);
    assert_eq!(paths.len(), 1);
    let (pts, closed) = &paths[0];
    assert!(!closed);
    assert_eq!(pts.len(), 2);
    assert!(
        pts[0].distance(pos2(500.0, 300.0)) < 0.01,
        "the segment must start at z_world=1, not the point before the near plane: {pts:?}"
    );
    assert!(pts[1].distance(pos2(450.0, 300.0)) < 0.01);
}

#[test]
fn highlight_near_clip_does_not_join_disconnected_visible_runs() {
    let poly = crate::pick_highlight::WorldPolyline {
        pts: vec![
            [0.0, 0.0, 2.0],
            [0.5, 0.0, 2.0],
            [0.0, 0.0, -1.0],
            [0.0, 0.5, 2.0],
            [0.5, 0.5, 2.0],
        ],
        closed: false,
    };
    let mut paths = Vec::new();
    super::push_screen_polyline_runs(&poly, &CLIP_MVP, &clip_to_px, &mut paths);
    assert_eq!(paths.len(), 2, "the invisible corner separates the two visible runs");
    assert!(
        paths.iter().all(|(pts, closed)| !closed && pts.len() == 3),
        "each run retains its edge up to the near plane: {paths:?}"
    );
}

#[test]
fn highlight_near_clip_preserves_a_fully_visible_closed_outline() {
    let poly = crate::pick_highlight::WorldPolyline {
        pts: vec![[-0.5, -0.5, 2.0], [0.5, -0.5, 2.0], [0.5, 0.5, 2.0], [-0.5, 0.5, 2.0]],
        closed: true,
    };
    let mut paths = Vec::new();
    super::push_screen_polyline_runs(&poly, &CLIP_MVP, &clip_to_px, &mut paths);
    assert_eq!(paths.len(), 1);
    assert!(paths[0].1, "a fully visible closed outline retains its seam");
    assert_eq!(paths[0].0.len(), 4);
}

#[test]
fn highlight_near_clip_nonfinite_corner_does_not_connect_its_neighbors() {
    let poly = crate::pick_highlight::WorldPolyline {
        pts: vec![
            [0.0, 0.0, 2.0],
            [0.5, 0.0, 2.0],
            [f64::NAN, 0.0, 2.0],
            [0.0, 0.5, 2.0],
            [0.5, 0.5, 2.0],
        ],
        closed: false,
    };
    let mut paths = Vec::new();
    super::push_screen_polyline_runs(&poly, &CLIP_MVP, &clip_to_px, &mut paths);
    assert_eq!(paths.len(), 2);
    assert!(paths.iter().all(|(pts, closed)| !closed && pts.len() == 2));
    assert!(paths.iter().flat_map(|(pts, _)| pts).all(|p| p.is_finite()));
}

#[test]
fn highlight_near_clip_keeps_a_segment_crossing_the_positive_w_boundary() {
    // This matrix keeps clip z positive, so only the eye-plane guard clips.
    let mvp = [
        [1.0, 0.0, 0.0, 0.0],
        [0.0, 1.0, 0.0, 0.0],
        [0.0, 0.0, 0.0, 1.0],
        [0.0, 0.0, 1.0, 0.0],
    ];
    let poly = crate::pick_highlight::WorldPolyline {
        pts: vec![[0.25, 0.0, -1.0], [0.25, 0.0, 1.0]],
        closed: false,
    };
    let mut paths = Vec::new();
    super::push_screen_polyline_runs(&poly, &mvp, &clip_to_px, &mut paths);
    assert_eq!(paths.len(), 1, "clip the crossing segment instead of dropping it");
    assert!(!paths[0].1);
    assert_eq!(paths[0].0.len(), 2);
    assert!(paths[0].0.iter().all(|p| p.is_finite()));
    assert!(paths[0].0[0].x > 1_000_000.0, "clip at the positive-w limit");
    assert!(paths[0].0[1].distance(pos2(500.0, 300.0)) < 0.01);
}
