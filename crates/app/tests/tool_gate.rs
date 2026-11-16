//! M10.1 D (the command half): the tool law's gates at command
//! construction. A kind with no fitting tool in the active setup's
//! tools is refused by kind and setup name before any entity exists;
//! a named tool from the wrong family is refused by name; the mirror
//! edge (a groove blade into a milling op) holds; a tool retarget, an
//! op retype and a tool retype meet the same wall; kind-keeping tool
//! edits and source-only link edits never do.
//!
//! Provenance: machine-tool handbook tooling practice read against our
//! own command model (validate early, AGENTS.md); no vendor material
//! consulted.
//!
//! Gate map (M4.4): covers app::operation::tool_gate, app::commands
//! (CreateOperation, EditOperationLinks, ChangeOperationKind, ChangeTool).
//! Lane shape (M4.4): all tests here run in milliseconds.

use flecs_ecs::prelude::World;

use camcraft_app::commands::{
    AddTool, ChangeOperationKind, ChangeTool, CreateOperation, CreateSetup, EditOperationLinks,
    History, LoadMachine, SetStock,
};
use camcraft_app::machine::Machine;
use camcraft_app::model;
use camcraft_app::operation::{tool_gate, Height, OpParams, OpType, SrcRef};
use camcraft_app::schema::Location;
use camcraft_app::stock::StockShape;
use camcraft_app::tools::{catalog, Tool, ToolKind};
use camcraft_app::world::new_world;

fn catalog_tool(name: &str) -> Tool {
    catalog()
        .into_iter()
        .find(|(n, _)| *n == name)
        .map(|(_, t)| t)
        .expect("catalog entry")
}

/// Box stock on the document's machine (`None` = the unstated mill),
/// with the named catalog tools on the rack.
fn world_with(machine: Option<&str>, tools: &[&str]) -> (World, History) {
    let world = new_world();
    let mut history = History::default();
    let stock = StockShape::Box { dx: 40.0, dy: 40.0, dz: 20.0 };
    history.execute(&world, SetStock::new(&world, stock, Location::default()).expect("stock"));
    if let Some(word) = machine {
        let m = Machine {
            model: format!("TestBench {word}"),
            kind: Some(word.to_string()),
            ..Default::default()
        };
        let cmd = LoadMachine::new(&world, format!("{word}.mch"), Some(m));
        history.execute(&world, cmd.expect("machine"));
    }
    for t in tools {
        history.execute(&world, AddTool::new(&world, t, catalog_tool(t)).expect("tool"));
    }
    (world, history)
}

fn face() -> OpParams {
    OpParams::Face { holes: Default::default(),
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
    }
}

fn groove() -> OpParams {
    OpParams::GrooveFinish {
        tolerance: 0.05,
        clearance_z: Height::fixed(20.0),
        top_z: Height::fixed(-3.6),
        bottom_z: Height::fixed(-6.8),
        feed: 150.0,
        plunge_feed: 60.0,
    }
}

fn refusal(r: Result<CreateOperation, camcraft_app::commands::CommandError>) -> String {
    match r {
        Ok(_) => panic!("the tool law must refuse this creation"),
        Err(e) => e.to_string(),
    }
}

#[test]
fn an_end_mill_cannot_create_a_groove() {
    // The hand-test case: a lathe whose only tool is an end mill.
    let (world, _h) = world_with(Some("lathe"), &["EM-0400-2FL"]);
    let msg = refusal(CreateOperation::new(&world, "G1", "EM-0400-2FL", SrcRef::Stock, groove()));
    assert!(msg.contains("No grooving blade in the setup's tools"), "{msg}");
    assert!(model::find_by_name(&world, "G1").is_none(), "refused before any entity exists");
}

#[test]
fn an_end_mill_beside_a_blade_is_refused_by_name() {
    let (world, mut h) = world_with(Some("lathe"), &["EM-0400-2FL", "GT-0200"]);
    let msg = refusal(CreateOperation::new(&world, "G1", "EM-0400-2FL", SrcRef::Stock, groove()));
    assert!(msg.contains("'EM-0400-2FL' is a flat end mill"), "{msg}");
    assert!(msg.contains("grooving blade"), "{msg}");
    let ok = CreateOperation::new(&world, "G1", "GT-0200", SrcRef::Stock, groove());
    h.execute(&world, ok.expect("the blade fits the groove"));
}

