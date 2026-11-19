//! Issue #740: a generator refusal printed in an inch document must speak
//! inches, not millimeters. Each scenario pins one refusal family twice:
//! byte-identical millimeters under the default unit (what the standing
//! suites already assert, re-pinned here so the fix cannot move them),
//! then the same numbers converted under `Unit::Inches` on the very same
//! world - the lens law (`crate::units`: stored values stay millimeters,
//! the sentence is the edge). The `GenError::At` location stays
//! millimeters everywhere.
//!
//! Gate map (M4.4): covers app::operation (drill_reach's too_deep and
//! shank-fit sentences, the cross arm's reach naming, circle_fit's
//! deviation, area's part-fills-the-selection sentence) through
//! operation::generate.
//! Lane shape (M4.4): all tests here run in seconds.

mod punched_bar;

use flecs_ecs::prelude::World;

use camcraft_app::commands::{AddTool, History, ImportModel, LoadMachine, SetStock};
use camcraft_app::drilling::{CrossHole, CycleChoice, Hole};
use camcraft_app::geo2d::Point2;
use camcraft_app::geometry::feature::{self, FaceKind};
use camcraft_app::geometry::{IDENTITY_Q, Mesh, Point3, Triangle, cube};
use camcraft_app::machine::{AxisLimits, Machine};
use camcraft_app::model;
use camcraft_app::operation::{self, FeatureSel, GeoSel, Height, OpParams, SrcRef};
use camcraft_app::schema::{Location, Operation};
use camcraft_app::stock::StockShape;
use camcraft_app::tools::{Tool, catalog};
use camcraft_app::units::{self, Unit};
use camcraft_app::world::new_world;

const PART_R: f64 = 10.0;

/// One generated refusal, as the sentence the user would read.
fn refusal(world: &World, op: &Operation, why: &str) -> String {
    operation::generate(world, op, "")
        .err()
        .map(|e| e.to_string())
        .unwrap_or_else(|| panic!("{why}"))
}

/// A catalog tool, re-ground by `grind`.
fn tool(name: &str, grind: impl Fn(&mut Tool)) -> Tool {
    let mut t = catalog().into_iter().find(|(n, _)| *n == name).map(|(_, t)| t).expect("catalog entry");
    grind(&mut t);
    t
}

fn drill_op(src: &str, top: f64, clear: f64, holes: Vec<Hole>, cross: Vec<CrossHole>) -> Operation {
    Operation {
        tool: "DR-0400".into(),
        src: SrcRef::Entity(src.into()),
        params: OpParams::Drill {
            break_through: None,
            break_through_from: Default::default(),
            clearance_z: Height::fixed(top + clear),
            retract_z: None,
            top_z: Height::fixed(top),
            bottom_z: None,
            cycle: CycleChoice::Auto,
            peck: 0.0,
            dwell: 0.0,
            feed: 120.0,
            plunge_feed: 80.0,
            holes,
            cross,
        },
        stamp: None,
        setup: None,
        area: Default::default(),
        coolant: Default::default(),
        after: Default::default(),
        suppressed: false,
    }
}

// ---- A: the axial too_deep sentence (drill_reach) ---------------------

/// A 40 x 40 x 20 block (top at z20) on a mill, with `stock` boxed over
/// it when given (its top Z).
fn mill_world(stock_top: Option<f64>) -> (World, History) {
    let world = new_world();
    let mut history = History::default();
    if let Some(top) = stock_top {
        let at = Location { pos: Point3::new(0.0, 0.0, 0.0), rot: IDENTITY_Q };
        let s = SetStock::new(&world, StockShape::Box { dx: 50.0, dy: 50.0, dz: top }, at);
        history.execute(&world, s.expect("stock"));
    }
    history.execute(&world, AddTool::new(&world, "DR-0400", tool("DR-0400", |_| {})).expect("tool"));
    history.execute(&world, ImportModel::from_mesh("block", cube(Point3::new(-20.0, -20.0, 0.0), Point3::new(20.0, 20.0, 20.0))));
    (world, history)
}

