//! #1621: a part-source 2D operation answers for EVERY part its setup
//! holds, not only the one its source names
//! (`operation::keep_law::neighbour_parts`). Before #1621 its law
//! (`Mark` for Engrave / Trace, `Section` for Contour and the picked
//! kinds, `Keep` for a whole-model pocket) read only its source's mesh:
//! a cut on part B whose flank entered part A beside it had no finding
//! on the operation - with the stock aligned the program sim held the
//! post, with it turned off the machine axes (the sim refuses to judge)
//! nothing judged A. (#2177: a quarter turn is square to the axes, and
//! the sim judges it.)
//! Now the setup's other parts take the plain solid law (no mark budget:
//! a mark is on the part it marks): the cut into A is a blocking
//! `Warning::Gouge` on the operation on both stocks; a legal mark on B
//! clear of A, and a pocket on B's own floor, post as before.
//!
//! Provenance: our own fixtures (`pocket_support::block_with_pocket`,
//! the #1585 block, beside a plain box) over the solid and mark laws
//! and the column sim; no vendor material consulted.
//!
//! Gate map: covers app::operation::keep_law (neighbour_parts),
//! app::operation::job (neighbour_parts), app::operation::generate (the
//! neighbour law), app::operation::mark_law (budget_of, target_facets,
//! sim_mark_of), app::stock_sim (marks), sim::stock_sim::mark,
//! app::posting, app::posting_sim, app::mcp (add_operation, verify).

mod area_support;
mod pocket_support;

use area_support::*;
use camcraft_app::commands::GenerateOperation;
use camcraft_app::geometry::{quat_from_yaw, Mesh, Triangle};
use camcraft_app::mcp::Server;
use camcraft_app::posting;
use camcraft_app::verify::Warning;
use serde_json::{json, Value};

/// A 6 mm end mill: its flank stands 3 mm off the line it traces.
const CUTTER: &str = "EM-0600-4FL";
/// The round pocket's radius and floor; both parts stand to [`TOP`].
const RP: f64 = 15.0;
const FLOOR: f64 = 4.0;
const TOP: f64 = 8.0;
/// The gap between the parts: 0.7 mm lets the flank 2.3 mm into A (the
/// issue's probe), 5 mm keeps it clear.
const TIGHT: f64 = 0.7;
const CLEAR: f64 = 5.0;

/// Part B, "part": the 40 x 40 x 8 pocket block on x [0, 40].
fn part_b() -> Mesh {
    let ring: Vec<(f64, f64)> = (0..128)
        .map(|i| {
            let a = std::f64::consts::TAU * f64::from(i) / 128.0;
            (RP * a.cos(), RP * a.sin())
        })
        .collect();
    let m = pocket_support::block_with_pocket(20.0, 20.0, &ring, FLOOR, TOP);
    let at = |p: Point3| Point3::new(p.x + 20.0, p.y, p.z);
    Mesh::from_facets(m.facets.iter().map(|t| Triangle::new(at(t.a), at(t.b), at(t.c))).collect())
}

/// Part A, "other": a plain box on x [-40, -gap], as tall as B.
fn part_a(gap: f64) -> Mesh {
    Mesh::from_facets(cube(Point3::new(-40.0, -20.0, 0.0), Point3::new(-gap, 20.0, TOP)).facets.to_vec())
}

/// Both parts in an 80 x 80 x 8 box stock centred on the origin:
/// axis-aligned, or turned [`OBLIQUE`] about Z (the same footprint
/// within 1e-4 mm, but a turn the program sim refuses to model).
fn job(gap: f64, rotated: bool) -> (World, History) {
    job_turned(gap, stock_turn(rotated))
}