#[test]
fn a_groove_blade_cannot_create_a_milling_op() {
    // The mirror edge, and the setup's own name once one is declared.
    let (world, mut h) = world_with(Some("mill-turn"), &["GT-0200"]);
    let msg = refusal(CreateOperation::new(&world, "F1", "GT-0200", SrcRef::Stock, face()));
    assert!(msg.contains("No flat end mill or bull-nose in the setup's tools"), "{msg}");
    h.execute(&world, CreateSetup::new(&world, None, None).expect("materialize Setup1"));
    let msg = refusal(CreateOperation::new(&world, "F1", "GT-0200", SrcRef::Stock, face()));
    assert!(msg.contains("No flat end mill or bull-nose in Setup1's tools"), "{msg}");
    assert!(model::find_by_name(&world, "F1").is_none());
}

#[test]
fn the_query_agrees_with_the_door() {
    let (world, _h) = world_with(None, &["DR-0400"]);
    let pocket = OpType::Pocket.info();
    assert!(tool_gate::fitting_tools(&world, pocket).is_empty());
    let why = tool_gate::create_refusal(&world, pocket).expect("no mill on the rack");
    assert!(why.starts_with("No flat end mill or bull-nose in the setup's tools"), "{why}");
    let drill = OpType::Drill.info();
    assert_eq!(tool_gate::fitting_tools(&world, drill), vec![String::from("DR-0400")]);
    assert!(tool_gate::create_refusal(&world, drill).is_none());
    assert_eq!(
        tool_gate::kinds_phrase(drill.tool_kinds),
        "drill, tap or boring bar"
    );
}

#[test]
fn a_tool_retarget_meets_the_same_wall() {
    let (world, mut h) = world_with(None, &["EM-0400-2FL", "DR-0400"]);
    let ok = CreateOperation::new(&world, "F1", "EM-0400-2FL", SrcRef::Stock, face());
    h.execute(&world, ok.expect("end mill faces"));
    let Err(err) = EditOperationLinks::new(&world, "F1", Some("DR-0400"), None) else {
        panic!("a drill cannot face");
    };
    let msg = err.to_string();
    assert!(msg.contains("'DR-0400' is a drill") && msg.contains("Face"), "{msg}");
    // Naming the tool the op already holds is no reassignment.
    EditOperationLinks::new(&world, "F1", Some("EM-0400-2FL"), Some(SrcRef::Stock))
        .expect("a same-tool link edit is not gated");
}

#[test]
fn an_op_retype_meets_the_same_wall() {
    let (world, mut h) = world_with(None, &["EM-0400-2FL"]);
    let ok = CreateOperation::new(&world, "F1", "EM-0400-2FL", SrcRef::Stock, face());
    h.execute(&world, ok.expect("end mill faces"));
    let Err(err) = ChangeOperationKind::new(&world, "F1", OpType::Finish3D as usize) else {
        panic!("a flat end mill cannot ride a ball's tangent locus");
    };
    assert!(err.to_string().contains("ball-nose"), "{err}");
    ChangeOperationKind::new(&world, "F1", OpType::Pocket as usize)
        .expect("a pocket takes the same end mill");
}

#[test]
fn change_tool_refuses_a_retype_that_strands_an_op() {
    let (world, mut h) = world_with(None, &["EM-0400-2FL"]);
    let ok = CreateOperation::new(&world, "F1", "EM-0400-2FL", SrcRef::Stock, face());
    h.execute(&world, ok.expect("end mill faces"));
    let blade = catalog_tool("GT-0200");
    assert_eq!(blade.kind, ToolKind::Groove);
    let Err(err) = ChangeTool::new(&world, "EM-0400-2FL", blade.clone()) else {
        panic!("a retype that strands F1 must be refused");
    };
    let msg = err.to_string();
    assert!(
        msg.contains("cannot become a grooving blade") && msg.contains("F1"),
        "{msg}"
    );
    // A kind-keeping edit (a re-ground diameter) is never gated.
    let mut wider = catalog_tool("EM-0400-2FL");
    wider.diameter += 1.0;
    h.execute(&world, ChangeTool::new(&world, "EM-0400-2FL", wider).expect("same kind"));
    // An unreferenced tool may become anything.
    h.execute(&world, AddTool::new(&world, "Spare", catalog_tool("EM-0600-4FL")).expect("add"));
    ChangeTool::new(&world, "Spare", blade).expect("nothing rides Spare");
}

