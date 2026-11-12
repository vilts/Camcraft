//! Built-in vises (#788): the size table's rows build closed, outward
//! shells of the stated dimensions; nonsense parameters are refused by
//! name; a vise fitted to the stock opens to it and follows it; a placed
//! 6 in vise convicts a pass that crosses its jaws and clears one over
//! them; placing and editing are one undo unit each; the parameters
//! survive a save and load. Review of PR #802: each jaw's body runs
//! `jaw_body_depth` back along the screw (steel behind the plate is
//! struck), undoing `SetVise` restores the old facets, and a nonsense
//! `vise` key on load is dropped, logged, and the steel kept.
//!
//! Gate map (M4.4): covers app::vise, app::commands::builtin_vise,
//! app::commands::insert_fixture, app::project (fixture vise key),
//! app::fixture, sim::fixture, app::posting.

use std::collections::HashMap;
use std::io::Write;
use std::sync::{Arc, Mutex};

use camcraft_app::commands::{
    AddTool, CreateOperation, GenerateOperation, History, ImportModel, InsertVise, SetStock, SetVise, TagFixture,
    VisePlacement,
};
use camcraft_app::fixture;
use camcraft_app::fixture_view::{self, MeshState};
use camcraft_app::geometry::{IDENTITY_Q, Mesh, Point3, Triangle};
use camcraft_app::model;
use camcraft_app::operation::{Height, OpParams, SrcRef};
use camcraft_app::posting;
use camcraft_app::project;
use camcraft_app::schema::Location;
use camcraft_app::stock::StockShape;
use camcraft_app::tools::catalog;
use camcraft_app::vise::{self, ViseParams};
use camcraft_app::world::new_world;
use camcraft_sim::fixture_mesh::FixtureMesh;

const IN: f64 = 25.4;

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

fn key(p: Point3) -> [u64; 3] {
    [p.x.to_bits(), p.y.to_bits(), p.z.to_bits()]
}

