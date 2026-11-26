//! #221 / #2442: a tool's own controller number. `AddTool` and
//! `ImportTools` assign the lowest unused number at creation, `SetToolNumber`
//! changes it (undoable, refusing `None`, out-of-range, taken, and no-op
//! numbers), `ChangeTool`, `DeleteTool`, and `DeleteEntity` keep or restore
//! it, legacy unnumbered project files freeze to their 1-based name-sorted
//! list position on load, and two tools on one machine that would post the
//! same number are refused by name.
//! Provenance: our own tests over our own commands and post stream.
//!
//! Gate map (M4.4): covers app::commands (tool, tool_number, delete_entity),
//! app::tool_numbers, app::posting (T/H numbering), app::project (tool number).

use camcraft_app::commands::{
    AddTool, ChangeTool, CreateOperation, DeleteEntity, DeleteTool, GenerateOperation, History,
    ImportModel, ImportTools, SetStock, SetToolNumber,
};
use camcraft_app::geometry::{Point3, cube};
use camcraft_app::model;
use camcraft_app::operation::{Height, OpParams, SrcRef};
use camcraft_app::post::cnc::Record;
use camcraft_app::posting;
use camcraft_app::project;
use camcraft_app::schema::Location;
use camcraft_app::stock::StockShape;
use camcraft_app::tool_library::LibraryTool;
use camcraft_app::tool_numbers;
use camcraft_app::tools::{Tool, catalog};
use camcraft_app::world::new_world;
use flecs_ecs::prelude::World;

fn tool(name: &str) -> Tool {
    catalog()
        .into_iter()
        .find(|(n, _)| *n == name)
        .map(|(_, t)| t)
        .expect("catalog tool")
}

fn number(world: &World, name: &str) -> Option<u32> {
    model::find_by_name(world, name).and_then(|e| model::tool_number_of(world, &e))
}

fn set(world: &World, h: &mut History, name: &str, n: Option<u32>) {
    h.execute(
        world,
        SetToolNumber::new(world, name, n).expect("set the number"),
    );
}

/// Stock, a part, two end mills (catalog order: EM-0400 first) and one
/// generated Face per tool, so the program uses both.
fn two_tool_job() -> (World, History) {
    tool_job(["EM-0400-2FL", "EM-0600-4FL"])
}

/// [`two_tool_job`] with the two end mills added in `order` (#2517: a
/// document order that is not the name order). Face1 still uses EM-0400,
/// Face2 EM-0600.
fn tool_job(order: [&str; 2]) -> (World, History) {
    let world = new_world();
    let mut h = History::default();
    let stock = SetStock::new(
        &world,
        StockShape::Box {
            dx: 60.0,
            dy: 60.0,
            dz: 20.0,
        },
        Location::default(),
    );
    h.execute(&world, stock.expect("stock"));
    h.execute(
        &world,
        ImportModel::from_mesh(
            "part",
            cube(Point3::new(0.0, 0.0, 0.0), Point3::new(40.0, 40.0, 19.0)),
        ),
    );
    for t in order {
        h.execute(&world, AddTool::new(&world, t, tool(t)).expect("tool"));
    }
    for (op, t, z) in [
        ("Face1", "EM-0400-2FL", 19.5),
        ("Face2", "EM-0600-4FL", 19.0),
    ] {
        let face = OpParams::Face {
            holes: Default::default(),
            blend_turns: false, blend_radius: None,
            tilt: None,
            stepover: 3.0,
            clearance_z: Height::fixed(25.0),
            cut_z: Height::fixed(z),
            top_z: None,
            stepdown: None,
            feed: 1200.0,
            plunge_feed: 300.0,
            climb: true,
            picks: Default::default(),
        };
        h.execute(
            &world,
            CreateOperation::new(&world, op, t, SrcRef::Stock, face).expect("face"),
        );
        h.execute(
            &world,
            GenerateOperation::new(&world, op).expect("generate"),
        );
    }
    (world, h)
}

/// The TOOL records of the posted stream, in program order.
fn posted_tools(world: &World) -> Result<Vec<(u32, String)>, String> {
    Ok(posting::stream_records(world)?
        .into_iter()
        .filter_map(|r| match r {
            Record::Tool { number, name } => Some((number, name)),
            _ => None,
        })
        .collect())
}

