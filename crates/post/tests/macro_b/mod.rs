//! Test-only: a Fanuc-style controller that runs the subset of custom
//! macro B the posted probing blocks use (#392), on a simulated machine
//! whose X / Y may be inverted, against a simulated part.
//!
//! What it models: G0 / G1 (every move swept for a collision with the
//! part - a stylus driven into material is a crash), G31 skip (the move
//! stops where the stylus first meets the part; `#5061..#5063` hold that
//! point in workpiece coordinates, or the block's end when nothing
//! tripped), G90 / G91, G54..G59 and their registers `#5221 + 20 x slot +
//! axis`, G10 L2 (set in G90, add in G91), G28 (the program's reference
//! return: leaves the part, not swept), `#n = expr`, `IF[..]THEN` /
//! `IF[..]GOTOn`, `#3000` (alarm) and `#3006` / M0 (operator stop).
//! Anything else is an error: an unmodelled word must fail the test,
//! never pass it.
//!
//! The frame law it simulates: the controller's machine position `M`
//! is register + program position; physically the tip stands at `S * M`
//! (S = -1 on an inverted axis). Tool length (G43 H) is taken as zero:
//! Z is never inverted, so it only shifts Z for every program alike.
//!
//! Provenance: Fanuc's published operator documentation (G31 skip and
//! its system variables, custom macro B, G10 L2 work offsets) and
//! RS-274 motion semantics. Our own code and names.
#![allow(dead_code)]

pub mod expr;

use expr::{Cursor, Vars};
use std::collections::HashMap;

pub type Pt = [f64; 3];

/// Why the program stopped.
#[derive(Clone, Debug, PartialEq)]
pub enum Stop {
    /// M30 / M2, or the end of the text.
    End,
    /// `#3000 = n (message)`: the control alarms.
    Alarm(u32, String),
    /// `#3006 = n (message)` or M0: the program stops for the operator.
    Halt(String),
    /// A G0 / G1 drove the stylus into the part: the line (1-based) and
    /// the physical point.
    Crash(usize, Pt),
}

/// The outcome: the stop, the work-offset registers as the program
/// left them, and every physical point a G31 tripped at.
#[derive(Clone, Debug)]
pub struct Run {
    pub stop: Stop,
    pub offsets: [Pt; 6],
    pub trips: Vec<Pt>,
}

/// The sweep step, mm: finer than any feature the tests probe.
const STEP: f64 = 0.005;

struct Ctl<'a> {
    sign: Pt,
    material: &'a dyn Fn(Pt) -> bool,
    vars: HashMap<u32, f64>,
    offsets: [Pt; 6],
    active: usize,
    abs: bool,
    motion: u32,
    /// Controller machine coordinates; `None` before the first move and
    /// after a reference return.
    pos: Option<Pt>,
    /// #1633: Z alone, while `pos` is `None` - the first traverse's height,
    /// stated after a tool change before X/Y are known.
    z_only: Option<f64>,
    skip: Pt,
    trips: Vec<Pt>,
}

impl Vars for Ctl<'_> {
    fn read(&self, n: u32) -> Result<f64, String> {
        match n {
            5061..=5063 => Ok(self.skip[(n - 5061) as usize]),
            5221..=5323 if (n - 5221) % 20 < 3 => {
                let k = (n - 5221) as usize;
                Ok(self.offsets[k / 20][k % 20])
            }
            _ => self.vars.get(&n).copied().ok_or_else(|| format!("read of vacant #{n}")),
        }
    }
}

/// Run `program` on a controller with registers `offsets` (G54..G59,
/// controller machine coordinates), axes `flip` inverted, and the part
/// `material(p)` true at physical tip points `p` inside it. `Err` is a
/// block this reader does not model.
pub fn run(program: &str, flip: [bool; 3], offsets: [Pt; 6], material: &dyn Fn(Pt) -> bool) -> Result<Run, String> {
    let s = |on: bool| if on { -1.0 } else { 1.0 };
    let mut c = Ctl {
        sign: [s(flip[0]), s(flip[1]), s(flip[2])],
        material,
        vars: HashMap::new(),
        offsets,
        active: 0,
        abs: true,
        motion: 0,
        pos: None,
        z_only: None,
        skip: [0.0; 3],
        trips: Vec::new(),
    };
    let lines: Vec<&str> = program.lines().collect();
    let (mut pc, mut steps) = (0usize, 0usize);
    while let Some(line) = lines.get(pc) {
        steps += 1;
        if steps > 100_000 {
            return Err("the program does not end (a GOTO loop?)".into());
        }
        let n = pc + 1;
        pc += 1;
        match c.line(line, n).map_err(|e| format!("line {n} {line:?}: {e}"))? {
            Flow::Next => {}
            Flow::Goto(label) => {
                pc = lines
                    .iter()
                    .position(|l| {
                        let mut c = Cursor::new(l);
                        c.kw("N") && c.num().ok() == Some(f64::from(label))
                    })
                    .ok_or_else(|| format!("line {n}: GOTO {label} names no block"))?;
            }
            Flow::Stop(stop) => return Ok(Run { stop, offsets: c.offsets, trips: c.trips }),
        }
    }
    Ok(Run { stop: Stop::End, offsets: c.offsets, trips: c.trips })
}

