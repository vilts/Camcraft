//! Editing a placed mod vise pair (#832): `SetModJaw` changes a block's
//! figures and regenerates its solid where it stands, `RefitModVise`
//! re-fits the adjustable block after the stock changed (a new hole, a
//! new slide, its face on the stock to 1e-6) - one undo each, exact to
//! the bit - and every refusal is named with nothing moved. A plate
//! edit whose jaw was not in front of the stock before is judged as if
//! clear (the #833 review's probe): bringing a jaw 16 mm into the stock
//! is refused, not passed on a warning.
//!
//! Gate map (M4.4): covers app::commands::mod_vise_edit (SetModJaw,
//! RefitModVise, partner_of), app::commands::mod_vise (judge_face,
//! jaws_after_reseat), app::modvise::fit (footprint_at).

use camcraft_app::commands::fixture_plate::plate_of;
use camcraft_app::commands::{CommandError, History, InsertPlate, PlaceModVisePair, PlatePlacement, RefitModVise, SetModJaw, SetPlate, SetStock};
use camcraft_app::fixture;
use camcraft_app::geometry::{IDENTITY_Q, Point3, transform_point};
use camcraft_app::model;
use camcraft_app::modvise::{self, JawParams};
use camcraft_app::plate::{self, MountAt};
use camcraft_app::schema::{FixtureDef, Location};
use camcraft_app::stock::StockShape;
use camcraft_app::world::new_world;
use flecs_ecs::prelude::World;

const IN: f64 = 25.4;

/// A `dy`-deep 60 x _ x 20 stock box placed at `at`.
fn restock(world: &World, history: &mut History, dy: f64, at: Point3) {
    let cmd = SetStock::new(world, StockShape::Box { dx: 60.0, dy, dz: 20.0 }, Location { pos: at, rot: IDENTITY_Q }).expect("stock");
    history.execute(world, cmd);
}

/// The #820 review's rig: a 60 mm stock (centred, z 0..20), a 770MX
/// plate laid so the 1 in preset's fixed block at C14 has its face on
/// the stock's -Y side; the pair placed (adjustable at I16).
fn rig() -> (World, History, String, [String; 2]) {
    let world = new_world();
    let mut history = History::default();
    restock(&world, &mut history, 60.0, Point3::ZERO);
    let row = modvise::preset("1 in grid 3/8-16").expect("preset");
    let (f, a) = (row.fixed(), row.adjustable());
    let p = plate::preset("770MX table 28 x 15 in").map(plate::PlatePreset::params).expect("preset");
    let (hx, hy) = p.grid().locate("C14").expect("hole");
    let at = Location { pos: Point3::new(-f.centre_x() - hx, -30.0 - f.face_offset - hy, 0.0), rot: IDENTITY_Q };
    let cmd = InsertPlate::new(&world, &p, None, PlatePlacement::At(at)).expect("plate");
    let plate = cmd.name().to_owned();
    history.execute(&world, cmd);
    let cmd = PlaceModVisePair::new(&world, &plate, &f, &a, "C14", 0.0).expect("pair");
    assert_eq!((cmd.fit().fixed_at.as_str(), cmd.fit().adjustable_at.as_str()), ("C14", "I16"), "the rig");
    let names = [cmd.names().0.to_owned(), cmd.names().1.to_owned()];
    history.execute(&world, cmd);
    (world, history, plate, names)
}

fn def(world: &World, name: &str) -> FixtureDef {
    model::find_by_name(world, name).and_then(|e| fixture::def_of(world, &e)).expect(name)
}

fn jaw(world: &World, name: &str) -> JawParams {
    def(world, name).mod_jaw.expect("a jaw block")
}

/// Everything an undo must put back: the marker, the pose, the facets.
fn snap(world: &World, name: &str) -> (FixtureDef, Location, Vec<[f64; 9]>) {
    let e = model::find_by_name(world, name).expect(name);
    let tris = model::facets_of(world, &e).map(|f| f.iter().map(|t| [t.a.x, t.a.y, t.a.z, t.b.x, t.b.y, t.b.z, t.c.x, t.c.y, t.c.z]).collect()).unwrap_or_default();
    (def(world, name), model::transform_of(world, &e).unwrap_or_default(), tris)
}

/// The block's world points.
fn world_pts(world: &World, name: &str) -> Vec<Point3> {
    let e = model::find_by_name(world, name).expect(name);
    let loc = model::transform_of(world, &e).unwrap_or_default();
    model::facets_of(world, &e).map(|f| f.iter().flat_map(|t| [t.a, t.b, t.c]).map(|p| transform_point(p, loc.pos, loc.rot)).collect()).unwrap_or_default()
}

