//! The lathe face-drilling exit law (#1011, #994's hop law on the
//! lathe): a lathe post that drills on centre leaves the hole at the
//! stream's own exit level - the chain's rapid up to clearance - before
//! any further motion, never at the R plane its face cycle returns to.
//! One read-back checker follows each posted program as a control would
//! (G83/G84/G85 return to the level the tool stood at when the cycle
//! was read; G80 moves nothing; G28/G30 send the tool home) and is run
//! on both shipped lathe posts - the Fanuc one that writes the face
//! cycle and the PathPilot one that posts the chain as plain moves - on
//! the axial-drill document the post goldens lock (clearance 30, R 20).
//! The seat law (#1026, #994's cycleSeat on the lathe): a face cycle is
//! read with the tool on centre at R, and the post never reaches that
//! seat by an X move below R - from below R it climbs out first (+Z is
//! away from the face), then moves onto the centre, then descends.
//!
//! Provenance: our own test; face-cycle semantics from the public
//! RS-274 / ISO 6983 vocabulary and general lathe handbook practice,
//! no vendor material.
//!
//! Gate map (M4.4): covers assets/posts/fanuc-lathe.cps and
//! tormach-lathe.cps (their drilling paths), post::post (engine on the
//! lathe dialects), app::posting (stream_records).
//! Lane shape (M4.4): all tests here run in seconds.

mod tormach_lathe_support;

use camcraft_app::drilling::{CycleChoice, Hole};
use camcraft_app::geo2d::Point2;
use camcraft_app::operation::{Height, OpParams};
use camcraft_app::post::{cnc, engine};
use camcraft_app::posting;
use tormach_lathe_support::{create, revolve, world_with, CPS as TORMACH_CPS};

const FANUC_CPS: &str = include_str!("../../../assets/posts/fanuc-lathe.cps");
const EPS: f64 = 1e-6;

fn word(tok: &str) -> Option<(char, f64)> {
    let mut c = tok.chars();
    let l = c.next()?.to_ascii_uppercase();
    if !l.is_ascii_alphabetic() {
        return None;
    }
    c.as_str().parse::<f64>().ok().map(|v| (l, v))
}

/// Read a lathe `program` back: the faults found, and the holes it
/// drilled (a face cycle word, or a feed down to `floor` on centre).
/// After the first hole, every move that changes X must start and end
/// at Z >= `exit`, and the tool must stand at Z >= `exit` when it
/// leaves the cut (its first home move after the hole, or the end).
/// Before it (#1026), a move that changes X from a known position must
/// start and end at Z >= `seat` (the R level), and a face cycle must be
/// read with the tool on centre at `seat`.
fn check(program: &str, floor: f64, seat: f64, exit: f64) -> (Vec<String>, usize) {
    let mut faults = Vec::new();
    let mut holes = 0usize;
    let (mut x, mut z): (Option<f64>, f64) = (None, f64::INFINITY);
    let mut motion = 0u32;
    let mut left_at: Option<f64> = None;
    for (n, raw) in program.lines().enumerate() {
        let mut text = String::new();
        let mut depth = 0;
        for ch in raw.chars() {
            match ch {
                '(' => depth += 1,
                ')' => depth -= 1,
                _ if depth == 0 => text.push(ch),
                _ => {}
            }
        }
        let words: Vec<(char, f64)> = text.split_whitespace().filter_map(word).collect();
        let gs: Vec<u32> = words.iter().filter(|(l, _)| *l == 'G').map(|(_, v)| (v * 10.0).round() as u32).collect();
        let get = |l: char| words.iter().find(|(w, _)| *w == l).map(|(_, v)| *v);
        if gs.iter().any(|g| matches!(*g, 280 | 300)) {
            if holes > 0 && left_at.is_none() {
                left_at = Some(z);
            }
            x = None;
            z = f64::INFINITY;
            continue;
        }
        let mut cycle = false;
        for g in &gs {
            match *g {
                0 | 10 | 20 | 30 => motion = *g / 10,
                800 => motion = 0,
                830 | 840 | 850 => cycle = true,
                _ => {}
            }
        }
        if cycle {
            // The face cycle drills on centre and returns to the level
            // it was read at: Z is where it was - which must be R.
            if !x.is_some_and(|v| v.abs() <= EPS) || (z - seat).abs() > EPS {
                faults.push(format!("line {}: `{}` is read off its seat (X{x:?} Z{z}, not X0 Z{seat})", n + 1, raw.trim()));
            }
            holes += 1;
            left_at = None;
            continue;
        }
        let (nx, nz) = (get('X').or(x), get('Z'));
        if get('X').is_none() && nz.is_none() {
            continue;
        }
        let to_z = nz.unwrap_or(z);
        let moves_x = match (x, nx) {
            (Some(a), Some(b)) => (a - b).abs() > EPS,
            _ => true,
        };
        if moves_x && holes > 0 && z.min(to_z) < exit - EPS {
            faults.push(format!("line {}: `{}` moves X at Z{}, below the exit Z{exit}", n + 1, raw.trim(), z.min(to_z)));
        }
        if moves_x && holes == 0 && x.is_some() && z.min(to_z) < seat - EPS {
            faults.push(format!("line {}: `{}` moves X at Z{}, below the seat Z{seat}", n + 1, raw.trim(), z.min(to_z)));
        }
        if motion == 1 && nx.is_some_and(|v| v.abs() <= EPS) && (to_z - floor).abs() <= EPS {
            holes += 1;
        }
        x = nx;
        z = to_z;
        if holes > 0 {
            left_at = None;
        }
    }
    let final_z = left_at.unwrap_or(z);
    if holes > 0 && final_z < exit - EPS {
        faults.push(format!("the program leaves its hole at Z{final_z}, below the exit Z{exit}"));
    }
    (faults, holes)
}

