//! #1595, review of PR #1615: a mark is bounded by the part under the
//! cutter, and only the cutting edge gets the mark budget.
//!
//! - A plate thinner than the budget: Top 2, Bottom -0.9 with a 6 mm
//!   end mill states a 2.9 mm "mark" on a 2 mm plate. Lowered by the
//!   budget the plate dropped clear of the cutter and the sim's credit
//!   floor lay under it, so a cut THROUGH the finished part posted on
//!   both stocks with no finding. The edge may not come within
//!   `MARK_FLOOR` of the part's underside (`operation::mark_law`,
//!   `verify::seat_skin`), and the sim credits a mark no deeper
//!   (`stock_sim::credit_marks`): a through-cut is a blocking
//!   `Warning::Gouge` said by name, a legal shallow mark still posts.
//! - The shank and the holder are judged against the part as it stands
//!   (`verify::stack_hits`), not lowered by the budget.
//! - The retract out of a mark's kerf is judged too (second review):
//!   only the pose the edge already stands in is excused, so a part
//!   overhang the lift carries the holder into is a `RapidInStock` on
//!   that lift.
//!
//! Provenance: our own fixtures (plain boxes) over the solid law and
//! the column sim; no vendor material consulted.
//!
//! #1607: the same floor holds on a Stock-source Engrave or Trace.
//!
//! Gate map: covers app::operation::mark_law, app::operation::generate
//! (the Mark arm), app::stock_sim (marks, credit_marks), app::posting,
//! app::posting_sim.

use flecs_ecs::prelude::World;

use camcraft_app::commands::{AddTool, CreateOperation, GenerateOperation, History, ImportModel, SetStock};
use camcraft_app::geometry::{cube, quat_from_yaw, Mesh, Point3};
use camcraft_app::model;
use camcraft_app::operation::{Height, OpParams, SrcRef};
use camcraft_app::posting;
use camcraft_app::schema::Location;
use camcraft_app::stock::StockShape;
use camcraft_app::stock_sim::{self, MARK_FLOOR};
use camcraft_app::tools::catalog;
use camcraft_app::verify::Warning;
use camcraft_app::world::new_world;

/// A 6 mm end mill: the budget's cap is its 3 mm radius.
const TOOL: &str = "EM-0600-4FL";
/// The same cutter with only 2 mm of flute under its shank.
const STUB: &str = "EM-0600-STUB";

/// A 40 x 40 x `thick` plate as "part" (Z 0..thick) in a stock of its
/// own size: axis-aligned, or turned a quarter about Z (the same
/// footprint, but a rotation the program sim refuses to model).
fn job(thick: f64, rotated: bool) -> (World, History) {
    job_of(cube(Point3::new(-20.0, -20.0, 0.0), Point3::new(20.0, 20.0, thick)), thick, rotated)
}

/// [`job`] with `part` as the part: its plate is Z 0..`thick`, and the
/// stock is that plate's box.
fn job_of(part: Mesh, thick: f64, rotated: bool) -> (World, History) {
    let world = new_world();
    let mut h = History::default();
    let tool = catalog().into_iter().find(|(n, _)| *n == TOOL).map(|(_, t)| t).expect("tool");
    let stub = camcraft_app::tools::Tool { flute_len: 2.0, ..tool.clone() };
    assert!(tool.flute_len > 3.0 && stub.shank_d >= tool.diameter, "the fixture's tool: {} flute, {} shank", tool.flute_len, stub.shank_d);
    h.execute(&world, AddTool::new(&world, TOOL, tool).expect("tool"));
    h.execute(&world, AddTool::new(&world, STUB, stub).expect("stub tool"));
    h.execute(&world, ImportModel::from_mesh("part", part));
    let rot = if rotated { quat_from_yaw(std::f64::consts::FRAC_PI_2) } else { Location::default().rot };
    let at = Location { pos: Point3::new(0.0, 0.0, 0.0), rot };
    h.execute(&world, SetStock::new(&world, StockShape::Box { dx: 40.0, dy: 40.0, dz: thick }, at).expect("stock"));
    let b = model::stock_world_aabb_for(&world, None).expect("stock box");
    let on = |v: f64, want: f64| (v - want).abs() < 1e-9;
    assert!(
        on(b.min.x, -20.0) && on(b.min.y, -20.0) && on(b.min.z, 0.0) && on(b.max.x, 20.0) && on(b.max.y, 20.0) && on(b.max.z, thick),
        "stock at {:?}..{:?}",
        b.min,
        b.max
    );
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
    cut_under(kind, top, bottom, 25.0)
}

