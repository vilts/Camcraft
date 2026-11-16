//! #2152: a cache judged under the old rapid law never posts silently.
//! #2121 (PR #2138) made the rapid law judge a traverse AT the clearance
//! plane against the stock, but the post gate reads only the WARN
//! records a cache carries. A project saved before the change holds a
//! "clear" cache for a traverse through uncut stock; its stamp still
//! matched, so it posted with no warning. The op stamp's salt moved
//! (v2 -> v3): every cache made under the old law reads Stale and the
//! post refuses it by name until the user regenerates.
//!
//! The fixture `stale_rapid_law/rapid_law_v2_cache.camproj` is our own: the
//! #2121 repro (a Contour with clearance and retract at Z 8 under a
//! Z 10 stock top) generated and saved at e7050be17 (salt v2), with the
//! new law's two `rapid_in_stock` verdicts and its "Clearance in stock"
//! note taken out of the cache - what the old law wrote for that path.
//!
//! Provenance: our own synthetic fixture and the Prime Directive (an
//! unsafe result never passes silently); no vendor material.
//!
//! Gate map (#2152): covers app::operation (stamp), app::posting (the
//! freshness gate). Lane shape: runs in seconds.

use camcraft_app::cl::MoveKind;
use camcraft_app::commands::{GenerateOperation, History};
use camcraft_app::model;
use camcraft_app::operation::{self, OpStatus};
use camcraft_app::posting;
use camcraft_app::verify::Warning;

const OP: &str = "Op1Contour";

fn old_law_document() -> flecs_ecs::prelude::World {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/stale_rapid_law/rapid_law_v2_cache.camproj");
    let world = camcraft_app::world::new_world();
    camcraft_app::project::load(&world, &path).expect("the recorded project loads");
    world
}

#[test]
fn a_cache_judged_under_the_old_rapid_law_reads_stale_and_holds_the_post() {
    let world = old_law_document();
    let e = model::find_by_name(&world, OP).expect("op");
    // The recorded cache is the old law's: a traverse at Z 8 through the
    // Z 10 stock, and no verdict on it.
    let tp = model::toolpath_of(&world, &e).expect("the cache rides the file");
    let first = &tp.path.moves[0];
    assert!(first.kind == MoveKind::Rapid && first.from.z == 8.0 && first.to.z == 8.0, "{first:?}");
    assert!(tp.warnings.is_empty(), "the fixture must hold the old law's clear verdict: {:?}", tp.warnings);
    // The salt moved, so the old cache is not fresh.
    assert_eq!(operation::status_of(&world, &e), OpStatus::Stale);
    // The post refuses it by name, says to regenerate, and writes nothing.
    let err = posting::post_setup_unwritten(&world).expect_err("an old-law cache must not post");
    assert!(err.contains(OP) && err.contains("regenerate"), "{err}");
    let out = std::env::temp_dir().join(format!("camcraft-stale-rapid-law-{}.nc", std::process::id()));
    let _ = std::fs::remove_file(&out);
    assert!(posting::post_setup(&world, &out).is_err());
    assert!(!out.exists(), "a refused post writes nothing");
}

#[test]
fn regenerating_the_old_cache_judges_it_under_the_new_law() {
    let world = old_law_document();
    let mut history = History::default();
    history.execute(&world, GenerateOperation::new(&world, OP).expect("regenerate"));
    let e = model::find_by_name(&world, OP).expect("op");
    assert_eq!(operation::status_of(&world, &e), OpStatus::Ok);
    let tp = model::toolpath_of(&world, &e).expect("path");
    assert!(
        tp.warnings.iter().any(|w| matches!(w, Warning::RapidInStock { at: 0 })),
        "the traverse at the plane is convicted now: {:?}",
        tp.warnings
    );
    let err = posting::post_setup_unwritten(&world).expect_err("the convicted program must not post");
    assert!(err.contains("travels through stock"), "{err}");
}
