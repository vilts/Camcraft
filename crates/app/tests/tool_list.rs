//! M3.2 stage C acceptance: the per-setup tool list is a pure read of
//! the program order, first-use order and change law documented in
//! `camcraft_app::tool_list` - rows follow the order tools enter the
//! spindle, a switch back re-charges a change, time is the moves' own
//! feeds (honest recomputation, the `peck_chain_agrees_with_cycle_record`
//! idiom), ungenerated ops are listed at zero, setups split the list,
//! the program roll-up counts the whole sequence, and an empty
//! document says so without panicking.
//!
//! Gate map (M4.4): covers cl::cl, app::commands, app::model, app::operation, app::schema, geometry::geometry::stock, app::tool_list, tools::tools, app::world.

use flecs_ecs::prelude::World;

use camcraft_app::cl::{Move, Toolpath};
use camcraft_app::commands::{
    ActivateSetup, AddTool, CreateOperation, CreateSetup, GenerateOperation, History, SetStock,
};
use camcraft_app::model;
use camcraft_app::operation::{self, Height, OpParams, OpStatus, SrcRef};
use camcraft_app::stock::StockShape;
use camcraft_app::tool_list::{self, RAPID_MM_S, TOOL_CHANGE_S};
use camcraft_app::tools::{catalog, Tool};
use camcraft_app::schema::Location;
use camcraft_app::world::new_world;

fn catalog_tool(name: &str) -> Tool {
    catalog()
        .into_iter()
        .find(|(n, _)| *n == name)
        .map(|(_, t)| t)
        .expect("catalog entry")
}

/// Face parameters for the 60x60x20 box stock with a 4 mm cutter - the
/// same fixture the setups tests face on (feed 1200, plunge 300), with
/// the feeds chosen per test so a row's minutes can be re-added by
/// hand. The plunge keeps a fixed quarter of the table feed, so a
/// doubled parameter set doubles every feed word on the path.
fn face_params(cut_z: f64, feed: f64) -> OpParams {
    OpParams::Face { holes: Default::default(), tilt: None, blend_turns: false,
        stepover: 3.0,
        clearance_z: Height::fixed(25.0),
        cut_z: Height::fixed(cut_z),
        top_z: None,
        stepdown: None,
        feed,
        plunge_feed: feed / 4.0,
        climb: true,
    }
}

fn box_stock(world: &World, history: &mut History, dx: f64, dy: f64, dz: f64) {
    let cmd = SetStock::new(world, StockShape::Box { dx, dy, dz }, Location::default());
    history.execute(world, cmd.expect("box stock"));
}

fn add_tool(world: &World, history: &mut History, name: &str) {
    let cmd = AddTool::new(world, name, catalog_tool(name));
    history.execute(world, cmd.expect("catalog tool"));
}

fn create_face(world: &World, history: &mut History, name: &str, tool: &str, feed: f64) {
    let cmd = CreateOperation::new(world, name, tool, SrcRef::Stock, face_params(19.5, feed));
    history.execute(world, cmd.expect("create face op"));
    let cmd = GenerateOperation::new(world, name).expect("generate face op");
    history.execute(world, cmd);
}

fn create_face_nogen(world: &World, history: &mut History, name: &str, tool: &str) {
    let cmd = CreateOperation::new(world, name, tool, SrcRef::Stock, face_params(19.5, 1200.0));
    history.execute(world, cmd.expect("create face op"));
}

/// Stock, one tool per name used, and two materialized setups - the
/// stage A fixture: Setup2 is active, its private stock the same box.
fn two_setup_world() -> (World, History) {
    let world = new_world();
    let mut history = History::default();
    box_stock(&world, &mut history, 60.0, 60.0, 20.0);
    add_tool(&world, &mut history, "EM-0400-2FL");
    add_tool(&world, &mut history, "EM-0600-4FL");
    let cmd = CreateSetup::new(&world, None, None);
    history.execute(&world, cmd.expect("materialize"));
    let cmd = CreateSetup::new(&world, None, None);
    history.execute(&world, cmd.expect("second setup"));
    box_stock(&world, &mut history, 60.0, 60.0, 20.0);
    (world, history)
}