fn param(records: &[cnc::Record], name: &str) -> f64 {
    records
        .iter()
        .find_map(|r| match r {
            cnc::Record::Param { name: n, value } if n == name => value.as_f64(),
            _ => None,
        })
        .unwrap_or_else(|| panic!("the stream states no {name}"))
}

#[test]
fn both_lathe_posts_leave_a_face_drilled_hole_at_the_streams_exit() {
    let bar = revolve(&[(0.0, -20.0), (10.0, -20.0), (10.0, 20.0), (2.05, 20.0), (2.05, 10.0), (0.0, 10.0)], 64);
    let stock = camcraft_app::stock::StockShape::Box { dx: 24.0, dy: 24.0, dz: 40.0 };
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
    let records = posting::stream_records(&world).expect("every op generated");
    let exit = param(&records, "ClearancePlane");
    assert!((exit - 30.0).abs() < EPS, "the stream's exit is the op's clearance: {exit}");
    let stream = cnc::serialize(&records);
    for (name, cps) in [("fanuc-lathe", FANUC_CPS), ("tormach-lathe", TORMACH_CPS)] {
        let out = engine::run(cps, &posting::machine_value(&world), &stream).expect("the lathe script runs");
        assert!(out.warnings.is_empty(), "{name}: a clean hole posts without refusal: {:?}", out.warnings);
        let (faults, holes) = check(&out.program, 10.0, 20.0, exit);
        assert_eq!(holes, 1, "{name}: one hole drilled:\n{}", out.program);
        assert!(faults.is_empty(), "{name}: {faults:#?}\n{}", out.program);
    }
}

