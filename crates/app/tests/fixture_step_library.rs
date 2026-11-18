//! #744 pin: a STEP fixture imports, saves to the fixture library as
//! STEP (never converted to a profile), and re-inserts unchanged.
//!
//! Gate map (M4.4): covers app::commands::import_model,
//! app::commands::import_fixture, app::commands::insert_fixture,
//! app::fixture_library, app::project.
//! Provenance: our own fixture-library persistence law.

use std::path::Path;

use camcraft_app::commands::{History, ImportFixture, ImportModel, InsertFixture};
use camcraft_app::fixture_library::{self, DatumChoice, entry_from_model};
use camcraft_app::geometry::{IDENTITY_Q, Point3};
use camcraft_app::model;
use camcraft_app::project;
use camcraft_app::schema::Location;
use camcraft_app::world::new_world;

fn box_step_bytes() -> Vec<u8> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testdata/models/box.step");
    std::fs::read(&path).expect("read box.step")
}

#[test]
fn step_fixture_saves_to_library_as_step_and_reinserts_unchanged() {
    let step_bytes = box_step_bytes();
    let step_text = std::str::from_utf8(&step_bytes).expect("utf-8 STEP");

    let world = new_world();
    let mut history = History::default();
    let import = ImportModel::from_bytes("vendor_jaw.step", &step_bytes, None).expect("import STEP");
    let fix = ImportFixture::new(&world, import, None).expect("fixture");
    let name = fix.name().to_owned();
    history.execute(&world, fix);

    let original_facets = model::find_by_name(&world, &name)
        .and_then(|e| model::facets_of(&world, &e))
        .expect("facets");

    // Save to fixture library: the entry must carry the raw STEP source
    // (kept as STEP, not converted to a profile).
    let entry = entry_from_model(&world, &name, DatumChoice::ModelOrigin, Some("vendor STEP solid"))
        .expect("entry from STEP fixture");
    let lib_text = fixture_library::to_text(&[entry]).expect("serialize fixture library");
    let lib_json: serde_json::Value = serde_json::from_str(&lib_text).expect("library json");
    let saved_row = &lib_json["fixtures"][0];
    assert_eq!(
        saved_row.get("step_source").and_then(serde_json::Value::as_str),
        Some(step_text),
        "fixture library must keep and save the raw STEP source unchanged: {saved_row}"
    );
    assert!(
        saved_row.get("profile").is_none(),
        "STEP fixture must not be replaced by a profile: {saved_row}"
    );

    // Re-parse the saved library and insert into a fresh world unchanged.
    let parsed = fixture_library::parse(&lib_text).expect("parse fixture library");
    let fresh = new_world();
    let mut fresh_history = History::default();
    let place = Location { pos: Point3::new(10.0, 20.0, 0.0), rot: IDENTITY_Q };
    let ins = InsertFixture::new(&fresh, &parsed[0], None, place).expect("re-insert STEP fixture");
    let placed_name = ins.name().to_owned();
    fresh_history.execute(&fresh, ins);

    let reinserted_e = model::find_by_name(&fresh, &placed_name).expect("placed");
    let reinserted_facets = model::facets_of(&fresh, &reinserted_e).expect("reinserted facets");
    assert_eq!(reinserted_facets.as_ref(), original_facets.as_ref(), "facets preserved unchanged");

    // Saving the placed STEP fixture to the library again keeps the exact STEP bytes.
    let again = entry_from_model(&fresh, &placed_name, DatumChoice::ModelOrigin, None).expect("second entry");
    let again_text = fixture_library::to_text(&[again]).expect("second serialize");
    let again_json: serde_json::Value = serde_json::from_str(&again_text).expect("second json");
    assert_eq!(
        again_json["fixtures"][0].get("step_source").and_then(serde_json::Value::as_str),
        Some(step_text),
        "re-inserted fixture must keep original STEP source"
    );

    // Project save + load also keeps the STEP source on the fixture.
    let proj_text = serde_json::to_string(&project::collect(&fresh)).expect("project save");
    let reloaded = new_world();
    project::open_noted(&reloaded, &proj_text).expect("project load");
    let round_entry = entry_from_model(&reloaded, &placed_name, DatumChoice::ModelOrigin, None).expect("round entry");
    let round_text = fixture_library::to_text(&[round_entry]).expect("round serialize");
    let round_json: serde_json::Value = serde_json::from_str(&round_text).expect("round json");
    assert_eq!(
        round_json["fixtures"][0].get("step_source").and_then(serde_json::Value::as_str),
        Some(step_text),
        "project save/reload must keep STEP source on the fixture"
    );
}