#[test]
fn a_number_is_set_refused_to_clear_and_walked_by_undo_and_redo() {
    let (world, mut h) = two_tool_job();
    assert_eq!(
        number(&world, "EM-0400-2FL"),
        Some(1),
        "first added tool starts at T1"
    );
    assert_eq!(
        number(&world, "EM-0600-4FL"),
        Some(2),
        "second added tool starts at T2"
    );
    set(&world, &mut h, "EM-0400-2FL", Some(31));
    assert_eq!(number(&world, "EM-0400-2FL"), Some(31));
    let clear_err = SetToolNumber::new(&world, "EM-0400-2FL", None)
        .err()
        .map(|e| e.to_string())
        .unwrap_or_default();
    assert!(
        clear_err.contains("EM-0400-2FL") && clear_err.contains("1-99999"),
        "{clear_err}"
    );
    assert_eq!(number(&world, "EM-0400-2FL"), Some(31));
    assert!(h.undo(&world));
    assert_eq!(
        number(&world, "EM-0400-2FL"),
        Some(1),
        "undo restores add-time T1"
    );
    assert!(h.redo(&world));
    assert_eq!(number(&world, "EM-0400-2FL"), Some(31), "redo re-applies");
}

#[test]
fn bad_numbers_are_refused_by_name() {
    let (world, mut h) = two_tool_job();
    for (n, why) in [(0, "out of range"), (tool_numbers::MAX + 1, "out of range")] {
        let err = SetToolNumber::new(&world, "EM-0400-2FL", Some(n))
            .err()
            .map(|e| e.to_string())
            .unwrap_or_default();
        assert!(err.contains(why), "{n}: {err}");
    }
    set(&world, &mut h, "EM-0400-2FL", Some(31));
    let taken = SetToolNumber::new(&world, "EM-0600-4FL", Some(31))
        .err()
        .map(|e| e.to_string())
        .unwrap_or_default();
    assert!(
        taken.contains("already") && taken.contains("EM-0400-2FL"),
        "{taken}"
    );
    assert!(
        SetToolNumber::new(&world, "EM-0400-2FL", Some(31)).is_err(),
        "a no-op is refused"
    );
    assert!(
        SetToolNumber::new(&world, "part", Some(5)).is_err(),
        "a model is not a tool"
    );
    assert!(
        SetToolNumber::new(&world, "nobody", Some(5)).is_err(),
        "an unknown name"
    );
}

#[test]
fn change_and_delete_keep_the_number() {
    let (world, mut h) = two_tool_job();
    set(&world, &mut h, "EM-0400-2FL", Some(31));
    // ChangeTool edits geometry, never the tool-table number - either way.
    let mut bigger = tool("EM-0400-2FL");
    bigger.flute_len += 1.0;
    h.execute(
        &world,
        ChangeTool::new(&world, "EM-0400-2FL", bigger).expect("change"),
    );
    assert_eq!(
        number(&world, "EM-0400-2FL"),
        Some(31),
        "ChangeTool keeps it"
    );
    assert!(h.undo(&world));
    assert_eq!(
        number(&world, "EM-0400-2FL"),
        Some(31),
        "undoing ChangeTool keeps it"
    );
    // DeleteTool and the generic delete both bring it back on undo.
    set(&world, &mut h, "EM-0600-4FL", Some(12));
    h.execute(
        &world,
        DeleteTool::new(&world, "EM-0600-4FL").expect("delete tool"),
    );
    assert!(model::find_by_name(&world, "EM-0600-4FL").is_none());
    assert!(h.undo(&world));
    assert_eq!(
        number(&world, "EM-0600-4FL"),
        Some(12),
        "DeleteTool undo restores it"
    );
    h.execute(
        &world,
        DeleteEntity::new(&world, "EM-0400-2FL").expect("delete entity"),
    );
    assert!(h.undo(&world));
    assert_eq!(
        number(&world, "EM-0400-2FL"),
        Some(31),
        "DeleteEntity undo restores it"
    );
}

#[test]
fn the_post_writes_a_tools_own_number_and_positions_for_the_rest() {
    let (world, mut h) = two_tool_job();
    assert_eq!(
        posted_tools(&world).expect("posts"),
        vec![(1, "EM-0400-2FL".to_owned()), (2, "EM-0600-4FL".to_owned())],
        "tools post as the numbers assigned at add"
    );
    set(&world, &mut h, "EM-0400-2FL", Some(31));
    assert_eq!(
        posted_tools(&world).expect("posts"),
        vec![
            (31, "EM-0400-2FL".to_owned()),
            (2, "EM-0600-4FL".to_owned())
        ],
        "a numbered tool posts its own number"
    );
    // Through a real dialect: the T word and the H word both carry it.
    let path =
        std::env::temp_dir().join(format!("camcraft_tool_numbers_{}.nc", std::process::id()));
    let posted = posting::post_setup(&world, &path);
    let _ = std::fs::remove_file(&path);
    let program = posted.expect("the default post writes it").program;
    assert!(
        program.contains("T31") && program.contains("H31"),
        "{program}"
    );
}

