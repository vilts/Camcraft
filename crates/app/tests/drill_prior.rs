//! Issue #89: drilling reads the stock as the setup's earlier operations
//! left it. A hole on the floor of a pocket an earlier op cleared is
//! measured from that floor (the #84 reach law), an R plane on a faced
//! top is not under "uncut stock" (the #91 R-plane law) - mill and lathe
//! alike - while a drill with nothing ahead, or with an ahead op that
//! never cut, still reads the raw stock and is refused. A drill judged
//! against an earlier op's removal goes Stale when that op's path moves.
//! Provenance: our own fixtures, specced from the Prime Directive and the
//! shop rule "measure the drill's reach from the surface it enters"; no
//! vendor material.
//!
//! Gate map (#89, #122): covers app::operation (drill_reach, drill_top,
//! drill_prior, the Drill arm of generate, the Drill stamp), app::rest
//! (PriorPlan), app::posting (the gate).
//! Lane shape (M4.4): all tests here run in seconds.

use flecs_ecs::prelude::World;

use camcraft_app::cl::MoveKind;
use camcraft_app::commands::{
    AddTool, CreateOperation, EditOperationParams, GenerateOperation, History, ImportModel, LoadMachine, SetStock,
};
use camcraft_app::drilling::{CycleChoice, Hole};
use camcraft_app::geo2d::Point2;
use camcraft_app::geometry::{Mesh, Point3, Triangle, cube};
use camcraft_app::links::Links;
use camcraft_app::machine::Machine;
use camcraft_app::machining2d::{FaceHoles, PocketStrategy};
use camcraft_app::machining3d::Entry3D;
use camcraft_app::model;
use camcraft_app::operation::{self, Height, OpParams, OpStatus, SrcRef};
use camcraft_app::posting;
use camcraft_app::schema::Location;
use camcraft_app::stock::StockShape;
use camcraft_app::tools::catalog;
use camcraft_app::world::new_world;

const FANUC: &str = include_str!("../../../assets/posts/fanuc.cps");
const TOP: f64 = 20.0;
/// The pocket floor; DR-0400's flute is 12 mm.
const FLOOR: f64 = 12.0;
/// The hole's floor under the pocket: 10 mm deep from the floor, 18
/// from the raw top - inside the flute only from the floor.
const HOLE_FLOOR: f64 = 2.0;

fn flip(t: &Triangle) -> Triangle {
    Triangle::new(t.a, t.c, t.b)
}

fn quad(a: Point3, b: Point3, c: Point3, d: Point3, f: &mut Vec<Triangle>) {
    f.push(Triangle::new(a, b, c));
    f.push(Triangle::new(a, c, d));
}

/// The flat ring at `z` between the squares `outer` and `inner` (half
/// widths), facing up.
fn ring(outer: f64, inner: f64, z: f64, f: &mut Vec<Triangle>) {
    let sq = |h: f64| [(-h, -h), (h, -h), (h, h), (-h, h)];
    let (o, i) = (sq(outer), sq(inner));
    let p = |(x, y): (f64, f64)| Point3::new(x, y, z);
    for k in 0..4 {
        let n = (k + 1) % 4;
        quad(p(o[k]), p(o[n]), p(i[n]), p(i[k]), f);
    }
}

