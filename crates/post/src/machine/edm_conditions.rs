//! The EDM condition table (#531, part of #290): `edm.conditions[]`,
//! the generator conditions the user states from their machine's
//! manual. CamCraft reads only the geometry a path needs - the spark gap
//! per side and the wire diameter (wire) or the electrode undersize per
//! side (sinker) - and names each condition by the control's own id.
//! What the wire kerf offset (wire radius + gap) and the sinker
//! undersize read is bounded here, refused by condition id, key and
//! unit, so no stated value can move a cut without a word.
//!
//! Provenance: the umbrella's design notes (#290, D4: no vendor
//! condition data ships) over public EDM terminology; the plausible wire
//! range (about 0.02-0.5 mm) from the public EDM review literature the
//! umbrella cites. No vendor manual table was copied.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::def::Kind;
use super::edm::Edm;

/// What a generator condition is for: the rough cut, a skim pass, or
/// the finish. The word is the file's; this is CamCraft's.
pub const CONDITION_ROLES: [&str; 3] = ["rough", "skim", "finish"];

/// The keys one condition may state; any other refuses by name.
pub const CONDITION_KEYS: [&str; 7] = ["id", "name", "gap", "wire_diameter", "undersize", "role", "notes"];

/// The largest wire diameter and electrode undersize per side a
/// condition may state, mm: a wire is in `[MIN_WIRE_MM, MAX_WIRE_MM]`,
/// an undersize in `(0, MAX_WIRE_MM]`. Wire EDM wire runs about
/// 0.02-0.5 mm; a "50 mm wire" is a typo the kerf offset would
/// otherwise trust.
pub const MAX_WIRE_MM: f64 = 1.0;

/// The thinnest wire a condition may state, mm: the low end of the
/// same cited range. A mm file's `0.010` is an inch value typed into
/// the wrong unit, and a kerf from it would be a few microns.
pub const MIN_WIRE_MM: f64 = 0.02;

/// The largest spark gap per side a condition may state, mm, on its
/// own: `(0, MAX_GAP_MM]`, half the largest wire, whether or not the
/// condition states a wire.
pub const MAX_GAP_MM: f64 = MAX_WIRE_MM / 2.0;

/// How close a condition's wire must be to one of the machine's stated
/// `edm.wire_diameters`, mm, after unit scaling (an inch file's 0.01 in
/// is 0.254 mm on both sides of the comparison).
pub const WIRE_MATCH_MM: f64 = 0.001;

/// One generator condition (#531, #290 D4).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Condition {
    /// The control's own selector for this condition.
    pub id: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub name: String,
    /// Spark gap per side, mm (> 0).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gap: Option<f64>,
    /// Wire: the wire diameter this condition burns with, mm.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wire_diameter: Option<f64>,
    /// Sinker: the electrode undersize per side this condition wants, mm.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub undersize: Option<f64>,
    /// One of [`CONDITION_ROLES`], or empty: not stated.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub role: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub notes: String,
}

/// A field an operation needs from a condition.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConditionField {
    Gap,
    WireDiameter,
    Undersize,
}

impl ConditionField {
    fn word(self) -> &'static str {
        match self {
            ConditionField::Gap => "gap",
            ConditionField::WireDiameter => "wire_diameter",
            ConditionField::Undersize => "undersize",
        }
    }
}

impl Edm {
    /// The condition `id` with every field in `need` stated, or the
    /// refusal that names what is missing - the door an EDM op's command
    /// calls at creation, so a missing gap is refused before any
    /// generator runs. The id matches exactly as the table writes it
    /// (the table's ids are unique even ignoring case and leading
    /// zeros, so no near-miss can answer for another row).
    ///
    /// # Errors
    /// No condition `id` on this machine, or one of `need` not stated.
    pub fn condition(&self, machine: &str, id: &str, need: &[ConditionField]) -> Result<&Condition, String> {
        let c = self
            .conditions
            .iter()
            .find(|c| c.id == id)
            .ok_or_else(|| format!("machine {machine} states no EDM condition {id:?}"))?;
        for f in need {
            let stated = match f {
                ConditionField::Gap => c.gap.is_some(),
                ConditionField::WireDiameter => c.wire_diameter.is_some(),
                ConditionField::Undersize => c.undersize.is_some(),
            };
            if !stated {
                return Err(format!("machine {machine}: EDM condition {id:?} states no {}", f.word()));
            }
        }
        Ok(c)
    }

