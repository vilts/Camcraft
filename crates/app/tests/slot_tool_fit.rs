//! #3475: a tool wider than a plain Slot is refused at the command, by
//! the slot generator's own rule, with both sizes and the largest tool
//! that fits - at create, at a tool reassignment (MCP `set_tool`, the
//! UI's links edit), at a change of kind and at a tool library edit that
//! widens the cutter. A Slot made from the seed is as wide as its tool.
//! A refused command changes nothing; a tool that fits is accepted.
//!
//! Provenance: plain offset geometry of a slot (machine-tool handbook
//! practice) read against our own command model; no vendor material
//! consulted.
//!
//! Gate map (M4.4): covers app::operation::tool_gate, app::commands
//! (CreateOperation, EditOperationLinks, ChangeOperationKind, ChangeTool).
//! Lane shape (M4.4): all tests here run in milliseconds.

use flecs_ecs::prelude::World;

use camcraft_app::commands::{
    AddTool, ChangeOperationKind, ChangeTool, CommandError, CreateOperation, EditOperationLinks, History, SetStock,
};
use camcraft_app::model;
use camcraft_app::operation::{self, tool_gate, Height, HeightReference, OpParams, OpType, SrcRef};
use camcraft_app::schema::Location;
use camcraft_app::stock::StockShape;
use camcraft_app::tools::{catalog, Tool};
use camcraft_app::units::Unit;
use camcraft_app::world::new_world;

const SMALL: &str = "EM-0400-2FL";
const WIDE: &str = "EM-WIDE";

fn end_mill(diameter: f64) -> Tool {
    let mut t = catalog().into_iter().find(|(n, _)| *n == SMALL).map(|(_, t)| t).expect("catalog end mill");
    t.diameter = diameter;
    t
}

/// Box stock, the 4 mm catalog end mill and a 10 mm end mill.
fn world() -> (World, History) {
    let world = new_world();
    let mut history = History::default();
    let stock = StockShape::Box { dx: 80.0, dy: 40.0, dz: 20.0 };
    history.execute(&world, SetStock::new(&world, stock, Location::default()).expect("stock"));
    history.execute(&world, AddTool::new(&world, SMALL, end_mill(4.0)).expect("small tool"));
    history.execute(&world, AddTool::new(&world, WIDE, end_mill(10.0)).expect("wide tool"));
    (world, history)
}

/// A typed 6 mm slot with stock heights.
fn slot(leave: f64) -> OpParams {
    let mut params = operation::seed_params(OpType::Slot.ordinal());
    if let OpParams::Slot { width, stock_to_leave, clearance_z, top_z, bottom_z, stepdown, .. } = &mut params {
        *width = 6.0;
        *stock_to_leave = leave;
        *clearance_z = Height::of(HeightReference::TopOfStock, 10.0);
        *top_z = Height::of(HeightReference::TopOfStock, 0.0);
        *bottom_z = Height::of(HeightReference::TopOfStock, -2.0);
        *stepdown = 2.0;
    } else {
        panic!("the Slot seed is a Slot");
    }
    params
}

fn refused<T>(r: Result<T, CommandError>, what: &str) -> String {
    match r {
        Ok(_) => panic!("{what} must be refused"),
        Err(e) => e.to_string(),
    }
}

fn tool_of_op(world: &World, name: &str) -> String {
    model::find_by_name(world, name).and_then(|e| model::operation_of(world, &e)).expect("the operation").tool
}

const SLOT_TEXT: &str =
    "invalid parameters: the tool 'EM-WIDE' is 10 mm wide and the slot is 6 mm wide: choose a tool of 6 mm or less";

#[test]
fn a_wide_tool_on_a_slot_is_refused_at_set_tool_and_changes_nothing() {
    let (world, mut h) = world();
    h.execute(&world, CreateOperation::new(&world, "S1", SMALL, SrcRef::Stock, slot(0.0)).expect("the 4 mm tool fits"));
    let msg = refused(EditOperationLinks::new(&world, "S1", Some(WIDE), None), "a 10 mm tool on a 6 mm slot");
    assert_eq!(msg, SLOT_TEXT);
    assert_eq!(tool_of_op(&world, "S1"), SMALL, "the refused edit changes nothing");
    // The last command on the stack is still the create: one undo removes the op.
    assert!(h.undo(&world));
    assert!(model::find_by_name(&world, "S1").is_none());
}

