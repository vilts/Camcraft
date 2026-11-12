//! Split from `drilling.rs` by M5.0.1; structure only.

use crate::cl::{Cycle, CycleKind, MoveKind, Toolpath};
use crate::geo2d::Point2;
use crate::geometry::{Point3, Triangle};
use crate::machining2d::GenError;
use crate::tools::{Tool, ToolKind};
use super::*;

/// Vertical walls of an XY ring, z in [z0, z1]. Ring order decides
/// the facing: CCW rings face outward (a plate, a boss), CW rings
/// face inward (a hole's wall in an outward-wound solid).
pub(super) fn ring_walls(poly: &[(f64, f64)], z0: f64, z1: f64) -> Vec<Triangle> {
    let mut f = Vec::new();
    let n = poly.len();
    for i in 0..n {
        let (x0, y0) = poly[i];
        let (x1, y1) = poly[(i + 1) % n];
        let (a, b) = (Point3::new(x0, y0, z0), Point3::new(x1, y1, z0));
        let (c, d) = (Point3::new(x1, y1, z1), Point3::new(x0, y0, z1));
        f.push(Triangle::new(a, b, c));
        f.push(Triangle::new(a, c, d));
    }
    f
}

pub(super) fn circle(cx: f64, cy: f64, r: f64, n: usize) -> Vec<(f64, f64)> {
    (0..n)
        .map(|i| {
            let a = std::f64::consts::TAU * (i as f64) / (n as f64);
            (cx + r * a.cos(), cy + r * a.sin())
        })
        .collect()
}

pub(super) fn circle_cw(cx: f64, cy: f64, r: f64, n: usize) -> Vec<(f64, f64)> {
    let mut p = circle(cx, cy, r, n);
    p.reverse();
    p
}

fn rect(x0: f64, y0: f64, x1: f64, y1: f64) -> Vec<(f64, f64)> {
    vec![(x0, y0), (x1, y0), (x1, y1), (x0, y1)]
}

/// A 20x20x10 plate's walls plus `extra` void/collar walls.
fn plate(extra: Vec<Triangle>) -> Vec<Triangle> {
    let mut t = ring_walls(&rect(0.0, 0.0, 20.0, 20.0), 0.0, 10.0);
    t.extend(extra);
    t
}

pub(super) fn drill_tool(d: f64) -> Tool {
    Tool {
        kind: ToolKind::Drill,
        diameter: d,
        corner_r: 0.0,
        tip_angle: 118.0,
        pitch: 0.0,
        nose_radius: 0.0,
        width: 0.0,
        flute_len: 40.0,
        shank_d: d,
        shank_len: 40.0,
        holder_d: 44.0,
        holder_len: 35.0,
        material_group: "aluminum".into(), center_cutting: true,
        rpm: 10_000.0,
        feed: 120.0,
        tool_material: None,
        flutes: None,
        orientation: None,
        turning: None,
    }
}

fn hole(xy: (f64, f64), r: f64, entry: f64, floor: f64, through: bool) -> Hole {
    Hole {
        xy: Point2::new(xy.0, xy.1),
        radius: r,
        entry_z: entry,
        floor_z: floor,
        through,
        stepped: false,
    }
}

fn params<'a>(tool: &'a Tool, holes: &'a [Hole]) -> CycleParams<'a> {
    CycleParams {
        clearance_z: 15.0,
        r_z: 10.0,
        hop_z: None,
        bottom_z: None,
        cycle: CycleChoice::Auto,
        peck: 0.0,
        dwell: 0.0,
        feed: 500.0,
        plunge_feed: 100.0,
        tool,
        holes,
    }
}

// ---- recognition

#[test]
fn recognize_finds_through_and_blind_holes() {
    let mut t = plate(ring_walls(&circle_cw(10.0, 10.0, 2.0, 16), 0.0, 10.0));
    t.extend(ring_walls(&circle_cw(5.0, 5.0, 1.5, 16), 4.0, 10.0));
    let holes = recognize(&t);
    assert_eq!(holes.len(), 2, "{holes:?}");
    let through = holes.iter().find(|h| (h.xy.x - 10.0).abs() < 1e-6).unwrap();
    assert!(through.through);
    assert!((through.radius - 2.0).abs() < 1e-6);
    assert_eq!(through.entry_z, 10.0);
    assert_eq!(through.floor_z, 0.0);
    let blind = holes.iter().find(|h| (h.xy.x - 5.0).abs() < 1e-6).unwrap();
    assert!(!blind.through);
    assert_eq!(blind.floor_z, 4.0);
}

