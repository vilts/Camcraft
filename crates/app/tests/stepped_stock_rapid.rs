//! #1091: the rapid-in-stock law (#96, `RapidInStock`) caps the stock
//! column at the stock's highest point. On a stepped "from model" stock -
//! a casting solid to Z 20 over the hole's mouth with a boss to Z 30
//! elsewhere - a Bore's entry dive to Z 22 was convicted as if the boss
//! stood everywhere, and the post was blocked for a legal program. These
//! tests pin the law reading the stock's top under the dive
//! (`verify::acquit_by_stock_top`): the dive over the low part passes, and
//! the same dive with the boss under it - or the boss reaching into the
//! tool's footprint by less than its radius - is still convicted and
//! still blocks the post. Both a part-sourced and a stock-sourced Bore.
//! The casting stays in the tree, marked raw stock only (#1094); a
//! coarse casting's excess is read, and one whose source is deleted - its
//! chord unknown (#1056) - is never acquitted.
//!
//! Provenance: our own fixtures (`ladder_entry_height.rs`'s blind hole
//! under a stepped casting), specced from the Prime Directive and the
//! shop rule "never rapid into uncut stock"; no vendor material.
//!
//! Gate map (#1091): covers app::operation (generate's verify wiring,
//! the Bore arm, stock_top acquittal), sim::verify (rapids, stock_top),
//! app::posting (the gate).
//! Lane shape: all tests here run in seconds.

use camcraft_app::cl::MoveKind;
use camcraft_app::commands::{
    AddTool, CreateOperation, DeleteEntity, GenerateOperation, History, ImportModel, SetStock, SetStockSourceOnly,
};
use camcraft_app::drilling::Hole;
use camcraft_app::geo2d::Point2;
use camcraft_app::geometry::{Mesh, Point3, Triangle, cube};
use camcraft_app::model;
use camcraft_app::operation::{Height, OpParams, SrcRef};
use camcraft_app::posting;
use camcraft_app::schema::Location;
use camcraft_app::stock::StockShape;
use camcraft_app::tools::catalog;
use camcraft_app::verify::Warning;
use camcraft_app::world::new_world;

/// The part's top, the hole's mouth and the casting's low top: Z 20.
const MOUTH: f64 = 20.0;
/// The boss's top.
const BOSS: f64 = 30.0;
const FLOOR: f64 = 10.0;
const TOOL: &str = "EM-0800-4FL";
/// The 8 mm end mill's radius.
const R: f64 = 4.0;

fn quad(a: Point3, b: Point3, c: Point3, d: Point3, f: &mut Vec<Triangle>) {
    f.push(Triangle::new(a, b, c));
    f.push(Triangle::new(a, c, d));
}

/// An outward-wound casting: a 60 x 60 slab Z -10..20 with a boss over
/// `[x0, x1] x [y0, y1]` up to Z 30 (inside the slab's outline). Closed,
/// unless `open`: then the boss's two top facets are left out (PR #1098
/// review) - the walls still stand to Z 30, the shell has a hole.
fn casting(x0: f64, x1: f64, y0: f64, y1: f64, open: bool) -> Mesh {
    let slab = cube(Point3::new(-30.0, -30.0, -10.0), Point3::new(30.0, 30.0, MOUTH));
    let boss = cube(Point3::new(x0, y0, MOUTH), Point3::new(x1, y1, BOSS));
    let mut f: Vec<Triangle> = slab.facets[0..2].to_vec(); // bottom
    f.extend(slab.facets[4..12].iter().cloned()); // sides
    let o = [(-30.0, -30.0), (30.0, -30.0), (30.0, 30.0), (-30.0, 30.0)];
    let i = [(x0, y0), (x1, y0), (x1, y1), (x0, y1)];
    let p = |(x, y): (f64, f64)| Point3::new(x, y, MOUTH);
    for k in 0..4 {
        let n = (k + 1) % 4;
        quad(p(o[k]), p(o[n]), p(i[n]), p(i[k]), &mut f); // the low top, ringed round the boss
    }
    let from = if open { 4 } else { 2 };
    f.extend(boss.facets[from..12].iter().cloned()); // the boss's top and sides
    Mesh::from_facets(f)
}

fn flip(t: &Triangle) -> Triangle {
    Triangle::new(t.a, t.c, t.b)
}