#[test]
fn a_tool_that_fits_is_accepted() {
    let (world, mut h) = world();
    h.execute(&world, AddTool::new(&world, "EM-SIX", end_mill(6.0)).expect("6 mm tool"));
    h.execute(&world, CreateOperation::new(&world, "S1", SMALL, SrcRef::Stock, slot(0.0)).expect("create"));
    let edit = EditOperationLinks::new(&world, "S1", Some("EM-SIX"), None).expect("a tool of the slot width fits");
    h.execute(&world, edit);
    assert_eq!(tool_of_op(&world, "S1"), "EM-SIX");
}

#[test]
fn a_wide_tool_on_a_slot_is_refused_at_create() {
    let (world, _h) = world();
    let msg = refused(CreateOperation::new(&world, "S1", WIDE, SrcRef::Stock, slot(0.0)), "create with a 10 mm tool");
    assert_eq!(msg, SLOT_TEXT);
    assert!(model::find_by_name(&world, "S1").is_none());
}

#[test]
fn stock_to_leave_is_part_of_the_fit() {
    let (world, mut h) = world();
    h.execute(&world, AddTool::new(&world, "EM-SIX", end_mill(6.0)).expect("6 mm tool"));
    let msg = refused(CreateOperation::new(&world, "S1", "EM-SIX", SrcRef::Stock, slot(0.5)), "6 mm tool, 0.5 mm on each wall");
    assert_eq!(
        msg,
        "invalid parameters: the tool 'EM-SIX' is 6 mm wide and the slot is 6 mm wide with 0.5 mm left on each wall: choose a tool of 5 mm or less"
    );
}

#[test]
fn the_sentence_follows_the_document_unit() {
    let why = tool_gate::slot_fit_refusal("T", 25.4, 12.7, 0.0, Unit::Inches).expect("too wide");
    assert_eq!(why, "the tool 'T' is 1 in wide and the slot is 0.5 in wide: choose a tool of 0.5 in or less");
    assert!(tool_gate::slot_fit_refusal("T", 6.0, 6.0, 0.0, Unit::Millimeters).is_none());
    assert!(tool_gate::slot_fit_refusal("T", f64::NAN, 6.0, 0.0, Unit::Millimeters).is_none(), "no slot: others name it");
}

#[test]
fn a_tool_library_edit_that_widens_a_slot_cutter_is_refused() {
    let (world, mut h) = world();
    h.execute(&world, CreateOperation::new(&world, "S1", SMALL, SrcRef::Stock, slot(0.0)).expect("create"));
    let msg = refused(ChangeTool::new(&world, SMALL, end_mill(100.0)), "a 100 mm cutter on the 6 mm slot");
    assert_eq!(msg, "invalid parameters: S1 uses it: the tool 'EM-0400-2FL' is 100 mm wide and the slot is 6 mm wide: choose a tool of 6 mm or less");
    // A smaller cutter, or one that still fits, passes.
    h.execute(&world, ChangeTool::new(&world, SMALL, end_mill(6.0)).expect("6 mm still fits"));
    h.execute(&world, ChangeTool::new(&world, SMALL, end_mill(3.0)).expect("a smaller cutter"));
}

fn face() -> OpParams {
    OpParams::Face {
        holes: Default::default(),
        blend_turns: false,
        blend_radius: None,
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

fn slot_width(world: &World, name: &str) -> f64 {
    match model::find_by_name(world, name).and_then(|e| model::operation_of(world, &e)).map(|op| op.params) {
        Some(OpParams::Slot { width, .. }) => width,
        other => panic!("{name} is no Slot: {other:?}"),
    }
}

#[test]
fn a_change_of_kind_to_slot_holds_the_ops_own_tool() {
    let (world, mut h) = world();
    h.execute(&world, CreateOperation::new(&world, "W", WIDE, SrcRef::Stock, face()).expect("a 10 mm face"));
    h.execute(&world, CreateOperation::new(&world, "N", SMALL, SrcRef::Stock, face()).expect("a 4 mm face"));
    for name in ["W", "N"] {
        let retype = ChangeOperationKind::new(&world, name, OpType::Slot.ordinal());
        h.execute(&world, retype.unwrap_or_else(|e| panic!("{name} becomes a Slot: {e}")));
    }
    assert!((slot_width(&world, "W") - 10.0).abs() < 1e-9, "the 10 mm tool widens the seed");
    assert!((slot_width(&world, "N") - 4.0).abs() < 1e-9, "a 4 mm tool keeps the seed's 4 mm");
}