fn z_hole(floor_z: f64) -> Hole {
    Hole { xy: Point2::new(0.0, 0.0), radius: 2.0, entry_z: 20.0, floor_z, through: false, stepped: false }
}

#[test]
fn an_axial_reach_refusal_speaks_the_document_unit() {
    // 15 mm from the part top: past DR-0400's 12 mm flute.
    let (world, _h) = mill_world(None);
    let op = drill_op("block", 20.0, 10.0, vec![z_hole(5.0)], vec![]);
    let e = refusal(&world, &op, "the 15 mm hole on a 12 mm flute must refuse in mm");
    for s in ["hole at (0.000, 0.000)", "15.000 mm deep from the stock surface", "DR-0400", "12.000 mm"] {
        assert!(e.contains(s), "the mm refusal names '{s}': {e}");
    }
    // The very same world, the document switched to inches: the same
    // numbers through the lens, sentence by sentence.
    units::set(&world, Unit::Inches);
    let e = refusal(&world, &op, "the 15 mm hole on a 12 mm flute must refuse in inches");
    for s in ["hole at (0.00000, 0.00000)", "0.59055 in deep from the stock surface", "DR-0400", "0.47244 in"] {
        assert!(e.contains(s), "the inch refusal names '{s}': {e}");
    }
    assert!(!e.contains(" mm"), "an inch document reads no millimeters: {e}");
}

// ---- B: the cross arm's reach naming (cross_spindle) ------------------

/// The finished bar of radius `r`, z in [-80, 20], revolved about world
/// Z, with this suite's cross holes modelled.
fn bar(r: f64) -> Mesh {
    punched_bar::punched_bar(r, punched_bar::FOOT, punched_bar::TOP, &[cross_hole(r, -1.0, -6.0), cross_hole(-r, 1.0, -3.0)])
}

/// A `word` machine (live-tooling rotaries stated) with the r10 bar,
/// the raw r20 bar as stock when `raw`, and DR-0400 re-ground by `grind`.
fn turret_world(word: &str, raw: bool, grind: impl Fn(&mut Tool)) -> (World, History) {
    let world = new_world();
    let mut history = History::default();
    if raw {
        let at = Location { pos: Point3::new(0.0, 0.0, -20.0), rot: IDENTITY_Q };
        let s = SetStock::new(&world, StockShape::Cylinder { diameter: 40.0, height: 40.0 }, at);
        history.execute(&world, s.expect("stock"));
    }
    let r = |name: &str, axis: [f64; 3]| AxisLimits { name: name.into(), rotary: true, axis: Some(axis), ..Default::default() };
    let axes = vec![r("A", [1.0, 0.0, 0.0]), r("B", [0.0, 1.0, 0.0]), r("C", [0.0, 0.0, 1.0])];
    let m = Machine { model: format!("TestBench {word}"), kind: Some(word.into()), axes, ..Default::default() };
    history.execute(&world, LoadMachine::new(&world, "bench.mch".into(), Some(m)).expect("machine"));
    history.execute(&world, AddTool::new(&world, "DR-0400", tool("DR-0400", grind)).expect("tool"));
    history.execute(&world, ImportModel::from_mesh("bar", bar(PART_R)));
    (world, history)
}

fn cross_hole(x: f64, dir: f64, floor_d: f64) -> CrossHole {
    CrossHole {
        origin: Point3::new(x, 0.0, 0.0),
        dir: Point3::new(dir, 0.0, 0.0),
        radius: 2.0,
        entry_d: 0.0,
        floor_d,
        through: false,
    }
}

/// One +X hole 6 mm into the r10 part: on the raw r20 bar its mouth
/// stands at r20, 16 mm above the floor - past DR-0400's 12 mm flute.
fn raw_cross_op() -> Operation {
    drill_op("bar", 20.0, 5.0, vec![], vec![cross_hole(PART_R, -1.0, -6.0)])
}