#[test]
fn recognize_refuses_walls_bosses_and_slots() {
    // The plate's own outer wall: straight, normals face away.
    assert!(recognize(&plate(Vec::new())).is_empty(), "outer wall is not a hole");
    // A raised boss: closed round wall, normals face away.
    let mut m = ring_walls(&rect(-20.0, -20.0, 20.0, 20.0), 0.0, 2.0);
    m.extend(ring_walls(&circle(0.0, 0.0, 5.0, 32), 2.0, 8.0)); // CCW = outward-facing
    assert!(recognize(&m).is_empty(), "a boss is not a hole");
    // A racetrack slot: a void, but its vertices fit no circle.
    let mut track: Vec<(f64, f64)> = (1..16)
        .map(|i| {
            let a = -std::f64::consts::PI / 2.0 + std::f64::consts::PI * (i as f64) / 16.0;
            (13.0 + 3.0 * a.cos(), 10.0 + 3.0 * a.sin())
        })
        .collect();
    track.extend((1..16).map(|i| {
        let a = std::f64::consts::PI / 2.0 + std::f64::consts::PI * (i as f64) / 16.0;
        (7.0 + 3.0 * a.cos(), 10.0 + 3.0 * a.sin())
    }));
    let mut m2 = plate(ring_walls(&rect(0.0, 0.0, 40.0, 20.0), 0.0, 10.0));
    m2.extend(ring_walls(&track, 0.0, 10.0));
    assert!(recognize(&m2).is_empty(), "a slot is not a drilled hole");
}

#[test]
fn recognize_merges_a_counterbore() {
    // A wider bore stepped above the main wall: one axis, two z-spans.
    let mut m = plate(ring_walls(&circle_cw(10.0, 10.0, 2.0, 24), 0.0, 8.0));
    m.extend(ring_walls(&circle_cw(10.0, 10.0, 3.0, 24), 8.0, 10.0));
    let h = recognize(&m);
    assert_eq!(h.len(), 1, "counterbore is one hole: {h:?}");
    assert!((h[0].radius - 2.0).abs() < 1e-6, "the narrow wall is the bore");
    assert_eq!(h[0].entry_z, 10.0);
    assert_eq!(h[0].floor_z, 0.0);
    assert!(h[0].through);
}

#[test]
fn recognize_is_stable_under_vertex_noise() {
    let base = plate(ring_walls(&circle_cw(10.0, 10.0, 2.0, 16), 0.0, 10.0));
    let noisy = base
        .iter()
        .map(|t| {
            let jit = |p: Point3| {
                let k = p.x.to_bits() ^ (p.y.to_bits() << 1) ^ (p.z.to_bits() >> 3);
                let e = ((k & 0xFFFF) as f64 / 65535.0 - 0.5) * 2e-7;
                Point3::new(p.x + e, p.y + e, p.z)
            };
            Triangle::new(jit(t.a), jit(t.b), jit(t.c))
        })
        .collect::<Vec<_>>();
    let (a, b) = (recognize(&base), recognize(&noisy));
    assert_eq!(a.len(), b.len());
    for (x, y) in a.iter().zip(&b) {
        assert!((x.xy.x - y.xy.x).abs() < 1e-4 && (x.xy.y - y.xy.y).abs() < 1e-4);
        assert!((x.radius - y.radius).abs() < 1e-4);
    }
}

#[test]
fn recognize_degenerate_input_never_panics() {
    assert!(recognize(&[]).is_empty());
    let nan = Triangle::new(
        Point3::new(f64::NAN, 0.0, 0.0),
        Point3::new(1.0, 0.0, 0.0),
        Point3::new(0.0, 1.0, 0.0),
    );
    assert!(recognize(&[nan]).is_empty());
    let mut t = plate(ring_walls(&circle_cw(10.0, 10.0, 2.0, 16), 0.0, 10.0));
    for x in t.iter_mut() {
        x.b = x.a; // collapse every triangle to zero area
    }
    assert!(recognize(&t).is_empty(), "zero-area facets must drop, not divide");
}

// ---- generator

fn one(t: &Toolpath) -> &Cycle {
    assert_eq!(t.cycles.len(), 1);
    &t.cycles[0]
}

