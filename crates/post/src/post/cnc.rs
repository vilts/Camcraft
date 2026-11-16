//! The intermediate stream the post consumes - our `.cnc` role.
//!
//! Provenance: our own line-oriented format, specced in
//! `docs/CNC-INTERMEDIATE.md`; only its *role* follows the published
//! Fusion 360 post architecture (behavior, not text, per the
//! Clean-Room Protocol). It is a superset of the golden `.cl` dumps:
//! a file with no `HSMW-CNC` magic parses by the `.cl` rules, so every
//! committed golden is a legal post input. Geometry stands on the µm
//! lattice the `.cl` dumps use; parameter reals are 6-decimal fixed.
//! Deterministic text in, deterministic record stream out - no
//! timestamps, no ordering luck.

use crate::cl::{ArcPlane, CompSide, CycleKind, MoveKind};
use serde_json::{json, Value};
use std::collections::BTreeMap;

/// One named `PARAM`, a `WARN` line, the tool block, a section
/// boundary, a motion record, or a cycle.
#[derive(Clone, Debug, PartialEq)]
pub enum Record {
    /// A configuration parameter, in file order, before any section.
    Param { name: String, value: Value },
    /// A verification warning attached to the path - the post gate's
    /// evidence, carried in the stream so CLI and in-app paths share
    /// one source of truth.
    Warn { text: String },
    /// The tool block (`TOOL n name`).
    Tool { number: u32, name: String },
    /// A `SECTION`/`ENDSECTION` (legacy `START` ends no section; the
    /// parser opens one on the first record after `START` implicitly).
    Section { begin: bool },
    Move(MoveRec),
    Cycle(CycleRec),
    /// A bar-feeder event inside a parting op's section (M5.2 G.2):
    /// `loading` true is PART LOAD - the feeder stands the next bar
    /// out `advance` mm (0 signals the act alone) - and false is
    /// PART UNLOAD, the catcher taking the severed part. The stream
    /// carries intent, not motion: no geometry rides here, and a
    /// post whose machine has no feeder prints its comment and
    /// changes nothing else.
    Part { loading: bool, advance: f64 },
    /// A part transfer (M5.3 C): the choreography step between
    /// handlers - `from` spindle hands the part to `to` spindle
    /// (1-based ordinals, the order the machine file states its
    /// spindles). The general form M5.2 G.2's feeder signals were
    /// heading to: still intent, not motion, still no geometry
    /// rides here, and still a comment-only default - the script's
    /// dialect turns it into whatever the machine's handoff protocol
    /// is. Whether the machine HAS the second spindle is the
    /// stated-what's-stated law: the program gate names it, the
    /// engine never guesses.
    Transfer { from: u32, to: u32 },
    /// A measurement ask (M5.3 D): probe the part against
    /// `tol_mm` - the deviation budget the op's author wrote, in
    /// millimeters (µm-integer on the lattice like every other
    /// length here). The fourth name in the lifecycle family: no
    /// motion, no geometry, and no PROTOCOL - RS238 or a Renishaw
    /// macro cycle or a laser is the script's dialect, exactly as
    /// the handoff's handshake is the transfer script's (the
    /// probing choice stays deferred per the ROADMAP's note). The
    /// engine voices the ask; the deviation verdict is the POST's
    /// composed blade scan - every cut on the part measured together
    /// under this budget - and it is what decides whether the part
    /// gets a re-run. The stream only says what to measure.
    Probe { tol_mm: f64 },
    /// Cutter radius compensation (M13.11): `Some(side)` engages it on
    /// the NEXT motion record - a straight one, carrying G41 (left) or
    /// G42 (right) with the tool's offset register, the control holding
    /// `radius_mm` there - and `None` cancels it on the next motion
    /// record (G40). The motion records between are the PART contour,
    /// not the cutter centre: a script that cannot voice this record
    /// must refuse by name, and the engine refuses for a script that
    /// does not define the hook at all - a centre-less contour posted
    /// without its G41 is an undersize part, the silent wrong answer.
    /// `wear` (#38): the records between are the cutter-CENTRE path at
    /// `radius_mm`, and the register holds only the wear delta.
    Comp { side: Option<CompSide>, radius_mm: f64, wear: bool },
    /// A threading cycle (issue #34): `Some` opens the span of explicit
    /// passes the cycle stands for, `None` (`THREAD END`) closes it.
    /// The passes between are ordinary motion records - a script with
    /// a threading cycle writes the record and skips them, any other
    /// posts them as they are.
    Thread(Option<super::cnc_thread::ThreadRec>),
    /// A probing move (#108): probe toward the target, stop on contact,
    /// a skip move (G31) on a Fanuc-style control. A script that
    /// cannot voice it is refused by name: a touch posted as a feed
    /// drives the stylus through the part.
    Touch(super::probe_rec::TouchRec),
    /// The probing result (#108): set a work offset from the measured
    /// trips or stop for the operator, and stop when out of tolerance.
    Found(super::probe_rec::FoundRec),
    /// The incremental back-off after a trip (#108).
    Retreat(super::probe_rec::RetreatRec),
    /// A protected positioning move of a probe path (#174): feed to the
    /// point, stop on contact and ALARM if the stylus trips on the way.
    /// A probing script that cannot voice it is refused by name - a
    /// plain feed would drive the stylus into an unexpected obstacle.
    Guard(super::probe_rec::GuardRec),
    /// The plasma header of a section (#671): the kerf and what the
    /// program states for the torch (`cnc_torch`). It precedes the
    /// section's `SECTION`, and its properties hold until `ENDSECTION`.
    Plasma(super::cnc_torch::PlasmaRec),
    /// A torch event (#671): struck before the next motion record, or
    /// put out after the previous one. A script without the torch
    /// hooks is refused by name - a cut posted without its torch is
    /// a dry run at best, and a mill post's spindle words at worst.
    Torch(super::cnc_torch::TorchRec),
}

