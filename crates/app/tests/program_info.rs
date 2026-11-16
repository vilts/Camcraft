//! #258: the document's program number and comment - stated through
//! the undoable `SetProgramInfo` (refused by name at creation), saved in
//! the project (v61; an older file opens with nothing stated), and
//! written by every post in its own dialect: the O-word (Fanuc, Fanuc
//! Lathe, Mazak, Haas), the program name (Heidenhain `%NAME G71 *`,
//! SINUMERIK `%_N_NAME_MPF`), a header comment (PathPilot). Nothing
//! stated posts byte-identical to before (O0001, the machine's name).
//! Provenance: our own fixtures; the dialect rules' sources are cited
//! in assets/posts/*.cps and app::posting's program module.
//!
//! Gate map (#258): covers app::commands (SetProgramInfo),
//! app::posting (program_info, program_id, parse_program_number,
//! post_script), app::project (ProgramDto, v61), post::engine
//! (run_program_with), post::js (ProgramId, the `program` global),
//! assets/posts/*.cps (headers).
//! Lane shape (M4.4): all tests here run in seconds.

use camcraft_app::cl::{Move, MoveKind, Toolpath};
use camcraft_app::commands::{
    AddTool, CreateOperation, GenerateOperation, History, SetProgramInfo, SetStock,
};
use camcraft_app::geometry::Point3;
use camcraft_app::operation::{Height, OpParams, SrcRef};
use camcraft_app::post::{cnc, dump, engine, Dump, ProgramId};
use camcraft_app::posting;
use camcraft_app::project;
use camcraft_app::schema::ProgramInfo;
use camcraft_app::stock::StockShape;
use camcraft_app::tools::catalog;
use camcraft_app::world::new_world;
use std::path::Path;

fn read(rel: &str) -> String {
    let p = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../").join(rel);
    std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("{rel}: {e}"))
}

fn mill() -> serde_json::Value {
    serde_json::from_str(&read("assets/machines/3axis-vmc.mch")).expect("machine json")
}

fn stream() -> String {
    let p = |x, y, z| Point3 { x, y, z };
    let t = Toolpath::new(vec![
        Move::new(MoveKind::Rapid, p(0.0, 0.0, 10.0), p(5.0, 5.0, 10.0), None),
        Move::new(MoveKind::Feed, p(5.0, 5.0, 10.0), p(5.0, 5.0, -1.0), Some(200.0)),
        Move::new(MoveKind::Feed, p(5.0, 5.0, -1.0), p(25.0, 5.0, -1.0), Some(800.0)),
        Move::new(MoveKind::Rapid, p(25.0, 5.0, -1.0), p(25.0, 5.0, 10.0), None),
    ]);
    let d = Dump {
        op_name: "Contour1",
        op_kind: "2D Contour",
        setup_name: "Setup1",
        tool_number: 3,
        tool_name: "EM-0600",
        tool_kind: "flat",
        tool_diameter: 6.0,
        rpm: 8000.0,
        clearance_z: 10.0,
        retract_z: 5.0,
        ..Default::default()
    };
    cnc::serialize(&dump(&t, &d))
}

fn id(number: Option<u32>, comment: &str) -> ProgramId {
    ProgramId { number: number.unwrap_or(1), numbered: number.is_some(), comment: comment.to_owned() }
}

fn post(stem: &str, id: &ProgramId) -> Result<String, String> {
    let mch = if stem == "fanuc-lathe" { serde_json::Value::Null } else { mill() };
    let s = if stem == "fanuc-lathe" { String::new() } else { stream() };
    engine::run_program_with(&read(&format!("assets/posts/{stem}.cps")), &mch, &s, &serde_json::Value::Null, id)
        .map(|o| {
            assert!(o.warnings.is_empty(), "{stem}: {:?}", o.warnings);
            o.program
        })
        .map_err(|e| e.to_string())
}

const POSTS: &[&str] = &["fanuc", "fanuc-lathe", "mazak", "haas", "tormach", "heidenhain", "sinumerik"];

/// Nothing stated is what every post wrote before #258, byte for byte.
#[test]
fn nothing_stated_posts_byte_identical_in_every_dialect() {
    for stem in POSTS {
        let mch = if *stem == "fanuc-lathe" { serde_json::Value::Null } else { mill() };
        let s = if *stem == "fanuc-lathe" { String::new() } else { stream() };
        let before = engine::run(&read(&format!("assets/posts/{stem}.cps")), &mch, &s).expect("posts").program;
        assert_eq!(post(stem, &ProgramId::default()).expect("posts"), before, "{stem}");
    }
}

