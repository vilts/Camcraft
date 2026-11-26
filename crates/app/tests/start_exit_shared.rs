//! The shared start / exit pick mechanism (#3324, design #1448): one
//! capability per kind in the registry, one accessor that reads the picks
//! of every kind, one refusal that names the kind and its reason, and the
//! MCP doors (`edit_operation`'s schema, `list_operation_types`) that say
//! which kinds take a pick.
//!
//! Gate map (#3324): covers app::operation (kinds pick capability,
//! path_picks accessor, sane), app::mcp (tools schema, job kind list).
//! #3330: app::operation (generate, the 5-Axis Contour arm) and
//! motion::finishing3d (ring_seam) through the application.
//! Gate map (#3331): covers app::operation (generate: the Facing, Profile
//! Rough and Profile Finish picks), app::commands (EditOperationParams).
//! Gate map (#3333, `start_exit_shared/sinker.rs`): covers
//! motion::sinker_picks, app::operation (sinker_op, sinker_side_op).

// #3333: the sinker kinds' orbit picks and side-move picks.
#[path = "start_exit_shared/sinker.rs"]
mod sinker;

mod common;
// #3328: the hole order of Drill, Bore and Thread under the picks.
#[path = "start_exit_shared/hole_order.rs"]
mod hole_order;

use camcraft_app::mcp::Server;
use camcraft_app::geo2d::Point2;
use camcraft_app::operation::{Carried, Height, HeightReference, KindInfo, OpParams, OpType, PickCapability};
use camcraft_app::path_picks::PickRole;
use serde_json::{json, Value};

// #3331: the turning kinds' picks, in the lathe (Z, X) plane.
#[path = "start_exit_shared/turning.rs"]
mod turning;

fn call(s: &mut Server, name: &str, args: Value) -> (bool, Value) {
    let msg = json!({ "jsonrpc": "2.0", "id": 1, "method": "tools/call", "params": { "name": name, "arguments": args } });
    let reply = s.handle(&msg).expect("an answer");
    let r = reply["result"].clone();
    (r["isError"] == json!(true), r)
}

fn job() -> Server {
    let mut s = Server::new();
    assert!(!call(&mut s, "set_stock", json!({ "box": { "dx": 60.0, "dy": 40.0, "dz": 20.0 } })).0);
    assert!(!call(&mut s, "add_tool", json!({ "catalog": "EM-1000-4FL" })).0);
    s
}

/// The maintainer's case (#1448, 2026-11-22): a start point on a 2D
/// Offset. It is refused, and the refusal names the kind and says why.
#[test]
fn a_pick_on_a_kind_that_does_not_take_it_names_the_kind_and_the_reason() {
    let mut s = job();
    for (kind, label) in [("2D Offset", "2D Offset"), ("2D Pocket", "2D Pocket"), ("3D Pocket", "3D Pocket")] {
        let (err, r) = call(&mut s, "add_operation", json!({ "kind": kind, "tool": "EM-1000-4FL", "name": "Op" }));
        assert!(!err, "{kind}: {r}");
        for (role, what) in [("start", "start point"), ("exit", "exit point")] {
            let patch = json!({ "name": "Op", "params": { "picks": { role: [[35.0, 0.0]] } } });
            let (err, r) = call(&mut s, "edit_operation", patch);
            let text = r.to_string();
            assert!(err, "{kind}: a {what} is refused: {text}");
            assert!(text.contains(label), "{kind}: the refusal names the kind: {text}");
            assert!(text.contains(what), "{kind}: the refusal names the pick: {text}");
            assert!(text.contains("#1448"), "{kind}: the refusal says where the work is tracked: {text}");
        }
        assert!(!call(&mut s, "delete_operation", json!({ "name": "Op" })).0);
    }
}

