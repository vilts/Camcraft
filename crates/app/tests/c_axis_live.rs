//! Continuous C-axis live tooling (issue #33): a rotary kind on a
//! mill-turn's spindle-as-C generates a path that STATES its C angle
//! on every move (`cl::Move::rotary`), split so the machine's C-linear
//! motion follows each verified part-frame chord; the stream carries
//! the C word and a `PARAM LiveTool`; the Fanuc lathe post writes X
//! (diameter) Z C under G98 with the machine's own engage words, and
//! refuses by name what it cannot voice - a machine without live-tool
//! words, a tip C does not turn onto the station - while a post that
//! does not declare `liveTooling` (every mill post) is refused by the
//! engine. A C-turning rapid on a machine that states no C rate is
//! refused by name too (#966): the bench states one so it posts.
//!
//! Regenerate the golden after an intentional change:
//! `WRITE_GOLDEN=1 cargo test -p camcraft-app --test c_axis_live`
//!
//! Gate map (M4.4): covers camcraft_app::operation (rotary_gate's C
//! stamp), cl::cl (stamp_c), post::post (the dump's
//! C word, the engine's liveTooling refusal, fanuc-lathe.cps).
//! Lane shape (M4.4): all tests here run in seconds.

mod common;

use flecs_ecs::prelude::World;

use camcraft_app::cl::{stamp_c, Move, MoveKind, Toolpath};
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
const MILL_CPS: &str = include_str!("../../../assets/posts/fanuc.cps");

/// Test-bench words - placeholders, not any builder's codes.
fn codes() -> LiveToolCodes {
    LiveToolCodes {
        c_axis_on: "M1001".into(),
        c_axis_off: "M1002".into(),
        spindle_on: "M1003".into(),
        spindle_off: "M1004".into(),
    }
}

fn mill_turn(live: Option<LiveToolCodes>) -> Machine {
    Machine {
        model: "TestBench mill-turn".into(),
        kind: Some("mill-turn".into()),
        spindle_max: Some(6000.0),
        coolants: vec![Coolant { name: "Flood".into(), m_code: "M8".into() }],
        axes: vec![
            AxisLimits { name: "X".into(), feed: Some(4000.0), ..Default::default() },
            AxisLimits { name: "Y".into(), feed: Some(4000.0), ..Default::default() },
            AxisLimits { name: "Z".into(), feed: Some(4000.0), ..Default::default() },
            // A stated C rate (deg/min, the shipped millturn.mch's
            // figures): the post caps every C-turning G01 by it (#966).
            AxisLimits {
                name: "C".into(),
                rotary: true,
                axis: Some([0.0, 0.0, 1.0]),
                feed: Some(C_RATE),
                rapid: Some(2.0 * C_RATE),
                ..Default::default()
            },
        ],
        live_tool: live,
        ..Default::default()
    }
}

const C_RATE: f64 = 6000.0;

/// The bench whose C states neither a travel nor a rate (#966).
fn unrated(live: Option<LiveToolCodes>) -> Machine {
    let mut m = mill_turn(live);
    for a in m.axes.iter_mut().filter(|a| a.rotary) {
        (a.feed, a.rapid) = (None, None);
    }
    m
}

/// The turned bar standing on the spindle: wall r6, shoulder, neck r5.
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

fn world() -> World {
    let world = camcraft_app::world::new_world();
    let mut h = History::default();
    let tool = catalog().into_iter().find(|(n, _)| *n == "BN-0600").map(|(_, t)| t).expect("catalog ball");
    h.execute(&world, AddTool::new(&world, "BN-0600", tool).expect("tool"));
    let stock = SetStock::new(&world, StockShape::Box { dx: 14.0, dy: 14.0, dz: 34.0 }, Location::default());
    h.execute(&world, stock.expect("stock"));
    h.execute(&world, ImportModel::from_mesh("bar", Mesh::from_facets(bar())));
    let cmd = LoadMachine::new(&world, "millturn.mch".into(), Some(mill_turn(Some(codes()))));
    h.execute(&world, cmd.expect("machine"));
    world
}

