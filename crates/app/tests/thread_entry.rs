//! A thread mill's way in (#992): a thread mill is not centre-cutting
//! and the ride feeds it down the hole's axis, so a Thread operation
//! whose hole the program ahead has not opened - nothing drilled, a
//! drill too narrow or too shallow, or a Drill moved after it - is
//! refused by name at generate; the same thread after a tap drill
//! generates, verifies clean and posts.
//!
//! Provenance: public handbook thread milling (tap-drill first, to the
//! minor diameter, deeper than the thread); our own fixtures.
//!
//! Gate map (M4.4): covers app::operation (thread_entry, thread_op,
//! drill_prior, stamp), app::rest (prior_plan, prior_digest),
//! app::commands (GenerateOperation, ReorderOperation), app::posting.
//! Lane shape (M4.4): all tests here run in seconds.

mod common;
mod thread_support;

use flecs_ecs::prelude::World;

use camcraft_app::cl::ThreadHand;
use camcraft_app::commands::{CreateOperation, GenerateOperation, History, ReorderOperation};
use camcraft_app::model;
use camcraft_app::operation::{self, OpStatus, SrcRef};
use camcraft_app::posting;

use thread_support::*;

fn multi_world() -> (World, History) {
    bare_world(R_M8, &[(MULTI, catalog_tool(MULTI))])
}

/// Create "Thread1" (M8 x 1.25, multi-form) and [`generate`] it.
fn try_thread(world: &World, history: &mut History) -> String {
    let params = thread(R_M8, None, ThreadHand::Right, true);
    let op = CreateOperation::new(world, "Thread1", MULTI, SrcRef::Entity("disc".into()), params);
    history.execute(world, op.expect("an undrilled thread is a valid operation to make"));
    generate(world, history)
}

/// Generate "Thread1": the refusal's words, or empty when it generated.
fn generate(world: &World, history: &mut History) -> String {
    match GenerateOperation::new(world, "Thread1") {
        Ok(cmd) => {
            history.execute(world, cmd);
            String::new()
        }
        Err(e) => e.to_string(),
    }
}

fn status(world: &World, name: &str) -> OpStatus {
    operation::status_of(world, &model::find_by_name(world, name).expect("op"))
}

#[test]
fn a_thread_with_nothing_drilled_ahead_is_refused_by_name() {
    let (world, mut history) = multi_world();
    let msg = try_thread(&world, &mut history);
    assert!(msg.contains("the hole at (0.000, 0.000) is not drilled yet"), "{msg}");
    assert!(msg.contains("add a drilling operation of at least the 6.647 mm minor diameter"), "{msg}");
    assert_eq!(status(&world, "Thread1"), OpStatus::Never, "no path is cached for a refused thread");
    assert!(!posting::verification_gate(&world).is_empty(), "nothing posts");
}

#[test]
fn a_tap_drill_ahead_opens_the_hole_and_the_thread_verifies_clean() {
    let (world, mut history) = multi_world();
    drill_ahead(&world, &mut history, tap_drill_for(R_M8), DRILL_FLOOR);
    assert_eq!(try_thread(&world, &mut history), "", "the drilled thread generates");
    assert_eq!(status(&world, "Thread1"), OpStatus::Ok);
    assert_clean(&world);
}

#[test]
fn a_drill_of_the_mills_own_diameter_opens_the_hole_too() {
    // The issue's lower bound: a Ø6 drill under a Ø6 mill in an M8 hole - the grid reads the mill's own
    // cylinder (the minor circle less one cell is narrower here), and it is air.
    let (world, mut history) = multi_world();
    drill_ahead(&world, &mut history, 6.0, DRILL_FLOOR);
    assert_eq!(try_thread(&world, &mut history), "");
}

#[test]
fn a_drill_too_narrow_or_too_shallow_is_refused() {
    // Ø5 under a Ø6 mill: the mill's rim lands on the ring the drill left.
    let (world, mut history) = multi_world();
    drill_ahead(&world, &mut history, 5.0, DRILL_FLOOR);
    let msg = try_thread(&world, &mut history);
    assert!(msg.contains("not drilled yet") && msg.contains("stands at z 20.000"), "{msg}");
    // The tap drill's point ends above the thread's floor: its cone stands in the mill's way.
    let (world, mut history) = multi_world();
    drill_ahead(&world, &mut history, tap_drill_for(R_M8), FLOOR + 1.0);
    let msg = try_thread(&world, &mut history);
    assert!(msg.contains("not drilled yet") && msg.contains("above the thread mill's lowest point at z 8.000"), "{msg}");
}

