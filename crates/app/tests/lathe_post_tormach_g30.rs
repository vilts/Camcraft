//! The moves around G30 on the Tormach PathPilot LATHE post (#1604):
//! LinuxCNC's G30 moves every axis to the operator's stored position
//! in one straight line, so the post never knows where it ends. After a
//! G30 the section's first point is reached one axis at a time (X then
//! Z for a point outside the stock's diameter; Z then X for one in
//! front of the face), and before a G30 the tool leaves +X to the
//! stock's diameter, then +Z to the face - so no rapid this post
//! writes, and no G30 line from or to the stock's outer corner, runs
//! through the stock for any stored G30 outside it. No stock, or no
//! stated clearance plane where one is needed: refused by name.
//!
//! Provenance: our own fixtures and checks; the G30 behaviour (every
//! axis to the stored position, params 5181-5189) is from the public
//! LinuxCNC G-code reference (behaviour only).
//!
//! Gate map (M4.4): covers post::post (engine on the PathPilot lathe
//! dialect: the G30 approach and departure), app::posting
//! (stream_records on the Tormach lathe post).
//! Lane shape (M4.4): all tests here run in seconds.

mod common;
mod tormach_lathe_support;

use flecs_ecs::prelude::World;
use serde_json::json;

use camcraft_app::cl::MoveKind;
use camcraft_app::commands::History;
use camcraft_app::drilling::{CycleChoice, Hole};
use camcraft_app::geo2d::Point2;
use camcraft_app::operation::{Height, OpParams};
use camcraft_app::post::{cnc, engine};
use camcraft_app::posting;
use camcraft_app::schema::PostOptions;
use tormach_lathe_support::{create, g76, lathe_mch, program_with, revolve, stock_params, thread_world, world_with, CPS};

// ------------------------------------------------------------ fixtures

/// The flanged bar's rough and finish (the golden suite's turning world).
fn turning_world() -> (World, History) {
    let flanged = [(0.0, -16.0), (6.0, -16.0), (6.0, 2.0), (3.5, 2.0), (3.5, 16.0), (0.0, 16.0)];
    let stock = camcraft_app::stock::StockShape::Cylinder { diameter: 12.0, height: 32.0 };
    let (world, mut history) = world_with(revolve(&flanged, 16), stock, -16.0, &["TN-1600-0R8"], false);
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

/// A Ø4.1 hole on centre, its first point ON the face (the golden
/// suite's drill world, on a round bar).
fn drill_world() -> (World, History) {
    let bar = revolve(&[(0.0, -20.0), (10.0, -20.0), (10.0, 20.0), (2.05, 20.0), (2.05, 10.0), (0.0, 10.0)], 64);
    let stock = camcraft_app::stock::StockShape::Cylinder { diameter: 20.0, height: 40.0 };
    let (world, mut history) = world_with(bar, stock, -20.0, &["DR-0400"], false);
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

/// The stock as the stream states it, in the program's frame (mm):
/// (radius, face Z).
fn stock_of(world: &World) -> (f64, f64) {
    let records = posting::stream_records(world).expect("generated");
    let param = |n: &str| {
        records.iter().find_map(|r| match r {
            cnc::Record::Param { name, value } if name == n => value.as_f64(),
            _ => None,
        })
    };
    let (r, top, oz) = (param("StockXYRight"), param("StockZTop"), param("OriginZ"));
    (r.expect("stock stated"), top.expect("face stated") - oz.unwrap_or(0.0))
}

fn word(line: &str, c: char) -> Option<f64> {
    line.split(' ').skip(1).find_map(|w| w.strip_prefix(c)).and_then(|v| v.parse().ok())
}
fn is_motion(l: &str) -> bool {
    ["G0 ", "G1 ", "G2 ", "G3 ", "G33 ", "G76 "].iter().any(|g| l.starts_with(g))
}

fn rapids<'a>(it: impl Iterator<Item = &'a str>) -> Vec<&'a str> {
    it.filter(|l| is_motion(l)).take_while(|l| l.starts_with("G0 ")).collect()
}

/// The motion lines right after each G30 up to the first feed or cycle,
/// and right before each G30 back to the last feed or cycle.
fn around_g30(text: &str) -> (Vec<Vec<&str>>, Vec<Vec<&str>>) {
    let lines: Vec<&str> = text.lines().collect();
    let (mut after, mut before) = (Vec::new(), Vec::new());
    for (i, l) in lines.iter().enumerate() {
        if *l != "G30" {
            continue;
        }
        after.push(rapids(lines[i + 1..].iter().copied()));
        let mut b = rapids(lines[..i].iter().rev().copied());
        b.reverse();
        before.push(b);
    }
    (after, before)
}

