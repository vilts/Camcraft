//! The `THREAD` record of the intermediate stream (issue #34): the
//! threading cycle `cl::ThreadCycle` states, in stream clothing.
//!
//! Provenance: our own line format (`docs/CNC-INTERMEDIATE.md`); the
//! quantities are the ones public lathe programming references list
//! for a multi-pass threading cycle (behaviour only). `THREAD ...`
//! opens the span of explicit passes the record stands for and
//! `THREAD END` closes it; the passes between stay ordinary `MOVE`
//! records, so a post with no cycle hook posts them as it always did.
//! `HAND right|left` (#629) states the thread's hand; a record without
//! it is refused by name.

use serde_json::{json, Value};
use std::collections::BTreeMap;

use crate::cl::ThreadHand;

/// One threading-cycle record. Points and lengths are millimeters
/// (lengths on the µm lattice in the text), radial quantities radius
/// values; `lead` and `infeed_angle` ride as 6-decimal reals - a
/// lead of 24 TPI is not a whole number of microns.
#[derive(Clone, Debug, PartialEq)]
pub struct ThreadRec {
    pub start: [f64; 3],
    pub end: [f64; 3],
    pub major_diameter: f64,
    pub minor_diameter: f64,
    pub lead: f64,
    pub height: f64,
    pub first_depth: f64,
    pub min_depth: f64,
    pub finish_allowance: f64,
    pub infeed_angle: f64,
    pub spring_passes: u32,
    pub pull_out: f64,
    /// The hand the passes cut (#629): `HAND right|left`, required.
    pub hand: ThreadHand,
    /// Alternating flank infeed (issue #417): written `ALT 1` only when
    /// set, so every earlier stream reads and writes the same bytes.
    pub alternating: bool,
    /// Every roughing depth cut twice (issue #418): written `REP 1`
    /// only when set, the `ALT` precedent.
    pub repeat_each_depth: bool,
}

/// µm-integer text, the stream's length law.
fn q(v: f64) -> String {
    if !v.is_finite() {
        return "nan".into();
    }
    ((v * 1000.0).round() as i64).to_string()
}

impl ThreadRec {
    /// From the CL record (the dump's one door).
    #[must_use]
    pub fn from_cycle(t: &crate::cl::ThreadCycle) -> Self {
        ThreadRec {
            start: [t.start.x, t.start.y, t.start.z],
            end: [t.end.x, t.end.y, t.end.z],
            major_diameter: t.major_diameter,
            minor_diameter: t.minor_diameter,
            lead: t.lead,
            height: t.height,
            first_depth: t.first_depth,
            min_depth: t.min_depth,
            finish_allowance: t.finish_allowance,
            infeed_angle: t.infeed_angle,
            spring_passes: t.spring_passes,
            pull_out: t.pull_out,
            hand: t.hand,
            alternating: t.alternating,
            repeat_each_depth: t.repeat_each_depth,
        }
    }

    /// The property object the script's `onThreadCycle` reads.
    #[must_use]
    pub fn props(&self) -> BTreeMap<String, Value> {
        let mut m = BTreeMap::new();
        m.insert("command".into(), json!("Threading"));
        m.insert("X".into(), json!(self.start[0]));
        m.insert("Y".into(), json!(self.start[1]));
        m.insert("Z".into(), json!(self.start[2]));
        m.insert("EndX".into(), json!(self.end[0]));
        m.insert("EndY".into(), json!(self.end[1]));
        m.insert("EndZ".into(), json!(self.end[2]));
        m.insert("MajorDiameter".into(), json!(self.major_diameter));
        m.insert("MinorDiameter".into(), json!(self.minor_diameter));
        m.insert("Lead".into(), json!(self.lead));
        m.insert("ThreadHeight".into(), json!(self.height));
        m.insert("FirstDepth".into(), json!(self.first_depth));
        m.insert("MinimumDepth".into(), json!(self.min_depth));
        m.insert("FinishAllowance".into(), json!(self.finish_allowance));
        m.insert("InfeedAngle".into(), json!(self.infeed_angle));
        m.insert("SpringPasses".into(), json!(self.spring_passes));
        m.insert("PullOut".into(), json!(self.pull_out));
        m.insert("Hand".into(), json!(self.hand.word()));
        m.insert("Alternating".into(), json!(self.alternating));
        m.insert("RepeatEachDepth".into(), json!(self.repeat_each_depth));
        m
    }