/// One feed/rapid/plunge/arc record. Coordinates are millimeters
/// (parsed from µm integers, exact by the lattice law); `feed` is
/// mm/min (`None` for rapids). The record index in the stream is the
/// move index into the source path - legacy `.cl` files carry no
/// `from`, so chaining happens in the driver, not here.
#[derive(Clone, Debug, PartialEq)]
pub struct MoveRec {
    pub kind: MoveKind,
    pub x: f64,
    pub y: f64,
    pub z: f64,
    pub feed: Option<f64>,
    /// Arc: the absolute center, as the two in-plane coordinates of
    /// `plane` in word order - `(X, Y)` (I J), `(X, Z)` (I K) or
    /// `(Y, Z)` (J K).
    pub center: Option<(f64, f64)>,
    pub ccw: bool,
    /// The arc's plane (M11.2 A); `Xy` for every straight record and
    /// every arc written before M11.2. Meaningless when `center` is
    /// None.
    pub plane: ArcPlane,
    /// Commanded absolute A/B/C angles, degrees (M3.3 stage D, RTCP).
    /// `None` - every record our generators write - means the tool
    /// stands as it always has: vertical, table at home. `Some` means
    /// the X/Y/Z above locate the TOOL TIP in the work offset while
    /// the machine turns to these angles; a post whose dialect has no
    /// RTCP output must refuse such a record by name, never post its
    /// XYZ as if the tool were vertical (the Prime Directive's exact
    /// shape: silently reinterpreted coordinates are a gouge).
    pub abc: Option<[f64; 3]>,
}

/// One canned-cycle record: the M1.6 `Cycle` in stream clothing. Zs and
/// peck are millimeters; `dwell` seconds (the stream carries ms);
/// `first_move` the cycle's move index - the agreement-law anchor.
#[derive(Clone, Debug, PartialEq)]
pub struct CycleRec {
    pub kind: CycleKind,
    pub x: f64,
    pub y: f64,
    pub r_z: f64,
    pub bottom_z: f64,
    pub entry_z: f64,
    pub peck: f64,
    pub dwell: f64,
    pub first_move: usize,
    /// The cycle's cutting feed, mm/min. `dump` synthesizes it from
    /// the group's first cutting move; legacy `; CYCLE` markers
    /// carry none, so it is `None` there (the post falls back to the
    /// modal feed).
    pub feed: Option<f64>,
}

impl MoveRec {
    /// The property object the script sees for this record (names per
    /// `docs/CNC-INTERMEDIATE.md`; the driver adds the position keys).
    pub fn props(&self) -> BTreeMap<String, Value> {
        let mut m = BTreeMap::new();
        let kind = match self.kind {
            MoveKind::Rapid => "Rapid",
            MoveKind::Feed => "Linear",
            MoveKind::Plunge => "Linear",
        };
        m.insert("command".into(), json!(kind));
        if let Some((cx, cy)) = self.center {
            m.insert("command".into(), json!("Circular"));
            m.insert(
                "CircularDirection".into(),
                json!(if self.ccw { "CCW" } else { "CW" }),
            );
            // The center words of the arc's plane (M11.2 A): I J for
            // XY - the pre-M11.2 object exactly - I K for XZ, J K
            // for YZ; `CircularPlane` names it for the script.
            let (a, b) = self.plane.center_letters();
            let mut ijk = serde_json::Map::new();
            ijk.insert(a.to_string(), json!(cx));
            ijk.insert(b.to_string(), json!(cy));
            m.insert("IJKIncrement".into(), Value::Object(ijk));
            m.insert("CircularPlane".into(), json!(self.plane.name()));
        }
        m.insert("X".into(), json!(self.x));
        m.insert("Y".into(), json!(self.y));
        m.insert("Z".into(), json!(self.z));
        m.insert("FeedRate".into(), json!(self.feed));
        if let Some([a, b, c]) = self.abc {
            m.insert("A".into(), json!(a));
            m.insert("B".into(), json!(b));
            m.insert("C".into(), json!(c));
        }
        if self.kind == MoveKind::Plunge {
            m.insert("LinearMovementIncludeOpts".into(), json!({ "L": [1] }));
        }
        m
    }
}