#[test]
fn a_cross_reach_refusal_speaks_the_document_unit() {
    let (world, _h) = turret_world("lathe", true, |_| {});
    let op = raw_cross_op();
    let e = refusal(&world, &op, "the 16 mm raw-bar cross hole must refuse in mm");
    for s in ["cross hole 0 at (10.000, 0.000, 0.000)", "16.000 mm deep from the stock surface", "DR-0400", "12.000 mm"] {
        assert!(e.contains(s), "the mm refusal names '{s}': {e}");
    }
    units::set(&world, Unit::Inches);
    let err = operation::generate(&world, &op, "")
        .err()
        .unwrap_or_else(|| panic!("the raw-bar cross hole must refuse in inches"));
    // The At mark stays millimeters: the lens is for the sentence.
    assert_eq!(err.location(), Some(Point3::new(10.0, 0.0, 0.0)), "the location stays millimeters");
    let e = err.to_string();
    for s in ["cross hole 0 at (0.39370, 0.00000, 0.00000)", "0.62992 in deep from the stock surface", "DR-0400", "0.47244 in"] {
        assert!(e.contains(s), "the inch refusal names '{s}': {e}");
    }
    assert!(!e.contains(" mm"), "an inch document reads no millimeters: {e}");
}

// ---- C: the shank-fit sentence in a pre-cut hole (drill_reach) --------

const TOP: f64 = 20.0;
const FLOOR: f64 = -5.0;

fn flip(t: &Triangle) -> Triangle {
    Triangle::new(t.a, t.c, t.b)
}

/// A 40 x 40 x [-10, 20] block with a square hole `2 h` wide from the
/// top, blind to z-5 (25 mm deep). Outward-wound.
fn block_with_hole(h: f64) -> Mesh {
    let quad = |a: Point3, b: Point3, c: Point3, d: Point3, f: &mut Vec<Triangle>| {
        f.push(Triangle::new(a, b, c));
        f.push(Triangle::new(a, c, d));
    };
    let mut f = Vec::new();
    let outer = cube(Point3::new(-20.0, -20.0, -10.0), Point3::new(20.0, 20.0, TOP));
    f.extend(outer.facets[4..12].iter().cloned());
    f.extend(outer.facets[0..2].iter().cloned());
    let o = [(-20.0, -20.0), (20.0, -20.0), (20.0, 20.0), (-20.0, 20.0)];
    let i = [(-h, -h), (h, -h), (h, h), (-h, h)];
    let p = |(x, y): (f64, f64)| Point3::new(x, y, TOP);
    for k in 0..4 {
        let n = (k + 1) % 4;
        quad(p(o[k]), p(o[n]), p(i[n]), p(i[k]), &mut f);
    }
    let inner = cube(Point3::new(-h, -h, FLOOR), Point3::new(h, h, TOP));
    f.extend(inner.facets[4..12].iter().map(flip));
    f.extend(inner.facets[0..2].iter().map(flip));
    Mesh::from_facets(f)
}

fn fit_world(hole_h: f64, name: &str, t: Tool) -> (World, History) {
    let world = new_world();
    let mut history = History::default();
    history.execute(&world, AddTool::new(&world, name, t).expect("tool"));
    history.execute(&world, ImportModel::from_mesh("block", block_with_hole(hole_h)));
    (world, history)
}

fn params(radius: f64, cycle: CycleChoice) -> OpParams {
    let hole = Hole { xy: Point2::new(0.0, 0.0), radius, entry_z: TOP, floor_z: FLOOR, through: false, stepped: false };
    OpParams::Drill {
        break_through: None,
        break_through_from: Default::default(),
        clearance_z: Height::fixed(TOP + 10.0),
        retract_z: None,
        top_z: Height::fixed(TOP),
        bottom_z: None,
        cycle,
        peck: 0.0,
        dwell: 0.0,
        feed: 120.0,
        plunge_feed: 80.0,
        holes: vec![hole],
        cross: vec![],
    }
}

