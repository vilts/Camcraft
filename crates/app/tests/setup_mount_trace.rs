//! #1224: a setup's own machine and post traced from the pick to the
//! posted bytes. Setup1 follows the document (machine A, the shipped VMC
//! 3-axis; post A, the built-in Fanuc); Setup2 owns machine B (the shipped
//! Tormach 770M, a 10000 rpm spindle) and post B (Siemens 840D), each
//! picked by its own command. Every stage reads each operation's OWN
//! setup: generation and the machine checks with the other setup active,
//! the per-mount files (each its own dialect, machine, operations and
//! work offset), a document mount change, untick, save/reopen and
//! undo/redo, and the edges of the grouping. The UI picks are pinned in
//! the ui crate (`setup_mount_pages`), the MCP picks in
//! `mcp_setup_mount_trace.rs`, a post that does not serve its own machine
//! in `setup_mounts_post.rs`.
//! Provenance: our own fixtures over our own commands and the shipped
//! machine and post files; no vendor material.
//!
//! Gate map (#1224): covers app::setup_mount (program_mounts, machine_of,
//! post_of), app::posting (post_mounts, verification_gate, index_swing),
//! app::commands (SetSetupMachine, SetSetupPost, LoadMachine, SelectPost),
//! app::project (save, open).
//! Lane shape (M4.4): all tests here run in seconds.

use flecs_ecs::prelude::World;

use camcraft_app::commands::{
    ActivateSetup, AddTool, CreateOperation, CreateSetup, GenerateOperation, History, LoadMachine, SelectPost,
    SetSetupMachine, SetSetupPost, SetSetupWorkOffset, SetStock, SetWcsOrigin,
};
use camcraft_app::geometry::Point3;
use camcraft_app::operation::{Height, OpParams, OpStatus, SrcRef};
use camcraft_app::schema::{Location, PostRef};
use camcraft_app::stock::StockShape;
use camcraft_app::tools::catalog;
use camcraft_app::world::new_world;
use camcraft_app::{model, posting, project, setup_mount};

const TOOL: &str = "EM-0400-2FL";
/// The same end mill at 8000 rpm: inside machine B's spindle.
const SLOW: &str = "EM-0400-2FL 8k";
const MACHINE_A: &str = "VMC 3-axis";
const MACHINE_B: &str = "Tormach 770M";
const POST_B: &str = "Siemens 840D";
const OPS: [&str; 3] = ["Face1a", "Face1b", "Face2"];

fn face(cut_z: f64) -> OpParams {
    OpParams::Face {
        holes: Default::default(),
        blend_turns: false,
        tilt: None,
        stepover: 3.0,
        clearance_z: Height::fixed(25.0),
        cut_z: Height::fixed(cut_z),
        top_z: None,
        stepdown: None,
        feed: 1200.0,
        plunge_feed: 300.0,
        climb: true,
    }
}

/// Two setups on one 60 x 60 x 20 box each, WCS on the top face. Setup1:
/// Face1a then Face1b cut by `tool1`, G54. Setup2: Face2 cut by `tool2`, G55. The
/// document mounts machine A with post A; no setup owns a mount yet.
/// Setup2 is active; nothing is generated.
fn doc_with(tool1: &str, tool2: &str) -> (World, History) {
    let world = new_world();
    let mut h = History::default();
    h.execute(&world, LoadMachine::builtin(&world, MACHINE_A).expect("machine A"));
    let tool = catalog().into_iter().find(|(n, _)| *n == TOOL).map(|(_, t)| t).expect("tool");
    h.execute(&world, AddTool::new(&world, TOOL, tool.clone()).expect("add tool"));
    let slow = camcraft_app::tools::Tool { rpm: 8000.0, ..tool };
    h.execute(&world, AddTool::new(&world, SLOW, slow).expect("add slow tool"));
    let ops: [&[(&str, f64)]; 2] = [&[("Face1a", 19.5), ("Face1b", 19.0)], &[("Face2", 19.5)]];
    for (i, ops) in ops.into_iter().enumerate() {
        h.execute(&world, CreateSetup::new(&world, None, None).expect("setup"));
        let stock = SetStock::new(&world, StockShape::Box { dx: 60.0, dy: 60.0, dz: 20.0 }, Location::default());
        h.execute(&world, stock.expect("box"));
        let top = Location { pos: Point3::new(0.0, 0.0, 20.0), ..Location::default() };
        h.execute(&world, SetWcsOrigin::new(&world, top).expect("wcs on top"));
        if i == 1 {
            h.execute(&world, SetSetupWorkOffset::new(&world, "Setup2", Some(1)).expect("G55"));
        }
        for (name, z) in ops {
            let t = if i == 1 { tool2 } else { tool1 };
            h.execute(&world, CreateOperation::new(&world, name, t, SrcRef::Stock, face(*z)).expect("face"));
        }
    }
    (world, h)
}

