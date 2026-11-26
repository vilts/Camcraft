//! Fixture plates (#819): every preset builds a watertight solid slab of
//! its exact dimensions; grid locations are named letters-by-row,
//! numbers-by-column; the 770MX preset is the machine file's table; a
//! vise placed at a grid location lands where the grid says and rides
//! the plate when it moves (one undo each); bad locations, overhangs and
//! nonsense figures are refused by name; a plate is judged by the real
//! posting gate (the imported-plate grid: `fixture_plate_detect`);
//! the keys survive a save and an unreadable key is dropped, not fatal.
//!
//! Gate map (M4.4): covers app::plate, app::commands::fixture_plate,
//! app::commands::fixture (MoveFixture carrying riders),
//! app::project (fixture plate keys), app::posting.

use std::collections::HashMap;
use std::io::Write;
use std::sync::{Arc, Mutex};

use camcraft_app::commands::{
    AddTool, CreateOperation, GenerateOperation, History, InsertPlate, InsertVise, LoadMachine, MoveFixture,
    PlaceOnPlate, PlatePlacement, SetPlate, SetStock, VisePlacement,
};
use camcraft_app::fixture;
use camcraft_app::geometry::{IDENTITY_Q, Point3, Triangle, quat_from_euler};
use camcraft_app::model;
use camcraft_app::operation::{Height, OpParams, SrcRef};
use camcraft_app::plate::{self, MountAt, PlateParams, location_name, parse_location, row_letters};
use camcraft_app::posting;
use camcraft_app::project;
use camcraft_app::schema::Location;
use camcraft_app::stock::StockShape;
use camcraft_app::tools::catalog;
use camcraft_app::vise;
use camcraft_app::world::new_world;
use camcraft_sim::fixture_mesh::FixtureMesh;
use flecs_ecs::prelude::World;

const IN: f64 = 25.4;

fn key(p: Point3) -> [u64; 3] {
    [p.x.to_bits(), p.y.to_bits(), p.z.to_bits()]
}

fn assert_watertight(tris: &[Triangle], what: &str) {
    let mut edges: HashMap<([u64; 3], [u64; 3]), i32> = HashMap::new();
    for t in tris {
        for (a, b) in [(t.a, t.b), (t.b, t.c), (t.c, t.a)] {
            *edges.entry((key(a), key(b))).or_default() += 1;
        }
    }
    for ((a, b), n) in &edges {
        assert_eq!(*n, 1, "{what}: a directed edge used {n} times");
        assert_eq!(edges.get(&(*b, *a)), Some(&1), "{what}: an open edge");
    }
}

fn volume(tris: &[Triangle]) -> f64 {
    tris.iter()
        .map(|t| {
            let (a, b, c) = (t.a, t.b, t.c);
            a.x * (b.y * c.z - b.z * c.y) - a.y * (b.x * c.z - b.z * c.x) + a.z * (b.x * c.y - b.y * c.x)
        })
        .sum::<f64>()
        / 6.0
}

fn bounds(tris: &[Triangle]) -> ([f64; 3], [f64; 3]) {
    let mut lo = [f64::MAX; 3];
    let mut hi = [f64::MIN; 3];
    for p in tris.iter().flat_map(|t| [t.a, t.b, t.c]) {
        for (k, v) in [p.x, p.y, p.z].into_iter().enumerate() {
            lo[k] = lo[k].min(v);
            hi[k] = hi[k].max(v);
        }
    }
    (lo, hi)
}

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-6
}

fn preset(label: &str) -> PlateParams {
    plate::preset(label).map(plate::PlatePreset::params).expect(label)
}

