//! The first rule an EDM `edm` section breaks, named by key (#1162):
//! `Edm::is_sane` says yes or no; an agent editing the section needs
//! to know which key. Same laws as `is_sane`, one sentence each.
//!
//! Provenance: our own wording over the checks in `edm.rs` and
//! `edm_conditions.rs` (no external source).

use super::edm::{Edm, ORBIT_KINDS};
use super::edm_conditions::{condition_refusal, id_key};

impl Edm {
    /// `"edm.<key>: <rule>"` for the first rule broken, or `None` when
    /// [`Edm::is_sane`] holds.
    #[must_use]
    pub fn refusal(&self) -> Option<String> {
        let pos = |k: &str, r: Option<f64>| match r {
            Some(x) if !(x.is_finite() && x > 0.0) => Some(format!("edm.{k}: must be a positive finite number")),
            _ => None,
        };
        for (k, r) in [("lower_guide_z", self.lower_guide_z), ("upper_guide_z", self.upper_guide_z)] {
            if r.is_some_and(|x| !x.is_finite()) {
                return Some(format!("edm.{k}: must be a finite number"));
            }
        }
        if let (Some(lo), Some(hi)) = (self.lower_guide_z, self.upper_guide_z)
            && hi <= lo
        {
            return Some("edm.upper_guide_z: must be above lower_guide_z".to_owned());
        }
        if self.max_taper_deg.is_some_and(|a| !(a.is_finite() && a > 0.0 && a < 90.0)) {
            return Some("edm.max_taper_deg: must be between 0 and 90 degrees, exclusive".to_owned());
        }
        if self.wire_diameters.iter().any(|d| !(d.is_finite() && *d > 0.0)) {
            return Some("edm.wire_diameters: every diameter must be a positive finite number".to_owned());
        }
        if let Some(r) = pos("max_orbit_radius", self.max_orbit_radius) {
            return Some(r);
        }
        for (i, o) in self.orbits.iter().enumerate() {
            if !ORBIT_KINDS.contains(&o.kind.as_str()) {
                return Some(format!("edm.orbits[{i}].kind: must be one of {}", ORBIT_KINDS.join(", ")));
            }
            if o.word.trim().is_empty() {
                return Some(format!("edm.orbits[{i}].word: must name the control's own word"));
            }
        }
        for (i, c) in self.conditions.iter().enumerate() {
            if let Some(r) = condition_refusal(c, &self.wire_diameters) {
                return Some(format!("edm.conditions[{i}].{r}"));
            }
            if self.conditions.iter().skip(i + 1).any(|d| id_key(&d.id) == id_key(&c.id)) {
                return Some(format!("edm.conditions[{i}].id: '{}' is stated twice", c.id));
            }
        }
        None
    }
}
