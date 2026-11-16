//! The silent-gouge fix (Prime Directive): a 2D op whose source is a
//! finished part answers to the part SOLID at every level it cuts, not
//! to one plane's flat section. The Contour generator walks each ring
//! of a whole-model section only down to the floor of the wall it trims
//! (a blind hole's ring ends at its floor plus the stock to leave); the
//! verifier judges Contour, Face and the part-picked pocket family
//! against the part's section at each move's plane, and drill / bore /
//! thread mill by containment - so a path cut below a feature's floor is
//! a Gouge warning, and a warned path never posts.
//!
//! Gate map (silent-gouge fix): covers app::operation (generate, job),
//! motion::machining2d (contour_on_part), sim::verify (model law,
//! solid_hits), app::posting, app::commands.

use camcraft_app::cl::{MoveKind, Toolpath};
use camcraft_app::commands::{AddTool, CreateOperation, GenerateOperation, History, ImportModel, SetStock};
use camcraft_app::drilling::{CycleChoice, Hole};
use camcraft_app::geo2d::{Point2, Region};
use camcraft_app::geometry::{Mesh, Point3, Triangle, cube};
use camcraft_app::machining2d::{ContourParams, Lead, Side, contour, contour_on_part};
use camcraft_app::machining3d::Entry3D;
use camcraft_app::model;
use camcraft_app::operation::{Height, OpParams, SrcRef};
use camcraft_app::posting;
use camcraft_app::schema::Location;
use camcraft_app::stock::StockShape;
use camcraft_app::tools::catalog;
use camcraft_app::verify::{Mode, ToolGeom, VerifyContext, Warning, verify};
use camcraft_app::world::new_world;
use flecs_ecs::prelude::World;

/// Half the hole's side (the hole is 20 x 20, the block 40 x 40 x 20).
const H: f64 = 10.0;

fn flip(t: &Triangle) -> Triangle {
    Triangle::new(t.a, t.c, t.b)
}

/// A 40 x 40 x [0, 20] block with a 20 x 20 square hole from the top:
/// blind to `floor`, or through when `floor` is `None`. Outward-wound.
fn block_with_hole(floor: Option<f64>) -> Mesh {
    let quad = |a: Point3, b: Point3, c: Point3, d: Point3, f: &mut Vec<Triangle>| {
        f.push(Triangle::new(a, b, c));
        f.push(Triangle::new(a, c, d));
    };
    let mut f = Vec::new();
    let outer = cube(Point3::new(-20.0, -20.0, 0.0), Point3::new(20.0, 20.0, 20.0));
    f.extend(outer.facets[4..12].iter().cloned()); // block walls
    let ring = |z: f64, up: bool, f: &mut Vec<Triangle>| {
        let o = [(-20.0, -20.0), (20.0, -20.0), (20.0, 20.0), (-20.0, 20.0)];
        let i = [(-H, -H), (H, -H), (H, H), (-H, H)];
        for k in 0..4 {
            let n = (k + 1) % 4;
            let p = |(x, y): (f64, f64)| Point3::new(x, y, z);
            if up {
                quad(p(o[k]), p(o[n]), p(i[n]), p(i[k]), f);
            } else {
                quad(p(o[k]), p(i[k]), p(i[n]), p(o[n]), f);
            }
        }
    };
    ring(20.0, true, &mut f);
    let zf = floor.unwrap_or(0.0);
    let inner = cube(Point3::new(-H, -H, zf), Point3::new(H, H, 20.0));
    // The hole walls face the axis: the inner box's walls, flipped.
    f.extend(inner.facets[4..12].iter().map(flip));
    match floor {
        Some(_) => {
            f.extend(outer.facets[0..2].iter().cloned()); // full bottom
            f.extend(inner.facets[0..2].iter().map(flip)); // floor, +z
        }
        None => ring(0.0, false, &mut f),
    }
    Mesh::from_facets(f)
}

fn contour_op(stepdown: f64, leave: f64) -> OpParams {
    OpParams::Contour {
        stock_to_leave: leave,
        clearance_z: Height::fixed(30.0),
        retract_z: Height::fixed(25.0),
        top_z: Height::fixed(20.0),
        bottom_z: Height::fixed(-2.0),
        stepdown,
        feed: 1000.0,
        plunge_feed: 300.0,
        climb: true,
        side: Side::Auto,
        lead: Lead::None,
        tolerance: 0.01,
        tab_count: 0,
        tab_width: 5.0,
        tab_height: 1.0,
        entry: Entry3D::Plunge,
        helix_radius: 1.0,
        blend_turns: false,
        tilt: None,
        links: camcraft_app::links::Links::PLANAR,
    }
}