/// `ladder_entry_height.rs`'s part: a 40 x 40 x [0, 20] block with a
/// 20 x 20 blind hole from its top to Z 10.
fn part() -> Mesh {
    let mut f = Vec::new();
    let outer = cube(Point3::new(-20.0, -20.0, 0.0), Point3::new(20.0, 20.0, MOUTH));
    f.extend(outer.facets[4..12].iter().cloned());
    let o = [(-20.0, -20.0), (20.0, -20.0), (20.0, 20.0), (-20.0, 20.0)];
    let i = [(-10.0, -10.0), (10.0, -10.0), (10.0, 10.0), (-10.0, 10.0)];
    let p = |(x, y): (f64, f64)| Point3::new(x, y, MOUTH);
    for k in 0..4 {
        let n = (k + 1) % 4;
        quad(p(o[k]), p(o[n]), p(i[n]), p(i[k]), &mut f);
    }
    let inner = cube(Point3::new(-10.0, -10.0, FLOOR), Point3::new(10.0, 10.0, MOUTH));
    f.extend(inner.facets[4..12].iter().map(flip));
    f.extend(outer.facets[0..2].iter().cloned());
    f.extend(inner.facets[0..2].iter().map(flip));
    Mesh::from_facets(f)
}

/// A milled bore of a 16 mm hole at the origin, mouth Z 20.
fn bore() -> OpParams {
    OpParams::Bore {
        links: camcraft_app::links::Links::PLANAR,
        clearance_z: Height::fixed(40.0),
        retract_z: None,
        top_z: Height::fixed(MOUTH),
        bottom_z: None,
        stock_to_leave: 0.0,
        stepdown: 5.0,
        feed: 500.0,
        plunge_feed: 150.0,
        climb: true,
        tolerance: 0.01,
        holes: vec![Hole { xy: Point2::new(0.0, 0.0), radius: 8.0, entry_z: MOUTH, floor_z: FLOOR, through: false, stepped: false }],
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Src {
    /// The Bore reads the part; the casting is only the stock.
    Part,
    /// The Bore reads the stock itself (the part stands beside it).
    Stock,
}

/// The Bore from `src` under the casting with its boss over `boss`: the
/// first vertical rapid down (index, XY, end Z), the warnings, the post.
fn run(src: Src, boss: [f64; 4]) -> ((usize, Point2, f64), Vec<Warning>, Result<(), String>) {
    run_on(src, boss, Casting::default())
}

/// How the casting stands in the document.
#[derive(Clone, Copy, Default)]
struct Casting {
    /// The boss's top facets left out (PR #1098 review).
    open: bool,
    /// Imported at this chord error, mm (a reduced-fidelity import, #864).
    coarse: Option<f64>,
    /// Deleted from the tree after the stock copies it (the stock's
    /// chord is then unknown, #1056); otherwise kept and marked raw
    /// stock only (#1094) so the verdict does not read it as a part.
    deleted: bool,
    /// After the delete, an unrelated EXACT model is imported under the
    /// casting's name (PR #1098 fourth review): the stock still holds
    /// the coarse casting's mesh.
    replaced: bool,
}

fn run_on(src: Src, boss: [f64; 4], c: Casting) -> ((usize, Point2, f64), Vec<Warning>, Result<(), String>) {
    let world = new_world();
    let mut h = History::default();
    let [x0, x1, y0, y1] = boss;
    h.execute(&world, ImportModel::from_mesh("casting", casting(x0, x1, y0, y1, c.open)));
    if c.coarse.is_some() {
        let e = model::find_by_name(&world, "casting").expect("casting");
        camcraft_app::model_fidelity::set(&world, &e, c.coarse);
    }
    // The finished part stands in the tree either way: the verdict judges
    // it, and a raw-stock-only casting needs a part beside it (#1094).
    h.execute(&world, ImportModel::from_mesh("part", part()));
    let source = match src {
        Src::Part => SrcRef::Entity("part".into()),
        Src::Stock => SrcRef::Stock,
    };
    let shape = StockShape::FromModel { source: "casting".into() };
    h.execute(&world, SetStock::new(&world, shape, Location::default()).expect("stock"));
    if c.deleted {
        h.execute(&world, DeleteEntity::new(&world, "casting").expect("delete casting"));
        if c.replaced {
            let other = cube(Point3::new(-30.0, -30.0, -10.0), Point3::new(30.0, 30.0, MOUTH));
            h.execute(&world, ImportModel::from_mesh("casting", other));
        }
    } else {
        h.execute(&world, SetStockSourceOnly::new(&world, true).expect("raw stock only"));
    }
    let tool = catalog().into_iter().find(|(n, _)| *n == TOOL).map(|(_, t)| t).expect("catalog");
    h.execute(&world, AddTool::new(&world, TOOL, tool).expect("tool"));
    h.execute(&world, CreateOperation::new(&world, "Op1", TOOL, source, bore()).expect("create op"));
    h.execute(&world, GenerateOperation::new(&world, "Op1").expect("generate op"));
    let e = model::find_by_name(&world, "Op1").expect("op");
    let cache = model::toolpath_of(&world, &e).unwrap_or_else(|| panic!("{src:?}: no toolpath"));
    let moves = &cache.path.moves;
    let entry = moves
        .iter()
        .position(|m| m.kind == MoveKind::Rapid && m.from.x == m.to.x && m.from.y == m.to.y && m.to.z < m.from.z)
        .and_then(|i| moves.get(i).map(|m| (i, Point2::new(m.to.x, m.to.y), m.to.z)))
        .unwrap_or_else(|| panic!("{src:?}: no entry rapid"));
    // One file per run: the tests run in parallel and repeat a boss.
    static RUN: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let n = RUN.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let out = std::env::temp_dir().join(format!("camcraft-1091-{src:?}-{n}-{}.nc", std::process::id()));
    let _ = std::fs::remove_file(&out);
    let posted = posting::post_setup(&world, &out).map(|_| ());
    let _ = std::fs::remove_file(&out);
    (entry, cache.warnings.clone(), posted)
}

/// The boss well clear of the hole: x 18..28 (the hole's rim is at 8).
const FAR: [f64; 4] = [18.0, 28.0, -5.0, 5.0];

fn convicted_and_blocked(src: Src, boss: [f64; 4], what: &str) {
    let ((at, _, z), warns, posted) = run(src, boss);
    assert!(z < BOSS, "{src:?} {what}: the fixture must dive under the boss's top, not to Z {z}");
    assert!(warns.contains(&Warning::RapidInStock { at }), "{src:?} {what}: the dive went unnamed: {warns:?}");
    match posted {
        Ok(()) => panic!("{src:?} {what}: a program diving into the boss posted"),
        Err(e) => assert!(
            e.contains("post blocked") && e.contains(&format!("rapid move {at} ")),
            "{src:?} {what}: refused, but not by the rapid law: {e}"
        ),
    }
}

#[test]
fn a_dive_over_the_low_part_of_a_stepped_stock_passes_and_posts() {
    // Solid to Z 20 over the mouth, the boss to Z 30 well away: the
    // entry rapid to Z 22 ends 2 mm over the stock really under it.
    for src in [Src::Part, Src::Stock] {
        let ((_, _, z), warns, posted) = run(src, FAR);
        assert!(z > MOUTH && z < BOSS, "{src:?}: the fixture must dive between the two tops, not to Z {z}");
        assert!(!warns.iter().any(|w| matches!(w, Warning::RapidInStock { .. })), "{src:?}: {warns:?}");
        assert!(posted.is_ok(), "{src:?}: {posted:?}");
    }
}

#[test]
fn the_same_dive_with_the_boss_under_it_is_convicted() {
    // The boss stands over the whole hole.
    for src in [Src::Part, Src::Stock] {
        convicted_and_blocked(src, [-12.0, 12.0, -12.0, 12.0], "boss over the hole");
    }
}

#[test]
fn a_boss_reaching_into_the_tool_footprint_is_convicted() {
    // The boss's edge stands 1.5 mm beside the dive's centre line, under
    // the tool's rim (radius 4) but not under its centre.
    for src in [Src::Part, Src::Stock] {
        let ((_, c, _), _, _) = run(src, FAR);
        let edge = c.x + R - 2.5;
        convicted_and_blocked(src, [edge, edge + 10.0, c.y - 5.0, c.y + 5.0], "boss under the rim");
    }
}

#[test]
fn an_open_stock_mesh_proves_nothing_and_the_dive_stays_convicted() {
    // PR #1098 review: the boss over the hole without its top facets. No
    // facet stands over the dive, yet the boss's walls do - the re-reading
    // must not take a hole in the shell for air.
    for src in [Src::Part, Src::Stock] {
        let open = Casting { open: true, ..Casting::default() };
        let ((at, _, z), warns, posted) = run_on(src, [-12.0, 12.0, -12.0, 12.0], open);
        assert!(z < BOSS, "{src:?}: the fixture must dive under the boss's walls, not to Z {z}");
        assert!(warns.contains(&Warning::RapidInStock { at }), "{src:?}: the dive into the open boss went unnamed: {warns:?}");
        match posted {
            Ok(()) => panic!("{src:?}: a program diving into the open boss posted"),
            Err(e) => assert!(e.contains("post blocked"), "{src:?}: {e}"),
        }
    }
}

#[test]
fn a_coarse_stock_whose_source_is_gone_is_never_acquitted() {
    // PR #1098 third review: a casting imported at chord 2.5 mm, then
    // deleted - the stock copy no longer knows its chord (#1056). The dive
    // ends 2 mm over the mesh, and the real surface may stand 2.5 mm above
    // it: the flat cap's conviction stands, and the post is blocked by it.
    for src in [Src::Part, Src::Stock] {
        let gone = Casting { coarse: Some(2.5), deleted: true, ..Casting::default() };
        let ((at, _, z), warns, posted) = run_on(src, FAR, gone);
        assert!(z > MOUTH && z < BOSS, "{src:?}: Z {z}");
        assert!(warns.contains(&Warning::RapidInStock { at }), "{src:?}: acquitted on an unknown chord: {warns:?}");
        match posted {
            Ok(()) => panic!("{src:?}: a dive into a coarse stock of unknown chord posted"),
            Err(e) => assert!(e.contains(&format!("rapid move {at} ")), "{src:?}: not blocked by the rapid law: {e}"),
        }
        // The same casting exact and deleted is unknown too: kept.
        let exact_gone = Casting { deleted: true, ..Casting::default() };
        let ((at, _, _), warns, _) = run_on(src, FAR, exact_gone);
        assert!(warns.contains(&Warning::RapidInStock { at }), "{src:?}: {warns:?}");
    }
}

#[test]
fn a_coarse_stock_whose_source_stands_reads_its_excess() {
    // Source kept: the chord is known. 2.5 mm of chord against 2 mm of
    // clearance - convicted; 1 mm of chord - clear (the fidelity gate
    // may still refuse the post on its own grounds).
    for src in [Src::Part, Src::Stock] {
        let ((at, _, _), warns, posted) = run_on(src, FAR, Casting { coarse: Some(2.5), ..Casting::default() });
        assert!(warns.contains(&Warning::RapidInStock { at }), "{src:?}: {warns:?}");
        assert!(posted.is_err(), "{src:?}: posted");
        let (_, warns, _) = run_on(src, FAR, Casting { coarse: Some(1.0), ..Casting::default() });
        assert!(!warns.iter().any(|w| matches!(w, Warning::RapidInStock { .. })), "{src:?}: {warns:?}");
    }
}

#[test]
fn an_exact_model_imported_under_the_old_name_proves_nothing() {
    // PR #1098 fourth review: the coarse casting deleted, an exact model
    // imported as "casting". The name resolves, but not to the mesh the
    // stock holds - the chord stays unknown and the dive stays convicted.
    for src in [Src::Part, Src::Stock] {
        let swapped = Casting { coarse: Some(2.5), deleted: true, replaced: true, ..Casting::default() };
        let ((at, _, z), warns, posted) = run_on(src, FAR, swapped);
        assert!(z > MOUTH && z < BOSS, "{src:?}: Z {z}");
        assert!(warns.contains(&Warning::RapidInStock { at }), "{src:?}: acquitted against a stranger's chord: {warns:?}");
        assert!(posted.is_err(), "{src:?}: posted");
    }
}

/// The casting as stock with its source kept: the world, to ask the
/// guard directly after an edit.
fn stocked(coarse: Option<f64>) -> (flecs_ecs::prelude::World, History) {
    let world = new_world();
    let mut h = History::default();
    h.execute(&world, ImportModel::from_mesh("casting", casting(18.0, 28.0, -5.0, 5.0, false)));
    if coarse.is_some() {
        let e = model::find_by_name(&world, "casting").expect("casting");
        camcraft_app::model_fidelity::set(&world, &e, coarse);
    }
    let shape = StockShape::FromModel { source: "casting".into() };
    h.execute(&world, SetStock::new(&world, shape, Location::default()).expect("stock"));
    (world, h)
}

#[test]
fn the_guard_follows_the_mesh_not_the_name() {
    use camcraft_app::commands::{MoveModel, UpdateModel};
    use camcraft_app::model_fidelity::stock_fidelity_known as known;
    // Copied and untouched: known.
    let (world, _h) = stocked(Some(2.5));
    assert!(known(&world, None));
    // Moved: the stock moves with it, the mesh and its record are the
    // same - still known.
    let (world, mut h) = stocked(Some(2.5));
    let to = Location { pos: Point3::new(5.0, 0.0, 0.0), ..Location::default() };
    h.execute(&world, MoveModel::new(&world, "casting", to).expect("move"));
    assert!(known(&world, None), "a moved source carries the same mesh");
    // Re-imported in place: the copy and the record are traded together.
    let (world, mut h) = stocked(Some(2.5));
    let fresh = casting(18.0, 28.0, -5.0, 6.0, false);
    h.execute(&world, UpdateModel::new(&world, "casting", fresh).expect("update"));
    assert!(known(&world, None), "a re-import in place updates the copy with its source");
    // Re-meshed behind the copy's back (no command does this; a raw
    // write stands in for any path that would): not known.
    let (world, _h) = stocked(None);
    let e = model::find_by_name(&world, "casting").expect("casting");
    let other = cube(Point3::new(-30.0, -30.0, -10.0), Point3::new(30.0, 30.0, MOUTH)).facets;
    e.entity_view(&world).set(camcraft_app::schema::Facets { tris: other });
    assert!(!known(&world, None), "a source re-meshed without its copy is not the copy's source");
}