fn op_entity(world: &World, name: &str) -> flecs_ecs::prelude::Entity {
    model::find_by_name(world, name).expect("entity")
}

fn cached_path(world: &World, name: &str) -> Toolpath {
    model::toolpath_of(world, &op_entity(world, name))
        .expect("generated")
        .path
}

// ---------------------------------------------------------------------
// The independent recomputation (law-agreement, not a golden copy):
// re-add every move by the documented arithmetic and demand equality.
// ---------------------------------------------------------------------

fn seg_len(m: &Move) -> f64 {
    let dz = m.to.z - m.from.z;
    let dxy = ((m.to.x - m.from.x).powi(2) + (m.to.y - m.from.y).powi(2)).sqrt();
    match m.arc {
        None => (dxy * dxy + dz * dz).sqrt(),
        Some(a) => {
            let ra = ((m.from.x - a.center.x).powi(2) + (m.from.y - a.center.y).powi(2)).sqrt();
            let rb = ((m.to.x - a.center.x).powi(2) + (m.to.y - a.center.y).powi(2)).sqrt();
            let r = ra.max(rb);
            let a0 = (m.from.y - a.center.y).atan2(m.from.x - a.center.x);
            let a1 = (m.to.y - a.center.y).atan2(m.to.x - a.center.x);
            let mut d = if a.ccw { a1 - a0 } else { a0 - a1 };
            d = d.rem_euclid(2.0 * std::f64::consts::PI);
            let xy = if r > 0.0 && d > 0.0 { r * d } else { dxy };
            (xy * xy + dz * dz).sqrt()
        }
    }
}

/// (cut_mm, rapid_mm, seconds) over one cached path, move by move.
fn recompute(path: &Toolpath) -> (f64, f64, f64) {
    let (mut cut, mut rap, mut secs) = (0.0f64, 0.0f64, 0.0f64);
    for m in &path.moves {
        let l = seg_len(m);
        match m.kind {
            camcraft_app::cl::MoveKind::Rapid => {
                rap += l;
                secs += l / RAPID_MM_S;
            }
            camcraft_app::cl::MoveKind::Feed | camcraft_app::cl::MoveKind::Plunge => {
                cut += l;
                match m.feed {
                    Some(f) if f > 0.0 => secs += 60.0 * l / f,
                    _ => secs += l / RAPID_MM_S,
                }
            }
        }
    }
    for c in &path.cycles {
        secs += c.dwell;
    }
    (cut, rap, secs)
}

fn near(a: f64, b: f64) -> bool {
    (a - b).abs() <= 1e-6 * a.max(b).max(1.0)
}

#[test]
fn rows_follow_first_use_order_and_count_switches() {
    let world = new_world();
    let mut history = History::default();
    box_stock(&world, &mut history, 60.0, 60.0, 20.0);
    add_tool(&world, &mut history, "EM-0400-2FL");
    add_tool(&world, &mut history, "EM-0600-4FL");
    create_face(&world, &mut history, "A_Face", "EM-0400-2FL", 1200.0);
    create_face(&world, &mut history, "B_Face", "EM-0600-4FL", 1800.0);
    create_face(&world, &mut history, "C_Face", "EM-0400-2FL", 1200.0);

    let rows = tool_list::for_setup(&world, None);
    assert_eq!(rows.len(), 2, "two tools, first-use order");
    assert_eq!(rows[0].tool, "EM-0400-2FL");
    assert_eq!(rows[1].tool, "EM-0600-4FL");
    assert_eq!(rows[0].ops, vec!["A_Face".to_string(), "C_Face".to_string()]);
    assert_eq!(rows[1].ops, vec!["B_Face".to_string()]);
    // Loaded at A, switched away, switched BACK at C: two loads.
    assert_eq!(rows[0].changes, 2);
    assert_eq!(rows[1].changes, 1);
    // The roll-up of a setup-free document is the same list.
    let prog = tool_list::program(&world);
    assert_eq!(prog.len(), 2);
    assert_eq!(prog[0].changes, 2);
    assert_eq!(prog[1].changes, 1);
}

