//! Posting for a machine with an inverted linear axis (#374): the
//! controller's frame is the CAM frame mirrored in that axis, so the
//! record stream is translated once, here, before any script sees it.
//!
//! Provenance: plane geometry plus the public G-code standards (RS-274 /
//! NIST RS274NGC, ISO 6983): G2/G3 are read from the plane normal's
//! positive end and G41/G42 looking along the travel, so a reflection of
//! one in-plane axis reverses both, and a second reflection in the same
//! plane cancels the first. The grammar and every name here are ours.
//!
//! The rules, for each inverted axis `a` (sign `s = -1`):
//! - every coordinate on `a` is written `s * v`: move and cycle end
//!   points, arc centres (so the I/J/K increments flip with them), the
//!   `Start*` chain and the work-offset origin (`Origin*`), the stock
//!   box (whose left/right or front/back swap as they flip);
//! - an arc whose plane holds exactly one inverted axis reverses sense
//!   (CW <-> CCW, i.e. G2 <-> G3);
//! - cutter compensation in the XY plane (the plane every mill
//!   generator compensates in) swaps sides (G41 <-> G42) when exactly
//!   one of X and Y is inverted;
//! - probing (#392), for a script that declares `probeAxisInversion =
//!   true`: a touch's target and nominal trip, a back-off's incremental
//!   delta and nominal end, and each found axis's expected value negate
//!   on the inverted axis. The script's trip read, deviation (found -
//!   expected) and work-offset write then all run in the controller's
//!   frame, where the register holds the part datum negated on that
//!   axis - so the correction it adds is the physical one, negated with
//!   it (proved by the macro interpreter in the app's
//!   `inverted_probe` suite on a simulated inverted machine).
//!
//! Refused by name, never translated: an inverted Z or any letter but
//! X / Y (machine level, [`crate::machine::untranslated_inversion`]), a
//! script that does not declare `axisInversion = true` (dialects not
//! covered by goldens), helical arcs, threading cycles, tilted (A/B/C)
//! moves, C-axis live tooling, and probing through a script that does
//! not declare `probeAxisInversion = true`. A refusal is a hard error:
//! nothing is posted, so no caller can write a half-translated program.

use super::cnc::Record;
use super::js::Script;
use super::PostError;
use crate::cl::{ArcPlane, CompSide};
use serde_json::{json, Value};

/// Which program axes the controller reads negated.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Flip {
    pub x: bool,
    pub y: bool,
    pub z: bool,
}

impl Flip {
    fn any(self) -> bool {
        self.x || self.y || self.z
    }

    fn sign(on: bool) -> f64 {
        if on {
            -1.0
        } else {
            1.0
        }
    }

    /// The two in-plane axes' inversion, in the plane's word order.
    fn in_plane(self, p: ArcPlane) -> (bool, bool) {
        match p {
            ArcPlane::Xy => (self.x, self.y),
            ArcPlane::Xz => (self.x, self.z),
            ArcPlane::Yz => (self.y, self.z),
        }
    }
}

/// The model name and the inverted letters a machine value states:
/// the mirror's `invertedAxes` list (what `machine::mirror` writes), or,
/// through the raw door (a `.mch` file's own JSON), the parts'
/// `"direction": -1`, read by the one parser that owns that grammar.
pub fn stated(mch: &Value) -> Result<(String, Vec<String>), String> {
    if mch.pointer("/kinematics/default/parts").is_some() {
        return match crate::machine::parse_mch(&mch.to_string()) {
            Ok(m) => {
                let letters = m.inverted_axes().into_iter().map(str::to_owned).collect();
                Ok((m.model, letters))
            }
            // A file the parser refuses still posts as it always did -
            // unless it states a direction, which must never fall back
            // to "standard" unread.
            Err(e) if states_direction(mch.pointer("/kinematics/default/parts")) => Err(e),
            Err(_) => Ok((String::new(), Vec::new())),
        };
    }
    let model = mch.get("name").and_then(Value::as_str).unwrap_or("").to_owned();
    if let Some(axes) = mch.pointer("/kinematics/static/axes").and_then(Value::as_array) {
        for a in axes {
            if a.get("reversal").and_then(Value::as_bool) == Some(true) {
                let l = a.get("letter").and_then(Value::as_str).unwrap_or("?");
                return Err(format!(
                    "machine {model}: axis {l} states \"reversal\": true, which the post does not read - \
                     state an inverted axis as \"direction\": -1 in the machine file"
                ));
            }
        }
    }
    match mch.get("invertedAxes") {
        None | Some(Value::Null) => Ok((model, Vec::new())),
        Some(Value::Array(a)) => a
            .iter()
            .map(|l| {
                l.as_str()
                    .map(str::to_owned)
                    .ok_or_else(|| format!("machine {model}: invertedAxes must list axis letters, not {l}"))
            })
            .collect::<Result<Vec<_>, _>>()
            .map(|v| (model, v)),
        Some(other) => Err(format!("machine {model}: invertedAxes must be a list of axis letters, not {other}")),
    }
}

