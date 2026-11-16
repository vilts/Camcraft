//! #2121: the rapid law judges a traverse AT the clearance plane. A
//! clearance plane set inside the stock (a Fixed height under the stock
//! top) sent the op's traverse through uncut stock, and the program
//! posted with no warning: the law read only rapids UNDER the plane.
//! These pin the traverse convicted by name and the post held, for a
//! Contour (the issue's repro) and a Slot; the same plane over stock an
//! earlier Face cut down is still legal and still posts, a Face that
//! stops above the plane does not excuse it, and a plane inside raw stock
//! is named when the operation is made (the MCP tools' `warning`).
//!
//! Provenance: our own synthetic fixture (`top_stock.rs`'s 40 x 40 stock
//! over a 10 x 10 x 5 part), specced from the Prime Directive and the
//! shop rule "never traverse through uncut stock"; no vendor material.
//!
//! Gate map (#2121): covers app::operation (generate's verify wiring,
//! clearance_stock), app::mcp (edit's `warning`), sim::verify (rapids,
//! prior_traverse), app::posting (the gate).
//! Lane shape: all tests here run in seconds.

use camcraft_app::cl::MoveKind;
use camcraft_app::commands::{AddTool, CreateOperation, GenerateOperation, History, ImportModel, SetStock, SetStockSourceOnly};
use camcraft_app::geo2d::Point2;
use camcraft_app::geometry::{Mesh, Point3, cube};
use camcraft_app::machining2d::{Lead, Side};
use camcraft_app::model;
use camcraft_app::operation::{self, CLEARANCE_STOCK_WARNING, Height, OpParams, SrcRef};
use camcraft_app::posting;
use camcraft_app::stock::StockShape;
use camcraft_app::tools::catalog;
use camcraft_app::verify::Warning;
use flecs_ecs::prelude::World;

/// The stock's top, the part's top and the plane under test.
const STOCK_TOP: f64 = 10.0;
const PART_TOP: f64 = 5.0;
const LOW: f64 = 8.0;

fn contour(clearance: f64, retract: f64) -> OpParams {
    OpParams::Contour {
        tilt: None,
        stock_to_leave: 0.0,
        clearance_z: Height::fixed(clearance),
        retract_z: Height::fixed(retract),
        top_z: Height::fixed(PART_TOP),
        bottom_z: Height::fixed(0.0),
        stepdown: 2.5,
        feed: 800.0,
        plunge_feed: 200.0,
        climb: true,
        side: Side::Auto,
        lead: Lead::None,
        tolerance: 0.01,
        tab_count: 0,
        tab_width: 0.0,
        tab_height: 0.0,
        entry: camcraft_app::machining3d::Entry3D::Plunge,
        helix_radius: 1.0,
        blend_turns: false,
        links: camcraft_app::links::Links::PLANAR,
    }
}

/// A slot across the stock, clear of the part.
fn slot(clearance: f64) -> OpParams {
    OpParams::Slot {
        links: camcraft_app::links::Links::PLANAR,
        a: Point2::new(-12.0, -12.0),
        b: Point2::new(12.0, -12.0),
        width: 8.0,
        stock_to_leave: 0.0,
        clearance_z: Height::fixed(clearance),
        retract_z: None,
        top_z: Height::fixed(PART_TOP),
        bottom_z: Height::fixed(2.5),
        stepdown: 2.5,
        stepover: 2.0,
        feed: 800.0,
        plunge_feed: 200.0,
        climb: true,
        entry: camcraft_app::machining3d::Entry3D::Plunge,
        helix_radius: 1.0,
        tilt: None,
        extend_start: 0.0,
        extend_end: 0.0,
    }
}

fn face(cut: f64) -> OpParams {
    OpParams::Face {
        holes: Default::default(),
        blend_turns: false,
        tilt: None,
        stepover: 4.0,
        clearance_z: Height::fixed(20.0),
        cut_z: Height::fixed(cut),
        top_z: None,
        stepdown: None,
        feed: 1200.0,
        plunge_feed: 300.0,
        climb: true,
    }
}

