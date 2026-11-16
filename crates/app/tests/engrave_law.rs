//! #1595: an Engrave or a Trace on the finished part carries its own
//! depth law (`DepthLaw::Mark`, `operation::mark_law`). Both ride the
//! cutter's centre on a line of the part, so they enter it by design -
//! but only as a mark: under the cutter the part's surface may give way
//! by `min(Top - Bottom, cutter radius)` and no more. A deeper cut, a
//! Top typed under the surface, or a wall standing beside the line is a
//! blocking `Warning::Gouge` on the operation, and the post is refused
//! by name on its moves - with the stock aligned and with it turned
//! (where the program sim refuses to judge). A legal mark is clean on
//! the operation, and the program sim credits it (`credit_marks`), so
//! it posts on both stocks. Before #1595 both kinds fell to
//! `DepthLaw::None`: aligned, the sim refused every engraving (0.02 mm
//! included); turned, a cut 7 mm under the model posted.
//!
//! Provenance: our own fixtures (`pocket_support::block_with_pocket`)
//! over the solid law and the column sim; no vendor material consulted.
//!
//! Gate map: covers app::operation::mark_law, app::operation::generate
//! (depth_law_of, the Mark arm), app::stock_sim (marks, credit_marks),
//! app::posting, app::posting_sim (uncovered), app::mcp (add_operation,
//! verify).

mod pocket_support;

use flecs_ecs::prelude::World;
use serde_json::{json, Value};

use camcraft_app::commands::{AddTool, CreateOperation, GenerateOperation, History, ImportModel, SetStock};
use camcraft_app::geometry::feature::FaceKind;
use camcraft_app::geometry::{Mesh, Point3, quat_from_yaw};
use camcraft_app::mcp::Server;
use camcraft_app::model;
use camcraft_app::operation::{FeatureSel, GeoSel, Height, OpParams, SrcRef};
use camcraft_app::posting;
use camcraft_app::schema::{Location, STOCK_NAME};
use camcraft_app::stock::StockShape;
use camcraft_app::stock_sim;
use camcraft_app::tools::catalog;
use camcraft_app::verify::Warning;
use camcraft_app::world::new_world;

/// A 6 mm end mill: the mark budget's cap is its 3 mm radius.
const TOOL: &str = "EM-0600-4FL";
/// The round pocket's radius and its floor; the block is 40 x 40 x 8.
const R: f64 = 15.0;
const FLOOR: f64 = 4.0;
const TOP: f64 = 8.0;

/// The 40 x 40 x 8 block with a radius-[`R`] round pocket (128 chords)
/// sunk to [`FLOOR`] - the #1585 / #1593 review fixture.
fn pocket_block() -> Mesh {
    let ring: Vec<(f64, f64)> = (0..128)
        .map(|i| {
            let a = std::f64::consts::TAU * i as f64 / 128.0;
            (R * a.cos(), R * a.sin())
        })
        .collect();
    pocket_support::block_with_pocket(20.0, 20.0, &ring, FLOOR, TOP)
}

