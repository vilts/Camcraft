//! #2191: `useRadiusArcs: true` must never post an R-arc whose radius
//! cannot reach its rounded end point (chord > 2|R|) or whose center
//! becomes ill-conditioned near 180 or 360 degrees.
//!
//! Provenance: RS-274 / ISO 6983 circular R-word semantics and published
//! controller behaviour (Fanuc, Haas, Mazak, LinuxCNC/PathPilot, grblHAL).
//! Independent RS-274 arc reader checks the posted G-code directly.
//!
//! Gate map (M4.4): covers assets/posts/fanuc.cps, assets/posts/haas.cps,
//! assets/posts/mazak.cps, assets/posts/doosan-dnm.cps,
//! assets/posts/tormach.cps, assets/posts/grblhal.cps,
//! camcraft_post::post::engine.

use camcraft_post::cl::{ArcPlane, MoveKind};
use camcraft_post::post::cnc::{self, MoveRec, Record};
use camcraft_post::post::engine;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

const RADIUS_POSTS: &[&str] = &["fanuc", "haas", "mazak", "doosan-dnm", "tormach", "grblhal"];

fn repo(rel: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../").join(rel)
}

fn read(rel: &str) -> String {
    std::fs::read_to_string(repo(rel)).unwrap_or_else(|e| panic!("{rel}: {e}"))
}

fn mv(kind: MoveKind, x: f64, y: f64, z: f64, feed: Option<f64>) -> Record {
    Record::Move(MoveRec { kind, x, y, z, feed, center: None, ccw: false, plane: ArcPlane::Xy, abc: None })
}

fn arc(x: f64, y: f64, z: f64, center: (f64, f64), ccw: bool, plane: ArcPlane, feed: f64) -> Record {
    Record::Move(MoveRec { kind: MoveKind::Feed, x, y, z, feed: Some(feed), center: Some(center), ccw, plane, abc: None })
}

fn stream_with_origin(origin: [f64; 3], start: [f64; 3], body: Vec<Record>) -> String {
    let mut recs = vec![
        Record::Param { name: "StartX".into(), value: json!(start[0]) },
        Record::Param { name: "StartY".into(), value: json!(start[1]) },
        Record::Param { name: "StartZ".into(), value: json!(start[2]) },
        Record::Param { name: "OperationName".into(), value: json!("RadiusArcCheck") },
        Record::Param { name: "ToolDiameter".into(), value: json!(6.0) },
        Record::Param { name: "SpindleSpeed".into(), value: json!(8000.0) },
        Record::Param { name: "WorkOffset".into(), value: json!(1) },
        Record::Param { name: "OriginX".into(), value: json!(origin[0]) },
        Record::Param { name: "OriginY".into(), value: json!(origin[1]) },
        Record::Param { name: "OriginZ".into(), value: json!(origin[2]) },
        Record::Param { name: "ClearancePlane".into(), value: json!(50.0) },
        Record::Param { name: "RetractPlane".into(), value: json!(20.0) },
        Record::Param { name: "TopOfStock".into(), value: json!(15.0) },
        Record::Param { name: "StockXYLeft".into(), value: json!(-50.0) },
        Record::Param { name: "StockXYRight".into(), value: json!(50.0) },
        Record::Param { name: "StockXYFront".into(), value: json!(-50.0) },
        Record::Param { name: "StockXYBack".into(), value: json!(50.0) },
        Record::Param { name: "StockZTop".into(), value: json!(15.0) },
        Record::Tool { number: 1, name: "EM-0600".into() },
        Record::Section { begin: true },
    ];
    recs.extend(body);
    recs.push(Record::Section { begin: false });
    cnc::serialize(&recs)
}

#[derive(Debug)]
struct PostedArc {
    line: String,
    plane: u8,
    ccw: bool,
    start: [f64; 3],
    end: [f64; 3],
    r: Option<f64>,
    ijk: [Option<f64>; 3],
}

