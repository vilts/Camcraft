//! EDM machine data (#530, part of #290): the `.mch` file's `edm`
//! section for the `wire_edm` and `sinker_edm` kinds. Wire: the lower
//! and upper guide planes, the maximum taper and the wire diameters the
//! machine takes, AWT. Sinker: the orbit patterns the control offers
//! (with the control's own word for each), the maximum orbit radius and
//! the pickup method. The U/V and Z strokes are ordinary kinematics
//! axes. Every number is optional by the unstated-limits law - absent is
//! "not stated", never zero - and the generator codes are separate
//! sub-issues (#533, #542); the condition table (#531, `conditions[]`,
//! either kind) is `edm_conditions`.
//!
//! Provenance: the umbrella's design notes (#290, D1-D10) over public
//! EDM terminology (machine builders' published tutorials and the
//! EDM review literature it cites); no vendor machine file, manual
//! table or post was copied.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::def::{kind, Kind, Machine};
use super::no_spindle::{head_refusals, NoSpindle};

pub use super::edm_conditions::{Condition, ConditionField, CONDITION_ROLES};

/// The orbit pattern geometries CamCraft can generate (#290 D6). A
/// machine lists which it offers; the word is the control's.
pub const ORBIT_KINDS: [&str; 3] = ["circular", "vector", "planetary"];

/// One orbit pattern a sinker control offers: CamCraft's geometry name
/// (one of [`ORBIT_KINDS`]) and the control's word for it.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct OrbitWord {
    pub kind: String,
    pub word: String,
}

/// The `edm` section. Lengths are mm (the parser scales the file's
/// unit on the way in); the taper is degrees from vertical.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Edm {
    /// Wire: the lower guide's plane, machine Z, mm.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lower_guide_z: Option<f64>,
    /// Wire: the upper guide's plane, machine Z, mm (above the lower).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub upper_guide_z: Option<f64>,
    /// Wire: the largest taper angle the machine can cut, degrees from
    /// vertical (0 < angle < 90).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_taper_deg: Option<f64>,
    /// Wire: the wire diameters the machine takes, mm.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub wire_diameters: Vec<f64>,
    /// Wire: automatic wire threading. Unstated = not claimed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub awt: Option<bool>,
    /// Sinker: the orbit patterns the control offers.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub orbits: Vec<OrbitWord>,
    /// Sinker: the largest orbit radius, mm.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_orbit_radius: Option<f64>,
    /// Sinker: how the electrode is picked up / referenced (free text
    /// from the machine's manual: a chuck or holder system's name).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pickup: Option<String>,
    /// Either kind: the generator conditions the user states (#531).
    /// Empty = none stated (absent and `[]` read the same), and every
    /// EDM op refuses by name.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub conditions: Vec<Condition>,
}

/// The keys only a wire machine states.
pub const WIRE_KEYS: [&str; 5] = ["lower_guide_z", "upper_guide_z", "max_taper_deg", "wire_diameters", "awt"];
/// The keys only a sinker machine states.
pub const SINKER_KEYS: [&str; 3] = ["orbits", "max_orbit_radius", "pickup"];
/// The keys either EDM kind states.
pub const COMMON_KEYS: [&str; 1] = ["conditions"];
/// The keys of one orbit entry.
const ORBIT_KEYS: [&str; 2] = ["kind", "word"];

/// Why an EDM states no spindle or coolant: one sentence per key.
const EDM_HEAD: NoSpindle = NoSpindle {
    spindle: "an EDM machine has no spindle - a wire or an electrode states no rpm, power, speed range or tapping",
    live_tool: "an EDM machine has no spindle to drive a live tool",
    coolant: "an EDM machine states no coolant - its dielectric is the machine's own, not an M7/M8/M9 list",
};

/// The EDM head rules over a file's raw JSON (#530, the plasma law's
/// twin): no `spindle` key, live tool or coolant list, each named.
/// The caller has read the declared kind as wire or sinker EDM. A
/// rotary is allowed (an indexer on a wire or sinker table).
#[must_use]
pub fn edm_refusals(v: &Value) -> Vec<(String, String)> {
    head_refusals(v, &EDM_HEAD).0
}