/// [`pocket_block`] as "part", and its stock over it: axis-aligned, or
/// the same box turned a quarter about Z (the same footprint, but a
/// rotation the program sim refuses to model).
fn job(rotated: bool) -> (World, History) {
    let world = new_world();
    let mut h = History::default();
    let tool = catalog().into_iter().find(|(n, _)| *n == TOOL).map(|(_, t)| t).expect("tool");
    h.execute(&world, AddTool::new(&world, TOOL, tool).expect("tool"));
    h.execute(&world, ImportModel::from_mesh("part", pocket_block()));
    // A box stock stands centred on its place, so a quarter turn about
    // Z leaves the footprint over the part.
    let rot = if rotated { quat_from_yaw(std::f64::consts::FRAC_PI_2) } else { Location::default().rot };
    let at = Location { pos: Point3::new(0.0, 0.0, 0.0), rot };
    h.execute(&world, SetStock::new(&world, StockShape::Box { dx: 40.0, dy: 40.0, dz: 8.0 }, at).expect("stock"));
    let b = model::stock_world_aabb_for(&world, None).expect("stock box");
    let on = |v: f64, want: f64| (v - want).abs() < 1e-9;
    assert!(on(b.min.x, -20.0) && on(b.min.y, -20.0) && on(b.max.x, 20.0) && on(b.max.y, 20.0), "stock at {:?}..{:?}", b.min, b.max);
    (world, h)
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Kind {
    Engrave,
    Trace,
}

const KINDS: [Kind; 2] = [Kind::Engrave, Kind::Trace];

/// The kind's centreline cut from `top` down to `bottom`, one level.
fn cut(kind: Kind, top: f64, bottom: f64) -> OpParams {
    let (clearance_z, top_z, bottom_z) = (Height::fixed(25.0), Height::fixed(top), Height::fixed(bottom));
    match kind {
        Kind::Engrave => OpParams::Engrave {
            clearance_z, retract_z: None, top_z, bottom_z, stepdown: 10.0, feed: 800.0, plunge_feed: 200.0, climb: true, tilt: None,
        },
        Kind::Trace => OpParams::Trace {
            clearance_z, retract_z: None, top_z, bottom_z, stepdown: 10.0, feed: 800.0, plunge_feed: 200.0, climb: true, tilt: None,
        },
    }
}

/// The planar up-facing face of entity `on` at height `z`, picked.
fn face_pick(world: &World, on: &str, z: f64) -> SrcRef {
    let e = model::find_by_name(world, on).expect("entity");
    let f = model::features_of(world, &e).expect("features");
    let id = f
        .faces
        .iter()
        .filter(|fa| matches!(fa.kind, FaceKind::Planar) && fa.normal[2] > 0.5)
        .find(|fa| (fa.offset - z).abs() < 1e-6)
        .map(|fa| fa.id)
        .expect("the face");
    SrcRef::Selection(vec![GeoSel { entity: on.into(), feature: FeatureSel::Face(id) }])
}

fn whole() -> SrcRef {
    SrcRef::Entity("part".into())
}

/// Create and generate "E1"; its cached warnings and notes.
fn generated(world: &World, h: &mut History, src: SrcRef, params: OpParams) -> (Vec<Warning>, Vec<String>) {
    h.execute(world, CreateOperation::new(world, "E1", TOOL, src, params).expect("create"));
    h.execute(world, GenerateOperation::new(world, "E1").expect("generate"));
    let e = model::find_by_name(world, "E1").expect("op");
    let tp = model::toolpath_of(world, &e).expect("cached path");
    assert!(!tp.path.moves.is_empty(), "the line was generated");
    (tp.warnings.clone(), tp.path.notes.clone())
}

fn gouges(w: &[Warning]) -> usize {
    w.iter().filter(|w| matches!(w, Warning::Gouge { .. })).count()
}

fn nc(tag: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!("camcraft-engrave-law-{tag}-{}.nc", std::process::id()))
}

/// The post is refused and nothing is written. `by_op`: the refusal
/// names the operation's own gouge (where the program sim judges too,
/// its clause may be the one the message leads with).
fn assert_post_refused(world: &World, tag: &str, by_op: bool) {
    let out = nc(tag);
    let _ = std::fs::remove_file(&out);
    let err = posting::post_setup(world, &out).expect_err("a gouge blocks the post");
    assert!(err.contains("post blocked"), "{tag}: {err}");
    if by_op {
        assert!(err.contains("E1: move") && err.contains("cuts past the finish surface"), "{tag}: {err}");
    }
    assert!(!out.exists(), "{tag}: blocked output is never written");
}

fn assert_posts(world: &World, tag: &str) {
    let out = nc(tag);
    let _ = std::fs::remove_file(&out);
    posting::post_setup(world, &out).unwrap_or_else(|e| panic!("{tag}: a legal mark posts: {e}"));
    assert!(out.exists(), "{tag}: the program was written");
    let _ = std::fs::remove_file(&out);
}