fn err<T>(r: Result<T, CommandError>) -> String {
    r.err().map(|e| e.to_string()).unwrap_or_default()
}

#[test]
fn a_figure_edit_regenerates_the_block_where_it_stands_in_one_undo() {
    let (world, mut history, _plate, [fixed, _adj]) = rig();
    let before = snap(&world, &fixed);
    let mut p = jaw(&world, &fixed);
    p.jaw_height = 20.0;
    p.body_height = 30.0;
    p.insert = modvise::JawInsert::Soft;
    let cmd = SetModJaw::new(&world, &fixed, p.clone()).expect("a lawful edit");
    assert_eq!(cmd.warning(), None, "the face stays on the stock");
    history.execute(&world, cmd);
    assert_eq!(jaw(&world, &fixed), p, "the marker carries the new figures");
    let top = world_pts(&world, &fixed).iter().map(|q| q.z).fold(f64::MIN, f64::max);
    assert!((top - 30.0).abs() < 1e-9, "the solid is regenerated: top {top}");
    let after = snap(&world, &fixed);
    assert_eq!(after.1, before.1, "the block stays on its hole and turn");
    assert_eq!(after.0.plate.mount, before.0.plate.mount);
    assert!(history.undo(&world), "one undo");
    assert_eq!(snap(&world, &fixed), before, "undo is exact: marker, pose and facets");
    assert!(history.redo(&world));
    assert_eq!(snap(&world, &fixed), after, "redo is exact");
}

#[test]
fn a_figure_edit_is_refused_by_name_and_a_lifted_jaw_warns() {
    let (world, _h, plate, [fixed, adj]) = rig();
    let f = jaw(&world, &fixed);
    let a = jaw(&world, &adj);
    let side = err(SetModJaw::new(&world, &fixed, JawParams { side: modvise::JawSide::Adjustable, slot_travel: IN, ..f.clone() }));
    assert!(side.contains("is the fixed jaw block of its pair") && side.contains("keeps its side"), "{side}");
    assert!(err(SetModJaw::new(&world, &fixed, JawParams { body_length: 0.0, ..f.clone() })).contains("body length must be more than zero"));
    let off = err(SetModJaw::new(&world, &fixed, JawParams { bolts: vec![[0.0, 0.0], [1.5 * IN, 0.0]], ..f.clone() }));
    assert!(off.contains("at C14") && off.contains("between holes"), "{off}");
    // The face pushed 5 mm forward, into the stock it clamps.
    let deep = err(SetModJaw::new(&world, &fixed, JawParams { face_offset: f.face_offset + 5.0, body_length: f.body_length + 5.0, ..f.clone() }));
    assert!(deep.contains(&format!("mod vise '{fixed}''s fixed jaw 5.000 mm into the stock")), "{deep}");
    let slid = err(SetModJaw::new(&world, &adj, a.with_slide(a.slide + 2.0)));
    assert!(slid.contains("adjustable jaw 2.000 mm into the stock"), "{slid}");
    assert!(err(SetModJaw::new(&world, &plate, f.clone())).contains("is not a mod vise jaw block"));
    assert!(err(SetModJaw::new(&world, "Nope", f)).contains("Nope"));
    // Backed off 5 mm: accepted, with the gap named.
    let back = SetModJaw::new(&world, &adj, a.with_slide(a.slide - 5.0)).expect("a lifted jaw is lawful");
    let w = back.warning().unwrap_or_default();
    assert!(w.contains("adjustable jaw 5.000 mm off the stock") && w.contains("re-fit"), "{w}");
}

