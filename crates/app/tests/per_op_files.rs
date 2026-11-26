//! One NC file per operation (#111): the Output option, its command
//! and undo, its file key, and the per-op post - standalone programs,
//! suppressed ops skipped, sanitised names, counted O-words, and a
//! refusal anywhere writing nothing.
//!
//! Gate map (M4.4): covers app::posting (post_per_op, file_stem,
//! op_streams, stream_records), app::commands (SetPostOptions),
//! app::project, app::schema (PostOptions), post::engine
//! (run_numbered, extension_of), assets/posts/*.cps (oWord).

use std::path::{Path, PathBuf};

use flecs_ecs::prelude::World;

use camcraft_app::commands::{
    AddTool, CreateOperation, EditOperationParams, GenerateOperation, History, SelectPost, SetPostOptions,
    SetStock,
};
use camcraft_app::operation::{AfterOperation, Height, OpParams, SrcRef};
use camcraft_app::posting;
use camcraft_app::schema::{PostOptions, PostRef};
use camcraft_app::stock::StockShape;
use camcraft_app::tools::catalog;
use camcraft_app::world::new_world;

fn face_params(cut_z: f64) -> OpParams {
    OpParams::Face {
        holes: Default::default(),
        blend_turns: false, blend_radius: None,
        tilt: None,
        stepover: 3.0,
        clearance_z: Height::fixed(25.0),
        cut_z: Height::fixed(cut_z),
        top_z: None,
        stepdown: None,
        feed: 1200.0,
        plunge_feed: 300.0,
        climb: true,
        picks: Default::default(),
    }
}

/// Two generated face ops on two tools, named as given.
fn two_faces(names: [&str; 2]) -> (World, History) {
    let world = new_world();
    let mut history = History::default();
    let stock = SetStock::new(&world, StockShape::Box { dx: 60.0, dy: 60.0, dz: 20.0 }, Default::default());
    history.execute(&world, stock.expect("box stock"));
    for t in ["EM-0400-2FL", "EM-0600-4FL"] {
        let tool = catalog().into_iter().find(|(n, _)| *n == t).map(|(_, t)| t).expect("catalog entry");
        history.execute(&world, AddTool::new(&world, t, tool).expect("catalog tool"));
    }
    for (name, tool, z) in [(names[0], "EM-0400-2FL", 19.5), (names[1], "EM-0600-4FL", 19.0)] {
        let cmd = CreateOperation::new(&world, name, tool, SrcRef::Stock, face_params(z));
        history.execute(&world, cmd.expect("create"));
        history.execute(&world, GenerateOperation::new(&world, name).expect("generate"));
    }
    (world, history)
}

