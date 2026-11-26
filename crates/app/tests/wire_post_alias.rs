//! Gate map: covers app::posting::agie (the registry name and its hidden alias),
//! app::project (a saved post name and option key), app::commands::SelectPost.
//! Provenance: issue #3537 - the native wire EDM post was renamed; its old
//! name stays a hidden alias so a saved project still opens and posts.

use std::collections::BTreeMap;

use camcraft_app::commands::{Command, History, SelectPost, SetPostScriptOptions};
use camcraft_app::post::options::OptionValue;
use camcraft_app::posting::{self, agie};
use camcraft_app::schema::{PostRef, PostScriptOptions};
use camcraft_app::{project, setup_post_options};

fn tmp_path(tag: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!("camcraft_alias_{tag}_{}.camproj", std::process::id()))
}

#[test]
fn the_new_name_is_listed_and_the_old_name_is_not() {
    assert_eq!(agie::POST, "Native wire EDM job files");
    assert!(posting::WIRE_POSTS.contains(&agie::POST));
    assert!(posting::BUILTINS.iter().any(|(n, _)| *n == agie::POST));
    for (n, _) in posting::BUILTINS {
        assert!(!n.contains("Agie"), "a listed post names the machine: {n}");
    }
    assert!(!posting::WIRE_POSTS.contains(&agie::LEGACY_POST));
}

#[test]
fn the_old_name_selects_the_new_post_by_command_script_and_lookup() {
    assert_eq!(agie::current_name(agie::LEGACY_POST), agie::POST);
    assert_eq!(agie::current_name("Generic ISO Wire EDM"), "Generic ISO Wire EDM");
    assert!(agie::is_native(&PostRef::Named(agie::LEGACY_POST.into())));

    // A script or MCP `cps` with the old name resolves to the new name.
    let (post, text) = posting::resolve_cps(agie::LEGACY_POST).expect("the alias resolves");
    assert_eq!(post, PostRef::Named(agie::POST.into()));
    assert_eq!(text, posting::builtin_post_text(agie::POST).expect("the post text reads"));

    // The command stores the registry name; undo restores the previous post.
    let world = camcraft_app::world::new_world();
    let before = posting::document_post_of(&world);
    let mut h = History::default();
    h.execute(&world, SelectPost::new(&world, PostRef::Named(agie::LEGACY_POST.into())).expect("the alias selects"));
    assert_eq!(posting::document_post_of(&world), PostRef::Named(agie::POST.into()));
    assert_eq!(posting::post_ref_label(&posting::document_post_of(&world)), format!("{} (built-in)", agie::POST));
    assert!(
        SelectPost::new(&world, PostRef::Named(agie::POST.into())).is_err(),
        "the alias and the name are the same post"
    );
    h.undo(&world);
    assert_eq!(posting::document_post_of(&world), before);
}

#[test]
fn a_project_saved_with_the_old_name_opens_posts_and_saves_the_new_name() {
    let world = camcraft_app::world::new_world();
    let mut h = History::default();
    h.execute(&world, SelectPost::new(&world, PostRef::Named(agie::POST.into())).expect("select"));
    let key = format!("builtin:{}", agie::POST);
    let ov = BTreeMap::from([("separationLengthMm".to_owned(), OptionValue::Float(1.0))]);
    let map = PostScriptOptions { by_post: BTreeMap::from([(key.clone(), ov.clone())]) };
    let cmd = SetPostScriptOptions::new(&world, map).expect("options");
    cmd.apply(&world);

    // Write a file as the old release wrote it: the old name as the post
    // and in the option key.
    let p = tmp_path("legacy");
    project::save(&world, &p).expect("save");
    let new_text = std::fs::read_to_string(&p).expect("read");
    assert!(new_text.contains(agie::POST), "{new_text}");
    let old_text = new_text.replace(agie::POST, agie::LEGACY_POST);
    assert!(old_text.contains(&format!("builtin:{}", agie::LEGACY_POST)), "{old_text}");

    let opened = camcraft_app::world::new_world();
    project::open(&opened, &old_text).expect("a project with the old name opens");
    assert_eq!(posting::document_post_of(&opened), PostRef::Named(agie::POST.into()));
    assert!(posting::script_source(&opened).is_ok(), "the old project posts with the shipped script");
    let doc = setup_post_options::script_document(&opened);
    assert_eq!(doc.for_post(&key), &ov, "the option under the old key moves to the new key");
    assert!(doc.by_post.keys().all(|k| !k.contains(agie::LEGACY_POST)));

    // A save after the open writes the new name only.
    project::save(&opened, &p).expect("save again");
    let saved = std::fs::read_to_string(&p).expect("read");
    let _ = std::fs::remove_file(&p);
    assert!(!saved.contains(agie::LEGACY_POST), "{saved}");
    assert!(saved.contains(&key), "{saved}");
}

#[test]
fn an_option_map_under_the_old_key_normalizes_to_the_new_key() {
    let ov = BTreeMap::from([("separationLengthMm".to_owned(), OptionValue::Float(1.0))]);
    let mut map = PostScriptOptions {
        by_post: BTreeMap::from([(format!("builtin:{}", agie::LEGACY_POST), ov.clone())]),
    };
    map.normalize();
    assert_eq!(map.for_post(&format!("builtin:{}", agie::POST)), &ov);
    assert_eq!(map.by_post.len(), 1);
    assert_eq!(agie::current_key("builtin:Fanuc"), "builtin:Fanuc");
}