#[test]
fn the_checker_catches_the_r_plane_exit() {
    // The program #1011 found: the exit rapid swallowed, home from R.
    let bad = "G00 X0 Z20\nG83 Z10 Q12000 F0.008\nG80\nM9\nM05\nG28 U0 W0\nM30\n";
    let (faults, holes) = check(bad, 10.0, 20.0, 30.0);
    assert_eq!(holes, 1);
    assert_eq!(faults.len(), 1, "{faults:#?}");
    assert!(faults[0].contains("leaves its hole at Z20"), "{faults:#?}");
    // A move off centre at R after the hole is caught too.
    let (faults, _) = check("G00 X0 Z20\nG83 Z10 F0.008\nG80\nG00 X40 Z20\nG00 Z30\n", 10.0, 20.0, 30.0);
    assert_eq!(faults.len(), 1, "{faults:#?}");
    // And the shape the fix posts passes, cycle or plain moves.
    let good = "G00 X0 Z20\nG83 Z10 Q12000 F0.008\nG80\nG00 X0 Z30\nM9\nG28 U0 W0\nM30\n";
    assert_eq!(check(good, 10.0, 20.0, 30.0), (vec![], 1));
    let plain = "G0 X0 Z20\nG1 X0 Z10 F0.008\nG0 X0 Z20\nG0 X0 Z30\nM5\nG30\nM30\n";
    assert_eq!(check(plain, 10.0, 20.0, 30.0), (vec![], 1));
}

#[test]
fn the_checker_catches_an_off_seat_cycle_reached_by_one_diagonal() {
    // #1026: the program the old post wrote for a cycle read below R
    // and off centre - one X+Z rapid up to R past the face.
    let bad = "G00 X30 Z18\nG00 X0 Z22\nG83 Z10 F0.1\nG80\nG00 X0 Z40\nG28 U0 W0\nM30\n";
    let (faults, holes) = check(bad, 10.0, 22.0, 40.0);
    assert_eq!(holes, 1);
    assert_eq!(faults.len(), 1, "{faults:#?}");
    assert!(faults[0].contains("below the seat Z22"), "{faults:#?}");
    // A cycle read where the tool happened to stand is caught too.
    let (faults, _) = check("G00 X30 Z18\nG83 Z10 F0.1\nG80\nG00 X0 Z40\n", 10.0, 22.0, 40.0);
    assert!(faults.iter().any(|f| f.contains("read off its seat")), "{faults:#?}");
    // And the seated shape passes: out to R, onto the centre, the cycle.
    let good = "G00 X30 Z18\nG00 Z22\nG00 X0\nG83 Z10 F0.1\nG80\nG00 X0 Z40\nG28 U0 W0\nM30\n";
    assert_eq!(check(good, 10.0, 22.0, 40.0), (vec![], 1));
}

fn lathe_mch() -> serde_json::Value {
    camcraft_app::post::machine::mirror(&camcraft_app::machine::Machine {
        model: "TestBench lathe".into(),
        kind: Some("lathe".into()),
        spindle_max: Some(12000.0),
        coolants: vec![camcraft_app::machine::Coolant { name: "Flood".into(), m_code: "M8".into() }],
        ..Default::default()
    })
}

fn rapid(x: f64, z: f64) -> cnc::Record {
    cnc::Record::Move(cnc::MoveRec {
        kind: camcraft_app::cl::MoveKind::Rapid, x, y: 0.0, z, feed: None, center: None, ccw: false,
        plane: Default::default(), abc: None,
    })
}

/// A face drill whose cycle record is read with the tool at radius
/// `from_x`, Z `from_z` (R 22, bottom 10, exit 40): the chain that
/// follows (approach, plunge, retract, exit) is the stream's.
fn off_seat_stream(from: Option<(f64, f64)>, r_z: f64) -> Vec<cnc::Record> {
    let mut r = vec![
        cnc::Record::Param { name: "SpindleSpeed".into(), value: serde_json::json!(1000.0) },
        cnc::Record::Param { name: "WorkOffset".into(), value: serde_json::json!("G54") },
        cnc::Record::Tool { number: 1, name: "DR-0400".into() },
        cnc::Record::Section { begin: true },
    ];
    if let Some((x, z)) = from {
        r.push(rapid(x, z));
    }
    r.push(cnc::Record::Cycle(cnc::CycleRec {
        kind: camcraft_app::cl::CycleKind::Drill, x: 0.0, y: 0.0, r_z, bottom_z: 10.0, entry_z: 20.0, peck: 0.0,
        // The stream anchors a cycle at its chain's first move: after
        // the approach rapid when there is one.
        dwell: 0.0, first_move: usize::from(from.is_some()), feed: Some(100.0),
    }));
    r.push(rapid(0.0, r_z));
    r.push(cnc::Record::Move(cnc::MoveRec {
        kind: camcraft_app::cl::MoveKind::Feed, x: 0.0, y: 0.0, z: 10.0, feed: Some(100.0), center: None, ccw: false,
        plane: Default::default(), abc: None,
    }));
    r.push(rapid(0.0, r_z));
    r.push(rapid(0.0, 40.0));
    r.push(cnc::Record::Section { begin: false });
    r
}