#[test]
fn every_preset_builds_a_watertight_slab_of_its_exact_dimensions() {
    let labels: Vec<&str> = plate::PRESETS.iter().map(|p| p.label).collect();
    assert_eq!(labels, ["12 x 12 in", "300 x 300 mm", "400 x 300 mm", "770MX table 28 x 15 in"]);
    for row in plate::PRESETS {
        let p = row.params();
        p.check().unwrap_or_else(|e| panic!("{}: {e}", row.label));
        let tris = p.mesh().expect("meshes");
        assert_eq!(tris.len(), 12, "{}: a solid slab, holes drawn solid", row.label);
        assert_watertight(&tris, row.label);
        let want = p.length * p.width * p.thickness;
        assert!((volume(&tris) - want).abs() < 1e-6 * want, "{}: outward volume", row.label);
        let (lo, hi) = bounds(&tris);
        assert_eq!((lo, hi), ([0.0, 0.0, -p.thickness], [p.length, p.width, 0.0]), "{}: exact bounds", row.label);
        assert!(FixtureMesh::build(&tris).is_ok(), "{}: the mesh law takes it", row.label);
    }
    let twelve = preset("12 x 12 in");
    assert!(close(twelve.length, 12.0 * IN) && close(twelve.pitch_x, IN) && twelve.thread == "3/8-16");
}

#[test]
fn grid_locations_are_letters_by_row_and_numbers_by_column() {
    for (r, s) in [(0, "A"), (1, "B"), (25, "Z"), (26, "AA"), (27, "AB"), (701, "ZZ"), (702, "AAA")] {
        assert_eq!(row_letters(r), s);
        assert_eq!(parse_location(&format!("{s}7")), Some((r, 6)), "{s}7 round-trips");
    }
    assert_eq!(location_name(1, 2), "B3");
    assert_eq!(parse_location(" b 3 "), Some((1, 2)), "case and spaces are ignored");
    for bad in ["", "3B", "B0", "B", "7", "B-3", "Ä1"] {
        assert_eq!(parse_location(bad), None, "'{bad}' is no location");
    }
    let g = preset("12 x 12 in").grid();
    assert_eq!((g.rows, g.cols), (12, 12));
    assert_eq!(g.extent(), "rows A-L, columns 1-12");
    let at = |n: &str| g.locate(n).expect(n);
    assert_eq!(at("A1"), (0.5 * IN, 0.5 * IN), "A1 is nearest the origin corner");
    assert_eq!(at("B3"), (2.5 * IN, 1.5 * IN), "B is the second row (+Y), 3 the third column (+X)");
    assert_eq!(at("L12"), (11.5 * IN, 11.5 * IN));
    let names: Vec<String> = g.holes().into_iter().map(|h| h.0).collect();
    assert_eq!(names.len(), 144);
    assert_eq!((names[0].as_str(), names[12].as_str()), ("A1", "B1"), "listed row by row");
    assert!(g.locate("M1").err().is_some_and(|e| e.contains("no location M1") && e.contains("rows A-L")));
    let dowels = preset("300 x 300 mm").dowel_grid().expect("dowels");
    assert_eq!((dowels.rows, dowels.cols, dowels.x0), (6, 6, 25.0));
}

#[test]
fn the_770mx_preset_is_its_machine_files_table_and_lies_on_it() {
    let world = new_world();
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../assets/machines/tormach-770mx.mch");
    let text = std::fs::read_to_string(path).expect("the shipped 770MX machine file");
    let m = camcraft_app::machine::parse_mch(&text).expect("parses");
    History::default().execute(&world, LoadMachine::new(&world, path.into(), Some(m)).expect("load"));
    let table = fixture::table_box(&world, None).expect("the 770MX states a table");
    let p = preset("770MX table 28 x 15 in");
    assert!(close(table.max.x - table.min.x, p.length) && close(table.max.y - table.min.y, p.width), "{table:?}");
    let cmd = InsertPlate::new(&world, &p, None, PlatePlacement::OnTable).expect("on the table");
    let name = cmd.name().to_owned();
    History::default().execute(&world, cmd);
    let b = model::entity_world_aabb(&world, &model::find_by_name(&world, &name).expect("placed")).expect("box");
    assert!(close(b.min.x, table.min.x) && close(b.max.y, table.max.y), "centred on the table: {b:?}");
    assert!(close(b.min.z, table.max.z), "lying on the table top: {b:?}");
    assert!(InsertPlate::new(&new_world(), &p, None, PlatePlacement::OnTable).is_err(), "no machine, no table");
}

fn insert_plate(world: &World, history: &mut History, p: &PlateParams, at: Point3) -> String {
    let cmd = InsertPlate::new(world, p, None, PlatePlacement::At(Location { pos: at, rot: IDENTITY_Q })).expect("plate");
    let name = cmd.name().to_owned();
    history.execute(world, cmd);
    name
}

