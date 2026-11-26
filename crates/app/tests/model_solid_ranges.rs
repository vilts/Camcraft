//! #917: the closed solids of a model (`app::mesh_solids`, component
//! `SolidRanges`) ride the facets they were landed with. Import, update
//! and undo land them; any other landing of facets leaves no stale ranges;
//! the project file carries them as `solids` (serde default, no version
//! step); a file without the key reads as before; ranges that do not tile
//! the facets are dropped with a logged note, never trusted.
//! Provenance: our own fixtures (two disjoint unit cubes).
//!
//! Gate map (#917): covers app::mesh_solids, app::commands (import_model,
//! update_model), app::project (wire, apply), app::schema (SolidRanges).
//! Lane shape: all tests here run in well under a second.

use std::ops::Range;
use std::path::PathBuf;

use flecs_ecs::prelude::World;
use serde_json::{Value, json};

use camcraft_app::commands::{History, ImportModel, UpdateModel};
use camcraft_app::geometry::{Mesh, Point3, Triangle, cube};
use camcraft_app::world::new_world;
use camcraft_app::{mesh_solids, model, project};

fn two_cubes() -> (Vec<Triangle>, Vec<Range<usize>>) {
    let a = cube(Point3::new(0.0, 0.0, 0.0), Point3::new(1.0, 1.0, 1.0));
    let b = cube(Point3::new(5.0, 0.0, 0.0), Point3::new(6.0, 1.0, 1.0));
    let n = a.facets.len();
    let facets: Vec<Triangle> = a.facets.iter().chain(b.facets.iter()).copied().collect();
    let len = facets.len();
    (facets, vec![0..n, n..len])
}

fn solid_mesh() -> (Mesh, Vec<Range<usize>>) {
    let (facets, ranges) = two_cubes();
    (Mesh::from_solids(facets, ranges.clone()).expect("two solids tile the facets"), ranges)
}

fn solids(world: &World) -> Vec<Range<usize>> {
    let e = model::find_by_name(world, "p").expect("the part");
    mesh_solids::of(world, &e)
}

fn tmp(tag: &str) -> PathBuf {
    std::env::temp_dir().join(format!("camcraft_917_solids_{tag}_{}.camproj", std::process::id()))
}

#[test]
fn import_lands_the_solids_and_undo_redo_restores_them() {
    let world = new_world();
    let mut h = History::default();
    let (mesh, ranges) = solid_mesh();
    h.execute(&world, ImportModel::from_mesh("p", mesh));
    assert_eq!(solids(&world), ranges, "the import lands the solids");
    assert!(h.undo(&world), "undo the import");
    assert!(model::find_by_name(&world, "p").is_none(), "the import is undone");
    assert!(h.redo(&world), "redo the import");
    assert_eq!(solids(&world), ranges, "redo lands the same solids");

    // An update to one soup clears them; undo brings the solids back with
    // the facets they were landed with.
    let (facets, _) = two_cubes();
    let before = model::facets_of(&world, &model::find_by_name(&world, "p").expect("p")).expect("facets");
    h.execute(&world, UpdateModel::new(&world, "p", Mesh::from_facets(facets)).expect("update"));
    assert!(solids(&world).is_empty(), "a soup has no solids");
    assert!(h.undo(&world), "undo the update");
    let e = model::find_by_name(&world, "p").expect("p");
    assert_eq!(model::facets_of(&world, &e).expect("facets")[..], before[..], "the facets come back");
    assert_eq!(solids(&world), ranges, "the solids come back with them");
    assert!(h.redo(&world), "redo the update");
    assert!(solids(&world).is_empty(), "redo clears them again");
}

