//! Fixture plates (#819, follow-ups #826): a dropped `plate` / `on_plate` /
//! `mod_jaw` key or a dangling mount reaches the user's load notes (one
//! each, the file still loads); a located plate's riders are clash-checked
//! where they land; a plate whose mesh changes (`UpdateModel`, which a
//! same-name re-import, `cam.import` and `RepairModel` all ride) loses its
//! grid and its riders in the same undo step; and a staggered plate with
//! a clamp ear or chamfered corners is no longer refused as "turned".
//!
//! Gate map (M4.4): covers app::project (plate_dto, modvise_dto,
//! fixture_keys, apply), app::plate::detect (square_edges),
//! app::commands::update_model, app::commands::locate (LocateFixture),
//! app::commands::fixture_move.
//! Provenance: our own fixtures over our own file format.

use camcraft_app::commands::{
    History, ImportModel, InsertPlate, LocateFixture, MoveFixture, PlaceOnPlate, PlatePlacement, SetPlateGrid, TagFixture, UpdateModel,
};
use camcraft_app::fixture;
use camcraft_app::fixture_locate::{LocateOffset, LocateSpec, Mate};
use camcraft_app::geometry::{IDENTITY_Q, Mesh, Point3, Triangle};
use camcraft_app::model;
use camcraft_app::operation::{FeatureSel, GeoSel};
use camcraft_app::plate::{self, MountAt};
use camcraft_app::project;
use camcraft_app::schema::Location;
use camcraft_app::world::new_world;
use flecs_ecs::prelude::World;
use serde_json::{Value, json};

fn at(x: f64, y: f64, z: f64) -> Location {
    Location { pos: Point3::new(x, y, z), rot: IDENTITY_Q }
}

fn insert_plate(world: &World, history: &mut History) -> String {
    let p = plate::preset("300 x 300 mm").map(plate::PlatePreset::params).expect("preset");
    let cmd = InsertPlate::new(world, &p, None, PlatePlacement::At(at(0.0, 0.0, 0.0))).expect("plate");
    let name = cmd.name().to_owned();
    history.execute(world, cmd);
    name
}

fn block(world: &World, history: &mut History, name: &str, plate: Option<&str>) {
    history.execute(world, ImportModel::from_mesh(name, Mesh::from_facets(plate::slab(10.0, 10.0, 10.0))));
    history.execute(world, TagFixture::new(world, name, None).expect("tag"));
    if let Some(p) = plate {
        history.execute(world, PlaceOnPlate::new(world, name, p, MountAt::Hole("B2".into()), 0.0).expect("B2"));
    }
}

fn def(world: &World, name: &str) -> Option<camcraft_app::schema::FixtureDef> {
    model::find_by_name(world, name).and_then(|e| fixture::def_of(world, &e))
}

fn marker<'a>(j: &'a mut Value, name: &str) -> &'a mut serde_json::Map<String, Value> {
    let entities = j.get_mut("entities").and_then(Value::as_array_mut).expect("entities");
    let e = entities.iter_mut().find(|e| e.get("name").and_then(Value::as_str) == Some(name)).expect(name);
    e.get_mut("fixture").and_then(Value::as_object_mut).expect("a fixture marker")
}

#[test]
fn dropped_plate_keys_and_dangling_mounts_reach_the_load_notes() {
    let world = new_world();
    let mut history = History::default();
    let p = insert_plate(&world, &mut history);
    for n in ["BadPlate", "BadOnPlate", "BadJaw", "Dangling", "Fine"] {
        block(&world, &mut history, n, Some(&p));
    }
    let mut j: Value = serde_json::from_str(&serde_json::to_string(&project::collect(&world)).expect("save")).expect("json");
    marker(&mut j, "BadPlate").insert("plate".into(), json!({ "nonsense": true }));
    marker(&mut j, "BadOnPlate").insert("on_plate".into(), json!(5));
    marker(&mut j, "BadJaw").insert("mod_jaw".into(), json!({ "nonsense": true }));
    if let Some(m) = marker(&mut j, "Dangling").get_mut("on_plate").and_then(Value::as_object_mut) {
        m.insert("plate".into(), json!("Ghost plate"));
    }

    let back = new_world();
    let notes = project::open_noted(&back, &j.to_string()).expect("the file still loads");
    assert_eq!(notes.len(), 4, "one note per dropped key: {notes:?}");
    for (name, what) in [
        ("BadPlate", "plate key not understood"),
        ("BadOnPlate", "plate location not understood"),
        ("BadJaw", "mod_jaw key not understood"),
        ("Dangling", "plate location dropped on load: no such plate"),
    ] {
        let hits = notes.iter().filter(|n| n.contains(&format!("fixture '{name}'")) && n.contains(what)).count();
        assert_eq!(hits, 1, "one note for {name} ({what}): {notes:?}");
        assert!(def(&back, name).is_some_and(|d| d.mod_jaw.is_none()), "{name} has no jaw block");
        let unmounted = matches!(name, "BadOnPlate" | "Dangling");
        assert_eq!(def(&back, name).is_some_and(|d| d.plate.mount.is_none()), unmounted, "{name}'s place");
    }
    assert!(def(&back, "Fine").is_some_and(|d| d.plate.mount.is_some()), "the good rider keeps its place");
}