fn six_vise(world: &World, history: &mut History) -> (String, vise::ViseParams) {
    let six = vise::size("6 in").expect("6 in").half_open();
    let at = Location { pos: Point3::new(0.0, 0.0, 500.0), rot: IDENTITY_Q };
    let cmd = InsertVise::new(world, &six, None, VisePlacement::At(at)).expect("vise");
    let name = cmd.name().to_owned();
    history.execute(world, cmd);
    (name, six)
}

fn pose(world: &World, name: &str) -> Location {
    model::find_by_name(world, name).and_then(|e| model::transform_of(world, &e)).expect(name)
}

fn same(a: Location, b: Location) -> bool {
    let d = (a.pos.x - b.pos.x).abs() + (a.pos.y - b.pos.y).abs() + (a.pos.z - b.pos.z).abs();
    let q = (0..4).map(|k| (a.rot[k] - b.rot[k]).abs()).sum::<f64>().min((0..4).map(|k| (a.rot[k] + b.rot[k]).abs()).sum());
    d < 1e-6 && q < 1e-9
}

fn mount_of(world: &World, name: &str) -> Option<plate::PlateMount> {
    model::find_by_name(world, name).and_then(|e| fixture::def_of(world, &e)).and_then(|d| d.plate.mount)
}

#[test]
fn a_vise_at_a_grid_location_lands_there_and_rides_the_plate() {
    let world = new_world();
    let mut history = History::default();
    let plate = insert_plate(&world, &mut history, &preset("770MX table 28 x 15 in"), Point3::ZERO);
    assert_eq!(plate, "Plate 770MX table 28 x 15 in");
    let (vise, six) = six_vise(&world, &mut history);
    let start = pose(&world, &vise);

    history.execute(&world, PlaceOnPlate::new(&world, &vise, &plate, MountAt::Hole("h10".into()), 90.0).expect("fits at H10"));
    let want = Location { pos: Point3::new(9.5 * IN, 7.5 * IN, six.base_height), rot: quat_from_euler(0.0, 0.0, 90f64.to_radians()) };
    assert!(same(pose(&world, &vise), want), "on H10, turned 90, its base on the plate top: {:?}", pose(&world, &vise));
    assert_eq!(mount_of(&world, &vise).map(|m| m.at), Some(MountAt::Hole("h10".into())), "the place is recorded");

    // Moving the plate moves the vise: one command, one undo.
    let moved = Location { pos: Point3::new(100.0, -50.0, 10.0), rot: quat_from_euler(0.0, 0.0, 0.5) };
    let mv = MoveFixture::new(&world, &plate, moved).expect("move");
    assert_eq!(mv.carried().collect::<Vec<_>>(), [vise.as_str()]);
    history.execute(&world, mv);
    let carried = camcraft_app::setup_align::compose(moved, want);
    assert!(same(pose(&world, &vise), carried), "it rode along: {:?}", pose(&world, &vise));
    assert!(history.undo(&world));
    assert!(same(pose(&world, &vise), want) && same(pose(&world, &plate), Location::default()), "one undo puts both back");
    assert!(history.redo(&world));
    assert!(same(pose(&world, &vise), carried));
    assert!(history.undo(&world));

    // Editing the plate re-seats the vise at H10 on the new grid.
    let mut p = preset("770MX table 28 x 15 in");
    p.offset_x = 1.0 * IN;
    history.execute(&world, SetPlate::new(&world, &plate, p).expect("edit"));
    assert!(close(pose(&world, &vise).pos.x, 10.0 * IN), "H10 moved with the grid");
    assert!(history.undo(&world));
    assert!(same(pose(&world, &vise), want));

    // Moving the vise by hand takes it off its grid location (undoable).
    history.execute(&world, MoveFixture::new(&world, &vise, start).expect("move"));
    assert!(mount_of(&world, &vise).is_none());
    assert!(history.undo(&world));
    assert!(mount_of(&world, &vise).is_some());

    // One undo takes the placement back.
    assert!(history.undo(&world));
    assert!(same(pose(&world, &vise), start) && mount_of(&world, &vise).is_none());
}