impl CycleRec {
    pub fn props(&self) -> BTreeMap<String, Value> {
        let mut m = BTreeMap::new();
        m.insert("command".into(), json!("Drilling"));
        m.insert("CycleType".into(), json!(if self.kind == CycleKind::FloatingTap { "TAP" } else { cycle_name(self.kind) }));
        m.insert("TappingMode".into(), json!(if self.kind == CycleKind::FloatingTap { "floating" } else { "rigid" }));
        m.insert("X".into(), json!(self.x));
        m.insert("Y".into(), json!(self.y));
        m.insert("Z".into(), json!(self.bottom_z));
        m.insert("R".into(), json!(self.r_z));
        m.insert("EntryZ".into(), json!(self.entry_z));
        m.insert("Q".into(), json!(self.peck));
        m.insert("P".into(), json!(self.dwell));
        m.insert("FeedRate".into(), json!(self.feed));
        m
    }
}

/// The script-facing cycle name, matching the v1 `CYCLE` token spelling.
pub fn cycle_name(kind: CycleKind) -> &'static str {
    match kind {
        CycleKind::Spot => "SPOT",
        CycleKind::Drill => "DRILL",
        CycleKind::Peck => "PECK",
        CycleKind::Deep => "DEEP",
        CycleKind::Tap => "TAP",
        CycleKind::FloatingTap => "FLOATINGTAP",
        CycleKind::Bore => "BORE",
        CycleKind::BackBore => "BACKBORE",
        CycleKind::Ream => "REAM",
    }
}

impl Record {
    /// The debugging name of one record (M3.3 stage D, post
    /// debugging): what a block of emitted lines stands for, in the
    /// trace pane and the `--trace` log. Positions are the stream's
    /// own millimeters; angles, when the record commands them, ride
    /// along - a trace that hides the tilt is not a trace.
    pub fn trace_label(&self) -> String {
        let pos = |x: f64, y: f64, z: f64| format!("({x:.3}, {y:.3}, {z:.3})");
        match self {
            Record::Param { name, .. } => format!("param {name}"),
            Record::Warn { text } => format!("warning: {text}"),
            Record::Tool { number, name } => format!("tool {number} · {name}"),
            Record::Section { begin: true } => "section".into(),
            Record::Section { begin: false } => "end section".into(),
            Record::Move(m) => {
                let kind = match m.kind {
                    MoveKind::Rapid => "rapid",
                    MoveKind::Feed => "feed",
                    MoveKind::Plunge => "plunge",
                };
                let mut s = format!("{kind} → {}", pos(m.x, m.y, m.z));
                if m.center.is_some() {
                    // A vertical-plane arc names its plane (M11.2 A);
                    // an XY arc's label is what it always was.
                    let plane = if m.plane.is_xy() {
                        String::new()
                    } else {
                        format!(" {}", m.plane.name())
                    };
                    s = format!(
                        "arc {}{plane} → {}",
                        if m.ccw { "ccw" } else { "cw" },
                        pos(m.x, m.y, m.z)
                    );
                }
                if let Some(f) = m.feed {
                    s.push_str(&format!(" @ {f:.0} mm/min"));
                }
                if let Some([a, b, c]) = m.abc {
                    s.push_str(&format!(" A {a:.3}° B {b:.3}° C {c:.3}°"));
                }
                s
            }
            Record::Cycle(c) => format!(
                "cycle {} → {}",
                cycle_name(c.kind),
                pos(c.x, c.y, c.bottom_z)
            ),
            Record::Part { loading: true, advance } if *advance > 0.0 => {
                format!("part load · advance {advance:.3} mm")
            }
            Record::Part { loading: true, .. } => "part load".into(),
            Record::Part { loading: false, .. } => "part unload".into(),
            Record::Transfer { from, to } => format!("part transfer {from}→{to}"),
            Record::Probe { tol_mm } => format!("probe · tol {tol_mm:.3} mm"),
            Record::Comp { side: Some(side), radius_mm, wear } => format!(
                "comp {} · r {radius_mm:.3} mm{}",
                if *side == CompSide::Left { "left" } else { "right" },
                if *wear { " · wear" } else { "" }
            ),
            Record::Comp { side: None, .. } => "comp off".into(),
            Record::Thread(Some(t)) => t.label(),
            Record::Thread(None) => "thread cycle end".into(),
            Record::Touch(t) => format!(
                "probe → {}{}",
                pos(t.target[0], t.target[1], t.target[2]),
                t.measure.map_or_else(String::new, |k| format!(" · measure {k}"))
            ),
            Record::Retreat(r) => format!("probe retreat Δ {}", pos(r.delta[0], r.delta[1], r.delta[2])),
            Record::Guard(g) => format!("probe protected move → {}", pos(g.to[0], g.to[1], g.to[2])),
            Record::Plasma(p) => p.label(),
            Record::Torch(t) => t.label(),
            Record::Found(f) => match f.set_wcs {
                Some(w) => format!("probe result · set G{} · tol {:.3} mm", 54 + u32::from(w), f.tol_mm),
                None => format!("probe result · confirm · tol {:.3} mm", f.tol_mm),
            },
        }
    }

    /// The `onCommand` name for this record, "" for structural ones.
    pub fn command_name(&self) -> &'static str {
        match self {
            Record::Move(m) => match m.props().get("command") {
                Some(v) => match v.as_str() {
                    Some("Rapid") => "Rapid",
                    Some("Circular") => "Circular",
                    _ => "Linear",
                },
                None => "",
            },
            Record::Cycle(_) => "Drilling",
            _ => "",
        }
    }
}