/// [`job`] with the stock turned by `rot` about its place.
fn job_turned(gap: f64, rot: [f64; 4]) -> (World, History) {
    let world = new_world();
    let mut h = History::default();
    let tool = catalog().into_iter().find(|(n, _)| *n == CUTTER).map(|(_, t)| t).expect("tool");
    h.execute(&world, AddTool::new(&world, CUTTER, tool).expect("tool"));
    h.execute(&world, ImportModel::from_mesh("part", part_b()));
    h.execute(&world, ImportModel::from_mesh("other", part_a(gap)));
    let at = Location { pos: Point3::new(0.0, 0.0, 0.0), rot };
    h.execute(&world, SetStock::new(&world, StockShape::Box { dx: 80.0, dy: 80.0, dz: TOP }, at).expect("stock"));
    let (lo, hi) = stock_box(&world);
    // The oblique turn moves the corners by 40 * sqrt(2) * 1e-6 mm.
    let on = |v: f64, want: f64| (v - want).abs() < 1e-4;
    let level = |v: f64, want: f64| (v - want).abs() < 1e-9;
    assert!(on(lo.x, -40.0) && on(hi.x, 40.0) && on(lo.y, -40.0) && on(hi.y, 40.0) && level(hi.z, TOP), "stock at {lo:?}..{hi:?}");
    (world, h)
}

/// An Engrave (`trace` false) or a Trace, one level from `top` to `bottom`.
fn mark(trace: bool, top: f64, bottom: f64) -> OpParams {
    let (clearance_z, top_z, bottom_z) = (Height::fixed(25.0), Height::fixed(top), Height::fixed(bottom));
    if trace {
        OpParams::Trace { clearance_z, retract_z: None, top_z, bottom_z, stepdown: 10.0, feed: 800.0, plunge_feed: 200.0, climb: true, tilt: None, picks: Default::default() }
    } else {
        OpParams::Engrave { clearance_z, retract_z: None, top_z, bottom_z, stepdown: 10.0, feed: 800.0, plunge_feed: 200.0, climb: true, tilt: None, picks: Default::default() }
    }
}

fn whole_b() -> SrcRef {
    SrcRef::Entity("part".into())
}

/// Create and generate "N1"; its cached warnings and path.
fn generated_path(world: &World, h: &mut History, src: SrcRef, params: OpParams) -> (Vec<Warning>, Toolpath) {
    h.execute(world, CreateOperation::new(world, "N1", CUTTER, src, params).expect("create"));
    h.execute(world, GenerateOperation::new(world, "N1").expect("generate"));
    let e = model::find_by_name(world, "N1").expect("op");
    let tp = model::toolpath_of(world, &e).expect("cached path");
    assert!(!tp.path.moves.is_empty(), "the cut was generated");
    (tp.warnings.clone(), tp.path.clone())
}

/// [`generated_path`]'s warnings.
fn generated(world: &World, h: &mut History, src: SrcRef, params: OpParams) -> Vec<Warning> {
    generated_path(world, h, src, params).0
}

fn gouges(w: &[Warning]) -> usize {
    w.iter().filter(|w| matches!(w, Warning::Gouge { .. })).count()
}

/// The gouges on moves whose cutter (3 mm radius) can reach part A
/// (x below -gap): the conviction is of the neighbour, not of B.
fn gouges_in_a(w: &[Warning], path: &Toolpath, gap: f64) -> usize {
    let reaches = |i: usize| path.moves.get(i).is_some_and(|m| m.from.x.min(m.to.x) < -gap + 3.0);
    w.iter().filter(|w| matches!(w, Warning::Gouge { at } if reaches(*at))).count()
}

fn nc(tag: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!("camcraft-neighbour-law-{tag}-{}.nc", std::process::id()))
}

