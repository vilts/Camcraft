//! #326 in the surface: the Setups page's part-alignment control queues
//! `AlignPart` (never a world poke), the drain lands it as one undo unit,
//! Ctrl+Z / Ctrl+Y walk it, the typed angles reach the same command, and
//! a refused choice pushes nothing. The work offset is not touched.
//! Provenance: our own page law; no vendor material consulted.

use egui::accesskit::Role;
use egui::{Event, Key, Modifiers, Vec2};
use egui_kittest::kittest::Queryable;
use egui_kittest::Harness;

use camcraft_app::commands::ImportModel;
use camcraft_app::geometry::{Point3, cube, transform_point};
use camcraft_app::setup_align::{self, AlignPreset};
use camcraft_app::setup_turn::TurnPivot;

use crate::state::{CommandKind, State};

fn run(state: &mut State, k: CommandKind) {
    state.pending.push_back(k);
    state.drain_actions();
}

fn with_part() -> State {
    let mut state = State::new();
    let part = ImportModel::from_mesh("part", cube(Point3::new(-20.0, -10.0, 0.0), Point3::new(20.0, 10.0, 8.0)));
    state.history.execute(&state.world, part);
    run(&mut state, CommandKind::CreateSetup { name: None, datum_from: None });
    state
}

fn frame(ui: &mut egui::Ui, st: &mut State) {
    super::panes::shortcuts(ui, st);
    st.drain_actions();
    super::setup_align_pick::picker(ui, st, "Setup1");
}

fn key(h: &mut Harness<'static, State>, k: Key) {
    h.input_mut().events.push(Event::ModifiersChanged(Modifiers::COMMAND));
    h.input_mut().events.push(Event::Key {
        key: k,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: Modifiers::COMMAND,
    });
    h.run_steps(2);
}

fn held(state: &State) -> String {
    super::align_text(setup_align::stored(&state.world, "Setup1"))
}

#[test]
fn the_alignment_control_turns_the_part_through_the_command() {
    let mut h = Harness::builder()
        .with_size(Vec2::new(600.0, 400.0))
        .build_ui_state(frame, with_part());
    h.run();
    assert_eq!(held(h.state()), "As modelled");
    assert!(h.state().pending.is_empty(), "drawing the control queues nothing");
    h.get_by(|n| n.role() == Role::ComboBox && n.value().as_deref() == Some("As modelled")).click();
    h.run();
    h.get_by_label("Flip over X").click();
    h.run();
    assert_eq!(held(h.state()), "Flip over X", "the drain lands the flip");
    assert_eq!(h.state().status, "Setup1 part alignment: Flip over X");
    assert_eq!(camcraft_app::setup::work_offset(&h.state().world, "Setup1"), None, "the work offset is untouched");
    key(&mut h, Key::Z);
    assert_eq!(held(h.state()), "As modelled", "undo restores as modelled");
    key(&mut h, Key::Y);
    assert_eq!(held(h.state()), "Flip over X", "redo restores the flip");
}

#[test]
fn typed_angles_reach_the_same_command_and_a_refusal_says_why() {
    let mut state = with_part();
    run(&mut state, CommandKind::AlignPart { setup: "Setup1".into(), angles: [0.0, 0.0, 90.0], pivot: TurnPivot::PartCentre });
    assert_eq!(held(&state), AlignPreset::TurnZ90.label());
    run(&mut state, CommandKind::AlignPart { setup: "Setup1".into(), angles: [30.0, 0.0, 0.0], pivot: TurnPivot::PartCentre });
    assert_eq!(held(&state), "turned X 30°, Y 0°, Z 0°");
    run(&mut state, CommandKind::AlignPart { setup: "Setup1".into(), angles: [30.0, 0.0, 0.0], pivot: TurnPivot::PartCentre });
    assert!(state.status.contains("part alignment not changed"), "{}", state.status);
    run(&mut state, CommandKind::AlignPart { setup: "Setup1".into(), angles: [f64::NAN, 0.0, 0.0], pivot: TurnPivot::PartCentre });
    assert!(state.status.contains("finite"), "{}", state.status);
    // Two turns landed, the refusals pushed nothing.
    run(&mut state, CommandKind::Undo);
    run(&mut state, CommandKind::Undo);
    assert_eq!(held(&state), "As modelled");
}