fn doc() -> (World, History) {
    doc_with(TOOL, SLOW)
}

/// `setup` takes its own machine B and its own post B (two commands).
fn own_b(world: &World, h: &mut History, setup: &str) {
    h.execute(world, SetSetupMachine::builtin(world, setup, MACHINE_B).expect("own machine B"));
    h.execute(world, SetSetupPost::new(world, setup, Some(PostRef::Named(POST_B.into()))).expect("own post B"));
}

fn generate(world: &World, h: &mut History, ops: &[&str]) {
    for &op in ops {
        h.execute(world, GenerateOperation::new(world, op).expect("generates"));
        let e = model::find_by_name(world, op).expect("op");
        assert_eq!(camcraft_app::operation::status_of(world, &e), OpStatus::Ok, "{op}");
    }
}

/// Generate Setup1's ops with Setup2 active and Setup2's with Setup1
/// active: each against its own machine, never the active one's.
fn generate_crossed(world: &World, h: &mut History) {
    h.execute(world, ActivateSetup::new(world, Some("Setup2")).expect("activate"));
    generate(world, h, &["Face1a", "Face1b"]);
    h.execute(world, ActivateSetup::new(world, Some("Setup1")).expect("activate"));
    generate(world, h, &["Face2"]);
}

fn tmp(tag: &str, ext: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!("camcraft-1224-{tag}-{}.{ext}", std::process::id()))
}

/// One written file: its name, its text, the setups of its mount.
type File = (String, String, Vec<String>);

/// Post one file per mount; each file read back and removed again.
fn post(world: &World, tag: &str) -> Result<Vec<File>, String> {
    let files = posting::post_mounts(world, &tmp(tag, "nc"))?;
    let mut out = Vec::new();
    for f in files {
        let text = std::fs::read_to_string(&f.path).expect("written");
        assert_eq!(text, f.posted.program, "the file is the posted program");
        let _ = std::fs::remove_file(&f.path);
        let name = f.path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        out.push((name, text, f.mount.setups));
    }
    Ok(out)
}

fn index_of(text: &str, needle: &str) -> usize {
    text.find(needle).unwrap_or_else(|| panic!("{needle:?} not in:\n{text}"))
}

/// Post A's dialect for `machine`: Fanuc's % / O-word header naming it, M30.
fn is_fanuc_on(text: &str, machine: &str) -> bool {
    text.starts_with(&format!("%\nO0001 ({machine})")) && text.contains("M30") && !text.contains("G710")
}

/// Post B's dialect for `machine`: Siemens' ; header naming it, N words, G710, M2.
fn is_siemens_on(text: &str, machine: &str) -> bool {
    text.starts_with(&format!("; {machine}\n")) && text.contains("N10 G17 G710") && !text.contains("M30")
}