#[test]
fn other_facets_landing_leave_no_stale_ranges() {
    let world = new_world();
    let mut h = History::default();
    let (mesh, ranges) = solid_mesh();
    h.execute(&world, ImportModel::from_mesh("p", mesh));
    assert_eq!(solids(&world), ranges);
    // Any other landing of facets (here the same count, moved) steps the
    // revision: the ranges no longer speak for the facets.
    let e = model::find_by_name(&world, "p").expect("p");
    let moved: Vec<Triangle> = model::facets_of(&world, &e)
        .expect("facets")
        .iter()
        .map(|t| {
            let up = |p: Point3| Point3::new(p.x, p.y, p.z + 2.0);
            Triangle::new(up(t.a), up(t.b), up(t.c))
        })
        .collect();
    model::set_facets(&world, &e, moved.into());
    assert!(mesh_solids::of(&world, &e).is_empty(), "no stale ranges after another landing");
}

#[test]
fn the_solids_ride_the_project_file_and_a_file_without_them_reads_as_before() {
    let world = new_world();
    let mut h = History::default();
    let (mesh, ranges) = solid_mesh();
    h.execute(&world, ImportModel::from_mesh("p", mesh));
    let a = tmp("a");
    project::save(&world, &a).expect("save");
    let first = std::fs::read_to_string(&a).expect("read");

    let loaded = new_world();
    project::load(&loaded, &a).expect("load");
    assert_eq!(solids(&loaded), ranges, "the solids load");
    project::save(&loaded, &a).expect("save again");
    assert_eq!(std::fs::read_to_string(&a).expect("reread"), first, "the round trip is byte-identical");

    // A file saved before #917 has no `solids` key: it reads, as one soup.
    let mut doc: Value = serde_json::from_str(&first).expect("json");
    let entities = doc["entities"].as_array_mut().expect("entities");
    let p = entities.iter_mut().find(|e| e["name"] == json!("p")).expect("p in the file");
    assert_eq!(p["solids"], json!([[0, 12], [12, 24]]), "the file form");
    p.as_object_mut().expect("object").remove("solids");
    let old = serde_json::to_string_pretty(&doc).expect("json");
    std::fs::write(&a, &old).expect("write");
    let w = new_world();
    project::load(&w, &a).expect("a file without solids loads");
    assert!(solids(&w).is_empty(), "no solids: read as one soup");
    let e = model::find_by_name(&w, "p").expect("p");
    assert_eq!(model::facets_of(&w, &e).expect("facets").len(), 24, "the same facets");
    // A model without solids writes no key: the file is as main writes it.
    project::save(&w, &a).expect("save");
    let again: Value = serde_json::from_str(&std::fs::read_to_string(&a).expect("read")).expect("json");
    let p = again["entities"].as_array().expect("entities").iter().find(|e| e["name"] == json!("p")).expect("p");
    assert!(p.get("solids").is_none(), "no solids key: {p}");
    let _ = std::fs::remove_file(&a);
}

#[test]
fn ranges_that_do_not_tile_the_facets_are_dropped_at_load() {
    let world = new_world();
    let mut h = History::default();
    h.execute(&world, ImportModel::from_mesh("p", solid_mesh().0));
    let a = tmp("bad");
    project::save(&world, &a).expect("save");
    let text = std::fs::read_to_string(&a).expect("read");
    let bad = [
        json!([[0, 12], [12, 999]]),
        json!([[0, 14], [12, 24]]),
        json!([[0, 12], [14, 24]]),
        json!([[0, 12], [12, 20]]),
        json!([[12, 24], [0, 12]]),
        json!([[0, 0], [0, 24]]),
        json!([[0, 24]]),
        json!([[18446744073709551615u64, 0], [0, 24]]),
    ];
    for b in bad {
        let mut doc: Value = serde_json::from_str(&text).expect("json");
        let p = doc["entities"].as_array_mut().expect("entities").iter_mut().find(|e| e["name"] == json!("p")).expect("p");
        p["solids"] = b.clone();
        std::fs::write(&a, serde_json::to_string_pretty(&doc).expect("json")).expect("write");
        let w = new_world();
        project::load(&w, &a).unwrap_or_else(|e| panic!("{b}: the file still loads: {e}"));
        assert!(solids(&w).is_empty(), "{b}: dropped, read as one soup");
        let e = model::find_by_name(&w, "p").expect("p");
        assert_eq!(model::facets_of(&w, &e).expect("facets").len(), 24, "{b}: the facets load");
    }
    let _ = std::fs::remove_file(&a);
}