fn raw_op(tool_name: &str, p: OpParams) -> Operation {
    Operation { tool: tool_name.into(), src: SrcRef::Entity("block".into()), params: p, stamp: None, setup: None, area: Default::default(), coolant: Default::default(), after: Default::default(), suppressed: false }
}

#[test]
fn a_shank_fit_refusal_speaks_the_document_unit() {
    // BR-1000's re-ground Ø14 shank fed into a Ø12 hole 25 mm deep.
    let (world, _h) = fit_world(6.0, "BR-1000", tool("BR-1000", |t| t.shank_d = 14.0));
    let op = raw_op("BR-1000", params(6.0, CycleChoice::Bore));
    let err = operation::generate(&world, &op, "")
        .err()
        .unwrap_or_else(|| panic!("the Ø14 shank in a Ø12 hole must refuse in mm"));
    let at = err.location();
    assert!(at.is_some_and(|p| p.x == 0.0 && p.y == 0.0 && p.z.is_finite()), "marks the hole at (0, 0): {at:?}");
    let e = err.to_string();
    for s in ["Bore cycle at (0.000, 0.000)", "25.000 mm deep", "BR-1000's 20.000 mm flute", "shank dia. 14.000", "dia. 12.000 hole"] {
        assert!(e.contains(s), "the mm refusal names '{s}': {e}");
    }
    units::set(&world, Unit::Inches);
    let e = refusal(&world, &op, "the Ø14 shank in a Ø12 hole must refuse in inches");
    for s in ["Bore cycle at (0.00000, 0.00000)", "0.98425 in deep", "BR-1000's 0.78740 in flute", "shank dia. 0.55118", "dia. 0.47244 hole"] {
        assert!(e.contains(s), "the inch refusal names '{s}': {e}");
    }
    assert!(!e.contains(" mm"), "an inch document reads no millimeters: {e}");
}

// ---- D: circle_fit's deviation sentence (Circular) --------------------

const DISC_TOOL: &str = "EM-0600-4FL";
const N: usize = 128;

/// A 40x40x8 block centred on `c` with one hole whose rim is `rim(a)`
/// (offset from `c` at angle `a`): blind pocket with a flat floor at
/// that height.
fn block(c: Point2, rim: impl Fn(f64) -> (f64, f64), floor: Option<f64>) -> Mesh {
    let tau = std::f64::consts::TAU;
    let ang = |i: usize| tau * (i % N) as f64 / N as f64;
    let bp = |i: usize, z: f64| {
        let (x, y) = rim(ang(i));
        Point3::new(c.x + x, c.y + y, z)
    };
    let op = |i: usize, z: f64| {
        let (x, y) = (ang(i).cos(), ang(i).sin());
        let s = x.abs().max(y.abs());
        Point3::new(c.x + x * 20.0 / s, c.y + y * 20.0 / s, z)
    };
    let quad = |a: Point3, b: Point3, cc: Point3, d: Point3, f: &mut Vec<Triangle>| {
        f.push(Triangle::new(a, b, cc));
        f.push(Triangle::new(a, cc, d));
    };
    let mid = |z: f64| Point3::new(c.x, c.y, z);
    let low = floor.unwrap_or(0.0);
    let mut f = Vec::new();
    for i in 0..N {
        let j = i + 1;
        quad(bp(j, 8.0), bp(i, 8.0), op(i, 8.0), op(j, 8.0), &mut f);
        quad(bp(i, 8.0), bp(j, 8.0), bp(j, low), bp(i, low), &mut f);
        quad(op(j, 8.0), op(i, 8.0), op(i, 0.0), op(j, 0.0), &mut f);
        match floor {
            None => quad(bp(i, 0.0), bp(j, 0.0), op(j, 0.0), op(i, 0.0), &mut f),
            Some(zf) => {
                f.push(Triangle::new(mid(zf), bp(i, zf), bp(j, zf)));
                f.push(Triangle::new(mid(0.0), op(j, 0.0), op(i, 0.0)));
            }
        }
    }
    Mesh::from_facets(f)
}

