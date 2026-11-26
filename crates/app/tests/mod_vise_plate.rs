//! Mod vises on their plate (#820): both jaw blocks are riders of the
//! plate - moving it carries both in its one command (one undo puts all
//! three back), deleting it lets go of both (they stay where they
//! stand); the blocks are judged by the real posting gate; and a pair
//! survives a save and load, a `mod_jaw` key this build can't read (or
//! that breaks the block law) dropped and logged, never a failed load. A
//! plate edit (`SetPlate`) that would re-seat a block's jaw into the
//! stock is refused by name with the depth (#820 review).
//!
//! Gate map (M4.4): covers app::commands::mod_vise (PlaceModVisePair
//! with app::commands::fixture MoveFixture/DeleteFixture,
//! app::commands::fixture_plate SetPlate), app::modvise::reach,
//! app::project::modvise_dto, app::posting (verification_gate).

use std::io::Write;
use std::sync::{Arc, Mutex};

use camcraft_app::commands::{
    AddTool, CreateOperation, DeleteFixture, GenerateOperation, History, InsertPlate, MoveFixture, PlaceModVisePair, PlatePlacement,
    SetPlate, SetStock,
};
use camcraft_app::fixture;
use camcraft_app::geometry::{IDENTITY_Q, Point3};
use camcraft_app::model;
use camcraft_app::modvise::{self, JawParams};
use camcraft_app::operation::{Height, OpParams, SrcRef};
use camcraft_app::plate;
use camcraft_app::posting;
use camcraft_app::project;
use camcraft_app::schema::{FixtureDef, Location};
use camcraft_app::stock::StockShape;
use camcraft_app::tools::catalog;
use camcraft_app::world::new_world;
use flecs_ecs::prelude::World;

/// The 1 in preset pair, both blocks `tall` high when given.
fn pair(tall: Option<f64>) -> (JawParams, JawParams) {
    let row = modvise::preset("1 in grid 3/8-16").expect("preset");
    let (mut f, mut a) = (row.fixed(), row.adjustable());
    if let Some(h) = tall {
        for b in [&mut f, &mut a] {
            b.body_height = h;
            b.jaw_height = h;
        }
    }
    (f, a)
}

/// 60 x 60 x 20 stock (centred, z 0..20), and when `cut` is given a
/// 4 mm end mill facing it at that height; a 770MX plate under it laid
/// so the fixed block at C14 has its face on the stock's -Y side; the
/// pair placed. Returns the plate and the block names.
fn rig(cut: Option<f64>, tall: Option<f64>) -> (World, History, String, [String; 2]) {
    let world = new_world();
    let mut history = History::default();
    history.execute(&world, SetStock::new(&world, StockShape::Box { dx: 60.0, dy: 60.0, dz: 20.0 }, Location::default()).expect("stock"));
    if let Some(cut) = cut {
        let tool = catalog().into_iter().find(|(n, _)| *n == "EM-0400-2FL").map(|(_, t)| t).expect("catalog");
        history.execute(&world, AddTool::new(&world, "EM-0400-2FL", tool).expect("tool"));
        let params = OpParams::Face {
            holes: Default::default(),
            tilt: None,
            blend_turns: false, blend_radius: None,
            stepover: 3.0,
            clearance_z: Height::fixed(25.0),
            cut_z: Height::fixed(cut),
            top_z: None,
            stepdown: None,
            feed: 1200.0,
            plunge_feed: 300.0,
            climb: true,
            picks: Default::default(),
        };
        history.execute(&world, CreateOperation::new(&world, "Face1", "EM-0400-2FL", SrcRef::Stock, params).expect("op"));
        history.execute(&world, GenerateOperation::new(&world, "Face1").expect("generate"));
    }
    let (f, a) = pair(tall);
    let p = plate::preset("770MX table 28 x 15 in").map(plate::PlatePreset::params).expect("preset");
    let (hx, hy) = p.grid().locate("C14").expect("hole");
    // Anchor: the stock's near side less the face offset, the body centred.
    let anchor = (-f.centre_x(), -30.0 - f.face_offset);
    let at = Location { pos: Point3::new(anchor.0 - hx, anchor.1 - hy, 0.0), rot: IDENTITY_Q };
    let cmd = InsertPlate::new(&world, &p, None, PlatePlacement::At(at)).expect("plate");
    let plate = cmd.name().to_owned();
    history.execute(&world, cmd);
    let cmd = PlaceModVisePair::new(&world, &plate, &f, &a, "C14", 0.0).expect("pair");
    assert_eq!((cmd.fit().fixed_at.as_str(), cmd.fit().adjustable_at.as_str()), ("C14", "I16"), "the reviewer's rig");
    let names = [cmd.names().0.to_owned(), cmd.names().1.to_owned()];
    history.execute(&world, cmd);
    (world, history, plate, names)
}