/// One whole-model cut on one stock: (gouges on the op, whether it posts).
fn verdict(kind: Kind, rotated: bool, top: f64, bottom: f64, tag: &str) -> (usize, bool) {
    let (world, mut h) = job(rotated);
    let (warns, _) = generated(&world, &mut h, whole(), cut(kind, top, bottom));
    let out = nc(tag);
    let _ = std::fs::remove_file(&out);
    let posted = posting::post_setup(&world, &out).is_ok();
    let _ = std::fs::remove_file(&out);
    (gouges(&warns), posted)
}

#[test]
fn a_whole_model_cut_deep_under_the_part_gouges_and_never_posts_on_either_stock() {
    // The issue's repro: Top Z8, Bottom Z1 - every edge of the part
    // followed 3 mm under the pocket floor and 7 mm down the outside
    // wall. Before #1595: no finding on the op, and it POSTED with the
    // stock turned.
    for kind in KINDS {
        for rotated in [false, true] {
            let (world, mut h) = job(rotated);
            let (warns, notes) = generated(&world, &mut h, whole(), cut(kind, TOP, 1.0));
            assert!(gouges(&warns) > 0, "{kind:?} rotated {rotated}: convicted on the op: {warns:?}");
            assert!(
                notes.iter().any(|n| n.contains("cuts deeper than a mark") && n.contains("3.000 mm")),
                "{kind:?} rotated {rotated}: the budget is said by name: {notes:?}"
            );
            // Turned, the op's own gouge is the only judge; aligned,
            // the program sim also sees it and its clause may lead.
            assert_post_refused(&world, &format!("deep-{kind:?}-{rotated}"), rotated);
        }
    }
}

#[test]
fn a_shallow_mark_is_clean_on_the_operation_and_posts_on_both_stocks() {
    // 0.3 mm into the top face along every edge. Before #1595 the
    // aligned program sim refused this (and a 0.02 mm one) as a gouge.
    for kind in KINDS {
        for rotated in [false, true] {
            let (world, mut h) = job(rotated);
            let (warns, notes) = generated(&world, &mut h, whole(), cut(kind, TOP, 7.7));
            let blocking: Vec<_> = warns.iter().filter(|w| w.blocks_post()).collect();
            assert!(blocking.is_empty(), "{kind:?} rotated {rotated}: {blocking:?}");
            assert!(!notes.iter().any(|n| n.contains("deeper than a mark")), "{notes:?}");
            assert_posts(&world, &format!("shallow-{kind:?}-{rotated}"));
        }
    }
}

#[test]
fn the_mark_budget_ends_at_the_cutter_radius_on_both_stocks_alike() {
    // 3 mm deep with a 3 mm radius is a mark; 3.1 mm is a channel the
    // model does not show. The two stocks never disagree.
    for kind in KINDS {
        for rotated in [false, true] {
            assert_eq!(verdict(kind, rotated, TOP, 5.0, "at-radius"), (0, true), "{kind:?} rotated {rotated}: 3.0 deep");
            let (g, posted) = verdict(kind, rotated, TOP, 4.9, "past-radius");
            assert!(g > 0 && !posted, "{kind:?} rotated {rotated}: 3.1 deep: {g} gouge(s), posted {posted}");
        }
    }
}

#[test]
fn a_top_typed_under_the_surface_is_not_a_mark() {
    // Top Z7 on a face at Z8, Bottom Z6.7: the operation states a 0.3 mm
    // mark and cuts 1.3 mm - a full millimetre the stated depth hides,
    // well inside the radius cap.
    for kind in KINDS {
        for rotated in [false, true] {
            let (world, mut h) = job(rotated);
            let (warns, notes) = generated(&world, &mut h, whole(), cut(kind, 7.0, 6.7));
            assert!(gouges(&warns) > 0, "{kind:?} rotated {rotated}: {warns:?}");
            assert!(notes.iter().any(|n| n.contains("0.300 mm")), "the stated depth is the budget: {notes:?}");
            assert_post_refused(&world, &format!("low-top-{kind:?}-{rotated}"), rotated);
        }
    }
}

