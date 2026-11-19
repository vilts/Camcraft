//! The cross drill stands off the RAW stock, not only the finished part
//! (issue #77): a cross drill run before the bar is turned to size -
//! the setup's stock wider than the part - measures its hop, approach
//! and clearance radii about the larger of the two, so no rapid between
//! stations passes through uncut stock (the Prime Directive: a rapid
//! through material is a collision). With the stock equal to the part
//! the chain is the part-only chain, bit for bit.
//!
//! Gate map (M4.4): covers app::operation (cross_spindle's stock
//! reading and its rapid-clear refusal, the cross arm of generate,
//! collect_job's stock facets).
//! Lane shape (M4.4): all tests here run in seconds.

mod punched_bar;

use flecs_ecs::prelude::World;

use camcraft_app::cl::{MoveKind, Toolpath};
use camcraft_app::commands::{AddTool, History, ImportModel, LoadMachine, SetStock};
use camcraft_app::drilling::{CrossHole, CycleChoice};
use camcraft_app::geometry::{IDENTITY_Q, Mesh, Point3, Triangle};
use camcraft_app::machine::{AxisLimits, Machine};
use camcraft_app::operation::{self, Height, OpParams, SrcRef};
use camcraft_app::schema::{Location, Operation};
use camcraft_app::stock::StockShape;
use camcraft_app::tools::{Tool, catalog};
use camcraft_app::world::new_world;

/// The finished bar's radius and the raw bar's: drilled before turning.
const PART_R: f64 = 10.0;
const STOCK_R: f64 = 20.0;

/// The finished bar of radius `r`, z in [-80, 20] (`punched_bar::FOOT`), revolved about world
/// Z, with `cross` modelled in it (issue #117: the part law refuses a
/// hole the model lacks).
fn bar(r: f64, cross: &[CrossHole]) -> Mesh {
    punched_bar::punched_bar(r, punched_bar::FOOT, punched_bar::TOP, cross)
}

/// Three stations on the finished r10 face: +X, +Y, then -X (the
/// opposite pair takes the end-face detour).
fn holes() -> Vec<CrossHole> {
    let h = |x: f64, y: f64, z: f64, dir: Point3| CrossHole {
        origin: Point3::new(x, y, z),
        dir,
        radius: 2.0,
        entry_d: 0.0,
        floor_d: -6.0,
        through: false,
    };
    vec![
        h(PART_R, 0.0, 10.0, Point3::new(-1.0, 0.0, 0.0)),
        h(0.0, PART_R, 0.0, Point3::new(0.0, -1.0, 0.0)),
        h(-PART_R, 0.0, -10.0, Point3::new(1.0, 0.0, 0.0)),
    ]
}

fn op() -> Operation {
    Operation {
        tool: "DR-0400".into(),
        src: SrcRef::Entity("bar".into()),
        params: OpParams::Drill {
            break_through: None,
            break_through_from: Default::default(),
            clearance_z: Height::fixed(25.0),
            retract_z: None,
            top_z: Height::fixed(20.0),
            bottom_z: None,
            cycle: CycleChoice::Auto,
            peck: 0.0,
            dwell: 0.0,
            feed: 120.0,
            plunge_feed: 80.0,
            holes: vec![],
            cross: holes(),
        },
        stamp: None,
        setup: None,
        area: Default::default(),
        coolant: Default::default(),
        after: Default::default(),
        suppressed: false,
    }
}

/// A world on `word`'s machine with the finished r10 bar and `stock` as
/// the setup's raw material (`None`: no stock declared).
fn world_with(word: &str, stock: Option<(StockShape, f64)>) -> (World, History) {
    world_on(word, stock, (0.0, 0.0), |_| {})
}

/// `world_with` with the bar (and its stock) centred on `at`, and the
/// catalog DR-0400 re-ground by `grind` (a wider drill, a fatter shank).
fn world_on(word: &str, stock: Option<(StockShape, f64)>, at: (f64, f64), grind: impl Fn(&mut Tool)) -> (World, History) {
    world_part(word, stock, at, grind, bar(PART_R, &holes()))
}