/// The rapids this post adds around G30 - the straight G30 lines to and
/// from `g30` (program mm, X a diameter) and the one-axis G0s next to
/// them (the stream's own rapids state both X and Z) - as (r, z)
/// segments. The stream's own rapids may run through material its
/// earlier passes removed, so they are not judged against the raw stock.
fn g30_segments(text: &str, g30: (f64, f64)) -> Vec<((f64, f64), (f64, f64))> {
    let mut segs = Vec::new();
    let mut at: Option<(f64, f64)> = None;
    for l in text.lines() {
        if l == "G30" {
            if let Some(p) = at {
                segs.push((p, g30));
            }
            at = Some(g30);
            continue;
        }
        if !is_motion(l) {
            continue;
        }
        let Some((x0, z0)) = at else {
            at = Some((word(l, 'X').map_or(0.0, |x| x / 2.0), word(l, 'Z').unwrap_or(0.0)));
            continue;
        };
        let to = (word(l, 'X').map_or(x0, |x| x / 2.0), word(l, 'Z').unwrap_or(z0));
        if l.starts_with("G0 ") && (word(l, 'X').is_none() || word(l, 'Z').is_none()) {
            segs.push(((x0, z0), to));
        }
        at = Some(to);
    }
    segs
}

/// The first point of a segment that stands inside the stock (|r| under
/// its radius, Z behind its face), sampled finely.
fn inside(seg: ((f64, f64), (f64, f64)), r: f64, top: f64) -> Option<(f64, f64)> {
    let ((r0, z0), (r1, z1)) = seg;
    (0..=400).map(|i| f64::from(i) / 400.0).map(|t| (r0 + (r1 - r0) * t, z0 + (z1 - z0) * t)).find(|(pr, pz)| pr.abs() < r - 1e-6 && *pz < top - 1e-6)
}

// ------------------------------------------------------------ pins

#[test]
fn the_first_move_after_g30_is_x_alone_then_z_alone() {
    // A turning start stands outside the stock's diameter: X first.
    for text in [program_with(&turning_world().0, PostOptions::default()), program_with(&thread_world(false).0, g76())] {
        let (after, _) = around_g30(&text);
        assert!(after.first().is_some_and(|a| a.len() == 2), "the approach is two rapids: {text}");
        for a in after.iter().filter(|a| a.len() > 1) {
            let first = a.first().copied().unwrap_or("");
            let second = a.get(1).copied().unwrap_or("");
            assert!(word(first, 'X').is_some() && word(first, 'Z').is_none(), "X alone first after G30: {a:?}\n{text}");
            assert!(word(second, 'Z').is_some() && word(second, 'X').is_none(), "then Z alone: {a:?}\n{text}");
        }
    }
    // A hole's first point stands ON the face, inside the diameter: Z to
    // the section's ClearancePlane first, then X onto the centre, then Z.
    let text = program_with(&drill_world().0, PostOptions::default());
    let (after, _) = around_g30(&text);
    assert_eq!(after[0][..3], ["G0 Z30", "G0 X0", "G0 Z20"], "{text}");
}

#[test]
fn the_departure_goes_plus_x_then_plus_z_before_g30() {
    // The hole ends on centre in front of the face: +X to the stock's
    // diameter before the closing G30.
    let text = program_with(&drill_world().0, PostOptions::default());
    assert!(text.ends_with("G0 X0 Z30\nM9\nM5\nG0 X20\nG30\nM30\n"), "{text}");
    // A hand stream ending outside the diameter, behind the face, then a
    // tool change: +Z to the face before that G30, in the old frame.
    let mut r = vec![cnc::Record::Param { name: "SpindleSpeed".into(), value: json!(1000.0) }];
    r.extend(stock_params(6.0, 10.0, 30.0));
    r.push(cnc::Record::Tool { number: 1, name: "T1".into() });
    r.push(cnc::Record::Section { begin: true });
    r.push(rapid(8.0, -5.0));
    r.push(feed(6.5, -5.0));
    r.push(rapid(8.0, -5.0));
    r.push(cnc::Record::Section { begin: false });
    r.push(cnc::Record::Param { name: "WorkOffset".into(), value: json!(2.0) });
    r.extend(stock_params(6.0, 10.0, 30.0));
    r.push(cnc::Record::Tool { number: 2, name: "T2".into() });
    r.push(cnc::Record::Section { begin: true });
    r.push(rapid(8.0, 0.0));
    r.push(cnc::Record::Section { begin: false });
    let out = engine::run(CPS, &lathe_mch(false), &cnc::serialize(&r)).expect("runs");
    assert!(out.warnings.is_empty(), "{:?}", out.warnings);
    let p = &out.program;
    let change = p.find("(T0202").expect("the second tool");
    assert!(
        p[..change].ends_with("G0 X16 Z-5\nM9\nM5\nG0 Z10\nG55\n"),
        "+Z to the face, in the old frame - before the next work offset: {p}"
    );
    assert!(p.ends_with("G0 X16\nG0 Z0\nM9\nM5\nG0 Z10\nG30\nM30\n"), "{p}");
}