/// The anchors of the active setup's "model bounds" box, at the bottom
/// level: (SW, NE, C) points.
fn model_anchors(state: &mut State) -> [[f64; 3]; 3] {
    state.wcs_ui.anchor_source = 1;
    state.wcs_ui.anchor_z = 0;
    assert_eq!(crate::interact::anchor_reference(state).map(|(_, w)| w), Some("model bounds"));
    let pts = crate::interact::anchor_points(state);
    assert_eq!((pts[6].1, pts[2].1, pts[4].1), ("SW", "NE", "C"));
    [pts[6].0, pts[2].0, pts[4].0]
}

/// The part's own eight corners carried through the stored alignment by
/// plain transform math, boxed: what the operator sees on the table.
fn aligned_corner_box(state: &State) -> ([f64; 3], [f64; 3]) {
    let a = setup_align::stored(&state.world, "Setup1").unwrap_or_default();
    let mut lo = [f64::MAX; 3];
    let mut hi = [f64::MIN; 3];
    for x in [-20.0, 20.0] {
        for y in [-10.0, 10.0] {
            for z in [0.0, 8.0] {
                let p = transform_point(Point3::new(x, y, z), a.pos, a.rot);
                for (i, v) in [p.x, p.y, p.z].into_iter().enumerate() {
                    lo[i] = lo[i].min(v);
                    hi[i] = hi[i].max(v);
                }
            }
        }
    }
    (lo, hi)
}

fn near(a: [f64; 3], b: [f64; 3]) -> bool {
    a.iter().zip(b).all(|(x, y)| (x - y).abs() < 1e-9)
}

/// Review of PR #341: the work-offset anchors read the part in the active
/// setup's frame, so an aligned part's corner is the corner on the table,
/// and a G54 placed on it lands there. An untouched setup is unchanged.
#[test]
fn model_bounds_anchors_follow_the_aligned_part() {
    let mut state = with_part();
    assert_eq!(camcraft_app::setup::active(&state.world).as_deref(), Some("Setup1"));
    // As modelled: the 40 x 20 x 8 part's own corners, as before #326.
    let plain = model_anchors(&mut state);
    assert!(near(plain[0], [-20.0, -10.0, 0.0]) && near(plain[1], [20.0, 10.0, 0.0]), "{plain:?}");
    assert!(near(plain[2], [0.0, 0.0, 0.0]), "{plain:?}");

    // Flipped over X about its centre: the box keeps its footprint.
    run(&mut state, CommandKind::AlignPart { setup: "Setup1".into(), angles: [180.0, 0.0, 0.0], pivot: TurnPivot::PartCentre });
    let flipped = model_anchors(&mut state);
    let (lo, hi) = aligned_corner_box(&state);
    assert!(near(flipped[0], [lo[0], lo[1], lo[2]]) && near(flipped[1], [hi[0], hi[1], lo[2]]), "{flipped:?}");
    assert!(near(flipped[0], [-20.0, -10.0, 0.0]), "{flipped:?}");

    // Flipped AND quarter-turned: the part now runs 20 along X, 40 along Y.
    for angles in [[0.0, 0.0, 90.0], [180.0, 0.0, 90.0]] {
        run(&mut state, CommandKind::AlignPart { setup: "Setup1".into(), angles, pivot: TurnPivot::PartCentre });
        let turned = model_anchors(&mut state);
        let (lo, hi) = aligned_corner_box(&state);
        assert!(near(turned[0], [lo[0], lo[1], lo[2]]), "{angles:?} SW {turned:?} vs {lo:?}");
        assert!(near(turned[1], [hi[0], hi[1], lo[2]]), "{angles:?} NE {turned:?} vs {hi:?}");
        assert!(near(turned[0], [-10.0, -20.0, 0.0]), "{angles:?} SW pinned: {turned:?}");
        assert!(near(turned[1], [10.0, 20.0, 0.0]), "{angles:?} NE pinned: {turned:?}");
        assert!(near(turned[2], [0.0, 0.0, 0.0]), "{angles:?} C pinned: {turned:?}");
    }

    // A G54 placed on the SW corner lands on the turned part's corner.
    let sw = model_anchors(&mut state)[0];
    run(&mut state, CommandKind::SetWcs { pos: sw, roll_deg: 0.0, pitch_deg: 0.0, yaw_deg: 0.0 });
    let g54 = camcraft_app::setup::wcs_entity(&state.world, Some("Setup1"))
        .and_then(|e| camcraft_app::model::transform_of(&state.world, &e))
        .map(|l| [l.pos.x, l.pos.y, l.pos.z]);
    assert!(g54.is_some_and(|p| near(p, [-10.0, -20.0, 0.0])), "G54 at {g54:?}");

    // Back to as modelled: the anchors are the untouched ones again.
    run(&mut state, CommandKind::AlignPart { setup: "Setup1".into(), angles: [0.0, 0.0, 0.0], pivot: TurnPivot::PartCentre });
    assert_eq!(model_anchors(&mut state), plain);
}