    /// The canonical `THREAD` line (newline-terminated).
    #[must_use]
    pub fn line(&self) -> String {
        format!(
            "THREAD X {} Y {} Z {} EX {} EY {} EZ {} MAJOR {} MINOR {} LEAD {:.6} HEIGHT {} \
             FIRST {} MIN {} FINISH {} ANGLE {:.6} SPRING {} PULLOUT {} HAND {}{}{}\n",
            q(self.start[0]),
            q(self.start[1]),
            q(self.start[2]),
            q(self.end[0]),
            q(self.end[1]),
            q(self.end[2]),
            q(self.major_diameter),
            q(self.minor_diameter),
            self.lead,
            q(self.height),
            q(self.first_depth),
            q(self.min_depth),
            q(self.finish_allowance),
            self.infeed_angle,
            self.spring_passes,
            q(self.pull_out),
            self.hand.word(),
            if self.alternating { " ALT 1" } else { "" },
            if self.repeat_each_depth { " REP 1" } else { "" },
        )
    }

    /// The trace pane's name for the record.
    #[must_use]
    pub fn label(&self) -> String {
        format!(
            "thread cycle · {}-hand · lead {:.4} · Z {:.3} → {:.3}",
            self.hand.word(), self.lead, self.start[2], self.end[2]
        )
    }
}

/// Parse the words after `THREAD`: `Ok(None)` is `THREAD END`. Every
/// key but `ALT` (issue #417) and `REP` (issue #418) - absent is 0 -
/// is required, each once; a
/// missing, repeated, unknown or non-finite word is refused by name - a cycle the parser guessed at
/// is a thread cut to the wrong depth.
pub fn parse<'a>(mut it: impl Iterator<Item = &'a str>) -> Result<Option<ThreadRec>, String> {
    const KEYS: [&str; 18] = [
        "X", "Y", "Z", "EX", "EY", "EZ", "MAJOR", "MINOR", "LEAD", "HEIGHT", "FIRST", "MIN", "FINISH", "ANGLE",
        "SPRING", "PULLOUT", "ALT", "REP",
    ];
    let mut seen: BTreeMap<&str, f64> = BTreeMap::new();
    let mut hand: Option<ThreadHand> = None;
    let mut first = true;
    while let Some(k) = it.next() {
        if first && k == "END" {
            return match it.next() {
                None => Ok(None),
                Some(w) => Err(format!("THREAD END takes no words, found {w:?}")),
            };
        }
        first = false;
        if k == "HAND" {
            let v = it.next().ok_or("THREAD HAND needs a value")?;
            let h = ThreadHand::from_word(v).ok_or_else(|| format!("THREAD HAND {v:?} is not right or left"))?;
            if hand.replace(h).is_some() {
                return Err("THREAD HAND given twice".into());
            }
            continue;
        }
        let Some(key) = KEYS.iter().find(|&&c| c == k) else {
            return Err(format!("THREAD: unknown word {k:?}"));
        };
        let v = it.next().ok_or_else(|| format!("THREAD {k} needs a value"))?;
        let n = v
            .parse::<f64>()
            .ok()
            .filter(|n| n.is_finite())
            .ok_or_else(|| format!("THREAD {k} {v:?} is not a number"))?;
        if seen.insert(key, n).is_some() {
            return Err(format!("THREAD {k} given twice"));
        }
    }
    let get = |k: &str| seen.get(k).copied().ok_or_else(|| format!("THREAD needs {k}"));
    let um = |k: &str| get(k).map(|v| v / 1000.0);
    // The hand is never guessed (#629): a record without it would
    // post its cycle under whichever spindle sense the script assumed.
    let hand = hand.ok_or("THREAD needs HAND")?;
    let spring = get("SPRING")?;
    if !(spring >= 0.0 && spring.fract() == 0.0 && spring <= f64::from(u32::MAX)) {
        return Err(format!("THREAD SPRING {spring} is not a pass count"));
    }
    let flag = |k: &str| match seen.get(k).copied() {
        None => Ok(false),
        Some(v) if v.to_bits() == 0.0_f64.to_bits() => Ok(false),
        Some(v) if v.to_bits() == 1.0_f64.to_bits() => Ok(true),
        Some(v) => Err(format!("THREAD {k} {v} is not 0 or 1")),
    };
    let alternating = flag("ALT")?;
    let repeat_each_depth = flag("REP")?;
    Ok(Some(ThreadRec {
        start: [um("X")?, um("Y")?, um("Z")?],
        end: [um("EX")?, um("EY")?, um("EZ")?],
        major_diameter: um("MAJOR")?,
        minor_diameter: um("MINOR")?,
        lead: get("LEAD")?,
        height: um("HEIGHT")?,
        first_depth: um("FIRST")?,
        min_depth: um("MIN")?,
        finish_allowance: um("FINISH")?,
        infeed_angle: get("ANGLE")?,
        spring_passes: spring as u32,
        pull_out: um("PULLOUT")?,
        hand,
        alternating,
        repeat_each_depth,
    }))
}