/// Rows 1 (stored), 2 (generated with the other setup active) and 4
/// (posted): one file per mount, each written by its own post for its own
/// machine, holding only its own setup's ops in program order at its own
/// work offset, the file names telling the mounts apart - the same bytes
/// whichever setup is active.
#[test]
fn each_mount_file_is_its_own_post_machine_ops_and_work_offset() {
    let (world, mut h) = doc();
    own_b(&world, &mut h, "Setup2");
    assert_eq!(setup_mount::document_machine(&world).0, posting::builtin_path(MACHINE_A));
    assert_eq!(setup_mount::document_post(&world), PostRef::Builtin);
    assert_eq!(setup_mount::own_machine(&world, Some("Setup1")), None, "Setup1 follows the document");
    assert_eq!(setup_mount::own_post(&world, Some("Setup1")), None);
    let own = setup_mount::own_machine(&world, Some("Setup2")).expect("Setup2 owns its machine");
    assert_eq!(own.path, posting::builtin_path(MACHINE_B));
    assert_eq!(own.machine.map(|m| m.model).as_deref(), Some(MACHINE_B));
    assert_eq!(setup_mount::own_post(&world, Some("Setup2")), Some(PostRef::Named(POST_B.into())));

    generate_crossed(&world, &mut h);
    let files = post(&world, "row4").expect("two mounts post");
    h.execute(&world, ActivateSetup::new(&world, Some("Setup2")).expect("activate"));
    assert_eq!(post(&world, "row4").expect("posts"), files, "activation moves no byte");
    assert_eq!(files.len(), 2, "one file per mount");
    let ((name1, a, setups1), (name2, b, setups2)) = (&files[0], &files[1]);
    assert!(name1.ends_with("_01_Setup1.nc") && name2.ends_with("_02_Setup2.nc"), "{name1} {name2}");
    assert_eq!((setups1.join(","), setups2.join(",")), ("Setup1".to_owned(), "Setup2".to_owned()));

    assert!(is_fanuc_on(a, "VMC 3-AXIS"), "post A for machine A:\n{a}");
    assert!(a.contains("(SETUP: Setup1)") && !a.contains("N10 "), "{a}");
    assert!(index_of(a, "(OPERATION: Face1a - FACE - G54)") < index_of(a, "(OPERATION: Face1b - FACE - G54)"));
    assert!(a.contains("G90 G54") && !a.contains("G55"), "Setup1's own work offset only:\n{a}");
    assert!(!a.contains("Face2") && !a.contains("Setup2") && !a.contains("770M"), "nothing of Setup2:\n{a}");

    assert!(is_siemens_on(b, MACHINE_B), "post B for machine B:\n{b}");
    assert!(b.contains("; SETUP: Setup2") && b.contains("; OPERATION: Face2 - FACE - G55"), "{b}");
    assert!(b.contains("G90 G55") && !b.contains("G54"), "Setup2's own work offset only:\n{b}");
    assert!(!b.contains("Face1") && !b.contains("Setup1") && !b.contains("VMC"), "nothing of Setup1:\n{b}");
}

/// Row 3: the spindle limit is judged on each op's own machine. Face2's
/// 12000 rpm outruns machine B; Face1a/b's same rpm is inside machine A.
/// The verdict names Face2 alone from either active setup, writes no
/// file, and leaves when Setup2 follows the document's machine again.
#[test]
fn a_spindle_limit_is_judged_on_each_ops_own_machine_from_either_active_setup() {
    let (world, mut h) = doc_with(TOOL, TOOL);
    own_b(&world, &mut h, "Setup2");
    generate_crossed(&world, &mut h);
    for active in ["Setup1", "Setup2"] {
        h.execute(&world, ActivateSetup::new(&world, Some(active)).expect("activate"));
        let gate = posting::verification_gate(&world);
        let limits: Vec<&String> = gate.iter().filter(|p| p.contains("limit problem")).collect();
        assert_eq!(limits.len(), 1, "{active} active: {gate:?}");
        assert!(limits[0].starts_with("machine Tormach 770M:") && limits[0].contains("Face2: tool EM-0400-2FL runs 12000 rpm"), "{gate:?}");
        assert!(!limits[0].contains("Face1"), "Face1a/b run on machine A: {gate:?}");
        let why = post(&world, "row3-rpm").expect_err("the gate holds every file");
        assert!(why.contains("Face2:") && !why.contains("Face1"), "{why}");
        let p = tmp("row3-rpm", "nc");
        let stem = p.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
        for k in ["01_Setup1", "02_Setup2"] {
            assert!(!p.with_file_name(format!("{stem}_{k}.nc")).exists(), "a refusal writes nothing");
        }
    }
    h.execute(&world, SetSetupMachine::new(&world, "Setup2", None).expect("follow the document's machine"));
    let gate = posting::verification_gate(&world);
    assert!(!gate.iter().any(|p| p.contains("limit problem")), "machine A runs 12000 rpm: {gate:?}");
}