#[test]
fn two_tools_posting_one_number_refuse_by_name() {
    let (world, mut h) = two_tool_job();
    // Simulate a legacy or hand-edited file where two tools share number 2;
    // the post clash check still catches it and names both tools.
    let path = std::env::temp_dir().join(format!(
        "camcraft_tool_numbers_clash_{}.camproj",
        std::process::id()
    ));
    project::save(&world, &path).expect("save");
    let dup = std::fs::read_to_string(&path)
        .expect("read")
        .replacen("\"number\": 1", "\"number\": 2", 1);
    std::fs::write(&path, &dup).expect("write");
    let clash_world = new_world();
    project::load(&clash_world, &path).expect("load");
    let _ = std::fs::remove_file(&path);
    let err = posted_tools(&clash_world).err().unwrap_or_default();
    assert!(
        err.contains("EM-0400-2FL") && err.contains("EM-0600-4FL") && err.contains("T2"),
        "{err}"
    );
    // Giving the other its own number clears it.
    let mut clash_h = History::default();
    set(&clash_world, &mut clash_h, "EM-0600-4FL", Some(6));
    assert_eq!(
        posted_tools(&clash_world).expect("posts"),
        vec![(2, "EM-0400-2FL".to_owned()), (6, "EM-0600-4FL".to_owned())]
    );
    let _ = &mut h;
}

#[test]
fn the_project_file_carries_the_number_from_add() {
    let (world, mut h) = two_tool_job();
    let path = std::env::temp_dir().join(format!(
        "camcraft_tool_numbers_{}.camproj",
        std::process::id()
    ));
    project::save(&world, &path).expect("save");
    let text = std::fs::read_to_string(&path).expect("read");
    assert!(
        text.contains("\"number\": 1") && text.contains("\"number\": 2"),
        "every added tool carries its own number into the project file:\n{text}"
    );
    set(&world, &mut h, "EM-0400-2FL", Some(31));
    project::save(&world, &path).expect("save");
    let world2 = new_world();
    project::load(&world2, &path).expect("load");
    assert_eq!(
        number(&world2, "EM-0400-2FL"),
        Some(31),
        "the number survives a save and load"
    );
    assert_eq!(number(&world2, "EM-0600-4FL"), Some(2));
    // A stated 0 is a corrupt file, refused by name - never "no tool".
    let corrupt = std::fs::read_to_string(&path)
        .expect("read")
        .replace("\"number\": 31", "\"number\": 0");
    std::fs::write(&path, &corrupt).expect("write");
    let err = project::load(&new_world(), &path)
        .err()
        .map(|e| e.to_string())
        .unwrap_or_default();
    let _ = std::fs::remove_file(&path);
    assert!(
        err.contains("EM-0400-2FL") && err.contains("1-99999"),
        "{err}"
    );
}

#[test]
fn an_unnumbered_tool_with_no_free_number_is_refused_by_name() {
    // #2517: every number taken, one tool without its own - it has no
    // number to post as, so the law refuses it by name (it once took u32::MAX).
    let mut tools: Vec<(String, Option<u32>)> =
        (1..=tool_numbers::MAX).map(|n| (format!("T{n}"), Some(n))).collect();
    tools.push(("LAST".to_owned(), None));
    let err = tool_numbers::assign(tools).expect_err("no number is free");
    assert!(err.contains("tool 'LAST'") && err.contains(tool_numbers::RANGE_TEXT), "{err}");
    // With a number free, the unnumbered tool takes the lowest one, in order.
    let tools = vec![("A".to_owned(), Some(1)), ("B".to_owned(), None), ("C".to_owned(), Some(2))];
    let got = tool_numbers::assign(tools).expect("a number is free");
    assert_eq!(got, vec![("A".to_owned(), 1), ("B".to_owned(), 3), ("C".to_owned(), 2)]);
}

// ---- Numbers that never shift (#2442) and legacy loads ---------------

#[path = "tool_numbers/stable.rs"]
mod stable;
