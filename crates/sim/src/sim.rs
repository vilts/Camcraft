//! Move simulation: the timeline law that turns generated toolpaths
//! into time.
//!
//! Provenance: our own design; time = distance ÷ rate is general
//! machining arithmetic (machine-tool handbooks), and the traverse
//! numbers ride the `.mch` kinematics specced in
//! `docs/CNC-INTERMEDIATE.md` - the same machine file the post reads,
//! so the simulation and the program disagree about nothing.
//!
//! The chain is the truth (M1.6's law): the sim walks the expanded
//! moves - rapids, feeds, plunge hops, tapping retracts - never the
//! cycle records, and so it sees exactly what verify saw and the
//! overlay draws. Since M5.1 stage G the chain carries its ORIENTATIONS
//! too: every seg knows the A/B/C letters it starts and ends with
//! (`cl::pose`'s canonical convention - the same letters the post
//! commands), the machine's deg/min rates pace the turning legs, and
//! a frame's tool axis interpolates by each family's own clock. A
//! prismatic document - every one this sim ever ran before stage G -
//! holds `home` letters throughout: identical totals, identical
//! frames. Cycles contribute one thing only: their dwell, the
//! seconds the machine waits at the bottom, which the move chain
//! cannot express (a zero-length move would poison the distance
//! laws). Between operations the sim inserts the connector rapid the
//! machine physically makes, so one document is one continuous
//! timeline.

use crate::cl::{ArcMove, Move, MoveKind, Toolpath};
use crate::geometry::Point3;
use serde_json::Value;
use std::f64::consts::PI;

/// Axis traverse rates, mm/min, read off the machine's X/Y/Z rapids.
/// The default stands in when a machine file names no usable axis:
/// the shipped 3-axis VMC's own numbers. The rotary rates (M5.1
/// stage G) are degrees per minute under their OWN roof - an angle's
/// pace never crosses into the mm/min fields above (stage C's unit
/// law, kept) - indexed by RS-274 address: `[0]` is A, `[1]` is B,
/// `[2]` is C. `None` is the unstated-limit law again: a machine that
/// states no rate for a letter is given no wait for it; the timeline
/// never invents a pause the file did not promise.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Traverse {
    pub xy: f64,
    pub z: f64,
    pub rot: [RotRate; 3],
}

/// One rotary's stated rates, degrees per minute.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct RotRate {
    /// Coordinated (G1) pace - what turns the head while the cut runs.
    pub normal: Option<f64>,
    /// Positioning (G0) pace - what turns the head through the air.
    pub rapid: Option<f64>,
}

impl Default for Traverse {
    fn default() -> Self {
        Self {
            xy: 6000.0,
            z: 4000.0,
            rot: [RotRate::default(); 3],
        }
    }
}

fn rate(v: Option<&Value>) -> Option<f64> {
    let v = v?.as_f64()?;
    (v.is_finite() && v > 0.0).then_some(v)
}

/// The RS-274 rotary address as its timeline slot: A turns about X,
/// B about Y, C about Z - everything else is not a letter the post's
/// motion model commands (`pose.rs`'s convention, same inventory
/// `machine::is_rotary_letter` keeps).
fn rot_slot(letter: &str) -> Option<usize> {
    match letter {
        "A" => Some(0),
        "B" => Some(1),
        "C" => Some(2),
        _ => None,
    }
}

impl Traverse {
    /// The rapid rates from a parsed `.mch`: XY takes the *slowest*
    /// horizontal rapid (a simultaneous two-axis move is paced by its
    /// slowest axis), Z its own. Unreadable files leave the defaults
    /// standing. Since stage G the rotary entries carry the same
    /// `speeds` shape under `kinematics.static.rotaries`, in degrees
    /// per minute, and land in the letter's own slot (an A/B/C letter
    /// that is not an address - a Q slide - is not a rotation the
    /// post's motion model commands, and joins nothing).
    #[must_use]
    pub fn from_machine(m: &Value) -> Self {
        let mut t = Self::default();
        for r in m
            .get("kinematics")
            .and_then(|k| k.get("static"))
            .and_then(|s| s.get("rotaries"))
            .and_then(|a| a.as_array())
            .map(|v| v.iter())
            .unwrap_or_default()
        {
            let Some(i) = r
                .get("letter")
                .and_then(Value::as_str)
                .and_then(rot_slot)
            else {
                continue;
            };
            let sp = r.get("speeds");
            t.rot[i] = RotRate {
                normal: sp.and_then(|s| rate(s.get("normal"))),
                rapid: sp.and_then(|s| rate(s.get("rapid"))),
            };
        }
        let Some(axes) = m
            .get("kinematics")
            .and_then(|k| k.get("static"))
            .and_then(|s| s.get("axes"))
            .and_then(|a| a.as_array())
        else {
            return t;
        };
        let mut xy: Option<f64> = None;
        for ax in axes {
            let dir: Vec<f64> = ax
                .get("direction")
                .and_then(|d| d.as_array())
                .map(|a| a.iter().filter_map(|x| x.as_f64()).collect())
                .unwrap_or_default();
            let Some(rapid) = ax.get("speeds").and_then(|s| rate(s.get("rapid"))) else {
                continue;
            };
            // A Z-dominant direction owns the Z rate; anything else
            // participates in the XY pace.
            let dz = dir.get(2).copied().unwrap_or(0.0).abs();
            if dz > 0.5 {
                t.z = rapid;
            } else {
                xy = Some(xy.map_or(rapid, |c| c.min(rapid)));
            }
        }
        if let Some(v) = xy {
            t.xy = v;
        }
        t
    }

    /// Simultaneous-axis rapid time: each axis travels on its own
    /// rate; the move ends when its slowest axis arrives. The rotary
    /// legs' time joins through [`Traverse::rot_secs`] at the move
    /// law, so this stays the linear story - and the prismatic guard
    /// (no usable traverse rates, no time) its history had.
    fn rapid_secs(&self, d: Point3) -> f64 {
        if !(self.xy > 0.0 && self.z > 0.0) {
            return 0.0;
        }
        60.0 * (d.x.abs() / self.xy)
            .max(d.y.abs() / self.xy)
            .max(d.z.abs() / self.z)
    }

