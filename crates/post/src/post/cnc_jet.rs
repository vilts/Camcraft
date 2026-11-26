//! The waterjet records of the intermediate stream (#1882): the
//! `WATERJET` header and the `JET ON` / `JET OFF` spans that
//! `cl::WaterjetCut` states, in stream clothing - and the engine's laws
//! over them.
//!
//! Provenance: our own line format (`docs/CNC-INTERMEDIATE.md`,
//! "Waterjet records"), specced from general abrasive-waterjet practice
//! (jet on over the pierce point, a valve lag dwell, abrasive on, a
//! pierce dwell). One reader (`cl::parse_waterjet_header` /
//! `parse_jet_on` / `parse_jet_off`) takes the `.cnc` record and the
//! `.cl` comment form. It shares no state with the plasma records
//! (`cnc_torch`): a waterjet has no torch height control.

use super::js::Script;
use super::PostError;
use crate::cl::{ContourRole, JetSpan, WaterjetCut};
use serde_json::{json, Value};
use std::collections::BTreeMap;

/// The section's waterjet header. Dwells are seconds, `None` when the
/// program states none.
#[derive(Clone, Debug, PartialEq)]
pub struct WaterjetRec {
    pub abrasive: bool,
    pub pierce_dwell: Option<f64>,
    pub valve_lag: Option<f64>,
}

/// One jet event: `Some(role)` opens the jet before the next motion
/// record, `None` closes contour `contour` after the previous one.
#[derive(Clone, Debug, PartialEq)]
pub struct JetRec {
    pub contour: u32,
    pub on: Option<ContourRole>,
}

impl WaterjetRec {
    #[must_use]
    pub fn from_cut(w: &WaterjetCut) -> Self {
        WaterjetRec { abrasive: w.abrasive, pierce_dwell: w.pierce_dwell, valve_lag: w.valve_lag }
    }

    /// The canonical line: the `.cl` header without its `; `.
    #[must_use]
    pub fn line(&self) -> String {
        let cut = WaterjetCut { abrasive: self.abrasive, pierce_dwell: self.pierce_dwell, valve_lag: self.valve_lag, jets: Vec::new() };
        format!("{}\n", cut.header_line().trim_start_matches("; "))
    }

    /// The properties the header publishes for the section. An unstated
    /// dwell is ABSENT, never 0.
    #[must_use]
    pub fn props(&self) -> BTreeMap<String, Value> {
        let mut m = BTreeMap::new();
        m.insert("WaterjetAbrasive".into(), json!(self.abrasive));
        if let Some(d) = self.pierce_dwell {
            m.insert("PierceDwell".into(), json!(d));
        }
        if let Some(d) = self.valve_lag {
            m.insert("ValveLagDwell".into(), json!(d));
        }
        m
    }

    #[must_use]
    pub fn label(&self) -> String {
        format!("waterjet · {}", if self.abrasive { "abrasive" } else { "pure water" })
    }
}

/// The keys [`WaterjetRec::props`] may set - cleared at the section end.
pub const KEYS: [&str; 3] = ["WaterjetAbrasive", "PierceDwell", "ValveLagDwell"];

impl JetRec {
    #[must_use]
    pub fn on(j: &JetSpan) -> Self {
        JetRec { contour: j.contour, on: Some(j.role) }
    }

    #[must_use]
    pub fn off(contour: u32) -> Self {
        JetRec { contour, on: None }
    }

    /// The canonical line: the `.cl` record without its `; `.
    #[must_use]
    pub fn line(&self) -> String {
        let span = JetSpan { contour: self.contour, role: self.on.unwrap_or(ContourRole::Open), on: 0, off: 0 };
        let l = if self.on.is_some() { span.on_line() } else { span.off_line() };
        format!("{}\n", l.trim_start_matches("; "))
    }

    #[must_use]
    pub fn props(&self) -> BTreeMap<String, Value> {
        let mut m = BTreeMap::new();
        m.insert("Contour".into(), json!(self.contour));
        match self.on {
            Some(role) => {
                m.insert("command".into(), json!("JetOn"));
                m.insert("ContourRole".into(), json!(role.word().to_ascii_lowercase()));
            }
            None => {
                m.insert("command".into(), json!("JetOff"));
            }
        }
        m
    }

    #[must_use]
    pub fn label(&self) -> String {
        match self.on {
            Some(role) => format!("jet on · C{} {}", self.contour, role.word().to_ascii_lowercase()),
            None => format!("jet off · C{}", self.contour),
        }
    }
}