/// `world_on` with `part` (centred on the world origin) for the bar.
fn world_part(
    word: &str,
    stock: Option<(StockShape, f64)>,
    at: (f64, f64),
    grind: impl Fn(&mut Tool),
    part: Mesh,
) -> (World, History) {
    let world = new_world();
    let mut history = History::default();
    if let Some((shape, z0)) = stock {
        let at = Location { pos: Point3::new(at.0, at.1, z0), rot: IDENTITY_Q };
        history.execute(&world, SetStock::new(&world, shape, at).expect("stock"));
    }
    let m = Machine { model: format!("TestBench {word}"), kind: Some(word.into()), axes: rotaries(), ..Default::default() };
    history.execute(&world, LoadMachine::new(&world, "bench.mch".into(), Some(m)).expect("machine"));
    let mut tool = catalog().into_iter().find(|(n, _)| *n == "DR-0400").map(|(_, t)| t).expect("catalog entry");
    // A long-series flute (issue #84's reach law): the mouths on the raw
    // bar stand 16-24 mm above their floors, past the jobber's 12 mm.
    tool.flute_len = 30.0;
    grind(&mut tool);
    history.execute(&world, AddTool::new(&world, "DR-0400", tool).expect("catalog tool"));
    let shift = |p: Point3| Point3::new(p.x + at.0, p.y + at.1, p.z);
    let moved = part.facets.iter().map(|t| Triangle::new(shift(t.a), shift(t.b), shift(t.c))).collect();
    history.execute(&world, ImportModel::from_mesh("bar", Mesh::from_facets(moved)));
    (world, history)
}

/// A live-tooling turret that states the A and B tilts the radial
/// poses ask for - so the machine-limits clause passes and the post
/// gate's verdict on the rapids is the collision laws' own.
fn rotaries() -> Vec<AxisLimits> {
    let r = |name: &str, axis: [f64; 3]| AxisLimits { name: name.into(), rotary: true, axis: Some(axis), ..Default::default() };
    vec![r("A", [1.0, 0.0, 0.0]), r("B", [0.0, 1.0, 0.0]), r("C", [0.0, 0.0, 1.0])]
}

/// The raw r20 bar over the part's own Z span.
fn raw_bar() -> Option<(StockShape, f64)> {
    Some((StockShape::Cylinder { diameter: 2.0 * STOCK_R, height: 40.0 }, -20.0))
}

/// Closest approach of a segment to the spindle (world Z) in XY.
fn approach(a: Point3, b: Point3) -> f64 {
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    let ll = dx * dx + dy * dy;
    let t = if ll <= 0.0 { 0.0 } else { (-(a.x * dx + a.y * dy) / ll).clamp(0.0, 1.0) };
    (a.x + t * dx).hypot(a.y + t * dy)
}

/// The smallest radius any rapid OUT of a kerf reaches: rapids that
/// follow a cut (the retract up the tool's own hole) are excused, as
/// are the swings above the stock's end face (z >= 20).
fn rapid_floor(t: &Toolpath) -> f64 {
    let mut in_kerf = false;
    let mut floor = f64::INFINITY;
    for m in &t.moves {
        if m.kind != MoveKind::Rapid {
            in_kerf = true;
            continue;
        }
        if in_kerf {
            in_kerf = false; // the retract to the feed-entry point
            continue;
        }
        if m.from.z.min(m.to.z) >= 20.0 {
            continue;
        }
        floor = floor.min(approach(m.from, m.to));
    }
    floor
}

/// Before #77 this chain started and approached at r15 and chorded the
/// station change 15 mm from the spindle - 5 mm inside the r20 raw bar -
/// and a mill-turn with its A/B rotaries stated POSTED it (the rapid-in-
/// stock law stands down in `Mode::Radial`). Now every rapid stands off
/// the raw stock: start and approach at r25, the chord at r25.
#[test]
fn a_cross_drill_on_raw_stock_wider_than_the_part_hops_outside_the_stock() {
    for word in ["lathe", "mill-turn"] {
        let (world, _h) = world_with(word, raw_bar());
        let (t, tw) = operation::generate(&world, &op(), "")
            .unwrap_or_else(|e| panic!("{word}: the raw-stock cross drill generates: {e}"));
        assert!(!tw.iter().any(|w| w.blocks_post()), "{word}: a clear raw-stock cross drill is convicted: {tw:?}");
        let floor = rapid_floor(&t);
        let first = t.moves.first().map(|m| m.from.x.hypot(m.from.y)).unwrap_or(0.0);
        // The drill's rim (d/2 = 2), not only its centre line, stays out.
        assert!(
            floor >= STOCK_R + 2.0 - 1e-9,
            "{word}: a rapid reaches r {floor:.3}, the r2 rim inside the r{STOCK_R} raw stock"
        );
        assert!(first >= STOCK_R + 5.0 - 1e-9, "{word}: the clearance stands off the stock: {first:.3}");
    }
}

