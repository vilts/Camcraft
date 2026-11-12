//! Shared fixtures for the inverted-axis post suites (#374): the probe
//! stream (lines, G2/G3 in all three planes, cutter compensation, a
//! drilled hole, a non-zero work-offset origin), the inverted machines,
//! and a small RS-274 interpreter that reads a posted program the way a
//! controller with inverted axes does and maps it back to the physical
//! (CAM) frame.
//!
//! Provenance: RS-274 / NIST RS274NGC (NISTIR 6300) word semantics - G0
//! G1 G2 G3, G17/G18/G19 plane select, I/J/K as start-relative centre
//! increments, G2/G3 sense seen from the plane normal's positive end,
//! G41/G42 left/right of the travel, G81-family holes at X Y. Fixture
//! values are our own invention.
#![allow(dead_code)]

use camcraft_app::cl::{ArcPlane, CompSide, CycleKind, MoveKind};
use camcraft_app::post::cnc::{self, CycleRec, MoveRec, Record};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

pub const ORIGIN: [f64; 3] = [10.0, 20.0, 0.0];

pub fn repo(rel: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../").join(rel)
}

pub fn read(rel: &str) -> String {
    std::fs::read_to_string(repo(rel)).unwrap_or_else(|e| panic!("{rel}: {e}"))
}

pub fn cps(stem: &str) -> String {
    read(&format!("assets/posts/{stem}.cps"))
}

/// The shipped 3-axis machine, stating `letters` inverted the way the
/// mirror does (`invertedAxes`).
pub fn machine(letters: &[&str]) -> Value {
    let mut m: Value = serde_json::from_str(&read("assets/machines/3axis-vmc.mch")).expect("machine json");
    if !letters.is_empty()
        && let Some(o) = m.as_object_mut()
    {
        o.insert("invertedAxes".into(), json!(letters));
    }
    m
}

pub fn mv(kind: MoveKind, p: [f64; 3], feed: Option<f64>) -> Record {
    Record::Move(MoveRec { kind, x: p[0], y: p[1], z: p[2], feed, center: None, ccw: false, plane: ArcPlane::Xy, abc: None })
}

pub fn arc(p: [f64; 3], center: (f64, f64), ccw: bool, plane: ArcPlane) -> Record {
    Record::Move(MoveRec {
        kind: MoveKind::Feed,
        x: p[0],
        y: p[1],
        z: p[2],
        feed: Some(400.0),
        center: Some(center),
        ccw,
        plane,
        abc: None,
    })
}

/// The probe stream: every word the translation touches.
pub fn probe_records() -> Vec<Record> {
    use MoveKind::{Feed, Plunge, Rapid};
    let mut r = vec![
        Record::Param { name: "OperationName".into(), value: json!("Mirror probe") },
        Record::Param { name: "SpindleSpeed".into(), value: json!(8000) },
        Record::Param { name: "WorkOffset".into(), value: json!("G54") },
        Record::Param { name: "OriginX".into(), value: json!(ORIGIN[0]) },
        Record::Param { name: "OriginY".into(), value: json!(ORIGIN[1]) },
        Record::Param { name: "OriginZ".into(), value: json!(ORIGIN[2]) },
        Record::Param { name: "StockXYLeft".into(), value: json!(0.0) },
        Record::Param { name: "StockXYRight".into(), value: json!(80.0) },
        Record::Param { name: "StockXYFront".into(), value: json!(0.0) },
        Record::Param { name: "StockXYBack".into(), value: json!(60.0) },
        Record::Tool { number: 1, name: "6MM END MILL".into() },
        Record::Section { begin: true },
        mv(Rapid, [30.0, 30.0, 5.0], None),
        mv(Plunge, [30.0, 30.0, -1.0], Some(200.0)),
        mv(Feed, [50.0, 30.0, -1.0], Some(600.0)),
        // G17: a CCW quarter, then a CW three-quarter about one centre.
        arc([60.0, 40.0, -1.0], (50.0, 40.0), true, ArcPlane::Xy),
        arc([50.0, 50.0, -1.0], (50.0, 40.0), false, ArcPlane::Xy),
        mv(Feed, [40.0, 50.0, -1.0], Some(600.0)),
        // G18 (centre X Z) and G19 (centre Y Z) quarters.
        arc([35.0, 50.0, 4.0], (35.0, -1.0), true, ArcPlane::Xz),
        arc([35.0, 45.0, -1.0], (45.0, 4.0), false, ArcPlane::Yz),
        // Back in G17 before compensation engages (the comp plane).
        arc([35.0, 35.0, -1.0], (35.0, 40.0), true, ArcPlane::Xy),
        // Control compensation: engage on a straight lead, a compensated
        // arc, cancel on a straight lead-out.
        Record::Comp { side: Some(CompSide::Left), radius_mm: 3.0, wear: false },
        mv(Feed, [35.0, 30.0, -1.0], Some(600.0)),
        mv(Feed, [20.0, 30.0, -1.0], Some(600.0)),
        arc([20.0, 10.0, -1.0], (20.0, 20.0), true, ArcPlane::Xy),
        Record::Comp { side: None, radius_mm: 3.0, wear: false },
        mv(Feed, [35.0, 10.0, -1.0], Some(600.0)),
        mv(Rapid, [35.0, 10.0, 5.0], None),
    ];
    // The cycle anchors on its first cut, as the generator lays a hole
    // out (the agreement law): the rapid down to R before it is the
    // positioning move, posted as motion (#994).
    r.push(mv(Rapid, [70.0, 20.0, 2.0], None));
    let first = r.iter().filter(|x| matches!(x, Record::Move(_))).count();
    r.push(Record::Cycle(CycleRec {
        kind: CycleKind::Drill,
        x: 70.0,
        y: 20.0,
        r_z: 2.0,
        bottom_z: -5.0,
        entry_z: 0.0,
        peck: 0.0,
        dwell: 0.0,
        first_move: first,
        feed: Some(150.0),
    }));
    r.push(mv(Plunge, [70.0, 20.0, -5.0], Some(150.0)));
    r.push(mv(Rapid, [70.0, 20.0, 2.0], None));
    r.push(Record::Section { begin: false });
    r
}