/// Row 3: the rotary index-swing clause reads each op's own machine. A
/// work-turning table A that states no nose height cannot be shown to
/// clear: refused for the ops of the setup that mounts it, never the
/// other's, whichever is active; moved to Setup1 the verdict moves too.
#[test]
fn a_rotary_index_swing_is_judged_on_each_ops_own_machine() {
    let text = posting::MACHINES.iter().find(|(n, _)| *n == "Tormach 770MX + microARC 4").map(|(_, t)| *t);
    let mut arc4 = camcraft_app::machine::parse_mch(text.expect("shipped")).expect("parses");
    for a in &mut arc4.axes {
        a.nose_to_axis = None;
    }
    let (world, mut h) = doc();
    let mount = || Some(("arc4-no-nose.mch".to_owned(), Some(arc4.clone())));
    h.execute(&world, SetSetupMachine::new(&world, "Setup2", mount()).expect("A on Setup2"));
    generate_crossed(&world, &mut h);
    let swing = |w: &World| -> Vec<String> {
        posting::verification_gate(w).into_iter().filter(|p| p.contains("nose_to_axis")).collect()
    };
    for active in ["Setup1", "Setup2"] {
        h.execute(&world, ActivateSetup::new(&world, Some(active)).expect("activate"));
        let lines = swing(&world);
        assert_eq!(lines.len(), 1, "{active} active: {lines:?}");
        assert!(lines[0].starts_with("Face2: "), "{lines:?}");
    }
    h.execute(&world, SetSetupMachine::new(&world, "Setup2", None).expect("Setup2 follows"));
    h.execute(&world, SetSetupMachine::new(&world, "Setup1", mount()).expect("A on Setup1"));
    generate(&world, &mut h, &OPS);
    let lines = swing(&world);
    assert_eq!(lines.len(), 2, "{lines:?}");
    assert!(lines[0].starts_with("Face1a: ") && lines[1].starts_with("Face1b: "), "{lines:?}");
}

/// Row 5: a new document machine and post move Setup1's file and never a
/// byte of Setup2's; unticking Setup2's own post, then its own machine,
/// lets it follow the document again - one undoable step each.
#[test]
fn the_document_mount_moves_only_the_following_setup_and_untick_follows_again() {
    let (world, mut h) = doc();
    own_b(&world, &mut h, "Setup2");
    generate(&world, &mut h, &OPS);
    let before = post(&world, "row5").expect("posts");
    h.execute(&world, LoadMachine::builtin(&world, "Tilt-head 5-axis").expect("document machine"));
    h.execute(&world, SelectPost::new(&world, PostRef::Named("Haas".into())).expect("document post"));
    let moved = post(&world, "row5").expect("posts");
    assert_eq!(moved.len(), 2);
    assert_eq!(moved[1], before[1], "Setup2's file keeps every byte");
    let a = &moved[0].1;
    assert!(a.starts_with("%\n(TILT-HEAD 5-AXIS)") && !a.contains("O0001"), "Haas for the tilt head:\n{a}");
    assert!(a.contains("(OPERATION: Face1a") && !a.contains("Face2"), "{a}");

    h.execute(&world, SetSetupPost::new(&world, "Setup2", None).expect("untick the post"));
    let post_followed = post(&world, "row5").expect("posts");
    assert_eq!(post_followed.len(), 2, "two machines, one post: still two files");
    assert_eq!(post_followed[0], moved[0], "Setup1's file is untouched");
    let b = &post_followed[1].1;
    assert!(b.starts_with("%\n(Tormach 770M)") && b.contains("G55") && !b.contains("G710"), "Haas on machine B:\n{b}");

    h.execute(&world, SetSetupMachine::new(&world, "Setup2", None).expect("untick the machine"));
    let followed = post(&world, "row5").expect("posts");
    assert_eq!(followed.len(), 1, "one mount, one file");
    let one = &followed[0].1;
    assert!(one.starts_with("%\n(TILT-HEAD 5-AXIS)") && !one.contains("770M"), "{one}");
    assert!(index_of(one, "Face1b - FACE - G54") < index_of(one, "Face2 - FACE - G55"), "{one}");

    // Each untick is one step: undo walks back through both outputs.
    h.undo(&world);
    assert_eq!(setup_mount::own_machine(&world, Some("Setup2")).map(|m| m.path), Some(posting::builtin_path(MACHINE_B)));
    assert_eq!(post(&world, "row5").expect("posts"), post_followed, "undo the machine untick");
    h.undo(&world);
    assert_eq!(setup_mount::own_post(&world, Some("Setup2")), Some(PostRef::Named(POST_B.into())));
    assert_eq!(post(&world, "row5").expect("posts"), moved, "undo the post untick");
}