fn square(h: f64) -> impl Fn(f64) -> (f64, f64) {
    move |a| {
        let (x, y) = (a.cos(), a.sin());
        let s = x.abs().max(y.abs());
        (x * h / s, y * h / s)
    }
}

fn disc_world(mesh: Mesh, c: Point2) -> (World, History) {
    let world = new_world();
    let mut history = History::default();
    let t = catalog().into_iter().find(|(n, _)| *n == DISC_TOOL).map(|(_, t)| t).expect("tool");
    history.execute(&world, AddTool::new(&world, DISC_TOOL, t).expect("tool"));
    history.execute(&world, ImportModel::from_mesh("part", mesh));
    let at = Location { pos: Point3::new(c.x - 20.0, c.y - 20.0, 0.0), ..Location::default() };
    history.execute(
        &world,
        SetStock::new(&world, StockShape::Box { dx: 40.0, dy: 40.0, dz: 8.0 }, at).expect("stock"),
    );
    (world, history)
}

fn circular(bottom: f64) -> OpParams {
    OpParams::Circular {
        links: camcraft_app::links::Links::PLANAR,
        center: Point2::new(-3.0, 7.0),
        radius: 12.0,
        vault: false,
        stepover: 2.0,
        stock_to_leave: 0.5,
        clearance_z: Height::fixed(25.0),
        retract_z: None,
        top_z: Height::fixed(8.0),
        bottom_z: Height::fixed(bottom),
        stepdown: 4.0,
        feed: 800.0,
        plunge_feed: 200.0,
        climb: true,
        entry: camcraft_app::machining3d::Entry3D::Plunge,
        helix_radius: 1.0,
        tilt: None,
    }
}

fn pick(f: FeatureSel) -> SrcRef {
    SrcRef::Selection(vec![GeoSel { entity: "part".into(), feature: f }])
}

/// The upward planar face standing at `z`.
fn floor_face(world: &World, z: f64) -> FeatureSel {
    let e = model::find_by_name(world, "part").expect("part");
    let f = model::features_of(world, &e).expect("features");
    let id = f
        .faces
        .iter()
        .filter(|fa| matches!(fa.kind, FaceKind::Planar) && fa.normal[2] > 0.5)
        .find(|fa| (fa.offset - z).abs() < 1e-6)
        .map(|fa| fa.id)
        .expect("the floor face");
    FeatureSel::Face(id)
}

fn disc_op(src: SrcRef, p: OpParams) -> Operation {
    Operation { tool: DISC_TOOL.into(), src, params: p, stamp: None, setup: None, area: Default::default(), coolant: Default::default(), after: Default::default(), suppressed: false }
}

#[test]
fn a_circle_fit_refusal_speaks_the_document_unit() {
    let c = Point2::new(0.0, 0.0);
    let (world, _h) = disc_world(block(c, square(10.0), Some(3.0)), c);
    let op = disc_op(pick(floor_face(&world, 3.0)), circular(3.0));
    let e = refusal(&world, &op, "the square pocket pick must refuse in mm");
    for s in [
        "the selection is not a circle (deviation 2.917 mm at (10.000, 10.000))",
        "Circular needs a round hole or face",
        "clear the selection to use the typed centre and radius",
    ] {
        assert!(e.contains(s), "the mm refusal names '{s}': {e}");
    }
    units::set(&world, Unit::Inches);
    let e = refusal(&world, &op, "the square pocket pick must refuse in inches");
    assert!(e.contains("the selection is not a circle (deviation"), "the inch refusal: {e}");
    assert!(e.contains("deviation 0.11482 in at (0.39370, 0.39370))"), "the inch refusal names the deviation and point in inches: {e}");
    assert!(e.contains("clear the selection to use the typed centre and radius"), "the inch refusal: {e}");
    assert!(!e.contains("2.917"), "the deviation number itself crosses the lens: {e}");
    assert!(!e.contains(" mm"), "an inch document reads no millimeters: {e}");
}

