//! Re-import through the script verb preserves the new STEP source and undo history.
//! Provenance: clean-room regression pin for PR #2101 review finding 4.

use super::{Env, api};
use crate::fixture_library::{DatumChoice, entry_from_model};
use crate::{model, project};
use serde_json::json;

fn saved_source(env: &Env) -> String {
    let entry =
        entry_from_model(&env.world, "jaw", DatumChoice::ModelOrigin, None).expect("library entry");
    entry.step_source.expect("library keeps STEP")
}

#[test]
fn script_step_reimport_saves_new_source_and_undo_redo_restores_each_source() {
    let original = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../testdata/models/box.step"
    ))
    .expect("STEP");
    let updated = original.replace(
        "ISO-10303-21;",
        "ISO-10303-21;\n/* updated vendor source */",
    );
    let dir = std::env::temp_dir().join(format!("camcraft-step-script-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("directory");
    let path = dir.join("jaw.step");
    let mut env = Env::fresh();
    std::fs::write(&path, &original).expect("initial STEP");
    api::v_import(&mut env, vec![json!(path)]).expect("first import");
    assert_eq!(saved_source(&env), original);
    std::fs::write(&path, &updated).expect("new STEP");
    api::v_import(&mut env, vec![json!(path)]).expect("re-import");
    assert_eq!(saved_source(&env), updated);
    let saved = serde_json::to_string(&project::collect(&env.world)).expect("save project");
    let loaded = crate::world::new_world();
    project::open_noted(&loaded, &saved).expect("load project");
    let entity = model::find_by_name(&loaded, "jaw").expect("loaded jaw");
    assert_eq!(
        crate::step_source::of(&loaded, &entity).as_deref(),
        Some(updated.as_str())
    );
    env.history.undo(&env.world);
    assert_eq!(saved_source(&env), original);
    env.history.redo(&env.world);
    assert_eq!(saved_source(&env), updated);
    std::fs::remove_dir_all(dir).expect("cleanup");
}
