//! The Tormach PathPilot (LinuxCNC) LATHE post (#612):
//! `assets/posts/tormach-lathe.cps` posts the turning documents' own
//! streams - profile rough and finish (the tool's control point, #630),
//! the axial drill as plain moves - to
//! byte-identical G-code in inch and mm, and refuses by name what the
//! dialect cannot say (a tap, a back-bore, a hole off the axis, a G76
//! record it cannot state exactly, a tool number outside T01-T99,
//! control comp, a C-axis section). Every program also passes a
//! LinuxCNC syntax sanity check: G7 stated, no G32 / G80 / G81 / G83 /
//! M6 / G43, no O word or % fence, no nested comment, every word from
//! the sets the LinuxCNC reference and Tormach's lathe tables list.
//! Threading (G33 passes, the one-block G76, the hand) is the sibling
//! suite `lathe_post_tormach_thread`; the fixtures are shared from
//! `tormach_lathe_support`.
//!
//! Regenerate after an intentional change:
//! `WRITE_GOLDEN=1 cargo test -p camcraft-app --test lathe_post_tormach`
//!
//! Gate map (M4.4): covers app::posting (the registry, stream_records,
//! post_setup on the Tormach lathe post), post::post (engine on the
//! PathPilot lathe dialect), app::commands (SelectPost / SetPostOptions).
//! Lane shape (M4.4): all tests here run in seconds.

mod common;
mod tormach_lathe_support;

use flecs_ecs::prelude::World;
use serde_json::json;

use camcraft_app::cl::{CycleKind, MoveKind};
use camcraft_app::commands::{History, SelectPost};
use camcraft_app::drilling::{CycleChoice, Hole};
use camcraft_app::geo2d::Point2;
use camcraft_app::machine::Kind;
use camcraft_app::operation::{Height, OpParams};
use camcraft_app::post::{cnc, engine};
use camcraft_app::posting;
use camcraft_app::schema::{PostOptions, PostRef};
use camcraft_app::world::new_world;
use tormach_lathe_support::{create, gated, lathe_mch, linuxcnc_sane, program_with, revolve, world_with, CPS, NAME};

// ------------------------------------------------------------ fixtures

/// The flanged bar's rough and finish (the Fanuc lathe suite's stage B).
fn turning_world(inch: bool) -> (World, History) {
    let flanged = [(0.0, -16.0), (6.0, -16.0), (6.0, 2.0), (3.5, 2.0), (3.5, 16.0), (0.0, 16.0)];
    let stock = camcraft_app::stock::StockShape::Cylinder { diameter: 12.0, height: 32.0 };
    let (world, mut history) = world_with(revolve(&flanged, 16), stock, -16.0, &["TN-1600-0R8"], inch);
    let rough = OpParams::ProfileRough {
        tolerance: 0.05,
        clearance_z: Height::fixed(25.0),
        top_z: Height::fixed(16.0),
        bottom_z: Height::fixed(-16.0),
        stepdown: 0.8,
        stock_to_leave: 0.25,
        feed: 150.0,
        plunge_feed: 80.0,
    };
    let finish = OpParams::ProfileFinish {
        tolerance: 0.05,
        clearance_z: Height::fixed(25.0),
        top_z: Height::fixed(16.0),
        bottom_z: Height::fixed(2.0),
        feed: 120.0,
        plunge_feed: 60.0,
    };
    create(&world, &mut history, "R1", "TN-1600-0R8", rough);
    create(&world, &mut history, "S1", "TN-1600-0R8", finish);
    (world, history)
}

/// The axial drill fixture of the Fanuc lathe suite: a Ø4.1 hole 10 deep
/// on centre.
fn drill_world(inch: bool) -> (World, History) {
    let bar = revolve(&[(0.0, -20.0), (10.0, -20.0), (10.0, 20.0), (2.05, 20.0), (2.05, 10.0), (0.0, 10.0)], 64);
    let stock = camcraft_app::stock::StockShape::Box { dx: 24.0, dy: 24.0, dz: 40.0 };
    let (world, mut history) = world_with(bar, stock, -20.0, &["DR-0400"], inch);
    let hole = Hole { xy: Point2::new(0.0, 0.0), radius: 2.05, entry_z: 20.0, floor_z: 10.0, through: false, stepped: false };
    let params = OpParams::Drill {
        clearance_z: Height::fixed(30.0),
        retract_z: None,
        top_z: Height::fixed(20.0),
        bottom_z: None,
        cycle: CycleChoice::Auto,
        peck: 0.0,
        dwell: 0.0,
        feed: 120.0,
        plunge_feed: 80.0,
        holes: vec![hole],
        cross: vec![],
    };
    create(&world, &mut history, "AX1", "DR-0400", params);
    (world, history)
}