#[test]
fn a_cross_drill_with_the_stock_equal_to_the_part_drills_the_part_only_chain() {
    for word in ["lathe", "mill-turn"] {
        let (world, _h) = world_with(word, None);
        let (plain, pw) = operation::generate(&world, &op(), "")
            .unwrap_or_else(|e| panic!("{word}: the part-only cross drill generates: {e}"));
        // Issue #80: the clear chain is not convicted on either machine.
        assert!(!pw.iter().any(|w| w.blocks_post()), "{word}: a clear cross drill is convicted: {pw:?}");
        // The stock modeled as the part itself (stock from model).
        let (world, mut history) = world_with(word, None);
        let same = SetStock::new(&world, StockShape::FromModel { source: "bar".into() }, Location::default());
        history.execute(&world, same.expect("stock from the part"));
        let (t, _) = operation::generate(&world, &op(), "")
            .unwrap_or_else(|e| panic!("{word}: the stock==part cross drill generates: {e}"));
        assert_eq!(plain.moves.len(), t.moves.len(), "{word}: the same chain");
        for (i, (p, q)) in plain.moves.iter().zip(&t.moves).enumerate() {
            assert!(
                p.from == q.from && p.to == q.to && p.axis == q.axis && p.kind == q.kind,
                "{word}: move {i} reads {q:?}, the part-only chain {p:?}"
            );
        }
        assert!(rapid_floor(&t) >= PART_R - 1e-9, "{word}: the part-only chain hops outside the part");
    }
}

#[test]
fn a_station_change_with_mouths_off_their_centre_lines_hops_clear() {
    // #81: two mouths on the r10 face off their stations' centre lines.
    // The old quarter-turn chord law assumed mouths ON the station axes,
    // so this chord passed 10.757 mm from the spindle - the r2 drill's
    // rim 1.24 mm inside the bar - and #77's rapid proof refused it.
    // The kernel now measures the chord between the real hop points and
    // raises the hop until it clears: the pair drills, nothing blocks the
    // post, and no rapid brings the rim inside the bar.
    let h = |x: f64, y: f64, dir: Point3| CrossHole {
        origin: Point3::new(x, y, 0.0),
        dir,
        radius: 2.0,
        entry_d: 0.0,
        floor_d: -3.0,
        through: false,
    };
    let pair = vec![h(8.0, -6.0, Point3::new(-1.0, 0.0, 0.0)), h(-6.0, 8.0, Point3::new(0.0, -1.0, 0.0))];
    for word in ["lathe", "mill-turn"] {
        // The pair modelled in the bar (the #117 part law reads the model).
        let (world, _h) = world_part(word, None, (0.0, 0.0), |_| {}, bar(PART_R, &pair));
        let mut o = op();
        if let OpParams::Drill { cross, .. } = &mut o.params {
            *cross = pair.clone();
        }
        let (t, w) = operation::generate(&world, &o, "")
            .unwrap_or_else(|e| panic!("{word}: the off-centre pair must hop clear, not refuse: {e}"));
        assert!(!w.iter().any(|w| w.blocks_post()), "{word}: the off-centre pair is convicted: {w:?}");
        let floor = rapid_floor(&t);
        assert!(
            floor >= PART_R + 2.0 - 1e-9,
            "{word}: a rapid reaches r {floor:.3}, the r2 rim inside the r{PART_R} bar"
        );
    }
}

#[test]
fn re_sizing_the_raw_stock_stales_the_cross_drill_and_the_post_refuses_it() {
    use camcraft_app::commands::{CreateOperation, GenerateOperation};
    use camcraft_app::model;
    use camcraft_app::operation::OpStatus;
    use camcraft_app::posting;
    let (world, mut history) = world_with("mill-turn", raw_bar());
    let o = op();
    let create = CreateOperation::new(&world, "CD1", "DR-0400", o.src.clone(), o.params.clone());
    history.execute(&world, create.expect("create"));
    history.execute(&world, GenerateOperation::new(&world, "CD1").expect("generate"));
    let status = || operation::status_of(&world, &model::find_by_name(&world, "CD1").expect("op"));
    assert_eq!(status(), OpStatus::Ok, "the raw-stock cross drill generates fresh");
    let path = std::env::temp_dir().join(format!("camcraft-77-{}.nc", std::process::id()));
    let posted = posting::post_setup(&world, &path);
    let _ = std::fs::remove_file(&path);
    assert!(posted.is_ok(), "the clear chain posts: {:?}", posted.err());
    // A wider raw bar after generation: the cached hops stand inside it.
    let wider = Location { pos: Point3::new(0.0, 0.0, -20.0), rot: IDENTITY_Q };
    let set = SetStock::new(&world, StockShape::Cylinder { diameter: 60.0, height: 40.0 }, wider);
    history.execute(&world, set.expect("wider stock"));
    assert_eq!(status(), OpStatus::Stale, "the stock the standoffs read moved");
    let posted = posting::post_setup(&world, &path);
    let _ = std::fs::remove_file(&path);
    let e = posted.err().map(|e| e.to_string()).unwrap_or_default();
    assert!(
        e.contains("CD1") && e.contains("inputs moved since generation"),
        "a cross drill whose stock moved never posts - refused as stale: {e:?}"
    );
}

