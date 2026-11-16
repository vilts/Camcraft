//! #330 (reopened)'s frame test: the seed's block turned 30 degrees
//! about X, its most camera-facing tilted face picked by a real viewport
//! click with the Setups page's "Lay on a plane" row armed, "Face down"
//! on (the #606 default), "Apply face" clicked - all at the hand-test
//! window size, where the row's Face down, level box and Apply must be
//! ON SCREEN (one long row ran
//! them past the properties pane's edge, out of reach). The part as the
//! viewport draws it is TURNED, not only moved: the face's outward normal
//! is -Z, the face lies in Z 0 and the part stands above it. Ctrl+Z puts
//! the part back where it was. On main before #554 the click took an
//! edge (Edge-mode snapping) and nothing could be applied.
//! Provenance: our own pick law test over our own shell; no vendor
//! material consulted.

use camcraft_app::geometry::{Point3, quat_rotate_point, transform_point};
use egui_kittest::kittest::NodeT;

use crate::state::SetupPage;

use super::*;

/// The seed block's (60 x 60 x 30) faces that a roll about X tilts, in
/// its own (local) frame: centre and outward normal.
const TILTED: [([f64; 3], [f64; 3]); 4] = [
    ([30.0, 30.0, 30.0], [0.0, 0.0, 1.0]),
    ([30.0, 30.0, 0.0], [0.0, 0.0, -1.0]),
    ([30.0, 0.0, 15.0], [0.0, -1.0, 0.0]),
    ([30.0, 60.0, 15.0], [0.0, 1.0, 0.0]),
];

/// The block's pose as the viewport draws it (the active setup's frame).
fn drawn(sh: &ShellUi) -> camcraft_app::schema::Location {
    let e = model::find_by_name(&sh.state.world, "Block").expect("the seed's Block");
    camcraft_app::setup_align::view_pose(&sh.state.world, &e).unwrap_or_default()
}

fn placed(loc: camcraft_app::schema::Location, p: [f64; 3]) -> [f64; 3] {
    let q = transform_point(Point3::new(p[0], p[1], p[2]), loc.pos, loc.rot);
    [q.x, q.y, q.z]
}

fn turned(loc: camcraft_app::schema::Location, v: [f64; 3]) -> [f64; 3] {
    let q = quat_rotate_point(loc.rot, Point3::new(v[0], v[1], v[2]));
    [q.x, q.y, q.z]
}

fn pixel(sh: &ShellUi, p: [f64; 3]) -> egui::Pos2 {
    let ([x0, y0], [x1, y1]) = sh.state.viewport_rect;
    let (w, hgt) = (x1 - x0, y1 - y0);
    let mvp = sh.state.cam.mvp(f64::from(w / hgt));
    let uv = interact::project_uv(&mvp, p).expect("the face is in front of the camera");
    egui::pos2(x0 + uv[0] * w, y0 + uv[1] * hgt)
}