    /// The rotary leg: each commanded letter arrives on its own rate
    /// and the axis group waits for its slowest. An unstated rate is
    /// no wait at all (the timeline will not invent a pause the file
    /// did not promise), and a letter with no motion contributes zero
    /// - the same totals the linear law would take on a zero leg.
    fn rot_secs(&self, dabc: [f64; 3], rapid: bool) -> f64 {
        let mut best: f64 = 0.0;
        for (i, d) in dabc.iter().enumerate() {
            let r = if rapid {
                self.rot[i].rapid.or(self.rot[i].normal)
            } else {
                self.rot[i].normal.or(self.rot[i].rapid)
            };
            if let Some(r) = r.filter(|r| r.is_finite() && *r > 0.0) {
                best = best.max(60.0 * d.abs() / r);
            }
        }
        best
    }
}

/// One traversed move on the timeline. Arcs stay exact (the sweep is
/// interpolated by angle, never pre-flattened - display and verify
/// flatten, the timeline should not).
#[derive(Clone, Debug, PartialEq)]
pub struct Seg {
    pub from: Point3,
    pub to: Point3,
    pub arc: Option<ArcMove>,
    /// Travel time in seconds (0 for a zero-length move).
    pub secs: f64,
    /// Standstill at the end - a cycle dwell, 0.0 for nearly every seg.
    pub dwell: f64,
    /// The feed the readout shows while moving; None means rapid.
    pub feed: Option<f64>,
    /// Index into [`Script::ops`].
    pub op: usize,
    /// The commanded A/B/C letters (canonical degrees, `cl::pose`'s
    /// convention) at the move's start and end (M5.1 stage G).
    /// `(X, X)` is a no-turn move - and every move ever scripted
    /// before stage G has `X = [0, 0, 0]` at both ends.
    pub abc: ([f64; 3], [f64; 3]),
    /// The linear legs' own travel time and the rotary legs' (stage
    /// G): a coordinated move drives every axis at once and waits for
    /// its slowest, so `secs == pl.max(rl)` (dwell-free) and `at()`
    /// advances each family's fraction by its own clock, arriving and
    /// holding early exactly like the axes do. `rl = 0` and `pl =
    /// secs` on every prismatic move - bit-identical to the law that
    /// walked one shared fraction before stage G.
    pub pl: f64,
    pub rl: f64,
}

/// What the readout says at a point in time.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Readout {
    /// Nothing to show (empty script, or standing past the end).
    Idle,
    /// Moving under G0.
    Rapid,
    /// Moving at this feed, mm/min.
    Feed(f64),
    /// Standing at a cycle bottom for this many seconds.
    Dwell(f64),
}

/// The machine's state at one instant of the script.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Frame {
    pub pos: Point3,
    /// The operation being executed, if any.
    pub op: Option<usize>,
    pub readout: Readout,
    /// The timeline is spent (t at or past total).
    pub done: bool,
    /// The commanded tool axis (unit, tip-to-spindle, `cl::pose`'s
    /// convention) at this instant - the letters mid-travel while a
    /// turning move runs, its endpoints otherwise. Every frame of a
    /// prismatic program is exactly `[0.0, 0.0, 1.0]`.
    pub axis: [f64; 3],
}

/// A whole document's timeline. `cum[i]` is the *end* of `segs[i]`'s
/// window (travel plus dwell) and `total == cum.last()`; segment `i`
/// starts at `cum[i-1]` (0.0 for the first).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Script {
    /// Operation names in document order, the ones the script runs.
    pub ops: Vec<String>,
    pub segs: Vec<Seg>,
    pub cum: Vec<f64>,
    pub total: f64,
    /// Past [`MAX_SIM_SEGS`] the build stops; the UI must say so.
    pub truncated: bool,
}

/// The overlay vertex budget's sibling: a pathological cache (millions
/// of file-loaded moves) must not turn every frame into a
/// million-segment allocation. Past it the script ends early and
/// `truncated` speaks.
pub const MAX_SIM_SEGS: usize = 1 << 20;

impl Script {
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.segs.is_empty()
    }

    /// The machine at time `t` (seconds). Out-of-range t clamps:
    /// before 0 is the untouched start, past the end stands at the
    /// final point with `done`.
    #[must_use]
    pub fn at(&self, t: f64) -> Frame {
        let home = || [0.0, 0.0, 1.0];
        let Some(first) = self.segs.first() else {
            return Frame {
                pos: Point3::new(0.0, 0.0, 0.0),
                op: None,
                readout: Readout::Idle,
                done: true,
                axis: home(),
            };
        };
        if !t.is_finite() {
            return self.at(self.total);
        }
        if t <= 0.0 {
            return Frame {
                pos: first.from,
                op: Some(first.op),
                readout: Readout::Idle,
                done: false,
                axis: seg_axis(first, 0.0),
            };
        }
        if t >= self.total {
            // The program is over; the machine stands where it ended.
            // Checked against the total directly - the window walk
            // below cannot be trusted for one ULP at the tail.
            let last = self.segs.last().unwrap_or(first);
            return Frame {
                pos: last.to,
                op: Some(last.op),
                readout: Readout::Idle,
                done: true,
                axis: seg_axis(last, 1.0),
            };
        }
        // The first segment whose window ends after t; nothing past
        // the end has one, and the clamp makes the last hold.
        let i = self.cum.partition_point(|&e| e <= t).min(self.segs.len() - 1);
        let s = &self.segs[i];
        let start = if i == 0 { 0.0 } else { self.cum[i - 1] };
        let local = t - start;
        if local < s.secs {
            let f = if s.secs > 0.0 {
                (local / s.secs).clamp(0.0, 1.0)
            } else {
                1.0
            };
            Frame {
                pos: at_seg(s, leg_frac(s.pl, f, s.secs)),
                op: Some(s.op),
                readout: match s.feed {
                    Some(v) => Readout::Feed(v),
                    None => Readout::Rapid,
                },
                done: false,
                axis: seg_axis(s, leg_frac(s.rl, f, s.secs)),
            }
        } else if local - s.secs < s.dwell {
            Frame {
                pos: s.to,
                op: Some(s.op),
                readout: Readout::Dwell(s.dwell),
                done: false,
                axis: seg_axis(s, 1.0),
            }
        } else {
            // Past every window (or a float whisper of one at the
            // tail): the machine stands where it ended. `done` reads
            // the total directly, so no rounding of the window walk
            // can claim it early.
            Frame {
                pos: s.to,
                op: Some(s.op),
                readout: Readout::Idle,
                done: t >= self.total,
                axis: seg_axis(s, 1.0),
            }
        }
    }
}