/// µm-integer formatting, the `.cl` golden law (`scale=0.001mm`):
/// value times a thousand, nearest, written as an integer.
fn q(v: f64) -> String {
    if !v.is_finite() {
        return "nan".into();
    }
    ((v * 1000.0).round() as i64).to_string()
}

/// The writer side: the stream as text, `records` in driver order. The
/// canonical v1 spelling keys every coordinate (`X 1000`, spaced, as
/// `docs/CNC-INTERMEDIATE.md` specs it); the parser also accepts the
/// joined (`X1000`) and legacy positional forms.
pub fn serialize(records: &[Record]) -> String {
    let mut s = String::from("; HSMW-CNC v1\n");
    for r in records {
        match r {
            Record::Param { name, value } => {
                let v = match value {
                    Value::String(t) => t.clone(),
                    Value::Number(n) if n.is_f64() => format!("{:.6}", n.as_f64().unwrap_or(0.0)),
                    other => other.to_string(),
                };
                s.push_str(&format!("PARAM {name} {v}\n"));
            }
            Record::Warn { text } => s.push_str(&format!("WARN {text}\n")),
            Record::Tool { number, name } => s.push_str(&format!("TOOL {number} {name}\n")),
            Record::Section { begin } => {
                s.push_str(if *begin { "SECTION\n" } else { "ENDSECTION\n" });
            }
            Record::Move(m) => {
                let f = m
                    .feed
                    .map(|f| format!(" F {}", (f * 1000.0).round() as i64))
                    .unwrap_or_default();
                // Commanded angles (stage D): milli-degrees on the
                // same lattice law as the microns above - the angle
                // a record states is the angle it parses back to.
                let abc = m
                    .abc
                    .map(|[a, b, c]| format!(" A {} B {} C {}", q(a), q(b), q(c)))
                    .unwrap_or_default();
                match m.center {
                    // The center words name the plane (M11.2 A): I J
                    // is XY - byte-identical to every pre-M11.2
                    // stream - I K is XZ, J K is YZ.
                    Some((cx, cy)) => {
                        let (a, b) = m.plane.center_letters();
                        s.push_str(&format!(
                            "Arc {} X {} Y {} Z {}{f}{abc} {a} {} {b} {}\n",
                            if m.ccw { "CCW" } else { "CW" },
                            q(m.x),
                            q(m.y),
                            q(m.z),
                            q(cx),
                            q(cy),
                        ));
                    }
                    None => s.push_str(&format!(
                        "{} X {} Y {} Z {}{f}{abc}\n",
                        match m.kind {
                            MoveKind::Rapid => "Rapid",
                            MoveKind::Feed => "Feed",
                            MoveKind::Plunge => "Plunge",
                        },
                        q(m.x),
                        q(m.y),
                        q(m.z),
                    )),
                }
            }
            Record::Cycle(c) => {
                let f = c
                    .feed
                    .map(|f| format!(" F {}", (f * 1000.0).round() as i64))
                    .unwrap_or_default();
                s.push_str(&format!(
                    "CYCLE {} X {} Y {} Z {} R {} ENTRY {} Q {} P {}{} AT {}\n",
                    cycle_name(c.kind),
                    q(c.x),
                    q(c.y),
                    q(c.bottom_z),
                    q(c.r_z),
                    q(c.entry_z),
                    q(c.peck),
                    (c.dwell * 1000.0).round() as i64,
                    f,
                    c.first_move,
                ))
            }
            Record::Part { loading, advance } => {
                let adv = if *loading && *advance > 0.0 {
                    format!(" ADVANCE {}", q(*advance))
                } else {
                    String::new()
                };
                s.push_str(&format!(
                    "PART {}{adv}\n",
                    if *loading { "LOAD" } else { "UNLOAD" }
                ));
            }
            Record::Transfer { from, to } => {
                s.push_str(&format!("PART TRANSFER {from} {to}\n"));
            }
            Record::Probe { tol_mm } => {
                // A length like the loader's ADVANCE: the µm integer,
                // the lattice law - not a parameter real.
                s.push_str(&format!("PROBE {}\n", q(*tol_mm)));
            }
            Record::Comp { side, radius_mm, wear } => {
                // The radius rides as microns like every length here;
                // a trailing WEAR (#38) marks the centre-path variant.
                let w = if *wear { " WEAR" } else { "" };
                s.push_str(&match side {
                    Some(CompSide::Left) => format!("COMP LEFT {}{w}\n", q(*radius_mm)),
                    Some(CompSide::Right) => format!("COMP RIGHT {}{w}\n", q(*radius_mm)),
                    None => "COMP OFF\n".to_string(),
                });
            }
            Record::Thread(Some(t)) => s.push_str(&t.line()),
            Record::Thread(None) => s.push_str("THREAD END\n"),
            Record::Touch(t) => s.push_str(&super::probe_rec::write_touch(t)),
            Record::Found(f) => s.push_str(&super::probe_rec::write_found(f)),
            Record::Retreat(r) => s.push_str(&super::probe_rec::write_retreat(r)),
            Record::Guard(g) => s.push_str(&super::probe_rec::write_guard(g)),
            Record::Plasma(p) => s.push_str(&p.line()),
            Record::Torch(t) => s.push_str(&t.line()),
        }
    }
    s
}

/// Parse errors: the Prime Directive's shape - a malformed stream is a
/// clean refusal, never a partial record set.
#[derive(Debug, thiserror::Error)]
pub enum CncError {
    #[error("line {line}: {why}")]
    Syntax { line: usize, why: String },
}

