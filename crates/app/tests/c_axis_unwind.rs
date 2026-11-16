//! Continuous C-axis live tooling, slice 2 (issue #33): a C-stamped
//! path on a machine that STATES its C travel is unwound by whole turns
//! at its clear links (`cl::unwind_c`), the rewinds written as orbit
//! rapids the verifier and the post both see; a cut whose own C span no
//! rewind can fit inside the travel is refused by name at generation.
//! A machine stating no C travel keeps the path exactly as stamped.
//!
//! Regenerate the golden after an intentional change:
//! `WRITE_GOLDEN=1 cargo test -p camcraft-app --test c_axis_unwind`
//!
//! Gate map (M4.4): covers camcraft_app::operation (rotary_gate's C
//! stamp and unwind), cl::cl (unwind_c), post::post
//! (fanuc-lathe.cps voicing the orbit).
//! Lane shape (M4.4): all tests here run in seconds.

mod common;

use flecs_ecs::prelude::World;

use camcraft_app::cl::{stamp_c, unwind_c, Move, MoveKind, Toolpath};
use camcraft_app::commands::{AddTool, History, ImportModel, LoadMachine, SetStock};
use camcraft_app::geometry::{Mesh, Point3, Triangle};
use camcraft_app::machine::{AxisLimits, Coolant, LiveToolCodes, Machine};
use camcraft_app::operation::{self, Height, OpParams, SrcRef};
use camcraft_app::post::{self, cnc, engine};
use camcraft_app::schema::{Location, Operation};
use camcraft_app::stock::StockShape;
use camcraft_app::tools::catalog;

const TAU: f64 = std::f64::consts::TAU;
const LATHE_CPS: &str = include_str!("../../../assets/posts/fanuc-lathe.cps");

/// Test-bench words - placeholders, not any builder's codes.
fn codes() -> LiveToolCodes {
    LiveToolCodes {
        c_axis_on: "M1001".into(),
        c_axis_off: "M1002".into(),
        spindle_on: "M1003".into(),
        spindle_off: "M1004".into(),
    }
}

/// A mill-turn whose C states `travel` (None: unstated), with the
/// shipped millturn.mch's rates: 4000 mm/min on X/Y/Z, C 6000 deg/min
/// normal and 12000 rapid.
fn mill_turn(travel: Option<(f64, f64)>) -> Machine {
    rated(travel, 4000.0, 6000.0)
}

/// The same bench with its own linear feed and C rate (deg/min).
fn rated(travel: Option<(f64, f64)>, feed: f64, c_rate: f64) -> Machine {
    Machine {
        model: "TestBench mill-turn".into(),
        kind: Some("mill-turn".into()),
        spindle_max: Some(6000.0),
        coolants: vec![Coolant { name: "Flood".into(), m_code: "M8".into() }],
        axes: vec![
            AxisLimits { name: "X".into(), feed: Some(feed), ..Default::default() },
            AxisLimits { name: "Y".into(), feed: Some(feed), ..Default::default() },
            AxisLimits { name: "Z".into(), feed: Some(feed), ..Default::default() },
            AxisLimits {
                name: "C".into(),
                rotary: true,
                axis: Some([0.0, 0.0, 1.0]),
                travel,
                feed: Some(c_rate),
                rapid: Some(c_rate * 2.0),
                ..Default::default()
            },
        ],
        live_tool: Some(codes()),
        ..Default::default()
    }
}

/// Every C block's axis rates under its composite F, read back from
/// the posted text: X (diameter) and Z drive at F * lin / lcmd, C at
/// F * |dC| / lcmd - none may pass what the machine states.
fn assert_feeds_within(program: &str, feed: f64, c_rate: f64) {
    let word = |l: &str, k: char| l.split(' ').find_map(|w| w.strip_prefix(k).and_then(|v| v.parse::<f64>().ok()));
    let mut at: Option<(f64, f64, f64)> = None;
    let mut blocks = 0;
    for l in program.lines() {
        let (x, z, c) = (word(l, 'X'), word(l, 'Z'), word(l, 'C'));
        if l.starts_with("G01")
            && let (Some(x), Some(z), Some(c), Some(f), Some((r0, z0, c0))) = (x, z, c, word(l, 'F'), at)
        {
            let (dr, dz, dc) = (x / 2.0 - r0, z - z0, c - c0);
            let lcmd = (dr * dr + dz * dz + dc * dc).sqrt();
            if lcmd > 1e-9 {
                let lin = (dr * dr + dz * dz).sqrt();
                // One decimal of F and three of each word: a hair of slack.
                assert!(f * lin / lcmd <= feed * 1.001 + 0.1, "linear rate past {feed}: {l}");
                assert!(f * dc.abs() / lcmd <= c_rate * 1.001 + 0.1, "C rate past {c_rate}: {l}");
                blocks += 1;
            }
        }
        let prev = at.unwrap_or((0.0, 0.0, 0.0));
        if x.is_some() || z.is_some() || c.is_some() {
            at = Some((x.map_or(prev.0, |x| x / 2.0), z.unwrap_or(prev.1), c.unwrap_or(prev.2)));
        }
    }
    assert!(blocks > 0, "the program carries C blocks to judge");
}