#[test]
fn a_picked_floor_traced_along_its_wall_gouges_the_wall_and_never_posts() {
    // The pocket floor picked, 0.3 mm deep: a mark by depth - but the
    // line is the foot of the 4 mm wall, and the cutter's flank takes
    // 3 mm off it, full height. The wall is the part, not the mark.
    for kind in KINDS {
        for rotated in [false, true] {
            let (world, mut h) = job(rotated);
            let src = face_pick(&world, "part", FLOOR);
            let (warns, _) = generated(&world, &mut h, src, cut(kind, FLOOR, 3.7));
            assert!(gouges(&warns) > 0, "{kind:?} rotated {rotated}: the wall is convicted: {warns:?}");
            assert_post_refused(&world, &format!("wall-{kind:?}-{rotated}"), rotated);
        }
    }
}

#[test]
fn a_picked_top_face_marked_shallow_posts_on_both_stocks() {
    for kind in KINDS {
        for rotated in [false, true] {
            let (world, mut h) = job(rotated);
            let src = face_pick(&world, "part", TOP);
            let (warns, _) = generated(&world, &mut h, src, cut(kind, TOP, 7.7));
            let blocking: Vec<_> = warns.iter().filter(|w| w.blocks_post()).collect();
            assert!(blocking.is_empty(), "{kind:?} rotated {rotated}: {blocking:?}");
            assert_posts(&world, &format!("picked-top-{kind:?}-{rotated}"));
        }
    }
}

#[test]
fn a_cut_on_the_stock_answers_to_the_mark_law_against_the_part() {
    // The stock made from the part: the Stock source, or its top face
    // picked, is the material to remove (`part_solid` false) - but the
    // part standing in it is not (#1607): the line down its outside
    // wall, 7 mm on a 3 mm budget, is convicted as a mark too deep.
    for kind in KINDS {
        for picked in [false, true] {
            let (world, mut h) = job(false);
            h.execute(&world, SetStock::new(&world, StockShape::FromModel { source: "part".into() }, Location::default()).expect("stock"));
            let src = if picked { face_pick(&world, STOCK_NAME, TOP) } else { SrcRef::Stock };
            let (warns, notes) = generated(&world, &mut h, src, cut(kind, TOP, 1.0));
            assert!(gouges(&warns) > 0, "{kind:?} picked {picked}: the part in the stock is kept: {warns:?}");
            assert!(notes.iter().any(|n| n.contains("deeper than a mark")), "{notes:?}");
        }
    }
}

#[test]
fn the_program_sim_credits_the_mark_and_nothing_past_it() {
    // Aligned stock, where the program sim judges. A legal mark leaves
    // no gouge cell; one past its budget is charged at its depth UNDER
    // the budget, never at zero and never at its full depth.
    let sim = |top: f64, bottom: f64, src: SrcRef| {
        let (world, mut h) = job(false);
        generated(&world, &mut h, src, cut(Kind::Engrave, top, bottom));
        stock_sim::simulate_document(&world).expect("an aligned stock is judged").dev
    };
    let legal = sim(TOP, 7.7, whole());
    assert_eq!(legal.gouge_cells, 0, "worst {} at {:?}", legal.max_gouge, legal.max_gouge_at);
    let deep = sim(TOP, 1.0, whole());
    assert!(deep.gouge_cells > 0);
    assert!((deep.max_gouge - 4.0).abs() < 1e-6, "7 mm deep on a 3 mm budget is charged 4: {}", deep.max_gouge);
    // A Top typed 1 mm low: the budget is the stated 0.3, the cut 1.3.
    let low = sim(7.0, 6.7, whole());
    assert!((low.max_gouge - 1.0).abs() < 1e-6, "charged {}", low.max_gouge);
    // #1607: a cut whose source is the stock carries the same mark law
    // against the part in it, so it declares its mark too: the same
    // 0.3 mm along the block's outline is credited, and 7 mm is charged
    // past its budget exactly as the whole model's is.
    let on_stock = sim(TOP, 7.7, SrcRef::Stock);
    assert_eq!(on_stock.gouge_cells, 0, "worst {} at {:?}", on_stock.max_gouge, on_stock.max_gouge_at);
    let deep_on_stock = sim(TOP, 1.0, SrcRef::Stock);
    assert!((deep_on_stock.max_gouge - 4.0).abs() < 1e-6, "charged {}", deep_on_stock.max_gouge);
}