fn parse_posted_arcs(program: &str) -> Vec<PostedArc> {
    let mut out = Vec::new();
    let (mut pos, mut motion, mut plane, mut xy_known) = ([0.0f64; 3], 0u32, 17u8, false);
    for raw in program.lines() {
        let t = raw.trim();
        if t.is_empty() || t.starts_with('(') || t.starts_with(';') || t.starts_with('%') || t.starts_with('O') {
            continue;
        }
        let code = t.split('(').next().unwrap_or("");
        let words: Vec<(char, f64)> = code
            .split_whitespace()
            .filter_map(|w| {
                let mut cs = w.chars();
                let l = cs.next()?;
                cs.as_str().parse::<f64>().ok().map(|v| (l, v))
            })
            .collect();
        let gs: Vec<u32> = words.iter().filter(|(l, _)| *l == 'G').map(|(_, v)| (*v * 10.0).round() as u32).collect();
        if gs.contains(&280) || gs.contains(&530) {
            continue;
        }
        for g in &gs {
            match g {
                0 | 10 | 20 | 30 => motion = *g,
                170 => plane = 17,
                180 => plane = 18,
                190 => plane = 19,
                _ => {}
            }
        }
        let get = |l: char| words.iter().find(|(c, _)| *c == l).map(|(_, v)| *v);
        let (x, y, z) = (get('X'), get('Y'), get('Z'));
        if x.is_none() && y.is_none() && z.is_none() {
            continue;
        }
        if !xy_known && x.is_none() && y.is_none() {
            pos[2] = z.unwrap_or(pos[2]);
            continue;
        }
        xy_known = true;
        let end = [x.unwrap_or(pos[0]), y.unwrap_or(pos[1]), z.unwrap_or(pos[2])];
        if motion == 20 || motion == 30 {
            out.push(PostedArc {
                line: t.to_string(),
                plane,
                ccw: motion == 30,
                start: pos,
                end,
                r: get('R'),
                ijk: [get('I'), get('J'), get('K')],
            });
        }
        pos = end;
    }
    out
}

fn plane_axes(plane: u8) -> (usize, usize) {
    match plane {
        18 => (2, 0),
        19 => (1, 2),
        _ => (0, 1),
    }
}

fn run_arcs(stem: &str, stream: &str, opts: &Value, ctx: &str) -> (String, Vec<PostedArc>) {
    let cps = read(&format!("assets/posts/{stem}.cps"));
    let out = engine::run_with(&cps, &Value::Null, stream, opts).unwrap_or_else(|e| panic!("{ctx}: {e}"));
    assert!(out.warnings.is_empty(), "{ctx}: {:?}", out.warnings);
    let arcs = parse_posted_arcs(&out.program);
    assert!(!arcs.is_empty(), "{ctx}: no arcs posted:\n{}", out.program);
    (out.program, arcs)
}

fn assert_r_arc_reachable(arc: &PostedArc, ctx: &str) -> f64 {
    let r_signed = arc.r.unwrap_or_else(|| panic!("{ctx}: expected R word in `{}`", arc.line));
    let r = r_signed.abs();
    assert!(r > 0.0, "{ctx}: zero R in `{}`", arc.line);
    let (u, v) = plane_axes(arc.plane);
    let chord = (arc.end[u] - arc.start[u]).hypot(arc.end[v] - arc.start[v]);
    assert!(chord > 0.0, "{ctx}: collapsed endpoints on R arc `{}`", arc.line);
    assert!(chord <= 2.0 * r + 1e-12, "{ctx}: chord {chord:.6} exceeds 2R {:.6} in `{}`", 2.0 * r, arc.line);
    r_signed
}

fn assert_r_arc_valid(arc: &PostedArc, expected_center: [f64; 3], expected_r: f64, tol: f64, ctx: &str) {
    let r_signed = assert_r_arc_reachable(arc, ctx);
    let r = r_signed.abs();
    let (u, v) = plane_axes(arc.plane);
    let (du, dv) = (arc.end[u] - arc.start[u], arc.end[v] - arc.start[v]);
    let chord = du.hypot(dv);
    let h = (r * r - 0.25 * chord * chord).max(0.0).sqrt();
    let (nx, ny) = (-dv / chord, du / chord);
    let left_sign = if (arc.ccw && r_signed > 0.0) || (!arc.ccw && r_signed < 0.0) { 1.0 } else { -1.0 };
    let cu = 0.5 * (arc.start[u] + arc.end[u]) + left_sign * h * nx;
    let cv = 0.5 * (arc.start[v] + arc.end[v]) + left_sign * h * ny;
    let c_err = (cu - expected_center[u]).hypot(cv - expected_center[v]);
    assert!(c_err <= tol, "{ctx}: center ({cu:.4}, {cv:.4}) drifted {c_err:.4} > {tol} in `{}`", arc.line);
    assert!((r - expected_r).abs() <= tol, "{ctx}: R={r:.4} vs {expected_r:.4} in `{}`", arc.line);
}