/// The unit normal's Z of a facet (0 when degenerate).
fn nz(t: &Triangle) -> f64 {
    let n = t.normal();
    let len = (n.x * n.x + n.y * n.y + n.z * n.z).sqrt();
    if len > 0.0 { n.z / len } else { 0.0 }
}

fn top_face(world: &World, name: &str) -> GeoSel {
    let e = model::find_by_name(world, name).expect("entity");
    let f = model::features_of(world, &e).expect("features");
    let tris = model::facets_of(world, &e).expect("facets");
    let id = f
        .faces
        .iter()
        .filter(|f| f.facets.iter().all(|&i| nz(&tris[i as usize]) > 0.99))
        .max_by_key(|f| f.facets.len())
        .map(|f| f.id)
        .expect("a top face");
    GeoSel { entity: name.into(), feature: FeatureSel::Face(id) }
}

fn bottom_face(world: &World, name: &str) -> GeoSel {
    let e = model::find_by_name(world, name).expect("entity");
    let f = model::features_of(world, &e).expect("features");
    let tris = model::facets_of(world, &e).expect("facets");
    let id = f
        .faces
        .iter()
        .filter(|f| f.facets.iter().all(|&i| nz(&tris[i as usize]) < -0.99))
        .max_by_key(|f| f.facets.len())
        .map(|f| f.id)
        .expect("a bottom face");
    GeoSel { entity: name.into(), feature: FeatureSel::Face(id) }
}

/// A located plate carries its riders: where they land inside the part
/// they are warned about by name, like a `MoveFixture` does.
#[test]
fn a_located_plates_riders_are_clash_checked() {
    let world = new_world();
    let mut history = History::default();
    let p = insert_plate(&world, &mut history);
    block(&world, &mut history, "Block", Some(&p));
    // A 20 mm cube over B2, its bottom face sunk 5 mm into the plate top.
    history.execute(&world, ImportModel::from_mesh("Part", Mesh::from_facets(plate::slab(20.0, 20.0, 20.0))));
    let part = model::find_by_name(&world, "Part").expect("part");
    model::set_transform(&world, &part, at(32.5, 32.5, 30.0));
    let spec = LocateSpec {
        part: "Part".into(),
        fixture: p.clone(),
        body: Default::default(),
        mates: vec![Mate { part: bottom_face(&world, "Part"), fixture: top_face(&world, &p) }],
        offset: LocateOffset { dz: -5.0, ..LocateOffset::default() },
    };
    let cmd = LocateFixture::new(&world, &spec).expect("locate");
    let w = cmd.warning().unwrap_or_default().to_owned();
    assert!(w.contains("Block"), "the carried block is judged where it lands: {w}");
    // The move alone (no riders in the way) stays quiet about it.
    let quiet = MoveFixture::new(&world, &p, at(0.0, 400.0, 0.0)).expect("move");
    assert_eq!(quiet.warning(), None);
}