/// The post is refused and nothing is written. `by_op`: the refusal
/// names the operation's own gouge (on an aligned stock the program
/// sim's clause may lead).
fn assert_post_refused(world: &World, tag: &str, by_op: bool) {
    let out = nc(tag);
    let _ = std::fs::remove_file(&out);
    let err = posting::post_setup(world, &out).expect_err("a gouge blocks the post");
    assert!(err.contains("post blocked"), "{tag}: {err}");
    if by_op {
        assert!(err.contains("N1: move") && err.contains("cuts past the finish surface"), "{tag}: {err}");
        assert!(oblique_refused(world), "{tag}: the program sim judged the oblique stock");
    }
    assert!(!out.exists(), "{tag}: blocked output is never written");
}

fn assert_posts(world: &World, tag: &str) {
    let out = nc(tag);
    let _ = std::fs::remove_file(&out);
    posting::post_setup(world, &out).unwrap_or_else(|e| panic!("{tag}: a legal cut on B posts: {e}"));
    assert!(out.exists(), "{tag}: the program was written");
    let _ = std::fs::remove_file(&out);
}

#[test]
fn a_cut_on_b_into_its_neighbour_gouges_on_the_op_and_never_posts() {
    // The issue's probe: a 0.3 mm mark round B's outline, its flank
    // 2.3 mm into A along their shared side - and a Contour (a `Section`
    // kind) round B to Z 1, wholly in A there. Before #1621: no finding
    // for A on the op; turned off the axes, nothing judged A.
    let cases = [
        ("trace", mark(true, TOP, 7.7)),
        ("engrave", mark(false, TOP, 7.7)),
        ("contour", contour(TOP, 1.0, Side::Outside)),
    ];
    for (label, params) in &cases {
        for rotated in [false, true] {
            let (world, mut h) = job(TIGHT, rotated);
            let (warns, path) = generated_path(&world, &mut h, whole_b(), params.clone());
            assert!(gouges_in_a(&warns, &path, TIGHT) > 0, "{label} rotated {rotated}: A is kept on the op: {warns:?}");
            assert_post_refused(&world, &format!("into-a-{label}-{rotated}"), rotated);
        }
    }
}

#[test]
fn the_program_sim_refuses_a_mark_on_b_whose_flank_enters_a_neighbour() {
    // #1627: on the aligned stock, the program sim itself must convict
    // the #1621 probe (a 0.3 mm part-source mark on B whose flank enters
    // A by 2.3 mm x 0.3 mm) instead of crediting B's mark budget on A.
    for trace in [false, true] {
        for picked in [false, true] {
            let (world, mut h) = job(TIGHT, false);
            let src = if picked { sel("part", top_face(&world, "part", TOP)) } else { whole_b() };
            generated(&world, &mut h, src, mark(trace, TOP, 7.7));
            let dev = camcraft_app::stock_sim::simulate_document(&world)
                .expect("the aligned stock is judged")
                .dev;
            assert!(
                dev.gouge_cells > 0,
                "trace {trace} picked {picked}: the program sim credited B's mark on A"
            );
            assert!(
                (dev.max_gouge - 0.3).abs() < 1e-6,
                "trace {trace} picked {picked}: expected 0.3 mm gouge on A, got {}",
                dev.max_gouge
            );
            let at = dev.max_gouge_at.expect("located");
            assert!(
                at.x < -TIGHT + 0.125,
                "trace {trace} picked {picked}: worst gouge must lie on A (x < -{TIGHT}), got {at:?}"
            );
        }
    }
    // Clear of A, the same mark on B is clean in the program sim too.
    for trace in [false, true] {
        let (world, mut h) = job(CLEAR, false);
        generated(&world, &mut h, whole_b(), mark(trace, TOP, 7.7));
        let dev = camcraft_app::stock_sim::simulate_document(&world)
            .expect("the aligned stock is judged")
            .dev;
        assert_eq!(dev.gouge_cells, 0, "trace {trace}: worst {} at {:?}", dev.max_gouge, dev.max_gouge_at);
    }
    // A stock-source mark still credits every part in the setup (#1607).
    let (world, mut h) = job(TIGHT, false);
    generated(&world, &mut h, SrcRef::Stock, mark(false, TOP, 7.7));
    let dev = camcraft_app::stock_sim::simulate_document(&world)
        .expect("the aligned stock is judged")
        .dev;
    assert_eq!(dev.gouge_cells, 0, "stock source worst {} at {:?}", dev.max_gouge, dev.max_gouge_at);
}