// ------------------------------------------------------------ goldens

#[test]
fn turning_posts_to_the_pathpilot_lathe_goldens_in_mm_and_inch() {
    for (inch, file) in [(false, "turning_mm"), (true, "turning_inch")] {
        let (world, _history) = turning_world(inch);
        let text = program_with(&world, PostOptions::default());
        assert!(text.starts_with("(TestBench PathPilot lathe)\n"), "{text}");
        let unit = if inch { "G20" } else { "G21" };
        assert!(text.contains(&format!("G7 G18 {unit} G90 G91.1 G40 G95\n")), "{text}");
        assert_eq!(text.matches("G97 S3000 M3").count(), 2, "one spindle start per section: {text}");
        assert!(text.contains("G30\nT0101\n"), "G30 then the T word, no M6: {text}");
        if inch {
            // 150 mm/min at 3000 rpm: 0.05 mm/rev = 0.00197 in/rev.
            assert!(text.contains("F0.00197"), "{text}");
        } else {
            assert!(text.contains("F0.05") && text.contains("F0.04"), "{text}");
        }
        assert!(text.ends_with("M5\nG30\nM30\n"), "{text}");
        // #630: the stream is the r0.8 insert's control point for
        // orientation 2 (its imaginary tip), not the nose centre - the
        // finish stands ON the part's Ø7 neck and its Z2 shoulder face
        // (the nose centre would read X8.6 Z2.8: +1.6 on X, +0.8 on Z),
        // and the tool note names the orientation to touch it off by.
        assert_eq!(text.matches("TURNING INSERT ORIENTATION 2)").count(), 2, "{text}");
        let finish = &text[text.find("PROFILE FINISH").expect("the finish")..];
        let neck = if inch { "G1 X0.2756 Z0.0787 " } else { "G1 X7 Z2 " };
        assert!(finish.contains(neck), "the tip on the neck and the shoulder: {finish}");
        assert!(!finish.contains(if inch { "X0.3386" } else { "X8.6 " }), "no nose-centre diameter: {finish}");
        assert_eq!(gated(&world, file), text, "the gated program is the stream's own translation");
        common::write_golden(&format!("nc/tormach-lathe/{file}.nc"), &text);
    }
}

#[test]
fn an_axial_drill_posts_as_plain_moves_in_mm_and_inch() {
    for (inch, file) in [(false, "axial_drill_mm"), (true, "axial_drill_inch")] {
        let (world, _history) = drill_world(inch);
        let text = program_with(&world, PostOptions::default());
        for no in ["G81", "G83", "G80", "G98"] {
            assert!(!text.contains(no), "no canned cycle word on the PathPilot lathe ({no}): {text}");
        }
        let floor = if inch { "G1 X0 Z0.3937 " } else { "G1 X0 Z10 " };
        assert!(text.contains(floor), "the chain feeds to the floor on centre: {text}");
        assert!(text.contains("G97 S10000 M3"), "{text}");
        assert_eq!(gated(&world, file), text);
        common::write_golden(&format!("nc/tormach-lathe/{file}.nc"), &text);
    }
}

// ------------------------------------------------------- synthetic

fn mv(kind: MoveKind, x: f64, y: f64, z: f64, feed: Option<f64>) -> cnc::Record {
    cnc::Record::Move(cnc::MoveRec { kind, x, y, z, feed, center: None, ccw: false, plane: Default::default(), abc: None })
}

fn preface() -> Vec<cnc::Record> {
    vec![
        cnc::Record::Param { name: "SpindleSpeed".into(), value: json!(1000.0) },
        cnc::Record::Param { name: "WorkOffset".into(), value: json!("G54") },
        cnc::Record::Param { name: "OperationName".into(), value: json!("OP1") },
        cnc::Record::Tool { number: 1, name: "TN-1600-0R8".into() },
        cnc::Record::Section { begin: true },
    ]
}

fn run(records: Vec<cnc::Record>) -> engine::PostOutput {
    engine::run(CPS, &lathe_mch(false), &cnc::serialize(&records)).expect("the script runs")
}

fn cycle(kind: CycleKind, y: f64, dwell: f64) -> cnc::Record {
    cnc::Record::Cycle(cnc::CycleRec {
        kind, x: 0.0, y, r_z: 22.0, bottom_z: 10.0, entry_z: 20.0, peck: 0.0, dwell,
        first_move: 0, feed: Some(100.0),
    })
}