/// A document point as Setup1 holds it.
fn held_at(state: &State, p: [f64; 3]) -> [f64; 3] {
    let a = setup_align::stored(&state.world, "Setup1").unwrap_or_default();
    let q = transform_point(Point3::new(p[0], p[1], p[2]), a.pos, a.rot);
    [q.x, q.y, q.z]
}

/// #606 by real clicks: Z typed as 90 and the row's Apply clicked turns
/// the part about the WCS origin (the row's default, named on it), so the
/// part's corner on the origin stays there; "part centre" chosen in the
/// row's pivot combo turns it about the bounds centre instead. Ctrl+Z /
/// Ctrl+Y walk each turn as one undo unit.
#[test]
fn the_typed_turn_pivots_on_the_wcs_origin_by_real_clicks_and_undoes() {
    let mut state = with_part();
    let o = [-20.0, -10.0, 0.0];
    run(&mut state, CommandKind::SetWcs { pos: o, roll_deg: 0.0, pitch_deg: 0.0, yaw_deg: 0.0 });
    let mut h = Harness::builder()
        .with_size(Vec2::new(600.0, 400.0))
        .build_ui_state(frame, state);
    h.run();
    let pivot = |h: &Harness<'static, State>, v: &str| {
        h.query_by(|n| n.role() == Role::ComboBox && n.value().as_deref() == Some(v)).is_some()
    };
    assert!(pivot(&h, "WCS origin"), "the row names its pivot: the WCS origin by default");
    // Z is the row's third angle: click it, type 90, then Apply.
    let z = h.get_all_by_role(Role::SpinButton).nth(2).expect("the Z angle").rect();
    h.get_all_by_role(Role::SpinButton).find(|n| n.rect() == z).expect("Z").click();
    h.run();
    h.get_all_by_role(Role::SpinButton)
        .find(|n| (n.rect().top() - z.top()).abs() < 1.0 && n.rect().left() >= z.left() - 1.0)
        .expect("Z, editing")
        .type_text("90");
    h.run();
    h.get_by_label("Apply").click();
    h.run();
    assert_eq!(held(h.state()), AlignPreset::TurnZ90.label(), "{}", h.state().status);
    assert!(near(held_at(h.state(), o), o), "the corner on the origin stays: {:?}", held_at(h.state(), o));
    assert!(!near(held_at(h.state(), [0.0, 0.0, 4.0]), [0.0, 0.0, 4.0]), "not about the centre");
    let by_origin = setup_align::stored(&h.state().world, "Setup1");
    key(&mut h, Key::Z);
    assert_eq!(held(h.state()), "As modelled", "undo restores as modelled");
    key(&mut h, Key::Y);
    assert_eq!(setup_align::stored(&h.state().world, "Setup1"), by_origin, "redo turns it again");
    key(&mut h, Key::Z);
    // The other pivot, by the row's own combo.
    h.get_by(|n| n.role() == Role::ComboBox && n.value().as_deref() == Some("WCS origin")).click();
    h.run();
    h.get_by_label("part centre").click();
    h.run();
    assert!(pivot(&h, "part centre"), "the row now names the centre");
    h.get_by_label("Apply").click();
    h.run();
    assert_eq!(held(h.state()), AlignPreset::TurnZ90.label());
    assert!(near(held_at(h.state(), [0.0, 0.0, 4.0]), [0.0, 0.0, 4.0]), "the centre stays put");
    assert_eq!(camcraft_app::setup::work_offset(&h.state().world, "Setup1"), None, "the work offset is untouched");
}