pub fn probe_stream() -> String {
    cnc::serialize(&probe_records())
}

// ------------------------------------------------------------ the physical path

/// One physical event: a motion's sampled points (end last), a hole
/// centre, or a compensation engage with the physical side.
#[derive(Clone, Debug, PartialEq)]
pub enum Event {
    Motion(Vec<[f64; 3]>),
    Hole([f64; 2]),
    Engage(CompSide),
}

const SAMPLES: usize = 8;

fn normal(plane: u8) -> [f64; 3] {
    match plane {
        18 => [0.0, 1.0, 0.0],
        19 => [1.0, 0.0, 0.0],
        _ => [0.0, 0.0, 1.0],
    }
}

fn cross(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}

fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

/// Points along an arc from `s` to `e` about `c` in the plane with
/// normal `n`, CCW = a positive rotation about `n` (the RS-274 sense:
/// counter-clockwise seen from the normal's positive end). Rodrigues'
/// rotation of the start radius; the end is appended exactly.
pub fn arc_points(s: [f64; 3], e: [f64; 3], c: [f64; 3], n: [f64; 3], ccw: bool) -> Vec<[f64; 3]> {
    let u = [s[0] - c[0], s[1] - c[1], s[2] - c[2]];
    let v = [e[0] - c[0], e[1] - c[1], e[2] - c[2]];
    let mut t = dot(n, cross(u, v)).atan2(dot(u, v));
    let tau = std::f64::consts::TAU;
    if ccw && t <= 1e-12 {
        t += tau;
    }
    if !ccw && t >= -1e-12 {
        t -= tau;
    }
    let mut out = Vec::new();
    for k in 1..SAMPLES {
        let a = t * k as f64 / SAMPLES as f64;
        let (sn, cs) = a.sin_cos();
        let nxu = cross(n, u);
        let ndu = dot(n, u);
        let mut p = [0.0; 3];
        for i in 0..3 {
            p[i] = c[i] + u[i] * cs + nxu[i] * sn + n[i] * ndu * (1.0 - cs);
        }
        out.push(p);
    }
    out.push(e);
    out
}

/// The CAM path of the probe stream, in world (= physical) millimetres:
/// what the machine must do. The drilled hole's expanded chain is the
/// cycle's business, so it is the hole, not three moves.
pub fn cam_events(records: &[Record]) -> Vec<Event> {
    let mut ev = Vec::new();
    let mut pos = [0.0; 3];
    let mut chain_from = usize::MAX;
    let mut k = 0usize;
    for r in records {
        match r {
            Record::Move(m) => {
                let e = [m.x, m.y, m.z];
                if k < chain_from {
                    let pts = match m.center {
                        None => vec![e],
                        Some((a, b)) => {
                            let (c, n) = match m.plane {
                                ArcPlane::Xy => ([a, b, m.z], normal(17)),
                                ArcPlane::Xz => ([a, m.y, b], normal(18)),
                                ArcPlane::Yz => ([m.x, a, b], normal(19)),
                            };
                            arc_points(pos, e, c, n, m.ccw)
                        }
                    };
                    ev.push(Event::Motion(pts));
                }
                pos = e;
                k += 1;
            }
            Record::Cycle(c) => {
                chain_from = c.first_move;
                ev.push(Event::Hole([c.x, c.y]));
            }
            Record::Comp { side: Some(s), .. } => ev.push(Event::Engage(*s)),
            _ => {}
        }
    }
    ev
}