impl Edm {
    /// The same law `parse_edm` enforces at the front door, for a
    /// hand-edited project file's machine (the project reader's gate).
    #[must_use]
    pub fn is_sane(&self) -> bool {
        let pos = |r: Option<f64>| r.is_none_or(|x| x.is_finite() && x > 0.0);
        let finite = |r: Option<f64>| r.is_none_or(f64::is_finite);
        finite(self.lower_guide_z)
            && finite(self.upper_guide_z)
            && match (self.lower_guide_z, self.upper_guide_z) {
                (Some(lo), Some(hi)) => hi > lo,
                _ => true,
            }
            && self.max_taper_deg.is_none_or(|a| a.is_finite() && a > 0.0 && a < 90.0)
            && self.wire_diameters.iter().all(|d| d.is_finite() && *d > 0.0)
            && pos(self.max_orbit_radius)
            && self.orbits.iter().all(|o| ORBIT_KINDS.contains(&o.kind.as_str()) && !o.word.trim().is_empty())
            && self.conditions_sane()
    }

    /// Does this section state only keys of `k` (a wire states no
    /// sinker key, a sinker no wire key, and no other kind states an
    /// `edm` section at all)? The parsed form of `parse_edm`'s rule.
    #[must_use]
    pub fn fits(&self, k: Kind) -> bool {
        let wire = self.lower_guide_z.is_some()
            || self.upper_guide_z.is_some()
            || self.max_taper_deg.is_some()
            || !self.wire_diameters.is_empty()
            || self.awt.is_some()
            || self.conditions.iter().any(|c| c.wire_diameter.is_some());
        let sinker = !self.orbits.is_empty()
            || self.max_orbit_radius.is_some()
            || self.pickup.is_some()
            || self.conditions.iter().any(|c| c.undersize.is_some());
        match k {
            Kind::WireEdm => !sinker,
            Kind::SinkerEdm => !wire,
            Kind::Mill | Kind::Lathe | Kind::MillTurn | Kind::Plasma => false,
        }
    }
}

impl Machine {
    /// The EDM rules over the PARSED machine (a hand-edited project file
    /// never went through `parse_mch`): an `edm` section only on an EDM
    /// kind and only with that kind's keys; an EDM states no spindle
    /// number, range, tapping word, spindle count, live tool or coolant.
    #[must_use]
    pub fn edm_sane(&self) -> bool {
        let k = kind(Some(self));
        if self.edm.as_ref().is_some_and(|e| !e.fits(k)) {
            return false;
        }
        if !matches!(k, Kind::WireEdm | Kind::SinkerEdm) {
            return true;
        }
        self.spindle_min.is_none()
            && self.spindle_max.is_none()
            && self.spindle_power.is_none()
            && self.spindle_ranges.is_empty()
            && self.rigid_tapping.is_none()
            && self.spindles.is_none()
            && self.live_tool.is_none()
            && self.coolants.is_empty()
    }
}

/// A stated number, scaled by `f`, finite AFTER the scaling (an inch
/// `1e308` is no length), refused by its key.
fn num(v: &Value, key: &str, f: f64) -> Result<Option<f64>, String> {
    match v.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(x) => match x.as_f64().map(|n| n * f) {
            Some(n) if n.is_finite() => Ok(Some(n)),
            Some(_) => Err(format!("machine file: edm.{key} = {x} is out of range")),
            None => Err(format!("machine file: edm.{key} must be a number, not {x}")),
        },
    }
}

/// Every key of the section is one of the declared kind's, by name.
fn keys_fit(o: &serde_json::Map<String, Value>, k: Kind) -> Result<(), String> {
    let (own, other, other_kind) = match k {
        Kind::WireEdm => (&WIRE_KEYS[..], &SINKER_KEYS[..], Kind::SinkerEdm),
        _ => (&SINKER_KEYS[..], &WIRE_KEYS[..], Kind::WireEdm),
    };
    for key in o.keys() {
        if own.contains(&key.as_str()) || COMMON_KEYS.contains(&key.as_str()) {
            continue;
        }
        if other.contains(&key.as_str()) {
            return Err(format!(
                "machine file: edm.{key} is a {} key; this is a {} machine",
                other_kind.word(),
                k.word()
            ));
        }
        return Err(format!("machine file: edm.{key} is not an edm key ({}, {})", own.join(", "), COMMON_KEYS.join(", ")));
    }
    Ok(())
}