/// Split one word into (key, value-text): `X: 5`, `X=5`, or the joined
/// `X5` / `RZ-1000` spelling where the key is the leading run of ASCII
/// letters and the value starts at the first digit or sign.
fn cnc_word(word: &str) -> Option<(&str, &str)> {
    if let Some((k, v)) = word.split_once([':', '=']) {
        return Some((k, v));
    }
    let split = word
        .find(|c: char| c == '-' || c.is_ascii_digit())
        .unwrap_or(word.len());
    if split > 0
        && split < word.len()
        && word[..split].bytes().all(|b| b.is_ascii_alphabetic())
    {
        Some((&word[..split], &word[split..]))
    } else {
        None
    }
}

fn num(tok: &str) -> Result<f64, ()> {
    // Integer microns by our own law; a decimal parse accepts other
    // spellings. Non-finite text fails the gate below rather than
    // poisoning a record.
    let v = tok.parse::<f64>().map_err(|_| ())?;
    if v.is_finite() {
        Ok(v)
    } else {
        Err(())
    }
}

/// One motion word, in any of the three legal spellings: a bare number
/// (legacy positional), a pure key whose value is the next token, or a
/// key with the value attached or `:`/`=`-separated.
enum MotionWord {
    Value(f64),
    Keyed(&'static str, f64),
}

fn motion_words<'a>(
    it: &mut std::iter::Peekable<std::str::SplitWhitespace<'a>>,
    line_no: usize,
) -> Result<Vec<MotionWord>, CncError> {
    let fail = |why: String| CncError::Syntax { line: line_no, why };
    let mut out = Vec::new();
    while let Some(w) = it.next() {
        if let Ok(v) = num(w) {
            out.push(MotionWord::Value(v));
            continue;
        }
        if !w.is_empty() && w.bytes().all(|b| b.is_ascii_alphabetic()) {
            // Pure key: the value is the next token (the docs' spaced
            // canonical form `X 1000` arrives this way).
            let Some(key) = normalize_key(w) else {
                return Err(fail(format!("unknown word {w:?}")));
            };
            let Some(vtok) = it.next() else {
                return Err(fail(format!("word {w:?} has no value")));
            };
            let Ok(v) = num(vtok) else {
                return Err(fail(format!("bad value {vtok:?} for {w:?}")));
            };
            out.push(MotionWord::Keyed(key, v));
            continue;
        }
        let Some((k, v)) = cnc_word(w) else {
            return Err(fail(format!("move word {w:?}")));
        };
        let Ok(n) = num(v) else {
            return Err(fail(format!("move value {w:?}")));
        };
        let Some(key) = normalize_key(k) else {
            return Err(fail(format!("unknown word {k:?}")));
        };
        out.push(MotionWord::Keyed(key, n));
    }
    Ok(out)
}

fn normalize_key(k: &str) -> Option<&'static str> {
    Some(match k.to_ascii_uppercase().as_str() {
        "X" => "X",
        "Y" => "Y",
        "Z" => "Z",
        "F" => "F",
        "I" => "I",
        "J" => "J",
        "K" => "K",
        "S" => "S",
        // Stage D: commanded rotary angles, absolute degrees written
        // in milli-degrees. Keyed words only - a positional value
        // never means an angle (the legacy law is unchanged).
        "A" => "A",
        "B" => "B",
        "C" => "C",
        _ => return None,
    })
}

/// One cycle's word stream: `KIND [X.. Y.. Z..|BZ.. R..|RZ.. ENTRY..
/// Q..|PECK.. P..|DWELL.. AT..]`, each word spaced (`X 1000`), joined
/// (`X1000`), or `:`/`=`-separated. `default_move` is the `AT` value
/// when the stream carries none: the cycle's position in the file.
/// Shared by the v1 `CYCLE` record and the legacy `; CYCLE` marker.
fn parse_cycle_rec(
    it: &mut std::iter::Peekable<std::str::SplitWhitespace<'_>>,
    default_move: usize,
) -> Result<CycleRec, String> {
    let Some(k) = it.next() else {
        return Err("CYCLE needs a kind".into());
    };
    let Some(kind) = cycle_kind(k) else {
        return Err(format!("unknown cycle kind {k:?}"));
    };
    let mut r = CycleRec {
        kind,
        x: 0.0,
        y: 0.0,
        r_z: 0.0,
        bottom_z: 0.0,
        entry_z: 0.0,
        peck: 0.0,
        dwell: 0.0,
        first_move: default_move,
        feed: None,
    };
    while let Some(w) = it.next() {
        // A pure-alphabetic token is a spaced key; its value follows.
        let (k, vtok) = if !w.is_empty() && w.bytes().all(|b| b.is_ascii_alphabetic()) {
            let Some(v) = it.next() else {
                return Err(format!("cycle word {w:?} has no value"));
            };
            (w, v)
        } else {
            let Some((k, v)) = cnc_word(w) else {
                return Err(format!("cycle word {w:?}"));
            };
            (k, v)
        };
        let Ok(n) = num(vtok) else {
            return Err(format!("cycle value {vtok:?}"));
        };
        match k.to_ascii_uppercase().as_str() {
            "X" => r.x = n / 1000.0,
            "Y" => r.y = n / 1000.0,
            "Z" | "BZ" => r.bottom_z = n / 1000.0,
            "R" | "RZ" => r.r_z = n / 1000.0,
            "Q" | "PECK" => r.peck = n / 1000.0,
            "P" | "DWELL" => r.dwell = n / 1000.0,
            "F" => r.feed = Some(n / 1000.0),
            "ENTRY" => r.entry_z = n / 1000.0,
            "AT" => r.first_move = n.max(0.0) as usize,
            k => return Err(format!("unknown cycle word {k:?}")),
        }
    }
    Ok(r)
}