#[test]
fn agreement_moves_and_cycles_describe_the_same_holes() {
    // The law this module is named for: per cycle, from first_move
    // the moves that stay on the hole's XY are its group, the group
    // opens with a cut, and its deepest point is bottom_z.
    let tool = drill_tool(4.0);
    let holes = vec![
        hole((5.0, 5.0), 2.0, 10.0, 8.0, false),
        hole((15.0, 15.0), 2.0, 10.0, 0.0, true),
    ];
    let t = drill(&params(&tool, &holes)).unwrap();
    assert_eq!(t.cycles.len(), 2);
    assert!(t.is_chained() && t.all_finite());
    for (i, c) in t.cycles.iter().enumerate() {
        let end = t.cycles.get(i + 1).map_or(t.moves.len(), |n| n.first_move);
        let on_hole: Vec<&crate::cl::Move> = t.moves[c.first_move..end]
            .iter()
            .take_while(|m| (m.to.x - c.xy.x).abs() < 1e-9 && (m.to.y - c.xy.y).abs() < 1e-9)
            .collect();
        assert!(!on_hole.is_empty(), "cycle {i} has no moves at its XY");
        assert_ne!(on_hole[0].kind, MoveKind::Rapid, "cycle must open with a cut");
        let deepest = on_hole.iter().map(|m| m.to.z).fold(f64::INFINITY, f64::min);
        assert!((deepest - c.bottom_z).abs() < 1e-9, "cycle {i}: {deepest} vs {}", c.bottom_z);
        assert!((c.entry_z - 10.0).abs() < 1e-9);
    }
}

#[test]
fn auto_cycle_picks_by_depth() {
    let tool = drill_tool(4.0);
    let cases = [
        (hole((10.0, 10.0), 2.0, 10.0, 7.0, false), CycleKind::Drill),
        (hole((10.0, 10.0), 2.0, 10.0, 3.0, false), CycleKind::Peck),
        (hole((10.0, 10.0), 2.0, 10.0, -10.0, false), CycleKind::Deep),
    ];
    for (h, want) in cases {
        let t = drill(&params(&tool, &[h])).unwrap();
        assert_eq!(one(&t).kind, want, "depth {} on d 4", 10.0 - h.floor_z);
    }
}

#[test]
fn deep_peck_retracts_to_r_between_pecks() {
    let tool = drill_tool(2.0);
    // Depth 9 > 4d: the Deep branch of the auto law.
    let t = drill(&params(&tool, &[hole((10.0, 10.0), 1.0, 10.0, 1.0, false)])).unwrap();
    let c = one(&t);
    assert_eq!(c.kind, CycleKind::Deep);
    assert!((c.peck - 2.0).abs() < 1e-9, "auto G83 peck = 1d");
    assert_eq!(c.dwell, 0.0);
    let plunges = t.moves[c.first_move..]
        .iter()
        .take_while(|m| (m.to.x - 10.0).abs() < 1e-9)
        .filter(|m| m.kind == MoveKind::Plunge)
        .count();
    assert_eq!(plunges, 5, "ceil(9 / peck 2)");
    // Full returns to R between pecks: rapids up to exactly 10.0.
    let ups = t.moves[c.first_move..]
        .iter()
        .filter(|m| m.kind == MoveKind::Rapid && m.to.z == 10.0 && m.from.z < 10.0)
        .count();
    assert!(ups >= 3, "G83 retracts to R between the four pecks: {ups}");
}

#[test]
fn peck_hops_only_chip_break() {
    let tool = drill_tool(2.0);
    let hs = [hole((10.0, 10.0), 1.0, 10.0, 2.0, false)];
    let mut p = params(&tool, &hs);
    p.cycle = CycleChoice::Peck; // depth 8 > 4d would auto-Deep; force the G73 shape
    let t = drill(&p).unwrap();
    let c = one(&t);
    assert_eq!(c.kind, CycleKind::Peck);
    assert!((c.peck - 6.0).abs() < 1e-9, "auto G73 peck = 3d");
    // Hops rise by about CHIP_BREAK, not to R: no rapid reaches 10.0
    // from below except the cycle's own final retract.
    let to_r = t.moves[c.first_move..]
        .iter()
        .filter(|m| m.kind == MoveKind::Rapid && m.to.z == 10.0 && m.from.z < 10.0)
        .count();
    assert!(to_r <= 2, "G73 does not fully retract mid-cycle: {to_r}");
}

