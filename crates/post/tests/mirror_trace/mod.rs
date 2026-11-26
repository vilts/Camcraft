//! A dialect-tolerant controller reader for the mirror suites (#2475):
//! it reads a posted mill program as a controller whose axes `flip` run
//! reversed, and returns the physical cuts as dense polylines, so that a
//! dialect that splits an arc, writes it as chords, expands a drilled
//! hole or calls a cycle still compares with the CAM path point by point.
//!
//! Provenance: RS-274 / NIST RS274NGC (NISTIR 6300) word semantics - G0
//! G1 G2 G3, G17/G18/G19, I/J/K as start-relative centre increments,
//! G2/G3 sense seen from the plane normal's positive end, G41/G42 left /
//! right of the travel, G81-family holes (X Y, bottom Z, retract R). The
//! Heidenhain DIN/ISO forms (an `I J` block as the absolute circle
//! centre, G200 Q201 / Q203 with an M99 call) follow the reading that
//! `assets/posts/heidenhain.cps` cites. Fixture values are our own.
#![allow(dead_code)]

use camcraft_post::cl::{ArcPlane, CompSide, MoveKind};
use camcraft_post::post::cnc::Record;
use std::collections::HashMap;

use crate::inverted_support::ORIGIN;

/// What a program does, in physical (CAM) millimetres.
#[derive(Debug, Default)]
pub struct Trace {
    /// Each run of cutting moves between rapids, start point first.
    pub cuts: Vec<Vec<[f64; 3]>>,
    /// Where each rapid ends, once X and Y are known.
    pub rapids: Vec<[f64; 3]>,
    /// The physical compensation side, with the cut run it engages in.
    pub engages: Vec<(usize, CompSide)>,
}

/// The largest chord sagitta of a sampled arc, mm.
const SAG: f64 = 0.0002;

fn axes(plane: u8) -> (usize, usize, usize) {
    match plane {
        18 => (2, 0, 1), // G18: Z X about +Y
        19 => (1, 2, 0), // G19: Y Z about +X
        _ => (0, 1, 2),
    }
}

/// Points along an arc (a helix when the normal coordinate travels) from
/// `s` to `e` about the in-plane centre `c`, the end last. CCW is a
/// positive rotation about the plane normal (the RS-274 sense).
pub fn arc_dense(s: [f64; 3], e: [f64; 3], c: [f64; 3], plane: u8, ccw: bool) -> Vec<[f64; 3]> {
    let (u, v, h) = axes(plane);
    let r = ((s[u] - c[u]).powi(2) + (s[v] - c[v]).powi(2)).sqrt();
    let a0 = (s[v] - c[v]).atan2(s[u] - c[u]);
    let a1 = (e[v] - c[v]).atan2(e[u] - c[u]);
    let tau = std::f64::consts::TAU;
    let mut sweep = if ccw { a1 - a0 } else { a0 - a1 };
    if sweep <= 1e-12 {
        sweep += tau;
    }
    let step = 2.0 * (1.0 - (SAG / r.max(1e-9)).min(1.0)).acos();
    let n = ((sweep / step.max(1e-6)).ceil() as usize).clamp(2, 100_000);
    let mut out = Vec::with_capacity(n);
    for k in 1..n {
        let t = k as f64 / n as f64;
        let a = a0 + if ccw { sweep * t } else { -sweep * t };
        let mut p = [0.0; 3];
        p[u] = c[u] + r * a.cos();
        p[v] = c[v] + r * a.sin();
        p[h] = s[h] + (e[h] - s[h]) * t;
        out.push(p);
    }
    out.push(e);
    out
}

#[derive(Default)]
struct Builder {
    t: Trace,
    cutting: bool,
}

impl Builder {
    fn cut(&mut self, from: [f64; 3], pts: Vec<[f64; 3]>) {
        if !self.cutting {
            self.t.cuts.push(vec![from]);
            self.cutting = true;
        }
        if let Some(run) = self.t.cuts.last_mut() {
            run.extend(pts);
        }
    }

    fn rapid(&mut self, to: [f64; 3]) {
        self.cutting = false;
        self.t.rapids.push(to);
    }

    /// The run a cut written now belongs to.
    fn run(&self) -> usize {
        self.t.cuts.len().saturating_sub(usize::from(self.cutting))
    }
}