/// The turned bar standing on the spindle: wall r6, shoulder, neck r5
/// (the c_axis_live fixture).
fn bar() -> Vec<Triangle> {
    const MERIDIAN: [(f64, f64); 6] = [(0.0, -16.0), (6.0, -16.0), (6.0, -2.0), (5.0, 2.0), (5.0, 16.0), (0.0, 16.0)];
    let pt = |rho: f64, s: f64, j: usize| {
        let th = TAU * j as f64 / 16.0;
        Point3::new(rho * th.cos(), rho * th.sin(), s)
    };
    let mut f = Vec::new();
    for w in MERIDIAN.windows(2) {
        let ((ra, za), (rb, zb)) = (w[0], w[1]);
        for j in 0..16 {
            let k = (j + 1) % 16;
            let (a1, a2, b1, b2) = (pt(ra, za, j), pt(ra, za, k), pt(rb, zb, j), pt(rb, zb, k));
            if ra <= 1e-12 {
                f.push(Triangle::new(a1, b2, b1));
            } else if rb <= 1e-12 {
                f.push(Triangle::new(a1, a2, b2));
            } else {
                f.push(Triangle::new(a1, a2, b2));
                f.push(Triangle::new(a1, b2, b1));
            }
        }
    }
    f
}

fn world(travel: Option<(f64, f64)>) -> World {
    world_on(mill_turn(travel))
}

/// The bench whose C states `travel` but no rate at all - neither feed
/// nor rapid (#906).
fn unrated(travel: (f64, f64)) -> Machine {
    let mut m = mill_turn(Some(travel));
    for a in m.axes.iter_mut().filter(|a| a.rotary) {
        (a.feed, a.rapid) = (None, None);
    }
    m
}

fn world_on(machine: Machine) -> World {
    let world = camcraft_app::world::new_world();
    let mut h = History::default();
    let tool = catalog().into_iter().find(|(n, _)| *n == "BN-0600").map(|(_, t)| t).expect("catalog ball");
    h.execute(&world, AddTool::new(&world, "BN-0600", tool).expect("tool"));
    let stock = SetStock::new(&world, StockShape::Box { dx: 14.0, dy: 14.0, dz: 34.0 }, Location::default());
    h.execute(&world, stock.expect("stock"));
    h.execute(&world, ImportModel::from_mesh("bar", Mesh::from_facets(bar())));
    let cmd = LoadMachine::new(&world, "millturn.mch".into(), Some(machine));
    h.execute(&world, cmd.expect("machine"));
    world
}

fn contour() -> Operation {
    Operation {
        tool: "BN-0600".into(),
        src: SrcRef::Entity("bar".into()),
        params: OpParams::RotaryContour {
            tolerance: 0.05,
            // Above the box stock's top, so the clearance plane is air.
            clearance_z: Height::fixed(40.0),
            retract_z: None,
            top_z: Height::fixed(6.0),
            bottom_z: Height::fixed(-8.0),
            feed: 500.0,
            plunge_feed: 100.0,
            climb: true,
            lead_deg: 0.0,
            lag_deg: 0.0,
            max_tilt_deg: 60.0,
        },
        stamp: None,
        setup: None,
        area: Default::default(),
        coolant: Default::default(),
        after: Default::default(),
        suppressed: false,
    }
}

fn c_range(t: &Toolpath) -> (f64, f64) {
    t.moves
        .iter()
        .filter_map(|m| m.rotary)
        .fold((f64::INFINITY, f64::NEG_INFINITY), |(l, h), c| (l.min(c), h.max(c)))
}

/// The stream a path posts as, with the context the lathe post reads.
fn stream(path: &Toolpath) -> String {
    let d = post::Dump { op_name: "C unwind", tool_number: 1, tool_name: "BN-0600", tool_diameter: 6.0, rpm: 3000.0, ..Default::default() };
    cnc::serialize(&post::dump(path, &d))
}