#[test]
fn bad_locations_overhangs_and_nonsense_are_refused_by_name() {
    let world = new_world();
    let mut history = History::default();
    let small = insert_plate(&world, &mut history, &preset("12 x 12 in"), Point3::ZERO);
    let (vise, _) = six_vise(&world, &mut history);
    let refused = |r: Result<PlaceOnPlate, _>| r.err().map(|e: camcraft_app::commands::CommandError| e.to_string()).unwrap_or_default();
    let hole = |s: &str| MountAt::Hole(s.into());
    let e = refused(PlaceOnPlate::new(&world, &vise, &small, hole("M1"), 0.0));
    assert!(e.contains("no location M1") && e.contains("rows A-L"), "{e}");
    let e = refused(PlaceOnPlate::new(&world, &vise, &small, hole("B0"), 0.0));
    assert!(e.contains("not a grid location"), "{e}");
    let e = refused(PlaceOnPlate::new(&world, &vise, &small, hole("F6"), 0.0));
    assert!(e.contains("hang") && e.contains("off the plate"), "an 18.5 in vise on a 12 in plate: {e}");
    let e = refused(PlaceOnPlate::new(&world, &vise, &small, MountAt::Dowels(["A1".into(), "A2".into()]), 0.0));
    assert!(e.contains("no dowel pattern"), "{e}");
    let e = refused(PlaceOnPlate::new(&world, &vise, &small, hole("F6"), f64::NAN));
    assert!(e.contains("finite"), "{e}");
    let e = refused(PlaceOnPlate::new(&world, &small, &vise, hole("A1"), 0.0));
    assert!(e.contains("not a fixture plate"), "{e}");
    let e = refused(PlaceOnPlate::new(&world, &small, &small, hole("A1"), 0.0));
    assert!(e.contains("itself"), "{e}");
    let e = refused(PlaceOnPlate::new(&world, "Nope", &small, hole("A1"), 0.0));
    assert!(e.contains("Nope"), "{e}");
    // A plate on a plate is fine; the loop back is not.
    let big = insert_plate(&world, &mut history, &preset("770MX table 28 x 15 in"), Point3::new(0.0, 0.0, -100.0));
    history.execute(&world, PlaceOnPlate::new(&world, &small, &big, hole("A1"), 0.0).expect("a subplate"));
    let e = refused(PlaceOnPlate::new(&world, &big, &small, hole("A1"), 0.0));
    assert!(e.contains("stands on"), "{e}");
    // Moving the big plate carries the subplate.
    let mv = MoveFixture::new(&world, &big, Location { pos: Point3::new(1.0, 0.0, -100.0), rot: IDENTITY_Q }).expect("move");
    assert_eq!(mv.carried().collect::<Vec<_>>(), [small.as_str()]);

    let p = preset("300 x 300 mm");
    let bad = |q: PlateParams, word: &str| {
        let e = q.check().expect_err(word);
        assert!(e.contains(word), "'{e}' names {word}");
        assert!(InsertPlate::new(&world, &q, None, PlatePlacement::At(Location::default())).is_err());
    };
    bad(PlateParams { thickness: 0.0, ..p.clone() }, "thickness");
    bad(PlateParams { pitch_x: f64::NAN, ..p.clone() }, "pitch X");
    bad(PlateParams { thread_diameter: 25.0, ..p.clone() }, "smaller than the pitch");
    bad(PlateParams { offset_x: 2.0, ..p.clone() }, "over the edge");
    bad(PlateParams { offset_y: 200.0, ..p.clone() }, "more than half the plate");
    bad(PlateParams { thread: " ".into(), ..p.clone() }, "thread needs a name");
    bad(PlateParams { pitch_x: 0.01, pitch_y: 0.01, thread_diameter: 0.005, offset_x: 0.01, offset_y: 0.01, dowels: None, ..p.clone() }, "more than 20000");
    let mut d = p.clone();
    if let Some(dw) = d.dowels.as_mut() {
        dw.offset_x = 12.5;
        dw.offset_y = 12.5;
    }
    bad(d, "runs into tapped hole A1");
    let q = preset("400 x 300 mm");
    let mut c = q.clone();
    if let Some(cb) = c.counterbore.as_mut() {
        cb.depth = 40.0;
    }
    bad(c, "counterbore depth");
    let e = SetPlate::new(&world, &vise, p).err().map(|e| e.to_string()).unwrap_or_default();
    assert!(e.contains("not a built-in plate"), "{e}");
}

