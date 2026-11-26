//! The form mill presets (#3369): the outline of a roundover, cove, ogee,
//! V-groove or bead router bit, written from the numbers the public router
//! bit catalogues state. Quarter arcs tangent to the pilot and the body,
//! and a ball tangent to a cone, are elementary plane geometry. Our own
//! design; no vendor source or file consulted.
//!
//! Every outline starts on the axis at the tip and climbs: the pilot
//! bearing (when `pilot_len` is over 0), then the cutting edge, then the
//! straight body to the cutting height. The radius never narrows upward.

use serde::{Deserialize, Serialize};

use crate::spun::SpunPoint;

/// A segment shorter than this (mm) is not drawn: the cutting height then
/// ends where the form ends.
const MIN_STEP: f64 = 1e-6;

/// A router bit preset with the numbers it is sold by, mm (the V-groove's
/// angle in degrees). `pilot_radius` is the bearing's radius and the
/// radius where the cut starts; `height` is the cutting height above the
/// pilot.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum FormPreset {
    /// Corner rounding: a concave quarter arc of `radius` from the pilot
    /// out to the body.
    Roundover { radius: f64, pilot_radius: f64, height: f64 },
    /// A convex quarter arc of `radius` from the pilot out to the body.
    Cove { radius: f64, pilot_radius: f64, height: f64 },
    /// A concave arc of `radius`, a flat `step` across the axis, then a
    /// convex arc of `radius2`.
    Ogee { radius: f64, radius2: f64, step: f64, pilot_radius: f64, height: f64 },
    /// A cone of `angle` (included, degrees) to `diameter`, its tip
    /// rounded by a ball of `tip_radius` (0 = sharp). No pilot.
    VGroove { angle: f64, tip_radius: f64, diameter: f64, height: f64 },
    /// A beading bit: a flat `step` out from the pilot (the quirk), then
    /// a concave quarter arc of `radius`.
    Bead { radius: f64, step: f64, pilot_radius: f64, height: f64 },
}

/// The preset names, in [`FormPreset::index`] order.
pub const NAMES: [&str; 5] = ["Roundover", "Cove", "Ogee", "V-groove", "Bead"];

impl FormPreset {
    /// The preset names, in [`FormPreset::index`] order.
    pub const NAMES: [&'static str; 5] = NAMES;

    /// The position of this preset in [`NAMES`].
    #[must_use]
    pub fn index(&self) -> usize {
        match self {
            Self::Roundover { .. } => 0,
            Self::Cove { .. } => 1,
            Self::Ogee { .. } => 2,
            Self::VGroove { .. } => 3,
            Self::Bead { .. } => 4,
        }
    }

    /// The preset's name, as the editor and the refusals say it.
    #[must_use]
    pub fn name(&self) -> &'static str {
        NAMES.get(self.index()).copied().unwrap_or("form")
    }

    /// Plausible starting numbers for the preset at `index` (a 1/2 inch
    /// router bit class, in mm); anything past the list is a roundover.
    #[must_use]
    pub fn starter(index: usize) -> Self {
        match index {
            1 => Self::Cove { radius: 6.35, pilot_radius: 6.35, height: 9.5 },
            2 => Self::Ogee { radius: 4.0, radius2: 4.0, step: 1.5, pilot_radius: 6.35, height: 12.7 },
            3 => Self::VGroove { angle: 90.0, tip_radius: 0.0, diameter: 12.7, height: 6.35 },
            4 => Self::Bead { radius: 6.35, step: 1.6, pilot_radius: 6.35, height: 9.5 },
            _ => Self::Roundover { radius: 6.35, pilot_radius: 6.35, height: 9.5 },
        }
    }

    /// The preset's numbers by label, for the editor's rows.
    #[must_use]
    pub fn numbers_mut(&mut self) -> Vec<(&'static str, &mut f64)> {
        match self {
            Self::Roundover { radius, pilot_radius, height } | Self::Cove { radius, pilot_radius, height } => {
                vec![("Radius", radius), ("Pilot radius", pilot_radius), ("Cutting height", height)]
            }
            Self::Ogee { radius, radius2, step, pilot_radius, height } => vec![
                ("Lower radius", radius),
                ("Upper radius", radius2),
                ("Step", step),
                ("Pilot radius", pilot_radius),
                ("Cutting height", height),
            ],
            Self::VGroove { angle, tip_radius, diameter, height } => vec![
                ("Included angle (deg)", angle),
                ("Tip radius", tip_radius),
                ("Cutting diameter", diameter),
                ("Cutting height", height),
            ],
            Self::Bead { radius, step, pilot_radius, height } => {
                vec![("Radius", radius), ("Step", step), ("Pilot radius", pilot_radius), ("Cutting height", height)]
            }
        }
    }