/// The CAM path of a record stream: what the machine must do. A drilled
/// hole is its expanded chain (the moves after the CYCLE record).
pub fn cam_trace(records: &[Record]) -> Trace {
    let mut b = Builder::default();
    let mut pos = [0.0; 3];
    let mut pending: Option<CompSide> = None;
    for r in records {
        match r {
            Record::Move(m) => {
                let e = [m.x, m.y, m.z];
                if m.kind == MoveKind::Rapid {
                    b.rapid(e);
                } else {
                    if let Some(side) = pending.take() {
                        b.t.engages.push((b.run(), side));
                    }
                    let pts = match m.center {
                        None => vec![e],
                        Some((ca, cb)) => {
                            let (plane, c) = match m.plane {
                                ArcPlane::Xy => (17, [ca, cb, 0.0]),
                                ArcPlane::Xz => (18, [ca, 0.0, cb]),
                                ArcPlane::Yz => (19, [0.0, ca, cb]),
                            };
                            arc_dense(pos, e, c, plane, m.ccw)
                        }
                    };
                    b.cut(pos, pts);
                }
                pos = e;
            }
            Record::Comp { side: Some(s), .. } => pending = Some(*s),
            _ => {}
        }
    }
    b.t
}

/// How a dialect's words differ from plain RS-274.
#[derive(Clone, Copy, Debug, Default)]
pub struct Dialect {
    /// Heidenhain DIN/ISO: an `I J` block states the absolute circle
    /// centre for the next G02/G03; a G200 definition is called by M99.
    pub heidenhain: bool,
}

/// The G words this reader models or knows to be no part of the path,
/// times ten (G54.1 is 541). Any other G word panics, so an unexpected
/// dialect turn (G91, G68, a mirror word) cannot pass as a trace.
const KNOWN_G: &[u32] = &[
    0, 10, 20, 30, 40, 170, 180, 190, // motion, dwell, plane
    210, 710, 7100, // millimetres: G21, Heidenhain G71, Sinumerik G710
    280, 300, 530, // machine-frame returns: the line is skipped
    400, 410, 420, 430, 490, // cutter compensation, tool length
    540, 550, 560, 570, 580, 590, 610, 640, // work offsets, exact stop / blending
    730, 800, 810, 820, 830, 850, 860, 890, 980, 990, // canned cycles and their return level
    900, 911, 940, // absolute positions, incremental arc centres, feed per minute
];

/// The centre of an R-word arc (RS-274: a positive R is the arc of 180
/// degrees or less, a negative R the larger one).
fn radius_centre(s: [f64; 3], e: [f64; 3], r: f64, plane: u8, ccw: bool) -> [f64; 3] {
    let (u, v, _) = axes(plane);
    let (du, dv) = (e[u] - s[u], e[v] - s[v]);
    let chord = (du * du + dv * dv).sqrt();
    assert!(chord > 1e-9 && chord <= 2.0 * r.abs() + 2e-3, "an R arc that cannot reach its end: chord {chord}, R {r}");
    let h = (r * r - chord * chord / 4.0).max(0.0).sqrt();
    // The centre is left of the chord for a CCW minor arc.
    let side = if ccw == (r > 0.0) { 1.0 } else { -1.0 };
    let mut c = s;
    c[u] = (s[u] + e[u]) / 2.0 - side * h * dv / chord;
    c[v] = (s[v] + e[v]) / 2.0 + side * h * du / chord;
    c
}

fn strip(line: &str) -> String {
    let mut out = String::new();
    let mut depth = 0u32;
    for ch in line.chars() {
        match ch {
            '(' => depth += 1,
            ')' => depth = depth.saturating_sub(1),
            ';' if depth == 0 => break,
            _ if depth == 0 => out.push(ch),
            _ => {}
        }
    }
    out.trim().trim_end_matches('*').trim().to_owned()
}