/// A 40 x 40 x [-10, 20] block. `pocket` = Some(half): a square pocket
/// from the top to [`FLOOR`], a 4.1 mm square hole from its floor down
/// to [`HOLE_FLOOR`]; None: the 4.1 mm hole from the top down to
/// [`FLOOR`] alone.
fn block(pocket: Option<f64>) -> Mesh {
    let mut f = Vec::new();
    let outer = cube(Point3::new(-20.0, -20.0, -10.0), Point3::new(20.0, 20.0, TOP));
    f.extend(outer.facets[4..12].iter().cloned());
    f.extend(outer.facets[0..2].iter().cloned());
    let h = 2.05;
    let walls = |lo: f64, hi: f64, half: f64, f: &mut Vec<Triangle>| {
        let c = cube(Point3::new(-half, -half, lo), Point3::new(half, half, hi));
        f.extend(c.facets[4..12].iter().map(flip));
    };
    let floor = |z: f64, f: &mut Vec<Triangle>| {
        let c = cube(Point3::new(-h, -h, z), Point3::new(h, h, z + 1.0));
        f.extend(c.facets[0..2].iter().map(flip));
    };
    match pocket {
        Some(p) => {
            ring(20.0, p, TOP, &mut f);
            walls(FLOOR, TOP, p, &mut f);
            ring(p, h, FLOOR, &mut f);
            walls(HOLE_FLOOR, FLOOR, h, &mut f);
            floor(HOLE_FLOOR, &mut f);
        }
        None => {
            ring(20.0, h, TOP, &mut f);
            walls(FLOOR, TOP, h, &mut f);
            floor(FLOOR, &mut f);
        }
    }
    Mesh::from_facets(f)
}

fn add_tool(world: &World, h: &mut History, name: &str) {
    let t = catalog().into_iter().find(|(n, _)| *n == name).map(|(_, t)| t).expect("catalog");
    h.execute(world, AddTool::new(world, name, t).expect("tool"));
}

/// The block under a 40 x 40 box stock, z -10..`stock_top`.
fn mill_world(pocket: Option<f64>, stock_top: f64) -> (World, History) {
    let world = new_world();
    let mut h = History::default();
    h.execute(&world, ImportModel::from_mesh("block", block(pocket)));
    let at = Location { pos: Point3::new(0.0, 0.0, -10.0), ..Default::default() };
    let s = SetStock::new(&world, StockShape::Box { dx: 40.0, dy: 40.0, dz: stock_top + 10.0 }, at);
    h.execute(&world, s.expect("stock"));
    for t in ["DR-0400", "EM-0600-4FL", "EM-0800-4FL"] {
        add_tool(&world, &mut h, t);
    }
    (world, h)
}

fn drill(entry: f64, floor: f64, r: f64) -> OpParams {
    let hole = Hole { xy: Point2::new(0.0, 0.0), radius: 2.0, entry_z: entry, floor_z: floor, through: false, stepped: false };
    OpParams::Drill {
        break_through: None,
        break_through_from: Default::default(),
        clearance_z: Height::fixed(TOP + 15.0),
        retract_z: None,
        top_z: Height::fixed(r),
        bottom_z: None,
        cycle: CycleChoice::Drill,
        peck: 0.0,
        dwell: 0.0,
        feed: 120.0,
        plunge_feed: 80.0,
        holes: vec![hole],
        cross: vec![],
    }
}

fn pocket(bottom: f64) -> OpParams {
    OpParams::Pocket3D {
        links: Links::ROUGH3D,
        stepover: 2.5,
        stock_to_leave: 0.0,
        clearance_z: Height::fixed(30.0),
        retract_z: None,
        top_z: Height::fixed(TOP),
        bottom_z: Height::fixed(bottom),
        stepdown: 4.0,
        feed: 1500.0,
        plunge_feed: 400.0,
        climb: true,
        strategy: PocketStrategy::Zigzag,
        entry: Entry3D::Plunge,
        helix_radius: 1.0,
        rest_rough: false,
        optimal_load: camcraft_app::load::load_of_degrees(60.0),
        feed_opt: false,
        shell: false,
    }
}

fn face(cut: f64) -> OpParams {
    OpParams::Face {
        holes: FaceHoles::CutAir,
        blend_turns: false,
        tilt: None,
        stepover: 5.0,
        clearance_z: Height::fixed(35.0),
        cut_z: Height::fixed(cut),
        top_z: None,
        stepdown: None,
        feed: 1200.0,
        plunge_feed: 300.0,
        climb: true,
    }
}