/// Read a posted Fanuc/Haas-family program as a controller whose axes
/// `flip` are inverted does, and return what the machine physically
/// does: controller coordinate `c` on an inverted axis is physical
/// `-c`, plus the work-offset origin. Panics (test-only) on a word this
/// reader does not model, so an unexpected dialect turn cannot pass.
pub fn controller_events(program: &str, flip: [bool; 3]) -> Vec<Event> {
    let sign = |i: usize| if flip[i] { -1.0 } else { 1.0 };
    let phys = |c: [f64; 3]| [sign(0) * c[0] + ORIGIN[0], sign(1) * c[1] + ORIGIN[1], sign(2) * c[2] + ORIGIN[2]];
    let mut ev = Vec::new();
    let (mut pos, mut motion, mut plane, mut in_cycle) = ([0.0f64; 3], 0u32, 17u8, false);
    let mut comp_pending: Option<u32> = None;
    for line in program.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('(') || line.starts_with('%') || line.starts_with('O') {
            continue;
        }
        let words: Vec<(char, f64)> = line
            .split_whitespace()
            .filter_map(|w| {
                let mut ch = w.chars();
                let l = ch.next()?;
                ch.as_str().parse::<f64>().ok().map(|v| (l, v))
            })
            .collect();
        let gs: Vec<u32> = words.iter().filter(|(l, _)| *l == 'G').map(|(_, v)| (v * 10.0).round() as u32).collect();
        if gs.contains(&280) {
            continue; // the reference return home is no part of the path
        }
        assert!(!gs.contains(&910), "incremental mode is not modelled: {line}");
        let get = |l: char| words.iter().find(|(c, _)| *c == l).map(|(_, v)| *v);
        for g in &gs {
            match g {
                0 | 10 | 20 | 30 => {
                    motion = *g;
                    in_cycle = false;
                }
                170 => plane = 17,
                180 => plane = 18,
                190 => plane = 19,
                410 | 420 => comp_pending = Some(*g),
                800 => in_cycle = false,
                730 | 810 | 820 | 830 | 840 | 860 | 890 => {
                    in_cycle = true;
                }
                _ => {}
            }
        }
        let (x, y, z) = (get('X'), get('Y'), get('Z'));
        if x.is_none() && y.is_none() && z.is_none() {
            continue;
        }
        if in_cycle {
            let h = phys([x.unwrap_or(pos[0]), y.unwrap_or(pos[1]), 0.0]);
            ev.push(Event::Hole([h[0], h[1]]));
            continue;
        }
        let end = [x.unwrap_or(pos[0]), y.unwrap_or(pos[1]), z.unwrap_or(pos[2])];
        if let Some(g) = comp_pending.take() {
            assert_eq!(motion, 10, "comp engages on a G1: {line}");
            // The controller offsets left (G41) / right (G42) of its own
            // travel; map both vectors to the physical frame and read
            // which side that is there.
            let d = [end[0] - pos[0], end[1] - pos[1], 0.0];
            let left = [-d[1], d[0], 0.0];
            let off = if g == 410 { left } else { [-left[0], -left[1], 0.0] };
            let dp = [sign(0) * d[0], sign(1) * d[1], 0.0];
            let op = [sign(0) * off[0], sign(1) * off[1], 0.0];
            let side = if cross(dp, op)[2] > 0.0 { CompSide::Left } else { CompSide::Right };
            ev.push(Event::Engage(side));
        }
        let pts = match motion {
            20 | 30 => {
                let (i, j, k) = (get('I').unwrap_or(0.0), get('J').unwrap_or(0.0), get('K').unwrap_or(0.0));
                let c = [pos[0] + i, pos[1] + j, pos[2] + k];
                arc_points(pos, end, c, normal(plane), motion == 30)
            }
            _ => vec![end],
        };
        ev.push(Event::Motion(pts.into_iter().map(phys).collect()));
        pos = end;
    }
    ev
}

/// The two event lists agree within `tol` mm, event by event.
pub fn assert_same_path(want: &[Event], got: &[Event], tol: f64, what: &str) {
    assert_eq!(want.len(), got.len(), "{what}: event count\nwant {want:?}\ngot  {got:?}");
    for (n, (a, b)) in want.iter().zip(got).enumerate() {
        match (a, b) {
            (Event::Motion(pa), Event::Motion(pb)) => {
                assert_eq!(pa.len(), pb.len(), "{what}: event {n} sample count");
                for (p, q) in pa.iter().zip(pb) {
                    let d = ((p[0] - q[0]).powi(2) + (p[1] - q[1]).powi(2) + (p[2] - q[2]).powi(2)).sqrt();
                    assert!(d <= tol, "{what}: event {n} strays {d:.4} mm: CAM {p:?} vs machine {q:?}");
                }
            }
            (Event::Hole(p), Event::Hole(q)) => {
                assert!((p[0] - q[0]).abs() <= tol && (p[1] - q[1]).abs() <= tol, "{what}: hole {n}: {p:?} vs {q:?}");
            }
            _ => assert_eq!(a, b, "{what}: event {n}"),
        }
    }
}
