//! #1386: the Explain G-code post option as the document holds it. Off
//! (the default) the script's `postOptions` carries no `Explain` key and
//! the project file no `explain` key - byte-identical to before; on, both
//! say so, the switch undoes and redoes, a setup's own options carry it,
//! and every built-in post either explains its blocks or is refused by
//! its name.
//!
//! Gate map (#1386): covers app::posting (options_value), app::schema
//! (PostOptions.explain), app::project (the `explain` keys),
//! app::commands (SetPostOptions, SetSetupPostOptions).
//! Provenance: our own fixtures; no vendor material.

use camcraft_app::commands::{CreateSetup, History, SetPostOptions, SetSetupPostOptions};
use camcraft_app::post::{engine, PostError};
use camcraft_app::schema::PostOptions;
use camcraft_app::{posting, project, setup_post_options, world::new_world};
use serde_json::Value;

#[test]
fn explain_is_published_only_when_on() {
    let off = posting::options_value(PostOptions::default());
    assert!(off.get("Explain").is_none(), "off states no key: {off}");
    let on = posting::options_value(PostOptions { explain: true, ..PostOptions::default() });
    assert_eq!(on.get("Explain"), Some(&Value::Bool(true)), "{on}");
}

#[test]
fn the_explain_switch_undoes_redoes_and_round_trips_the_project_file() {
    let world = new_world();
    let mut h = History::default();
    assert!(!posting::post_options(&world).explain, "off by default");
    let on = PostOptions { explain: true, ..posting::post_options(&world) };
    h.execute(&world, SetPostOptions::new(&world, on).expect("set"));
    assert!(posting::post_options(&world).explain);
    assert!(h.undo(&world) && !posting::post_options(&world).explain, "undo turns it off");
    assert!(h.redo(&world) && posting::post_options(&world).explain, "redo turns it on");
    let path = std::env::temp_dir().join(format!("camcraft-explain-{}.camproj", std::process::id()));
    project::save(&world, &path).expect("save");
    let text = std::fs::read_to_string(&path).expect("read");
    assert!(text.contains("\"explain\": true"), "the key is written when on");
    let back = new_world();
    project::load(&back, &path).expect("load");
    assert!(posting::post_options(&back).explain, "the key round-trips");
    assert!(h.undo(&world));
    project::save(&world, &path).expect("save off");
    let off = std::fs::read_to_string(&path).expect("read off");
    assert!(!off.contains("explain"), "off writes no key");
    let _ = std::fs::remove_file(&path);
}

#[test]
fn a_setups_own_options_carry_explain() {
    let world = new_world();
    let mut h = History::default();
    h.execute(&world, CreateSetup::new(&world, None, None).expect("setup"));
    let own = PostOptions { explain: true, ..PostOptions::default() };
    h.execute(&world, SetSetupPostOptions::new(&world, "Setup1", Some(own)).expect("own options"));
    assert_eq!(setup_post_options::own(&world, Some("Setup1")), Some(own));
    let path = std::env::temp_dir().join(format!("camcraft-explain-setup-{}.camproj", std::process::id()));
    project::save(&world, &path).expect("save");
    let back = new_world();
    project::load(&back, &path).expect("load");
    assert_eq!(setup_post_options::own(&back, Some("Setup1")), Some(own), "a setup's explain round-trips");
    assert!(h.undo(&world));
    assert_eq!(setup_post_options::own(&world, Some("Setup1")), None, "undo drops the setup's own options");
    let _ = std::fs::remove_file(&path);
}

#[test]
fn every_builtin_explains_or_is_refused_by_name() {
    let on = posting::options_value(PostOptions { explain: true, ..PostOptions::default() });
    for (name, cps) in posting::BUILTINS {
        let declares = cps.lines().any(|l| l.trim_start().starts_with("explains ="));
        match engine::run_with(cps, &Value::Null, "", &on) {
            Err(PostError::Refused(why)) if !declares => {
                assert!(why.contains("does not declare explains"), "{name}: {why}");
                assert!(why.contains(" post "), "{name} named: {why}");
            }
            Err(PostError::Refused(why)) => panic!("{name} declares explains but was refused: {why}"),
            _ if declares => {}
            other => panic!("{name} states no explains yet was not refused: {:?}", other.map(|o| o.program)),
        }
    }
}