enum Flow {
    Next,
    Goto(u32),
    Stop(Stop),
}

/// The block without its `( )` comments, and the comments' text.
fn split_comment(line: &str) -> (String, String) {
    let (mut code, mut note, mut depth) = (String::new(), String::new(), 0);
    for ch in line.chars() {
        match ch {
            '(' => depth += 1,
            ')' if depth > 0 => depth -= 1,
            _ if depth > 0 => note.push(ch),
            _ => code.push(ch),
        }
    }
    (code, note)
}

impl Ctl<'_> {
    fn line(&mut self, raw: &str, n: usize) -> Result<Flow, String> {
        let (code, note) = split_comment(raw);
        let mut cur = Cursor::new(&code);
        if cur.done() || cur.peek() == Some(b'%') {
            return Ok(Flow::Next);
        }
        if cur.peek() == Some(b'O') {
            return Ok(Flow::Next); // the program number
        }
        if cur.kw("N") {
            cur.num()?;
        }
        if cur.kw("IF[") {
            let yes = cur.cond(self)?;
            cur.want("]")?;
            if cur.kw("GOTO") {
                let to = cur.num()?;
                return Ok(if yes { Flow::Goto(to as u32) } else { Flow::Next });
            }
            cur.want("THEN")?;
            if !yes {
                return Ok(Flow::Next);
            }
            return self.assign(&mut cur, &note);
        }
        if cur.peek() == Some(b'#') {
            return self.assign(&mut cur, &note);
        }
        self.block(&mut cur, n)
    }

    fn assign(&mut self, cur: &mut Cursor, note: &str) -> Result<Flow, String> {
        cur.want("#")?;
        let to = cur.var_no(self)?;
        cur.want("=")?;
        let v = cur.expr(self)?;
        if !cur.done() {
            return Err(format!("trailing text {:?}", cur.rest()));
        }
        match to {
            3000 => return Ok(Flow::Stop(Stop::Alarm(v as u32, note.trim().to_owned()))),
            3006 => return Ok(Flow::Stop(Stop::Halt(note.trim().to_owned()))),
            5221..=5323 if (to - 5221) % 20 < 3 => {
                let k = (to - 5221) as usize;
                self.offsets[k / 20][k % 20] = v;
            }
            100..=199 | 500..=999 => {
                self.vars.insert(to, v);
            }
            _ => return Err(format!("#{to} is not a variable this reader writes")),
        }
        Ok(Flow::Next)
    }

    fn block(&mut self, cur: &mut Cursor, n: usize) -> Result<Flow, String> {
        let mut words: Vec<(char, f64)> = Vec::new();
        while !cur.done() {
            let l = cur.letter().ok_or_else(|| format!("expected an address at {:?}", cur.rest()))?;
            words.push((l, cur.unary(self)?));
        }
        let get = |l: char| words.iter().find(|(c, _)| *c == l).map(|(_, v)| *v);
        let (mut skip, mut g10, mut home) = (false, false, false);
        for (l, v) in &words {
            let code = (v * 10.0).round() as i64;
            match (l, code) {
                ('G', 0 | 10) => self.motion = code as u32,
                ('G', 310) => skip = true,
                ('G', 900) => self.abs = true,
                ('G', 910) => self.abs = false,
                ('G', 100) => g10 = true,
                ('G', 280) => home = true,
                ('G', 540..=590) if code % 10 == 0 => self.active = (code / 10 - 54) as usize,
                // G21: the program's unit (#901) - this reader works in mm;
                // G20 (inch) stays unmodelled, so an inch program is an error.
                ('G', 170 | 210 | 400 | 430 | 490 | 800) => {}
                ('M', 0) => return Ok(Flow::Stop(Stop::Halt("M0".into()))),
                ('M', 20 | 300) => return Ok(Flow::Stop(Stop::End)),
                ('M', 10 | 50 | 60 | 80 | 90) | ('T' | 'H' | 'F' | 'S', _) => {}
                ('X' | 'Y' | 'Z', _) => {}
                ('L' | 'P', _) if g10 || words.iter().any(|(c, w)| *c == 'G' && *w == 10.0) => {}
                _ => return Err(format!("{l}{v} is not modelled")),
            }
        }
        if g10 {
            if get('L') != Some(2.0) {
                return Err("only G10 L2 (work offsets) is modelled".into());
            }
            let p = get('P').ok_or("G10 L2 needs P")?;
            if !(1.0..=6.0).contains(&p) || p.fract() != 0.0 {
                return Err(format!("G10 L2 P{p} is not G54..G59"));
            }
            let slot = p as usize - 1;
            for (a, l) in ['X', 'Y', 'Z'].into_iter().enumerate() {
                if let Some(v) = get(l) {
                    self.offsets[slot][a] = if self.abs { v } else { self.offsets[slot][a] + v };
                }
            }
            return Ok(Flow::Next);
        }
        if home {
            self.pos = None; // to the reference point, away from the part
            self.z_only = None;
            return Ok(Flow::Next);
        }
        let given = [get('X'), get('Y'), get('Z')];
        if given.iter().all(Option::is_none) {
            return Ok(Flow::Next);
        }
        // #1633: a Z-only absolute move from an unknown position (the
        // first traverse's stated height): X/Y stay unknown - away from
        // the part, as after a reference return - and Z is known.
        if let (None, None, Some(z), None, true, false) = (given[0], given[1], given[2], self.pos, self.abs, skip) {
            self.z_only = Some(self.offsets[self.active][2] + z);
            return Ok(Flow::Next);
        }
        let mut end = [0.0; 3];
        for a in 0..3 {
            end[a] = match (given[a], self.pos, self.abs) {
                (Some(v), _, true) => self.offsets[self.active][a] + v,
                (Some(v), Some(p), false) => p[a] + v,
                (None, Some(p), _) => p[a],
                (None, None, _) if a == 2 && self.z_only.is_some() => self.z_only.unwrap_or_default(),
                _ => return Err("a move from an unknown position".into()),
            };
        }
        if skip {
            let from = self.pos.ok_or("G31 from an unknown position")?;
            let at = self.skip_to(from, end);
            self.skip = [0, 1, 2].map(|a| at[a] - self.offsets[self.active][a]);
            self.pos = Some(at);
            return Ok(Flow::Next);
        }
        if let Some(hit) = self.sweep(self.pos, end) {
            return Ok(Flow::Stop(Stop::Crash(n, hit)));
        }
        self.pos = Some(end);
        Ok(Flow::Next)
    }

    fn phys(&self, m: Pt) -> Pt {
        [0, 1, 2].map(|a| self.sign[a] * m[a])
    }

    fn lerp(a: Pt, b: Pt, t: f64) -> Pt {
        [0, 1, 2].map(|k| a[k] + (b[k] - a[k]) * t)
    }

    fn samples(a: Pt, b: Pt) -> usize {
        let len = ((b[0] - a[0]).powi(2) + (b[1] - a[1]).powi(2) + (b[2] - a[2]).powi(2)).sqrt();
        ((len / STEP).ceil() as usize).max(1)
    }

    /// The first physical point of a feed / rapid inside the part.
    fn sweep(&self, from: Option<Pt>, to: Pt) -> Option<Pt> {
        let Some(a) = from else {
            let p = self.phys(to);
            return (self.material)(p).then_some(p);
        };
        let n = Self::samples(a, to);
        (1..=n).map(|k| self.phys(Self::lerp(a, to, k as f64 / n as f64))).find(|p| (self.material)(*p))
    }

    /// A G31 from `a` toward `b`: the controller position where the
    /// stylus first meets the part (bisected to well under a micron), or
    /// `b` when it never does.
    fn skip_to(&mut self, a: Pt, b: Pt) -> Pt {
        let n = Self::samples(a, b);
        let inside = |t: f64| (self.material)(self.phys(Self::lerp(a, b, t)));
        let Some(k) = (1..=n).find(|k| inside(*k as f64 / n as f64)) else {
            return b;
        };
        let (mut lo, mut hi) = ((k - 1) as f64 / n as f64, k as f64 / n as f64);
        for _ in 0..60 {
            let mid = 0.5 * (lo + hi);
            if inside(mid) {
                hi = mid;
            } else {
                lo = mid;
            }
        }
        let at = Self::lerp(a, b, lo);
        self.trips.push(self.phys(at));
        at
    }
}
