//! M13.4 face origin snap through the real shell: the seed's Contour1
//! carries the Block's TOP face, so its chip offers "Set origin" (the
//! centroid) and "Set origin (Z only)" (flat face). Both queue `SetWcs`;
//! Z only keeps the slot's XY.
//! Provenance: our own page law; no vendor material consulted.

use super::*;

fn g54(h: &Harness<'static, ShellUi>) -> [f64; 3] {
    let st = &h.state().state;
    let e = model::find_by_name(&st.world, "G54").expect("G54");
    let p = model::transform_of(&st.world, &e).unwrap_or_default().pos;
    [p.x, p.y, p.z]
}

fn near(a: [f64; 3], b: [f64; 3]) -> bool {
    (a[0] - b[0]).abs() + (a[1] - b[1]).abs() + (a[2] - b[2]).abs() < 1e-6
}

#[test]
fn face_chip_sets_origin_at_centroid_and_z_only_keeps_xy() {
    let mut harness = shell_harness();
    harness.run();
    // Where the top face sits in the world: its centroid through the
    // Block's placement (the seed block is 60 x 60 x 30).
    let top = {
        let st = &harness.state().state;
        let block = model::find_by_name(&st.world, "Block").expect("seed has Block");
        let loc = model::transform_of(&st.world, &block).unwrap_or_default().pos;
        [loc.x + 30.0, loc.y + 30.0, loc.z + 30.0]
    };

    harness.get_by_label("Set origin").click();
    harness.run_steps(2);
    assert!(near(g54(&harness), top), "G54 moved to the top face's centre: {:?}", g54(&harness));

    // Elsewhere first, then Z only: XY stays, Z lands on the face.
    harness.state_mut().state.pending.push_back(CommandKind::SetWcs {
        pos: [5.0, 7.0, -3.0],
        roll_deg: 0.0,
        pitch_deg: 0.0,
        yaw_deg: 0.0,
    });
    harness.run_steps(2);
    assert!(near(g54(&harness), [5.0, 7.0, -3.0]));
    harness.get_by_label("Set origin (Z only)").click();
    harness.run_steps(2);
    assert!(near(g54(&harness), [5.0, 7.0, top[2]]), "Z only keeps XY: {:?}", g54(&harness));

    // Undoable like every origin move.
    harness.state_mut().state.pending.push_back(CommandKind::Undo);
    harness.run_steps(2);
    assert!(near(g54(&harness), [5.0, 7.0, -3.0]), "undo restores the origin: {:?}", g54(&harness));
}

/// #187: the 3-2-1 button reads the open op's picks. The seed's top face
/// alone is not enough (dark, the tooltip names the missing edge); add the
/// block's front top edge and the click places G54 on the face, at the
/// edge's start, X along the edge - undoable like every origin move.
#[test]
fn align_wcs_321_places_g54_from_the_face_and_edge_picks() {
    let mut harness = shell_harness();
    harness.run();
    {
        use egui_kittest::kittest::NodeT;
        assert!(
            harness.get_by_label("Align WCS (3-2-1)").accesskit_node().is_disabled(),
            "a face alone cannot align: the button is dark"
        );
    }
    let (edge, block_loc) = {
        let st = &harness.state().state;
        let block = model::find_by_name(&st.world, "Block").expect("seed has Block");
        let f = model::features_of(&st.world, &block).expect("features");
        let id = camcraft_app::geometry::feature::chains(&f)
            .into_iter()
            .find(|c| {
                let p = camcraft_app::geometry::feature::chain_points(&f, c);
                p.len() == 2 && p.iter().all(|q| q[1].abs() < 1e-9 && (q[2] - 30.0).abs() < 1e-9)
            })
            .map(|c| c.id)
            .expect("the block's front top edge");
        (id, model::transform_of(&st.world, &block).unwrap_or_default())
    };
    harness.state_mut().state.op_ui.geo.push(camcraft_app::operation::GeoSel {
        entity: "Block".into(),
        feature: camcraft_app::operation::FeatureSel::Chain(edge),
    });
    harness.run();
    harness.get_by_label("Align WCS (3-2-1)").click();
    harness.run_steps(2);
    let p = g54(&harness);
    assert!((p[2] - (block_loc.pos.z + 30.0)).abs() < 1e-6, "G54 on the top face: {p:?}");
    assert!((p[1] - block_loc.pos.y).abs() < 1e-6, "on the front edge: {p:?}");
    let rot = {
        let st = &harness.state().state;
        let e = model::find_by_name(&st.world, "G54").expect("G54");
        model::transform_of(&st.world, &e).unwrap_or_default().rot
    };
    let x = camcraft_app::geometry::quat_rotate_point(rot, camcraft_app::geometry::Point3::new(1.0, 0.0, 0.0));
    assert!((x.x.abs() - 1.0).abs() < 1e-6, "X along the edge: {x:?}");
    harness.state_mut().state.pending.push_back(CommandKind::Undo);
    harness.run_steps(2);
    assert!(!near(g54(&harness), p), "undo moves G54 back");
}