/// One hole on `dir`'s station at `origin`, 3 mm deep.
fn one(origin: Point3, dir: Point3) -> CrossHole {
    CrossHole { origin, dir, radius: 3.0, entry_d: 0.0, floor_d: -3.0, through: false }
}

/// `op()` with `cross` and the clearance plane `clear` mm above Top Z.
fn op_with(cross: Vec<CrossHole>, clear: f64) -> Operation {
    let mut o = op();
    if let OpParams::Drill { cross: c, clearance_z, .. } = &mut o.params {
        *c = cross;
        *clearance_z = Height::fixed(20.0 + clear);
    }
    o
}

/// An off-centre hole's drill edge nearest the axis meets the raw bar
/// before its centre line does. r10 part, r20 raw, the hole 9 mm off
/// its station's centre line, a 6 mm drill, the feed-entry standoff
/// at its 1 mm floor: before the review fix the mouth stood where the
/// CENTRE line meets r20 (x 17.86), the rapid ended at x 18.86 and the
/// drill's inner edge (18.86, 6) stood at r19.79 - 0.2 mm inside the
/// raw bar at rapid speed. Now the mouth is where the edge meets it.
#[test]
fn an_off_centre_hole_s_drill_edge_stays_outside_the_raw_stock_on_the_approach() {
    let hole = one(Point3::new((PART_R * PART_R - 81.0).sqrt(), 9.0, 0.0), Point3::new(-1.0, 0.0, 0.0));
    for word in ["lathe", "mill-turn"] {
        // The r20 raw bar sets every standoff; the hole (y 6..12) would
        // be a notch in the r10 bar, so the part is an r5 core it never
        // reaches (issue #117) - the same chain.
        let grind = |t: &mut Tool| {
            t.diameter = 6.0;
            t.shank_d = 6.0;
        };
        let (world, _h) = world_part(word, raw_bar(), (0.0, 0.0), grind, bar(5.0, &[]));
        let (t, _) = operation::generate(&world, &op_with(vec![hole], 1.0), "")
            .unwrap_or_else(|e| panic!("{word}: the off-centre hole drills: {e}"));
        let entry = t.moves.iter().position(|m| m.kind == MoveKind::Plunge).map(|i| t.moves[i].from);
        let entry = entry.unwrap_or_else(|| panic!("{word}: the chain plunges"));
        let edge = entry.x.hypot(entry.y.abs() - 3.0);
        assert!(edge >= STOCK_R - 1e-9, "{word}: the drill's edge stands at r{edge:.3} inside the r{STOCK_R} raw bar");
    }
}

/// A 3 mm drill on a 6 mm shank swinging over the end face between
/// opposite stations at the minimum clearance: the planner's margin
/// (d/2 + 0.1) put the swing 1.6 mm above the stock's top - the shank,
/// lying across the end face, 1.4 mm into it. The swing now clears the
/// shank's own rim.
#[test]
fn the_end_face_swing_clears_a_shank_wider_than_the_drill() {
    let cross = vec![
        one(Point3::new(PART_R, 0.0, 0.0), Point3::new(-1.0, 0.0, 0.0)),
        one(Point3::new(-PART_R, 0.0, 0.0), Point3::new(1.0, 0.0, 0.0)),
    ];
    let cross: Vec<CrossHole> = cross.into_iter().map(|h| CrossHole { radius: 1.5, ..h }).collect();
    for (word, stock) in [("lathe", None), ("mill-turn", None), ("lathe", raw_bar()), ("mill-turn", raw_bar())] {
        let grind = |t: &mut Tool| {
            t.diameter = 3.0;
            t.shank_d = 6.0;
        };
        let (world, _h) = world_part(word, stock, (0.0, 0.0), grind, bar(PART_R, &cross));
        let (t, _) = operation::generate(&world, &op_with(cross.clone(), 0.0), "")
            .unwrap_or_else(|e| panic!("{word}: the opposite pair drills: {e}"));
        let swing = t.moves.iter().filter(|m| m.kind == MoveKind::Rapid && m.from.z.min(m.to.z) > 20.0);
        let low = swing.map(|m| m.from.z.min(m.to.z)).fold(f64::INFINITY, f64::min);
        assert!(low.is_finite(), "{word}: the opposite pair swings over the end face");
        assert!(low >= 20.0 + 3.0 - 1e-9, "{word}: the swing at z {low:.3} puts the r3 shank into the z20 end face");
    }
}