// ---- MCP parity -----------------------------------------------------

fn call(s: &mut Server, id: &mut i64, name: &str, args: Value) -> Value {
    *id += 1;
    let msg = json!({ "jsonrpc": "2.0", "id": *id, "method": "tools/call",
        "params": { "name": name, "arguments": args } });
    let reply = s.handle(&msg).expect("a request gets a response");
    let result = &reply["result"];
    assert_ne!(result["isError"], json!(true), "{name} refused: {}", result["content"][0]["text"]);
    result["structuredContent"].clone()
}

/// The pocket job over MCP: a whole-model `kind` cut from the top to
/// `bottom`; verify's reply.
fn mcp_verify(tag: &str, kind: &str, bottom: f64) -> Value {
    let dir = std::env::temp_dir().join(format!("camcraft-engrave-law-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("temp dir");
    let stl = dir.join("part.stl");
    pocket_support::write_stl(&pocket_block(), &stl);
    let (mut s, mut id) = (Server::new(), 0);
    let _ = s.handle(&json!({ "jsonrpc": "2.0", "id": 0, "method": "initialize", "params": {} }));
    call(&mut s, &mut id, "import_model", json!({ "name": "part", "path": stl.display().to_string() }));
    call(&mut s, &mut id, "set_stock", json!({ "box": { "dx": 40.0, "dy": 40.0, "dz": 8.0 } }));
    call(&mut s, &mut id, "add_tool", json!({ "catalog": TOOL }));
    let params = json!({ "top_z": TOP, "bottom_z": bottom, "stepdown": 10.0 });
    call(&mut s, &mut id, "add_operation", json!({ "kind": kind, "tool": TOOL, "source": "part", "name": "E1", "params": params }));
    let g = call(&mut s, &mut id, "generate", json!({}));
    assert_eq!(g["generated"][0]["status"], json!("ok"), "{g}");
    let ver = call(&mut s, &mut id, "verify", json!({}));
    let _ = std::fs::remove_dir_all(&dir);
    ver
}

/// The operation's own blocking gouge findings (not the program gate's).
fn op_gouges(ver: &Value) -> usize {
    ver["findings"]
        .as_array()
        .map_or(0, |f| f.iter().filter(|x| x["kind"] == json!("gouge") && x["op"] == json!("E1") && x["blocks_post"] == json!(true)).count())
}

#[test]
fn over_mcp_a_whole_model_engrave_and_trace_are_judged_the_same() {
    for kind in ["Engrave", "Trace"] {
        let deep = mcp_verify(&format!("deep-{kind}"), kind, 1.0);
        assert!(op_gouges(&deep) > 0, "{kind}: the operation's own gouge blocks the post: {deep}");
        assert_eq!(deep["ok"], json!(false), "{kind}: {deep}");
        let good = mcp_verify(&format!("legal-{kind}"), kind, 7.7);
        assert_eq!(op_gouges(&good), 0, "{kind}: {good}");
        assert_eq!(good["ok"], json!(true), "{kind}: a legal mark verifies ok: {good}");
    }
}