/// [`cut`] with its clearance plane at `clearance`.
fn cut_under(kind: Kind, top: f64, bottom: f64, clearance: f64) -> OpParams {
    let (clearance_z, top_z, bottom_z) = (Height::fixed(clearance), Height::fixed(top), Height::fixed(bottom));
    match kind {
        Kind::Engrave => OpParams::Engrave {
            clearance_z, retract_z: None, top_z, bottom_z, stepdown: 10.0, feed: 800.0, plunge_feed: 200.0, climb: true, tilt: None,
        },
        Kind::Trace => OpParams::Trace {
            clearance_z, retract_z: None, top_z, bottom_z, stepdown: 10.0, feed: 800.0, plunge_feed: 200.0, climb: true, tilt: None,
        },
    }
}

/// Create and generate "E1" on the whole model with `tool`; its cached
/// warnings, notes and deepest move.
fn generated(world: &World, h: &mut History, tool: &str, params: OpParams) -> (Vec<Warning>, Vec<String>, f64) {
    generated_on(world, h, tool, SrcRef::Entity("part".into()), params)
}

/// [`generated`] with the source given.
fn generated_on(world: &World, h: &mut History, tool: &str, src: SrcRef, params: OpParams) -> (Vec<Warning>, Vec<String>, f64) {
    h.execute(world, CreateOperation::new(world, "E1", tool, src, params).expect("create"));
    h.execute(world, GenerateOperation::new(world, "E1").expect("generate"));
    let e = model::find_by_name(world, "E1").expect("op");
    let tp = model::toolpath_of(world, &e).expect("cached path");
    assert!(!tp.path.moves.is_empty(), "the line was generated");
    let low = tp.path.moves.iter().map(|m| m.to.z).fold(f64::INFINITY, f64::min);
    (tp.warnings.clone(), tp.path.notes.clone(), low)
}

fn gouges(w: &[Warning]) -> usize {
    w.iter().filter(|w| matches!(w, Warning::Gouge { .. })).count()
}

fn nc(tag: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!("camcraft-mark-floor-{tag}-{}.nc", std::process::id()))
}

/// Whether the setup posts; a refusal's text.
fn post(world: &World, tag: &str) -> Result<(), String> {
    let out = nc(tag);
    let _ = std::fs::remove_file(&out);
    let r = posting::post_setup(world, &out).map(|_| ());
    assert_eq!(out.exists(), r.is_ok(), "{tag}: a blocked program is never written");
    let _ = std::fs::remove_file(&out);
    r
}

#[test]
fn a_cut_through_a_thin_plate_gouges_and_never_posts_on_either_stock() {
    // The review's repro: a 2.9 mm "mark" on a 2 mm plate, the cutter
    // 0.9 mm under the plate's underside along every edge. Before the
    // fix: no finding on the op, and it POSTED on both stocks.
    for kind in KINDS {
        for rotated in [false, true] {
            let (world, mut h) = job(2.0, rotated);
            let (warns, notes, low) = generated(&world, &mut h, TOOL, cut(kind, 2.0, -0.9));
            assert!((low + 0.9).abs() < 1e-9, "{kind:?}: the cut runs to Z-0.9: {low}");
            assert!(gouges(&warns) > 0, "{kind:?} rotated {rotated}: convicted on the op: {warns:?}");
            assert!(
                notes.iter().any(|n| n.contains("cuts through the part") && n.contains("0.200 mm")),
                "{kind:?} rotated {rotated}: the breakthrough is said by name: {notes:?}"
            );
            let err = post(&world, &format!("through-{kind:?}-{rotated}")).expect_err("a through-cut blocks the post");
            assert!(err.contains("post blocked"), "{kind:?} rotated {rotated}: {err}");
            // Turned, the op's own gouge is the only judge.
            if rotated {
                assert!(err.contains("E1: move") && err.contains("cuts past the finish surface"), "{kind:?}: {err}");
            }
        }
    }
}