#[test]
fn every_kind_has_one_pick_capability_and_only_the_taken_ones_take_a_pick() {
    for k in KindInfo::ALL {
        let cap = k.pick();
        match k.ty {
            OpType::Contour => assert_eq!(cap, PickCapability::Ring, "the contour takes a ring pick"),
            // #3330: the 5-axis contour seats its waterline rings at the pick.
            OpType::Contour5D => assert_eq!(cap, PickCapability::Ring, "the 5-axis contour takes a ring pick"),
            OpType::WireContour => assert_eq!(cap, PickCapability::Threading, "the wire takes its threading point"),
            // #3326: the Face takes the corner of its first row and the side of its last.
            OpType::Face => assert_eq!(cap, PickCapability::Raster, "the face takes a raster pick"),
            // #3328: the first hole and the last hole of the list.
            OpType::Drill | OpType::Bore | OpType::Thread => assert_eq!(cap, PickCapability::HoleOrder, "{}: a hole order pick", k.label),
            // #3331 slice 1: a point in the lathe (Z, X) plane.
            OpType::Facing | OpType::ProfileRough | OpType::ProfileFinish => {
                assert_eq!(cap, PickCapability::Meridian, "{} takes a lathe-plane pick", k.label)
            }
            // #3325: the Slot's slice of #1448 shipped.
            OpType::Slot => assert_eq!(cap, PickCapability::Ends, "the slot takes a pick at one of its ends"),
            // #3333: the orbit ring, and the two ends of the side move.
            OpType::SinkerBurn => assert_eq!(cap, PickCapability::Ring, "the sinker burn takes a pick on its orbit"),
            OpType::SinkerSideBurn => assert_eq!(cap, PickCapability::Ends, "the side burn takes an end of its side move"),
            _ => {
                let PickCapability::NotTaken(why) = cap else {
                    panic!("{}: only the contour, the wire, the face, the slot and the three turning kinds take a pick", k.label)
                };
                assert!(!why.is_empty(), "{}: the reason is named", k.label);
            }
        }
    }
    assert!(PickCapability::Ring.takes(PickRole::Start) && PickCapability::Ring.takes(PickRole::Exit));
    // #3332: the wire takes its exit too.
    assert!(PickCapability::Threading.takes(PickRole::Start) && PickCapability::Threading.takes(PickRole::Exit));
    assert!(PickCapability::Meridian.takes(PickRole::Start) && PickCapability::Meridian.takes(PickRole::Exit));
    let probe = KindInfo::ALL.iter().find(|k| k.ty == OpType::Probe).map(KindInfo::pick);
    assert!(matches!(probe, Some(PickCapability::NotTaken(w)) if w.contains("cuts no path")), "{probe:?}");
}

#[test]
fn path_picks_reads_the_contour_pick_and_the_wire_start() {
    let mut contour = seeded(OpType::Contour);
    assert!(contour.path_picks().is_empty(), "a new contour has no pick");
    if let Some(p) = contour.path_picks_mut() {
        p.start = vec![camcraft_app::path_picks::PathPick::xy(Point2::new(35.0, 0.0))];
        p.exit = vec![camcraft_app::path_picks::PathPick::xy(Point2::new(0.0, 25.0))];
    }
    let picks = contour.path_picks();
    let start = picks.first(PickRole::Start).expect("the start");
    let exit = picks.first(PickRole::Exit).expect("the exit");
    assert_eq!((start.xy.x, start.xy.y, start.z), (35.0, 0.0, None));
    assert_eq!((exit.xy.x, exit.xy.y, exit.z), (0.0, 25.0, None));
    assert!(picks.refusal().is_none());

    let mut wire = seeded(OpType::WireContour);
    if let OpParams::WireContour(w) = &mut wire {
        w.picks.start = vec![camcraft_app::path_picks::PathPick::xy(Point2::new(4.0, -3.0))];
    }
    let picks = wire.path_picks();
    let start = picks.first(PickRole::Start).expect("the wire start");
    assert_eq!((start.xy.x, start.xy.y), (4.0, -3.0));
    assert!(picks.first(PickRole::Exit).is_none());

    // The list form refuses a second pick by name (one pick per operation so far).
    let mut two = picks.clone();
    two.start.push(start);
    assert!(two.refusal().is_some_and(|m| m.contains("one start point")), "{:?}", two.refusal());
}

/// The registry's own seed of `ty`, from neutral carried numbers.
fn seeded(ty: OpType) -> OpParams {
    let h = |r, v| Height::of(r, v);
    let carried = Carried {
        feed: 1000.0,
        plunge: 300.0,
        climb: true,
        clearance: h(HeightReference::TopOfStock, 10.0),
        stepover: 3.0,
        stepdown: 2.0,
        top: h(HeightReference::TopOfStock, 0.0),
        bottom: h(HeightReference::TopOfStock, -4.0),
        retract: h(HeightReference::TopOfStock, 5.0),
        tilt: None,
    };
    let k = KindInfo::ALL.iter().find(|k| k.ty == ty).expect("the kind");
    (k.seed)(&carried)
}

#[test]
fn the_mcp_doors_name_the_pick_fields_and_the_kinds_that_take_them() {
    let mut s = Server::new();
    let reply = s.handle(&json!({ "jsonrpc": "2.0", "id": 1, "method": "tools/list", "params": {} })).expect("an answer");
    let edit = reply["result"]["tools"]
        .as_array()
        .and_then(|t| t.iter().find(|t| t["name"] == "edit_operation"))
        .expect("edit_operation")
        .clone();
    let doc = edit["inputSchema"]["properties"]["params"]["description"].as_str().unwrap_or_default().to_owned();
    for want in ["picks", "2D Contour", "list_operation_types"] {
        assert!(doc.contains(want), "the schema names {want}: {doc}");
    }
    let (err, kinds) = call(&mut s, "list_operation_types", json!({}));
    assert!(!err, "{kinds}");
    let text = kinds.to_string();
    assert!(text.contains("start_exit"), "each kind says if it takes a pick: {text:.400}");
}

// #3330: the 5-Axis Contour takes a start and an exit pick.
#[path = "start_exit_shared/contour5d.rs"]
mod contour5d;