/// M13.4: with an empty rack, every kind's refusal names a real
/// starter-catalog entry its family can cut with, and each family's
/// pick is the everyday-size one (nearest 6 mm, the blade/insert
/// families by ground width).
#[test]
fn the_refusal_names_a_fitting_catalog_entry() {
    let (world, _h) = world_with(None, &[]);
    let cat = catalog();
    for info in camcraft_app::operation::KindInfo::ALL {
        // #532: a kind that mounts no library tool (the wire EDM's) is
        // never refused for an empty rack - it needs no tool to exist.
        if !info.mounts_tool() {
            assert_eq!(tool_gate::create_refusal(&world, info), None, "{}", info.label);
            continue;
        }
        let why = tool_gate::create_refusal(&world, info).expect("an empty rack refuses every kind that mounts a tool");
        assert!(why.starts_with("No "), "{why}");
        let named = why
            .split_once(" - add '")
            .and_then(|(_, rest)| rest.split_once('\''))
            .map(|(name, _)| name)
            .unwrap_or_else(|| panic!("{}: no catalog hint in {why}", info.label));
        let (_, tool) = cat
            .iter()
            .find(|(n, _)| *n == named)
            .unwrap_or_else(|| panic!("{named} is not a catalog entry"));
        assert!(info.accepts(tool.kind), "{}: {named} cannot cut it", info.label);
        assert!(why.ends_with("from the catalog?"), "{why}");
    }
    for (ty, want) in [
        (OpType::Drill, "'DR-0600' (6 mm drill)"),
        (OpType::Thread, "'TM-0600-P125' (6 mm thread mill)"),
        (OpType::BoringTurning, "'BR-1000' (10 mm boring bar)"),
        (OpType::GrooveFinish, "'GT-0300' (3 mm grooving blade)"),
        (OpType::Threading, "'TD-1732-60' (1.732 mm threading insert)"),
        (OpType::Facing, "'TN-1600-0R8' (16 mm turning insert)"),
        (OpType::Pocket, "'EM-0600-4FL' (6 mm flat end mill)"),
        (OpType::Finish3D, "'BN-0600' (6 mm ball-nose)"),
        (OpType::FormTurning, "'FG-0600-04' (4 mm form gauge)"),
    ] {
        let why = tool_gate::create_refusal(&world, ty.info()).expect("empty rack");
        assert!(why.contains(&format!("add {want} from the catalog?")), "{why}");
    }
}

/// M13.4: a catalog that stocks no tool of the family's kinds suggests
/// nothing - the refusal never names a tool that cannot cut the op.
#[test]
fn no_catalog_entry_of_the_kind_means_no_hint() {
    let no_blades: Vec<(&str, Tool)> =
        catalog().into_iter().filter(|(_, t)| t.kind != ToolKind::Groove).collect();
    let blade = OpType::GrooveFinish.info().tool_kinds;
    assert_eq!(tool_gate::catalog_suggestion(&no_blades, blade), None);
    assert_eq!(tool_gate::catalog_suggestion(&[], OpType::Drill.info().tool_kinds), None);
    // A family's later kind still answers when its first is missing.
    let no_drills: Vec<(&str, Tool)> =
        catalog().into_iter().filter(|(_, t)| t.kind != ToolKind::Drill).collect();
    let hint = tool_gate::catalog_suggestion(&no_drills, OpType::Drill.info().tool_kinds);
    assert_eq!(hint.as_deref(), Some("add 'TAP-M6' (6 mm tap) from the catalog?"));
}