/// The 40 x 40 x 10 stock about the origin, the 6 mm mill and the part.
fn bench() -> (World, History) {
    let world = camcraft_app::world::new_world();
    let mut history = History::default();
    let stock = StockShape::Box { dx: 40.0, dy: 40.0, dz: STOCK_TOP };
    history.execute(&world, SetStock::new(&world, stock, Default::default()).expect("stock"));
    let (_, tool) = catalog().into_iter().find(|(n, _)| *n == "EM-0600-4FL").expect("catalog");
    history.execute(&world, AddTool::new(&world, "mill", tool).expect("tool"));
    history.execute(&world, ImportModel::from_mesh("part", cube(Point3::ZERO, Point3::new(10.0, 10.0, PART_TOP))));
    (world, history)
}

fn add(world: &World, history: &mut History, name: &str, src: SrcRef, params: OpParams) {
    history.execute(world, CreateOperation::new(world, name, "mill", src, params).expect(name));
    history.execute(world, GenerateOperation::new(world, name).expect(name));
}

fn cache(world: &World, name: &str) -> camcraft_app::schema::Toolpath {
    let e = model::find_by_name(world, name).expect("op");
    model::toolpath_of(world, &e).expect("path")
}

/// The convicted lateral rapids riding at or above `plane`.
fn traverses_convicted(tp: &camcraft_app::schema::Toolpath, plane: f64) -> Vec<usize> {
    tp.warnings
        .iter()
        .filter_map(|w| match *w {
            Warning::RapidInStock { at } => Some(at),
            _ => None,
        })
        .filter(|&at| {
            let m = &tp.path.moves[at];
            let lateral = (m.to.x - m.from.x).hypot(m.to.y - m.from.y) > 1e-6;
            m.kind == MoveKind::Rapid && lateral && m.from.z.min(m.to.z) >= plane - 1e-6
        })
        .collect()
}

fn clearance_note(tp: &camcraft_app::schema::Toolpath) -> Option<&String> {
    tp.path.notes.iter().find(|n| n.starts_with(CLEARANCE_STOCK_WARNING))
}

/// The post writes nothing and says why.
fn post_is_held(world: &World, tag: &str) {
    let err = posting::post_setup_unwritten(world).expect_err("the post must be held");
    assert!(err.contains("travels through stock"), "{err}");
    let out = std::env::temp_dir().join(format!("camcraft-clearance-stock-{tag}-{}.nc", std::process::id()));
    let _ = std::fs::remove_file(&out);
    assert!(posting::post_setup(world, &out).is_err());
    assert!(!out.exists(), "the unsafe program must write nothing");
}

/// A document whose first operation is a Face down to `cut`.
fn after_face(cut: f64) -> (World, History) {
    let (world, mut history) = bench();
    add(&world, &mut history, "Op1Face", SrcRef::Stock, face(cut));
    (world, history)
}

#[test]
fn a_contour_traverse_at_a_clearance_plane_inside_the_stock_is_convicted_and_holds_the_post() {
    // The issue's repro: clearance = retract = Z 8 under the stock top
    // Z 10, on the raw block.
    let (world, mut history) = bench();
    add(&world, &mut history, "Op2Contour", SrcRef::Entity("part".into()), contour(LOW, LOW));
    let tp = cache(&world, "Op2Contour");
    let first = &tp.path.moves[0];
    assert!(first.kind == MoveKind::Rapid && first.from.z == LOW && first.to.z == LOW, "the repro's traverse: {first:?}");
    let hit = traverses_convicted(&tp, LOW);
    assert!(hit.contains(&0), "the traverse at the plane must be convicted: {:?}", tp.warnings);
    let note = clearance_note(&tp).expect("the cause is named");
    for text in ["rapid move 0 travels through stock at Z8.000", "clearance plane Z8.000", "Z10.000", "raise Clearance"] {
        assert!(note.contains(text), "{note}");
    }
    post_is_held(&world, "contour");
}