fn create(world: &World, h: &mut History, name: &str, tool: &str, src: &str, p: OpParams) {
    let c = CreateOperation::new(world, name, tool, SrcRef::Entity(src.into()), p);
    h.execute(world, c.expect("create"));
}

/// Generate `name`; the generation error's text, if any.
fn generate(world: &World, h: &mut History, name: &str) -> Result<(), String> {
    h.execute(world, GenerateOperation::new(world, name).map_err(|e| format!("{e:?}"))?);
    let e = model::find_by_name(world, name).ok_or("no op")?;
    match operation::status_of(world, &e) {
        OpStatus::Ok => Ok(()),
        _ => {
            let op = model::operation_of(world, &e).ok_or("no op")?;
            Err(operation::generate(world, &op, name).err().map_or_else(|| "not Ok".into(), |e| e.to_string()))
        }
    }
}

fn status(world: &World, name: &str) -> OpStatus {
    model::find_by_name(world, name).map_or(OpStatus::Never, |e| operation::status_of(world, &e))
}

fn post(world: &World, cps: &str, tag: &str) -> Result<String, String> {
    let nc = std::env::temp_dir().join(format!("camcraft-drill-prior-{tag}-{}.nc", std::process::id()));
    let _ = std::fs::remove_file(&nc);
    let r = posting::post_script(world, &nc, cps).map(|p| p.program);
    let _ = std::fs::remove_file(&nc);
    r
}

#[test]
fn a_hole_on_a_pocket_floor_an_earlier_op_cleared_posts() {
    // R on the raw top (Z 20): the R-plane law is clean either way; the
    // reach from the raw top is 18 mm past the 12 mm flute - from the
    // pocket floor P1 left it is the hole's own 10 mm.
    let (world, mut h) = mill_world(Some(6.0), TOP);
    create(&world, &mut h, "Op1", "EM-0600-4FL", "block", pocket(FLOOR));
    generate(&world, &mut h, "Op1").expect("the pocket generates");
    create(&world, &mut h, "Op2", "DR-0400", "block", drill(FLOOR, HOLE_FLOOR, TOP));
    generate(&world, &mut h, "Op2").unwrap_or_else(|e| panic!("the hole on the pocket floor generates: {e}"));
    let text = post(&world, FANUC, "pocket").unwrap_or_else(|e| panic!("pocket-then-drill posts: {e}"));
    assert!(text.contains("G81"), "{text}");
}

#[test]
fn the_same_hole_with_nothing_ahead_is_still_refused() {
    let (world, mut h) = mill_world(Some(6.0), TOP);
    create(&world, &mut h, "Op2", "DR-0400", "block", drill(FLOOR, HOLE_FLOOR, TOP));
    let e = generate(&world, &mut h, "Op2").err().unwrap_or_else(|| panic!("drill-first generated past its flute"));
    assert!(e.contains("deep from the stock surface") && e.contains("flute"), "the reach refusal: {e}");
}

#[test]
fn an_ahead_op_that_never_cut_leaves_the_raw_reading() {
    // P1 created but never generated: what it would cut is unknown, so
    // the raw stock stands.
    let (world, mut h) = mill_world(Some(6.0), TOP);
    create(&world, &mut h, "Op1", "EM-0600-4FL", "block", pocket(FLOOR));
    create(&world, &mut h, "Op2", "DR-0400", "block", drill(FLOOR, HOLE_FLOOR, TOP));
    let e = generate(&world, &mut h, "Op2").err().unwrap_or_else(|| panic!("an uncut pocket was credited"));
    assert!(e.contains("deep from the stock surface"), "the reach refusal: {e}");
}

