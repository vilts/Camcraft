//! The angle-word taper dialect of a wire EDM (#1164, part of #290):
//! the `.mch` file's `edm.taper_angle` object. A machine stating it
//! tapers by an angle word (the control computes the lean from the
//! programmed contour) instead of U/V moves, and states as data what
//! its control does with that angle: which side of the direction of
//! travel a positive angle leans the wire's top toward, and how the
//! control carries the lean through a corner. Each is an enumerated
//! word with no default - an unstated one is refused at generation by
//! name, a garbled one refuses the file.
//!
//! Provenance: our own design over #290's D5 (control words are
//! machine data) and D10 (no vendor dialect: the words are invented,
//! not any control's); plane geometry for what each corner rule cuts
//! (`motion::wire_edm::taper`). No control manual consulted.

use serde::{Deserialize, Serialize};

/// Which side of the direction of travel a POSITIVE angle word leans
/// the wire's top toward.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AngleSide {
    TopLeft,
    TopRight,
}

/// What the control cuts at a corner of the programmed contour under
/// an angle word.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaperCorner {
    /// Both sides keep their plane at the angle and meet in a straight
    /// corner line: the lower corner moves along the mitre (the corner
    /// itself leans more than the angle).
    Mitre,
    /// The wire pivots about the corner point on the programmed plane,
    /// its lean held at the angle while it turns from one side's
    /// direction to the next's: a cone, the lower corner rounded.
    Cone,
}

impl TaperCorner {
    /// The rule's word, as the file, notes and refusals say it.
    #[must_use]
    pub fn word(self) -> &'static str {
        match self {
            TaperCorner::Mitre => "mitre",
            TaperCorner::Cone => "cone",
        }
    }
}

/// `edm.taper_angle`: the angle-word dialect, each rule as stated.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct TaperAngle {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub positive: Option<AngleSide>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub corner: Option<TaperCorner>,
}

impl TaperAngle {
    /// Both rules stated, or the refusal naming what is not.
    ///
    /// # Errors
    /// `positive` or `corner` unstated, named with the machine.
    pub fn complete(&self, machine: &str) -> Result<(AngleSide, TaperCorner), String> {
        match (self.positive, self.corner) {
            (Some(p), Some(c)) => Ok((p, c)),
            (None, _) => Err(format!(
                "machine {machine} tapers by angle word (edm.taper_angle) and states no edm.taper_angle.positive (top_left or top_right) - the side a positive angle leans the wire is never guessed"
            )),
            (_, None) => Err(format!(
                "machine {machine} tapers by angle word (edm.taper_angle) and states no edm.taper_angle.corner (mitre or cone) - what the control cuts at a corner is never guessed"
            )),
        }
    }
}

/// Read `edm.taper_angle` (`None` when absent).
///
/// # Errors
/// Not an object, or a word that is not one of the rule's values.
pub(super) fn parse_taper_angle(v: Option<&serde_json::Value>) -> Result<Option<TaperAngle>, String> {
    let Some(v) = v.filter(|v| !v.is_null()) else {
        return Ok(None);
    };
    if !v.is_object() {
        return Err("machine file: edm.taper_angle must be an object".to_owned());
    }
    let word = |k: &str, allowed: &str| -> Result<Option<serde_json::Value>, String> {
        match v.get(k) {
            None | Some(serde_json::Value::Null) => Ok(None),
            Some(x @ serde_json::Value::String(_)) => Ok(Some(x.clone())),
            Some(x) => Err(format!("machine file: edm.taper_angle.{k} must be one of {allowed}, not {x}")),
        }
    };
    let mut out = TaperAngle::default();
    if let Some(x) = word("positive", "top_left, top_right")? {
        out.positive = Some(
            serde_json::from_value(x.clone())
                .map_err(|_| format!("machine file: edm.taper_angle.positive must be one of top_left, top_right, not {x}"))?,
        );
    }
    if let Some(x) = word("corner", "mitre, cone")? {
        out.corner =
            Some(serde_json::from_value(x.clone()).map_err(|_| format!("machine file: edm.taper_angle.corner must be one of mitre, cone, not {x}"))?);
    }
    Ok(Some(out))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn taper_angle_words_parse_and_garbled_ones_refuse_the_file() {
        let v = serde_json::json!({ "positive": "top_left", "corner": "cone" });
        let t = parse_taper_angle(Some(&v)).ok().flatten();
        assert_eq!(t, Some(TaperAngle { positive: Some(AngleSide::TopLeft), corner: Some(TaperCorner::Cone) }));
        let partial = parse_taper_angle(Some(&serde_json::json!({ "positive": "top_right" }))).ok().flatten();
        assert!(partial.is_some_and(|t| t.complete("M").is_err_and(|e| e.contains("edm.taper_angle.corner"))));
        let bad = parse_taper_angle(Some(&serde_json::json!({ "corner": "round" })));
        assert!(bad.is_err_and(|e| e.contains("mitre, cone")));
        assert!(parse_taper_angle(None).is_ok_and(|t| t.is_none()));
    }

    /// #1164 review (PR #1218): a dialect that is not an object, and a
    /// garbled side word, refuse the file by name.
    #[test]
    fn a_taper_angle_not_an_object_or_a_garbled_side_refuses_the_file() {
        let err = parse_taper_angle(Some(&serde_json::json!("cone"))).err().unwrap_or_default();
        assert!(err.contains("edm.taper_angle must be an object"), "{err}");
        let err = parse_taper_angle(Some(&serde_json::json!({ "positive": "up", "corner": "mitre" }))).err().unwrap_or_default();
        assert!(err.contains("edm.taper_angle.positive must be one of top_left, top_right, not \"up\""), "{err}");
    }
}