#[test]
fn time_uses_each_moves_feed() {
    // Two operations, the SAME tool, different table feeds: the row's
    // minutes must be the moves' own arithmetic, not one average.
    let world = new_world();
    let mut history = History::default();
    box_stock(&world, &mut history, 60.0, 60.0, 20.0);
    add_tool(&world, &mut history, "EM-0400-2FL");
    create_face(&world, &mut history, "Face1", "EM-0400-2FL", 1200.0);
    create_face(&world, &mut history, "Face2", "EM-0400-2FL", 2400.0);

    let p1 = cached_path(&world, "Face1");
    let p2 = cached_path(&world, "Face2");
    assert!(!p1.moves.is_empty() && !p2.moves.is_empty(), "real caches");
    let (cut1, rap1, secs1) = recompute(&p1);
    let (cut2, rap2, secs2) = recompute(&p2);

    let rows = tool_list::program(&world);
    assert_eq!(rows.len(), 1, "one tool");
    let row = &rows[0];
    assert_eq!(row.ops, vec!["Face1".to_string(), "Face2".to_string()]);
    assert_eq!(row.changes, 1, "same tool twice is one load");
    assert!(near(row.cut_mm, cut1 + cut2), "{} vs {}", row.cut_mm, cut1 + cut2);
    assert!(near(row.rapid_mm, rap1 + rap2));
    let want = (secs1 + secs2) / 60.0 + TOOL_CHANGE_S / 60.0;
    assert!(near(row.minutes, want), "{} vs {}", row.minutes, want);

    // The feed law itself, isolated: the same cutter over the same
    // face at double the feed takes (about) half the cutting time -
    // only true if each move's own feed divided its own length.
    let c1 = (cut1 + rap1) / 1.0; // lengths, for context in a failure
    let half = secs2 - rap2 / RAPID_MM_S;
    let full = secs1 - rap1 / RAPID_MM_S;
    assert!(
        near(half, full / 2.0),
        "2400 cut {half} vs 1200 cut {full}/2 (lengths {c1})"
    );
}

#[test]
fn ungenerated_ops_are_listed_with_zero_time() {
    let world = new_world();
    let mut history = History::default();
    box_stock(&world, &mut history, 60.0, 60.0, 20.0);
    add_tool(&world, &mut history, "EM-0400-2FL");
    add_tool(&world, &mut history, "EM-0600-4FL");
    create_face(&world, &mut history, "Face1", "EM-0400-2FL", 1200.0);
    create_face_nogen(&world, &mut history, "Ghost", "EM-0600-4FL");
    assert_eq!(
        operation::status_of(&world, &op_entity(&world, "Ghost")),
        OpStatus::Never
    );

    let rows = tool_list::for_setup(&world, None);
    assert_eq!(rows.len(), 2, "the never-generated tool is still listed");
    let ghost = rows.iter().find(|r| r.tool == "EM-0600-4FL").expect("row");
    assert_eq!(ghost.ops, vec!["Ghost".to_string()]);
    assert_eq!(ghost.cut_mm, 0.0, "entry only: no path, no length");
    assert_eq!(ghost.rapid_mm, 0.0);
    assert_eq!(ghost.changes, 1);
    // The zero the law promises is the ZERO TRAVEL time: the row's
    // minutes still carry its one change allowance, and no phantom
    // cutting - minutes is exactly TOOL_CHANGE_S worth.
    assert!(near(ghost.minutes, TOOL_CHANGE_S / 60.0));
    let em400 = rows.iter().find(|r| r.tool == "EM-0400-2FL").expect("row");
    let (cut, _rap, secs) = recompute(&cached_path(&world, "Face1"));
    assert!(near(em400.cut_mm, cut));
    assert!(near(em400.minutes, secs / 60.0 + TOOL_CHANGE_S / 60.0));
}

