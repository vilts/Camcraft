//! #760 / #1542: Align WCS (3-2-1) on a milling setup must keep WCS +Z
//! aligned with machine +Z (steering tilted Z picks to Part alignment on
//! the Setup page), while flat-top 3-2-1 alignments with Z yaw succeed.
//!
//! Provenance: our own UI regression tests; no vendor material consulted.

use camcraft_app::commands::ImportModel;
use camcraft_app::geometry::{Point3, cube, feature};
use camcraft_app::model;
use camcraft_app::operation::{FeatureSel, GeoSel};

use crate::state::{CommandKind, State};

fn run(state: &mut State, k: CommandKind) {
    state.pending.push_back(k);
    state.drain_actions();
}

fn state_with_block() -> (State, std::sync::Arc<feature::Features>) {
    let mut state = State::new();
    let part = ImportModel::from_mesh(
        "block",
        cube(Point3::new(0.0, 0.0, 0.0), Point3::new(40.0, 20.0, 10.0)),
    );
    state.history.execute(&state.world, part);
    run(&mut state, CommandKind::CreateSetup { name: None, datum_from: None });
    let e = model::find_by_name(&state.world, "block").expect("block");
    let feats = model::features_of(&state.world, &e).expect("features");
    (state, feats)
}

fn face_matching(
    feats: &feature::Features,
    pred: impl Fn(&feature::Face) -> bool,
) -> GeoSel {
    let id = feats
        .faces
        .iter()
        .find(|f| pred(f))
        .map(|f| f.id)
        .expect("matching face");
    GeoSel { entity: "block".into(), feature: FeatureSel::Face(id) }
}

fn edge_between(feats: &feature::Features, a: [f64; 3], b: [f64; 3]) -> GeoSel {
    let eq = |p: [f64; 3], q: [f64; 3]| {
        (p[0] - q[0]).abs() < 1e-6 && (p[1] - q[1]).abs() < 1e-6 && (p[2] - q[2]).abs() < 1e-6
    };
    let id = feature::chains(feats)
        .into_iter()
        .find(|c| {
            let pts = feature::chain_points(feats, c);
            pts.len() == 2 && ((eq(pts[0], a) && eq(pts[1], b)) || (eq(pts[0], b) && eq(pts[1], a)))
        })
        .map(|c| c.id)
        .expect("edge");
    GeoSel { entity: "block".into(), feature: FeatureSel::Chain(id) }
}

#[test]
fn align_wcs_321_refuses_tilted_side_face_on_mill_and_names_part_alignment() {
    let (mut state, feats) = state_with_block();
    // Pick the +X side face (normal [+1, 0, 0]) and a vertical edge on that face.
    let side_face = face_matching(&feats, |f| f.normal[0] > 0.99);
    let y_edge = edge_between(&feats, [40.0, 0.0, 10.0], [40.0, 20.0, 10.0]);
    state.op_ui.geo = vec![side_face, y_edge];

    crate::vertex_pick::queue_wcs_321(&mut state);
    assert!(
        state.pending.is_empty(),
        "tilted 3-2-1 on a Mill setup must not queue SetWcs"
    );
    assert!(
        state.status.contains("Part alignment"),
        "refusal status must steer to Part alignment, got: {}",
        state.status
    );
}

#[test]
fn align_wcs_321_allows_top_face_with_z_yaw_on_mill() {
    let (mut state, feats) = state_with_block();
    let top = face_matching(&feats, |f| f.normal[2] > 0.99 && (f.offset - 10.0).abs() < 1e-9);
    let x_along_y = edge_between(&feats, [0.0, 0.0, 10.0], [0.0, 20.0, 10.0]);
    state.op_ui.geo = vec![top, x_along_y];

    crate::vertex_pick::queue_wcs_321(&mut state);
    assert_eq!(state.pending.len(), 1, "top face + yaw edge queues SetWcs");
    let Some(CommandKind::SetWcs { roll_deg, pitch_deg, yaw_deg, .. }) = state.pending.front() else {
        panic!("expected SetWcs, got {:?}", state.pending);
    };
    assert!(roll_deg.abs() < 1e-6 && pitch_deg.abs() < 1e-6, "Z stays vertical: {roll_deg} {pitch_deg}");
    assert!((yaw_deg.abs() - 90.0).abs() < 1e-6, "yaw ~ 90 deg, got {yaw_deg}");
}