/// Closed and consistently oriented: every directed edge is met once by
/// its reverse, and no edge is used twice in one direction.
fn assert_watertight(tris: &[Triangle], what: &str) {
    let mut edges: HashMap<([u64; 3], [u64; 3]), i32> = HashMap::new();
    for t in tris {
        for (a, b) in [(t.a, t.b), (t.b, t.c), (t.c, t.a)] {
            *edges.entry((key(a), key(b))).or_default() += 1;
        }
    }
    for ((a, b), n) in &edges {
        assert_eq!(*n, 1, "{what}: a directed edge used {n} times");
        assert_eq!(edges.get(&(*b, *a)), Some(&1), "{what}: an edge with no reverse (open shell)");
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

/// The profile's area by its parts: base, two jaws, the housing step.
fn expected_volume(p: &ViseParams) -> f64 {
    let base = p.length * p.base_height;
    let jaws = 2.0 * p.jaw_body_depth * p.jaw_height;
    let housing = p.housing_length * p.housing_height;
    (base + jaws + housing) * p.width
}

#[test]
fn every_table_row_builds_a_closed_shell_of_its_dimensions() {
    let labels: Vec<&str> = vise::SIZES.iter().map(|s| s.label).collect();
    assert_eq!(
        labels,
        ["3 in", "4 in", "5 in", "6 in", "8 in", "100 mm", "125 mm", "150 mm", "160 mm", "200 mm"],
        "the issue's ten sizes"
    );
    for row in vise::SIZES {
        for frac in [0.01, 0.5, 1.0] {
            let p = row.params(frac * row.max_opening);
            p.check().unwrap_or_else(|e| panic!("{} @ {frac}: {e}", row.label));
            let tris = p.mesh().expect("meshes");
            let what = format!("{} @ {frac}", row.label);
            assert_watertight(&tris, &what);
            let v = volume(&tris);
            let want = expected_volume(&p);
            assert!((v - want).abs() < 1e-6 * want, "{what}: outward volume {v} vs {want}");
            let (lo, hi) = bounds(&tris);
            let close = |a: f64, b: f64| (a - b).abs() < 1e-9;
            assert!(close(lo[0], -0.5 * p.width) && close(hi[0], 0.5 * p.width), "{what}: width");
            assert!(close(hi[1], p.jaw_body_depth) && close(lo[1], p.jaw_body_depth - p.length), "{what}: length");
            assert!(close(lo[2], -p.base_height) && close(hi[2], p.jaw_height), "{what}: height");
            assert!(FixtureMesh::build(&tris).is_ok(), "{what}: the mesh law takes it");
        }
    }
}

#[test]
fn the_table_keeps_the_issues_starting_figures() {
    let row = |l: &str| vise::size(l).copied().expect(l);
    assert!((row("3 in").jaw_height - 1.3125 * IN).abs() < 1e-9);
    assert!((4.75 * IN..=5.0 * IN).contains(&row("3 in").max_opening));
    assert!((row("4 in").jaw_height - 1.25 * IN).abs() < 1e-9);
    assert!((row("4 in").max_opening - 6.0 * IN).abs() < 1e-9);
    assert!((row("6 in").jaw_height - 1.75 * IN).abs() < 1e-9);
    assert!((5.5 * IN..=9.0 * IN).contains(&row("6 in").max_opening));
    assert!((row("8 in").jaw_height - 2.0 * IN).abs() < 1e-9);
    assert!(row("8 in").max_opening >= 8.25 * IN - 1e-9);
    for r in vise::SIZES {
        let w = if r.label.ends_with("in") { r.label.trim_end_matches(" in").parse::<f64>().map(|v| v * IN) } else { r.label.trim_end_matches(" mm").parse::<f64>() };
        assert_eq!(w.ok(), Some(r.jaw_width), "{}: the jaw width is the label", r.label);
    }
    assert!(vise::size("6IN").is_some(), "the lookup is case- and space-blind");
}

#[test]
fn nonsense_parameters_are_refused_by_name() {
    let six = vise::size("6 in").expect("6 in").half_open();
    let refused = |p: ViseParams, word: &str| {
        let e = p.mesh().expect_err(word);
        assert!(e.contains(word), "'{e}' names {word}");
    };
    refused(six.with_opening(six.max_opening + 0.01), "over the max opening");
    refused(six.with_opening(0.0), "jaw opening must be more than zero");
    refused(six.with_opening(f64::NAN), "jaw opening must be a finite length");
    refused(ViseParams { jaw_height: f64::INFINITY, ..six.clone() }, "jaw height");
    refused(ViseParams { base_height: -1.0, ..six.clone() }, "base height");
    refused(ViseParams { width: six.jaw_width - 1.0, ..six.clone() }, "narrower than its jaw width");
    refused(ViseParams { length: 100.0, ..six.clone() }, "overall length");
    refused(ViseParams { jaw_body_depth: six.jaw_thickness - 1.0, ..six.clone() }, "less than its jaw thickness");
    refused(ViseParams { jaw_body_depth: f64::NAN, ..six.clone() }, "jaw body depth");
    let world = new_world();
    let at = VisePlacement::At(Location::default());
    assert!(InsertVise::new(&world, &six.with_opening(-3.0), None, at).is_err(), "the command refuses too");
    assert!(InsertVise::new(&world, &six, None, VisePlacement::FitStock).is_err(), "no stock to fit to");
}

/// 60 x 60 stock `dz` tall, a 4 mm end mill, a face pass `cut` high.
fn face_world(dz: f64, cut: f64) -> (flecs_ecs::prelude::World, History) {
    let world = new_world();
    let mut history = History::default();
    history.execute(&world, SetStock::new(&world, StockShape::Box { dx: 60.0, dy: 60.0, dz }, Location::default()).expect("stock"));
    let tool = catalog().into_iter().find(|(n, _)| *n == "EM-0400-2FL").map(|(_, t)| t).expect("catalog");
    history.execute(&world, AddTool::new(&world, "EM-0400-2FL", tool).expect("tool"));
    let params = OpParams::Face {
        holes: Default::default(),
        tilt: None,
        blend_turns: false,
        stepover: 3.0,
        clearance_z: Height::fixed(dz + 5.0),
        cut_z: Height::fixed(cut),
        top_z: None,
        stepdown: None,
        feed: 1200.0,
        plunge_feed: 300.0,
        climb: true,
    };
    history.execute(&world, CreateOperation::new(&world, "Face1", "EM-0400-2FL", SrcRef::Stock, params).expect("op"));
    history.execute(&world, GenerateOperation::new(&world, "Face1").expect("generate"));
    (world, history)
}

fn six_fitted(world: &flecs_ecs::prelude::World, history: &mut History) -> String {
    let six = vise::size("6 in").expect("6 in").half_open();
    let cmd = InsertVise::new(world, &six, None, VisePlacement::FitStock).expect("fits");
    let name = cmd.name().to_owned();
    history.execute(world, cmd);
    name
}

fn struck(world: &flecs_ecs::prelude::World) -> bool {
    posting::verification_gate(world).iter().any(|p| p.starts_with("Face1:") && p.contains("strikes a fixture"))
}

#[test]
fn a_placed_six_inch_vise_convicts_a_pass_across_its_jaws_and_clears_one_over_them() {
    let (world, mut history) = face_world(20.0, 19.5);
    assert!(posting::verification_gate(&world).is_empty(), "a clean world posts");
    let name = six_fitted(&world, &mut history);
    assert_eq!(name, "Vise 6 in");
    let states = fixture_view::mesh_states(&world, None);
    assert_eq!(states, [(name.clone(), MeshState::Mesh)], "judged by its mesh, not a box");
    assert!(struck(&world), "the pass runs into jaws standing 1.75 in over the floor");

    let (world, mut history) = face_world(60.0, 59.5);
    six_fitted(&world, &mut history);
    let problems = posting::verification_gate(&world);
    assert!(problems.is_empty(), "the pass runs 15 mm over the jaws: {problems:?}");
}

#[test]
fn the_jaw_opening_follows_the_stock() {
    let (world, mut history) = face_world(20.0, 19.5);
    let name = six_fitted(&world, &mut history);
    let e = model::find_by_name(&world, &name).expect("placed");
    let def = fixture::def_of(&world, &e).expect("a fixture");
    let p = def.vise.expect("a built-in vise");
    assert!((p.opening - 60.0).abs() < 1e-9, "opened to the stock's 60 mm: {}", p.opening);
    let loc = model::transform_of(&world, &e).expect("pose");
    assert_eq!([loc.pos.x, loc.pos.y, loc.pos.z], [0.0, 30.0, 0.0], "fixed jaw on the stock's +y face, floor under it");
    let b = model::entity_world_aabb(&world, &e).expect("box");
    assert!((b.max.z - 1.75 * IN).abs() < 1e-9, "jaws stand 1.75 in over the floor");

    // The stock grows to 80 mm across: the jaws re-open to it.
    history.execute(&world, SetStock::new(&world, StockShape::Box { dx: 60.0, dy: 100.0, dz: 20.0 }, Location { pos: Point3::new(0.0, -20.0, 0.0), rot: IDENTITY_Q }).expect("stock"));
    let stock = model::stock_world_aabb(&world).expect("stock box");
    assert!((stock.max.y - 30.0).abs() < 1e-9, "still against the fixed jaw: {stock:?}");
    history.execute(&world, SetVise::opening_from_stock(&world, &name).expect("re-open"));
    let p = fixture::def_of(&world, &e).and_then(|d| d.vise).expect("vise");
    assert!((p.opening - 100.0).abs() < 1e-9, "{}", p.opening);
    let tris = model::world_facets(&world, &e).expect("facets");
    let (lo, _) = bounds(&tris);
    assert!(lo[1] < stock.min.y - p.jaw_thickness + 1e-9, "the moving jaw stands beyond the stock");

    // Stock wider than the vise opens, or reaching into the fixed jaw: refused by name.
    history.execute(&world, SetStock::new(&world, StockShape::Box { dx: 60.0, dy: 300.0, dz: 20.0 }, Location { pos: Point3::new(0.0, -120.0, 0.0), rot: IDENTITY_Q }).expect("stock"));
    let e1 = SetVise::opening_from_stock(&world, &name).err().map(|e| e.to_string()).unwrap_or_default();
    assert!(e1.contains("max opening"), "{e1}");
    history.execute(&world, SetStock::new(&world, StockShape::Box { dx: 60.0, dy: 60.0, dz: 20.0 }, Location { pos: Point3::new(0.0, 10.0, 0.0), rot: IDENTITY_Q }).expect("stock"));
    let e2 = SetVise::opening_from_stock(&world, &name).err().map(|e| e.to_string()).unwrap_or_default();
    assert!(e2.contains("into the fixed jaw"), "{e2}");
    let fresh = new_world();
    SetStock::new(&fresh, StockShape::Box { dx: 60.0, dy: 300.0, dz: 20.0 }, Location::default())
        .map(|c| History::default().execute(&fresh, c))
        .expect("wide stock");
    let wide = InsertVise::new(&fresh, &vise::size("6 in").expect("6").half_open(), None, VisePlacement::FitStock);
    assert!(wide.err().is_some_and(|e| e.to_string().contains("opens")), "a fit wider than the max is refused");
}

#[test]
fn placing_and_editing_are_one_undo_each_and_survive_a_save() {
    let (world, mut history) = face_world(20.0, 19.5);
    let name = six_fitted(&world, &mut history);
    let e = model::find_by_name(&world, &name).expect("placed");
    let rev = model::rev_of(&world, &e);
    history.execute(&world, SetVise::opening(&world, &name, 120.0).expect("open"));
    let opening = |w: &flecs_ecs::prelude::World| {
        model::find_by_name(w, &name).and_then(|e| fixture::def_of(w, &e)).and_then(|d| d.vise).map(|v| v.opening)
    };
    assert_eq!(opening(&world), Some(120.0));
    assert!(model::rev_of(&world, &e) > rev, "the geometry revision moves");

    let path = std::env::temp_dir().join(format!("camcraft-builtin-vise-{}.camproj", std::process::id()));
    project::save(&world, &path).expect("save");
    let text = std::fs::read_to_string(&path).expect("read");
    assert!(text.contains("\"vise\"") && text.contains("\"6 in\""), "the marker carries the parameters");
    let back = new_world();
    project::load(&back, &path).expect("load");
    let _ = std::fs::remove_file(&path);
    assert_eq!(opening(&back), Some(120.0), "the parameters reload");
    assert!(SetVise::opening(&back, &name, 80.0).is_ok(), "and stay editable");

    assert!(history.undo(&world));
    assert_eq!(opening(&world), Some(60.0), "one undo takes the edit");
    assert_eq!(model::rev_of(&world, &e), rev);
    assert!(history.undo(&world));
    assert!(model::find_by_name(&world, &name).is_none(), "one undo removes the vise");
    assert!(history.redo(&world));
    assert_eq!(opening(&world), Some(60.0), "redo places it again");

    // An imported fixture is not a built-in vise.
    history.execute(&world, ImportModel::from_mesh("Plate", Mesh::from_facets(vise::size("3 in").expect("3").half_open().mesh().expect("mesh"))));
    history.execute(&world, TagFixture::new(&world, "Plate", None).expect("tag"));
    let e = SetVise::opening(&world, "Plate", 10.0).err().map(|e| e.to_string()).unwrap_or_default();
    assert!(e.contains("not a built-in vise"), "{e}");
}

/// Review of PR #802: steel behind either jaw plate, below jaw height, is
/// in the solid - a tool there is struck, never passed as air.
#[test]
fn steel_behind_a_jaw_plate_below_jaw_height_is_struck() {
    for row in vise::SIZES {
        let p = row.half_open();
        let m = FixtureMesh::build(&p.mesh().expect("mesh")).expect("the mesh law takes it");
        let z = 0.5 * p.jaw_height;
        for (side, y) in [("fixed", 1.5 * p.jaw_thickness), ("moving", -p.opening - 1.5 * p.jaw_thickness)] {
            let what = format!("{} {side} jaw", row.label);
            assert!(m.contains(Point3::new(0.0, y, z)), "{what}: solid behind its plate");
            let (p0, p1) = (Point3::new(-p.width, y, z), Point3::new(p.width, y, z));
            assert!(m.band_meets(p0, p1, 0.0, 50.0, 2.0, 0.0), "{what}: a 4 mm tool crossing behind it is struck");
        }
    }
}

#[test]
fn undoing_set_vise_restores_the_old_facets() {
    let (world, mut history) = face_world(20.0, 19.5);
    let name = six_fitted(&world, &mut history);
    let e = model::find_by_name(&world, &name).expect("placed");
    let before = model::facets_of(&world, &e).expect("facets").to_vec();
    history.execute(&world, SetVise::opening(&world, &name, 120.0).expect("open"));
    let opened = model::facets_of(&world, &e).expect("facets").to_vec();
    assert_ne!(opened, before, "the edit regenerates the mesh");
    assert!(history.undo(&world));
    assert_eq!(model::facets_of(&world, &e).expect("facets").to_vec(), before, "undo puts the old facets back");
    assert!(history.redo(&world));
    assert_eq!(model::facets_of(&world, &e).expect("facets").to_vec(), opened, "redo the new ones");
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

/// Every `vise` object in a saved project, its opening set to `v`.
fn spoil_vise(j: &mut serde_json::Value, v: f64) -> usize {
    match j {
        serde_json::Value::Object(o) => {
            let mut n = 0;
            if let Some(serde_json::Value::Object(vise)) = o.get_mut("vise") {
                vise.insert("opening".into(), serde_json::json!(v));
                n += 1;
            }
            n + o.values_mut().map(|c| spoil_vise(c, v)).sum::<usize>()
        }
        serde_json::Value::Array(a) => a.iter_mut().map(|c| spoil_vise(c, v)).sum(),
        _ => 0,
    }
}

#[test]
fn a_nonsense_vise_key_is_dropped_on_load_logged_and_the_steel_kept() {
    let (world, mut history) = face_world(20.0, 19.5);
    let name = six_fitted(&world, &mut history);
    let path = std::env::temp_dir().join(format!("camcraft-vise-spoiled-{}.camproj", std::process::id()));
    project::save(&world, &path).expect("save");
    let mut j: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&path).expect("read")).expect("json");
    assert_eq!(spoil_vise(&mut j, 1.0e6), 1, "one vise key in the file");
    std::fs::write(&path, j.to_string()).expect("write");

    let back = new_world();
    let out = Captured::default();
    let sub = tracing_subscriber::fmt().with_writer(out.clone()).with_ansi(false).finish();
    let loaded = tracing::subscriber::with_default(sub, || project::load(&back, &path));
    let _ = std::fs::remove_file(&path);
    loaded.expect("the file still loads");
    let log = out.0.lock().map(|v| String::from_utf8_lossy(&v).into_owned()).unwrap_or_default();
    assert!(log.contains("vise parameters dropped on load") && log.contains("over the max opening"), "logged: {log}");
    let e = model::find_by_name(&back, &name).expect("the fixture is kept");
    let def = fixture::def_of(&back, &e).expect("still a fixture");
    assert!(def.vise.is_none(), "but no longer a built-in vise");
    assert!(model::facets_of(&back, &e).is_some_and(|f| !f.is_empty()), "its saved steel stays");
    let refused = SetVise::opening(&back, &name, 80.0).err().map(|e| e.to_string()).unwrap_or_default();
    assert!(refused.contains("not a built-in vise"), "{refused}");
}