/// One axis family's fraction at shared-time fraction `f` of a move
/// of `secs` whose family leg needs `leg` of them: the family runs
/// its own clock at full stroke and holds at 1.0 once arrived -
/// stage G's coordinated-motion law in one line. `leg == 0` (a
/// family nothing moves in, every family before stage G) arrives at
/// the start and holds; a zero-length move (`secs == 0`) is at its
/// end by definition.
pub(crate) fn leg_frac(leg: f64, f: f64, secs: f64) -> f64 {
    if secs <= 0.0 {
        return 1.0;
    }
    if leg <= 0.0 {
        return 1.0;
    }
    if leg >= secs {
        // The pace-setting family: exact on the shared clock.
        return f;
    }
    (f * secs / leg).min(1.0)
}

/// The tool axis at letters-fraction `g` of a segment's turn, by
/// `cl::pose`'s own composition: the letters interpolate linearly
/// (each rotary runs its own stroke - the straight line between two
/// commanded addresses IS the machine's motion), and the vector is
/// rebuilt FROM the letters, never round-tripped through floats the
/// way the pose law demands. Both endpoints vertical - every
/// prismatic move - gives the home vector bit-for-bit.
fn seg_axis(s: &Seg, g: f64) -> [f64; 3] {
    let [a0, b0, c0] = s.abc.0;
    let [a1, b1, c1] = s.abc.1;
    if (a0, b0, c0) == (a1, b1, c1) {
        // No turn at all: the endpoint letters ARE the held pose.
        return crate::cl::angles_to_axis([a1, b1, c1]).unwrap_or([0.0, 0.0, 1.0]);
    }
    let lerp = [
        a0 + (a1 - a0) * g,
        b0 + (b1 - b0) * g,
        c0 + (c1 - c0) * g,
    ];
    crate::cl::angles_to_axis(lerp).unwrap_or([0.0, 0.0, 1.0])
}

/// The point at fraction `f` along a segment (arc-aware; the angle
/// law is `arc_chords`' exact survivor - measured off `from`, so no
/// rounding accumulates).
fn at_seg(s: &Seg, f: f64) -> Point3 {
    point_along(s.from, s.to, s.arc, f)
}

/// The point at fraction `f` along the move `from..to` (straight, or
/// the arc `arc`): the tool's drawn position, shared by the timeline
/// and the playback's partial cut (#1516), so the material removed
/// mid-move ends exactly where the tool is drawn.
pub(crate) fn point_along(from: Point3, to: Point3, arc: Option<ArcMove>, f: f64) -> Point3 {
    let straight = |f: f64| {
        Point3::new(
            from.x + (to.x - from.x) * f,
            from.y + (to.y - from.y) * f,
            from.z + (to.z - from.z) * f,
        )
    };
    let Some(arc) = arc else {
        return straight(f);
    };
    if !arc.plane.is_xy() {
        // XZ / YZ (M11.2 A): the same angle law in the arc's own
        // plane (`cl::plane`), the normal coordinate lerping.
        return arc.point_at(from, to, f).unwrap_or_else(|| straight(f));
    }
    let o = arc.center;
    let ra = ((from.x - o.x).powi(2) + (from.y - o.y).powi(2)).sqrt();
    let rb = ((to.x - o.x).powi(2) + (to.y - o.y).powi(2)).sqrt();
    let r = ra.max(rb);
    let a0 = (from.y - o.y).atan2(from.x - o.x);
    let a1 = (to.y - o.y).atan2(to.x - o.x);
    let mut d = if arc.ccw { a1 - a0 } else { a0 - a1 };
    d = d.rem_euclid(2.0 * PI);
    if !r.is_finite() || !d.is_finite() || r <= f64::EPSILON || d <= 0.0 {
        // The straight-chord fallback of arc_chords, matched: a
        // degenerate arc still moves somewhere honest.
        return straight(f);
    }
    let th = a0 + if arc.ccw { d * f } else { -d * f };
    // Arcs are planar by construction (push_arc's law); Z lerps.
    Point3::new(
        o.x + r * th.cos(),
        o.y + r * th.sin(),
        from.z + (to.z - from.z) * f,
    )
}

/// The XY sweep of an arc about `o` in direction `ccw`, as
/// `(radius, sweep)` - `arc_chords`' angle law in one place. A
/// degenerate answer (radius 0, non-finite) is `(0.0, 0.0)`.
fn arc_sweep(a: Point3, b: Point3, o: crate::geo2d::Point2, ccw: bool) -> (f64, f64) {
    let ra = ((a.x - o.x).powi(2) + (a.y - o.y).powi(2)).sqrt();
    let rb = ((b.x - o.x).powi(2) + (b.y - o.y).powi(2)).sqrt();
    let r = ra.max(rb);
    let a0 = (a.y - o.y).atan2(a.x - o.x);
    let a1 = (b.y - o.y).atan2(b.x - o.x);
    let mut d = if ccw { a1 - a0 } else { a0 - a1 };
    d = d.rem_euclid(2.0 * PI);
    if r.is_finite() && d.is_finite() && r > f64::EPSILON && d > 0.0 {
        (r, d)
    } else {
        (0.0, 0.0)
    }
}

