//! #339 slice 6 (fixture library), the app side: the `camcraft-fixtures/1`
//! file (save -> load -> save byte-stable, the envelope's refusals by
//! name), the library folder resolver (`CAMCRAFT_FIXTURES` first), the
//! save / replace / delete file edits, and `InsertFixture` - one undo
//! unit that lands the entry's mesh with its datum on the pick, tagged
//! to the setup, and survives a project save and reload.
//!
//! Gate map (M4.4): covers app::fixture_library, app::commands::insert_fixture.

use std::ffi::OsString;
use std::path::PathBuf;

use camcraft_app::commands::{CreateSetup, History, InsertFixture};
use camcraft_app::fixture;
use camcraft_app::fixture_library::{
    self, DatumChoice, FixtureEntry, LibraryError, add_entries, delete_entry, entry_from_model, library_path,
};
use camcraft_app::geometry::{IDENTITY_Q, Point3, Triangle, quat_from_axis_angle, transform_point};
use camcraft_app::model;
use camcraft_app::project;
use camcraft_app::schema::Location;
use camcraft_app::world::new_world;

fn p(x: f64, y: f64, z: f64) -> Point3 {
    Point3::new(x, y, z)
}

/// A closed, outward box: 12 facets on 8 exact corners.
fn cube(lo: Point3, hi: Point3) -> Vec<Triangle> {
    let c = |i: usize| {
        p(if i & 1 == 0 { lo.x } else { hi.x }, if i & 2 == 0 { lo.y } else { hi.y }, if i & 4 == 0 { lo.z } else { hi.z })
    };
    let quads = [[0, 2, 3, 1], [4, 5, 7, 6], [0, 1, 5, 4], [2, 6, 7, 3], [0, 4, 6, 2], [1, 3, 7, 5]];
    quads
        .iter()
        .flat_map(|q| [Triangle::new(c(q[0]), c(q[1]), c(q[2])), Triangle::new(c(q[0]), c(q[2]), c(q[3]))])
        .collect()
}

fn entry(name: &str) -> FixtureEntry {
    FixtureEntry {
        name: name.into(),
        note: Some("100 mm vise, fixed jaw at Y0".into()),
        datum: Location { pos: p(50.0, 0.0, 0.0), rot: IDENTITY_Q },
        facets: cube(p(0.0, 0.0, 0.0), p(100.0, 40.0, 30.125)),
        step_source: None,
        profile: None,
    }
}