#[test]
fn a_dwelled_hole_dwells_in_seconds_at_its_floor() {
    let mut r = preface();
    r.push(cycle(CycleKind::Drill, 0.0, 0.5));
    r.push(mv(MoveKind::Rapid, 0.0, 0.0, 22.0, None));
    r.push(mv(MoveKind::Feed, 0.0, 0.0, 10.0, Some(100.0)));
    r.push(mv(MoveKind::Rapid, 0.0, 0.0, 22.0, None));
    r.push(cnc::Record::Section { begin: false });
    let out = run(r);
    assert!(out.warnings.is_empty(), "{:?}", out.warnings);
    assert!(out.program.contains("G1 X0 Z10 F0.1\nG4 P0.5\nG0 X0 Z22\n"), "{}", out.program);
    linuxcnc_sane(&out.program);
}

#[test]
fn css_arcs_part_load_and_a_trip_home_speak_linuxcnc() {
    // G96 with the machine's ceiling as D (m/min under G21, ft/min
    // under G20); a ZX arc with I a RADIUS increment (G7 keeps I a
    // radius); the part-load advance in the program's unit; Go home as
    // G30, written once at the end (#504).
    for inch in [false, true] {
        let mut r = vec![cnc::Record::Param { name: "SurfaceSpeed".into(), value: json!(200.0) }];
        r.extend(preface());
        r.insert(0, cnc::Record::Param { name: "AfterOperation".into(), value: json!("home") });
        r.push(cnc::Record::Part { loading: true, advance: 25.4 });
        r.push(mv(MoveKind::Rapid, 0.0, 5.0, 2.0, None));
        r.push(cnc::Record::Move(cnc::MoveRec {
            kind: MoveKind::Feed, x: 0.0, y: 7.0, z: 0.0, feed: Some(100.0), center: Some((0.0, 0.0)),
            ccw: false, plane: camcraft_app::cl::ArcPlane::Yz, abc: None,
        }));
        r.push(cnc::Record::Section { begin: false });
        let out = engine::run(CPS, &lathe_mch(inch), &cnc::serialize(&r)).expect("runs");
        assert!(out.warnings.is_empty(), "{:?}", out.warnings);
        let p = &out.program;
        if inch {
            assert!(p.contains("G96 D12000 S656 M3\n"), "200 m/min is 656 ft/min: {p}");
            assert!(p.contains("(PART LOAD ADVANCE 1 IN)"), "{p}");
        } else {
            assert!(p.contains("G96 D12000 S200 M3\n"), "{p}");
            assert!(p.contains("(PART LOAD ADVANCE 25.4 MM)"), "{p}");
            // r 5 -> r 7 about (r 0, z 0) seen on the +Y meridian: CW
            // there reads CCW in ZX; X diameter 14, I K from the start.
            assert!(p.contains("G3 X14 Z0 I-5 K-2 F0.1\n"), "{p}");
        }
        // #1173: the CSS section cancels its G96 after M5, before the home.
        assert!(p.ends_with("M9\nM5\nG97\nG30\nM30\n"), "home once, not twice: {p}");
        linuxcnc_sane(p);
    }
}

#[test]
fn a_section_without_surface_speed_posts_g97_after_one_with_it() {
    // #772: a CSS turning section, then an axial drill that states no
    // SurfaceSpeed. The drill must run at its own rpm (G97), never at
    // the stale G96 with the D cap - on centre that is the machine max.
    let mut r = vec![cnc::Record::Param { name: "SurfaceSpeed".into(), value: json!(200.0) }];
    r.extend(preface());
    r.push(mv(MoveKind::Rapid, 0.0, 6.0, 10.0, None));
    r.push(mv(MoveKind::Feed, 0.0, 6.0, 0.0, Some(100.0)));
    r.push(cnc::Record::Section { begin: false });
    r.push(cnc::Record::Param { name: "SpindleSpeed".into(), value: json!(500.0) });
    r.push(cnc::Record::Tool { number: 2, name: "DRILL-6".into() });
    r.push(cnc::Record::Section { begin: true });
    r.push(cycle(CycleKind::Drill, 0.0, 0.0));
    r.push(mv(MoveKind::Rapid, 0.0, 0.0, 22.0, None));
    r.push(cnc::Record::Section { begin: false });
    let out = run(r);
    assert!(out.warnings.is_empty(), "{:?}", out.warnings);
    let p = &out.program;
    assert_eq!(p.matches("G96").count(), 1, "only the turning section is CSS: {p}");
    assert!(p.contains("G96 D12000 S200 M3\n"), "{p}");
    let drill = &p[p.find("T0202").expect("the drill's tool change")..];
    assert!(drill.contains("G97 S500 M3\n"), "the drill states its own rpm: {drill}");
    // #1173: the CSS section itself cancels G96 at its end, so no section
    // that follows - whatever its kind - opens under it.
    let css_end = &p[p.find("G96").expect("the CSS section")..p.find("T0202").expect("the drill")];
    assert!(css_end.contains("M5\nG97\n"), "the CSS section ends in G97: {p}");
    linuxcnc_sane(p);
}