/// Distance of a move: the exact arc length over Z, or the straight
/// 3-D distance - the same geometric law `chords` flattens for
/// display and verify.
fn path_len(m: &Move) -> f64 {
    let dz = m.to.z - m.from.z;
    let dxy = || {
        ((m.to.x - m.from.x).powi(2) + (m.to.y - m.from.y).powi(2)).sqrt()
    };
    match m.arc {
        None => (dxy().powi(2) + dz * dz).sqrt(),
        // XZ / YZ (M11.2 A): in-plane arc length over the normal rise.
        Some(arc) if !arc.plane.is_xy() => arc
            .length(m.from, m.to)
            .unwrap_or_else(|| (dxy().powi(2) + dz * dz).sqrt()),
        Some(arc) => {
            let (r, d) = arc_sweep(m.from, m.to, arc.center, arc.ccw);
            let xy = if d > 0.0 { r * d } else { dxy() };
            (xy * xy + dz * dz).sqrt()
        }
    }
}

fn delta(a: Point3, b: Point3) -> Point3 {
    Point3::new(b.x - a.x, b.y - a.y, b.z - a.z)
}

/// One move's two leg times (stage G): the linear legs' time and the
/// rotary legs', feeds dividing the path through the feed word, G0
/// dividing through the axis rapids, letters dividing through the
/// machine's deg/min rates, and a move with nothing to say (no
/// usable feed where a feed belongs) falls back to the rapid law on
/// its linear legs - never zero, never a hang. The move takes the
/// longer leg and waits for it: coordinated motion.
fn move_legs(m: &Move, tr: &Traverse, dabc: [f64; 3]) -> (f64, f64) {
    let rl = match m.kind {
        MoveKind::Rapid => tr.rot_secs(dabc, true),
        MoveKind::Feed | MoveKind::Plunge => tr.rot_secs(dabc, false),
    };
    let pl = match m.kind {
        MoveKind::Rapid => tr.rapid_secs(delta(m.from, m.to)),
        MoveKind::Feed | MoveKind::Plunge => match m.feed {
            Some(f) if f.is_finite() && f > 0.0 => 60.0 * path_len(m) / f,
            _ => tr.rapid_secs(delta(m.from, m.to)),
        },
    };
    (pl, rl)
}