fn world_with(mesh: Mesh, tool: &str) -> (World, History) {
    let world = new_world();
    let mut h = History::default();
    h.execute(&world, ImportModel::from_mesh("part", mesh));
    let loc = Location { pos: Point3::new(0.0, 0.0, -2.0), ..Default::default() };
    let stock = SetStock::new(&world, StockShape::Box { dx: 60.0, dy: 60.0, dz: 22.0 }, loc);
    h.execute(&world, stock.expect("stock"));
    let mut t = catalog().into_iter().find(|(n, _)| *n == tool).map(|(_, t)| t).expect("catalog");
    if tool == "DR-0400" {
        // A long-series flute: these holes run 14-15 mm from the z20
        // stock top, past the jobber's 12 mm - issue #84's reach law
        // would refuse them before the law under test speaks.
        t.flute_len = 30.0;
    }
    h.execute(&world, AddTool::new(&world, tool, t).expect("tool"));
    (world, h)
}

/// Create + generate `params` on the part; the cached path and warnings.
fn run(world: &World, h: &mut History, name: &str, tool: &str, params: OpParams) -> (Toolpath, Vec<Warning>) {
    let op = CreateOperation::new(world, name, tool, SrcRef::Entity("part".into()), params);
    h.execute(world, op.expect("create"));
    h.execute(world, GenerateOperation::new(world, name).expect("generate"));
    let e = model::find_by_name(world, name).expect("op");
    let tp = model::toolpath_of(world, &e).expect("cached path");
    (tp.path.clone(), tp.warnings.clone())
}

/// The planar feed levels of the moves standing in `keep`'s XY.
fn levels_where(t: &Toolpath, keep: impl Fn(f64, f64) -> bool) -> Vec<f64> {
    let mut v: Vec<f64> = t
        .moves
        .iter()
        .filter(|m| m.kind == MoveKind::Feed && m.from.z == m.to.z && keep(m.to.x, m.to.y))
        .map(|m| m.to.z)
        .collect();
    v.dedup();
    v
}

fn in_hole(x: f64, y: f64) -> bool {
    x.abs() < H && y.abs() < H
}

fn outside_block(x: f64, y: f64) -> bool {
    x.abs() > 20.0 || y.abs() > 20.0
}

fn gouges(w: &[Warning]) -> usize {
    w.iter().filter(|w| matches!(w, Warning::Gouge { .. })).count()
}

fn em8() -> ToolGeom {
    ToolGeom {
        radius: 4.0,
        nose_radius: 0.0,
        flute_len: 30.0,
        shank_radius: 4.0,
        shank_len: 40.0,
        holder_radius: 20.0,
        holder_len: 60.0,
    }
}

fn direct_params(stepdown: f64, leave: f64) -> ContourParams {
    ContourParams {
        tool_diameter: 8.0,
        stock_to_leave: leave,
        clearance_z: 30.0,
        retract_z: 25.0,
        top_z: 20.0,
        bottom_z: -2.0,
        stepdown,
        feed: 1000.0,
        plunge_feed: 300.0,
        climb: true,
        side: Side::Outside,
        lead: Lead::None,
        tolerance: 0.01,
        tab_count: 0,
        tab_width: 5.0,
        tab_height: 1.0,
        entry: Entry3D::Plunge,
        helix_radius: 1.0,
        blend_turns: false,
        links: camcraft_app::links::Links::PLANAR,
    }
}

/// The section the app takes off a whole part at Top (Z 20, a micron
/// under it - the slice-retry law).
fn top_section(m: &Mesh) -> Region {
    camcraft_app::geometry::slice::slice(&m.facets, 20.0 - 1e-3).expect("section")
}

fn depth_verify(t: &Toolpath, region: &Region, model: Option<&[Triangle]>, leave: f64) -> Vec<Warning> {
    let tool = em8();
    verify(
        t,
        &VerifyContext {
            region,
            stock: None,
            model,
            tool: &tool,
            cutter: None,
            mode: Mode::Boundary,
            stock_top_z: 20.0,
            clearance_z: 30.0,
            stock_to_leave: leave,
            gouge_tol: 0.01,
            z_tol: 1e-6,
            finish_tol: None,
            fixtures: &[],
            spindle: None,
            ..Default::default()
        },
    )
}

