//! A form mill (#3369): a router-style cutter whose cutting edge is a
//! drawn outline (its meridian), with an optional pilot bearing at the
//! tip that occupies space and does not cut. The outline is written by a
//! preset ([`FormPreset`]) from the numbers of a public router bit
//! catalogue, or drawn point by point. The circle geometry is elementary
//! plane geometry (the centre of an arc through two points). Our own
//! design; no vendor source or file consulted.
//!
//! In this slice the radius never narrows upward: an undercut outline is
//! refused by name and stays with #896. No operation cuts with a form
//! mill yet; the operation doors refuse it by name.

use serde::{Deserialize, Serialize};

use crate::meridian::{Meridian, MeridianError};
use crate::spun::SpunPoint;
use crate::tools::{Tool, ToolKind, add_pt};

#[path = "form_mill_presets.rs"]
mod presets;
pub use presets::{FormPreset, NAMES};

/// Slack (mm) between the outline and the tool's diameter and flute
/// length.
pub const AGREE: f64 = 1e-6;
/// Slack (mm) on the monotonic checks: float dust only.
const EPS: f64 = 1e-9;

/// A form mill's outline: points `[z, r, arc]` from the tip up (the
/// [`SpunPoint`] law: `arc` signed, positive bulges away from the axis),
/// the pilot bearing's length at the tip (0 = no pilot), and the preset
/// that wrote the points (`None` = drawn by hand). With a preset, the
/// points are exactly what the preset writes.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FormSpec {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preset: Option<FormPreset>,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub pilot_len: f64,
    #[serde(default)]
    pub points: Vec<[f64; 3]>,
}

#[allow(clippy::trivially_copy_pass_by_ref)]
fn is_zero(v: &f64) -> bool {
    *v == 0.0
}

impl FormSpec {
    /// The outline a preset writes over a pilot of `pilot_len`.
    pub fn from_preset(preset: FormPreset, pilot_len: f64) -> Result<Self, String> {
        let pts = preset.points(pilot_len)?;
        Ok(Self { preset: Some(preset), pilot_len, points: pts.iter().map(|p| [p.z, p.r, p.arc]).collect() })
    }

    /// The stored points as [`SpunPoint`]s.
    #[must_use]
    pub fn spun(&self) -> Vec<SpunPoint> {
        self.points.iter().map(|p| SpunPoint { z: p[0], r: p[1], arc: p[2] }).collect()
    }

    /// One flag for each segment: a segment that ends at or under the
    /// pilot's top is the bearing and does not cut.
    #[must_use]
    pub fn cutting(&self) -> Vec<bool> {
        let pilot = self.pilot_len;
        self.points.windows(2).map(|w| !(pilot > 0.0 && w[1][0] <= pilot + AGREE)).collect()
    }

    /// The widest radius and the top of the outline.
    #[must_use]
    pub fn extent(&self) -> (f64, f64) {
        self.points.iter().fold((0.0, 0.0), |(r, z), p| (r.max(p[1]), z.max(p[0])))
    }
}

/// Write a form mill's outline from its preset, and its diameter and
/// flute length from the outline: the form is the one source of the
/// cutter's shape (what the editor and MCP call after an edit). Any other
/// kind, and a preset whose numbers make no bit, are left as they are, so
/// [`refusal`] names the problem.
pub fn settle(t: &mut Tool) {
    if t.kind != ToolKind::FormMill {
        return;
    }
    let Some(form) = t.form.as_mut() else { return };
    if let Some(preset) = form.preset
        && let Ok(f) = FormSpec::from_preset(preset, form.pilot_len)
    {
        *form = f;
    }
    if form.points.len() >= 2 {
        let (r, z) = form.extent();
        t.diameter = 2.0 * r;
        t.flute_len = z;
    }
}

impl Tool {
    /// [`settle`] as a method, for the layers above `app` that reach the
    /// tool through its re-export.
    pub fn settle_form(&mut self) {
        settle(self);
    }
}

/// Fill the points of a preset form that states none (a catalogue entry
/// gives only the preset). Unlike [`settle`], the stated diameter and
/// flute length stay, so [`refusal`] checks them.
pub fn fill(t: &mut Tool) {
    if let Some(form) = t.form.as_mut()
        && form.points.is_empty()
        && let Some(preset) = form.preset
        && let Ok(f) = FormSpec::from_preset(preset, form.pilot_len)
    {
        *form = f;
    }
}