/// Read a posted program as a controller whose axes `flip` are reversed
/// does: controller coordinate `c` on a reversed axis is physical `-c`,
/// plus the work-offset origin. Panics (test-only) on a G word it does
/// not model.
pub fn controller_trace(program: &str, flip: [bool; 3], d: Dialect) -> Trace {
    let sign = |i: usize| if flip[i] { -1.0 } else { 1.0 };
    let phys = |c: [f64; 3]| [sign(0) * c[0] + ORIGIN[0], sign(1) * c[1] + ORIGIN[1], sign(2) * c[2] + ORIGIN[2]];
    let mut b = Builder::default();
    let mut pos: [Option<f64>; 3] = [None; 3];
    let (mut motion, mut plane) = (0u32, 17u8);
    // The open canned cycle's modal bottom and retract.
    let mut cycle: Option<(Option<f64>, Option<f64>)> = None;
    let mut q: HashMap<u32, f64> = HashMap::new();
    let mut centre: Option<(f64, f64)> = None;
    for raw in program.lines() {
        let line = strip(raw);
        if line.is_empty() || line.starts_with('%') || line.starts_with('O') {
            continue;
        }
        let mut words: Vec<(char, f64)> = Vec::new();
        let mut m99 = false;
        for w in line.split_whitespace() {
            if let Some((name, value)) = w.split_once('=') {
                if let (Some(n), Ok(v)) = (name.strip_prefix('Q').and_then(|n| n.parse::<u32>().ok()), value.parse::<f64>()) {
                    q.insert(n, v);
                }
                continue;
            }
            let mut ch = w.chars();
            let Some(l) = ch.next() else { continue };
            if let Ok(v) = ch.as_str().parse::<f64>() {
                if l == 'N' && words.is_empty() {
                    continue; // the block number
                }
                m99 |= l == 'M' && (v - 99.0).abs() < 1e-9;
                words.push((l, v));
            }
        }
        let gs: Vec<u32> = words.iter().filter(|(l, _)| *l == 'G').map(|(_, v)| (v * 10.0).round() as u32).collect();
        let home = gs.iter().any(|g| [280, 300, 530].contains(g));
        for g in &gs {
            // Heidenhain: G2xx is a cycle definition or a preset.
            let cycle_def = d.heidenhain && (2000..=2999).contains(g);
            // `G91 G28 Z0`: the increment belongs to the return only.
            let home_inc = home && *g == 910;
            assert!(KNOWN_G.contains(g) || cycle_def || home_inc, "G{} is not modelled: {raw}", f64::from(*g) / 10.0);
        }
        let get = |l: char| words.iter().find(|(c, _)| *c == l).map(|(_, v)| *v);
        if home {
            // A machine-frame return: where it ends is not the program's
            // to know.
            for (i, l) in ['X', 'Y', 'Z'].into_iter().enumerate() {
                if get(l).is_some() {
                    pos[i] = None;
                }
            }
            b.cutting = false;
            continue;
        }
        if gs.iter().any(|g| (2000..=2999).contains(g)) {
            continue; // a cycle definition or a preset: its Q words are read
        }
        let mut comp: Option<u32> = None;
        for g in &gs {
            match g {
                0 | 10 | 20 | 30 => {
                    motion = *g;
                    cycle = None;
                }
                170 => plane = 17,
                180 => plane = 18,
                190 => plane = 19,
                410 | 420 => comp = Some(*g),
                800 => cycle = None,
                730 | 810 | 820 | 830 | 850 | 860 | 890 => cycle = Some(cycle.unwrap_or((None, None))),
                _ => {}
            }
        }
        let (x, y, z) = (get('X'), get('Y'), get('Z'));
        if d.heidenhain && x.is_none() && y.is_none() && z.is_none() {
            if let (Some(i), Some(j)) = (get('I'), get('J')) {
                centre = Some((i, j));
            }
            continue;
        }
        if x.is_none() && y.is_none() && z.is_none() {
            continue;
        }
        if let Some((bottom, retract)) = cycle.as_mut() {
            // A hole: X Y here, cut from the retract plane to the bottom.
            *bottom = z.or(*bottom);
            *retract = get('R').or(*retract);
            let hx = x.or(pos[0]).unwrap_or_else(|| panic!("a hole with no X: {raw}"));
            let hy = y.or(pos[1]).unwrap_or_else(|| panic!("a hole with no Y: {raw}"));
            let (zb, zr) = match (*bottom, *retract) {
                (Some(zb), Some(zr)) => (zb, zr),
                _ => panic!("a canned cycle with no Z or R: {raw}"),
            };
            b.cutting = false;
            b.cut(phys([hx, hy, zr]), vec![phys([hx, hy, zb])]);
            b.cutting = false;
            pos = [Some(hx), Some(hy), Some(zr)];
            continue;
        }
        let end = [x.or(pos[0]), y.or(pos[1]), z.or(pos[2])];
        if motion == 0 {
            if m99 {
                // Heidenhain: the positioning block calls the defined
                // cycle once - from this Z down to surface + depth.
                let known = |k: usize| end[k].unwrap_or_else(|| panic!("a cycle call at an unknown position: {raw}"));
                let at = [known(0), known(1), known(2)];
                let zb = q.get(&203).copied().unwrap_or(f64::NAN) + q.get(&201).copied().unwrap_or(f64::NAN);
                assert!(zb.is_finite(), "M99 without a G200 definition: {raw}");
                b.cutting = false;
                b.cut(phys(at), vec![phys([at[0], at[1], zb])]);
                b.cutting = false;
            } else if let (Some(ex), Some(ey), Some(ez)) = (end[0], end[1], end[2]) {
                b.rapid(phys([ex, ey, ez]));
            } else {
                b.cutting = false;
            }
            pos = end;
            continue;
        }
        let known = |p: [Option<f64>; 3], what: &str| -> [f64; 3] {
            [0, 1, 2].map(|k| p[k].unwrap_or_else(|| panic!("a cut with an unknown {what} position: {raw}")))
        };
        let (from, to) = (known(pos, "start"), known(end, "end"));
        if let Some(g) = comp {
            assert_eq!(motion, 10, "comp engages on a G1: {raw}");
            // The controller offsets left (G41) / right (G42) of its own
            // travel; map both vectors to the physical frame and read
            // which side that is there.
            let dv = [to[0] - from[0], to[1] - from[1]];
            let left = [-dv[1], dv[0]];
            let off = if g == 410 { left } else { [-left[0], -left[1]] };
            let (dp, op) = ([sign(0) * dv[0], sign(1) * dv[1]], [sign(0) * off[0], sign(1) * off[1]]);
            let side = if dp[0] * op[1] - dp[1] * op[0] > 0.0 { CompSide::Left } else { CompSide::Right };
            b.t.engages.push((b.run(), side));
        }
        let pts: Vec<[f64; 3]> = match motion {
            20 | 30 => {
                if d.heidenhain {
                    assert_eq!(from[2], to[2], "a Heidenhain helix requires polar interpolation: {raw}");
                }
                let c = if let Some(r) = get('R') {
                    radius_centre(from, to, r, plane, motion == 30)
                } else if d.heidenhain {
                    let (i, j) = centre.take().unwrap_or_else(|| panic!("an arc with no centre block: {raw}"));
                    assert_eq!(plane, 17, "a Heidenhain arc is in the main plane: {raw}");
                    [i, j, from[2]]
                } else {
                    let (i, j, k) = (get('I').unwrap_or(0.0), get('J').unwrap_or(0.0), get('K').unwrap_or(0.0));
                    [from[0] + i, from[1] + j, from[2] + k]
                };
                arc_dense(from, to, c, plane, motion == 30)
            }
            _ => vec![to],
        };
        b.cut(phys(from), pts.into_iter().map(phys).collect());
        pos = end;
    }
    b.t
}