/// The timeline for a run of operations: `(name, path)` pairs in
/// document order (the caller has walked the tree; hidden and
/// never-generated ones are already left out).
#[must_use]
pub fn script<'a>(
    ops: impl IntoIterator<Item = (&'a str, &'a Toolpath)>,
    tr: &Traverse,
) -> Script {
    let mut sc = Script::default();
    let mut cum: f64 = 0.0;
    let mut last: Option<Point3> = None;
    // The letters the controller is holding when this move's block is
    // read (M5.1 stage G): canonical form, home vertical, stated by
    // each cut move's axis and unchanged through rapids and op
    // connectors - the same modal law the post writes A/B/C words by
    // and `verify` reads an unposed move as vertical with.
    let mut held = [0.0f64; 3];
    for (name, t) in ops {
        let Some(path_start) = t.start() else {
            continue; // an empty path contributes no time and no name
        };
        if !path_start.is_finite() {
            // Nowhere to stand at the start is not a slow path, it is
            // a corrupt cache; it contributes nothing, and says so.
            eprintln!("sim: {name} starts nowhere finite and is left out");
            continue;
        }
        if sc.segs.len() >= MAX_SIM_SEGS {
            sc.truncated = true;
            break;
        }
        let oi = sc.ops.len();
        sc.ops.push(name.to_owned());
        // The connector the machine makes between programs: a rapid
        // from where the last one ended, attributed to the operation
        // it arrives for (its tool is in the spindle).
        if let Some(prev) = last
            && prev != path_start
        {
            // The connector commands no angle: the head holds its
            // pose through the air (the modal law, once more).
            let secs = tr.rapid_secs(delta(prev, path_start));
            sc.segs.push(Seg {
                from: prev,
                to: path_start,
                arc: None,
                secs,
                dwell: 0.0,
                feed: None,
                op: oi,
                abc: (held, held),
                pl: secs,
                rl: 0.0,
            });
            cum += secs;
            sc.cum.push(cum);
        }
        // One seg per move, index-aligned with the move list: the
        // dwell pass below addresses segments by move index.
        let seg0 = sc.segs.len();
        let mut here = path_start;
        for m in &t.moves {
            if sc.segs.len() >= MAX_SIM_SEGS {
                sc.truncated = true;
                break;
            }
            let (from, to) = if m.is_finite() {
                (m.from, m.to)
            } else {
                // The cache is validated on its way here; a corrupt
                // move stands still and says so, keeping the
                // move-index alignment the dwell pass depends on.
                eprintln!("sim: a non-finite move of {name} stands in as no motion");
                (here, here)
            };
            here = to;
            // The letters this move ends holding (the modal law with
            // verify's reading: a rapid commands no angle and keeps
            // the held pose; a cut that states one turns to it; a cut
            // that states none is vertical, which is what law (c)
            // already cuts it as). A corrupt move's axis is a non-
            // direction - vertical is the honest read of that too.
            let next = if m.is_finite() {
                match m.kind {
                    MoveKind::Rapid => held,
                    MoveKind::Feed | MoveKind::Plunge => m
                        .axis
                        .and_then(crate::cl::normalize_axis)
                        .map_or([0.0; 3], crate::cl::axis_to_angles),
                }
            } else {
                held
            };
            let dabc = [next[0] - held[0], next[1] - held[1], next[2] - held[2]];
            let (pl, rl) = if m.is_finite() {
                move_legs(m, tr, dabc)
            } else {
                (0.0, 0.0)
            };
            let secs = pl.max(rl);
            sc.segs.push(Seg {
                from,
                to,
                arc: if m.is_finite() { m.arc } else { None },
                secs,
                dwell: 0.0,
                feed: if m.kind == MoveKind::Rapid {
                    None
                } else {
                    m.feed
                },
                op: oi,
                abc: (held, next),
                pl,
                rl,
            });
            held = next;
            cum += secs;
            sc.cum.push(cum);
        }
        // Dwells ride the move that reaches the cycle bottom (the
        // agreement law guarantees the chain got there; the fallback
        // seats them at the group's last move, honestly).
        let end = sc.segs.len(); // may sit short of seg0 + moves.len() when truncated
        for (ci, c) in t.cycles.iter().enumerate() {
            if c.dwell <= 0.0 || !c.dwell.is_finite() {
                continue;
            }
            let g0 = c.first_move.min(t.moves.len());
            let g1 = t.cycles.get(ci + 1).map_or(t.moves.len(), |n| {
                n.first_move.max(g0).min(t.moves.len())
            });
            if g0 >= g1 {
                continue;
            }
            let anchor = t
                .moves
                .get(g0..g1)
                .and_then(|s| {
                    s.iter().position(|m| {
                        m.kind != MoveKind::Rapid && (m.to.z - c.bottom_z).abs() <= 1e-6
                    })
                })
                .map_or(g1 - 1, |k| g0 + k);
            let si = seg0 + anchor;
            if si < end {
                sc.segs[si].dwell += c.dwell;
                for slot in &mut sc.cum[si..] {
                    *slot += c.dwell;
                }
                cum += c.dwell;
            }
        }
        last = Some(here); // the last finite point the timeline saw
    }
    sc.total = cum;
    sc
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cl::{Cycle, CycleKind};
    use crate::geo2d::Point2;

    fn p(x: f64, y: f64, z: f64) -> Point3 {
        Point3::new(x, y, z)
    }

    fn near(a: Point3, b: Point3, tol: f64) -> bool {
        (a.x - b.x).abs() < tol && (a.y - b.y).abs() < tol && (a.z - b.z).abs() < tol
    }

    #[test]
    fn rapid_secs_take_the_slowest_axis() {
        let tr = Traverse {
            xy: 6000.0,
            z: 4000.0,
            ..Default::default()
        };
        // 60 mm XY arrives at 0.6 s; the 10 mm Z leg at 0.15 s - the
        // move is over at 0.6.
        assert!((tr.rapid_secs(delta(p(0.0, 0.0, 0.0), p(60.0, 0.0, -10.0))) - 0.6).abs() < 1e-9);
        // A pure-Z move is paced by Z alone.
        assert!((tr.rapid_secs(delta(p(0.0, 0.0, 0.0), p(0.0, 0.0, 40.0))) - 0.6).abs() < 1e-9);
        assert_eq!(tr.rapid_secs(delta(p(5.0, 5.0, 5.0), p(5.0, 5.0, 5.0))), 0.0);
    }

    #[test]
    fn feed_time_is_distance_over_feed() {
        let tr = Traverse::default();
        let m = Move::new(MoveKind::Feed, p(0.0, 0.0, 0.0), p(30.0, 0.0, 0.0), Some(500.0));
        let (pl, rl) = move_legs(&m, &tr, [0.0; 3]);
        assert!((pl.max(rl) - 3.6).abs() < 1e-9); // 30 mm at 500 mm/min
        // A feed move that lost its feed word falls back to G0 pace.
        let lost = Move::new(MoveKind::Feed, p(0.0, 0.0, 0.0), p(60.0, 0.0, 0.0), None);
        let (pl, rl) = move_legs(&lost, &tr, [0.0; 3]);
        assert!((pl.max(rl) - 0.6).abs() < 1e-9);
    }

    #[test]
    fn script_runs_a_path_end_to_end() {
        let tr = Traverse::default();
        let mut t = Toolpath::default();
        t.moves.push(Move::new(MoveKind::Rapid, p(0.0, 0.0, 10.0), p(10.0, 0.0, 10.0), None));
        t.moves.push(Move::new(
            MoveKind::Plunge,
            p(10.0, 0.0, 10.0),
            p(10.0, 0.0, 0.0),
            Some(100.0),
        ));
        t.moves.push(Move::new(
            MoveKind::Feed,
            p(10.0, 0.0, 0.0),
            p(20.0, 0.0, 0.0),
            Some(600.0),
        ));
        let sc = script([("C1", &t)], &tr);
        assert_eq!(sc.ops.len(), 1);
        assert_eq!(sc.segs.len(), 3);
        assert!(near(sc.at(0.0).pos, p(0.0, 0.0, 10.0), 1e-12));
        assert_eq!(sc.at(0.0).op, Some(0));
        assert!(!sc.at(0.0).done);
        // Mid-plunge is halfway down, and says its feed.
        let plunge = sc.segs[1].secs; // 60 s·mm/100 mm = 6.0
        assert!((plunge - 6.0).abs() < 1e-9);
        let f = sc.at(sc.cum[0] + plunge / 2.0);
        assert!((f.pos.z - 5.0).abs() < 1e-9);
        assert!(matches!(f.readout, Readout::Feed(v) if (v - 100.0).abs() < 1e-9));
        // The whole thing, then past the end.
        let e = sc.at(sc.total + 1.0);
        assert!(near(e.pos, p(20.0, 0.0, 0.0), 1e-9));
        assert!(e.done);
        assert_eq!(e.readout, Readout::Idle);
    }

    #[test]
    fn windows_never_overlap_and_boundaries_are_continuous() {
        // The cum[i] contract at(): at a boundary t == cum[i], the
        // next segment's from wins - and chaining makes that equal
        // the previous segment's to.
        let tr = Traverse::default();
        let mut t = Toolpath::default();
        t.moves.push(Move::new(MoveKind::Rapid, p(0.0, 0.0, 5.0), p(30.0, 0.0, 5.0), None));
        t.moves.push(Move::new(
            MoveKind::Feed,
            p(30.0, 0.0, 5.0),
            p(30.0, 40.0, 5.0),
            Some(1000.0),
        ));
        let sc = script([("A", &t)], &tr);
        let b = sc.at(sc.cum[0] - 1e-12);
        let c = sc.at(sc.cum[0]);
        assert!(near(b.pos, c.pos, 1e-6), "the boundary stands in one place");
        assert!(matches!(c.readout, Readout::Feed(_)), "after the boundary it cuts");
    }

    #[test]
    fn connector_rapids_join_operations() {
        let tr = Traverse::default();
        let mut a = Toolpath::default();
        a.moves.push(Move::new(
            MoveKind::Feed,
            p(0.0, 0.0, 0.0),
            p(10.0, 0.0, 0.0),
            Some(1000.0),
        ));
        let mut b = Toolpath::default();
        b.moves.push(Move::new(
            MoveKind::Feed,
            p(50.0, 0.0, 0.0),
            p(60.0, 0.0, 0.0),
            Some(1000.0),
        ));
        let sc = script([("A", &a), ("B", &b)], &tr);
        assert_eq!(sc.segs.len(), 3); // move, connector, move
        assert_eq!(sc.segs[1].op, 1); // connector arrives for B
        assert_eq!(sc.segs[1].feed, None);
        assert!((sc.segs[1].secs - 0.4).abs() < 1e-9); // 40 mm XY at 6000
        assert_eq!(sc.at(0.0).op, Some(0));
        assert_eq!(sc.at(sc.total - 1e-9).op, Some(1));
    }

    #[test]
    fn dwell_sits_after_the_move_that_reaches_bottom() {
        let tr = Traverse::default();
        let mut t = Toolpath::default();
        t.moves.push(Move::new(MoveKind::Rapid, p(0.0, 0.0, 10.0), p(0.0, 0.0, 10.0), None));
        t.moves.push(Move::new(
            MoveKind::Plunge,
            p(0.0, 0.0, 10.0),
            p(0.0, 0.0, -5.0),
            Some(100.0),
        ));
        t.moves.push(Move::new(
            MoveKind::Rapid,
            p(0.0, 0.0, -5.0),
            p(0.0, 0.0, 10.0),
            None,
        ));
        t.cycles.push(Cycle {
            kind: CycleKind::Drill,
            xy: Point2::new(0.0, 0.0),
            r_z: 10.0,
            bottom_z: -5.0,
            entry_z: 0.0,
            peck: 0.0,
            dwell: 1.0,
            first_move: 0,
        });
        let sc = script([("D1", &t)], &tr);
        assert!((sc.segs[1].dwell - 1.0).abs() < 1e-12);
        // The dwell window belongs to the plunge's end: at its middle
        // the tool stands at the bottom and the readout says DWELL.
        let mid = sc.cum[1] - sc.segs[1].dwell / 2.0;
        let f = sc.at(mid);
        assert!(near(f.pos, p(0.0, 0.0, -5.0), 1e-12));
        assert!(matches!(f.readout, Readout::Dwell(v) if (v - 1.0).abs() < 1e-12));
        // And it shifted the retract's whole window later.
        assert!(sc.cum[2] > sc.cum[1]);
    }

    #[test]
    fn arc_positions_are_exact_on_the_circle() {
        let tr = Traverse::default();
        let mut t = Toolpath::default();
        // Quarter ccw arc, r = 10 about the origin, from +X to +Y.
        let m = Move::new(
            MoveKind::Feed,
            p(10.0, 0.0, 0.0),
            p(0.0, 10.0, 0.0),
            Some(100.0 * 10.0 * PI / 2.0), // arc length × 100: secs = 0.6
        )
        .with_arc(Point2::new(0.0, 0.0), true);
        let (pl, rl) = move_legs(&m, &tr, [0.0; 3]);
        assert!((pl.max(rl) - 0.6).abs() < 1e-9);
        t.moves.push(m);
        let sc = script([("A", &t)], &tr);
        let a = sc.at(0.0);
        let m2 = sc.at(0.3);
        let b = sc.at(0.6);
        assert!(near(a.pos, p(10.0, 0.0, 0.0), 1e-9));
        assert!(
            near(m2.pos, p(10.0 / 2f64.sqrt(), 10.0 / 2f64.sqrt(), 0.0), 1e-9),
            "the halfway point is 45 degrees, not a chord midpoint"
        );
        assert!(near(b.pos, p(0.0, 10.0, 0.0), 1e-9));
        // The circle: every sample keeps its radius.
        for k in 0..=10 {
            let r = sc.at(0.6 * k as f64 / 10.0);
            let rr = (r.pos.x * r.pos.x + r.pos.y * r.pos.y).sqrt();
            assert!((rr - 10.0).abs() < 1e-9);
        }
    }

    #[test]
    fn empty_and_pathological_input_stay_calm() {
        let tr = Traverse::default();
        let s = script([], &tr);
        assert_eq!(s.total, 0.0);
        assert_eq!(s.at(1.0).readout, Readout::Idle);
        assert!(s.at(1.0).done);
        let empty = Toolpath::default();
        assert!(script([("E", &empty)], &tr).segs.is_empty());
        // Non-finite moves stand in as no motion: the position never
        // goes NaN, no time passes, nothing panics.
        let mut bad = Toolpath::default();
        bad.moves.push(Move::new(
            MoveKind::Rapid,
            p(1.0, 2.0, 3.0),
            p(f64::NAN, 0.0, 0.0),
            None,
        ));
        bad.moves.push(Move::new(
            MoveKind::Feed,
            p(f64::NAN, 0.0, 0.0),
            p(1.0, 2.0, 3.0),
            Some(100.0),
        ));
        let sc = script([("X", &bad)], &tr);
        assert_eq!(sc.segs.len(), 2); // alignment kept: one seg per move
        assert_eq!(sc.segs[0].secs, 0.0);
        assert_eq!(sc.total, 0.0);
        let f = sc.at(1e9);
        assert!(near(f.pos, p(1.0, 2.0, 3.0), 1e-12));
        assert!(f.done);
        // A path starting nowhere finite is left out entirely, loudly.
        let mut rot = Toolpath::default();
        rot.moves.push(Move::new(
            MoveKind::Feed,
            p(f64::NAN, 0.0, 0.0),
            p(1.0, 1.0, 0.0),
            Some(100.0),
        ));
        let sc = script([("R", &rot)], &tr);
        assert!(sc.is_empty());
        assert_eq!(sc.at(0.0).readout, Readout::Idle);
    }

    #[test]
    fn traverse_reads_the_machine_file() {
        let m: Value =
            serde_json::from_str(include_str!("../../../assets/machines/3axis-vmc.mch")).unwrap();
        let tr = Traverse::from_machine(&m);
        assert_eq!(
            tr,
            Traverse {
                xy: 6000.0,
                z: 4000.0,
                rot: [RotRate::default(); 3],
            }
        );
        assert_eq!(Traverse::from_machine(&Value::Null), Traverse::default());
    }

    // ---- M5.1 stage G: the kinematic timeline ----------------------------

    #[test]
    fn a_tilthead_file_turns_the_rotary_slots() {
        // The parts-tree machine: A and B state 5000 deg/min rapid,
        // 2000 normal, through the mirror's rotary speeds block -
        // parsed here in the same static grammar the mirror emits.
        let m = serde_json::json!({
            "kinematics": { "static": { "axes": [], "rotaries": [
                {"letter": "A", "type": "rotary", "direction": [1.0, 0.0, 0.0],
                 "speeds": {"normal": 2000.0, "rapid": 5000.0}},
                {"letter": "B", "type": "rotary", "direction": [0.0, 1.0, 0.0],
                 "speeds": {"normal": 2000.0, "rapid": 5000.0}},
                {"letter": "U", "type": "rotary"},
            ] } }
        });
        let tr = Traverse::from_machine(&m);
        assert_eq!(tr.rot[0].rapid, Some(5000.0));
        assert_eq!(tr.rot[0].normal, Some(2000.0));
        assert_eq!(tr.rot[1].rapid, Some(5000.0), "B turns too");
        assert_eq!(tr.rot[2], RotRate::default(), "no C on this head");
    }

    #[test]
    fn a_rapid_never_turns_the_head() {
        // The modal law the post writes by and stamp.rs keeps: an A/B/C
        // word rides the CUT block that changes the pose, never a G0 -
        // even a rapid whose cache entry carries an axis (which the
        // generators never write, but the timeline must not invent a
        // turn from) turns nothing and takes only its linear time.
        let tr = Traverse {
            rot: [
                RotRate::default(),
                RotRate {
                    normal: Some(2000.0),
                    rapid: Some(5000.0),
                },
                RotRate::default(),
            ],
            ..Default::default()
        };
        let mut t = Toolpath::default();
        t.moves.push(
            Move::new(MoveKind::Rapid, p(0.0, 0.0, 10.0), p(30.0, 0.0, 10.0), None)
                .with_axis(crate::cl::angles_to_axis([0.0, 90.0, 0.0]).unwrap()),
        );
        let sc = script([("A", &t)], &tr);
        assert!((sc.segs[0].secs - 0.3).abs() < 1e-9, "a rapid turns nothing");
        assert_eq!(sc.segs[0].abc, ([0.0; 3], [0.0; 3]));
        assert!(sc.segs[0].rl == 0.0);
        assert_eq!(sc.at(0.15).axis, [0.0, 0.0, 1.0], "the pose never left home");
    }

    #[test]
    fn the_head_turns_on_the_move_that_commands_it() {
        // The post's own motion model (fanuc_post's RTCP test): the
        // letters ride the block of the move that changes them, so a
        // coordinated G1 that ends posed 90 deg of B spends its turn
        // leg at the NORMAL rate - 90 deg at 2000 deg/min is 2.7 s,
        // pacing the whole move if the linear legs are quicker.
        let tr = Traverse {
            rot: [
                RotRate::default(),
                RotRate {
                    normal: Some(2000.0),
                    rapid: Some(5000.0),
                },
                RotRate::default(),
            ],
            ..Default::default()
        };
        let mut t = Toolpath::default();
        t.moves.push(
            Move::new(
                MoveKind::Feed,
                p(0.0, 0.0, 0.0),
                p(30.0, 0.0, 0.0),
                Some(6000.0),
            )
            .with_axis(crate::cl::angles_to_axis([0.0, 90.0, 0.0]).unwrap()),
        );
        let sc = script([("A", &t)], &tr);
        assert!((sc.segs[0].secs - 2.7).abs() < 1e-9, "the head paced the line");
        // Each family runs its own clock: the XY legs need 0.3 s of
        // the 2.7 s the head takes, so halfway through their leg the
        // table is at x = 15, and from 0.3 s on the position HOLDS at
        // the endpoint while B keeps turning - arrival, then a wait
        // for the slowest axis, exactly as the real axes do.
        let q = sc.at(0.15);
        assert!((q.pos.x - 15.0).abs() < 1e-9, "the linear legs pace themselves");
        assert!(
            (q.axis[0] - (90.0f64 * (0.15 / 2.7)).to_radians().sin()).abs() < 1e-12,
            "the head barely started"
        );
        let f = sc.at(0.5);
        assert!((f.pos.x - 30.0).abs() < 1e-9, "the fast legs hold");
        let g = sc.at(2.7 * 0.5);
        assert!((g.axis[0] - (45.0f64).to_radians().sin()).abs() < 1e-9, "B at 45 mid-turn");
        assert!((g.axis[2] - (45.0f64).to_radians().cos()).abs() < 1e-9);
        let e = sc.at(2.7);
        assert!(
            e.axis[0] > 1.0 - 1e-12 && e.axis[1].abs() < 1e-12 && e.axis[2].abs() < 1e-12,
            "the endpoint pose is the commanded one: {:?}",
            e.axis
        );
    }

    #[test]
    fn prismatic_programs_never_learn_the_head_moved() {
        // Stage G's purity law, the same shape every earlier stage
        // kept: no letters, no turn legs, no frame tilt - the axis
        // reads EXACTLY home through a whole 2.5-D program, and the
        // timelines are what they always were.
        let tr = Traverse {
            rot: [
                RotRate {
                    normal: Some(2000.0),
                    rapid: Some(5000.0),
                },
                RotRate::default(),
                RotRate::default(),
            ],
            ..Default::default()
        };
        let mut t = Toolpath::default();
        t.moves.push(Move::new(MoveKind::Rapid, p(0.0, 0.0, 10.0), p(0.0, 0.0, 2.0), None));
        t.moves.push(Move::new(
            MoveKind::Feed,
            p(0.0, 0.0, 2.0),
            p(20.0, 0.0, 2.0),
            Some(1000.0),
        ));
        t.moves.push(Move::new(
            MoveKind::Feed,
            p(20.0, 0.0, 2.0),
            p(20.0, 10.0, 2.0),
            Some(1000.0),
        ));
        let sc = script([("P", &t)], &tr);
        for k in 0..=20 {
            let f = sc.at(sc.total * k as f64 / 20.0);
            assert_eq!(f.axis, [0.0, 0.0, 1.0], "a prismatic frame tilted at t={k}");
        }
        // The totals are the M1.6 arithmetic, untouched by the
        // rotary rates the same machine states: 8 mm Z rapid at 4000,
        // 20 mm and 10 mm feeds at 1000.
        let want = 60.0 * 8.0 / 4000.0 + 60.0 * 20.0 / 1000.0 + 60.0 * 10.0 / 1000.0;
        assert!((sc.total - want).abs() < 1e-9);
    }

    #[test]
    fn an_unposed_cut_sends_the_letters_home() {
        // The modal law the post writes and verify reads: a cut that
        // states no axis IS vertical to the machine model - so the
        // move after a tilted row turns back through the head's leg,
        // and the turn is paced (and a pose change between the two
        // is what the sweep law of stage G's verify half exists for).
        let tr = Traverse {
            rot: [
                RotRate::default(),
                RotRate {
                    normal: Some(3000.0),
                    rapid: None,
                },
                RotRate::default(),
            ],
            ..Default::default()
        };
        let mut t = Toolpath::default();
        t.moves.push(
            Move::new(
                MoveKind::Feed,
                p(0.0, 0.0, 0.0),
                p(10.0, 0.0, 0.0),
                Some(1000.0),
            )
            .with_axis(crate::cl::angles_to_axis([0.0, 60.0, 0.0]).unwrap()),
        );
        t.moves.push(Move::new(
            MoveKind::Rapid,
            p(10.0, 0.0, 0.0),
            p(10.0, 5.0, 0.0),
            None,
        ));
        t.moves.push(Move::new(
            MoveKind::Feed,
            p(10.0, 5.0, 0.0),
            p(20.0, 5.0, 0.0),
            Some(1000.0),
        ));
        let sc = script([("P", &t)], &tr);
        // The letters ride an axis round-trip (60 deg -> vector ->
        // letters), so they speak with float accents: tolerance in,
        // exactness out of the comparison.
        let close = |a: [f64; 3], b: [f64; 3]| {
            a.iter().zip(b).all(|(x, y)| (x - y).abs() < 1e-9)
        };
        assert_eq!(sc.segs[0].abc.0, [0.0; 3], "home at the start of program");
        assert!(close(sc.segs[0].abc.1, [0.0, 60.0, 0.0]));
        // The rapid carries the tilt (post modal: no new word).
        assert!(close(sc.segs[1].abc.0, [0.0, 60.0, 0.0]) && sc.segs[1].abc.0 == sc.segs[1].abc.1);
        // The unposed cut returns home: 60 deg back at 3000 is 1.2 s,
        // inside the 0.6 s of 10 mm at 1000 - the turn paces it.
        assert!(close(sc.segs[2].abc.0, [0.0, 60.0, 0.0]));
        assert_eq!(sc.segs[2].abc.1, [0.0; 3], "home is home, exactly");
        assert!((sc.segs[2].secs - 1.2).abs() < 1e-9, "the way home was the slow leg");
    }

    #[test]
    fn leg_windows_meet_at_the_boundary_letters() {
        // Continuity, frame by frame: the pose at the last breath of
        // one seg is the pose at the first of the next, across turns,
        // holds, and resets - the chain a real controller walks.
        let tr = Traverse {
            rot: [
                RotRate::default(),
                RotRate {
                    normal: Some(2000.0),
                    rapid: Some(2000.0),
                },
                RotRate::default(),
            ],
            ..Default::default()
        };
        let mut t = Toolpath::default();
        t.moves.push(
            Move::new(MoveKind::Feed, p(0.0, 0.0, 0.0), p(5.0, 0.0, 0.0), Some(500.0))
                .with_axis(crate::cl::angles_to_axis([30.0, 30.0, 0.0]).unwrap()),
        );
        t.moves.push(Move::new(
            MoveKind::Rapid,
            p(5.0, 0.0, 0.0),
            p(5.0, 0.0, 8.0),
            None,
        ));
        t.moves.push(Move::new(
            MoveKind::Feed,
            p(5.0, 0.0, 8.0),
            p(9.0, 0.0, 8.0),
            Some(500.0),
        ));
        let sc = script([("C", &t)], &tr);
        for i in 0..sc.segs.len() - 1 {
            let a = sc.at(sc.cum[i] - 1e-9);
            let b = sc.at(sc.cum[i]);
            let d = (a.axis[0] * b.axis[0]
                + a.axis[1] * b.axis[1]
                + a.axis[2] * b.axis[2])
                .clamp(-1.0, 1.0)
                .acos()
                .to_degrees();
            assert!(d < 0.01, "the head jumped {d} deg across the seg boundary {i}");
        }
        // And every frame states a UNIT vector - the contract pose.rs
        // demands of any axis the stream carries.
        for k in 0..=40 {
            let f = sc.at(sc.total * k as f64 / 40.0);
            let len = f.axis[0] * f.axis[0] + f.axis[1] * f.axis[1] + f.axis[2] * f.axis[2];
            assert!((len - 1.0).abs() < 1e-12, "a non-unit axis at t={k}");
        }
    }
}