#[test]
fn a_mark_that_leaves_the_floor_posts_on_both_stocks_and_one_that_does_not_is_refused() {
    for kind in KINDS {
        for rotated in [false, true] {
            // 0.3 mm deep, and 1.8 mm deep (exactly the floor is left).
            for bottom in [1.7, MARK_FLOOR] {
                let (world, mut h) = job(2.0, rotated);
                let (warns, notes, _) = generated(&world, &mut h, TOOL, cut(kind, 2.0, bottom));
                let blocking: Vec<_> = warns.iter().filter(|w| w.blocks_post()).collect();
                assert!(blocking.is_empty(), "{kind:?} rotated {rotated} to Z{bottom}: {blocking:?}");
                assert!(!notes.iter().any(|n| n.contains("cuts through") || n.contains("deeper than a mark")), "{notes:?}");
                post(&world, &format!("legal-{kind:?}-{rotated}")).unwrap_or_else(|e| panic!("{kind:?} rotated {rotated} to Z{bottom}: a legal mark posts: {e}"));
            }
            // 1.9 mm deep leaves 0.1 mm of plate: inside the 2.9 mm
            // budget, and a breakthrough all the same.
            let (world, mut h) = job(2.0, rotated);
            let (warns, notes, _) = generated(&world, &mut h, TOOL, cut(kind, 2.0, 0.1));
            assert!(gouges(&warns) > 0, "{kind:?} rotated {rotated}: {warns:?}");
            assert!(notes.iter().any(|n| n.contains("cuts through the part")), "{notes:?}");
            assert!(!notes.iter().any(|n| n.contains("deeper than a mark")), "within the budget, so only the floor speaks: {notes:?}");
            assert!(post(&world, &format!("web-{kind:?}-{rotated}")).is_err(), "{kind:?} rotated {rotated}: 0.1 mm of plate left posted");
        }
    }
}

#[test]
fn the_program_sim_never_credits_a_cut_through_the_plate() {
    // Aligned stock, where the program sim judges on its own lattice.
    let sim = |bottom: f64| {
        let (world, mut h) = job(2.0, false);
        generated(&world, &mut h, TOOL, cut(Kind::Engrave, 2.0, bottom));
        stock_sim::simulate_document(&world).expect("an aligned stock is judged").dev
    };
    let legal = sim(1.7);
    assert_eq!(legal.gouge_cells, 0, "worst {} at {:?}", legal.max_gouge, legal.max_gouge_at);
    let floor = sim(MARK_FLOOR);
    assert_eq!(floor.gouge_cells, 0, "down to the floor is a mark: worst {}", floor.max_gouge);
    // 0.1 mm of plate left: charged what is missing from the floor.
    let web = sim(0.1);
    assert!(web.gouge_cells > 0, "0.1 mm of plate left was credited as a mark");
    assert!((web.max_gouge - (MARK_FLOOR - 0.1)).abs() < 1e-6, "charged {}", web.max_gouge);
    // Through: every column the cutter took stays convicted.
    let through = sim(-0.9);
    assert!(through.gouge_cells > web.gouge_cells / 2 && through.max_gouge >= MARK_FLOOR - 1e-6, "{} cells, worst {}", through.gouge_cells, through.max_gouge);
}

#[test]
fn a_shank_in_the_kerf_is_judged_against_the_part_as_it_stands() {
    // An 8 mm block, 2.5 mm deep with 2 mm of flute: the edge is within
    // its 2.5 mm mark, the shank stands 0.5 mm inside the part. Judged
    // against the part lowered by the budget it was excused.
    for kind in KINDS {
        for rotated in [false, true] {
            let (world, mut h) = job(8.0, rotated);
            let (warns, _, _) = generated(&world, &mut h, STUB, cut(kind, 8.0, 5.5));
            assert_eq!(gouges(&warns), 0, "{kind:?} rotated {rotated}: the edge is within its mark: {warns:?}");
            assert!(
                warns.iter().any(|w| matches!(w, Warning::ShankHit { part: true, .. })),
                "{kind:?} rotated {rotated}: the shank is convicted on the part: {warns:?}"
            );
            assert!(post(&world, &format!("shank-{kind:?}-{rotated}")).is_err(), "{kind:?} rotated {rotated}: a rubbing shank posted");
            // Within its flute the same tool marks clean and posts.
            let (world, mut h) = job(8.0, rotated);
            let (warns, _, _) = generated(&world, &mut h, STUB, cut(kind, 8.0, 6.5));
            let blocking: Vec<_> = warns.iter().filter(|w| w.blocks_post()).collect();
            assert!(blocking.is_empty(), "{kind:?} rotated {rotated}: {blocking:?}");
            post(&world, &format!("flute-{kind:?}-{rotated}")).unwrap_or_else(|e| panic!("{kind:?} rotated {rotated}: {e}"));
        }
    }
}