fn dist(a: [f64; 3], b: [f64; 3]) -> f64 {
    ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt()
}

/// The polyline's point at arc length `s` from its start.
fn at(run: &[[f64; 3]], lens: &[f64], s: f64) -> [f64; 3] {
    let k = lens.partition_point(|l| *l < s).clamp(1, run.len() - 1);
    let (a, b) = (run[k - 1], run[k]);
    let seg = lens[k] - lens[k - 1];
    let t = if seg > 1e-12 { ((s - lens[k - 1]) / seg).clamp(0.0, 1.0) } else { 1.0 };
    [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t, a[2] + (b[2] - a[2]) * t]
}

fn lengths(run: &[[f64; 3]]) -> Vec<f64> {
    let mut out = vec![0.0];
    for w in run.windows(2) {
        out.push(out[out.len() - 1] + dist(w[0], w[1]));
    }
    out
}

/// The machine's trace is on the CAM path within `tol` mm: the same cut
/// runs, each compared point by point at equal fractions of its length
/// (so a reversed or mirrored run strays); the same physical
/// compensation side in the same run; and no rapid ends over an X Y
/// that the CAM path never visits.
pub fn assert_same_trace(want: &Trace, got: &Trace, tol: f64, what: &str) {
    assert_eq!(want.cuts.len(), got.cuts.len(), "{what}: cut run count");
    for (n, (a, b)) in want.cuts.iter().zip(&got.cuts).enumerate() {
        assert!(a.len() >= 2 && b.len() >= 2, "{what}: run {n} is empty");
        let (la, lb) = (lengths(a), lengths(b));
        let (ta, tb) = (la[la.len() - 1], lb[lb.len() - 1]);
        assert!((ta - tb).abs() <= tol.max(ta * 1e-3), "{what}: run {n} length: CAM {ta:.4} vs machine {tb:.4}");
        let steps = ((ta / 0.2).ceil() as usize).max(4);
        for k in 0..=steps {
            let f = k as f64 / steps as f64;
            let (p, m) = (at(a, &la, ta * f), at(b, &lb, tb * f));
            let dd = dist(p, m);
            assert!(dd <= tol, "{what}: run {n} strays {dd:.4} mm at {:.0}%: CAM {p:?} vs machine {m:?}", f * 100.0);
        }
    }
    assert_eq!(want.engages, got.engages, "{what}: compensation side by cut run");
    assert!(!got.rapids.is_empty(), "{what}: the program has rapids");
    let visited: Vec<[f64; 3]> = want.rapids.iter().copied().chain(want.cuts.iter().flatten().copied()).collect();
    for r in &got.rapids {
        let near = visited.iter().any(|p| (p[0] - r[0]).abs() <= tol && (p[1] - r[1]).abs() <= tol);
        assert!(near, "{what}: a rapid ends at {r:?}, over an X Y that the CAM path never visits");
    }
}