    /// The outline these numbers write over a pilot of `pilot_len`, tip
    /// up, or why the numbers make no bit (refused by name).
    pub fn points(&self, pilot_len: f64) -> Result<Vec<SpunPoint>, String> {
        let name = self.name();
        let mut nums = *self;
        for (label, v) in nums.numbers_mut() {
            if !v.is_finite() || *v < 0.0 {
                return Err(format!("a {name} form's {} {} must be a finite number of 0 or more", label.to_lowercase(), v));
            }
        }
        if !(pilot_len.is_finite() && pilot_len >= 0.0) {
            return Err(format!("a form mill's pilot length {pilot_len} mm must be a finite number of 0 or more"));
        }
        let line = SpunPoint::line;
        let over = |label: &str, v: f64| {
            (v > 0.0).then_some(()).ok_or_else(|| format!("a {name} form's {label} {v} must be over 0"))
        };
        match *self {
            Self::VGroove { angle, tip_radius, diameter, height } => {
                if pilot_len > 0.0 {
                    return Err(format!("a V-groove form has no pilot bearing; its pilot length must be 0, not {pilot_len} mm"));
                }
                if !(angle > 0.0 && angle < 180.0) {
                    return Err(format!("a V-groove form's included angle {angle} degrees must be over 0 and under 180"));
                }
                over("cutting diameter", diameter)?;
                let half = (angle * 0.5).to_radians();
                let (r, tan) = (diameter * 0.5, half.tan());
                let mut p = vec![SpunPoint { z: 0.0, r: 0.0, arc: tip_radius }];
                let tangent = (tip_radius * (1.0 - half.sin()), tip_radius * half.cos());
                if tip_radius > 0.0 {
                    if tangent.1 >= r - MIN_STEP {
                        return Err(format!("a V-groove form's tip radius {tip_radius} mm leaves no cone under its {diameter} mm cutting diameter"));
                    }
                    p.push(line(tangent.0, tangent.1));
                }
                let cone_top = tangent.0 + (r - tangent.1) / tan;
                p.push(line(cone_top, r));
                top_out(p, cone_top, height, r, name)
            }
            Self::Roundover { radius, pilot_radius, height } => {
                over("radius", radius)?;
                let mut p = foot(pilot_len, pilot_radius, name)?;
                set_arc(&mut p, -radius);
                p.push(line(pilot_len + radius, pilot_radius + radius));
                top_out(p, radius, height, pilot_radius + radius, name)
            }
            Self::Cove { radius, pilot_radius, height } => {
                over("radius", radius)?;
                let mut p = foot(pilot_len, pilot_radius, name)?;
                set_arc(&mut p, radius);
                p.push(line(pilot_len + radius, pilot_radius + radius));
                top_out(p, radius, height, pilot_radius + radius, name)
            }
            Self::Ogee { radius, radius2, step, pilot_radius, height } => {
                over("lower radius", radius)?;
                over("upper radius", radius2)?;
                let mut p = foot(pilot_len, pilot_radius, name)?;
                set_arc(&mut p, -radius);
                let (z1, r1) = (pilot_len + radius, pilot_radius + radius);
                p.push(line(z1, r1));
                if step > MIN_STEP {
                    p.push(line(z1, r1 + step));
                }
                set_arc(&mut p, radius2);
                let r2 = r1 + step + radius2;
                p.push(line(z1 + radius2, r2));
                top_out(p, radius + radius2, height, r2, name)
            }
            Self::Bead { radius, step, pilot_radius, height } => {
                over("radius", radius)?;
                over("step", step)?;
                let mut p = foot(pilot_len, pilot_radius, name)?;
                let r0 = pilot_radius + step;
                p.push(line(pilot_len, r0));
                set_arc(&mut p, -radius);
                p.push(line(pilot_len + radius, r0 + radius));
                top_out(p, radius, height, r0 + radius, name)
            }
        }
    }
}

/// The tip and the pilot: the axis, out to the pilot radius, and up the
/// bearing when it has a length.
fn foot(pilot_len: f64, pilot_radius: f64, name: &str) -> Result<Vec<SpunPoint>, String> {
    if pilot_radius <= 0.0 {
        return Err(format!("a {name} form's pilot radius {pilot_radius} mm must be over 0"));
    }
    let mut p = vec![SpunPoint::line(0.0, 0.0), SpunPoint::line(0.0, pilot_radius)];
    if pilot_len > MIN_STEP {
        p.push(SpunPoint::line(pilot_len, pilot_radius));
    }
    Ok(p)
}

/// Make the segment from the last point an arc of signed radius `arc`.
fn set_arc(p: &mut [SpunPoint], arc: f64) {
    if let Some(last) = p.last_mut() {
        last.arc = arc;
    }
}

/// The straight body from the form's top (`form` above the pilot) to the
/// cutting height, at radius `r`. A cutting height under the form is
/// refused by name.
fn top_out(mut p: Vec<SpunPoint>, form: f64, height: f64, r: f64, name: &str) -> Result<Vec<SpunPoint>, String> {
    if height < form - MIN_STEP {
        return Err(format!("a {name} form's cutting height {height} mm is under the {form:.3} mm its form needs"));
    }
    let top = p.last().map_or(0.0, |q| q.z) + (height - form);
    if height - form > MIN_STEP {
        p.push(SpunPoint::line(top, r));
    }
    Ok(p)
}