fn pose(world: &World, name: &str) -> Point3 {
    model::find_by_name(world, name).and_then(|e| model::transform_of(world, &e)).expect(name).pos
}

fn def(world: &World, name: &str) -> FixtureDef {
    model::find_by_name(world, name).and_then(|e| fixture::def_of(world, &e)).expect(name)
}

fn close(a: Point3, b: Point3) -> bool {
    (a.x - b.x).abs() + (a.y - b.y).abs() + (a.z - b.z).abs() < 1e-6
}

#[test]
fn moving_the_plate_carries_both_blocks_in_one_undo() {
    let (world, mut history, plate, names) = rig(None, None);
    let before = names.clone().map(|n| pose(&world, &n));
    let plate_at = pose(&world, &plate);
    let to = Location { pos: Point3::new(plate_at.x + 40.0, plate_at.y - 15.0, 5.0), rot: IDENTITY_Q };
    let mv = MoveFixture::new(&world, &plate, to).expect("move");
    let mut carried: Vec<&str> = mv.carried().collect();
    carried.sort_unstable();
    let mut want: Vec<&str> = names.iter().map(String::as_str).collect();
    want.sort_unstable();
    assert_eq!(carried, want, "both blocks ride the plate");
    history.execute(&world, mv);
    for (n, b) in names.iter().zip(before) {
        assert!(close(pose(&world, n), Point3::new(b.x + 40.0, b.y - 15.0, b.z + 5.0)), "{n} moved with the plate");
    }
    assert!(history.undo(&world), "one undo");
    for (n, b) in names.iter().zip(before) {
        assert!(close(pose(&world, n), b), "{n} back");
    }
    assert!(close(pose(&world, &plate), plate_at));
}

#[test]
fn deleting_the_plate_lets_go_of_both_blocks() {
    let (world, mut history, plate, names) = rig(None, None);
    let before = names.clone().map(|n| pose(&world, &n));
    history.execute(&world, DeleteFixture::new(&world, &plate).expect("delete"));
    for (n, b) in names.iter().zip(before) {
        assert!(def(&world, n).plate.mount.is_none(), "{n} taken off the gone plate");
        assert!(close(pose(&world, n), b), "{n} stays where it stands");
        assert!(def(&world, n).mod_jaw.is_some(), "{n} is still a jaw block");
    }
    assert!(history.undo(&world));
    for n in &names {
        assert_eq!(def(&world, n).plate.mount.map(|m| m.plate), Some(plate.clone()), "{n} back on the plate");
    }
}

fn struck(world: &World) -> Vec<String> {
    posting::verification_gate(world).into_iter().filter(|p| p.starts_with("Face1:") && p.contains("strikes a fixture")).collect()
}