/// A plate's mesh changing drops its grid and unmounts its riders in
/// the same step; one undo puts both back.
#[test]
fn a_plates_mesh_change_drops_its_grid_and_riders_and_undo_restores() {
    let world = new_world();
    let mut history = History::default();
    let p = insert_plate(&world, &mut history);
    block(&world, &mut history, "Block", Some(&p));
    assert!(def(&world, &p).is_some_and(|d| d.plate.plate.is_some()));
    let before = model::facets_of(&world, &model::find_by_name(&world, &p).expect("plate")).expect("facets");

    let cmd = UpdateModel::new(&world, &p, Mesh::from_facets(plate::slab(50.0, 50.0, 5.0))).expect("update");
    history.execute(&world, cmd);
    assert!(def(&world, &p).is_some_and(|d| d.plate.plate.is_none()), "the stale grid is gone");
    assert!(def(&world, "Block").is_some_and(|d| d.plate.mount.is_none()), "the rider leaves the plate");

    assert!(history.undo(&world));
    assert!(def(&world, &p).is_some_and(|d| d.plate.plate.is_some()), "undo restores the grid");
    assert!(def(&world, "Block").is_some_and(|d| d.plate.mount.is_some()), "undo restores the place");
    assert_eq!(model::facets_of(&world, &model::find_by_name(&world, &p).expect("plate")).expect("facets").len(), before.len());
    assert!(history.redo(&world));
    assert!(def(&world, &p).is_some_and(|d| d.plate.plate.is_none()));
}

/// A staggered plate (as `fixture_plate_detect`'s) with the outline `poly`.
fn staggered_with(name: &str, poly: &[(f64, f64)]) -> Result<(), String> {
    let mut holes = Vec::new();
    for r in 0..6u32 {
        for c in 0..8u32 {
            if (r + c) % 2 == 0 {
                holes.push((10.0 + 15.0 * f64::from(c), 10.0 + 15.0 * f64::from(r), 3.0));
            }
        }
    }
    let ring = |poly: &[(f64, f64)], z0: f64, z1: f64| {
        let n = poly.len();
        (0..n)
            .flat_map(move |i| {
                let ((x0, y0), (x1, y1)) = (poly[i], poly[(i + 1) % n]);
                let (a, b) = (Point3::new(x0, y0, z0), Point3::new(x1, y1, z0));
                let (c, d) = (Point3::new(x1, y1, z1), Point3::new(x0, y0, z1));
                [Triangle::new(a, b, c), Triangle::new(a, c, d)]
            })
            .collect::<Vec<_>>()
    };
    let mut t = ring(poly, -20.0, 0.0);
    for &(x, y, r) in &holes {
        let mut c: Vec<(f64, f64)> = (0..32)
            .map(|i| {
                let a = std::f64::consts::TAU * f64::from(i) / 32.0;
                (x + r * a.cos(), y + r * a.sin())
            })
            .collect();
        c.reverse();
        t.extend(ring(&c, -20.0, 0.0));
    }
    let world = new_world();
    let mut history = History::default();
    history.execute(&world, ImportModel::from_mesh(name, Mesh::from_facets(t)));
    history.execute(&world, TagFixture::new(&world, name, None).expect("tag"));
    SetPlateGrid::detect(&world, name).map(|_| ()).map_err(|e| e.to_string())
}

#[test]
fn a_staggered_plate_with_an_ear_or_chamfers_is_not_refused_as_turned() {
    // Chamfered 30 mm corners on the left side: the least-X edge is short,
    // the greatest-X edge is straight.
    let chamfered = [(30.0, 0.0), (125.0, 0.0), (125.0, 95.0), (30.0, 95.0), (0.0, 65.0), (0.0, 30.0)];
    assert_eq!(staggered_with("Chamfered", &chamfered), Ok(()));
    // A 30 mm clamp ear on the left: the least-X edge is the ear's tip.
    let eared = [(0.0, 0.0), (125.0, 0.0), (125.0, 95.0), (0.0, 95.0), (0.0, 60.0), (-30.0, 60.0), (-30.0, 35.0), (0.0, 35.0)];
    assert_eq!(staggered_with("Eared", &eared), Ok(()));
    // Both ends spoilt on X: still refused as turned.
    let both = [(30.0, 0.0), (95.0, 0.0), (125.0, 30.0), (125.0, 65.0), (95.0, 95.0), (30.0, 95.0), (0.0, 65.0), (0.0, 30.0)];
    let e = staggered_with("Octagon", &both).expect_err("no straight edge on X");
    assert!(e.contains("turned"), "{e}");
}