#[test]
fn the_old_flat_ladder_gouges_under_a_blind_floor_and_the_depth_verify_says_so() {
    let m = block_with_hole(Some(10.0));
    let region = top_section(&m);
    // The pre-fix generator: the top plane's rings at every level.
    let old = contour(&region, &direct_params(4.0, 0.0)).expect("contour");
    assert!(levels_where(&old, in_hole).contains(&-2.0), "the old ladder walks the hole ring to the bottom");
    // The flat-region verdict was blind to it - the silent gouge.
    assert_eq!(gouges(&depth_verify(&old, &region, None, 0.0)), 0);
    // The depth-aware verdict convicts every move below the floor.
    let w = depth_verify(&old, &region, Some(&m.facets), 0.0);
    assert!(gouges(&w) > 0, "below the floor is a gouge");
    for wn in &w {
        if let Warning::Gouge { at } = wn {
            assert!(old.moves[*at].to.z < 10.0, "only moves under the floor are convicted: {:?}", old.moves[*at]);
        }
    }
    // The fixed generator, same params: silent under the same law.
    let new = contour_on_part(&region, &m.facets, &direct_params(4.0, 0.0)).expect("contour");
    assert_eq!(depth_verify(&new, &region, Some(&m.facets), 0.0), Vec::new(), "{:?}", new.notes);
}

#[test]
fn a_blind_hole_ring_ends_at_its_floor_and_the_outer_wall_goes_to_the_bottom() {
    for leave in [0.0, 0.5] {
        let (world, mut h) = world_with(block_with_hole(Some(10.0)), "EM-1200-2FL");
        let (t, w) = run(&world, &mut h, "C1", "EM-1200-2FL", contour_op(4.0, leave));
        assert_eq!(w, Vec::new(), "leave {leave}: {:?}", t.notes);
        // Levels 20, 16, 12 walk the ring; 8 is under the floor, so the
        // floor itself (plus the leave) is the ring's last level.
        assert_eq!(levels_where(&t, in_hole), [20.0, 16.0, 12.0, 10.0 + leave], "leave {leave}");
        assert_eq!(levels_where(&t, outside_block), [20.0, 16.0, 12.0, 8.0, 4.0, 0.0, -2.0]);
        let deepest = t.moves.iter().filter(|m| in_hole(m.to.x, m.to.y)).map(|m| m.to.z).fold(f64::INFINITY, f64::min);
        assert_eq!(deepest, 10.0 + leave, "nothing in the hole column goes below the floor");
        let notes = t.notes.join(" | ");
        assert!(notes.contains(&format!("ends at its floor Z {:.3}", 10.0 + leave)), "{notes}");
        assert!(notes.contains("levels below skipped (8.000, 4.000, 0.000, -2.000)"), "{notes}");
    }
}

#[test]
fn a_through_hole_ring_keeps_every_level() {
    let (world, mut h) = world_with(block_with_hole(None), "EM-1200-2FL");
    let (t, w) = run(&world, &mut h, "C1", "EM-1200-2FL", contour_op(4.0, 0.0));
    assert_eq!(w, Vec::new(), "{:?}", t.notes);
    let all = [20.0, 16.0, 12.0, 8.0, 4.0, 0.0, -2.0];
    assert_eq!(levels_where(&t, in_hole), all);
    assert_eq!(levels_where(&t, outside_block), all);
    assert!(!t.notes.iter().any(|n| n.starts_with("depth:")), "{:?}", t.notes);
    // A part whose rings all run the full depth posts the flat path.
    let m = block_with_hole(None);
    let region = top_section(&m);
    let p = direct_params(4.0, 0.0);
    assert_eq!(
        contour_on_part(&region, &m.facets, &p).expect("on part").moves,
        contour(&region, &p).expect("flat").moves
    );
}

#[test]
fn random_stepdowns_and_floors_never_cut_below_a_floor() {
    // A deterministic LCG: the sweep is reproducible, never flaky.
    let mut s: u64 = 0x5eed_cafe;
    let mut next = || {
        s = s.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        (s >> 11) as f64 / (1u64 << 53) as f64
    };
    for case in 0..14 {
        let stepdown = 0.7 + 8.0 * next();
        let floor = 1.0 + 17.0 * next();
        let leave = if case % 3 == 0 { 0.3 } else { 0.0 };
        let m = block_with_hole(Some(floor));
        let region = top_section(&m);
        let t = contour_on_part(&region, &m.facets, &direct_params(stepdown, leave)).expect("contour");
        let w = depth_verify(&t, &region, Some(&m.facets), leave);
        assert_eq!(gouges(&w), 0, "case {case}: stepdown {stepdown} floor {floor}: {w:?}");
        let ring = levels_where(&t, in_hole);
        let deepest = ring.last().copied().unwrap_or(f64::INFINITY);
        assert!(deepest >= floor + leave - 1e-9, "case {case}: ring at {deepest} under floor {floor}");
        // It reaches the floor: never stops a whole stepdown short.
        assert!((deepest - (floor + leave)).abs() < 1e-6, "case {case}: {ring:?} vs floor {floor}");
        for mv in &t.moves {
            if mv.kind != MoveKind::Rapid && in_hole(mv.to.x, mv.to.y) {
                assert!(mv.to.z >= floor + leave - 1e-9, "case {case}: {mv:?}");
            }
        }
        assert_eq!(levels_where(&t, outside_block).last(), Some(&-2.0), "case {case}");
    }
}