/// A box of raw stock hops about its circumscribed radius (40 x 40:
/// r28.28) and enters each hole at its face (r20, issue #85); a bar off
/// the world origin with G54 moved onto it and a wider r30 raw bar
/// reads that bar about the spindle. Every rapid off a hole's line
/// stands off the raw stock plus the drill's rim, and every approach
/// along one stops the feed-entry standoff (5) outside the stock's
/// surface on that line, about the spindle the post turns.
#[test]
fn a_box_stock_and_an_off_origin_spindle_stand_off_the_raw_stock_plus_the_rim() {
    use camcraft_app::commands::SetWcsOrigin;
    let boxed = Some((StockShape::Box { dx: 40.0, dy: 40.0, dz: 40.0 }, -20.0));
    let wide = Some((StockShape::Cylinder { diameter: 60.0, height: 40.0 }, -20.0));
    let at = (37.0, -12.0);
    for word in ["lathe", "mill-turn"] {
        for (stock, (ox, oy), raw, face) in [(boxed.clone(), (0.0, 0.0), 800.0_f64.sqrt(), 20.0), (wide.clone(), at, 30.0, 30.0)] {
            let (world, mut history) = world_on(word, stock, (ox, oy), |_| {});
            let g54 = Location { pos: Point3::new(ox, oy, 0.0), rot: IDENTITY_Q };
            history.execute(&world, SetWcsOrigin::new(&world, g54).expect("set G54"));
            let mut o = op();
            if let OpParams::Drill { cross, .. } = &mut o.params {
                for h in cross.iter_mut() {
                    h.origin = Point3::new(h.origin.x + ox, h.origin.y + oy, h.origin.z);
                }
            }
            let (t, _) = operation::generate(&world, &o, "")
                .unwrap_or_else(|e| panic!("{word} raw r{raw:.2}: the cross drill generates: {e}"));
            let rel = |p: Point3| Point3::new(p.x - ox, p.y - oy, p.z);
            let mut local = t.clone();
            for m in &mut local.moves {
                (m.from, m.to) = (rel(m.from), rel(m.to));
            }
            let mut hops = local.clone();
            let along = |m: &camcraft_app::cl::Move| {
                let same = |a: f64, b: f64| (a - b).abs() <= 1e-9;
                same(m.from.z, m.to.z) && (same(m.from.x, m.to.x) != same(m.from.y, m.to.y))
            };
            hops.moves.retain(|m| m.kind != MoveKind::Rapid || !along(m));
            let floor = rapid_floor(&hops);
            for m in local.moves.iter().filter(|m| m.kind == MoveKind::Plunge && m.from.z == m.to.z) {
                let at = m.from.x.abs().max(m.from.y.abs());
                assert!(at >= face + 5.0 - 1e-9, "{word} raw r{raw:.2}: a feed entry at r{at:.3} inside the face {face}");
            }
            assert!(floor >= raw + 2.0 - 1e-9, "{word} raw r{raw:.2}: a rapid reaches r{floor:.3}");
            let first = local.moves.first().map(|m| m.from.x.hypot(m.from.y)).unwrap_or(0.0);
            assert!(first >= raw + 5.0 - 1e-9, "{word} raw r{raw:.2}: the clearance stands off the stock: {first:.3}");
        }
    }
}

#[test]
fn a_hole_pointing_away_from_the_spindle_on_raw_stock_is_refused_by_name() {
    let away = one(Point3::new(PART_R, 0.0, 0.0), Point3::new(1.0, 0.0, 0.0));
    for word in ["lathe", "mill-turn"] {
        let (world, _h) = world_with(word, raw_bar());
        let e = operation::generate(&world, &op_with(vec![away], 5.0), "").err().map(|e| e.to_string());
        let e = e.unwrap_or_else(|| panic!("{word}: a hole plunging outward drilled"));
        assert!(e.contains("away from the spindle"), "{word}: the refusal names it: {e}");
    }
}