#[test]
fn tap_feeds_at_pitch_times_rpm_both_ways() {
    let mut tool = drill_tool(6.0);
    tool.kind = ToolKind::Tap;
    tool.pitch = 1.0;
    tool.rpm = 500.0;
    let t = drill(&params(&tool, &[hole((10.0, 10.0), 3.0, 10.0, 8.0, false)])).unwrap();
    let c = one(&t);
    assert_eq!(c.kind, CycleKind::Tap);
    assert_eq!(c.dwell, 0.0);
    let cut: Vec<&crate::cl::Move> = t.moves[c.first_move..]
        .iter()
        .take_while(|m| m.kind == MoveKind::Feed)
        .collect();
    assert_eq!(cut.len(), 2, "down and out, both feeds");
    assert!(cut.iter().all(|m| m.feed == Some(500.0)), "{:?}", cut.iter().map(|m| m.feed).collect::<Vec<_>>());
    // A pitchless tap cannot have its feed derived: refuse.
    tool.pitch = 0.0;
    assert!(
        matches!(drill(&params(&tool, &[hole((10.0, 10.0), 3.0, 10.0, 8.0, false)])), Err(GenError::Invalid(_))),
        "tap with no pitch must error"
    );
}

#[test]
fn bore_backbore_and_spot_shapes() {
    let mut tool = drill_tool(8.0);
    tool.kind = ToolKind::Bore;
    let h = hole((10.0, 10.0), 4.0, 10.0, 4.0, false);
    let t = drill(&params(&tool, &[h])).unwrap();
    assert_eq!(one(&t).kind, CycleKind::Bore);
    let g = &t.moves[one(&t).first_move..];
    assert_eq!(g[0].kind, MoveKind::Plunge, "feed in");
    assert_eq!(g[1].kind, MoveKind::Rapid, "rapid out");

    let hs = [h];
    let mut p = params(&tool, &hs);
    p.cycle = CycleChoice::BackBore;
    let t = drill(&p).unwrap();
    let g = &t.moves[one(&t).first_move..];
    assert!(g.iter().any(|m| m.kind == MoveKind::Feed && m.to.z > m.from.z), "back-bore retracts on a feed");

    let emt = drill_tool(4.0);
    let mut p = params(&emt, &hs);
    p.cycle = CycleChoice::Spot;
    p.dwell = 1.5;
    let t = drill(&p).unwrap();
    let c = one(&t);
    assert_eq!(c.kind, CycleKind::Spot);
    assert!((c.bottom_z - 8.0).abs() < 1e-9, "spot at r_z - SPOT_DEPTH");
    assert!((c.dwell - 1.5).abs() < 1e-9, "spot honors the dwell word");
    // A spot never digs through a floor standing in its way:
    let shallower = [hole((10.0, 10.0), 4.0, 10.0, 9.0, false)];
    let mut p2 = params(&emt, &shallower);
    p2.cycle = CycleChoice::Spot;
    let t2 = drill(&p2).unwrap();
    assert!((one(&t2).bottom_z - 9.0).abs() < 1e-9, "spot stops at the floor");
}

#[test]
fn bore_door_refuses_a_tool_that_will_not_enter() {
    let mut tool = drill_tool(8.0); // the slide's full cutting dia.
    tool.kind = ToolKind::Bore;
    let wall = [hole((10.0, 10.0), 3.0, 10.0, 4.0, false)]; // a dia. 6 hole
    let e = drill(&params(&tool, &wall)).unwrap_err();
    assert!(format!("{e:?}").contains("will not enter"), "{e:?}");
    // Inside the FIT_TOL band is snap noise, not a wrong tool: silent.
    let snug = [hole((10.0, 10.0), 3.9995, 10.0, 4.0, false)];
    assert!(drill(&params(&tool, &snug)).is_ok(), "the fit band stays silent");
    // Auto routes a Bore-kind tool to the same cycle, so the door reads
    // the answer, not just the user's word.
    let mut p = params(&tool, &wall);
    p.cycle = CycleChoice::Auto;
    assert!(drill(&p).is_err(), "the Auto answer reads the tool; the door reads the answer");
}

#[test]
fn bore_refuses_a_stepped_hole_a_drill_does_not() {
    // A folded counterbore: the narrow record hides a real ring, and a
    // bore cycle would machine the bore through and miss it - refused
    // by name, per the Prime Directive's unfinished over silently-wrong.
    let mut tool = drill_tool(8.0);
    tool.kind = ToolKind::Bore;
    let counter = [Hole {
        stepped: true,
        ..hole((10.0, 10.0), 4.0, 10.0, 2.0, false)
    }];
    let e = drill(&params(&tool, &counter)).unwrap_err();
    assert!(format!("{e:?}").contains("counterbore"), "{e:?}");
    // The very same hole list drills honest: the ring is another
    // operation's promise (recognize.rs's merge law, M5.0 words).
    let emt = drill_tool(8.0);
    assert!(drill(&params(&emt, &counter)).is_ok(), "a drill cycle owes nothing to the ring");
}