/// 60 x 60 x 20 stock, a 4 mm end mill, a face pass `cut` high.
fn face_world(cut: f64) -> (World, History) {
    let world = new_world();
    let mut history = History::default();
    history.execute(&world, SetStock::new(&world, StockShape::Box { dx: 60.0, dy: 60.0, dz: 20.0 }, Location::default()).expect("stock"));
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
    (world, history)
}

fn struck(world: &World) -> bool {
    posting::verification_gate(world).iter().any(|p| p.starts_with("Face1:") && p.contains("strikes a fixture"))
}

#[test]
fn a_plate_is_judged_by_the_posting_gate() {
    // Plate top at z 20: a face pass at 19.5 runs into it.
    let (world, mut history) = face_world(19.5);
    assert!(!struck(&world));
    insert_plate(&world, &mut history, &preset("300 x 300 mm"), Point3::new(-150.0, -150.0, 20.0));
    assert!(struck(&world), "the pass runs into the plate: {:?}", posting::verification_gate(&world));
    // Plate top at z 19: the same pass runs over it.
    let (world, mut history) = face_world(19.5);
    insert_plate(&world, &mut history, &preset("300 x 300 mm"), Point3::new(-150.0, -150.0, 19.0));
    assert!(!struck(&world), "{:?}", posting::verification_gate(&world));
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

/// Replace every `key` object in `j` with `with`.
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

#[test]
fn plates_and_places_survive_a_save_and_unreadable_keys_are_dropped() {
    let world = new_world();
    let mut history = History::default();
    let plate = insert_plate(&world, &mut history, &preset("770MX table 28 x 15 in"), Point3::ZERO);
    let (vise, _) = six_vise(&world, &mut history);
    history.execute(&world, PlaceOnPlate::new(&world, &vise, &plate, MountAt::Hole("H10".into()), 90.0).expect("place"));
    let path = std::env::temp_dir().join(format!("camcraft-plate-{}.camproj", std::process::id()));
    project::save(&world, &path).expect("save");
    let text = std::fs::read_to_string(&path).expect("read");
    let back = new_world();
    project::load(&back, &path).expect("load");
    let def = |w: &World, n: &str| model::find_by_name(w, n).and_then(|e| fixture::def_of(w, &e)).expect(n);
    assert_eq!(def(&back, &plate).plate.plate, def(&world, &plate).plate.plate, "the grid reloads");
    assert_eq!(mount_of(&back, &vise), mount_of(&world, &vise), "the place reloads");
    let mv = MoveFixture::new(&back, &plate, Location { pos: Point3::new(5.0, 0.0, 0.0), rot: IDENTITY_Q }).expect("move");
    assert_eq!(mv.carried().count(), 1, "and the reloaded plate still carries its vise");

    // Keys this build can't read: dropped and logged, the load goes on.
    let mut j: serde_json::Value = serde_json::from_str(&text).expect("json");
    assert_eq!(spoil(&mut j, "plate", &serde_json::json!({ "holes": "a future shape" })), 1 + 1, "the plate key, and the place's plate name");
    assert_eq!(spoil(&mut j, "on_plate", &serde_json::json!({ "plate": "P", "at": { "teleport": "Z9" } })), 1);
    std::fs::write(&path, j.to_string()).expect("write");
    let spoiled = new_world();
    let out = Captured::default();
    let sub = tracing_subscriber::fmt().with_writer(out.clone()).with_ansi(false).finish();
    let loaded = tracing::subscriber::with_default(sub, || project::load(&spoiled, &path));
    let _ = std::fs::remove_file(&path);
    loaded.expect("the file still loads");
    let log = out.0.lock().map(|v| String::from_utf8_lossy(&v).into_owned()).unwrap_or_default();
    assert!(log.contains("fixture plate key not understood") && log.contains("fixture plate location not understood"), "{log}");
    assert!(def(&spoiled, &plate).plate.plate.is_none() && mount_of(&spoiled, &vise).is_none());
    assert!(model::find_by_name(&spoiled, &plate).and_then(|e| model::facets_of(&spoiled, &e)).is_some_and(|f| f.len() == 12), "the steel stays");
}