fn states_direction(parts: Option<&Value>) -> bool {
    let Some(arr) = parts.and_then(Value::as_array) else {
        return false;
    };
    arr.iter().any(|p| p.get("direction").is_some_and(|d| !d.is_null()) || states_direction(p.get("parts")))
}

/// Translate the stream for the machine's inverted axes, or refuse by
/// name. A standard machine gets its records back untouched.
pub fn apply(records: Vec<Record>, mch: &Value, script: &Script) -> Result<Vec<Record>, PostError> {
    let (model, letters) = stated(mch).map_err(PostError::Refused)?;
    if letters.is_empty() {
        return Ok(records);
    }
    let refs: Vec<&str> = letters.iter().map(String::as_str).collect();
    if let Some(why) = crate::machine::untranslated_inversion(&model, &refs) {
        return Err(PostError::Refused(why));
    }
    let what = crate::machine::inverted_phrase(&model, &refs);
    if !script.translates_inverted_axes() {
        return Err(PostError::Refused(format!(
            "{what}, and this post script is not verified for an inverted axis (it does not declare \
             axisInversion = true) - post with a script that does (Fanuc, Haas)"
        )));
    }
    let flip = Flip { x: refs.contains(&"X"), y: refs.contains(&"Y"), z: refs.contains(&"Z") };
    translate_with(records, flip, script.translates_inverted_probing())
        .map_err(|why| PostError::Refused(format!("{what}: {}", why.join("; "))))
}

/// The record-by-record mirror, probing refused. `Err` lists every
/// refusal by name (once each), so one run tells the user everything at
/// once.
pub fn translate(records: Vec<Record>, f: Flip) -> Result<Vec<Record>, Vec<String>> {
    translate_with(records, f, false)
}

/// [`translate`], with `probing` true for a script whose probing blocks
/// are verified on the mirrored stream (`probeAxisInversion = true`).
pub fn translate_with(records: Vec<Record>, f: Flip, probing: bool) -> Result<Vec<Record>, Vec<String>> {
    translate_rules(records, f, Rules { probing, helix: false, cause: "an inverted axis" })
}

/// What one caller of the mirror carries (#1649): the machine door
/// ([`apply`]) keeps its #374 rules; a flipped WCS (`wcs_flip`) also
/// carries a helix, which its own equivalence suite pins.
#[derive(Clone, Copy, Debug)]
pub struct Rules<'a> {
    /// Probing records mirror (the script declares `probeAxisInversion`).
    pub probing: bool,
    /// A helical arc mirrors: the reflection is an isometry, so the arc
    /// reverses in its plane and the travel along the normal stays.
    pub helix: bool,
    /// The refusal's last words: "an inverted axis", "a flipped WCS axis".
    pub cause: &'a str,
}