/// #738: the page is one flow. A preset chosen AFTER a face lay flips the
/// part from the laid pose (the laid wall ends face down, where the
/// absolute "Flip over X" from as modelled would leave it facing +Y) -
/// it never wipes the face pick's move. Ctrl+Z steps back to the lay
/// exactly; "As modelled" is the one reset, and the header says so.
#[test]
fn a_preset_after_a_face_lay_composes_and_as_modelled_is_the_one_reset() {
    let mut state = with_part();
    let wall = {
        let e = camcraft_app::model::find_by_name(&state.world, "part").expect("part");
        let f = camcraft_app::model::features_of(&state.world, &e).expect("features");
        f.faces.iter().find(|x| x.normal[1].abs() > 0.99 && (x.normal[1] * -10.0 - x.offset).abs() < 1e-9).map(|x| x.id).expect("the front wall")
    };
    let picks = vec![camcraft_app::operation::GeoSel {
        entity: "part".into(),
        feature: camcraft_app::operation::FeatureSel::Face(wall),
    }];
    let lay = camcraft_app::pick::PlaneLay { plane: camcraft_app::cl::ArcPlane::Xy, flip: false, place: true };
    run(&mut state, CommandKind::LayPartOnPlane { setup: "Setup1".into(), picks, lay });
    let laid = setup_align::stored(&state.world, "Setup1").expect("laid");
    let wall_dir = |a: camcraft_app::schema::Location| {
        let v = camcraft_app::geometry::quat_rotate_point(a.rot, Point3::new(0.0, -1.0, 0.0));
        [v.x, v.y, v.z]
    };
    assert!(near(wall_dir(laid), [0.0, 0.0, 1.0]), "the wall faces up");

    let mut h = Harness::builder()
        .with_size(Vec2::new(600.0, 400.0))
        .build_ui_state(frame, state);
    h.run();
    assert!(h.query_by_label_contains("Part alignment").is_some(), "the row keeps its header (the flow is its tooltip)");
    let now = held(h.state());
    h.get_by(|n| n.role() == Role::ComboBox && n.value().as_deref() == Some(now.as_str())).click();
    h.run();
    h.get_by_label("Flip over X").click();
    h.run();
    let flipped = setup_align::stored(&h.state().world, "Setup1").expect("flipped");
    assert!(near(wall_dir(flipped), [0.0, 0.0, -1.0]), "composed: the laid wall now faces down: {:?}", wall_dir(flipped));
    key(&mut h, Key::Z);
    assert_eq!(setup_align::stored(&h.state().world, "Setup1"), Some(laid), "undo: back to the lay, exactly");

    let now = held(h.state());
    h.get_by(|n| n.role() == Role::ComboBox && n.value().as_deref() == Some(now.as_str())).click();
    h.run();
    h.get_by_label("As modelled").click();
    h.run();
    assert_eq!(held(h.state()), "As modelled", "the one reset");
    key(&mut h, Key::Z);
    assert_eq!(setup_align::stored(&h.state().world, "Setup1"), Some(laid), "undo brings the lay back");
}

/// #1538: with a 5 in vise closed on the stock, the "model bounds"
/// anchors are the 50 x 50 x 6 part's corners, not the vise's, and a
/// G54 set from the SW anchor lands on the part's corner.
#[test]
fn model_bounds_anchors_are_the_parts_never_the_vise() {
    use camcraft_app::commands::{InsertVise, SetStock, VisePlacement};
    use camcraft_app::schema::Location;
    use camcraft_app::stock::StockShape;
    let mut state = State::new();
    let part = ImportModel::from_mesh("block", cube(Point3::new(2.5, 2.5, 4.0), Point3::new(52.5, 52.5, 10.0)));
    state.history.execute(&state.world, part);
    let stock = SetStock::new(&state.world, StockShape::Box { dx: 55.0, dy: 55.0, dz: 11.0 }, Location::default());
    state.history.execute(&state.world, stock.expect("stock"));
    let p = camcraft_app::vise::size("5 in").expect("5 in").half_open();
    let vise = InsertVise::new(&state.world, &p, None, VisePlacement::FitStock).expect("vise");
    state.history.execute(&state.world, vise);
    let [sw, ne, c] = model_anchors(&mut state);
    assert_eq!((sw, ne, c), ([2.5, 2.5, 4.0], [52.5, 52.5, 4.0], [27.5, 27.5, 4.0]), "the part's corners");
    run(&mut state, CommandKind::SetWcs { pos: sw, roll_deg: 0.0, pitch_deg: 0.0, yaw_deg: 0.0 });
    let g54 = camcraft_app::model::find_by_name(&state.world, "G54").expect("the WCS");
    let at = camcraft_app::model::transform_of(&state.world, &g54).expect("placed").pos;
    assert_eq!([at.x, at.y, at.z], [2.5, 2.5, 4.0], "G54 on the part's SW corner");
}