// ---- E: the part-fills-the-selection sentence (pocket on a pick) ------

/// A 50 x 50 x [0, 20] block with a 36 x 36 blind pocket to Z 5 and a
/// 6 x 6 boss standing in it to Z 12. Outward-wound.
fn block_with_boss() -> Mesh {
    const H: f64 = 18.0;
    const B: f64 = 3.0;
    let flip = |t: &Triangle| Triangle::new(t.a, t.c, t.b);
    let quad = |a: Point3, b: Point3, c: Point3, d: Point3, f: &mut Vec<Triangle>| {
        f.push(Triangle::new(a, b, c));
        f.push(Triangle::new(a, c, d));
    };
    // A horizontal square annulus between half-widths `o` and `i` at
    // `z`, facing +z.
    let ring = |o: f64, i: f64, z: f64, f: &mut Vec<Triangle>| {
        let oo = [(-o, -o), (o, -o), (o, o), (-o, o)];
        let ii = [(-i, -i), (i, -i), (i, i), (-i, i)];
        for k in 0..4 {
            let n = (k + 1) % 4;
            let p = |(x, y): (f64, f64)| Point3::new(x, y, z);
            quad(p(oo[k]), p(oo[n]), p(ii[n]), p(ii[k]), f);
        }
    };
    let mut f = Vec::new();
    let outer = cube(Point3::new(-25.0, -25.0, 0.0), Point3::new(25.0, 25.0, 20.0));
    f.extend(outer.facets[0..2].iter().cloned()); // bottom
    f.extend(outer.facets[4..12].iter().cloned()); // block walls
    ring(25.0, H, 20.0, &mut f); // top
    let hole = cube(Point3::new(-H, -H, 5.0), Point3::new(H, H, 20.0));
    f.extend(hole.facets[4..12].iter().map(flip)); // pocket walls face the axis
    ring(H, B, 5.0, &mut f); // floor around the boss
    let boss = cube(Point3::new(-B, -B, 5.0), Point3::new(B, B, 12.0));
    f.extend(boss.facets[2..12].iter().cloned()); // boss top + walls
    Mesh::from_facets(f)
}

fn boss_world() -> (World, String) {
    let world = new_world();
    let mut h = History::default();
    let import = ImportModel::from_mesh("part", block_with_boss());
    let name = import.name.clone();
    h.execute(&world, import);
    let loc = Location { pos: Point3::new(0.0, 0.0, 0.0), ..Default::default() };
    h.execute(&world, SetStock::new(&world, StockShape::Box { dx: 70.0, dy: 70.0, dz: 20.0 }, loc).expect("stock"));
    let t = catalog().into_iter().find(|(n, _)| *n == "EM-1000-4FL").map(|(_, t)| t).expect("catalog");
    h.execute(&world, AddTool::new(&world, "EM-1000-4FL", t).expect("tool"));
    (world, name)
}

/// (max radius about the centroid, mean z, centroid x, centroid y).
fn ring_of(pts: &[[f64; 3]]) -> (f64, f64, f64, f64) {
    let n = pts.len().max(1) as f64;
    let cx = pts.iter().map(|p| p[0]).sum::<f64>() / n;
    let cy = pts.iter().map(|p| p[1]).sum::<f64>() / n;
    let z = pts.iter().map(|p| p[2]).sum::<f64>() / n;
    let r = pts.iter().map(|p| (p[0] - cx).hypot(p[1] - cy)).fold(0.0, f64::max);
    (r, z, cx, cy)
}