#[test]
fn a_refit_after_the_stock_changed_re_picks_the_hole_and_slide_in_one_undo() {
    let (world, mut history, plate, [fixed, adj]) = rig();
    let fixed_before = snap(&world, &fixed);
    // Three depths of stock, its near side kept on the fixed jaw (y -30).
    for (dy, want) in [(40.0, "H16"), (100.0, "J16"), (60.0, "I16")] {
        restock(&world, &mut history, dy, Point3::new(0.0, dy / 2.0 - 30.0, 0.0));
        let before = snap(&world, &adj);
        let cmd = RefitModVise::to_partner(&world, &adj).unwrap_or_else(|e| panic!("{dy}: {e}"));
        let fit = cmd.fit().clone();
        assert_eq!((fit.fixed_at.as_str(), fit.adjustable_at.as_str()), ("C14", want), "{dy}: the holes");
        assert!((fit.opening - dy).abs() < 1e-9 && fit.fixed_gap.abs() < 1e-9, "{dy}: {fit:?}");
        history.execute(&world, cmd);
        let face = world_pts(&world, &adj).iter().map(|q| q.y).fold(f64::MAX, f64::min);
        assert!((face - (dy - 30.0)).abs() < 1e-6, "{dy}: the adjustable jaw touches the far side: {face}");
        let j = jaw(&world, &adj);
        assert!((j.slide - fit.slide).abs() < 1e-12 && j.slide <= j.slot_travel, "{dy}: the marker's slide");
        assert_eq!(def(&world, &adj).plate.mount.map(|m| (m.plate, m.at, m.yaw_deg)), Some((plate.clone(), MountAt::Hole(want.into()), 180.0)));
        assert_eq!(snap(&world, &fixed), fixed_before, "{dy}: the fixed block stays");
        let after = snap(&world, &adj);
        assert!(history.undo(&world), "one undo");
        assert_eq!(snap(&world, &adj), before, "{dy}: undo is exact");
        assert!(history.redo(&world));
        assert_eq!(snap(&world, &adj), after, "{dy}: redo is exact");
    }
}

#[test]
fn a_refit_that_cannot_fit_is_refused_by_name_and_nothing_moves() {
    let (world, mut history, _plate, [fixed, adj]) = rig();
    let before = snap(&world, &adj);
    // The stock pushed 5 mm into the fixed jaw.
    restock(&world, &mut history, 60.0, Point3::new(0.0, -5.0, 0.0));
    let e = err(RefitModVise::to_partner(&world, &adj));
    assert!(e.contains(&format!("mod vise '{adj}' not re-fitted")) && e.contains("into the fixed jaw block at C14"), "{e}");
    // Beside the jaws.
    restock(&world, &mut history, 60.0, Point3::new(200.0, 0.0, 0.0));
    assert!(err(RefitModVise::to_partner(&world, &adj)).contains("not between the jaws"));
    assert!(err(RefitModVise::new(&world, &adj, &adj)).contains("not the fixed jaw block"));
    assert!(err(RefitModVise::new(&world, &fixed, &fixed)).contains("only the adjustable jaw block is re-fitted"));
    assert_eq!(snap(&world, &adj), before, "nothing moved");
}

/// The #833 review's probe: the stock moved 130 mm along X (beside the
/// jaws, so no face is in front of it); one plate edit (pitch X +10,
/// pitch Y -2) puts the adjustable jaw back in front of it, 16 mm in.
/// A face that was not in front before was clear: refused by name.
#[test]
fn a_plate_edit_that_brings_a_jaw_into_a_stock_it_was_beside_is_refused() {
    let (world, mut history, plate, [_fixed, adj]) = rig();
    restock(&world, &mut history, 60.0, Point3::new(130.0, 0.0, 0.0));
    let mut p = plate_of(&world, &plate).ok().and_then(|d| d.params).expect("built-in");
    p.pitch_x += 10.0;
    p.pitch_y -= 2.0;
    let e = err(SetPlate::new(&world, &plate, p));
    assert!(e.contains(&format!("mod vise '{adj}''s adjustable jaw 16.000 mm into the stock")), "{e}");
}

/// An edited and re-fitted pair saves and loads as it stands: the new
/// figures, the new hole and slide (no new file key - `mod_jaw` and
/// `on_plate` carry it).
#[test]
fn an_edited_and_re_fitted_pair_survives_a_save() {
    let (world, mut history, _plate, [fixed, adj]) = rig();
    let mut f = jaw(&world, &fixed);
    f.jaw_height = 22.0;
    history.execute(&world, SetModJaw::new(&world, &fixed, f).expect("edit"));
    restock(&world, &mut history, 40.0, Point3::new(0.0, -10.0, 0.0));
    history.execute(&world, RefitModVise::to_partner(&world, &adj).expect("refit"));
    let path = std::env::temp_dir().join(format!("camcraft-modvise-832-{}.camproj", std::process::id()));
    camcraft_app::project::save(&world, &path).expect("save");
    let back = new_world();
    let loaded = camcraft_app::project::load(&back, &path);
    let _ = std::fs::remove_file(&path);
    loaded.expect("load");
    for n in [&fixed, &adj] {
        assert_eq!(def(&back, n).mod_jaw, def(&world, n).mod_jaw, "{n}: the block reloads");
        assert_eq!(def(&back, n).plate.mount, def(&world, n).plate.mount, "{n}: its place reloads");
    }
    assert_eq!(def(&back, &adj).plate.mount.map(|m| m.at), Some(MountAt::Hole("H16".into())));
    assert!((jaw(&back, &fixed).jaw_height - 22.0).abs() < 1e-12);
}
