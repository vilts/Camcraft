//! A cross drill into a BOX of raw stock enters at the box's face, not
//! at its circumscribed (corner) radius (issue #85): each hole's mouth
//! is where its own line - the drill's body along it - first meets the
//! stock mesh, so a square bar drills no air above the face, its Auto
//! pecks and its reach depth (#84) are read from the face, and a valid
//! hole is not falsely refused. The station-change hops still swing
//! around the corners, and a round bar keeps its ordered numeric chain.
//!
//! Gate map (M4.4): covers app::operation (cross_stock's per-hole entry
//! and rapid proof, cross_spindle, drill_reach's cross_reach).
//! Lane shape (M4.4): all tests here run in seconds.
//!
//! Provenance: our own fixtures (a revolved r10 bar, box and cylinder
//! raw stock); no vendor material.

mod punched_bar;
mod cross_chain_reference;

use flecs_ecs::prelude::World;

use camcraft_app::cl::{MoveKind, Toolpath};
use camcraft_app::commands::{AddTool, CreateOperation, GenerateOperation, History, ImportModel, LoadMachine, SetStock};
use camcraft_app::drilling::{CrossHole, CycleChoice};
use camcraft_app::geometry::{IDENTITY_Q, Mesh, Point3, Triangle, cube};
use camcraft_app::machine::{AxisLimits, Machine};
use camcraft_app::operation::{self, Height, OpParams, SrcRef};
use camcraft_app::schema::{Location, Operation};
use camcraft_app::stock::StockShape;
use camcraft_app::tools::{Tool, catalog};
use camcraft_app::world::new_world;

const TAU: f64 = std::f64::consts::TAU;
const PART_R: f64 = 10.0;
/// The feed-entry standoff the op's planes give (clearance 25 - top 20).
const R_D: f64 = 5.0;

/// A closed bar of radius `r`, z in [-20, 20], revolved about world Z.
fn bar(r: f64) -> Mesh {
    let segs = 64;
    let v = |j: usize, r: f64, z: f64| {
        let th = TAU * j as f64 / segs as f64;
        Point3::new(r * th.cos(), r * th.sin(), z)
    };
    let mut f = Vec::new();
    for j in 0..segs {
        let (a1, a2) = (v(j, r, -20.0), v(j + 1, r, -20.0));
        let (b1, b2) = (v(j, r, 20.0), v(j + 1, r, 20.0));
        f.push(Triangle::new(a1, a2, b2));
        f.push(Triangle::new(a1, b2, b1));
        f.push(Triangle::new(Point3::new(0.0, 0.0, -20.0), a2, a1));
        f.push(Triangle::new(Point3::new(0.0, 0.0, 20.0), b1, b2));
    }
    Mesh::from_facets(f)
}

/// The finished r10 bar with this suite's on-line holes modelled
/// (issue #117: the part law refuses a hole the model lacks): +X at z0
/// 18 deep (the shallower +X holes ride inside it) and +Y at z0 3 deep.
fn drilled() -> Mesh {
    punched_bar::punched_bar(
        PART_R,
        punched_bar::FOOT,
        punched_bar::TOP,
        &[
            hole(Point3::new(PART_R, 0.0, 0.0), Point3::new(-1.0, 0.0, 0.0), -18.0),
            hole(Point3::new(0.0, PART_R, 0.0), Point3::new(0.0, -1.0, 0.0), -3.0),
        ],
    )
}

fn hole(origin: Point3, dir: Point3, floor_d: f64) -> CrossHole {
    CrossHole { origin, dir, radius: 2.0, entry_d: 0.0, floor_d, through: false }
}