/// Each dialect's header with a stated number and comment.
#[test]
fn a_stated_number_and_comment_post_in_each_dialect() {
    let both = id(Some(1234), "Bracket A");
    let lines = |stem: &str| post(stem, &both).unwrap_or_else(|e| panic!("{stem}: {e}"));
    for stem in ["fanuc", "fanuc-lathe", "mazak"] {
        let p = lines(stem);
        assert_eq!(p.lines().nth(1), Some("O1234 (Bracket A)"), "{stem}: {p}");
    }
    let haas = lines("haas");
    assert_eq!(haas.lines().nth(1), Some("O01234 (Bracket A)"), "Haas: five-digit O-word: {haas}");
    let tormach = lines("tormach");
    assert_eq!(tormach.lines().next(), Some("(PROGRAM 1234 - Bracket A)"), "PathPilot: no O-word: {tormach}");
    assert!(!tormach.lines().any(|l| l.starts_with('O')), "an O word is flow control on PathPilot: {tormach}");
    let tnc = lines("heidenhain");
    assert_eq!(tnc.lines().next(), Some("%Bracket_A G71 *"), "{tnc}");
    assert_eq!(tnc.lines().nth(1), Some("N10 ; PROGRAM 1234*"), "{tnc}");
    assert!(tnc.trim_end().ends_with("%Bracket_A G71 *"), "the end block names it too: {tnc}");
    let sin = lines("sinumerik");
    assert_eq!(sin.lines().next(), Some("%_N_Bracket_A_MPF"), "{sin}");
    assert_eq!(sin.lines().nth(1), Some("; PROGRAM 1234 - Bracket A"), "{sin}");
}

/// An inch document (#902) keeps the stated identity: the same header
/// as the mm program, then the dialect's inch word, never its mm one.
#[test]
fn a_stated_identity_posts_the_same_header_in_an_inch_program() {
    let both = id(Some(1234), "Bracket A");
    let inch = serde_json::json!({ "Units": "inch" });
    for (stem, word, mm) in [
        ("fanuc", "G20", "G21"),
        ("fanuc-lathe", "G20", "G21"),
        ("mazak", "G20", "G21"),
        ("haas", "G20", "G21"),
        ("tormach", "G20", "G21"),
        ("sinumerik", "G700", "G710"),
    ] {
        let mch = if stem == "fanuc-lathe" { serde_json::Value::Null } else { mill() };
        let s = if stem == "fanuc-lathe" { String::new() } else { stream() };
        let out = engine::run_program_with(&read(&format!("assets/posts/{stem}.cps")), &mch, &s, &inch, &both)
            .unwrap_or_else(|e| panic!("{stem} inch: {e}"));
        assert!(out.warnings.is_empty(), "{stem}: {:?}", out.warnings);
        let p = out.program;
        let metric = post(stem, &both).unwrap_or_else(|e| panic!("{stem}: {e}"));
        // The identity lines: `%` + O-word / `%_N_` + comment; PathPilot's
        // header comment alone (its unit block is the next line).
        let n = if stem == "tormach" { 1 } else { 2 };
        let head = |t: &str| t.lines().take(n).map(str::to_owned).collect::<Vec<_>>();
        assert_eq!(head(&p), head(&metric), "{stem}: the header does not depend on the unit: {p}");
        let words = |l: &str| l.split_whitespace().map(str::to_owned).collect::<Vec<_>>();
        assert!(p.lines().any(|l| words(l).iter().any(|w| w == word)), "{stem}: inch word {word}: {p}");
        assert!(!p.lines().any(|l| words(l).iter().any(|w| w == mm)), "{stem}: no mm word {mm}: {p}");
    }
}

/// The number alone: the O-word dialects keep the machine-name comment;
/// the TNC is named by the number; SINUMERIK has no number word.
#[test]
fn a_stated_number_alone_names_each_dialect() {
    let n = id(Some(42), "");
    let fanuc = post("fanuc", &n).expect("fanuc");
    assert!(fanuc.lines().nth(1).is_some_and(|l| l.starts_with("O0042 (")), "{fanuc}");
    let haas = post("haas", &n).expect("haas");
    assert!(haas.lines().nth(1).is_some_and(|l| l.starts_with("O00042 (")), "{haas}");
    let tnc = post("heidenhain", &n).expect("tnc");
    assert_eq!(tnc.lines().next(), Some("%42 G71 *"), "{tnc}");
    let sin = post("sinumerik", &n).expect("sinumerik");
    assert!(!sin.contains("_MPF"), "a number names no SINUMERIK program: {sin}");
    assert!(sin.lines().next().is_some_and(|l| l.starts_with("; PROGRAM 42 - ")), "{sin}");
}