#[test]
fn a_shallower_earlier_pocket_makes_the_drill_stale() {
    let (world, mut h) = mill_world(Some(6.0), TOP);
    create(&world, &mut h, "Op1", "EM-0600-4FL", "block", pocket(FLOOR));
    generate(&world, &mut h, "Op1").expect("Op1");
    create(&world, &mut h, "Op2", "DR-0400", "block", drill(FLOOR, HOLE_FLOOR, TOP));
    generate(&world, &mut h, "Op2").expect("Op2");
    assert_eq!(status(&world, "Op2"), OpStatus::Ok);
    h.execute(&world, EditOperationParams::new(&world, "Op1", pocket(FLOOR + 3.0)).expect("edit"));
    generate(&world, &mut h, "Op1").expect("P1 again");
    assert_eq!(status(&world, "Op2"), OpStatus::Stale, "the drill was judged against P1's old floor");
}

#[test]
fn face_then_drill_with_r_at_the_faced_height_posts() {
    // Stock 5 mm over the part top; F1 faces it to Z 20; R = 20.
    let (world, mut h) = mill_world(None, TOP + 5.0);
    create(&world, &mut h, "Op1", "EM-0800-4FL", "block", face(TOP));
    generate(&world, &mut h, "Op1").expect("the face generates");
    create(&world, &mut h, "Op2", "DR-0400", "block", drill(TOP, FLOOR, TOP));
    generate(&world, &mut h, "Op2").unwrap_or_else(|e| panic!("R on the faced top generates: {e}"));
    let text = post(&world, FANUC, "face").unwrap_or_else(|e| panic!("face-then-drill posts: {e}"));
    assert!(text.contains("G81") && text.contains("R20"), "{text}");
}

#[test]
fn drill_first_with_r_under_the_unfaced_stock_is_still_refused() {
    let (world, mut h) = mill_world(None, TOP + 5.0);
    create(&world, &mut h, "Op2", "DR-0400", "block", drill(TOP, FLOOR, TOP));
    let e = generate(&world, &mut h, "Op2").err().unwrap_or_else(|| panic!("R under the raw stock generated"));
    assert!(e.contains("R plane Z 20.000") && e.contains("below the stock top Z 25.000"), "{e}");
}

/// The closed-solid revolve the turning suites share.
fn revolve(meridian: &[(f64, f64)], segs: usize) -> Mesh {
    let v = |j: usize, r: f64, z: f64| {
        let th = std::f64::consts::TAU * j as f64 / segs as f64;
        Point3::new(r * th.cos(), r * th.sin(), z)
    };
    let mut facets = Vec::new();
    for seg in meridian.windows(2) {
        let ((ra, za), (rb, zb)) = (seg[0], seg[1]);
        for j in 0..segs {
            let (a1, a2, b1, b2) = (v(j, ra, za), v(j + 1, ra, za), v(j, rb, zb), v(j + 1, rb, zb));
            if ra <= 1e-12 {
                facets.push(Triangle::new(a1, b2, b1));
            } else if rb <= 1e-12 {
                facets.push(Triangle::new(a1, a2, b2));
            } else {
                facets.push(Triangle::new(a1, a2, b2));
                facets.push(Triangle::new(a1, b2, b1));
            }
        }
    }
    Mesh::from_facets(facets)
}

/// A bar r10 standing at its raw length (z -20..22, the Facing kind
/// faces the work model, so the allowance is modelled on it), bored
/// r2.05 to Z 10, in its box stock (z -20..22), on a lathe. The
/// finished end face is Z 20.
fn lathe_world() -> (World, History) {
    lathe_world_with(22.0, StockShape::Box { dx: 24.0, dy: 24.0, dz: 42.0 })
}

/// The lathe fixture with the part's end at `part_top` (the bore and
/// the stock's z -20..22 unchanged): 22 models the allowance on the
/// part, 20 is the part at its finished length with the allowance
/// declared only in the stock (#122).
fn lathe_world_with(part_top: f64, stock: StockShape) -> (World, History) {
    let world = new_world();
    let mut h = History::default();
    let at = Location { pos: Point3::new(0.0, 0.0, -20.0), ..Location::default() };
    let s = SetStock::new(&world, stock, at);
    h.execute(&world, s.expect("stock"));
    let lathe = Machine {
        model: "TestBench lathe".into(),
        vendor: "Camcraft Testbench".into(),
        kind: Some("lathe".into()),
        spindle_min: Some(25.0),
        spindle_max: Some(12000.0),
        ..Default::default()
    };
    h.execute(&world, LoadMachine::new(&world, "lathe.mch".into(), Some(lathe)).expect("lathe"));
    for t in ["DR-0400", "TN-1600-0R8", "GT-0300"] {
        add_tool(&world, &mut h, t);
    }
    let bar = revolve(
        &[(0.0, -20.0), (10.0, -20.0), (10.0, part_top), (2.05, part_top), (2.05, 10.0), (0.0, 10.0)],
        64,
    );
    h.execute(&world, ImportModel::from_mesh("bar", bar));
    (world, h)
}