#[test]
fn a_rotary_contour_on_a_stated_c_travel_unwinds_at_its_links_and_posts() {
    // Unstated travel: the rings wind well past one turn, as stamped.
    let (raw, raw_warns) = operation::generate(&world(None), &contour(), "C wrap").expect("the unstated C holds it");
    let (lo, hi) = c_range(&raw);
    assert!(hi - lo > 720.0, "the bar's rings wind past two turns: {lo}..{hi}");
    // Stated -360..360 (the shipped mill-turn's): every C inside it.
    let travel = (-360.0, 360.0);
    let (path, warns) = operation::generate(&world(Some(travel)), &contour(), "C wrap").expect("unwound at its links");
    let (lo, hi) = c_range(&path);
    assert!(lo >= travel.0 - 1e-9 && hi <= travel.1 + 1e-9, "C {lo}..{hi} inside the travel");
    assert!(path.moves.len() > raw.moves.len(), "the rewinds are moves of the path");
    // Every rewind chord is a rapid at the clearance plane, out of the work.
    let top = path.moves.iter().map(|m| m.to.z).fold(f64::NEG_INFINITY, f64::max);
    let orbit: Vec<&Move> = path.moves.iter().filter(|m| m.from.z >= top - 1e-6 && m.to.z >= top - 1e-6).collect();
    assert!(!orbit.is_empty() && orbit.iter().all(|m| m.kind == MoveKind::Rapid), "rewinds ride as clearance rapids");
    for w in path.moves.windows(2) {
        assert_eq!(w[0].to, w[1].from, "the chain holds through the rewinds");
    }
    let worst = path.moves.windows(2).filter_map(|w| Some((w[1].rotary? - w[0].rotary?).abs())).fold(0.0, f64::max);
    assert!(worst < 90.0, "C stays continuous through the rewinds, worst {worst}");
    // The verifier judged the rewinds as moves, in the air above the
    // stock: they add no finding to the stamped path's own (the
    // kernel's two entry rapids, untouched by this law).
    assert_eq!(warns.len(), raw_warns.len(), "the rewinds verify clean: {warns:?} vs {raw_warns:?}");
    let on_orbit = |at: usize| path.moves.get(at).is_some_and(|m| m.from.z >= top - 1e-6 && m.to.z >= top - 1e-6 && m.from != m.to);
    assert!(
        !warns.iter().any(|w| matches!(w, camcraft_app::verify::Warning::RapidInStock { at } if on_orbit(*at))),
        "{warns:?}"
    );
    let out = engine::run(LATHE_CPS, &post::machine::mirror(&mill_turn(Some(travel))), &stream(&path)).expect("runs");
    assert!(out.warnings.is_empty(), "the lathe post voices the unwound section: {:?}", out.warnings);
    assert_feeds_within(&out.program, 4000.0, 6000.0);
}

/// The rewind is judged, never assumed: with the clearance plane left
/// inside the box stock (z 25), the orbit chords are rapids through
/// stock, and the verifier convicts them by index - so the post gate
/// refuses what it would otherwise rewind blind.
#[test]
fn a_rewind_through_stock_is_convicted_by_the_verifier() {
    let mut op = contour();
    if let OpParams::RotaryContour { clearance_z, .. } = &mut op.params {
        *clearance_z = Height::fixed(25.0);
    }
    let (path, warns) = operation::generate(&world(Some((-360.0, 360.0))), &op, "C wrap").expect("generates, judged");
    let orbit_at = |i: usize| path.moves.get(i).is_some_and(|m| m.kind == MoveKind::Rapid && m.from.z >= 25.0 - 1e-6 && m.to.z >= 25.0 - 1e-6 && m.from != m.to);
    let convicted = warns.iter().filter(|w| matches!(w, camcraft_app::verify::Warning::RapidInStock { at } if orbit_at(*at))).count();
    assert!(convicted > 0, "the orbit chords in stock are convicted: {warns:?}");
}

#[test]
fn a_ring_wider_than_the_stated_travel_refuses_by_name() {
    let e = operation::generate(&world(Some((-90.0, 90.0))), &contour(), "C wrap").err();
    let e = e.map(|e| e.to_string()).unwrap_or_default();
    assert!(e.contains("Rotary Contour") && e.contains("no clear link stands inside it"), "{e}");
}

/// Two rings at r 10, linked over a rise to z 5.
fn two_rings() -> Toolpath {
    let at = |k: usize, z: f64| {
        let th = (k as f64 * 45.0).to_radians();
        Point3::new(10.0 * th.cos(), 10.0 * th.sin(), z)
    };
    let mut moves: Vec<Move> = vec![Move::new(MoveKind::Rapid, Point3::new(20.0, 0.0, 5.0), at(0, 5.0), None).with_axis([1.0, 0.0, 0.0])];
    for ring in 0..2 {
        let z = -(ring as f64);
        let top = at(0, 5.0);
        if let Some(end) = moves.last().map(|m| m.to).filter(|_| ring > 0) {
            moves.push(Move::new(MoveKind::Rapid, end, top, None).with_axis([1.0, 0.0, 0.0]));
        }
        moves.push(Move::new(MoveKind::Rapid, top, at(0, z), None).with_axis([1.0, 0.0, 0.0]));
        for k in 0..8 {
            let to = if k == 7 { at(0, z) } else { at(k + 1, z) };
            moves.push(Move::new(MoveKind::Feed, at(k, z), to, Some(400.0)).with_axis([to.x / 10.0, to.y / 10.0, 0.0]));
        }
    }
    let end = moves.last().map_or(at(0, -1.0), |m| m.to);
    moves.push(Move::new(MoveKind::Rapid, end, at(0, 5.0), None).with_axis([1.0, 0.0, 0.0]));
    Toolpath::new(moves)
}