#[test]
fn a_quarter_turned_stock_is_judged_by_the_program_sim() {
    // #2177: the stock turned a quarter about Z is the same box square
    // to the axes. The program sim judges it as it judges the aligned
    // one: B's mark whose flank enters A is convicted on A, and the
    // post is refused; clear of A the same mark posts.
    let quarter = quat_from_yaw(std::f64::consts::FRAC_PI_2);
    for trace in [false, true] {
        let (world, mut h) = job_turned(TIGHT, quarter);
        generated(&world, &mut h, whole_b(), mark(trace, TOP, 7.7));
        let dev = camcraft_app::stock_sim::simulate_document(&world).expect("a quarter-turned box is judged").dev;
        assert!((dev.max_gouge - 0.3).abs() < 1e-6, "trace {trace}: expected 0.3 mm gouge on A, got {}", dev.max_gouge);
        let at = dev.max_gouge_at.expect("located");
        assert!(at.x < -TIGHT + 0.125, "trace {trace}: worst gouge must lie on A (x < -{TIGHT}), got {at:?}");
        assert_post_refused(&world, &format!("quarter-into-a-{trace}"), false);
        let (world, mut h) = job_turned(CLEAR, quarter);
        generated(&world, &mut h, whole_b(), mark(trace, TOP, 7.7));
        let dev = camcraft_app::stock_sim::simulate_document(&world).expect("a quarter-turned box is judged").dev;
        assert_eq!(dev.gouge_cells, 0, "trace {trace}: worst {} at {:?}", dev.max_gouge, dev.max_gouge_at);
        assert_posts(&world, &format!("quarter-clear-{trace}"));
    }
}

#[test]
fn a_mark_on_b_within_its_budget_clear_of_a_posts_on_both_stocks() {
    // The same 0.3 mm mark with 5 mm between the parts: the flank stays
    // 2 mm off A, the mark is inside B's budget - clean, and it posts.
    // Whole model or B's top face picked: either way B is the source and
    // keeps its mark law; only A takes the plain solid law.
    for trace in [false, true] {
        for picked in [false, true] {
            for rotated in [false, true] {
                let (world, mut h) = job(CLEAR, rotated);
                let src = if picked { sel("part", top_face(&world, "part", TOP)) } else { whole_b() };
                let warns = generated(&world, &mut h, src, mark(trace, TOP, 7.7));
                let blocking: Vec<_> = warns.iter().filter(|w| w.blocks_post()).collect();
                assert!(blocking.is_empty(), "trace {trace} picked {picked} rotated {rotated}: {blocking:?}");
                assert_posts(&world, &format!("clear-{trace}-{picked}-{rotated}"));
            }
        }
    }
}

#[test]
fn a_mark_on_b_deeper_than_its_budget_is_still_judged_by_its_own_law() {
    // B keeps its own (mark) law beside a neighbour: 7 mm down its walls
    // on a 3 mm budget is a mark too deep, said by name, with A clear.
    for rotated in [false, true] {
        let (world, mut h) = job(CLEAR, rotated);
        h.execute(&world, CreateOperation::new(&world, "N1", CUTTER, whole_b(), mark(false, TOP, 1.0)).expect("create"));
        h.execute(&world, GenerateOperation::new(&world, "N1").expect("generate"));
        let e = model::find_by_name(&world, "N1").expect("op");
        let tp = model::toolpath_of(&world, &e).expect("cached path");
        assert!(gouges(&tp.warnings) > 0, "rotated {rotated}: {:?}", tp.warnings);
        assert!(tp.path.notes.iter().any(|n| n.contains("deeper than a mark")), "{:?}", tp.path.notes);
    }
}