fn contour() -> Operation {
    Operation {
        tool: "BN-0600".into(),
        src: SrcRef::Entity("bar".into()),
        params: OpParams::RotaryContour {
            tolerance: 0.05,
            clearance_z: Height::fixed(25.0),
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

/// The stream a path posts as, with the context the lathe post reads.
fn stream(path: &Toolpath) -> String {
    let d = post::Dump { op_name: "C wrap", tool_number: 1, tool_name: "BN-0600", tool_diameter: 6.0, rpm: 3000.0, ..Default::default() };
    cnc::serialize(&post::dump(path, &d))
}

/// Rz(C) of the part-frame tip, as the post reads it.
fn on_station(m: &Move) -> bool {
    let Some(c) = m.rotary else { return false };
    let (s, co) = c.to_radians().sin_cos();
    let y = m.to.x * s + m.to.y * co;
    let x = m.to.x * co - m.to.y * s;
    y.abs() < 1e-6 && x > -1e-6
}

#[test]
fn a_rotary_contour_on_the_spindle_states_c_on_every_move_and_posts() {
    let w = world();
    let (path, _warns) = operation::generate(&w, &contour(), "C wrap").expect("the spindle's C holds the standing bar");
    assert!(!path.moves.is_empty());
    assert!(path.moves.iter().all(on_station), "every move turns its tip onto the station");
    // Continuous: no move swings C more than the split law allows a
    // chord at these radii (a jump would be a whole-turn unwrap slip).
    let worst = path
        .moves
        .windows(2)
        .filter_map(|w| Some((w[1].rotary? - w[0].rotary?).abs()))
        .fold(0.0, f64::max);
    assert!(worst < 90.0, "C steps stay continuous, worst {worst}");
    let out = engine::run(LATHE_CPS, &post::machine::mirror(&mill_turn(Some(codes()))), &stream(&path)).expect("runs");
    assert!(out.warnings.is_empty(), "the lathe post voices the C section: {:?}", out.warnings);
    for word in ["G98", "M1001", "M1003 S3000", "M1004", "M1002", "G99"] {
        assert!(out.program.contains(word), "{word} in the program");
    }
    assert!(out.program.lines().any(|l| l.starts_with("G01 X") && l.contains(" C")), "X Z C blocks");
    assert!(!out.program.contains("M03"), "no turning spindle in a C section");
}

/// Every X Z C G01 holds X/Z to `feed` and C to `c_rate` (deg/min),
/// read back from its words and F the way the control drives them.
fn assert_feeds_within(program: &str, feed: f64, c_rate: f64) {
    let word = |l: &str, k: char| l.split(' ').find_map(|w| w.strip_prefix(k).and_then(|v| v.parse::<f64>().ok()));
    let mut at: Option<(f64, f64, f64)> = None;
    let mut turned = 0;
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
                turned += usize::from(dc.abs() > 1e-9);
            }
        }
        let prev = at.unwrap_or((0.0, 0.0, 0.0));
        if x.is_some() || z.is_some() || c.is_some() {
            at = Some((x.map_or(prev.0, |x| x / 2.0), z.unwrap_or(prev.1), c.unwrap_or(prev.2)));
        }
    }
    assert!(turned > 0, "the program carries C-turning blocks to judge");
}

/// #966: on a machine that states no C travel and no C rate, the
/// contour's C-turning link rapids are refused by name - never posted
/// at the max linear feed scaled by the C turn. The same path on the
/// bench that states its C rate posts with every F inside it.
#[test]
fn a_c_turning_rapid_without_a_stated_c_rate_is_refused_by_name() {
    let w = world();
    let (path, _warns) = operation::generate(&w, &contour(), "C wrap").expect("generates; posting is what refuses");
    let rapids_turn = path.moves.windows(2).any(|p| p[1].kind == MoveKind::Rapid && p[0].rotary != p[1].rotary);
    assert!(rapids_turn, "the contour links by C-turning rapids");
    let bare = engine::run(LATHE_CPS, &post::machine::mirror(&unrated(Some(codes()))), &stream(&path)).expect("runs");
    let named = "a C-axis rapid that turns C, but the machine file states no C rate";
    assert!(bare.warnings.iter().any(|w| w.contains(named)), "refused by name: {:?}", bare.warnings);
    let rated = engine::run(LATHE_CPS, &post::machine::mirror(&mill_turn(Some(codes()))), &stream(&path)).expect("runs");
    assert!(rated.warnings.is_empty(), "{:?}", rated.warnings);
    assert_feeds_within(&rated.program, 4000.0, C_RATE);
}