#[test]
fn a_clicked_tilted_face_is_turned_flat_onto_g17_not_only_moved() {
    let mut harness = shell_harness();
    harness.run();
    {
        let st = &mut harness.state_mut().state;
        st.pending.push_back(CommandKind::MoveModel {
            name: "Block".into(),
            pos: [0.0, 0.0, 0.0],
            roll_deg: 30.0,
            pitch_deg: 0.0,
            yaw_deg: 0.0,
        });
        st.pending.push_back(CommandKind::CreateSetup { name: None, datum_from: None });
        st.pending.push_back(CommandKind::FrameAll);
        st.drain_actions();
    }
    let setup = camcraft_app::setup::active(&harness.state().state.world).expect("Setup1");
    harness.state_mut().state.ui_target = UiTarget::SetupNode(setup.clone(), SetupPage::Settings);
    harness.run();
    let before = drawn(harness.state());
    // The tilted face that most faces the camera: its centre is on screen.
    let eye = harness.state().state.cam.eye_f32().map(f64::from);
    let (face_c, face_n) = TILTED
        .into_iter()
        .max_by(|a, b| {
            let facing = |(c, n): ([f64; 3], [f64; 3])| {
                let (w, d) = (placed(before, c), turned(before, n));
                (0..3).map(|k| (eye[k] - w[k]) * d[k]).sum::<f64>()
            };
            facing(*a).total_cmp(&facing(*b))
        })
        .expect("four faces");
    let n0 = turned(before, face_n);
    assert!(n0[2].abs() > 0.1 && n0[2].abs() < 0.9, "the face starts tilted: {n0:?}");

    // Arm the row by its own button, then click the tilted face.
    // #771: the face row sits behind the "Plane orientation" dropdown.
    harness
        .get_by(|n| n.role() == egui::accesskit::Role::ComboBox && n.value().as_deref() == Some("Axes"))
        .click();
    harness.run();
    harness.get_by_label("Face").click();
    harness.run();
    harness.get_by_label("Pick a face").click();
    harness.run();
    assert!(harness.state().state.plane_pick.is_some(), "the row is armed");
    let px = pixel(harness.state(), placed(before, face_c));
    click_viewport(&mut harness, px);
    let picks = harness.state().state.plane_pick.as_ref().map(|d| d.picks.clone()).unwrap_or_default();
    assert_eq!(picks.len(), 1, "one pick: {picks:?} ({})", harness.state().state.status);
    let FeatureSel::Face(fid) = picks[0].feature else { panic!("a face: {picks:?}") };
    let e = model::find_by_name(&harness.state().state.world, "Block").expect("Block");
    let f = model::features_of(&harness.state().state.world, &e).expect("features");
    let face = f.faces.iter().find(|x| x.id == fid).expect("the picked face");
    let plane = (0..3).map(|k| face.normal[k] * face_c[k]).sum::<f64>();
    assert!(
        (0..3).all(|k| (face.normal[k].abs() - face_n[k].abs()).abs() < 1e-9) && (plane - face.offset).abs() < 1e-9,
        "the clicked face, not an edge or another face: {picks:?}"
    );

    // Face down (on by default, #606): G17 face down, part above (the
    // floor of the table) - no click needed. The row's options are on
    // screen too (they ran past the pane's right edge at this window size).
    let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, Vec2::new(1280.0, 720.0));
    for label in ["Face down", "Face to plane level"] {
        let r = harness.get_by_label(label).rect();
        assert!(screen.contains_rect(r), "{label:?} is on screen: {r:?}");
    }
    assert_eq!(
        harness.get_by_label("Face down").accesskit_node().toggled(),
        Some(egui::accesskit::Toggled::True),
        "#606: a new pick lays the face down"
    );
    // The row's own Apply, ON SCREEN at the window's size: it used to sit
    // after the picked face's long id, past the pane's right edge.
    let apply = harness.get_by_label("Apply face");
    assert!(screen.contains_rect(apply.rect()), "Apply face is on screen: {:?} in {screen:?}", apply.rect());
    apply.click();
    harness.run_steps(3);
    let st = &harness.state().state;
    assert!(st.status.contains("laid on G17 (XY)"), "{}", st.status);
    let after = drawn(harness.state());
    let n = turned(after, face_n);
    let sin = (n[0] * n[0] + n[1] * n[1]).sqrt();
    assert!(n[2] < 0.0 && sin < 1e-9, "the face is turned to face -Z, not left tilted: {n:?}");
    let c = placed(after, face_c);
    assert!(c[2].abs() < 1e-9, "the face lies in Z 0: {c:?}");
    let tris = model::facets_of(&st.world, &e).expect("facets");
    let low = tris
        .iter()
        .flat_map(|t| [t.a, t.b, t.c])
        .map(|p| transform_point(p, after.pos, after.rot).z)
        .fold(f64::INFINITY, f64::min);
    assert!(low > -1e-9, "the part stands above the plane: lowest z {low}");

    // Undo puts it back exactly.
    harness.input_mut().events.push(Event::ModifiersChanged(Modifiers::COMMAND));
    harness.input_mut().events.push(Event::Key {
        key: Key::Z,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: Modifiers::COMMAND,
    });
    harness.run_steps(2);
    assert_eq!(drawn(harness.state()), before, "Ctrl+Z restores the placement");
}
