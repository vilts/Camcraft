//! The default machine file is the post's machine when a document mounts
//! none (#2319). A data directory whose posts folder is present but whose
//! `3axis-vmc.mch` is missing or malformed must refuse the post by the
//! file's name and write no program - never post with a null machine (no
//! limits, no coolant words). The spawns are real processes: the CLI exe
//! is copied beside a hand-made `assets/` folder, so the run-time lookup
//! reads that folder and no other.
//!
//! The three post paths are pinned: one program, one file per mount, one
//! file per operation. Each refusal writes no file at all, also none for a
//! mounted setup or operation posted before the refused one.
//!
//! Gate map: covers app::posting (default_machine_checked,
//! parse_default_machine, posted_machine_for, post_mounts, post_per_op),
//! app::cli.

use camcraft_app::commands::{AddTool, CreateOperation, CreateSetup, History, SetPostOptions, SetSetupMachine, SetStock};
use camcraft_app::machine::Machine;
use camcraft_app::operation::{Height, OpParams, SrcRef};
use camcraft_app::posting::{parse_default_machine, KeptMachine};
use camcraft_app::project;
use camcraft_app::schema::PostOptions;
use camcraft_app::stock::StockShape;
use camcraft_app::tools::catalog;
use camcraft_app::world::new_world;
use flecs_ecs::prelude::World;

use std::path::{Path, PathBuf};
use std::process::Command;

/// Which document the CLI posts.
#[derive(Clone, Copy)]
enum Doc {
    /// Box stock and one facing op, no machine mounted: the post reads the
    /// default machine file.
    Plain,
    /// Setup1 mounts a machine, Setup2 mounts none: one file per mount.
    PerMount,
    /// The same two setups posted one file per operation.
    PerOp,
}

fn face() -> OpParams {
    OpParams::Face {
        holes: Default::default(),
        blend_turns: false, blend_radius: None,
        tilt: None,
        stepover: 3.0,
        clearance_z: Height::fixed(25.0),
        cut_z: Height::fixed(19.5),
        top_z: None,
        stepdown: None,
        feed: 1200.0,
        plunge_feed: 300.0,
        climb: true,
        picks: Default::default(),
    }
}

fn fixture(doc: Doc) -> World {
    let world = new_world();
    let mut h = History::default();
    let tool = catalog().into_iter().find(|(n, _)| *n == "EM-0400-2FL").map(|(_, t)| t).expect("catalog entry");
    h.execute(&world, AddTool::new(&world, "EM-0400-2FL", tool).expect("catalog tool"));
    let stock = |h: &mut History| {
        let cmd = SetStock::new(&world, StockShape::Box { dx: 60.0, dy: 60.0, dz: 20.0 }, Default::default());
        h.execute(&world, cmd.expect("box stock"));
    };
    let op = |h: &mut History, name: &str| {
        h.execute(&world, CreateOperation::new(&world, name, "EM-0400-2FL", SrcRef::Stock, face()).expect("create"));
    };
    if let Doc::Plain = doc {
        stock(&mut h);
        op(&mut h, "Face1");
        return world;
    }
    h.execute(&world, CreateSetup::new(&world, None, None).expect("Setup1"));
    stock(&mut h);
    let mill = Machine { model: "Bench mill".into(), kind: Some("mill".into()), ..Default::default() };
    h.execute(&world, SetSetupMachine::new(&world, "Setup1", Some(("bench-mill.mch".into(), Some(mill)))).expect("mount"));
    op(&mut h, "Face1");
    h.execute(&world, CreateSetup::new(&world, None, None).expect("Setup2"));
    stock(&mut h);
    op(&mut h, "Face2");
    if let Doc::PerOp = doc {
        let cmd = SetPostOptions::new(&world, PostOptions { per_op: true, ..Default::default() });
        h.execute(&world, cmd.expect("one file per operation"));
    }
    world
}