#[test]
fn per_setup_lists_split_at_the_setup_boundary() {
    let (world, mut history) = two_setup_world();
    create_face(&world, &mut history, "Face2", "EM-0600-4FL", 1800.0); // joins Setup2
    let cmd = ActivateSetup::new(&world, Some("Setup1"));
    history.execute(&world, cmd.expect("activate Setup1"));
    create_face(&world, &mut history, "Face1", "EM-0400-2FL", 1200.0); // joins Setup1

    let s1 = tool_list::for_setup(&world, Some("Setup1"));
    assert_eq!(s1.len(), 1);
    assert_eq!(s1[0].tool, "EM-0400-2FL");
    assert_eq!(s1[0].ops, vec!["Face1".to_string()]);
    assert_eq!(s1[0].changes, 1);

    // The implicit-setup law: None and a name that wears nothing are
    // both the first setup.
    let none = tool_list::for_setup(&world, None);
    assert_eq!(none, s1);
    let ghost = tool_list::for_setup(&world, Some("Setup9"));
    assert_eq!(ghost, s1, "an unknown name is the first setup, as rank says");

    let s2 = tool_list::for_setup(&world, Some("Setup2"));
    assert_eq!(s2.len(), 1);
    assert_eq!(s2[0].tool, "EM-0600-4FL");
    assert_eq!(s2[0].ops, vec!["Face2".to_string()]);

    // The program walks the slices in order: rank first, names within.
    let prog = tool_list::program(&world);
    assert_eq!(prog.len(), 2);
    assert_eq!(prog[0].tool, "EM-0400-2FL", "Setup1's tool is first");
    assert_eq!(prog[1].tool, "EM-0600-4FL");

    // setup_minutes is exactly the row sum, change allowance in.
    let sum: f64 = s1.iter().map(|r| r.minutes).sum();
    assert!(near(tool_list::setup_minutes(&world, Some("Setup1")), sum));
}

#[test]
fn program_rollup_charges_a_change_per_switch_across_setups() {
    // Different tool across the boundary: the program charges the
    // switch exactly once, and the boundary is where it happens.
    let (world, mut history) = two_setup_world();
    create_face(&world, &mut history, "Face2", "EM-0600-4FL", 1800.0); // Setup2
    let cmd = ActivateSetup::new(&world, Some("Setup1"));
    history.execute(&world, cmd.expect("activate"));
    create_face(&world, &mut history, "Face1", "EM-0400-2FL", 1200.0); // Setup1

    let prog = tool_list::program(&world);
    assert_eq!(prog.iter().map(|r| r.changes).sum::<usize>(), 2);

    // Same tool across the boundary: it stayed in the spindle, so the
    // program charges ONE load - while each setup's own list charges
    // its first use, and their sum is larger. The two numbers are both
    // true at their own altitude (stage E prints both).
    let (world, mut history) = two_setup_world();
    create_face(&world, &mut history, "Face2", "EM-0400-2FL", 1200.0); // Setup2
    let cmd = ActivateSetup::new(&world, Some("Setup1"));
    history.execute(&world, cmd.expect("activate"));
    create_face(&world, &mut history, "Face1", "EM-0400-2FL", 1200.0); // Setup1

    let prog = tool_list::program(&world);
    assert_eq!(prog.len(), 1);
    assert_eq!(prog[0].changes, 1, "a re-clamp does not reload a standing tool");
    assert_eq!(prog[0].ops, vec!["Face1".to_string(), "Face2".to_string()]);
    let per: usize = tool_list::for_setup(&world, Some("Setup1"))
        .into_iter()
        .map(|r| r.changes)
        .chain(tool_list::for_setup(&world, Some("Setup2")).into_iter().map(|r| r.changes))
        .sum();
    assert_eq!(per, 2);
    assert!(
        tool_list::program(&world)[0].minutes
            < tool_list::setup_minutes(&world, Some("Setup1"))
                + tool_list::setup_minutes(&world, Some("Setup2")),
        "the roll-up skips the phantom second load"
    );
}

#[test]
fn empty_document_lists_nothing() {
    let world = new_world();
    assert!(tool_list::program(&world).is_empty());
    assert!(tool_list::for_setup(&world, None).is_empty());
    assert!(tool_list::for_setup(&world, Some("Setup1")).is_empty());
    assert_eq!(tool_list::setup_minutes(&world, None), 0.0);
    assert_eq!(tool_list::setup_minutes(&world, Some("Setup1")), 0.0);
}