fn facing() -> OpParams {
    OpParams::Facing {
        tolerance: 0.05,
        clearance_z: Height::fixed(30.0),
        top_z: Height::fixed(22.0),
        bottom_z: Height::fixed(20.0),
        stepdown: 0.5,
        feed: 150.0,
        plunge_feed: 80.0,
    }
}

/// R 1 mm over the faced end (the everyday R clearance): the nose ball
/// on the column grid reads the faced end a few microns high, never low.
#[test]
fn a_lathe_axial_drill_after_facing_posts() {
    let (world, mut h) = lathe_world();
    create(&world, &mut h, "Op1", "TN-1600-0R8", "bar", facing());
    generate(&world, &mut h, "Op1").expect("the facing generates");
    create(&world, &mut h, "Op2", "DR-0400", "bar", drill(TOP, 10.0, TOP + 1.0));
    generate(&world, &mut h, "Op2").unwrap_or_else(|e| panic!("the drill on the faced end generates: {e}"));
    // The drill's own verdict: Ok. (The whole program's post is not
    // asserted - this fixture models the allowance on the part, so its
    // facing pass reads as a gouge of it; the stock-allowance fixture
    // below (#122) posts the whole program.)
    assert_eq!(status(&world, "Op2"), OpStatus::Ok);
}

#[test]
fn a_lathe_axial_drill_before_facing_is_refused() {
    let (world, mut h) = lathe_world();
    create(&world, &mut h, "Op2", "DR-0400", "bar", drill(TOP, 10.0, TOP + 1.0));
    let e = generate(&world, &mut h, "Op2").err().unwrap_or_else(|| panic!("R under the facing allowance generated"));
    assert!(e.contains("below the stock top Z 22.000"), "{e}");
}

/// Review fix 3: R 0.1 under the faced end must refuse - a model that
/// over-cut the end by a nose radius would wave it through.
#[test]
fn a_lathe_axial_drill_with_r_just_under_the_faced_end_is_refused() {
    let (world, mut h) = lathe_world();
    create(&world, &mut h, "Op1", "TN-1600-0R8", "bar", facing());
    generate(&world, &mut h, "Op1").expect("the facing generates");
    create(&world, &mut h, "Op2", "DR-0400", "bar", drill(TOP, 10.0, TOP - 0.1));
    let e = generate(&world, &mut h, "Op2").err().unwrap_or_else(|| panic!("R under the faced end generated"));
    assert!(e.contains("R plane Z 19.900"), "{e}");
}

/// Review fix B1/B2: a parting kerf to the axis does not clear the end
/// face above it - on a lathe only the Facing kind is credited.
#[test]
fn a_parting_op_ahead_does_not_credit_the_lathe_end_face() {
    let (world, mut h) = lathe_world();
    let part = OpParams::Part {
        tolerance: 0.05,
        clearance_z: Height::fixed(30.0),
        top_z: Height::fixed(22.0),
        bottom_z: Height::fixed(0.0),
        stepdown: 0.5,
        feed: 150.0,
        plunge_feed: 60.0,
        bar_load: false,
        bar_advance: 0.0,
        bar_unload: false,
        bar_transfer: false,
        inspect_tol: 0.0,
    };
    create(&world, &mut h, "Op1", "GT-0300", "bar", part);
    generate(&world, &mut h, "Op1").expect("the parting generates");
    create(&world, &mut h, "Op2", "DR-0400", "bar", drill(TOP, 10.0, TOP + 1.0));
    let e = generate(&world, &mut h, "Op2").err().unwrap_or_else(|| panic!("the kerf credited the end face"));
    assert!(e.contains("below the stock top Z 22.000"), "{e}");
}