/// Every way to pick one ring: the up-facing faces whose outer loop
/// `fits`, the closed chains that do, the patches with a ring that does.
fn picks_of(world: &World, name: &str, fits: impl Fn(f64, f64, f64, f64) -> bool) -> Vec<(String, FeatureSel)> {
    let e = model::find_by_name(world, name).expect("model");
    let f = model::features_of(world, &e).expect("features");
    let vs = |vi: &[u32]| vi.iter().filter_map(|&v| f.verts.get(v as usize).copied()).collect::<Vec<_>>();
    let ok = |pts: &[[f64; 3]]| {
        let (r, z, cx, cy) = ring_of(pts);
        (fits(r, z, cx, cy), z)
    };
    let mut out = Vec::new();
    for face in f.faces.iter().filter(|face| face.normal[2] > 0.99) {
        if let Some(l) = face.loops.iter().find(|l| l.kind == feature::LoopKind::Outer)
            && let (true, z) = ok(&vs(&l.verts))
        {
            out.push((format!("face z {z:.1}"), FeatureSel::Face(face.id)));
        }
    }
    for c in feature::chains(&f).into_iter().filter(|c| c.closed) {
        if let (true, z) = ok(&feature::chain_points(&f, &c)) {
            out.push((format!("chain z {z:.1}"), FeatureSel::Chain(c.id)));
        }
    }
    for p in feature::patches(&f) {
        let Some(rings) = feature::patch_rings(&f, p.id) else { continue };
        if rings.iter().filter(|r| r.closed).any(|r| ok(&vs(&r.verts)).0) {
            out.push(("wall patch".to_owned(), FeatureSel::Patch(p.id)));
        }
    }
    out
}

fn pocket(top: f64, bottom: f64) -> OpParams {
    OpParams::Pocket { rest: None,
        blend_radius: None,
        links: camcraft_app::links::Links::PLANAR,
        stepover: 4.0,
        stock_to_leave: 0.0,
        clearance_z: Height::fixed(40.0),
        retract_z: None,
        top_z: Height::fixed(top),
        bottom_z: Height::fixed(bottom),
        stepdown: 5.0,
        feed: 1200.0,
        plunge_feed: 300.0,
        climb: true,
        strategy: camcraft_app::machining2d::PocketStrategy::OffsetRings,
        entry: camcraft_app::machining3d::Entry3D::Plunge,
        helix_radius: 1.0,
        optimal_load: camcraft_app::load::DEFAULT_OPTIMAL_LOAD,
        blend_turns: false,
        tilt: None,
    }
}

#[test]
fn a_part_fills_the_selection_refusal_speaks_the_document_unit() {
    // The boss's top face encloses nothing but the boss: the refusal
    // names the Z plane it checked.
    let (world, name) = boss_world();
    let top = picks_of(&world, &name, |r, z, _, _| (r - 18f64.sqrt()).abs() < 0.01 && (z - 12.0).abs() < 1e-9);
    let face = top.iter().find(|(l, _)| l.starts_with("face")).map(|(_, s)| *s).expect("boss top face");
    let op = Operation {
        tool: "EM-1000-4FL".into(),
        src: SrcRef::Selection(vec![GeoSel { entity: name.clone(), feature: face }]),
        params: pocket(12.0, 5.0),
        stamp: None,
        setup: None,
        area: Default::default(),
        coolant: Default::default(),
        after: Default::default(),
        suppressed: false,
    };
    let e = refusal(&world, &op, "a pick of solid part material must refuse in mm");
    assert!(e.contains("the part fills the whole selection at Z 7.000"), "the mm refusal: {e}");
    units::set(&world, Unit::Inches);
    let e = refusal(&world, &op, "a pick of solid part material must refuse in inches");
    assert!(e.contains("the part fills the whole selection"), "the inch refusal: {e}");
    assert!(e.contains(" at Z 0.27559"), "the inch refusal names the plane in inches: {e}");
    assert!(!e.contains(" mm"), "an inch document reads no millimeters: {e}");
}