/// Parse a stream (our v1 form, or any legacy `.cl` golden).
pub fn parse(text: &str) -> Result<Vec<Record>, CncError> {
    let mut out = Vec::new();
    let mut move_idx = 0usize;
    let mut last_z = 0.0f64;
    let mut in_section = false;
    for (lineno, raw) in text.lines().enumerate() {
        let line = raw.trim_end();
        let fail = |why: String| CncError::Syntax { line: lineno + 1, why };
        if line.is_empty() {
            continue;
        }
        // Comments: the CNC magic header opens the v1 dialect; a `.cl`
        // file is simply one without it (and its `; HSMW-CL` header is
        // an ordinary comment here). A `; CYCLE` line is the v3 golden
        // marker, parsed exactly like a v1 CYCLE record.
        if let Some(rest) = line.strip_prefix(";") {
            let t = rest.trim_start();
            if t.starts_with("HSMW-CNC") || t.starts_with("HSMW-CL") {
                continue; // version token
            }
            if let Some(c) = t.strip_prefix("CYCLE") {
                let mut wit = c.split_whitespace().peekable();
                let r = parse_cycle_rec(&mut wit, move_idx).map_err(fail)?;
                out.push(Record::Cycle(r));
                continue;
            }
            // The `.cl` v5 plasma records (#670) are the `.cnc` ones
            // behind a `; ` (#671): one reader, so a plasma golden is
            // a legal post input like every other.
            if let Some(w) = t.strip_prefix("PLASMA ") {
                out.push(Record::Plasma(super::cnc_torch::parse_header(w).map_err(fail)?));
                continue;
            }
            if let Some(w) = t.strip_prefix("TORCH ") {
                out.push(Record::Torch(super::cnc_torch::parse_torch(w).map_err(fail)?));
                continue;
            }
            continue; // any other comment is nothing to the post
        }
        let mut it = line.split_whitespace().peekable();
        let head = it.next().unwrap_or("");
        match head {
            "PARAM" => {
                let Some(name) = it.next() else {
                    return Err(fail("PARAM needs a name and a value".into()));
                };
                // The value is the rest of the line (whitespace run
                // collapsed to single spaces): strings may carry
                // spaces - operation names do.
                let parts: Vec<&str> = it.collect();
                if parts.is_empty() {
                    return Err(fail("PARAM needs a name and a value".into()));
                }
                out.push(Record::Param { name: name.into(), value: param_value(&parts.join(" ")) });
            }
            "WARN" => {
                let Some(text) = line.strip_prefix("WARN") else {
                    return Err(fail("WARN needs text".into()));
                };
                let text = text.trim();
                if text.is_empty() {
                    return Err(fail("WARN needs text".into()));
                }
                out.push(Record::Warn { text: text.into() });
            }
            "TOOL" => {
                let Some(n) = it.next() else {
                    return Err(fail("TOOL needs a number and a name".into()));
                };
                let Ok(number) = n.parse::<u32>() else {
                    return Err(fail(format!("TOOL number {n:?}")));
                };
                // The name is the rest of the line (runs collapsed):
                // library names carry spaces ("1/4 INCH 4FL").
                let parts: Vec<&str> = it.collect();
                if parts.is_empty() {
                    return Err(fail("TOOL needs a number and a name".into()));
                }
                out.push(Record::Tool { number, name: parts.join(" ") });
            }
            "SECTION" => {
                out.push(Record::Section { begin: true });
                in_section = true;
            }
            "ENDSECTION" => {
                if in_section {
                    out.push(Record::Section { begin: false });
                }
                in_section = false;
            }
            "END" => {
                // Legacy close: a section we never opened is still a
                // boundary worth reporting, but only after records.
                if move_idx > 0 || out.iter().any(|r| matches!(r, Record::Cycle(_))) {
                    out.push(Record::Section { begin: false });
                }
                in_section = false;
            }
            "START" => {
                // Legacy position line: X Y Z, microns; Z chains on.
                let nums: Vec<f64> = match it.map(num).collect::<Result<Vec<_>, _>>() {
                    Ok(v) => v,
                    Err(_) => return Err(fail("START needs numbers".into())),
                };
                last_z = nums.last().map_or(0.0, |v| v / 1000.0);
            }
            "Rapid" | "Feed" | "Plunge" | "Arc" => {
                let mut ccw = false;
                if head == "Arc" {
                    // v1 spells the direction CW/CCW as a word; legacy
                    // carries it in the S token instead.
                    let peeked = it.peek().copied().unwrap_or("");
                    if peeked == "CW" || peeked == "CCW" {
                        ccw = peeked == "CCW";
                        it.next();
                    }
                }
                let words = motion_words(&mut it, lineno + 1)?;
                let mut x = None;
                let mut y = None;
                let mut z = None;
                let mut feed = None;
                let mut ci = None;
                let mut cj = None;
                let mut ck = None;
                let mut abc = [0.0f64; 3];
                let mut tilted = false;
                let mut pos = 0usize;
                for w in words {
                    match w {
                        MotionWord::Value(v) => {
                            match pos {
                                0 => x = Some(v / 1000.0),
                                1 => y = Some(v / 1000.0),
                                2 => z = Some(v / 1000.0),
                                _ => return Err(fail("too many positional values".into())),
                            }
                            pos += 1;
                        }
                        MotionWord::Keyed(k, v) => match k {
                            "X" => x = Some(v / 1000.0),
                            "Y" => y = Some(v / 1000.0),
                            "Z" => z = Some(v / 1000.0),
                            "F" => feed = Some(v / 1000.0),
                            "I" => ci = Some(v / 1000.0),
                            "J" => cj = Some(v / 1000.0),
                            "K" => ck = Some(v / 1000.0),
                            "S" => ccw = v >= 0.0,
                            "A" => {
                                abc[0] = v / 1000.0;
                                tilted = true;
                            }
                            "B" => {
                                abc[1] = v / 1000.0;
                                tilted = true;
                            }
                            "C" => {
                                abc[2] = v / 1000.0;
                                tilted = true;
                            }
                            _ => {}
                        },
                    }
                }
                let (Some(x), Some(y)) = (x, y) else {
                    return Err(fail("move needs at least X and Y".into()));
                };
                let z = z.unwrap_or(last_z);
                last_z = z;
                // The center pair names the plane (M11.2 A): exactly
                // two of I J K, or none. One alone, or all three, is
                // no plane at all - refused by name, never guessed.
                let (center, plane) = match (ci, cj, ck) {
                    (Some(i), Some(j), None) => (Some((i, j)), ArcPlane::Xy),
                    (Some(i), None, Some(k)) => (Some((i, k)), ArcPlane::Xz),
                    (None, Some(j), Some(k)) => (Some((j, k)), ArcPlane::Yz),
                    (None, None, None) => (None, ArcPlane::Xy),
                    _ => {
                        return Err(fail(
                            "arc center needs exactly two of I, J, K (I J, I K or J K)".into(),
                        ))
                    }
                };
                if head == "Arc" && center.is_none() {
                    return Err(fail("arc needs a center (I J, I K or J K)".into()));
                }
                out.push(Record::Move(MoveRec {
                    kind: match head {
                        "Rapid" => MoveKind::Rapid,
                        "Plunge" => MoveKind::Plunge,
                        _ => MoveKind::Feed,
                    },
                    x,
                    y,
                    z,
                    feed,
                    center,
                    ccw,
                    plane,
                    abc: tilted.then_some(abc),
                }));
                move_idx += 1;
                in_section = true;
            }
            "CYCLE" => {
                let r = parse_cycle_rec(&mut it, move_idx).map_err(fail)?;
                out.push(Record::Cycle(r));
                in_section = true;
            }
            "PART" => {
                let dir = it.next().unwrap_or("");
                if dir == "TRANSFER" {
                    // The canonical spelling is exactly two 1-based
                    // station numbers: `PART TRANSFER 1 2`. A missing
                    // or non-numeric station is refused by name -
                    // guessing which spindles a hand-edited line
                    // meant is the silent-wrong shape.
                    let station = |w: &str| w.parse::<u32>().ok().filter(|n| *n >= 1);
                    let bad = |w: &str| {
                        fail(format!("PART TRANSFER station {w:?} is not a 1-based spindle number"))
                    };
                    let fw = it
                        .next()
                        .ok_or_else(|| fail("PART TRANSFER needs FROM and TO stations".into()))?;
                    let from = station(fw).ok_or_else(|| bad(fw))?;
                    let tw = it
                        .next()
                        .ok_or_else(|| fail(format!("PART TRANSFER {from} needs its TO station")))?;
                    let to = station(tw).ok_or_else(|| bad(tw))?;
                    out.push(Record::Transfer { from, to });
                } else {
                    let loading = match dir {
                        "LOAD" => true,
                        "UNLOAD" => false,
                        other => {
                            return Err(fail(format!(
                                "PART needs LOAD, UNLOAD or TRANSFER, found {other:?}"
                            )))
                        }
                    };
                    // One optional ADVANCE word - spaced, joined, or
                    // keyed - carrying microns, the lattice law again.
                    let mut advance = 0.0;
                    if let Some(w) = it.next() {
                        let inline = match w.split_once([':', '=']) {
                            Some((k, v)) if k.eq_ignore_ascii_case("ADVANCE") => Some(v),
                            Some(_) => None,
                            None => w
                                .strip_prefix("ADVANCE")
                                .filter(|r| !r.is_empty())
                                .map(|r| r as &str)
                                .or_else(|| {
                                    if w.eq_ignore_ascii_case("ADVANCE") {
                                        Some("")
                                    } else {
                                        None
                                    }
                                }),
                        };
                        let Some(inline) = inline else {
                            return Err(fail(format!("PART word {w:?} is not ADVANCE")));
                        };
                        let v = if inline.is_empty() {
                            it.next().ok_or_else(|| fail("ADVANCE needs a value".into()))?
                        } else {
                            inline
                        };
                        // ADVANCE rides the LOAD alone - the serializer
                        // writes it on no other line, so seeing it there
                        // is a hand-edited stream saying something
                        // confused; refuse by name, never drop silently.
                        if !loading {
                            return Err(fail("ADVANCE rides PART LOAD, not PART UNLOAD".into()));
                        }
                        advance = num(v).map_err(|_| fail(format!("ADVANCE {v:?}")))? / 1000.0;
                    }
                    out.push(Record::Part { loading, advance });
                }
            }
            "COMP" => {
                // Cutter compensation (M13.11): LEFT / RIGHT carry the
                // register's radius in microns; OFF carries nothing. A
                // side without a radius, or a radius that is not
                // positive, is refused by name - a register the parser
                // guessed is the undersize part.
                let dir = it.next().unwrap_or("");
                let side = match dir {
                    "LEFT" => Some(CompSide::Left),
                    "RIGHT" => Some(CompSide::Right),
                    "OFF" => None,
                    other => {
                        return Err(fail(format!("COMP needs LEFT, RIGHT or OFF, found {other:?}")))
                    }
                };
                let radius_mm = match side {
                    None => 0.0,
                    Some(_) => {
                        let w = it
                            .next()
                            .ok_or_else(|| fail(format!("COMP {dir} needs a radius in microns")))?;
                        let v = num(w).map_err(|_| fail(format!("COMP radius {w:?}")))? / 1000.0;
                        if v.is_nan() || v <= 0.0 {
                            return Err(fail(format!("COMP radius {w:?} is not positive")));
                        }
                        v
                    }
                };
                // An optional WEAR after the radius (#38); anything
                // else trailing is refused - a guessed mode is a part a
                // radius off.
                let wear = match (side, it.next()) {
                    (_, None) => false,
                    (Some(_), Some("WEAR")) => true,
                    (_, Some(other)) => {
                        return Err(fail(format!("COMP {dir} takes only WEAR after its radius, found {other:?}")))
                    }
                };
                out.push(Record::Comp { side, radius_mm, wear });
                in_section = true;
            }
            "THREAD" => {
                let t = super::cnc_thread::parse(it).map_err(fail)?;
                out.push(Record::Thread(t));
                in_section = true;
            }
            "PROBE" => {
                // The measurement ask (M5.3 D): one micron integer,
                // the lattice law like ADVANCE. A missing or
                // unmeasurable value is refused by name - a probe
                // whose budget the parser guessed at is the silent
                // wrong answer the Prime Directive forbids twice
                // over.
                let w = it
                    .next()
                    .ok_or_else(|| fail("PROBE needs a tolerance in microns".into()))?;
                let v = num(w).map_err(|_| fail(format!("PROBE tolerance {w:?}")))?;
                if v < 0.0 {
                    return Err(fail(format!("PROBE tolerance {w:?} is negative")));
                }
                out.push(Record::Probe { tol_mm: v / 1000.0 });
            }
            "TOUCH" => {
                let t = super::probe_rec::parse_touch(&mut it).map_err(fail)?;
                out.push(Record::Touch(t));
                move_idx += 1;
                in_section = true;
            }
            "RETREAT" => {
                let r = super::probe_rec::parse_retreat(&mut it).map_err(fail)?;
                out.push(Record::Retreat(r));
                move_idx += 1;
                in_section = true;
            }
            "GUARD" => {
                let g = super::probe_rec::parse_guard(&mut it).map_err(fail)?;
                out.push(Record::Guard(g));
                move_idx += 1;
                in_section = true;
            }
            "PLASMA" | "TORCH" => {
                let words = line.trim_start()[head.len()..].trim_start();
                out.push(if head == "PLASMA" {
                    Record::Plasma(super::cnc_torch::parse_header(words).map_err(fail)?)
                } else {
                    Record::Torch(super::cnc_torch::parse_torch(words).map_err(fail)?)
                });
            }
            "FOUND" => {
                let f = super::probe_rec::parse_found(&mut it).map_err(fail)?;
                out.push(Record::Found(f));
            }
            other => {
                return Err(fail(format!("unknown record {other:?}")));
            }
        }
    }
    Ok(out)
}

fn cycle_kind(name: &str) -> Option<CycleKind> {
    Some(match name.to_ascii_uppercase().as_str() {
        "SPOT" => CycleKind::Spot,
        "DRILL" => CycleKind::Drill,
        "PECK" => CycleKind::Peck,
        "DEEP" => CycleKind::Deep,
        "TAP" => CycleKind::Tap,
        "FLOATINGTAP" => CycleKind::FloatingTap,
        "BORE" => CycleKind::Bore,
        "BACKBORE" => CycleKind::BackBore,
        "REAM" => CycleKind::Ream,
        _ => return None,
    })
}

fn param_value(v: &str) -> Value {
    let t = v.trim();
    if t.eq_ignore_ascii_case("true") {
        return json!(true);
    }
    if t.eq_ignore_ascii_case("false") {
        return json!(false);
    }
    if let Ok(n) = t.parse::<f64>()
        && n.is_finite()
    {
        return json!(n);
    }
    json!(t.trim_matches('"'))
}