/// #122: the bar at its finished length (end Z 20), the 2 mm facing
/// allowance declared only in the setup's round stock (to Z 22). The
/// Facing faces the stock's allowance down to the part's end - it no
/// longer reports "nothing to face" - and the axial drill after it is
/// judged against that faced end and stands Ok. (The whole program's
/// post is not asserted yet: the footprint rapid law convicts lathe
/// facing's inward rapids on main too - #159, riding #103's meridian
/// law - and this pin grows its post line when that lands.)
#[test]
fn a_stock_facing_allowance_is_faced_and_the_drill_after_it_stands() {
    let (world, mut h) = lathe_world_with(TOP, StockShape::Cylinder { diameter: 24.0, height: 42.0 });
    create(&world, &mut h, "Op1", "TN-1600-0R8", "bar", facing());
    generate(&world, &mut h, "Op1").unwrap_or_else(|e| panic!("the stock allowance faces: {e}"));
    let e = model::find_by_name(&world, "Op1").expect("Op1");
    let path = model::toolpath_of(&world, &e).expect("the facing's path").path;
    let feeds_to_axis: Vec<f64> = path
        .moves
        .iter()
        .filter(|m| m.kind == MoveKind::Feed && m.to.x.hypot(m.to.y) < 1e-6)
        .map(|m| m.to.z)
        .collect();
    // #2004: four bites of the 0.5 mm stepdown remove the 2 mm allowance.
    // The old fifth pass had its tip at the stock end Z 22 and cut air.
    assert_eq!(feeds_to_axis.len(), 4, "four passes over the 2 mm allowance: {feeds_to_axis:?}");
    let nose = 0.8;
    assert!(feeds_to_axis.iter().all(|z| *z >= TOP + nose - 1e-6), "a pass below the finished end: {feeds_to_axis:?}");
    // The full allowance comes off: the last pass is at the finished end,
    // the first is one stepdown under the stock end, and no bite is deeper
    // than one stepdown.
    let last = feeds_to_axis.last().copied().unwrap_or(f64::NAN);
    assert!((last - (TOP + nose)).abs() < 1e-6, "the last pass faces to the finished end: {feeds_to_axis:?}");
    let first = feeds_to_axis.first().copied().unwrap_or(f64::NAN);
    assert!((first - (22.0 - 0.5 + nose)).abs() < 1e-6, "the first bite is one stepdown: {feeds_to_axis:?}");
    assert!(
        feeds_to_axis.windows(2).all(|w| w[0] - w[1] <= 0.5 + 1e-6 && w[0] > w[1]),
        "each bite at most one stepdown: {feeds_to_axis:?}"
    );
    create(&world, &mut h, "Op2", "DR-0400", "bar", drill(TOP, 10.0, TOP + 1.0));
    generate(&world, &mut h, "Op2").unwrap_or_else(|e| panic!("the drill on the faced end generates: {e}"));
    assert_eq!(status(&world, "Op2"), OpStatus::Ok);
}

#[test]
fn a_finished_part_with_no_stock_allowance_has_nothing_to_face() {
    // The stock's end level with the part's: neither has material
    // above the facing plane, and the refusal names it.
    let (world, mut h) = lathe_world_with(TOP, StockShape::Cylinder { diameter: 24.0, height: 40.0 });
    create(&world, &mut h, "Op1", "TN-1600-0R8", "bar", facing());
    let e = generate(&world, &mut h, "Op1").err().unwrap_or_else(|| panic!("a facing of nothing generated"));
    assert!(e.contains("nothing to face"), "{e}");
}