/// A fresh folder with the CLI exe and `assets/posts` copied in, and an
/// `assets/machines` folder that `machines` fills.
fn install(tag: &str, machines: impl FnOnce(&Path)) -> PathBuf {
    let root = std::env::temp_dir().join(format!("camcraft-2319-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let posts = root.join("assets").join("posts");
    std::fs::create_dir_all(&posts).expect("posts folder");
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/posts");
    for entry in std::fs::read_dir(&src).expect("shipped posts") {
        let entry = entry.expect("entry");
        if entry.file_type().is_ok_and(|t| t.is_file()) {
            std::fs::copy(entry.path(), posts.join(entry.file_name())).expect("copy post");
        }
    }
    let mdir = root.join("assets").join("machines");
    std::fs::create_dir_all(&mdir).expect("machines folder");
    machines(&mdir);
    let exe = Path::new(env!("CARGO_BIN_EXE_camcraft-cli"));
    std::fs::copy(exe, root.join(exe.file_name().expect("exe name"))).expect("copy cli");
    root
}

/// Posts the fixture with the installed CLI into `out/`: (exit code,
/// stderr, the text of each file written there).
fn post_with(root: &Path, doc: Doc) -> (i32, String, Vec<String>) {
    let proj = root.join("job.camproj");
    project::save(&fixture(doc), &proj).expect("save fixture");
    let out = root.join("out");
    std::fs::create_dir_all(&out).expect("out folder");
    let nc = out.join("job.nc");
    let exe = root.join(Path::new(env!("CARGO_BIN_EXE_camcraft-cli")).file_name().expect("exe name"));
    let o = Command::new(exe)
        .env("RUST_LOG", "info")
        .args(["process", proj.to_str().expect("utf8"), "-o", nc.to_str().expect("utf8")])
        .env("CAMCRAFT_LOG", "info")
        .output()
        .expect("the CLI runs");
    let code = o.status.code().unwrap_or(-1);
    let err = String::from_utf8_lossy(&o.stderr).into_owned();
    let files = std::fs::read_dir(&out).expect("out folder").flatten();
    let written = files.filter_map(|e| std::fs::read_to_string(e.path()).ok()).collect();
    (code, err, written)
}

fn assert_refused(tag: &str, doc: Doc, machines: impl FnOnce(&Path)) -> String {
    let root = install(tag, machines);
    let (code, err, written) = post_with(&root, doc);
    let _ = std::fs::remove_dir_all(&root);
    assert!(written.is_empty(), "{tag}: no file is written; got:\n{}", written.join("\n----\n"));
    assert_eq!(code, 1, "{tag}: the post refuses: {err}");
    assert!(err.contains("3axis-vmc.mch"), "{tag}: the refusal names the file: {err}");
    assert!(err.contains("default machine"), "{tag}: the refusal says which machine: {err}");
    err
}

#[test]
fn a_missing_default_machine_file_refuses_the_post() {
    assert_refused("missing", Doc::Plain, |_| {});
}

#[test]
fn a_missing_default_machine_file_refuses_the_post_per_mount() {
    let err = assert_refused("per-mount", Doc::PerMount, |_| {});
    assert!(err.contains("Setup2"), "the refusal names the unmounted setup: {err}");
}

#[test]
fn a_missing_default_machine_file_refuses_the_post_per_operation() {
    let err = assert_refused("per-op", Doc::PerOp, |_| {});
    assert!(err.contains("Face2"), "the output names the operation of the unmounted setup: {err}");
    assert!(
        err.lines().any(|line| !line.starts_with("camcraft-cli: post:") && line.contains("Face1")),
        "generation logs name the mounted operation: {err}"
    );
    // Generation logs name both operations; check only the CLI refusal.
    let err = err.lines().find(|line| line.starts_with("camcraft-cli: post:")).expect("CLI post refusal");
    assert!(err.contains("Setup2"), "the refusal names the unmounted setup: {err}");
    assert!(err.contains("Face2"), "the refusal names the operation of the unmounted setup: {err}");
    assert!(!err.contains("Face1"), "the mounted operation has its own machine: {err}");
}

#[test]
fn a_malformed_default_machine_file_refuses_the_post() {
    assert_refused("malformed", Doc::Plain, |dir| {
        std::fs::write(dir.join("3axis-vmc.mch"), "{ \"kinematics\": ").expect("write malformed file");
    });
}

#[test]
fn an_empty_object_as_default_machine_file_refuses_the_post() {
    // `{}` is JSON and an object, but not a machine: it once posted
    // `(CAMCRAFT)` with no coolant words.
    assert_refused("empty-object", Doc::Plain, |dir| {
        std::fs::write(dir.join("3axis-vmc.mch"), "{}").expect("write empty object");
    });
}

#[test]
fn a_good_default_machine_file_still_posts() {
    // The control: the same install with the shipped file posts as before.
    let root = install("good", |dir| {
        let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/machines/3axis-vmc.mch");
        std::fs::copy(src, dir.join("3axis-vmc.mch")).expect("copy default machine");
    });
    let (code, err, written) = post_with(&root, Doc::Plain);
    let _ = std::fs::remove_dir_all(&root);
    assert_eq!(code, 0, "{err}");
    assert_eq!(written.len(), 1, "one program is written");
    assert!(written[0].contains("G01") && written[0].contains("M8"), "a program with coolant:\n{}", written[0]);
}

#[test]
fn the_default_machine_file_must_be_a_machine_object() {
    for (text, why) in [
        ("[]", "not a machine object"),
        ("\"x\"", "not a machine object"),
        ("null", "not a machine object"),
        ("{}", "no machine name"),
        (r#"{ "name": " ", "kinematics": { "static": {} } }"#, "no machine name"),
        (r#"{ "name": "VMC" }"#, "no kinematics.static"),
        (r#"{ "name": "VMC", "kinematics": { "static": [] } }"#, "no kinematics.static"),
    ] {
        let err = parse_default_machine(Ok(text)).expect_err(text);
        assert!(err.contains("3axis-vmc.mch") && err.contains(why), "{text}: {err}");
    }
    let err = parse_default_machine(Err("gone".into())).expect_err("unreadable");
    assert!(err.contains("3axis-vmc.mch cannot be read (gone)"), "{err}");
}

#[test]
fn a_repaired_default_machine_file_is_read_again_and_a_good_one_is_kept() {
    // Missing, then `{}`, then the good file: Err, Err, Ok - no refusal or
    // refused text is kept until restart. After that the good machine is
    // kept: a later broken file is not read.
    let good = std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/machines/3axis-vmc.mch"),
    )
    .expect("shipped default machine");
    let kept = KeptMachine::new();
    let missing = kept.get_or_read(|| Err("not found".into())).expect_err("missing");
    assert!(missing.contains("cannot be read"), "{missing}");
    let empty = kept.get_or_read(|| Ok("{}".into())).expect_err("empty object");
    assert!(empty.contains("no machine name"), "{empty}");
    let v = kept.get_or_read(|| Ok(good)).expect("the repaired file posts");
    assert_eq!(v["name"], "VMC 3-AXIS");
    let again = kept.get_or_read(|| Ok("{}".into())).expect("the good machine is kept");
    assert_eq!(again, v);
}

#[test]
fn the_shipped_default_machine_file_is_accepted() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/machines/3axis-vmc.mch");
    let text = std::fs::read_to_string(path).expect("shipped default machine");
    let v = parse_default_machine(Ok(&text)).expect("the shipped file is a machine");
    assert_eq!(v["name"], "VMC 3-AXIS");
}