#[test]
fn helix_half_turns_use_reachable_r_and_stay_on_circle() {
    let cases = [
        ("issue_2191", [0.032, -0.493, 12.0], [2.501, -0.493, 11.0], [0.032, -0.493, 10.0], (1.2666, -0.493), 1.2345),
        ("half_micron", [0.032, -0.493, 12.0], [2.501, -0.493, 11.0], [0.032, -0.493, 10.0], (1.2665, -0.493), 1.2345),
        ("odd_micron_thread", [0.549, 0.0, 12.0], [0.0, 0.0, 11.5], [0.549, 0.0, 11.0], (0.275, 0.0), 0.2745),
    ];
    let opts = json!({ "useRadiusArcs": true });
    for (name, p0, p1, p2, center, exp_r) in cases {
        let stream = stream_with_origin(
            [0.0; 3],
            [p0[0], p0[1], 20.0],
            vec![
                mv(MoveKind::Rapid, p0[0], p0[1], p0[2], None),
                arc(p1[0], p1[1], p1[2], center, true, ArcPlane::Xy, 1200.0),
                arc(p2[0], p2[1], p2[2], center, true, ArcPlane::Xy, 1200.0),
                mv(MoveKind::Rapid, p2[0], p2[1], 20.0, None),
            ],
        );
        for stem in RADIUS_POSTS {
            let ctx = format!("{stem}/{name}");
            let (_, arcs) = run_arcs(stem, &stream, &opts, &ctx);
            for a in &arcs {
                assert_r_arc_reachable(a, &ctx);
            }
            let r0 = arcs[0].r.expect("r");
            for a in &arcs {
                assert_eq!(a.r, Some(r0), "{ctx}: helix half-turns wrote mismatched radii in `{}`", a.line);
                assert_r_arc_valid(a, [center.0, center.1, 0.0], exp_r, 0.003, &ctx);
            }
        }
    }
}

#[test]
fn near_180_and_full_circle_arcs_in_all_planes_hold_chord_and_center() {
    let opts = json!({ "useRadiusArcs": true });
    let (r, cx, cy, cz) = (1.2345, 10.0, 20.0, -5.0);
    for ccw in [true, false] {
        for sweep_deg in [176.0_f64, 180.0, 184.0, 359.99, 360.0] {
            let a1 = if (sweep_deg - 360.0).abs() < 1e-9 { 0.0 } else if ccw { sweep_deg.to_radians() } else { -sweep_deg.to_radians() };
            let s = [cx + r, cy, cz];
            let e = [cx + r * a1.cos(), cy + r * a1.sin(), cz - 1.0];
            let stream = stream_with_origin(
                [0.0; 3],
                [s[0], s[1], 20.0],
                vec![mv(MoveKind::Rapid, s[0], s[1], s[2], None), arc(e[0], e[1], e[2], (cx, cy), ccw, ArcPlane::Xy, 800.0), mv(MoveKind::Rapid, e[0], e[1], 20.0, None)],
            );
            for stem in RADIUS_POSTS {
                let ctx = format!("{stem} XY {sweep_deg} ccw={ccw}");
                let (_, arcs) = run_arcs(stem, &stream, &opts, &ctx);
                for a in &arcs {
                    assert_r_arc_valid(a, [cx, cy, cz], r, 0.003, &ctx);
                }
            }
        }
    }
    let xz = stream_with_origin([0.0; 3], [cx - r, cy, 20.0], vec![mv(MoveKind::Rapid, cx - r, cy, cz, None), arc(cx + r, cy, cz, (cx, cz), true, ArcPlane::Xz, 600.0), mv(MoveKind::Rapid, cx + r, cy, 20.0, None)]);
    let yz = stream_with_origin([0.0; 3], [cx, cy - r, 20.0], vec![mv(MoveKind::Rapid, cx, cy - r, cz, None), arc(cx, cy + r, cz, (cy, cz), false, ArcPlane::Yz, 600.0), mv(MoveKind::Rapid, cx, cy + r, 20.0, None)]);
    for stem in RADIUS_POSTS {
        for (label, stream, plane) in [("XZ", &xz, 18u8), ("YZ", &yz, 19u8)] {
            let ctx = format!("{stem} {label}");
            let (_, arcs) = run_arcs(stem, stream, &opts, &ctx);
            for a in &arcs {
                assert_eq!(a.plane, plane, "{ctx}: wrong plane in `{}`", a.line);
                assert_r_arc_valid(a, [cx, cy, cz], r, 0.003, &ctx);
            }
        }
    }
}