/// Read the `edm` section (`None` when the file has none). `k` is the
/// file's declared kind: only `wire_edm` / `sinker_edm` state the
/// section, each with its own keys. Lengths are scaled by `f` (the
/// file's mm factor); a garbled value refuses the file by name, never
/// reads as unstated.
///
/// # Errors
/// An `edm` section on another kind, a key that is not the kind's, a
/// non-number where a number is due (or one that is not finite once
/// scaled), a non-positive length, a taper outside (0, 90) degrees, an
/// upper guide not above the lower, an orbit kind CamCraft does not
/// generate, or a condition table entry `parse_conditions` refuses.
pub(super) fn parse_edm(v: Option<&Value>, f: f64, k: Option<Kind>) -> Result<Option<Edm>, String> {
    let Some(v) = v.filter(|v| !v.is_null()) else {
        return Ok(None);
    };
    let Some(k) = k.filter(|k| matches!(k, Kind::WireEdm | Kind::SinkerEdm)) else {
        return Err("machine file: edm: only a wire_edm or sinker_edm machine states an edm section".to_owned());
    };
    let Some(o) = v.as_object() else {
        return Err("machine file: edm must be an object".to_owned());
    };
    keys_fit(o, k)?;
    let mut e = Edm {
        lower_guide_z: num(v, "lower_guide_z", f)?,
        upper_guide_z: num(v, "upper_guide_z", f)?,
        max_taper_deg: num(v, "max_taper_deg", 1.0)?,
        max_orbit_radius: num(v, "max_orbit_radius", f)?,
        ..Edm::default()
    };
    if let (Some(lo), Some(hi)) = (e.lower_guide_z, e.upper_guide_z)
        && hi <= lo
    {
        return Err("machine file: edm.upper_guide_z must be above edm.lower_guide_z".to_owned());
    }
    if let Some(a) = e.max_taper_deg
        && !(a > 0.0 && a < 90.0)
    {
        return Err(format!("machine file: edm.max_taper_deg must be between 0 and 90 degrees, not {a}"));
    }
    if let Some(r) = e.max_orbit_radius
        && r <= 0.0
    {
        return Err(format!("machine file: edm.max_orbit_radius must be positive, not {r}"));
    }
    match v.get("wire_diameters") {
        None | Some(Value::Null) => {}
        Some(Value::Array(ds)) => {
            for d in ds {
                match d.as_f64().map(|n| n * f) {
                    Some(n) if n.is_finite() && n > 0.0 => e.wire_diameters.push(n),
                    _ => return Err(format!("machine file: edm.wire_diameters holds {d}, not a positive number")),
                }
            }
        }
        Some(x) => return Err(format!("machine file: edm.wire_diameters must be a list, not {x}")),
    }
    e.conditions = super::edm_conditions::parse_conditions(v.get("conditions"), f, k, &e.wire_diameters)?;
    match v.get("awt") {
        None | Some(Value::Null) => {}
        Some(Value::Bool(b)) => e.awt = Some(*b),
        Some(x) => return Err(format!("machine file: edm.awt must be true or false, not {x}")),
    }
    match v.get("pickup") {
        None | Some(Value::Null) => {}
        Some(Value::String(s)) if !s.trim().is_empty() => e.pickup = Some(s.clone()),
        Some(x) => return Err(format!("machine file: edm.pickup must be a word, not {x}")),
    }
    match v.get("orbits") {
        None | Some(Value::Null) => {}
        Some(Value::Array(os)) => {
            for o in os {
                e.orbits.push(orbit(o)?);
            }
        }
        Some(x) => return Err(format!("machine file: edm.orbits must be a list, not {x}")),
    }
    Ok(Some(e))
}

/// One `orbits` entry: an object of `kind` and `word`, nothing else.
fn orbit(o: &Value) -> Result<OrbitWord, String> {
    if let Some(key) = o.as_object().and_then(|m| m.keys().find(|k| !ORBIT_KEYS.contains(&k.as_str()))) {
        return Err(format!("machine file: edm.orbits: {key} is not an orbit key (kind, word)"));
    }
    let kind = o.get("kind").and_then(Value::as_str).unwrap_or_default();
    let word = o.get("word").and_then(Value::as_str).unwrap_or_default();
    if !ORBIT_KINDS.contains(&kind) {
        return Err(format!("machine file: edm orbit kind {kind:?} is not one of {}", ORBIT_KINDS.join(", ")));
    }
    if word.trim().is_empty() {
        return Err(format!("machine file: edm orbit {kind} states no control word"));
    }
    Ok(OrbitWord { kind: kind.to_owned(), word: word.to_owned() })
}

#[cfg(test)]
#[path = "edm_tests.rs"]
pub(super) mod tests;