/// A fresh, empty folder for one test.
fn folder(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("camcraft-per-op-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("temp folder");
    dir
}

/// Every entry in the folder, sorted by name.
fn listing(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(dir)
        .expect("folder")
        .filter_map(Result::ok)
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

fn words(line: &str) -> Vec<&str> {
    line.split_whitespace().collect()
}

#[test]
fn a_two_op_job_writes_two_standalone_programs() {
    let (world, _h) = two_faces(["Face1", "Face2"]);
    let dir = folder("two");
    let files = posting::post_per_op(&world, &dir, "job").expect("the gate releases both");
    assert_eq!(listing(&dir), vec!["job_01_Face1.nc", "job_02_Face2.nc"], "no scratch left behind");
    assert_eq!(files.len(), 2);
    for (k, f) in files.iter().enumerate() {
        let text = std::fs::read_to_string(&f.path).expect("written");
        assert_eq!(text, f.posted.program, "the disk holds what was posted");
        let lines: Vec<&str> = text.lines().collect();
        // Header: the tape mark, then the program's own O-word.
        assert_eq!(lines.first(), Some(&"%"), "{text}");
        assert!(lines[1].starts_with(&format!("O000{}", k + 1)), "{text}");
        // Safe start: absolute, the work offset stated, comp and
        // cycles cancelled - before the tool change.
        let start = lines.iter().position(|l| words(l).contains(&"G54")).expect("the work offset");
        assert!(words(lines[start]).contains(&"G90") && words(lines[start]).contains(&"G80"), "{text}");
        let tool = format!("T0{}", k + 1);
        let m6 = lines.iter().position(|l| words(l) == [tool.as_str(), "M6"]).expect("its own tool change");
        assert!(start < m6, "{text}");
        let spin = lines.iter().position(|l| words(l).contains(&"M3")).expect("its spindle");
        assert!(m6 < spin, "{text}");
        // Its own end: spindle off, home, program end, tape mark.
        let m5 = lines.iter().position(|l| words(l).contains(&"M5")).expect("spindle stop");
        let m30 = lines.iter().position(|l| words(l).contains(&"M30")).expect("program end");
        assert!(spin < m5 && m5 < m30, "{text}");
        assert!(lines[m5..m30].iter().any(|l| words(l) == ["G91", "G28", "Z0"]), "home before the end: {text}");
        assert_eq!(lines.last(), Some(&"%"), "{text}");
        // Only its own operation.
        let other = format!("T0{}", 2 - k);
        assert!(!text.contains(&other), "{other} is the other file's tool: {text}");
        assert_eq!(text.matches("M6").count(), 1, "{text}");
    }
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_combined_program_still_numbers_o0001_and_is_unchanged_by_the_split() {
    let (world, _h) = two_faces(["Face1", "Face2"]);
    let records = posting::stream_records(&world).expect("streams");
    let text = camcraft_app::post::cnc::serialize(&records);
    let out = camcraft_app::post::post(camcraft_app::posting::shipped_posts().expect("the shipped posts read")[0].1, &posting::machine_value(&world), &text).expect("runs");
    assert_eq!(out.program.lines().nth(1).map(|l| l.starts_with("O0001")), Some(true), "{}", out.program);
    let joined: usize = posting::op_streams(&world).expect("per op").iter().map(|(_, r)| r.len()).sum();
    assert_eq!(joined, records.len(), "the combined stream is the op streams joined");
}

#[test]
fn a_suppressed_op_writes_no_file() {
    let (world, mut history) = two_faces(["Face1", "Face2"]);
    // The LAST op: nothing posted after it was generated against its
    // cut (suppressing Face1 would refuse - #110 review B1).
    history.execute(&world, EditOperationParams::with_suppressed(&world, "Face2", true).expect("suppress"));
    let dir = folder("suppressed");
    let files = posting::post_per_op(&world, &dir, "job").expect("posts");
    assert_eq!(files.len(), 1);
    assert_eq!(listing(&dir), vec!["job_01_Face1.nc"], "numbered in the program that posts");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn file_names_are_sanitised() {
    assert_eq!(posting::file_stem("my job", 3, "Face #1 / top"), "my_job_03_Face_1_top");
    assert_eq!(posting::file_stem("..", 1, "***"), "program_01_op");
    assert_eq!(posting::file_stem("p", 12, "Rough-2"), "p_12_Rough-2");
    assert_eq!(posting::file_stem("p", 1, "Ä:\\b"), "p_01_b", "only portable characters");
    let (world, _h) = two_faces(["Face 1", "Face#2"]);
    let dir = folder("names");
    posting::post_per_op(&world, &dir, "a b").expect("posts");
    assert_eq!(listing(&dir), vec!["a_b_01_Face_1.nc", "a_b_02_Face_2.nc"]);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_numbering_dialect_counts_its_o_word_per_file() {
    for post in ["Fanuc", "Mazak"] {
        let (world, mut history) = two_faces(["Face1", "Face2"]);
        history.execute(&world, SelectPost::new(&world, PostRef::Named(post.into())).expect("select"));
        let dir = folder(&format!("onum-{post}"));
        let files = posting::post_per_op(&world, &dir, "job").expect("posts");
        let heads: Vec<String> = files
            .iter()
            .map(|f| f.posted.program.lines().nth(1).unwrap_or_default().split_whitespace().next().unwrap_or_default().to_owned())
            .collect();
        assert_eq!(heads, vec!["O0001", "O0002"], "{post}");
        let _ = std::fs::remove_dir_all(&dir);
    }
}

#[test]
fn a_refusal_in_op_two_writes_nothing() {
    // Heidenhain cannot trust a home: a home after op 2 is a counted
    // refusal in op 2's file alone - and no file is written at all.
    let (world, mut history) = two_faces(["Face1", "Face2"]);
    history.execute(&world, SelectPost::new(&world, PostRef::Named("Heidenhain".into())).expect("select"));
    history.execute(&world, EditOperationParams::with_after(&world, "Face2", AfterOperation::GoHome).expect("after"));
    let dir = folder("refused");
    let err = posting::post_per_op(&world, &dir, "job").expect_err("op 2 refuses");
    assert!(err.contains("Face2") && err.contains("home"), "named: {err}");
    assert!(listing(&dir).is_empty(), "nothing written, not even op 1: {:?}", listing(&dir));
    // A missing folder refuses by name too.
    let gone = dir.join("nope");
    assert!(posting::post_per_op(&world, &gone, "job").expect_err("no folder").contains("not a folder"));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_output_option_undoes_and_redoes() {
    let (world, mut history) = two_faces(["Face1", "Face2"]);
    assert!(!posting::post_options(&world).per_op, "one program is the default");
    let cmd = SetPostOptions::new(&world, PostOptions { per_op: true, ..Default::default() }).expect("switch");
    history.execute(&world, cmd);
    assert!(posting::post_options(&world).per_op);
    assert!(SetPostOptions::new(&world, PostOptions { per_op: true, ..Default::default() }).is_err(), "a no-op is refused");
    history.undo(&world);
    assert!(!posting::post_options(&world).per_op, "undo restores one program");
    history.redo(&world);
    assert!(posting::post_options(&world).per_op, "redo reapplies it");
}

#[test]
fn the_output_option_round_trips_and_old_files_load_one_program() {
    let (world, mut history) = two_faces(["Face1", "Face2"]);
    let path = std::env::temp_dir().join(format!("camcraft-per-op-{}.json", std::process::id()));
    camcraft_app::project::save(&world, &path).expect("save");
    let text = std::fs::read_to_string(&path).expect("written");
    assert!(!text.contains("per_op_files"), "the default writes no key: {text}");
    let cmd = SetPostOptions::new(&world, PostOptions { per_op: true, ..Default::default() }).expect("switch");
    history.execute(&world, cmd);
    camcraft_app::project::save(&world, &path).expect("save");
    let text = std::fs::read_to_string(&path).expect("written");
    assert!(text.contains("\"per_op_files\": true"), "{text}");
    let back = new_world();
    camcraft_app::project::load(&back, &path).expect("load");
    let _ = std::fs::remove_file(&path);
    assert_eq!(posting::post_options(&back), PostOptions { per_op: true, ..PostOptions::default() });
}

#[test]
fn the_lathe_post_numbers_its_programs_the_same_way() {
    let lathe = camcraft_app::posting::shipped_posts().expect("the shipped posts read")
        .iter()
        .find(|(n, _)| n.to_lowercase().contains("lathe"))
        .map(|(_, s)| *s)
        .expect("a lathe post");
    let mch = posting::machine_value(&new_world());
    for (n, word) in [(1, "O0001"), (3, "O0003")] {
        let out = camcraft_app::post::engine::run_numbered(lathe, &mch, "", n).expect("runs");
        assert!(out.program.lines().any(|l| l.split_whitespace().next() == Some(word)), "{}", out.program);
    }
    assert_eq!(camcraft_app::post::engine::extension_of(lathe).ok().as_deref(), Some("nc"));
}
