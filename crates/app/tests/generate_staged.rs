//! #2257: a generation split for a background worker. Staging then
//! finishing gives the same command as `GenerateOperation::new`, and a
//! finished result knows when its operation changed since it was staged
//! (an edit, an undo of a generation, a delete), so the UI never writes
//! old parameters over newer ones.
//!
//! Gate map: covers app::commands::operation (generate).
//! Provenance: our own tests over our own commands; no HSMWorks material.

mod area_support;
mod common;

use area_support::*;
use camcraft_app::commands::{DeleteOperation, GenerateOperation, StagedGeneration};

const OP: &str = "Face1";

fn world_with_face() -> (World, History) {
    let world = block_world();
    let mut h = History::default();
    h.execute(&world, CreateOperation::new(&world, OP, TOOL, SrcRef::Stock, face(9.0)).expect("create"));
    (world, h)
}

fn path_of(world: &World) -> Option<camcraft_app::schema::Toolpath> {
    model::find_by_name(world, OP).and_then(|e| model::toolpath_of(world, &e))
}

#[test]
fn staged_then_finished_lands_what_the_one_shot_command_lands() {
    let (world, mut h) = world_with_face();
    let staged = StagedGeneration::new(&world, OP).expect("stages");
    assert_eq!(staged.name(), OP);
    let done = staged.finish().expect("finishes");
    assert_eq!(done.name(), OP);
    assert!(done.still_current(&world));
    h.execute(&world, done);
    let a = path_of(&world).expect("a path");
    assert!(h.undo(&world));
    h.execute(&world, GenerateOperation::new(&world, OP).expect("one shot"));
    assert_eq!(path_of(&world).expect("a path"), a, "the same path either way");
}

#[test]
fn a_result_is_stale_after_an_edit_a_generation_or_a_delete() {
    let (world, mut h) = world_with_face();
    let done = StagedGeneration::new(&world, OP).and_then(StagedGeneration::finish).expect("generates");
    // An edit of the parameters.
    let p = face(8.0);
    h.execute(&world, EditOperationParams::new(&world, OP, p).expect("edit"));
    assert!(!done.still_current(&world), "an edit makes it stale");
    assert!(h.undo(&world));
    assert!(done.still_current(&world), "undoing the edit restores the staged state");
    // Another generation landed meanwhile: the cache moved.
    h.execute(&world, GenerateOperation::new(&world, OP).expect("generate"));
    assert!(!done.still_current(&world), "a newer cache makes it stale");
    assert!(h.undo(&world));
    // The operation is gone.
    h.execute(&world, DeleteOperation::new(&world, OP).expect("delete"));
    assert!(!done.still_current(&world), "a deleted operation takes nothing");
}