#[test]
fn a_slot_traverse_at_a_clearance_plane_inside_the_stock_is_convicted_and_holds_the_post() {
    let (world, mut history) = bench();
    add(&world, &mut history, "Op2Slot", SrcRef::Stock, slot(LOW));
    let tp = cache(&world, "Op2Slot");
    assert!(!traverses_convicted(&tp, LOW).is_empty(), "{:?}", tp.warnings);
    assert!(clearance_note(&tp).is_some(), "{:?}", tp.path.notes);
    post_is_held(&world, "slot");
}

#[test]
fn the_same_plane_over_stock_an_earlier_face_cut_down_is_air_and_posts() {
    // Faced to the part's top: the plane Z 8 rides 3 mm over what is left.
    let (world, mut history) = after_face(PART_TOP);
    add(&world, &mut history, "Op2Contour", SrcRef::Entity("part".into()), contour(LOW, LOW));
    let tp = cache(&world, "Op2Contour");
    assert!(tp.warnings.is_empty(), "{:?}", tp.warnings);
    assert!(clearance_note(&tp).is_none(), "{:?}", tp.path.notes);
    posting::post_setup_unwritten(&world).expect("a legal program posts");
}

#[test]
fn a_face_that_stops_above_the_plane_does_not_excuse_it() {
    // Faced to Z 9 only: the plane Z 8 is still 1 mm inside the stock.
    let (world, mut history) = after_face(9.0);
    add(&world, &mut history, "Op2Contour", SrcRef::Entity("part".into()), contour(LOW, LOW));
    let tp = cache(&world, "Op2Contour");
    assert!(traverses_convicted(&tp, LOW).contains(&0), "{:?}", tp.warnings);
    post_is_held(&world, "shallow-face");
}

#[test]
fn a_plane_at_the_stock_top_is_not_inside_the_stock() {
    // Clearance exactly at the stock top: the traverse is above every
    // column. (Top Z 5 under the stock is #104's note and the dive
    // law's, not this one's: retract at the plane too, plunges fed.)
    let (world, mut history) = bench();
    add(&world, &mut history, "Op1Contour", SrcRef::Entity("part".into()), contour(STOCK_TOP, STOCK_TOP));
    let tp = cache(&world, "Op1Contour");
    assert!(traverses_convicted(&tp, STOCK_TOP).is_empty(), "{:?}", tp.warnings);
    assert!(clearance_note(&tp).is_none(), "{:?}", tp.path.notes);
}

/// The word at creation for a new operation of `params` over `src`.
fn word(world: &World, src: SrcRef, params: &OpParams) -> Option<String> {
    operation::clearance_in_stock_advice(world, None, &src, params, None)
}

#[test]
fn a_clearance_plane_inside_raw_stock_is_named_when_the_operation_is_made() {
    // Nothing ahead in the program: the stock is the raw block, and the
    // plane Z 8 is 2 mm inside it - said before any path exists.
    let (world, mut history) = bench();
    for (src, params) in [(SrcRef::Entity("part".into()), contour(LOW, LOW)), (SrcRef::Stock, slot(LOW))] {
        let why = word(&world, src, &params).expect("named at creation");
        for text in ["Clearance Z 8.000 mm is inside the stock", "its top is Z 10.000 mm", "the post is held", "face the stock first"] {
            assert!(why.contains(text), "{why}");
        }
    }
    // At the stock top the plane is not inside it; a Retract under the
    // stock top is the rapid law's to judge, not the door's.
    assert_eq!(word(&world, SrcRef::Entity("part".into()), &contour(STOCK_TOP, LOW)), None);
    // A word, not a refusal: the operation is made, and the law holds
    // its post (move 0 is the traverse through the block).
    add(&world, &mut history, "Op1Contour", SrcRef::Entity("part".into()), contour(LOW, LOW));
    assert!(traverses_convicted(&cache(&world, "Op1Contour"), LOW).contains(&0));
    post_is_held(&world, "first-op");
    // The same operation, edited: the word reads the program ahead of
    // it, which is nothing.
    let op = model::operation_of(&world, &model::find_by_name(&world, "Op1Contour").expect("op")).expect("operation");
    assert!(operation::clearance_in_stock_advice(&world, None, &op.src, &op.params, Some("Op1Contour")).is_some());
}