fn op(cross: Vec<CrossHole>) -> Operation {
    Operation {
        tool: "DR-0400".into(),
        src: SrcRef::Entity("bar".into()),
        params: OpParams::Drill {
            break_through: None,
            clearance_z: Height::fixed(20.0 + R_D),
            retract_z: None,
            top_z: Height::fixed(20.0),
            bottom_z: None,
            cycle: CycleChoice::Auto,
            peck: 0.0,
            dwell: 0.0,
            feed: 120.0,
            plunge_feed: 80.0,
            holes: vec![],
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

fn rotaries() -> Vec<AxisLimits> {
    let r = |name: &str, axis: [f64; 3]| AxisLimits { name: name.into(), rotary: true, axis: Some(axis), ..Default::default() };
    vec![r("A", [1.0, 0.0, 0.0]), r("B", [0.0, 1.0, 0.0]), r("C", [0.0, 0.0, 1.0])]
}

fn square(side: f64) -> StockShape {
    StockShape::Box { dx: side, dy: side, dz: 40.0 }
}

/// The drilled r10 bar on `word`'s machine with `stock` (z -20..20)
/// and the catalog DR-0400 re-ground by `grind`.
fn world_on(word: &str, stock: StockShape, grind: impl Fn(&mut Tool)) -> (World, History) {
    world_with(word, stock, drilled(), grind)
}

/// `world_on` with `part` in place of the r10 bar.
fn world_with(word: &str, stock: StockShape, part: Mesh, grind: impl Fn(&mut Tool)) -> (World, History) {
    let world = new_world();
    let mut history = History::default();
    let at = Location { pos: Point3::new(0.0, 0.0, -20.0), rot: IDENTITY_Q };
    history.execute(&world, SetStock::new(&world, stock, at).expect("stock"));
    let m = Machine { model: format!("TestBench {word}"), kind: Some(word.into()), axes: rotaries(), ..Default::default() };
    history.execute(&world, LoadMachine::new(&world, "bench.mch".into(), Some(m)).expect("machine"));
    let mut tool = catalog().into_iter().find(|(n, _)| *n == "DR-0400").map(|(_, t)| t).expect("catalog entry");
    tool.flute_len = 30.0;
    grind(&mut tool);
    history.execute(&world, AddTool::new(&world, "DR-0400", tool).expect("catalog tool"));
    history.execute(&world, ImportModel::from_mesh("bar", part));
    (world, history)
}

fn first_plunge(t: &Toolpath) -> Option<(Point3, Point3)> {
    t.moves.iter().find(|m| m.kind == MoveKind::Plunge).map(|m| (m.from, m.to))
}

fn deepest(t: &Toolpath) -> f64 {
    t.moves.iter().filter(|m| m.kind == MoveKind::Plunge).map(|m| m.to.x).fold(f64::INFINITY, f64::min)
}

/// Before #85 a 44 mm square bar read as r31.11 (its corner radius):
/// the hole, 30 mm deep from the face (r22 to x -8), read 39.1 mm deep
/// and a 35 mm flute was refused. It reads 30 mm and posts.
#[test]
fn a_30_deep_hole_from_the_face_of_a_44_square_bar_posts_on_a_35_flute() {
    use camcraft_app::{model, operation::OpStatus, posting};
    let cross = vec![hole(Point3::new(PART_R, 0.0, 0.0), Point3::new(-1.0, 0.0, 0.0), -18.0)];
    for word in ["lathe", "mill-turn"] {
        let (world, mut history) = world_on(word, square(44.0), |t| t.flute_len = 35.0);
        let o = op(cross.clone());
        let (t, _) = operation::generate(&world, &o, "")
            .unwrap_or_else(|e| panic!("{word}: the 30 mm hole from the face drills on a 35 mm flute: {e}"));
        let (from, _) = first_plunge(&t).unwrap_or_else(|| panic!("{word}: the chain plunges"));
        assert!((from.x - (22.0 + R_D)).abs() < 1e-6, "{word}: the feed entry stands {R_D} off the r22 face: {from:?}");
        assert!((deepest(&t) + 8.0).abs() < 1e-6, "{word}: the floor stays at x -8");
        let create = CreateOperation::new(&world, "CD1", "DR-0400", o.src.clone(), o.params.clone());
        history.execute(&world, create.expect("create"));
        history.execute(&world, GenerateOperation::new(&world, "CD1").expect("generate"));
        let op_e = model::find_by_name(&world, "CD1").expect("op");
        assert_eq!(operation::status_of(&world, &op_e), OpStatus::Ok, "{word}: generates fresh");
        if word == "mill-turn" {
            let path = std::env::temp_dir().join(format!("camcraft-85-{}.nc", std::process::id()));
            let posted = posting::post_setup(&world, &path);
            let _ = std::fs::remove_file(&path);
            assert!(posted.is_ok(), "{word}: the valid square-bar hole posts: {:?}", posted.err());
        }
    }
}

/// A 60 x 60 box: the mouth sits at the face (r30 plus the feed-entry
/// standoff), exactly where a d60 round bar's mouth does on that line -
/// not at the corner radius r42.43.
#[test]
fn a_60_box_mouth_sits_at_the_face_not_the_corner_radius() {
    let cross = vec![hole(Point3::new(PART_R, 0.0, 0.0), Point3::new(-1.0, 0.0, 0.0), -3.0)];
    for word in ["lathe", "mill-turn"] {
        let (world, _h) = world_on(word, square(60.0), |_| {});
        let (t, _) = operation::generate(&world, &op(cross.clone()), "")
            .unwrap_or_else(|e| panic!("{word}: the box cross drill generates: {e}"));
        let (from, _) = first_plunge(&t).unwrap_or_else(|| panic!("{word}: the chain plunges"));
        assert!((from.x - (30.0 + R_D)).abs() < 1e-6, "{word}: the mouth stands at the r30 face: {from:?}");
        let (world, _h) = world_on(word, StockShape::Cylinder { diameter: 60.0, height: 40.0 }, |_| {});
        let (round, _) = operation::generate(&world, &op(cross.clone()), "").expect("the round bar drills");
        assert_eq!(first_plunge(&round).map(|p| p.0), Some(from), "{word}: the box face and the d60 bar enter alike");
    }
}

/// An off-centre hole on a box face: 7 mm off its station's centre
/// line with a 6 mm drill - the drill's edge band (y 4..10) lies on the
/// flat x = 30 face, so the mouth is the face, not the corner circle's
/// r42.24 at the edge.
#[test]
fn an_off_centre_hole_on_a_box_face_enters_at_the_face() {
    let cross = vec![hole(Point3::new((PART_R * PART_R - 49.0).sqrt(), 7.0, 0.0), Point3::new(-1.0, 0.0, 0.0), -3.0)];
    for word in ["lathe", "mill-turn"] {
        // The r30 face sets every standoff; the part is an r3.5 core
        // the hole (y 4..10, tip x 4.14) never reaches - a hole the
        // r10 bar would have to carry as a notch (issue #117).
        let (world, _h) = world_with(word, square(60.0), bar(3.5), |t| {
            t.diameter = 6.0;
            t.shank_d = 6.0;
        });
        let (t, _) = operation::generate(&world, &op(cross.clone()), "")
            .unwrap_or_else(|e| panic!("{word}: the off-centre box hole drills: {e}"));
        let (from, _) = first_plunge(&t).unwrap_or_else(|| panic!("{word}: the chain plunges"));
        assert!((from.x - (30.0 + R_D)).abs() < 1e-6, "{word}: the off-centre mouth stands at the face: {from:?}");
        assert!((from.y - 7.0).abs() < 1e-9, "{word}: on its own line");
    }
}

/// Closest approach of a segment to the spindle in XY.
fn approach(a: Point3, b: Point3) -> f64 {
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    let ll = dx * dx + dy * dy;
    let t = if ll <= 0.0 { 0.0 } else { (-(a.x * dx + a.y * dy) / ll).clamp(0.0, 1.0) };
    (a.x + t * dx).hypot(a.y + t * dy)
}

/// A quarter-turn station change on a 60 box: the mouths move in to the
/// faces, but every rapid that is not along its own hole's line (the
/// hop chord that swings round the bar) keeps the drill's rim outside
/// the box's corner radius r42.43 - the corners are still there.
#[test]
fn the_station_change_hop_still_clears_the_box_corners() {
    let cross = vec![
        hole(Point3::new(PART_R, 0.0, 0.0), Point3::new(-1.0, 0.0, 0.0), -3.0),
        hole(Point3::new(0.0, PART_R, 0.0), Point3::new(0.0, -1.0, 0.0), -3.0),
    ];
    let corner = 1800.0_f64.sqrt();
    for word in ["lathe", "mill-turn"] {
        let (world, _h) = world_on(word, square(60.0), |_| {});
        let (t, _) = operation::generate(&world, &op(cross.clone()), "")
            .unwrap_or_else(|e| panic!("{word}: the box station change drills: {e}"));
        let mut hops = 0;
        for m in t.moves.iter().filter(|m| m.kind == MoveKind::Rapid) {
            let along = (m.from.x - m.to.x).abs() < 1e-9 || (m.from.y - m.to.y).abs() < 1e-9;
            if along {
                continue;
            }
            hops += 1;
            let got = approach(m.from, m.to);
            assert!(got >= corner + 2.0 - 1e-9, "{word}: the hop passes r{got:.3}, the rim inside the corners");
        }
        assert!(hops >= 1, "{word}: the chain swings between stations");
    }
}

/// The three-station and off-centre chains captured before #151,
/// compared with a coordinate tolerance rather than raw float bits.
#[test]
fn a_cylinder_stock_chain_matches_its_numeric_reference() {
    let three = vec![
        hole(Point3::new(PART_R, 0.0, 10.0), Point3::new(-1.0, 0.0, 0.0), -6.0),
        hole(Point3::new(0.0, PART_R, 0.0), Point3::new(0.0, -1.0, 0.0), -6.0),
        hole(Point3::new(-PART_R, 0.0, -10.0), Point3::new(1.0, 0.0, 0.0), -6.0),
    ];
    let off = vec![hole(Point3::new((PART_R * PART_R - 81.0).sqrt(), 9.0, 0.0), Point3::new(-1.0, 0.0, 0.0), -3.0)];
    // The d40 raw bar sets every standoff, so the part only has to
    // carry the holes (issue #117): the three are modelled in the r10
    // bar; the off-centre one (y 7..11) would be a notch in it, so it
    // runs over an r6 core it never reaches - the same chain.
    let parts = [punched_bar::punched_bar(PART_R, punched_bar::FOOT, punched_bar::TOP, &three), bar(6.0)];
    for (case, (cross, part)) in [three, off].into_iter().zip(parts).enumerate() {
        let (world, _h) = world_with("mill-turn", StockShape::Cylinder { diameter: 40.0, height: 40.0 }, part, |_| {});
        let (t, _) = operation::generate(&world, &op(cross), "").expect("the round bar drills");
        cross_chain_reference::assert_chain(&t, case);
    }
}

/// The drilled r10 bar with a block (a modelling error) poking out
/// through the face of a 40 box (r20) to x `out`, over y `y0..y1`, z
/// -3..3.
fn bar_with_lump(out: f64, y0: f64, y1: f64) -> Mesh {
    let mut f = drilled().facets.to_vec();
    f.extend(cube(Point3::new(8.0, y0, -3.0), Point3::new(out, y1, 3.0)).facets.iter().copied());
    Mesh::from_facets(f)
}

/// The drilled r10 bar with four lumps out to x `out` in the CORNERS
/// of the drill's square body band about the +X line (|y|, |z| in
/// 1.6..3): inside the band the entry reads, outside the round drill
/// (1.6 * sqrt 2 > 2), so the hole drills clear of the part (#117).
fn bar_with_corner_lumps(out: f64) -> Mesh {
    let mut f = drilled().facets.to_vec();
    for (sy, sz) in [(1.0, 1.0), (1.0, -1.0), (-1.0, 1.0), (-1.0, -1.0)] {
        let (y0, y1) = if sy > 0.0 { (1.6, 3.0) } else { (-3.0, -1.6) };
        let (z0, z1) = if sz > 0.0 { (1.6, 3.0) } else { (-3.0, -1.6) };
        f.extend(cube(Point3::new(10.5, y0, z0), Point3::new(out, y1, z1)).facets.iter().copied());
    }
    Mesh::from_facets(f)
}

/// The approach rapids along the hole's own line (y 0, z 0, along X)
/// before the first cut - after it, a peck's rapids ride its own kerf.
fn along_line(t: &Toolpath) -> Vec<(Point3, Point3)> {
    let on = |p: Point3| p.y.abs() < 1e-9 && p.z.abs() < 1e-9;
    t.moves
        .iter()
        .take_while(|m| m.kind == MoveKind::Rapid)
        .filter(|m| on(m.from) && on(m.to))
        .map(|m| (m.from, m.to))
        .collect()
}

/// A part poking out of a 40 box's face to x 26 on the hole's line:
/// before, the per-line entry read the stock mesh alone - the mouth at
/// the r20 face and the feed-entry rapid down to x 25, inside the part.
/// The entry is the outer of the stock's and the part's crossings, so
/// every rapid along the line stays outside the part. Mill-turn only: a
/// lathe refuses the lumpy part as not turned about one axis.
#[test]
fn a_part_poking_out_of_the_box_face_still_bounds_the_along_line_rapids() {
    let cross = vec![hole(Point3::new(PART_R, 0.0, 0.0), Point3::new(-1.0, 0.0, 0.0), -3.0)];
    for word in ["mill-turn"] {
        let (world, _h) = world_with(word, square(40.0), bar_with_corner_lumps(26.0), |_| {});
        let (t, _) = operation::generate(&world, &op(cross.clone()), "")
            .unwrap_or_else(|e| panic!("{word}: the cross drill generates: {e}"));
        let rapids = along_line(&t);
        assert!(!rapids.is_empty(), "{word}: the chain approaches along the line");
        for (a, b) in rapids {
            let tip = a.x.min(b.x);
            assert!(tip >= 26.0 - 1e-6, "{word}: a rapid along the line reaches x {tip:.3}, inside the part (x 26)");
        }
    }
}

/// The shank branch of the along-line proof on box stock: a d4 drill on
/// a d12 shank, a part lump out to x 27.5 lying only in the shank's band
/// (y 3..5.5). The drill's mouth stays at the r20 face (its own band
/// meets only the face), the shank's reach is read off the lump, and
/// the chain is proven clear - the shank behind the flutes stands
/// outside the lump on every rapid along the line - and posts.
#[test]
fn a_wide_shank_on_box_stock_is_proven_clear_along_the_line() {
    use camcraft_app::posting;
    let cross = vec![hole(Point3::new(PART_R, 0.0, 0.0), Point3::new(-1.0, 0.0, 0.0), -3.0)];
    let (world, mut history) = world_with("mill-turn", square(40.0), bar_with_lump(27.5, 3.0, 5.5), |t| {
        t.shank_d = 12.0;
    });
    let o = op(cross);
    let (t, _) = operation::generate(&world, &o, "").unwrap_or_else(|e| panic!("the wide-shank box hole drills: {e}"));
    let (from, _) = first_plunge(&t).unwrap_or_else(|| panic!("the chain plunges"));
    assert!(from.x > 20.0 + R_D - 1e-6 && from.x < 27.5, "the drill's mouth is the face, not the shank's lump: {from:?}");
    for (a, b) in along_line(&t) {
        let shank = a.x.min(b.x) + 30.0;
        assert!(shank >= 27.5, "the shank stands at x {shank:.3}, inside the lump");
    }
    let create = CreateOperation::new(&world, "CD1", "DR-0400", o.src.clone(), o.params.clone());
    history.execute(&world, create.expect("create"));
    history.execute(&world, GenerateOperation::new(&world, "CD1").expect("generate"));
    let path = std::env::temp_dir().join(format!("camcraft-85-shank-{}.nc", std::process::id()));
    let posted = posting::post_setup(&world, &path);
    let _ = std::fs::remove_file(&path);
    assert!(posted.is_ok(), "the wide-shank box hole posts: {:?}", posted.err());
}