    /// The table's law over parsed conditions (the project reader's
    /// gate): every condition [`condition_refusal`]-clean and the ids
    /// unique by [`id_key`]. The kind's own fields are [`Edm::fits`].
    pub(super) fn conditions_sane(&self) -> bool {
        self.conditions.iter().all(|c| condition_refusal(c, &self.wire_diameters).is_none())
            && self
                .conditions
                .iter()
                .enumerate()
                .all(|(i, c)| self.conditions.iter().skip(i + 1).all(|d| id_key(&d.id) != id_key(&c.id)))
    }
}

/// The key two ids collide on: case ignored, and an all-digit id read
/// as its number (`"01"` and `1` are one selector). A control that
/// tells them apart is not one CamCraft can post to unambiguously.
#[must_use]
pub(super) fn id_key(id: &str) -> String {
    if !id.is_empty() && id.bytes().all(|b| b.is_ascii_digit()) {
        let t = id.trim_start_matches('0');
        return if t.is_empty() { "0".to_owned() } else { t.to_owned() };
    }
    id.to_lowercase()
}

/// Why a condition cannot be meant (`"<key>: <rule>"`), or `None`. One
/// law for both doors: the reader calls it on each parsed entry, the
/// project gate on each stored one. `wires` is the machine's
/// `edm.wire_diameters` (empty: not stated, any wire in range passes).
pub(super) fn condition_refusal(c: &Condition, wires: &[f64]) -> Option<String> {
    if c.id.is_empty() || c.id.trim() != c.id {
        return Some("id: must be a word with no surrounding spaces".to_owned());
    }
    if let Some(g) = c.gap
        && !(g.is_finite() && g > 0.0)
    {
        return Some(format!("gap: {g} mm must be above 0 mm (a spark gap of zero cannot be meant)"));
    }
    if let Some(d) = c.wire_diameter
        && !(d.is_finite() && (MIN_WIRE_MM..=MAX_WIRE_MM).contains(&d))
    {
        return Some(format!("wire_diameter: {d} mm is outside [{MIN_WIRE_MM}, {MAX_WIRE_MM}] mm"));
    }
    if let Some(u) = c.undersize
        && !(u.is_finite() && u > 0.0 && u <= MAX_WIRE_MM)
    {
        return Some(format!("undersize: {u} mm is outside (0, {MAX_WIRE_MM}] mm"));
    }
    if let (Some(g), Some(d)) = (c.gap, c.wire_diameter)
        && g >= d / 2.0
    {
        return Some(format!("gap: {g} mm must be smaller than the wire's radius ({} mm)", d / 2.0));
    }
    if let Some(g) = c.gap
        && g > MAX_GAP_MM
    {
        return Some(format!("gap: {g} mm is outside (0, {MAX_GAP_MM}] mm"));
    }
    if let (Some(g), Some(u)) = (c.gap, c.undersize)
        && g > u
    {
        return Some(format!(
            "gap: {g} mm is larger than the undersize ({u} mm) - an electrode undersized less than its spark gap burns past the finished wall"
        ));
    }
    if let Some(d) = c.wire_diameter
        && !wires.is_empty()
        && !wires.iter().any(|w| (w - d).abs() <= WIRE_MATCH_MM)
    {
        let list: Vec<String> = wires.iter().map(|w| format!("{w}")).collect();
        return Some(format!(
            "wire_diameter: {d} mm is not one of the machine's edm.wire_diameters ({} mm)",
            list.join(", ")
        ));
    }
    if !(c.role.is_empty() || CONDITION_ROLES.contains(&c.role.as_str())) {
        return Some(format!("role: {:?} is not one of {}", c.role, CONDITION_ROLES.join(", ")));
    }
    None
}