#[test]
fn two_rings_unwound_post_to_their_golden() {
    let stamped = stamp_c(&two_rings(), [0.0, 0.0], 0.05).expect("stamps");
    assert!(c_range(&stamped).0 < -700.0, "two rings read -720 at the end");
    let path = unwind_c(&stamped, [0.0, 0.0], (-360.0, 360.0), 0.05, false).expect("unwinds");
    let (lo, hi) = c_range(&path);
    assert!(lo >= -360.0 && hi <= 360.0, "{lo}..{hi}");
    let out = engine::run(LATHE_CPS, &post::machine::mirror(&mill_turn(Some((-360.0, 360.0)))), &stream(&path)).expect("runs");
    assert!(out.warnings.is_empty(), "{:?}", out.warnings);
    assert_feeds_within(&out.program, 4000.0, 6000.0);
    common::write_golden("nc/fanuc-lathe/c_axis_unwind.nc", &out.program);
}

/// The feed cap follows the machine file: a slower stated C rate and
/// linear feed post slower F words, every block within them.
#[test]
fn a_lower_stated_rate_posts_a_lower_feed() {
    let stamped = stamp_c(&two_rings(), [0.0, 0.0], 0.05).expect("stamps");
    let path = unwind_c(&stamped, [0.0, 0.0], (-360.0, 360.0), 0.05, false).expect("unwinds");
    let max_f = |m: &Machine| {
        let out = engine::run(LATHE_CPS, &post::machine::mirror(m), &stream(&path)).expect("runs");
        assert!(out.warnings.is_empty(), "{:?}", out.warnings);
        let f = out.program.lines().filter_map(|l| l.split(' ').find_map(|w| w.strip_prefix('F')?.parse::<f64>().ok())).fold(0.0, f64::max);
        (f, out.program)
    };
    let (fast, _) = max_f(&mill_turn(Some((-360.0, 360.0))));
    let (slow, text) = max_f(&rated(Some((-360.0, 360.0)), 1000.0, 500.0));
    assert_feeds_within(&text, 1000.0, 500.0);
    assert!(slow < fast, "a slower machine posts slower: {slow} vs {fast}");
    assert!(slow <= 1000.0 + 0.05, "no F past the slower machine's own limits: {slow}");
}

/// #906: a C travel with no C rate has nothing to cap a rewind by, so
/// the unwind is refused by name at generation - never posted at the
/// ~57 x max feed / radius an uncapped orbit would state.
#[test]
fn a_stated_c_travel_without_a_c_rate_refuses_the_rewind_by_name() {
    let e = operation::generate(&world_on(unrated((-360.0, 360.0))), &contour(), "C wrap").err();
    let e = e.map(|e| e.to_string()).unwrap_or_default();
    assert!(e.contains("Rotary Contour") && e.contains("states a C travel but no C rate"), "{e}");
    // A path that needs no rewind is not refused: the unstated rate
    // matters only where a rewind would post.
    let wide = (-100_000.0, 100_000.0);
    assert!(operation::generate(&world_on(unrated(wide)), &contour(), "C wrap").is_ok(), "no rewind, no refusal");
}

/// #906 through the lathe post: an unwound path handed to the post on
/// a machine stating a C travel but no C rate is refused by name - its
/// orbit rapids carry no uncapped F.
#[test]
fn the_lathe_post_refuses_a_c_turning_rapid_without_a_stated_c_rate() {
    let stamped = stamp_c(&two_rings(), [0.0, 0.0], 0.05).expect("stamps");
    let path = unwind_c(&stamped, [0.0, 0.0], (-360.0, 360.0), 0.05, false).expect("unwinds");
    let out = engine::run(LATHE_CPS, &post::machine::mirror(&unrated((-360.0, 360.0))), &stream(&path)).expect("runs");
    assert!(out.warnings.iter().any(|w| w.contains("states a C travel but no C rate")), "refused by name: {:?}", out.warnings);
    // The same path on the rated bench still posts clean (the golden's).
    let rated = engine::run(LATHE_CPS, &post::machine::mirror(&mill_turn(Some((-360.0, 360.0)))), &stream(&path)).expect("runs");
    assert!(rated.warnings.is_empty(), "{:?}", rated.warnings);
}