/// Parse the words after `WATERJET` (the `.cnc` record or the `.cl`
/// comment).
pub fn parse_header(words: &str) -> Result<WaterjetRec, String> {
    crate::cl::parse_waterjet_header(words).map(|w| WaterjetRec::from_cut(&w))
}

/// Parse the words after `JET`: `ON C<id> <ROLE>` or `OFF C<id>`.
pub fn parse_jet(words: &str) -> Result<JetRec, String> {
    let w = words.trim_start();
    if let Some(rest) = w.strip_prefix("ON ") {
        return Ok(JetRec::on(&crate::cl::parse_jet_on(rest, 0)?));
    }
    if let Some(rest) = w.strip_prefix("OFF") {
        return Ok(JetRec::off(crate::cl::parse_jet_off(rest)?));
    }
    Err(format!("JET needs ON or OFF, found {w:?}"))
}

/// The engine's waterjet state for one run: the header in force and
/// the open contour. A jet with no header, an ON while open, an OFF
/// that names another contour or none, a section left open and a rapid
/// while open are counted refusals by name, never repaired: an open
/// jet across a rapid cuts through the part.
#[derive(Default)]
pub struct JetLaw {
    header: bool,
    open: Option<u32>,
}

fn who(base: &BTreeMap<String, Value>) -> String {
    match base.get("OperationName").and_then(Value::as_str).filter(|s| !s.is_empty()) {
        Some(op) => format!("operation {op}: "),
        None => String::new(),
    }
}

impl JetLaw {
    /// A `WATERJET` header: the caller merges `props`. A script that
    /// cannot open and close a jet is refused by name.
    pub fn header(&mut self, s: &Script, base: &BTreeMap<String, Value>, warnings: &mut Vec<String>) {
        if self.header {
            warnings.push(format!("{}waterjet: a second WATERJET header in one section - refused", who(base)));
        }
        self.header = true;
        if !(s.hook_exists("onJetOn") && s.hook_exists("onJetOff")) {
            warnings.push(format!(
                "{}is waterjet cutting (a jet opened per contour), but this post script does not define onJetOn / onJetOff - use a waterjet post",
                who(base)
            ));
        }
    }

    /// A `JET` record: true when the hook may fire.
    pub fn jet(&mut self, j: &JetRec, base: &BTreeMap<String, Value>, warnings: &mut Vec<String>) -> bool {
        let w = who(base);
        if !self.header {
            warnings.push(format!("{w}waterjet: JET C{} with no WATERJET header in force - refused", j.contour));
            return false;
        }
        match (j.on.is_some(), self.open) {
            (true, Some(c)) => {
                warnings.push(format!("{w}waterjet: C{} is opened while the jet on C{c} is still on - refused", j.contour));
                false
            }
            (true, None) => {
                self.open = Some(j.contour);
                true
            }
            (false, Some(c)) if c == j.contour => {
                self.open = None;
                true
            }
            (false, open) => {
                warnings.push(format!(
                    "{w}waterjet: JET OFF C{} but {} - refused",
                    j.contour,
                    open.map_or_else(|| "no jet is on".to_owned(), |c| format!("C{c} is the open contour"))
                ));
                false
            }
        }
    }

    /// A rapid: never while the jet is on.
    pub fn rapid(&self, base: &BTreeMap<String, Value>, warnings: &mut Vec<String>) {
        if let Some(c) = self.open {
            warnings.push(format!("{}waterjet: a rapid move while the jet on C{c} is on - refused", who(base)));
        }
    }

    /// The section ends: no jet may stay on, and the header retires.
    pub fn section_end(&mut self, base: &mut BTreeMap<String, Value>, warnings: &mut Vec<String>) {
        if let Some(c) = self.open.take() {
            warnings.push(format!("{}waterjet: the jet on C{c} is never turned off - refused", who(base)));
        }
        self.header = false;
        for k in KEYS {
            base.remove(k);
        }
    }

    /// The stream ends: a jet still on there is the same refusal.
    pub fn close(&mut self, base: &mut BTreeMap<String, Value>, warnings: &mut Vec<String>) {
        if self.open.is_some() {
            self.section_end(base, warnings);
        }
    }
}

/// Fire the jet hook for a record the law admitted.
pub fn fire(s: &mut Script, j: &JetRec) -> Result<(), PostError> {
    s.call(if j.on.is_some() { "onJetOn" } else { "onJetOff" }, "")?;
    Ok(())
}