/// Why this tool's form is refused, by name: a form on another kind, a
/// form mill without one, preset numbers that make no bit, points that
/// disagree with their preset, an outline that is no meridian, an
/// undercut, a pilot that ends inside a segment, or a diameter or flute
/// length that is not the outline's. `None` when sound.
#[must_use]
pub fn refusal(t: &Tool) -> Option<String> {
    let Some(form) = t.form.as_ref() else {
        return (t.kind == ToolKind::FormMill).then(|| "a form mill needs a form outline: a preset or drawn points".to_owned());
    };
    if t.kind != ToolKind::FormMill {
        return Some(format!("only a form mill has a form outline; this {} states one", t.kind));
    }
    if !(form.pilot_len.is_finite() && form.pilot_len >= 0.0) {
        return Some(format!("a form mill's pilot length {} mm must be a finite number of 0 or more", form.pilot_len));
    }
    if let Some(preset) = form.preset {
        let want = match preset.points(form.pilot_len) {
            Ok(p) => p,
            Err(why) => return Some(why),
        };
        let near = |a: f64, b: f64| (a - b).abs() <= AGREE;
        let have = form.spun();
        let same = want.len() == have.len()
            && want.iter().zip(&have).all(|(a, b)| near(a.z, b.z) && near(a.r, b.r) && near(a.arc, b.arc));
        if !same {
            return Some(format!("the form outline is not the one its {} numbers write", preset.name()));
        }
    }
    if form.points.len() > crate::spun::MAX_POINTS {
        return Some(format!("a form outline has at most {} points, not {}", crate::spun::MAX_POINTS, form.points.len()));
    }
    let pts = form.spun();
    if pts.first().is_none_or(|p| p.z != 0.0 || p.r != 0.0) {
        return Some("a form outline starts on the axis at the tip, at z 0 and r 0".to_owned());
    }
    if let Err(e) = Meridian::new(pts.clone(), form.cutting()) {
        return Some(format!("the form outline is refused: {e}"));
    }
    if let Some(i) = undercut_at(&pts) {
        return Some(format!(
            "the form outline narrows upward from point {i}: an undercut form is not supported yet (#896)"
        ));
    }
    if form.pilot_len > 0.0 {
        if !pts.iter().any(|p| (p.z - form.pilot_len).abs() <= AGREE) {
            return Some(format!("the pilot length {} mm must end at a point of the form outline", form.pilot_len));
        }
        if !form.cutting().contains(&true) {
            return Some("a form outline needs a cutting edge above its pilot".to_owned());
        }
    }
    let (r, z) = form.extent();
    if (t.diameter - 2.0 * r).abs() > AGREE || (t.flute_len - z).abs() > AGREE {
        return Some(format!(
            "a form mill's diameter and flute length are its outline's: {:.4} mm and {z:.4} mm, not {} mm and {} mm",
            2.0 * r,
            t.diameter,
            t.flute_len
        ));
    }
    None
}

/// The first point (1-based) where the outline's radius starts to fall
/// as z climbs: a line that narrows, a convex arc past its widest point,
/// a concave arc that starts before its narrowest.
fn undercut_at(pts: &[SpunPoint]) -> Option<usize> {
    pts.windows(2).position(|w| !rises(w[0], w[1])).map(|i| i + 1)
}

fn rises(a: SpunPoint, b: SpunPoint) -> bool {
    if a.arc == 0.0 {
        return b.r >= a.r - EPS;
    }
    let (dz, dr) = (b.z - a.z, b.r - a.r);
    let len = dz.hypot(dr);
    let rad = a.arc.abs();
    if !(len > 0.0 && rad.is_finite()) {
        return false;
    }
    // The centre of the minor arc: behind the chord for a convex arc, in
    // front of it for a concave one (the spun profile's law).
    let back = (rad * rad - len * len * 0.25).max(0.0).sqrt();
    let s = a.arc.signum();
    let cz = a.z + dz * 0.5 + s * back * dr / len;
    let tol = 1e-7 * rad.max(1.0);
    if s > 0.0 { b.z <= cz + tol } else { a.z >= cz - tol }
}

/// The cutting meridian of a sound form mill: its outline with the pilot
/// flagged non-cutting. Refused by name when [`refusal`] refuses the tool.
pub(crate) fn meridian(t: &Tool) -> Result<Meridian, MeridianError> {
    if let Some(why) = refusal(t) {
        return Err(MeridianError::Unsound(why));
    }
    let form = t.form.as_ref().ok_or_else(|| MeridianError::Unsound("a form mill needs a form outline".into()))?;
    Meridian::new(form.spun(), form.cutting())
}

/// Append the form outline to a side-view polyline, each arc flattened
/// so that it never falls inside the cutter (the spun profile's law).
/// `false` when the tool states no outline to draw.
pub(crate) fn outline(t: &Tool, p: &mut Vec<(f64, f64)>) -> bool {
    let pts = t.form.as_ref().map(FormSpec::spun).unwrap_or_default();
    if pts.len() < 2 {
        return false;
    }
    for w in pts.windows(2) {
        for (z, r) in crate::spun::flatten(w[0], w[1]) {
            add_pt(p, z, r);
        }
    }
    true
}

#[cfg(test)]
#[path = "form_mill_tests.rs"]
mod tests;