#[test]
fn no_stored_g30_outside_the_stock_is_reached_across_it() {
    // Every stored G30 on the tool's side of the axis outside the stock:
    // no rapid of the program, and neither straight G30 line, crosses it.
    let worlds = [turning_world().0, drill_world().0, thread_world(false).0];
    for world in &worlds {
        let text = program_with(world, PostOptions::default());
        let (r, top) = stock_of(world);
        for i in 0..=40 {
            for k in 0..=40 {
                let g = (f64::from(i) * 1.0, top - 50.0 + f64::from(k) * 2.0);
                if g.0 < r + 0.5 && g.1 < top + 0.5 {
                    continue; // inside the stock: no program could reach it
                }
                for seg in g30_segments(&text, g) {
                    assert!(inside(seg, r, top).is_none(), "G30 at r{} z{}: {seg:?} crosses the stock\n{text}", g.0, g.1);
                }
            }
        }
        // And whatever G30 holds - inside the stock too - the post never
        // writes a diagonal from it: the first move after each G30 is one
        // axis. Each G30 line starts at the stock's outer corner (outside
        // its diameter and in front of its face, or the program's start).
        let (after, _) = around_g30(&text);
        for l in after.iter().filter_map(|a| a.first()) {
            assert!(word(l, 'X').is_none() || word(l, 'Z').is_none(), "a two-axis rapid from G30: {l}\n{text}");
        }
        for seg in g30_segments(&text, (f64::NAN, f64::NAN)).iter().filter(|(from, to)| to.0.is_nan() && !from.0.is_nan()) {
            let (pr, pz) = seg.0;
            assert!(pr >= r - 1e-6 && pz >= top - 1e-6, "a G30 line from r{pr} z{pz}, not the stock's corner\n{text}");
        }
    }
}

#[test]
fn a_missing_stock_or_clearance_is_refused_by_name() {
    let section = |stock: Vec<cnc::Record>, moves: Vec<cnc::Record>| {
        let mut r = vec![cnc::Record::Param { name: "SpindleSpeed".into(), value: json!(1000.0) }];
        r.extend(stock);
        r.push(cnc::Record::Tool { number: 1, name: "T1".into() });
        r.push(cnc::Record::Section { begin: true });
        r.extend(moves);
        r.push(cnc::Record::Section { begin: false });
        engine::run(CPS, &lathe_mch(false), &cnc::serialize(&r)).expect("runs")
    };
    // No stock: nothing tells the post which way is clear.
    let out = section(vec![], vec![rapid(8.0, 5.0)]);
    assert!(out.warnings.iter().any(|w| w.contains("no stock stated")), "{:?}", out.warnings);
    assert!(!out.program.contains("G0 "), "no guessed approach: {}", out.program);
    // A first point on the face, inside the diameter, and the clearance
    // plane not in front of the face: the approach is refused.
    let out = section(stock_params(6.0, 10.0, 10.0), vec![rapid(0.0, 10.0), feed(0.0, 5.0)]);
    assert!(out.warnings.iter().any(|w| w.contains("no ClearancePlane in front of the face")), "{:?}", out.warnings);
    assert!(!out.program.contains("G0 ") && !out.program.contains("G1 "), "{}", out.program);
    // A section that ends on the stock's corner with no clearance stated:
    // the departure is refused.
    let out = section(stock_params(6.0, 10.0, 10.0), vec![rapid(12.0, 10.0), feed(6.0, 10.0)]);
    assert!(out.warnings.iter().any(|w| w.contains("ends on the stock's corner")), "{:?}", out.warnings);
    // One that ends inside the stock's envelope.
    let out = section(stock_params(6.0, 10.0, 30.0), vec![rapid(8.0, 5.0), feed(4.0, 5.0)]);
    assert!(out.warnings.iter().any(|w| w.contains("inside the stock's envelope")), "{:?}", out.warnings);
    // A feed as the first motion after G30.
    let out = section(stock_params(6.0, 10.0, 30.0), vec![feed(8.0, 5.0)]);
    assert!(out.warnings.iter().any(|w| w.contains("first motion after G30")), "{:?}", out.warnings);
    assert!(!out.program.contains("G1 "), "{}", out.program);
}

fn rapid(r: f64, z: f64) -> cnc::Record {
    mv(MoveKind::Rapid, r, z, None)
}
fn feed(r: f64, z: f64) -> cnc::Record {
    mv(MoveKind::Feed, r, z, Some(100.0))
}
fn mv(kind: MoveKind, x: f64, z: f64, feed: Option<f64>) -> cnc::Record {
    cnc::Record::Move(cnc::MoveRec { kind, x, y: 0.0, z, feed, center: None, ccw: false, plane: Default::default(), abc: None })
}