#[test]
fn the_blocks_are_judged_by_the_posting_gate() {
    // Jaws 31.75 mm tall beside the stock: facing at 19.5 runs into them.
    let (world, _h, _p, _n) = rig(Some(19.5), None);
    assert!(!struck(&world).is_empty(), "the pass strikes the jaws: {:?}", posting::verification_gate(&world));
    // Blocks 15 mm tall: the same pass runs over them.
    let (world, _h, _p, _n) = rig(Some(19.5), Some(15.0));
    assert!(struck(&world).is_empty(), "{:?}", posting::verification_gate(&world));
}

#[derive(Clone, Default)]
struct Captured(Arc<Mutex<Vec<u8>>>);

impl Write for Captured {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        if let Ok(mut v) = self.0.lock() {
            v.extend_from_slice(buf);
        }
        Ok(buf.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl<'a> tracing_subscriber::fmt::MakeWriter<'a> for Captured {
    type Writer = Captured;
    fn make_writer(&'a self) -> Self::Writer {
        self.clone()
    }
}

/// Replace every `key` value in `j` with `with`.
fn spoil(j: &mut serde_json::Value, key: &str, with: &serde_json::Value) -> usize {
    match j {
        serde_json::Value::Object(o) => {
            let mut n = 0;
            if let Some(v) = o.get_mut(key) {
                *v = with.clone();
                n += 1;
            }
            n + o.values_mut().map(|c| spoil(c, key, with)).sum::<usize>()
        }
        serde_json::Value::Array(a) => a.iter_mut().map(|c| spoil(c, key, with)).sum(),
        _ => 0,
    }
}

fn load_logged(path: &std::path::Path) -> (World, String) {
    let w = new_world();
    let out = Captured::default();
    let sub = tracing_subscriber::fmt().with_writer(out.clone()).with_ansi(false).finish();
    let loaded = tracing::subscriber::with_default(sub, || project::load(&w, path));
    loaded.expect("the file still loads");
    let log = out.0.lock().map(|v| String::from_utf8_lossy(&v).into_owned()).unwrap_or_default();
    (w, log)
}

#[test]
fn a_pair_survives_a_save_and_unreadable_keys_are_dropped() {
    let (world, _h, plate, names) = rig(None, None);
    let path = std::env::temp_dir().join(format!("camcraft-modvise-{}.camproj", std::process::id()));
    project::save(&world, &path).expect("save");
    let text = std::fs::read_to_string(&path).expect("read");
    let back = new_world();
    project::load(&back, &path).expect("load");
    for n in &names {
        assert_eq!(def(&back, n).mod_jaw, def(&world, n).mod_jaw, "{n}: the block reloads");
        assert_eq!(def(&back, n).plate.mount, def(&world, n).plate.mount, "{n}: its place reloads");
    }
    let mv = MoveFixture::new(&back, &plate, Location { pos: Point3::new(5.0, 0.0, 0.0), rot: IDENTITY_Q }).expect("move");
    assert_eq!(mv.carried().count(), 2, "the reloaded plate still carries both");

    // A shape this build can't read, and figures that break the law.
    let mut j: serde_json::Value = serde_json::from_str(&text).expect("json");
    assert_eq!(spoil(&mut j, "mod_jaw", &serde_json::json!({ "side": "telescopic" })), 2);
    std::fs::write(&path, j.to_string()).expect("write");
    let (w, log) = load_logged(&path);
    assert!(log.contains("mod vise jaw block key not understood"), "{log}");
    for n in &names {
        assert!(def(&w, n).mod_jaw.is_none(), "{n}: dropped");
        assert!(def(&w, n).plate.mount.is_some(), "{n}: its place stays");
        assert!(model::find_by_name(&w, n).and_then(|e| model::facets_of(&w, &e)).is_some_and(|f| !f.is_empty()), "the steel stays");
    }
    let mut j: serde_json::Value = serde_json::from_str(&text).expect("json");
    let mut bad = serde_json::to_value(def(&world, &names[0]).mod_jaw).expect("value");
    bad["body_length"] = serde_json::json!(-1.0);
    assert_eq!(spoil(&mut j, "mod_jaw", &bad), 2);
    std::fs::write(&path, j.to_string()).expect("write");
    let (w, log) = load_logged(&path);
    let _ = std::fs::remove_file(&path);
    assert!(log.contains("mod vise jaw block parameters dropped on load") && log.contains("body length"), "{log}");
    assert!(def(&w, &names[0]).mod_jaw.is_none());
}

/// The rig's plate figures with pitch Y moved by `dy` (mm).
fn pitched(world: &World, plate: &str, dy: f64) -> plate::PlateParams {
    let mut p = camcraft_app::commands::fixture_plate::plate_of(world, plate).ok().and_then(|d| d.params).expect("built-in");
    p.pitch_y += dy;
    p
}

fn refused(r: Result<SetPlate, camcraft_app::commands::CommandError>) -> String {
    r.err().map(|e| e.to_string()).unwrap_or_default()
}

/// #820 review: a plate edit re-seats each block on its named hole with
/// its slide kept, but the stock stays put. Rows C and I are 2 and 8
/// pitches from the grid origin: pitch Y - 2 moves the adjustable block
/// at I16 16 mm toward the stock (its face 16 mm in); pitch Y + 2 moves
/// the fixed block at C14 4 mm toward it (its face 4 mm in).
#[test]
fn a_plate_edit_that_pushes_a_jaw_into_the_stock_is_refused_by_name() {
    let (world, _h, plate, [fixed, adjustable]) = rig(None, None);
    assert!((def(&world, &adjustable).mod_jaw.map_or(0.0, |j| j.slide) - 22.55).abs() < 0.01, "the rig's slide");
    let e = refused(SetPlate::new(&world, &plate, pitched(&world, &plate, -2.0)));
    assert!(e.contains(&format!("mod vise '{adjustable}'")) && e.contains("adjustable jaw 16.000 mm into the stock"), "{e}");
    let e = refused(SetPlate::new(&world, &plate, pitched(&world, &plate, 2.0)));
    assert!(e.contains(&format!("mod vise '{fixed}'")) && e.contains("fixed jaw 4.000 mm into the stock"), "{e}");
    assert!(e.contains("re-place the pair"), "{e}");
}

/// A pitch change across the jaws keeps both faces where they were along
/// the clamp: accepted, quiet, and undo/redo put every pose back to the bit.
#[test]
fn a_plate_edit_that_keeps_the_jaws_clear_is_accepted_and_undoes_exactly() {
    let (world, mut history, plate, names) = rig(None, None);
    let mut p = camcraft_app::commands::fixture_plate::plate_of(&world, &plate).ok().and_then(|d| d.params).expect("built-in");
    p.pitch_x += 0.2;
    let cmd = SetPlate::new(&world, &plate, p).expect("clear of the stock");
    assert_eq!(cmd.warning(), None, "no jaw in the stock, none lifted off it");
    let loc = |n: &str| model::find_by_name(&world, n).and_then(|e| model::transform_of(&world, &e)).expect(n);
    let before = names.clone().map(|n| loc(&n));
    history.execute(&world, cmd);
    let after = names.clone().map(|n| loc(&n));
    assert_ne!(before, after, "both blocks re-seated on the new grid");
    for (b, a) in before.iter().zip(&after) {
        assert!((a.pos.y - b.pos.y).abs() < 1e-9, "nothing moved along the clamp");
    }
    assert!(history.undo(&world));
    assert_eq!(names.clone().map(|n| loc(&n)), before, "undo is exact");
    assert!(history.redo(&world));
    assert_eq!(names.clone().map(|n| loc(&n)), after, "redo is exact");
    // A refused edit leaves nothing behind: the poses are still the redo's.
    let _ = SetPlate::new(&world, &plate, pitched(&world, &plate, -2.0));
    assert_eq!(names.map(|n| loc(&n)), after);
}