#[test]
fn fanuc_lathe_seats_an_off_seat_face_cycle_by_the_hop_law() {
    // #1026: read below R (Z18 < R22) and off centre (radius 15). Out
    // to R first, then onto the centre, then the cycle - never one
    // diagonal X+Z rapid below R.
    let stream = cnc::serialize(&off_seat_stream(Some((15.0, 18.0)), 22.0));
    let out = engine::run(FANUC_CPS, &lathe_mch(), &stream).expect("the lathe script runs");
    assert!(out.warnings.is_empty(), "a seatable cycle posts without refusal: {:?}\n{}\n{stream}", out.warnings, out.program);
    let (faults, holes) = check(&out.program, 10.0, 22.0, 40.0);
    assert_eq!(holes, 1, "{}", out.program);
    assert!(faults.is_empty(), "{faults:#?}\n{}", out.program);
    assert!(
        out.program.contains("G00 X30 Z18\nG00 Z22\nG00 X0\nG83 Z10 F0.1\nG80\nG00 X0 Z40\n"),
        "Z out to R, X onto the centre, the cycle: {}",
        out.program
    );
    // From above R off centre: over the centre first, then down to R.
    let stream = cnc::serialize(&off_seat_stream(Some((15.0, 30.0)), 22.0));
    let out = engine::run(FANUC_CPS, &lathe_mch(), &stream).expect("the lathe script runs");
    assert!(out.warnings.is_empty(), "{:?}", out.warnings);
    assert!(out.program.contains("G00 X30 Z30\nG00 X0\nG00 Z22\nG83 Z10"), "{}", out.program);
    assert_eq!(check(&out.program, 10.0, 22.0, 40.0), (vec![], 1), "{}", out.program);
}

#[test]
fn fanuc_lathe_refuses_a_face_cycle_it_cannot_seat_by_name() {
    // #1026: the cycle is its section's first motion - no stream
    // position to seat from, so no guessed positioning rapid.
    let stream = cnc::serialize(&off_seat_stream(None, 22.0));
    let out = engine::run(FANUC_CPS, &lathe_mch(), &stream).expect("the lathe script runs");
    assert!(out.warnings.iter().any(|w| w.contains("section's first motion")), "{:?}", out.warnings);
    assert!(!out.program.contains("G83"), "{}", out.program);
    // R not above the bottom: the post cannot tell which way is clear.
    let stream = cnc::serialize(&off_seat_stream(Some((15.0, 18.0)), 8.0));
    let out = engine::run(FANUC_CPS, &lathe_mch(), &stream).expect("the lathe script runs");
    assert!(out.warnings.iter().any(|w| w.contains("which way is clear")), "{:?}", out.warnings);
    assert!(!out.program.contains("G83"), "{}", out.program);
}

#[test]
fn tormach_lathe_adds_no_positioning_move_to_an_off_seat_face_cycle() {
    // #1026: the PathPilot post writes no cycle and no positioning
    // block of its own - every motion line is one of the stream's
    // moves, in order, so no diagonal the stream does not state.
    let records = off_seat_stream(Some((15.0, 18.0)), 22.0);
    let out = engine::run(TORMACH_CPS, &lathe_mch(), &cnc::serialize(&records)).expect("the lathe script runs");
    assert!(out.warnings.is_empty(), "{:?}", out.warnings);
    let motion: Vec<&str> = out.program.lines().filter(|l| l.starts_with("G0 ") || l.starts_with("G1 ")).collect();
    assert_eq!(
        motion,
        ["G0 X30 Z18", "G0 X0 Z22", "G1 X0 Z10 F0.1", "G0 X0 Z22", "G0 X0 Z40"],
        "{}",
        out.program
    );
}