#[test]
fn inch_and_nonzero_origin_arcs_hold_after_rounding() {
    let origin = [10.0004, -5.0004, 0.0];
    let r_mm = 1.27127; // 0.05005 in -> rounds to 0.0500 in while 2*r = 0.1001 in
    let start = [origin[0], origin[1], 10.0];
    let end = [origin[0] + 2.0 * r_mm, origin[1], 9.0];
    let center = (origin[0] + r_mm, origin[1]);
    let stream = stream_with_origin(
        origin,
        [start[0], start[1], 20.0],
        vec![
            mv(MoveKind::Rapid, start[0], start[1], start[2], None),
            arc(end[0], end[1], end[2], center, true, ArcPlane::Xy, 500.0),
            arc(start[0], start[1], 8.0, center, true, ArcPlane::Xy, 500.0),
            mv(MoveKind::Rapid, start[0], start[1], 20.0, None),
        ],
    );
    let opts = json!({ "useRadiusArcs": true, "Units": "inch" });
    let exp_r = r_mm / 25.4;
    for stem in RADIUS_POSTS {
        let ctx = format!("{stem} inch");
        let (prog, arcs) = run_arcs(stem, &stream, &opts, &ctx);
        assert!(prog.contains("G20"), "{ctx} missing G20");
        for a in &arcs {
            assert_r_arc_valid(a, [exp_r, 0.0, 0.0], exp_r, 0.0003, &ctx);
        }
    }
}

#[test]
fn major_arcs_use_negative_r_and_degenerate_arcs_fall_back_to_ijk() {
    let opts = json!({ "useRadiusArcs": true });
    let major = stream_with_origin([0.0; 3], [10.0, 0.0, 20.0], vec![mv(MoveKind::Rapid, 10.0, 0.0, 0.0, None), arc(0.0, -10.0, 0.0, (0.0, 0.0), true, ArcPlane::Xy, 600.0), mv(MoveKind::Rapid, 0.0, -10.0, 20.0, None)]);
    for stem in RADIUS_POSTS {
        let ctx = format!("{stem} major");
        let (_, arcs) = run_arcs(stem, &major, &opts, &ctx);
        assert_eq!(arcs.len(), 1, "{ctx}");
        assert!(arcs[0].r.is_some_and(|r| r < 0.0), "{ctx}: expected negative R in `{}`", arcs[0].line);
        assert_r_arc_valid(&arcs[0], [0.0, 0.0, 0.0], 10.0, 0.003, &ctx);
    }

    // Inch r=0.001 mm rounds to R0, and a 5->6 mm radius mismatch in G18 XZ must fall back to I/K with G18 intact.
    let tiny = stream_with_origin([0.0; 3], [0.001, 0.0, 20.0], vec![mv(MoveKind::Rapid, 0.001, 0.0, 0.0, None), arc(0.0, 0.001, 0.0, (0.0, 0.0), true, ArcPlane::Xy, 600.0), mv(MoveKind::Rapid, 0.0, 0.001, 20.0, None)]);
    let tiny_opts = json!({ "useRadiusArcs": true, "Units": "inch" });
    let mismatch = stream_with_origin([0.0; 3], [5.0, 0.0, 20.0], vec![mv(MoveKind::Rapid, 5.0, 0.0, 0.0, None), arc(0.0, 0.0, 6.0, (0.0, 0.0), true, ArcPlane::Xz, 600.0), mv(MoveKind::Rapid, 0.0, 0.0, 20.0, None)]);
    for stem in RADIUS_POSTS {
        for (label, stream, run_opts, plane) in [("tiny_inch", &tiny, &tiny_opts, 17u8), ("mismatch", &mismatch, &opts, 18u8)] {
            let ctx = format!("{stem}/{label}");
            let (_, arcs) = run_arcs(stem, stream, run_opts, &ctx);
            assert_eq!(arcs.len(), 1, "{ctx}");
            assert_eq!(arcs[0].plane, plane, "{ctx}: wrong plane in `{}`", arcs[0].line);
            assert!(arcs[0].r.is_none(), "{ctx}: must emit I/J/K, not R: `{}`", arcs[0].line);
            assert!(arcs[0].ijk[0].is_some() && (arcs[0].ijk[1].is_some() || arcs[0].ijk[2].is_some()), "{ctx}: missing center increment in `{}`", arcs[0].line);
        }
    }
}