fn drill_op(bottom: Option<f64>) -> OpParams {
    OpParams::Drill {
        clearance_z: Height::fixed(30.0),
        retract_z: None,
        top_z: Height::fixed(22.0),
        bottom_z: bottom.map(Height::fixed),
        cycle: CycleChoice::Auto,
        peck: 0.0,
        dwell: 0.0,
        feed: 130.0,
        plunge_feed: 100.0,
        holes: vec![Hole {
            xy: Point2::new(0.0, 0.0),
            radius: 2.0,
            entry_z: 20.0,
            floor_z: 10.0,
            through: false,
            stepped: false,
        }],
        cross: Vec::new(),
    }
}

#[test]
fn a_drill_past_a_blind_floor_is_a_gouge_and_never_posts() {
    let (world, mut h) = world_with(block_with_hole(Some(10.0)), "DR-0400");
    let (_, w) = run(&world, &mut h, "D1", "DR-0400", drill_op(None));
    assert_eq!(gouges(&w), 0, "to the floor is clean: {w:?}");
    // A Bottom Z override 5 mm under the blind floor: in the part.
    let (world, mut h) = world_with(block_with_hole(Some(10.0)), "DR-0400");
    let (_, w) = run(&world, &mut h, "D1", "DR-0400", drill_op(Some(5.0)));
    assert!(gouges(&w) > 0, "below the floor is a gouge: {w:?}");
    let nc = std::env::temp_dir().join(format!("camcraft-blind-floor-{}.nc", std::process::id()));
    let _ = std::fs::remove_file(&nc);
    let err = posting::post_setup(&world, &nc).expect_err("a gouge blocks the post");
    assert!(err.contains("post blocked"), "{err}");
    assert!(!nc.exists(), "blocked output is never written");
}

/// Issue #578: the gate's gouge clause speaks the document's numbers.
/// The same world, told in inches: depth and location divide by 25.4
/// with two extra decimals (the `len` law) and say "in", never mm.
#[test]
fn the_gouge_clause_speaks_the_document_s_units() {
    use camcraft_app::posting::sim_clause;
    use camcraft_app::units::{self, Unit};
    let (world, mut h) = world_with(block_with_hole(Some(10.0)), "DR-0400");
    let (_, w) = run(&world, &mut h, "D1", "DR-0400", drill_op(Some(5.0)));
    assert!(gouges(&w) > 0, "below the floor is a gouge: {w:?}");
    units::set(&world, Unit::Inches);
    let clause = sim_clause(&world);
    let problem = clause.problem.expect("a located gouge is a problem");
    assert!(problem.starts_with("gouge: program cuts below the model in "), "{problem}");
    assert!(problem.contains("worst") && problem.contains(" in at ("), "the inch clause: {problem}");
    assert!(!problem.contains("mm"), "no millimeters in an inch document: {problem}");
    units::set(&world, Unit::Millimeters);
    let mm = sim_clause(&world).problem.expect("the same gouge");
    assert!(mm.contains(" mm at ("), "mm mode keeps its sentence: {mm}");
}

#[test]
fn a_face_cut_into_the_part_is_a_gouge() {
    let face = |cut: f64| OpParams::Face { holes: Default::default(),
        blend_turns: false,
        tilt: None,
        stepover: 4.0,
        clearance_z: Height::fixed(30.0),
        cut_z: Height::fixed(cut),
        top_z: None,
        stepdown: None,
        feed: 1200.0,
        plunge_feed: 300.0,
        climb: true,
    };
    let (world, mut h) = world_with(block_with_hole(Some(10.0)), "EM-0800-4FL");
    let (_, w) = run(&world, &mut h, "F1", "EM-0800-4FL", face(20.0));
    assert_eq!(gouges(&w), 0, "skimming the part top is clean: {w:?}");
    let (world, mut h) = world_with(block_with_hole(Some(10.0)), "EM-0800-4FL");
    let (_, w) = run(&world, &mut h, "F1", "EM-0800-4FL", face(18.0));
    assert!(gouges(&w) > 0, "facing 2 mm into the finished part is a gouge: {w:?}");
}