/// A dialect's own rule refuses by name at post time: a SINUMERIK
/// program name must start with two letters; an O-word past its range.
#[test]
fn a_dialects_own_rule_refuses_by_name() {
    let e = post("sinumerik", &id(None, "12 bracket")).expect_err("a digit-first name");
    assert!(e.contains("SINUMERIK") && e.contains("two letters"), "{e}");
    let e = post("fanuc", &id(Some(10000), "")).expect_err("past O9999");
    assert!(e.contains("O0001-O9999"), "{e}");
    let e = post("haas", &id(Some(100000), "")).expect_err("past O99999");
    assert!(e.contains("O00001-O99999"), "{e}");
}

#[test]
fn set_program_info_validates_commits_and_undoes() {
    let world = new_world();
    let mut history = History::default();
    let ok = ProgramInfo { number: Some(1234), comment: Some("  Bracket A  ".into()) };
    history.execute(&world, SetProgramInfo::new(&world, ok).expect("valid"));
    let set = posting::program_info(&world);
    assert_eq!(set, ProgramInfo { number: Some(1234), comment: Some("Bracket A".into()) }, "trimmed");
    assert!(history.undo(&world));
    assert_eq!(posting::program_info(&world), ProgramInfo::default(), "undo restores nothing stated");
    assert!(history.redo(&world));
    assert_eq!(posting::program_info(&world), set, "redo reapplies it");

    let refused = |info: ProgramInfo, word: &str| {
        let e = SetProgramInfo::new(&world, info.clone()).expect_err("refused").to_string();
        assert!(e.contains(word), "{info:?}: {e}");
    };
    refused(ProgramInfo { number: Some(0), comment: None }, "outside 1 to 9999");
    refused(ProgramInfo { number: Some(10000), comment: None }, "outside 1 to 9999");
    refused(ProgramInfo { number: None, comment: Some("A (B)".into()) }, "'('");
    refused(ProgramInfo { number: None, comment: Some("50%".into()) }, "'%'");
    refused(ProgramInfo { number: None, comment: Some("Bräcket".into()) }, "printable ASCII");
    refused(ProgramInfo { number: None, comment: Some("x".repeat(41)) }, "at most 40");
    refused(set.clone(), "already set");
    // A blank comment is no comment.
    let cleared = SetProgramInfo::new(&world, ProgramInfo { number: Some(1234), comment: Some("   ".into()) }).expect("clear");
    history.execute(&world, cleared);
    assert_eq!(posting::program_info(&world).comment, None);
}

#[test]
fn a_typed_number_parses_or_refuses_by_name() {
    assert_eq!(posting::parse_program_number(""), Ok(None));
    assert_eq!(posting::parse_program_number(" 12 "), Ok(Some(12)));
    assert_eq!(posting::parse_program_number("O0012"), Ok(Some(12)));
    assert!(posting::parse_program_number("12a").is_err_and(|e| e.contains("not a whole number")));
    assert!(posting::parse_program_number("-3").is_err_and(|e| e.contains("not a whole number")));
    assert!(posting::parse_program_number("O").is_err_and(|e| e.contains("no digits")));
}

/// One file per operation counts up from the stated number (#111), and
/// a count past O9999 refuses by name rather than wrapping.
#[test]
fn per_op_files_count_up_from_the_stated_number() {
    let none = ProgramInfo::default();
    assert_eq!(posting::program_id(&none, 3).expect("id"), ProgramId { number: 3, numbered: false, comment: String::new() });
    let info = ProgramInfo { number: Some(100), comment: Some("Lid".into()) };
    assert_eq!(posting::program_id(&info, 1).expect("id"), id(Some(100), "Lid"));
    assert_eq!(posting::program_id(&info, 3).expect("id"), id(Some(102), "Lid"));
    let top = ProgramInfo { number: Some(9998), comment: None };
    assert!(posting::program_id(&top, 2).is_ok());
    let e = posting::program_id(&top, 3).expect_err("past O9999");
    assert!(e.contains("past O9999"), "{e}");
}