#[test]
fn recorded_arc_streams_post_valid_r_arcs() {
    let opts = json!({ "useRadiusArcs": true });
    for case in ["arcs", "full_circle", "full_circle_raw"] {
        let stream = read(&format!("crates/post/tests/recorded/streams/{case}.cnc"));
        for stem in RADIUS_POSTS {
            let ctx = format!("{stem}/{case}");
            let (_, arcs) = run_arcs(stem, &stream, &opts, &ctx);
            for a in &arcs {
                assert_r_arc_reachable(a, &ctx);
            }
        }
    }
}

// PR #2202: an R split must not put the whole rotary turn on its first
// segment. Keep the native I/J/K arc and its whole-arc feed cap instead.
fn assert_rotary_half_turn_preserved(stem: &str) {
    let mch = json!({ "kinematics": { "static": { "rotaries": [
        { "letter": "A", "type": "rotary", "direction": [1.0, 0.0, 0.0],
          "travel": [-120.0, 120.0], "speeds": { "normal": 90.0 } }
    ] } } });
    let with_a = |mut record: Record, a: f64| {
        if let Record::Move(m) = &mut record {
            m.abc = Some([a, 0.0, 0.0]);
        }
        record
    };
    let word = |line: &str, letter: char| {
        line.split_whitespace().find_map(|w| w.strip_prefix(letter).and_then(|v| v.parse::<f64>().ok()))
    };
    for feed in [10.0, 600.0] {
        let stream = stream_with_origin(
            [0.0; 3],
            [10.0, 0.0, 20.0],
            vec![
                with_a(mv(MoveKind::Rapid, 10.0, 0.0, 0.0, None), 0.0),
                with_a(arc(-10.0, 0.0, 0.0, (0.0, 0.0), true, ArcPlane::Xy, feed), 90.0),
                with_a(arc(10.0, 0.0, 0.0, (0.0, 0.0), true, ArcPlane::Xy, feed), 0.0),
                with_a(arc(-10.0, 0.0, 0.0, (0.0, 0.0), true, ArcPlane::Xy, feed), 0.0),
                mv(MoveKind::Rapid, -10.0, 0.0, 20.0, None),
            ],
        );
        let cps = read(&format!("assets/posts/{stem}.cps"));
        let out = engine::run_with(&cps, &mch, &stream, &json!({ "useRadiusArcs": true, "rotaryBrake": true })).unwrap();
        assert!(out.warnings.is_empty(), "{stem}: {:?}", out.warnings);
        let arcs = parse_posted_arcs(&out.program);
        assert_eq!(arcs.len(), 4, "{stem}: two rotary arcs and two fixed-angle segments:\n{}", out.program);
        let mut held_a = 0.0;
        for (a, target_a) in arcs[..2].iter().zip([90.0, 0.0]) {
            assert!(a.r.is_none(), "{stem}: changing rotary must keep one I/J/K arc: {}", a.line);
            assert_eq!(a.ijk[0], Some(-a.start[0]), "{stem}: original center");
            assert_eq!(a.ijk[1], Some(0.0), "{stem}: original center");
            let posted_a = word(&a.line, 'A').expect("rotary endpoint");
            assert_eq!(posted_a, target_a, "{stem}: endpoint angle");
            // With one continuous half-circle block, the control reaches
            // the halfway angle at X0 Y10 (then X0 Y-10 on the return).
            assert_eq!((held_a + posted_a) * 0.5, 45.0, "{stem}: midpoint angle");
            let posted_f = word(&a.line, 'F').expect("arc feed");
            assert_eq!(posted_f, feed.min(20.0), "{stem}: preserve the whole-arc feed cap");
            let minutes = std::f64::consts::PI * 10.0 / posted_f;
            assert!((posted_a - held_a).abs() / minutes <= 90.0, "{stem}: rotary rate exceeds 90 deg/min");
            held_a = posted_a;
        }
        for a in &arcs[2..] {
            assert_r_arc_valid(a, [0.0; 3], 10.0, 0.003, stem);
            assert!(word(&a.line, 'A').is_none(), "{stem}: unchanged rotary stays modal");
        }
    }
}

#[test]
fn fanuc_rotary_half_turn_keeps_continuous_angles_and_rate() {
    assert_rotary_half_turn_preserved("fanuc");
}

#[test]
fn doosan_rotary_half_turn_keeps_continuous_angles_and_rate() {
    assert_rotary_half_turn_preserved("doosan-dnm");
}