#[test]
fn a_stock_source_cut_through_the_plate_meets_the_same_floor() {
    // #1607 over this law: a Stock-source Engrave or Trace answers to
    // the mark law against the parts in the stock, the floor included.
    // The review's 2.9 mm "mark" on the 2 mm plate, and 1.9 mm (inside
    // the budget, 0.1 mm of plate left), are refused by name on the op on
    // both stocks; a 0.3 mm mark off the stock still posts.
    for kind in KINDS {
        for rotated in [false, true] {
            for (bottom, tag) in [(-0.9, "through"), (0.1, "web")] {
                let (world, mut h) = job(2.0, rotated);
                let (warns, notes, _) = generated_on(&world, &mut h, TOOL, SrcRef::Stock, cut(kind, 2.0, bottom));
                assert!(gouges(&warns) > 0, "{kind:?} rotated {rotated} to Z{bottom}: convicted on the op: {warns:?}");
                assert!(notes.iter().any(|n| n.contains("cuts through the part")), "{kind:?} rotated {rotated}: {notes:?}");
                let err = post(&world, &format!("stock-{tag}-{kind:?}-{rotated}")).expect_err("a through-cut off the stock blocks the post");
                if rotated {
                    assert!(err.contains("E1: move") && err.contains("cuts past the finish surface"), "{kind:?}: {err}");
                }
            }
            let (world, mut h) = job(2.0, rotated);
            let (warns, _, _) = generated_on(&world, &mut h, TOOL, SrcRef::Stock, cut(kind, 2.0, 1.7));
            let blocking: Vec<_> = warns.iter().filter(|w| w.blocks_post()).collect();
            assert!(blocking.is_empty(), "{kind:?} rotated {rotated}: {blocking:?}");
            post(&world, &format!("stock-legal-{kind:?}-{rotated}")).unwrap_or_else(|e| panic!("{kind:?} rotated {rotated}: {e}"));
        }
    }
}

/// The 8 mm plate with a slab over all of it at Z 120..125: an overhang
/// of the part above every point of the line, and above the holder's
/// top while the line is cut.
fn roofed() -> Mesh {
    let mut f = cube(Point3::new(-20.0, -20.0, 0.0), Point3::new(20.0, 20.0, 8.0)).facets.to_vec();
    f.extend(cube(Point3::new(-20.0, -20.0, 120.0), Point3::new(20.0, 20.0, 125.0)).facets.iter().copied());
    Mesh { facets: f.into() }
}

#[test]
fn a_retract_into_an_overhang_of_the_part_is_convicted_on_the_lift_and_never_posts() {
    // 0.3 mm deep on the 8 mm plate, clearance at Z 150 over the slab.
    // The holder's top stands under Z 100 on every cutting move, so
    // only a rapid can reach the slab: the lift out of the kerf carries
    // the holder through it. Before the fix that lift was excused whole
    // (it starts where a cutting move ended) and no law judged it.
    let reach = catalog().into_iter().find(|(n, _)| *n == TOOL).map(|(_, t)| t.flute_len + t.shank_len + t.holder_len).expect("tool");
    assert!(reach > 0.0 && 8.0 + reach < 120.0, "the slab stands over the holder's top: reach {reach}");
    for kind in KINDS {
        for rotated in [false, true] {
            let (world, mut h) = job_of(roofed(), 8.0, rotated);
            let (warns, _, _) = generated(&world, &mut h, TOOL, cut_under(kind, 8.0, 7.7, 150.0));
            let e = model::find_by_name(&world, "E1").expect("op");
            let moves = model::toolpath_of(&world, &e).expect("cached path").path.moves.clone();
            // The retract: a rapid straight up from where a cut ended.
            let lifts: Vec<usize> = (1..moves.len())
                .filter(|&i| {
                    let (c, m) = (&moves[i - 1], &moves[i]);
                    let rapid = |k| k == camcraft_app::cl::MoveKind::Rapid;
                    rapid(m.kind) && !rapid(c.kind) && c.to == m.from && m.from.x == m.to.x && m.from.y == m.to.y && m.to.z > 125.0
                })
                .collect();
            assert!(!lifts.is_empty(), "{kind:?} rotated {rotated}: the line ends in a retract through the slab");
            assert_eq!(gouges(&warns), 0, "{kind:?} rotated {rotated}: the mark itself is legal: {warns:?}");
            for at in lifts {
                assert!(
                    warns.contains(&Warning::RapidInStock { at }),
                    "{kind:?} rotated {rotated}: the retract (move {at}) into the overhang is not convicted: {warns:?}"
                );
            }
            assert!(post(&world, &format!("roof-{kind:?}-{rotated}")).is_err(), "{kind:?} rotated {rotated}: a retract into the part posted");
            // The same cut and clearance with no slab: clean, and posts.
            let (world, mut h) = job(8.0, rotated);
            let (warns, _, _) = generated(&world, &mut h, TOOL, cut_under(kind, 8.0, 7.7, 150.0));
            let blocking: Vec<_> = warns.iter().filter(|w| w.blocks_post()).collect();
            assert!(blocking.is_empty(), "{kind:?} rotated {rotated}: a plain retract is clean: {blocking:?}");
            post(&world, &format!("plain-{kind:?}-{rotated}")).unwrap_or_else(|e| panic!("{kind:?} rotated {rotated}: a plain retract posts: {e}"));
        }
    }
}