fn face_world() -> (flecs_ecs::prelude::World, History) {
    let world = new_world();
    let mut history = History::default();
    let stock = SetStock::new(&world, StockShape::Box { dx: 60.0, dy: 60.0, dz: 20.0 }, Default::default());
    history.execute(&world, stock.expect("box stock"));
    let tool = catalog().into_iter().find(|(n, _)| *n == "EM-0600-4FL").map(|(_, t)| t).expect("catalog entry");
    history.execute(&world, AddTool::new(&world, "EM-0600-4FL", tool).expect("catalog tool"));
    let params = OpParams::Face {
        holes: Default::default(),
        blend_turns: false,
        tilt: None,
        stepover: 3.0,
        clearance_z: Height::fixed(25.0),
        cut_z: Height::fixed(19.5),
        top_z: None,
        stepdown: None,
        feed: 1200.0,
        plunge_feed: 300.0,
        climb: true,
    };
    let cmd = CreateOperation::new(&world, "Face1", "EM-0600-4FL", SrcRef::Stock, params);
    history.execute(&world, cmd.expect("create"));
    history.execute(&world, GenerateOperation::new(&world, "Face1").expect("generate"));
    (world, history)
}

/// The live post: the document's number and comment reach the file, and
/// undo puts O0001 back.
#[test]
fn the_document_post_writes_the_stated_number() {
    let (world, mut history) = face_world();
    let dir = std::env::temp_dir().join(format!("camcraft-program-info-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    let out = dir.join("p.nc");
    let info = ProgramInfo { number: Some(77), comment: Some("Lid".into()) };
    history.execute(&world, SetProgramInfo::new(&world, info).expect("valid"));
    let posted = posting::post_script(&world, &out, &read("assets/posts/fanuc.cps")).expect("posts");
    assert_eq!(posted.program.lines().nth(1), Some("O0077 (Lid)"), "{}", posted.program);
    history.undo(&world);
    let posted = posting::post_script(&world, &out, &read("assets/posts/fanuc.cps")).expect("posts");
    assert!(posted.program.lines().nth(1).is_some_and(|l| l.starts_with("O0001 (")), "{}", posted.program);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_program_info_round_trips_and_an_older_file_states_none() {
    const _: () = assert!(project::PROJECT_VERSION >= 61, "the program key needs project v61 or later");
    let world = new_world();
    let info = ProgramInfo { number: Some(1234), comment: Some("Bracket A".into()) };
    let mut history = History::default();
    history.execute(&world, SetProgramInfo::new(&world, info.clone()).expect("valid"));
    let dir = std::env::temp_dir().join(format!("camcraft-program-rt-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    let path = dir.join("p.camproj");
    project::save(&world, &path).expect("save");
    let text = std::fs::read_to_string(&path).expect("read");
    assert!(text.contains("\"program\"") && text.contains("\"number\": 1234"), "{text}");
    let world2 = new_world();
    project::load(&world2, &path).expect("load");
    assert_eq!(posting::program_info(&world2), info);
    // An older file (v58, no key) opens with nothing stated - and lands
    // that over a stale value, never leaking the previous document's.
    project::open(&world2, r#"{"version": 58, "entities": []}"#).expect("v58 opens");
    assert_eq!(posting::program_info(&world2), ProgramInfo::default());
    // A v60 file (#902's unit law, the step before this key) steps up
    // to v61 unchanged: nothing stated, no load note.
    History::default().execute(&world2, SetProgramInfo::new(&world2, info.clone()).expect("valid"));
    let notes = project::open_noted(&world2, r#"{"version": 60, "entities": []}"#).expect("v60 opens");
    assert_eq!(posting::program_info(&world2), ProgramInfo::default());
    assert!(notes.is_empty(), "{notes:?}");
    // Nothing stated writes no key.
    project::save(&world2, &path).expect("save");
    assert!(!std::fs::read_to_string(&path).expect("read").contains("\"program\""));
    // A value the command would refuse is a corrupt file.
    let bad = format!(r#"{{"version": {}, "program": {{"number": 0}}, "entities": []}}"#, project::PROJECT_VERSION);
    let e = project::open(&new_world(), &bad).expect_err("corrupt").to_string();
    assert!(e.contains("outside 1 to 9999"), "{e}");
    let _ = std::fs::remove_dir_all(&dir);
}