#[test]
fn spot_dims_the_mouth_under_the_r_plane() {
    // A hole standing on a pocket floor, its mouth 4.0 under the R
    // plane: SPOT_DEPTH hangs off the ENTRY plane (hole.rs), so the
    // dimple sits on the mouth - measuring it off R would spot the air
    // above the floor and call it a cycle (M8.3).
    let tool = drill_tool(4.0);
    let pocketed = [hole((10.0, 10.0), 2.0, 6.0, 0.0, false)];
    let mut p = params(&tool, &pocketed);
    p.cycle = CycleChoice::Spot;
    let t = drill(&p).unwrap();
    let c = one(&t);
    assert!((c.bottom_z - 4.0).abs() < 1e-9, "2.0 below the mouth at 6.0");
}

#[test]
fn through_exit_clears_the_floor() {
    let tool = drill_tool(4.0);
    let t = drill(&params(&tool, &[hole((10.0, 10.0), 2.0, 10.0, 0.0, true)])).unwrap();
    let c = one(&t);
    assert!(c.bottom_z < 0.0);
    assert!((c.bottom_z + THROUGH_CLEAR).abs() < 1e-9);
    assert_eq!(c.entry_z, 10.0);
}

#[test]
fn depth_override_applies_to_every_hole() {
    let tool = drill_tool(4.0);
    let holes = vec![hole((5.0, 5.0), 2.0, 10.0, 8.0, false), hole((15.0, 5.0), 2.0, 10.0, 2.0, false)];
    let mut p = params(&tool, &holes);
    p.bottom_z = Some(-1.0);
    let t = drill(&p).unwrap();
    assert!(t.cycles.iter().all(|c| (c.bottom_z + 1.0).abs() < 1e-9));
    // An override at or above the R plane is nonsense, refused - and
    // the refusal points at the first hole's mouth (#261 part 2).
    p.bottom_z = Some(10.0);
    let e = drill(&p).expect_err("refused");
    assert!(matches!(e, GenError::At { .. }), "{e:?}");
    assert_eq!(e.location(), Some(crate::geometry::Point3::new(5.0, 5.0, 10.0)));
}

#[test]
fn empty_and_malformed_lists_error_never_panic() {
    let tool = drill_tool(4.0);
    assert!(matches!(drill(&params(&tool, &[])), Err(GenError::Empty(_))));
    assert!(matches!(
        drill(&params(&tool, &[hole((10.0, 10.0), 2.0, f64::NAN, 8.0, false)])),
        Err(GenError::Invalid(_))
    ));
    assert!(matches!(
        drill(&params(&tool, &[hole((10.0, 10.0), 2.0, 8.0, 8.0, false)])),
        Err(GenError::Invalid(_))
    ));
}

#[test]
fn chain_stays_contiguous_over_many_holes() {
    let tool = drill_tool(3.0);
    let holes: Vec<Hole> = (0..20)
        .map(|i| hole((2.0 + i as f64 * 0.9, 10.0), 1.5, 10.0, 9.0, false))
        .collect();
    let t = drill(&params(&tool, &holes)).unwrap();
    assert!(t.is_chained());
    assert_eq!(t.cycles.len(), 20);
    let mut prev = 0;
    for c in &t.cycles {
        assert!(c.first_move >= prev, "cycle groups must be ordered and disjoint");
        prev = c.first_move;
    }
}

#[test]
fn user_peck_overrides_the_auto_step() {
    let tool = drill_tool(2.0); // depth 8, auto Deep peck = 2
    let h = hole((10.0, 10.0), 1.0, 10.0, 2.0, false);
    let hs = [h];
    let mut p = params(&tool, &hs);
    p.peck = 4.0;
    let t = drill(&p).unwrap();
    assert!((one(&t).peck - 4.0).abs() < 1e-9);
    let plunges = t.moves[one(&t).first_move..]
        .iter()
        .take_while(|m| (m.to.x - 10.0).abs() < 1e-9)
        .filter(|m| m.kind == MoveKind::Plunge)
        .count();
    assert_eq!(plunges, 2);
}