#[test]
fn two_css_sections_in_a_row_each_state_their_own_g96() {
    // #1173's rule, its price stated: each CSS section cancels at its end,
    // so the second states G96 again after the first's bare G97, and the
    // program ends in G97.
    let section = |tool: u32| {
        let mut r = vec![cnc::Record::Param { name: "SurfaceSpeed".into(), value: json!(200.0) }];
        r.extend(preface());
        if let Some(cnc::Record::Tool { number, .. }) = r.iter_mut().find(|x| matches!(x, cnc::Record::Tool { .. })) {
            *number = tool;
        }
        r.push(mv(MoveKind::Rapid, 0.0, 6.0, 10.0, None));
        r.push(mv(MoveKind::Feed, 0.0, 6.0, 0.0, Some(100.0)));
        r.push(cnc::Record::Section { begin: false });
        r
    };
    let mut r = section(1);
    r.extend(section(2));
    let out = run(r);
    assert!(out.warnings.is_empty(), "{:?}", out.warnings);
    let p = &out.program;
    assert_eq!(p.matches("G96 D12000 S200 M3\n").count(), 2, "{p}");
    assert_eq!(p.matches("\nG97\n").count(), 2, "one cancel per CSS section: {p}");
    assert!(!p.contains("G97 S"), "{p}");
    assert!(p.ends_with("M5\nG97\nG30\nM30\n"), "{p}");
    linuxcnc_sane(p);
}

#[test]
fn what_the_dialect_cannot_say_is_refused_by_name() {
    // A tap (no G33.1 yet), a back-bore and a hole off the axis.
    for (kind, y, why) in [
        (CycleKind::Tap, 0.0, "tapping"),
        (CycleKind::BackBore, 0.0, "back-boring"),
        (CycleKind::Drill, 5.0, "off the spindle axis"),
    ] {
        let mut r = preface();
        r.push(cycle(kind, y, 0.0));
        r.push(mv(MoveKind::Rapid, 0.0, y, 22.0, None));
        r.push(mv(MoveKind::Feed, 0.0, y, 10.0, Some(100.0)));
        r.push(cnc::Record::Section { begin: false });
        let out = run(r);
        assert!(out.warnings.iter().any(|w| w.contains(why)), "{why}: {:?}", out.warnings);
        assert!(!out.program.contains("Z10"), "{why}: the refused hole wrote its chain: {}", out.program);
    }
    // A move off every meridian: milling work.
    let mut r = preface();
    r.push(mv(MoveKind::Feed, 40.0, 30.0, -1.0, Some(800.0)));
    r.push(cnc::Record::Section { begin: false });
    assert!(run(r).warnings.iter().any(|w| w.contains("milling work")));
    // A tool the T word cannot name.
    let r = vec![
        cnc::Record::Param { name: "SpindleSpeed".into(), value: json!(1000.0) },
        cnc::Record::Tool { number: 100, name: "X".into() },
        cnc::Record::Section { begin: true },
        cnc::Record::Section { begin: false },
    ];
    assert!(run(r).warnings.iter().any(|w| w.contains("T01-T99")));
    // Control compensation.
    let mut r = preface();
    r.push(cnc::Record::Comp { side: Some(camcraft_app::cl::CompSide::Left), radius_mm: 0.8, wear: false });
    r.push(cnc::Record::Section { begin: false });
    assert!(run(r).warnings.iter().any(|w| w.contains("nose-radius compensation")));
    // A C-axis live-tool section: the script does not declare liveTooling.
    let mut r = preface();
    r.insert(0, cnc::Record::Param { name: "LiveTool".into(), value: json!(true) });
    r.push(cnc::Record::Section { begin: false });
    assert!(run(r).warnings.iter().any(|w| w.contains("liveTooling")));
}

// ------------------------------------------------------------ registry

#[test]
fn the_registry_lists_the_pathpilot_lathe_post_for_lathes_only() {
    assert!(posting::BUILTINS.iter().any(|(n, _)| *n == NAME));
    assert!(posting::post_serves(NAME, Kind::Lathe));
    assert!(posting::post_serves(NAME, Kind::MillTurn));
    assert!(!posting::post_serves(NAME, Kind::Mill));
    // The lathe default is unchanged: Fanuc Lathe stays first.
    assert_eq!(posting::default_post_for(Kind::Lathe), PostRef::Named("Fanuc Lathe".into()));
    let world = new_world();
    assert!(SelectPost::new(&world, PostRef::Named(NAME.into())).is_ok());
}
