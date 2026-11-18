//! Saved tilt basis compatibility (#2133 / PR #2157).
//! Provenance: our own project from 06dfbcaafab3e6bc98fd2cbed85c9ee884bbfd2c,
//! with the display-only model removed. Operation definitions and caches are unchanged.
//!
//! Gate map (M4.4): covers app::project, app::operation, app::posting, cl::frame.

use camcraft_app::cl::{lattice, MoveKind};
use camcraft_app::commands::{GenerateOperation, History};
use camcraft_app::geometry::Point3;
use camcraft_app::operation::{self, OpStatus};
use camcraft_app::{model, posting, project};
use flecs_ecs::prelude::World;
use serde_json::Value;

const OLD: &str = include_str!("fixtures/legacy-tilt-basis.camproj");
const NAME: &str = "Flat A180";
const PROGRAM: &str = include_str!("../../../examples/first-4axis-part.nc");

fn opened(text: &str) -> World {
    let world = camcraft_app::world::new_world();
    project::open(&world, text).expect("load saved project");
    world
}

fn post(world: &World, suffix: &str) -> String {
    let out = std::env::temp_dir().join(format!("legacy-tilt-{}-{suffix}.nc", std::process::id()));
    let nc = posting::post_setup(world, &out)
        .expect("safe saved cut posts")
        .program;
    let _ = std::fs::remove_file(out);
    nc
}

#[test]
fn old_project_keeps_basis_status_regeneration_and_posted_cut() {
    let world = opened(OLD);
    let entity = model::find_by_name(&world, NAME).expect("saved operation");
    let op = model::operation_of(&world, &entity).expect("definition");
    let tilt = op.params.tilt().expect("saved tilt");
    assert_eq!(tilt.basis_version, 0);
    let world_point = tilt
        .frame()
        .expect("legacy frame")
        .to_world(Point3::new(0.0, -19.05, 0.0));
    assert!(
        (world_point.x + 19.05).abs() < 1e-9 && world_point.y.abs() < 1e-9,
        "{world_point:?}"
    );
    assert_eq!(
        operation::status_of(&world, &entity),
        OpStatus::Stale,
        "the v2 verification verdict must not be current under the v3 law"
    );
    let error = posting::post_setup_unwritten(&world).expect_err("old verdict must not post");
    assert!(
        error.contains("inputs moved since generation") && error.contains("regenerate"),
        "{error}"
    );
    let cached: Vec<_> = camcraft_app::setup::operation_order(&world)
        .into_iter()
        .map(|(name, _)| {
            let entity = model::find_by_name(&world, &name).expect("saved operation");
            (
                name,
                model::toolpath_of(&world, &entity)
                    .expect("saved cache")
                    .path,
            )
        })
        .collect();
    regenerate(&world);
    assert_eq!(operation::status_of(&world, &entity), OpStatus::Ok);
    let regenerated: Vec<_> = cached
        .iter()
        .map(|(name, old_path)| {
            let entity = model::find_by_name(&world, name).expect("regenerated operation");
            let mut path = model::toolpath_of(&world, &entity)
                .expect("regenerated path")
                .path;
            // Indexed transforms introduce roundoff. Compare on the same
            // coordinate and axis lattice as the saved project cache.
            for m in &mut path.moves {
                for point in [&mut m.from, &mut m.to] {
                    *point = Point3::new(lattice(point.x), lattice(point.y), lattice(point.z));
                }
                m.axis = m.axis.map(|axis| axis.map(lattice));
            }
            if name == "Flat A-90" {
                let old_cuts: Vec<_> = old_path
                    .moves
                    .iter()
                    .filter(|m| m.kind == MoveKind::Feed)
                    .collect();
                let cuts: Vec<_> = path
                    .moves
                    .iter()
                    .filter(|m| m.kind == MoveKind::Feed)
                    .collect();
                assert_eq!(
                    old_cuts.len(),
                    2,
                    "legacy cache includes the Top Z air pass"
                );
                assert_eq!(
                    cuts.len(),
                    1,
                    "regeneration removes only the Top Z air pass"
                );
                assert!(
                    (old_cuts[0].to.y - 9.525).abs() < 1e-6,
                    "old first cut is at Top Z"
                );
                assert!(
                    (cuts[0].to.y - 9.144).abs() < 1e-6,
                    "new first cut is one stepdown below Top Z"
                );
                assert_eq!(cuts[0], old_cuts[1], "the lower cut stays unchanged");
            }
            (name.clone(), path)
        })
        .collect();
    assert_eq!(
        post(&world, "after"),
        PROGRAM,
        "regeneration must preserve the intended machine program"
    );
    let saved = project::save_text(&world).expect("save explicit basis");
    let reopened = opened(&saved);
    let reopened_entity = model::find_by_name(&reopened, NAME).expect("reopened operation");
    assert_eq!(
        model::operation_of(&reopened, &reopened_entity)
            .unwrap()
            .params
            .tilt()
            .unwrap()
            .basis_version,
        0
    );
    assert_eq!(
        operation::status_of(&reopened, &reopened_entity),
        OpStatus::Ok
    );
    // Saving and reopening must preserve the regenerated, lattice-snapped path.
    for (name, regenerated_path) in regenerated {
        let entity = model::find_by_name(&reopened, &name).expect("reopened operation");
        assert_eq!(
            model::toolpath_of(&reopened, &entity).unwrap().path,
            regenerated_path,
            "{name}"
        );
    }
    assert_eq!(post(&reopened, "reopened"), PROGRAM);
}