/// `edm.conditions[]`: each entry an object of [`CONDITION_KEYS`] with a
/// required, unique id; lengths scale with the file's unit (`f`) and
/// are judged in mm after the scaling; `k` (a wire or sinker kind)
/// decides which of `wire_diameter` / `undersize` the entry may state;
/// `wires` is the machine's stated wire list. A garbled entry refuses
/// the file by its id and key.
///
/// # Errors
/// Any rule above broken, named.
pub(super) fn parse_conditions(v: Option<&Value>, f: f64, k: Kind, wires: &[f64]) -> Result<Vec<Condition>, String> {
    let list = match v {
        None | Some(Value::Null) => return Ok(Vec::new()),
        Some(Value::Array(a)) => a,
        Some(x) => return Err(format!("machine file: edm.conditions must be a list, not {x}")),
    };
    let mut out: Vec<Condition> = Vec::new();
    for c in list {
        let Some(o) = c.as_object() else {
            return Err(format!("machine file: edm.conditions holds {c}, not a condition object"));
        };
        let id = parse_id(c)?;
        if let Some(d) = out.iter().find(|d| id_key(&d.id) == id_key(&id)) {
            return Err(if d.id == id {
                format!("machine file: edm condition {id:?} is stated twice")
            } else {
                format!("machine file: edm condition {id:?} and {:?} are one selector (ids compare ignoring case and leading zeros)", d.id)
            });
        }
        let at = |rule: String| format!("machine file: edm condition {id:?} {rule}");
        if let Some(key) = o.keys().find(|key| !CONDITION_KEYS.contains(&key.as_str())) {
            return Err(at(format!("{key} is not a condition key ({})", CONDITION_KEYS.join(", "))));
        }
        let (own, other) = match k {
            Kind::WireEdm => ("wire_diameter", "undersize"),
            _ => ("undersize", "wire_diameter"),
        };
        if o.get(other).is_some_and(|x| !x.is_null()) {
            let field_kind = if k == Kind::WireEdm { "a sinker" } else { "a wire" };
            return Err(at(format!("{other} is {field_kind} field; this is a {} machine", k.word())));
        }
        let word = |key: &str| -> Result<String, String> {
            match c.get(key) {
                None | Some(Value::Null) => Ok(String::new()),
                Some(Value::String(s)) => Ok(s.clone()),
                Some(x) => Err(at(format!("{key} must be a word, not {x}"))),
            }
        };
        let num = |key: &str| -> Result<Option<f64>, String> {
            match c.get(key) {
                None | Some(Value::Null) => Ok(None),
                Some(x) => match x.as_f64().map(|n| n * f) {
                    Some(n) if n.is_finite() => Ok(Some(n)),
                    Some(_) => Err(at(format!("{key} = {x} is out of range once read as mm"))),
                    None => Err(at(format!("{key} must be a number, not {x}"))),
                },
            }
        };
        let cond = Condition {
            name: word("name")?,
            gap: num("gap")?,
            wire_diameter: if own == "wire_diameter" { num("wire_diameter")? } else { None },
            undersize: if own == "undersize" { num("undersize")? } else { None },
            role: word("role")?,
            notes: word("notes")?,
            id: id.clone(),
        };
        if let Some(rule) = condition_refusal(&cond, wires) {
            return Err(at(rule));
        }
        out.push(cond);
    }
    Ok(out)
}

/// A condition's id: a word with no surrounding spaces, or a whole
/// non-negative number (read as its digits). `1.0` refuses: one
/// selector, one spelling.
fn parse_id(c: &Value) -> Result<String, String> {
    match c.get("id") {
        Some(Value::String(s)) if s.trim().is_empty() => Err(format!("machine file: an edm condition states no id: {c}")),
        Some(Value::String(s)) if s.trim() != s => {
            Err(format!("machine file: edm condition id {s:?} has surrounding spaces - write it as the control selects it"))
        }
        Some(Value::String(s)) => Ok(s.clone()),
        Some(Value::Number(n)) => match n.as_u64() {
            Some(u) => Ok(u.to_string()),
            None => Err(format!(
                "machine file: edm condition id {n} is not a whole number - write a numeric selector as an integer"
            )),
        },
        _ => Err(format!("machine file: an edm condition states no id: {c}")),
    }
}

#[cfg(test)]
#[path = "edm_conditions_tests.rs"]
mod tests;