fn scratch(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("camcraft-fixlib-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

fn close(a: Point3, b: Point3) -> bool {
    (a.x - b.x).abs() < 1e-9 && (a.y - b.y).abs() < 1e-9 && (a.z - b.z).abs() < 1e-9
}

#[test]
fn a_library_saves_loads_and_saves_byte_stable_in_its_envelope() {
    let mut plate = entry("Plate");
    plate.note = None;
    plate.datum = Location { pos: p(0.1, -0.2, 1e-7), rot: quat_from_axis_angle(2, 0.3) };
    let text = fixture_library::to_text(&[entry("Vise"), plate.clone()]).expect("written");
    let v: serde_json::Value = serde_json::from_str(&text).expect("json");
    assert_eq!(v["format"], "camcraft-fixtures");
    assert_eq!(v["version"], 1);
    assert_eq!(v["units"], "mm");
    assert_eq!(v["fixtures"][0]["name"], "Plate", "sorted by name");
    assert!(v["fixtures"][0].get("note").is_none(), "no note writes no key");
    let back = fixture_library::parse(&text).expect("reads");
    assert_eq!(back, vec![plate, entry("Vise")]);
    assert_eq!(fixture_library::to_text(&back).expect("again"), text, "byte-stable");
    let empty = fixture_library::to_text(&[]).expect("empty");
    assert_eq!(fixture_library::parse(&empty).expect("empty reads"), Vec::new());
}

#[test]
fn a_bad_library_is_refused_whole_by_name() {
    let good = fixture_library::to_text(&[entry("Vise")]).expect("written");
    let refuse = |text: &str, says: &str| {
        let err = fixture_library::parse(text).expect_err(says);
        assert!(err.contains(says), "{err:?} should say {says:?}");
    };
    refuse(&good.replace("camcraft-fixtures", "camcraft-tools"), "format is 'camcraft-tools'");
    refuse(&good.replace("\"version\": 1", "\"version\": 2"), "version 2 is not supported");
    refuse(&good.replace("\"units\": \"mm\"", "\"units\": \"in\""), "units 'in'");
    refuse("[1, 2]", "expected an object");
    refuse("{", "bad JSON");
    refuse(&good.replacen("[0.0,0.0,0.0]", "[0.0,0.0]", 1), "'Vise': facet #1 is not three");
    refuse(&good.replacen("\"rot\":[0.0,0.0,0.0,1.0]", "\"rot\":[0.0,0.0,0.0,0.0]", 1), "'Vise': the datum rot");
    let mut empty = entry("Vise");
    empty.facets.clear();
    assert!(fixture_library::to_text(&[empty]).expect_err("empty").contains("'Vise' has no facets"));
    let text = good.replacen("\"facets\": [", "\"facets\": [],\"x\": [", 1);
    refuse(&text, "'Vise' has no facets");
    let twice = format!(
        "{{\"format\":\"camcraft-fixtures\",\"version\":1,\"fixtures\":[{0},{0}]}}",
        serde_json::to_string(&serde_json::from_str::<serde_json::Value>(&good).expect("v")["fixtures"][0])
            .expect("row")
    );
    refuse(&twice, "'Vise' appears twice");
    let mut bad = entry("Vise");
    bad.facets[0].a.x = f64::NAN;
    assert!(fixture_library::to_text(&[bad]).expect_err("nan").contains("non-finite"));
    let mut table = entry(fixture::MACHINE_TABLE);
    table.note = None;
    assert!(fixture_library::to_text(&[table]).is_err(), "the table's name is reserved");
}

#[test]
fn the_library_folder_is_the_override_else_a_sibling_of_machines() {
    // Owned copies, so the paths can be picked per host: `is_absolute()` is
    // judged on the host (a Windows host drops "/shop/fixtures"), as in
    // machine_edit.rs's user_dir_for test.
    let env = |pairs: &[(&str, &str)]| {
        let owned: Vec<(String, String)> = pairs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect();
        move |k: &str| owned.iter().find(|(n, _)| n.as_str() == k).map(|(_, v)| OsString::from(v))
    };
    let abs = if cfg!(windows) { "C:\\shop\\fixtures" } else { "/shop/fixtures" };
    let mac_home = if cfg!(windows) { "C:\\Users\\me" } else { "/Users/me" };
    let linux_home = if cfg!(windows) { "C:\\home\\me" } else { "/home/me" };
    let both: &[(&str, &str)] = &[("CAMCRAFT_FIXTURES", abs), ("HOME", mac_home)];
    assert_eq!(fixture_library::user_dir_for("macos", env(both)), Some(PathBuf::from(abs)));
    let home: &[(&str, &str)] = &[("HOME", mac_home), ("CAMCRAFT_FIXTURES", "relative")];
    assert_eq!(
        fixture_library::user_dir_for("macos", env(home)),
        Some(PathBuf::from(mac_home).join("Library").join("Application Support").join("camcraft").join("fixtures"))
    );
    assert_eq!(
        fixture_library::user_dir_for("linux", env(&[("HOME", linux_home)])),
        Some(PathBuf::from(linux_home).join(".local").join("share").join("camcraft").join("fixtures"))
    );
    assert_eq!(fixture_library::user_dir_for("linux", env(&[])), None);
    assert!(library_path(&PathBuf::from("/x")).ends_with("fixtures.camfixtures"));
}

#[test]
fn save_asks_before_replacing_and_delete_edits_the_file() {
    let dir = scratch("edit");
    let path = library_path(&dir);
    assert_eq!(fixture_library::load(&path).expect("missing = empty"), Vec::new());
    add_entries(&path, vec![entry("Vise")], false).expect("first save makes the folder");
    let mut changed = entry("Vise");
    changed.note = Some("re-measured".into());
    assert_eq!(
        add_entries(&path, vec![changed.clone(), entry("Plate")], false),
        Err(LibraryError::Exists(vec!["Vise".into()])),
        "a clash asks first and writes nothing"
    );
    assert_eq!(fixture_library::load(&path).expect("unchanged").len(), 1);
    add_entries(&path, vec![changed.clone(), entry("Plate")], true).expect("replace confirmed");
    let lib = fixture_library::load(&path).expect("two");
    assert_eq!(lib.iter().map(|e| e.name.as_str()).collect::<Vec<_>>(), ["Plate", "Vise"]);
    assert_eq!(lib[1].note.as_deref(), Some("re-measured"));
    delete_entry(&path, "Plate").expect("deleted");
    assert_eq!(delete_entry(&path, "Plate"), Err(LibraryError::NotFound("Plate".into())));
    assert_eq!(fixture_library::load(&path).expect("one"), vec![changed]);
    std::fs::write(&path, "{\"format\": \"other\"}").expect("corrupt");
    assert!(matches!(add_entries(&path, vec![entry("X")], true), Err(LibraryError::Refused(_))));
    assert_eq!(std::fs::read_to_string(&path).expect("kept"), "{\"format\": \"other\"}", "a refused file is not overwritten");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_fixture_saved_at_its_origin_reinserts_where_it_stood_and_bottom_centre_lands_on_the_pick() {
    let world = new_world();
    let mut history = History::default();
    let e = model::spawn(&world, "Jaw");
    model::set_facets(&world, &e, cube(p(-10.0, 0.0, 5.0), p(10.0, 8.0, 25.0)).into());
    let pose = Location { pos: p(3.0, 4.0, 5.0), rot: quat_from_axis_angle(2, 0.5) };
    model::set_transform(&world, &e, pose);
    let at_origin = entry_from_model(&world, "Jaw", DatumChoice::ModelOrigin, Some(" ")).expect("entry");
    assert_eq!(at_origin.datum.pos, Point3::ZERO);
    assert_eq!(at_origin.note, None, "a blank note is no note");
    let cmd = InsertFixture::new(&world, &at_origin, None, pose).expect("insert");
    assert_eq!(cmd.name(), "Jaw (2)", "a taken name takes the next free suffix");
    history.execute(&world, cmd);
    let copy = model::find_by_name(&world, "Jaw (2)").expect("landed");
    let a = model::world_facets(&world, &e).expect("a");
    let b = model::world_facets(&world, &copy).expect("b");
    assert!(a.iter().zip(&b).all(|(s, t)| close(s.a, t.a) && close(s.b, t.b) && close(s.c, t.c)));

    let bottom = entry_from_model(&world, "Jaw", DatumChoice::BottomCentre, None).expect("entry");
    assert!(close(bottom.datum.pos, p(0.0, 4.0, 5.0)));
    let pick = Location { pos: p(100.0, 50.0, 0.0), rot: quat_from_axis_angle(0, std::f64::consts::PI) };
    history.execute(&world, InsertFixture::new(&world, &bottom, None, pick).expect("insert"));
    let flipped = model::find_by_name(&world, "Jaw (3)").expect("landed");
    let loc = model::transform_of(&world, &flipped).expect("pose");
    assert!(close(transform_point(bottom.datum.pos, loc.pos, loc.rot), pick.pos), "the datum is on the pick");
    let bb = model::entity_world_aabb(&world, &flipped).expect("box");
    assert!(
        close(bb.min, p(90.0, 46.0, -20.0)) && close(bb.max, p(110.0, 54.0, 0.0)),
        "flipped about X: it hangs below the pick, {bb:?}"
    );
    assert!(fixture::def_of(&world, &flipped).is_some_and(|d| d.setup.is_none() && !d.fixed));
}

#[test]
fn insert_is_one_undo_unit_tagged_to_its_setup_and_survives_the_project_file() {
    let world = new_world();
    let mut history = History::default();
    let pick = Location { pos: p(10.0, 20.0, 0.0), rot: IDENTITY_Q };
    let missing = InsertFixture::new(&world, &entry("Vise"), Some("Op2"), pick);
    assert!(missing.is_err(), "a gone setup is refused");
    history.execute(&world, CreateSetup::new(&world, Some("Op2"), None).expect("setup"));
    let nan = Location { pos: p(f64::NAN, 0.0, 0.0), rot: IDENTITY_Q };
    assert!(InsertFixture::new(&world, &entry("Vise"), Some("Op2"), nan).is_err(), "a non-finite pick is refused");
    let mut hollow = entry("Vise");
    hollow.facets.clear();
    assert!(InsertFixture::new(&world, &hollow, Some("Op2"), pick).is_err(), "an empty mesh is refused");

    history.execute(&world, InsertFixture::new(&world, &entry("Vise"), Some("Op2"), pick).expect("insert"));
    let tagged = |w: &flecs_ecs::prelude::World| fixture::tagged(w, Some("Op2")).into_iter().map(|(n, _)| n).collect::<Vec<_>>();
    assert_eq!(tagged(&world), ["Vise"]);
    let at = model::transform_of(&world, &model::find_by_name(&world, "Vise").expect("vise")).expect("pose");
    assert!(close(at.pos, p(-40.0, 20.0, 0.0)), "the datum (50,0,0) sits on the pick: {at:?}");
    assert!(history.undo(&world));
    assert!(model::find_by_name(&world, "Vise").is_none(), "one undo takes the whole insert");
    assert!(history.redo(&world));
    assert_eq!(tagged(&world), ["Vise"]);

    let dir = scratch("proj");
    std::fs::create_dir_all(&dir).expect("dir");
    let path = dir.join("lib.camproj");
    project::save(&world, &path).expect("save");
    let world2 = new_world();
    project::load(&world2, &path).expect("load");
    assert_eq!(tagged(&world2), ["Vise"]);
    let v2 = model::find_by_name(&world2, "Vise").expect("vise");
    assert_eq!(model::facets_of(&world2, &v2).map(|f| f.len()), Some(12));
    assert!(close(model::transform_of(&world2, &v2).expect("pose").pos, p(-40.0, 20.0, 0.0)));
    let again = dir.join("again.camproj");
    project::save(&world2, &again).expect("resave");
    assert_eq!(std::fs::read(&path).expect("a"), std::fs::read(&again).expect("b"), "no new project key drifts");
    let _ = std::fs::remove_dir_all(&dir);
}