#[test]
fn a_pocket_on_bs_own_floor_is_unchanged_beside_a_neighbour() {
    // B's round pocket picked and cleared to its floor, with A 0.7 mm
    // off B's side: the pocket stands 5 mm inside B, nowhere near A, and
    // reads exactly as it does with A far away.
    // Path and findings alike: the neighbour adds no reading of B.
    let run = |gap: f64, rotated: bool| {
        let (world, mut h) = job(gap, rotated);
        let src = sel("part", top_face(&world, "part", FLOOR));
        let (warns, path) = generated_path(&world, &mut h, src, pocket(TOP, FLOOR));
        (world, warns, path)
    };
    for rotated in [false, true] {
        let (world, warns, path) = run(TIGHT, rotated);
        let blocking: Vec<_> = warns.iter().filter(|w| w.blocks_post()).collect();
        assert!(blocking.is_empty(), "rotated {rotated}: {blocking:?}");
        let (_, far, far_path) = run(CLEAR, rotated);
        assert_eq!(warns, far, "rotated {rotated}: the neighbour changes nothing on B's own pocket");
        assert_eq!(path.moves, far_path.moves, "rotated {rotated}: the same pocket");
        let low = path.moves.iter().map(|m| m.to.z).fold(f64::INFINITY, f64::min);
        assert!((low - FLOOR).abs() < 1e-9, "rotated {rotated}: cleared to B's floor: {low}");
        assert_posts(&world, &format!("pocket-{rotated}"));
    }
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

/// Both parts over MCP, `gap` apart: a `kind` on B with `params`;
/// verify's reply.
fn mcp_verify(tag: &str, gap: f64, kind: &str, params: Value) -> Value {
    let dir = std::env::temp_dir().join(format!("camcraft-neighbour-law-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("temp dir");
    let (b, a) = (dir.join("part.stl"), dir.join("other.stl"));
    pocket_support::write_stl(&part_b(), &b);
    pocket_support::write_stl(&part_a(gap), &a);
    let (mut s, mut id) = (Server::new(), 0);
    let _ = s.handle(&json!({ "jsonrpc": "2.0", "id": 0, "method": "initialize", "params": {} }));
    call(&mut s, &mut id, "import_model", json!({ "name": "part", "path": b.display().to_string() }));
    call(&mut s, &mut id, "import_model", json!({ "name": "other", "path": a.display().to_string() }));
    call(&mut s, &mut id, "set_stock", json!({ "box": { "dx": 80.0, "dy": 80.0, "dz": TOP } }));
    call(&mut s, &mut id, "add_tool", json!({ "catalog": CUTTER }));
    call(&mut s, &mut id, "add_operation", json!({ "kind": kind, "tool": CUTTER, "source": "part", "name": "N1", "params": params }));
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
        .map_or(0, |f| f.iter().filter(|x| x["kind"] == json!("gouge") && x["op"] == json!("N1") && x["blocks_post"] == json!(true)).count())
}

#[test]
fn over_mcp_a_cut_on_b_answers_for_its_neighbour_the_same() {
    for kind in ["Engrave", "Trace"] {
        let p = json!({ "top_z": TOP, "bottom_z": 7.7, "stepdown": 10.0 });
        let into = mcp_verify(&format!("into-{kind}"), TIGHT, kind, p.clone());
        assert!(op_gouges(&into) > 0, "{kind}: the operation's own gouge in A: {into}");
        assert_eq!(into["ok"], json!(false), "{kind}: {into}");
        let clear = mcp_verify(&format!("clear-{kind}"), CLEAR, kind, p);
        assert_eq!(op_gouges(&clear), 0, "{kind}: {clear}");
        assert_eq!(clear["ok"], json!(true), "{kind}: a legal mark on B verifies ok: {clear}");
    }
}