/// One and a half turns of a wrapped helix at r 10, 2 mm a turn,
/// posed radially: the smallest honest C-continuous path.
fn helix() -> Toolpath {
    let at = |k: usize| {
        let th = (k as f64 * 30.0).to_radians();
        Point3::new(10.0 * th.cos(), 10.0 * th.sin(), -(k as f64) * 2.0 / 12.0)
    };
    let radial = |p: Point3| [p.x / 10.0, p.y / 10.0, 0.0];
    let start = Point3::new(20.0, 0.0, 0.0);
    let mut moves = vec![
        Move::new(MoveKind::Rapid, start, Point3::new(12.0, 0.0, 0.0), None).with_axis([1.0, 0.0, 0.0]),
        Move::new(MoveKind::Feed, Point3::new(12.0, 0.0, 0.0), at(0), Some(200.0)).with_axis([1.0, 0.0, 0.0]),
    ];
    for k in 0..18 {
        moves.push(Move::new(MoveKind::Feed, at(k), at(k + 1), Some(400.0)).with_axis(radial(at(k + 1))));
    }
    let end = at(18);
    let out = Point3::new(end.x * 2.0, end.y * 2.0, end.z);
    moves.push(Move::new(MoveKind::Rapid, end, out, None).with_axis(radial(end)));
    Toolpath::new(moves)
}

#[test]
fn the_wrapped_helix_posts_to_its_golden() {
    let path = stamp_c(&helix(), [0.0, 0.0], 0.01).expect("the helix stamps");
    let last = path.moves.last().and_then(|m| m.rotary);
    assert!(last.is_some_and(|c| (c + 540.0).abs() < 1e-6), "one and a half turns read -540: {last:?}");
    let out = engine::run(LATHE_CPS, &post::machine::mirror(&mill_turn(Some(codes()))), &stream(&path)).expect("runs");
    assert!(out.warnings.is_empty(), "{:?}", out.warnings);
    common::write_golden("nc/fanuc-lathe/c_axis_helix.nc", &out.program);
}

#[test]
fn every_post_without_c_or_without_the_machines_words_refuses_by_name() {
    let path = stamp_c(&helix(), [0.0, 0.0], 0.01).expect("the helix stamps");
    let text = stream(&path);
    // A mill post: the engine refuses the section for want of liveTooling.
    let mill = engine::run(MILL_CPS, &post::machine::mirror(&mill_turn(Some(codes()))), &text).expect("runs");
    assert!(mill.warnings.iter().any(|w| w.contains("liveTooling")), "{:?}", mill.warnings);
    // The lathe post on a machine that states no live-tool words.
    let bare = engine::run(LATHE_CPS, &post::machine::mirror(&mill_turn(None)), &text).expect("runs");
    assert!(bare.warnings.iter().any(|w| w.contains("live_tool words")), "{:?}", bare.warnings);
    // A C that does not turn the tip onto the station: refused, never projected.
    let mut bad = path.clone();
    if let Some(m) = bad.moves.get_mut(5) {
        m.rotary = m.rotary.map(|c| c + 10.0);
    }
    let out = engine::run(LATHE_CPS, &post::machine::mirror(&mill_turn(Some(codes()))), &stream(&bad)).expect("runs");
    assert!(out.warnings.iter().any(|w| w.contains("live-tool station")), "{:?}", out.warnings);
}

#[test]
fn the_c_word_rides_the_stream_and_parses_back() {
    let path = stamp_c(&helix(), [0.0, 0.0], 0.01).expect("the helix stamps");
    let recs = cnc::parse(&stream(&path)).expect("parses");
    assert!(recs.iter().any(|r| matches!(r, cnc::Record::Param { name, .. } if name == "LiveTool")));
    let cs: Vec<f64> = recs
        .iter()
        .filter_map(|r| match r {
            cnc::Record::Move(m) => m.abc.map(|[_, _, c]| c),
            _ => None,
        })
        .collect();
    assert_eq!(cs.len(), path.moves.len(), "every move carries its C");
    // A path with no C writes no LiveTool and no letters - byte-shape
    // identical to every stream before #33.
    let plain = Toolpath::new(vec![Move::new(MoveKind::Feed, Point3::new(0.0, 0.0, 0.0), Point3::new(1.0, 0.0, 0.0), Some(100.0))]);
    let t = stream(&plain);
    assert!(!t.contains("LiveTool") && !t.contains(" A 0 B 0 C "), "{t}");
}