#[test]
fn moving_the_drill_after_the_thread_marks_it_stale_and_refuses_it() {
    let (world, mut history) = multi_world();
    drill_ahead(&world, &mut history, tap_drill_for(R_M8), DRILL_FLOOR);
    assert_eq!(try_thread(&world, &mut history), "");
    assert_eq!(status(&world, "Thread1"), OpStatus::Ok);
    history.execute(&world, ReorderOperation::new(&world, "Drill1", "Thread1", true).expect("reorder"));
    assert_eq!(status(&world, "Thread1"), OpStatus::Stale, "the program ahead of the thread changed");
    assert!(!posting::verification_gate(&world).is_empty(), "a stale thread does not post");
    let msg = generate(&world, &mut history);
    assert!(msg.contains("not drilled yet"), "{msg}");
    // Undo puts the drill back ahead: the cached thread is current again.
    assert!(history.undo(&world));
    assert_eq!(status(&world, "Thread1"), OpStatus::Ok);
}

/// #1212: the Thread operation's own R plane (`top_z`), as `params` with it moved to `r`.
fn with_r(mut params: camcraft_app::operation::OpParams, r: f64) -> camcraft_app::operation::OpParams {
    if let camcraft_app::operation::OpParams::Thread { top_z, .. } = &mut params {
        *top_z = camcraft_app::operation::Height::fixed(r);
    }
    params
}

#[test]
fn the_operations_r_plane_is_where_the_rapid_stops_and_the_feed_starts() {
    let (world, mut history) = multi_world();
    drill_ahead(&world, &mut history, tap_drill_for(R_M8), DRILL_FLOOR);
    let path = make(&world, &mut history, MULTI, with_r(thread(R_M8, None, ThreadHand::Right, true), TOP + 2.0));
    let first = path.moves.iter().position(|m| m.kind == camcraft_app::cl::MoveKind::Feed).expect("a feed");
    assert_eq!(path.moves[first - 1].to.z, TOP + 2.0, "the rapid stops on the stated R plane, not the hole's top");
    assert_eq!(path.moves[first].from.z, TOP + 2.0);
    assert_clean(&world);
}

#[test]
fn an_r_plane_under_the_holes_top_is_refused_when_the_operation_is_made() {
    let (world, _history) = multi_world();
    let params = with_r(thread(R_M8, None, ThreadHand::Right, true), TOP - 1.0);
    let msg = CreateOperation::new(&world, "Thread1", MULTI, SrcRef::Entity("disc".into()), params).err().map(|e| e.to_string()).unwrap_or_default();
    assert!(msg.contains("the R plane at z 19 is below the top of the hole at (0, 0) at z 20"), "{msg}");
}

#[test]
fn an_r_plane_that_ends_the_rapid_in_the_stock_left_ahead_is_refused_by_name() {
    // The block stands 2 mm over the hole's mouth and a Ø5 drill ahead opens only part of the Ø6 mill's
    // disc: the thread's R plane on the hole's top (z 20) would end its rapid in the ring left at z 22.
    let world = camcraft_app::world::new_world();
    let mut history = History::default();
    let at = camcraft_app::schema::Location { pos: camcraft_app::geometry::Point3::new(0.0, 0.0, BOTTOM), ..Default::default() };
    let stock = camcraft_app::commands::SetStock::new(&world, camcraft_app::stock::StockShape::Box { dx: 40.0, dy: 40.0, dz: TOP + 2.0 - BOTTOM }, at);
    history.execute(&world, stock.expect("box stock"));
    history.execute(&world, camcraft_app::commands::AddTool::new(&world, MULTI, catalog_tool(MULTI)).expect("tool"));
    history.execute(&world, camcraft_app::commands::ImportModel::from_mesh("disc", disc_with_hole(R_M8)));
    // The drill's own R plane on the stock top, so its cycle rapids over the stock, not into it.
    history.execute(&world, camcraft_app::commands::AddTool::new(&world, DRILL, tap_drill(5.0)).expect("drill"));
    let mut dr = drill(2.5, DRILL_FLOOR);
    if let camcraft_app::operation::OpParams::Drill { top_z, clearance_z, .. } = &mut dr {
        *top_z = camcraft_app::operation::Height::fixed(TOP + 2.0);
        *clearance_z = camcraft_app::operation::Height::fixed(TOP + 10.0);
    }
    let op = CreateOperation::new(&world, "Drill1", DRILL, SrcRef::Entity("disc".into()), dr).expect("create drill");
    history.execute(&world, op);
    history.execute(&world, GenerateOperation::new(&world, "Drill1").expect("generate drill"));
    let msg = try_thread(&world, &mut history);
    assert!(msg.contains("the rapid down to the R plane ends at z 20.000") && msg.contains("standing at z 22.000"), "{msg}");
    assert!(msg.contains("raise the R plane (feed starts here) to z 22.000"), "{msg}");
}