/// The mirror under a caller's [`Rules`].
pub fn translate_rules(records: Vec<Record>, f: Flip, rules: Rules) -> Result<Vec<Record>, Vec<String>> {
    if !f.any() {
        return Ok(records);
    }
    let Rules { probing, helix, cause } = rules;
    let (sx, sy, sz) = (Flip::sign(f.x), Flip::sign(f.y), Flip::sign(f.z));
    let mut refused: Vec<String> = Vec::new();
    let mut refuse = |op: &str, what: &str| {
        let who = if op.is_empty() { String::new() } else { format!("operation {op}: ") };
        let line = format!("{who}{what} is not supported for {cause} yet");
        if !refused.contains(&line) {
            refused.push(line);
        }
    };
    let mut op = String::new();
    // The chain in CAM coordinates: a helix is judged where it was made.
    let mut pos = (0.0f64, 0.0f64, 0.0f64);
    let mut out: Vec<Record> = Vec::with_capacity(records.len());
    // The stock box's first edge on each inverted axis, by its index in
    // `out`, waiting for its pair: the mirrored box's left edge is the
    // old right edge negated, so the two swap as they flip.
    let mut open_edge: [Option<usize>; 2] = [None, None];
    for r in records {
        let r = match r {
            Record::Param { name, value } => {
                let num = value.as_f64();
                match (name.as_str(), num) {
                    ("StartX", Some(v)) => pos.0 = v,
                    ("StartY", Some(v)) => pos.1 = v,
                    ("StartZ", Some(v)) => pos.2 = v,
                    _ => {}
                }
                if name == "OperationName" {
                    op = value.as_str().unwrap_or("").to_owned();
                }
                if name == "LiveTool" {
                    refuse(&op, "C-axis live tooling (polar / G12.1 work)");
                }
                let edge = match name.as_str() {
                    "StockXYLeft" | "StockXYRight" if f.x => Some((0, name == "StockXYLeft")),
                    "StockXYFront" | "StockXYBack" if f.y => Some((1, name == "StockXYFront")),
                    _ => None,
                };
                match edge {
                    Some((k, true)) => {
                        open_edge[k] = Some(out.len());
                        Record::Param { name, value }
                    }
                    Some((k, false)) => {
                        let first = open_edge[k].take();
                        let pair = first.and_then(|i| match out.get(i) {
                            Some(Record::Param { value: v, .. }) => v.as_f64().map(|a| (i, a)),
                            _ => None,
                        });
                        match (pair, value.as_f64()) {
                            (Some((i, a)), Some(b)) => {
                                if let Some(Record::Param { value: v, .. }) = out.get_mut(i) {
                                    *v = json!(-b);
                                }
                                Record::Param { name, value: json!(-a) }
                            }
                            _ => {
                                refuse(&op, "a stock box edge without its numeric pair");
                                Record::Param { name, value }
                            }
                        }
                    }
                    None => Record::Param { value: param(&name, value, f), name },
                }
            }
            Record::Move(mut m) => {
                if m.abc.is_some() {
                    refuse(&op, "a tilted (A/B/C rotary) move");
                }
                if let Some((ca, cb)) = m.center {
                    let normal = match m.plane {
                        ArcPlane::Xy => (pos.2, m.z),
                        ArcPlane::Xz => (pos.1, m.y),
                        ArcPlane::Yz => (pos.0, m.x),
                    };
                    if !helix && (normal.1 - normal.0).abs() > 1e-9 {
                        refuse(&op, "a helical arc");
                    }
                    let (ia, ib) = f.in_plane(m.plane);
                    m.center = Some((Flip::sign(ia) * ca, Flip::sign(ib) * cb));
                    if ia != ib {
                        m.ccw = !m.ccw;
                    }
                }
                pos = (m.x, m.y, m.z);
                m.x *= sx;
                m.y *= sy;
                m.z *= sz;
                Record::Move(m)
            }
            // A protected positioning move (#174) is an absolute straight
            // feed: it mirrors like one. The touches around it carry the
            // probing verdict.
            Record::Guard(mut g) => {
                pos = (g.to[0], g.to[1], g.to[2]);
                g.to = [g.to[0] * sx, g.to[1] * sy, g.to[2] * sz];
                Record::Guard(g)
            }
            Record::Cycle(mut c) => {
                if f.z {
                    refuse(&op, "a canned cycle on an inverted Z");
                }
                pos = (c.x, c.y, c.r_z);
                c.x *= sx;
                c.y *= sy;
                Record::Cycle(c)
            }
            Record::Comp { side, radius_mm, wear } => {
                let side = if f.x == f.y {
                    side
                } else {
                    side.map(|s| match s {
                        CompSide::Left => CompSide::Right,
                        CompSide::Right => CompSide::Left,
                    })
                };
                Record::Comp { side, radius_mm, wear }
            }
            Record::Thread(Some(t)) => {
                refuse(&op, "a threading cycle");
                Record::Thread(Some(t))
            }
            Record::Touch(mut t) => {
                if !probing {
                    refuse(&op, NO_PROBING);
                }
                pos = (t.expect[0], t.expect[1], t.expect[2]);
                t.target = mirror(t.target, f);
                t.expect = mirror(t.expect, f);
                Record::Touch(t)
            }
            Record::Retreat(mut t) => {
                if !probing {
                    refuse(&op, NO_PROBING);
                }
                pos = (t.expect[0], t.expect[1], t.expect[2]);
                // An increment flips with the axis exactly as a point
                // does: the controller's +X is the physical -X.
                t.delta = mirror(t.delta, f);
                t.expect = mirror(t.expect, f);
                Record::Retreat(t)
            }
            Record::Found(mut t) => {
                if !probing {
                    refuse(&op, NO_PROBING);
                }
                for a in &mut t.axes {
                    let on = [f.x, f.y, f.z].get(a.axis).copied().unwrap_or(false);
                    a.expected *= Flip::sign(on);
                }
                Record::Found(t)
            }
            other => other,
        };
        out.push(r);
    }
    if open_edge.iter().any(Option::is_some) {
        refuse(&op, "a stock box edge without its pair");
    }
    if refused.is_empty() {
        Ok(out)
    } else {
        Err(refused)
    }
}

/// The refusal for probing through a script not verified for it (#392).
const NO_PROBING: &str =
    "probing with a post script that does not declare probeAxisInversion = true (Fanuc and Haas do)";

/// A point or increment in the controller's frame: negated on each
/// inverted axis.
fn mirror(p: [f64; 3], f: Flip) -> [f64; 3] {
    [Flip::sign(f.x) * p[0], Flip::sign(f.y) * p[1], Flip::sign(f.z) * p[2]]
}

/// A PARAM's value in the controller's frame: the axis coordinates the
/// scripts measure from (start point, work-offset origin) negate. The
/// stock box's edges are paired in [`translate`] instead.
fn param(name: &str, value: Value, f: Flip) -> Value {
    let neg = |v: &Value| v.as_f64().map_or_else(|| v.clone(), |n| json!(-n));
    match name {
        "StartX" | "OriginX" if f.x => neg(&value),
        "StartY" | "OriginY" if f.y => neg(&value),
        "StartZ" | "OriginZ" if f.z => neg(&value),
        _ => value,
    }
}