fn regenerate(world: &World) {
    let mut history = History::default();
    for (name, _) in camcraft_app::setup::operation_order(world) {
        history.execute(
            world,
            GenerateOperation::new(world, &name).expect("regenerate saved cut"),
        );
        let entity = model::find_by_name(world, &name).expect("saved operation");
        assert_eq!(operation::status_of(world, &entity), OpStatus::Ok, "{name}");
    }
}

#[test]
fn corrected_basis_rejects_a_legacy_cache_stamp() {
    let world = opened(OLD);
    // Generate under the current verification law. This isolates the basis change.
    regenerate(&world);
    let saved = project::save_text(&world).expect("save current-law legacy basis cache");
    let unchanged = opened(&saved);
    let entity = model::find_by_name(&unchanged, NAME).unwrap();
    assert_eq!(operation::status_of(&unchanged, &entity), OpStatus::Ok);
    assert_eq!(post(&unchanged, "basis-unchanged"), PROGRAM);
    let mut file: Value = serde_json::from_str(&saved).unwrap();
    let entity = file["entities"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|e| e["name"] == NAME)
        .unwrap();
    // Keep the current-law stamp and cached path. Change only the basis contract.
    entity["operation"]["params"]["slot"]["tilt"]["basis_version"] = 1.into();
    let world = opened(&file.to_string());
    let entity = model::find_by_name(&world, NAME).unwrap();
    assert_eq!(operation::status_of(&world, &entity), OpStatus::Stale);
    let error =
        posting::post_setup_unwritten(&world).expect_err("incompatible cache must not post");
    assert!(
        error.contains(NAME)
            && error.contains("inputs moved since generation")
            && error.contains("regenerate"),
        "{error}"
    );
    let out = std::env::temp_dir().join(format!("legacy-tilt-{}-stale.nc", std::process::id()));
    let _ = std::fs::remove_file(&out);
    let error = posting::post_setup(&world, &out).expect_err("incompatible cache must not post");
    assert!(
        error.contains(NAME) && error.contains("regenerate"),
        "{error}"
    );
    assert!(!out.exists(), "a refused post writes nothing");
}

#[test]
fn live_json_tilts_use_current_basis_and_preserve_an_explicit_legacy_basis() {
    let file: Value = serde_json::from_str(OLD).unwrap();
    let op = file["entities"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["name"] == NAME)
        .unwrap();
    let fresh = project::params_from_json(&op["operation"]["params"])
        .expect("new tilt through live JSON door");
    assert_eq!(fresh.tilt().unwrap().basis_version, 1);
    let world = opened(OLD);
    let entity = model::find_by_name(&world, NAME).unwrap();
    let legacy = model::operation_of(&world, &entity).unwrap().params;
    let mut patch = project::params_to_json(&legacy);
    patch["slot"]["feed"] = 300.0.into();
    let edited = project::params_from_json(&patch).expect("partial edit through live JSON door");
    assert_eq!(edited.tilt().unwrap().basis_version, 0);
}

#[test]
fn unknown_saved_basis_is_visibly_refused_before_loading() {
    let mut file: Value = serde_json::from_str(OLD).unwrap();
    let entity = file["entities"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|e| e["name"] == NAME)
        .unwrap();
    entity["operation"]["params"]["slot"]["tilt"]["basis_version"] = 2.into();
    let world = camcraft_app::world::new_world();
    let error = project::open(&world, &file.to_string()).expect_err("unknown basis must not load");
    let message = error.to_string();
    assert!(
        message.contains(NAME) && message.contains("unsupported frame basis version"),
        "{message}"
    );
    assert!(
        model::find_by_name(&world, NAME).is_none(),
        "refuse before any operation is installed"
    );
}

#[test]
fn saved_basis_contract_requires_a_version_that_legacy_readers_refuse() {
    let old_file: Value = serde_json::from_str(OLD).unwrap();
    assert_eq!(old_file["version"], 62);
    let world = opened(OLD);
    let saved: Value = serde_json::from_str(&project::save_text(&world).unwrap()).unwrap();
    assert!(
        saved["version"].as_u64().unwrap() >= 65,
        "a version-64 reader ignores the new basis field and must refuse this file"
    );
    // The version guard also refuses future basis contracts in this reader.
    let mut future = saved;
    future["version"] = (project::PROJECT_VERSION + 1).into();
    let error = project::open(&camcraft_app::world::new_world(), &future.to_string()).unwrap_err();
    assert!(matches!(
        error,
        project::ProjectError::UnsupportedVersion(_)
    ));
}