#[test]
fn the_word_stands_down_where_only_the_law_can_know() {
    // An operation ahead may have cut the stock: no word (the law's
    // verdicts are the tests above).
    let (world, _history) = after_face(PART_TOP);
    assert_eq!(word(&world, SrcRef::Entity("part".into()), &contour(LOW, LOW)), None);
    // A tilted plane's heights run along the tool, not world Z.
    let (world, mut history) = bench();
    let mut tilted = contour(LOW, LOW);
    if let OpParams::Contour { tilt, .. } = &mut tilted {
        *tilt = Some(operation::Tilt { a_deg: 0.0, b_deg: 10.0, origin: [0.0; 3] });
    }
    assert_eq!(word(&world, SrcRef::Entity("part".into()), &tilted), None);
    // A stepped stock has no single top: Z 10 over x < 0, Z 5 beside it.
    // No word; the law reads the traverse under the tool, convicts it
    // where it meets the tall half and holds the post.
    let mut raw = cube(Point3::new(-20.0, -20.0, 0.0), Point3::new(0.0, 20.0, STOCK_TOP)).facets.to_vec();
    raw.extend(cube(Point3::new(0.0, -20.0, 0.0), Point3::new(20.0, 20.0, PART_TOP)).facets.iter().copied());
    history.execute(&world, ImportModel::from_mesh("raw-stock", Mesh::from_facets(raw)));
    let stepped = StockShape::FromModel { source: "raw-stock".into() };
    history.execute(&world, SetStock::new(&world, stepped, Default::default()).expect("stepped stock"));
    history.execute(&world, SetStockSourceOnly::new(&world, true).expect("raw stock only"));
    assert_eq!(word(&world, SrcRef::Entity("part".into()), &contour(LOW, LOW)), None);
    add(&world, &mut history, "Op1Contour", SrcRef::Entity("part".into()), contour(LOW, LOW));
    let tp = cache(&world, "Op1Contour");
    assert!(!traverses_convicted(&tp, LOW).is_empty(), "{:?}", tp.warnings);
    post_is_held(&world, "stepped");
}

#[test]
fn the_mcp_create_and_edit_tools_answer_with_the_word() {
    use serde_json::json;
    let mut server = camcraft_app::mcp::Server::new();
    let mut id = 0;
    let mut call = |name: &str, args: serde_json::Value| {
        id += 1;
        let msg = json!({ "jsonrpc": "2.0", "id": id, "method": "tools/call", "params": { "name": name, "arguments": args } });
        let reply = server.handle(&msg).expect("a response");
        assert!(reply["result"]["isError"] != json!(true), "{name}: {reply}");
        reply["result"]["structuredContent"].clone()
    };
    call("add_tool", json!({ "catalog": "EM-0600-4FL" }));
    call("set_stock", json!({ "box": { "dx": 40.0, "dy": 40.0, "dz": 10.0 } }));
    // The seed's plane stands over the stock: no word.
    let made = call("add_operation", json!({ "kind": "Face", "tool": "EM-0600-4FL", "name": "Face1" }));
    assert!(made.get("warning").is_none(), "{made}");
    // Lowered into the raw block: the answer says so.
    let low = call("edit_operation", json!({ "name": "Face1", "params": { "clearance_z": 8.0, "cut_z": 7.0 } }));
    let w = low["warning"].as_str().expect("the word rides the answer");
    assert!(w.contains("Clearance Z 8.000 mm is inside the stock"), "{w}");
    // A second operation has the first ahead of it: no word at creation.
    let next = call("add_operation", json!({ "kind": "Face", "tool": "EM-0600-4FL", "name": "Face2", "params": { "clearance_z": 8.0, "cut_z": 7.0 } }));
    assert!(next.get("warning").is_none(), "{next}");
}