/// Row 6: two mounts save, reopen and post byte-identical; undo and redo
/// of the tick of each pick restore each earlier output.
#[test]
fn two_mounts_round_trip_byte_identical_and_undo_redo_restores_each_output() {
    let (world, mut h) = doc();
    generate(&world, &mut h, &OPS);
    let s0 = post(&world, "row6").expect("one mount");
    h.execute(&world, SetSetupMachine::builtin(&world, "Setup2", MACHINE_B).expect("own machine"));
    let s1 = post(&world, "row6").expect("two machines");
    h.execute(&world, SetSetupPost::new(&world, "Setup2", Some(PostRef::Named(POST_B.into()))).expect("own post"));
    let s2 = post(&world, "row6").expect("two mounts");
    assert_eq!((s0.len(), s1.len(), s2.len()), (1, 2, 2));
    assert!(s1[1].1 != s2[1].1 && s1[0] == s2[0], "the post pick moves Setup2's file alone");

    let path = tmp("row6", "ccam");
    project::save(&world, &path).expect("save");
    let text = std::fs::read_to_string(&path).expect("saved");
    let _ = std::fs::remove_file(&path);
    let reopened = new_world();
    project::open(&reopened, &text).expect("reopens");
    assert_eq!(post(&reopened, "row6").expect("reopened posts"), s2, "byte-identical after the round trip");

    h.undo(&world);
    assert_eq!(post(&world, "row6").expect("posts"), s1, "undo the post pick");
    h.undo(&world);
    assert_eq!(post(&world, "row6").expect("posts"), s0, "undo the machine pick");
    h.redo(&world);
    assert_eq!(post(&world, "row6").expect("posts"), s1, "redo the machine pick");
    h.redo(&world);
    assert_eq!(post(&world, "row6").expect("posts"), s2, "redo the post pick");
}

/// Row 7: both setups owning the same machine and post are one mount -
/// one file through post B for machine B, both work offsets in it - as
/// `program_mounts` groups (same machine, same post); an own machine
/// with a followed post, and the reverse, are two files each.
#[test]
fn the_same_own_mount_is_one_file_and_half_a_mount_is_two() {
    let (world, mut h) = doc_with(SLOW, SLOW);
    own_b(&world, &mut h, "Setup1");
    own_b(&world, &mut h, "Setup2");
    generate(&world, &mut h, &OPS);
    let one = post(&world, "row7-same").expect("posts");
    assert_eq!(one.len(), 1, "one machine, one post: one file");
    assert!(one[0].0.ends_with(&format!("row7-same-{}.nc", std::process::id())), "at the path asked: {}", one[0].0);
    assert_eq!(one[0].2, vec!["Setup1".to_owned(), "Setup2".to_owned()]);
    let t = &one[0].1;
    assert!(is_siemens_on(t, MACHINE_B) && t.contains("G90 G54") && t.contains("G90 G55"), "{t}");

    // Own machine, followed post: two machines, both Fanuc.
    let (world, mut h) = doc();
    h.execute(&world, SetSetupMachine::builtin(&world, "Setup2", MACHINE_B).expect("own machine"));
    generate(&world, &mut h, &OPS);
    let f = post(&world, "row7-machine").expect("posts");
    assert_eq!(f.len(), 2);
    assert!(is_fanuc_on(&f[0].1, "VMC 3-AXIS") && is_fanuc_on(&f[1].1, MACHINE_B), "{f:?}");

    // Own post, followed machine: one machine, two dialects.
    let (world, mut h) = doc();
    h.execute(&world, SetSetupPost::new(&world, "Setup2", Some(PostRef::Named(POST_B.into()))).expect("own post"));
    generate(&world, &mut h, &OPS);
    let f = post(&world, "row7-post").expect("posts");
    assert_eq!(f.len(), 2);
    assert!(is_fanuc_on(&f[0].1, "VMC 3-AXIS") && is_siemens_on(&f[1].1, "VMC 3-AXIS"), "{f:?}");
}
