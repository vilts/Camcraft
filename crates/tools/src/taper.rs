//! The tip of a tapered mill (#3368, the specification in the research
//! of #895): its form (sharp, flat or ball), the sanity rules of the tip
//! numbers, and the tip that [`Tool::profile`] and [`Tool::meridian`]
//! draw - elementary plane geometry (a circle tangent to a line), on the
//! tapered end mill and chamfer mill forms of the machine-tool
//! handbooks. Our own design; no vendor source or file consulted.
//!
//! The numbers. `diameter` stays the largest cutting diameter and
//! `tip_angle` the included cone angle; half of it is `theta`. A flat tip
//! states `tip_diameter`, the cone's width extended down to the tip face
//! (`r0` is half of it), and may round its corner by `corner_r`. A ball
//! tip is a sphere of radius `corner_r` tangent to the cone. A sharp tip
//! (the default) is the cone's point: it states no tip diameter, and its
//! `corner_r` stays inert, as before this module.
//!
//! No generator, simulation or collision law reads a flat or ball tip
//! yet: the operation doors refuse such a tool by name
//! ([`is_unread_tip`]), so no toolpath reads it as a sharp cone.

use serde::{Deserialize, Serialize};

use crate::spun::SpunPoint;
use crate::tools::{Tool, ToolKind, add_pt};

/// The tip form of a tapered mill (`ToolKind::Taper`). Sharp on every
/// other kind.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaperTip {
    /// The cone's point: a V-bit or a plain chamfer mill.
    #[default]
    Sharp,
    /// A flat face of `tip_diameter` (a flat-tip chamfer mill or a
    /// tapered end mill), its corner rounded by `corner_r` when stated.
    Flat,
    /// A ball of radius `corner_r` tangent to the cone (a tapered ball
    /// nose or a radiused V-bit).
    Ball,
}

impl TaperTip {
    /// Every form, in the order the editor lists them.
    pub const ALL: [TaperTip; 3] = [TaperTip::Sharp, TaperTip::Flat, TaperTip::Ball];

    /// `skip_serializing_if` takes a reference.
    #[must_use]
    pub fn is_sharp(&self) -> bool {
        *self == TaperTip::Sharp
    }

    /// The word the files and MCP use.
    #[must_use]
    pub const fn word(self) -> &'static str {
        match self {
            TaperTip::Sharp => "sharp",
            TaperTip::Flat => "flat",
            TaperTip::Ball => "ball",
        }
    }

    /// The form a word names, `None` for any other word.
    #[must_use]
    pub fn from_word(w: &str) -> Option<Self> {
        TaperTip::ALL.into_iter().find(|t| t.word() == w)
    }
}

impl core::fmt::Display for TaperTip {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(self.word())
    }
}

/// The tip of a flat or ball taper, mm. The flank is the line
/// `r(z) = r0 + z * tan_half`, tangent to the corner arc at
/// `(z_t, r_t)`; the corner arc is centred at `(corner_r, r_flat)`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Tip {
    /// Radius of the flat face (0 on a ball tip).
    pub r_flat: f64,
    /// The corner or ball radius (0 on a square flat tip).
    pub corner_r: f64,
    /// Where the corner arc meets the cone: radius and height.
    pub r_t: f64,
    pub z_t: f64,
    /// The cone's radius extended down to `z = 0`.
    pub r0: f64,
    /// The tangent of half the included cone angle.
    pub tan_half: f64,
}

impl Tip {
    /// The height at which the cone reaches radius `r`.
    #[must_use]
    pub fn cone_z(&self, r: f64) -> f64 {
        (r - self.r0) / self.tan_half
    }
}

/// Is this a tapered mill whose tip no generator reads yet (a flat or
/// ball tip)? The operation doors refuse it by name.
#[must_use]
pub fn is_unread_tip(t: &Tool) -> bool {
    t.kind == ToolKind::Taper && !t.tip_form.is_sharp()
}

/// The tip of a flat or ball taper, from its numbers. `None` for a sharp
/// tip, another kind, or numbers that draw no tip (a cone angle outside
/// `(0, 180)`, a non-finite or negative radius, a corner too large for
/// its flat).
#[must_use]
pub fn tip(t: &Tool) -> Option<Tip> {
    if t.kind != ToolKind::Taper {
        return None;
    }
    let a = t.tip_angle;
    let cr = t.corner_r;
    if !(a.is_finite() && a > 0.0 && a < 180.0 && cr.is_finite() && cr >= 0.0) {
        return None;
    }
    let (s, c) = (a.to_radians() * 0.5).sin_cos();
    let tan_half = s / c;
    let g = match t.tip_form {
        TaperTip::Sharp => return None,
        TaperTip::Flat => {
            let r0 = t.tip_diameter * 0.5;
            if !(r0.is_finite() && r0 > 0.0) {
                return None;
            }
            let r_flat = r0 - cr * (1.0 - s) / c;
            Tip { r_flat, corner_r: cr, r_t: r_flat + cr * c, z_t: cr * (1.0 - s), r0, tan_half }
        }
        TaperTip::Ball => {
            if cr <= 0.0 {
                return None;
            }
            Tip { r_flat: 0.0, corner_r: cr, r_t: cr * c, z_t: cr * (1.0 - s), r0: cr * (1.0 - s) / c, tan_half }
        }
    };
    let finite = [g.r_flat, g.r_t, g.z_t, g.r0, g.tan_half].iter().all(|v| v.is_finite());
    (finite && g.r_flat >= 0.0 && g.tan_half > 0.0).then_some(g)
}

/// The first tip rule the tool breaks, in plain words, or `None`
/// (`Tool::why_not_sane` reads it). A sharp taper keeps every rule it had:
/// it states no tip diameter, and its corner radius stays inert.
#[must_use]
pub fn refusal(t: &Tool) -> Option<String> {
    let td = t.tip_diameter;
    if !td.is_finite() {
        return Some(format!("tip diameter {td} is not a finite number"));
    }
    if td < 0.0 {
        return Some(format!("tip diameter {td} mm is negative"));
    }
    if t.kind != ToolKind::Taper {
        return (!t.tip_form.is_sharp() || td != 0.0).then(|| {
            format!(
                "only a chamfer mill has a tip form and a tip diameter; this {} states a {} tip of {td} mm",
                t.kind, t.tip_form
            )
        });
    }
    let (d, cr, a) = (t.diameter, t.corner_r, t.tip_angle);
    match t.tip_form {
        TaperTip::Sharp if td != 0.0 => {
            return Some(format!(
                "a sharp chamfer mill has no tip diameter (it states {td} mm) - choose a flat tip"
            ));
        }
        TaperTip::Sharp => return None,
        TaperTip::Flat if !(td > 0.0 && td < d) => {
            return Some(format!(
                "a flat-tip chamfer mill's tip diameter {td} mm must be over 0 and under the {d} mm diameter"
            ));
        }
        TaperTip::Ball if cr <= 0.0 => {
            return Some(format!(
                "a ball-tip chamfer mill's ball radius (corner radius) {cr} mm must be over 0"
            ));
        }
        TaperTip::Ball if td != 0.0 && (td - 2.0 * cr).abs() > 1e-9 => {
            return Some(format!(
                "a ball-tip chamfer mill's tip diameter {td} mm must be 0 or twice its {cr} mm corner radius"
            ));
        }
        TaperTip::Flat | TaperTip::Ball => {}
    }
    // A cone angle outside (0, 180) is the kind rule's to name.
    if !(a > 0.0 && a < 180.0) {
        return None;
    }
    let Some(g) = tip(t) else {
        return Some(format!(
            "the {cr} mm corner radius does not fit the {td} mm flat tip of a {a} degree cone"
        ));
    };
    if g.r_t >= d * 0.5 {
        return Some(format!(
            "the {} tip meets the cone {} mm across, not under the {d} mm diameter",
            t.tip_form,
            2.0 * g.r_t
        ));
    }
    if g.z_t >= t.flute_len {
        return Some(format!(
            "the {} tip is {} mm tall, not under the {} mm flute length",
            t.tip_form, g.z_t, t.flute_len
        ));
    }
    None
}

/// Draw a flat or ball tip into a profile: the flat face, the corner or
/// ball arc (points on the arc), then the cone up to the full radius `r`
/// or the flute top `zc`. `false` when the tool has no such tip, and the
/// caller draws the sharp cone. Half-typed numbers draw short, never NaN.
pub(crate) fn outline(t: &Tool, r: f64, zc: f64, p: &mut Vec<(f64, f64)>) -> bool {
    let Some(g) = tip(t) else {
        return false;
    };
    if g.r_flat > 0.0 {
        add_pt(p, 0.0, g.r_flat.min(r));
    }
    if g.corner_r > 0.0 {
        // The arc turns from the tip face to the flank: 90 degrees less
        // half the cone angle.
        let turn = std::f64::consts::FRAC_PI_2 - g.tan_half.atan();
        let steps = 12;
        for i in 1..=steps {
            let phi = turn * f64::from(i) / f64::from(steps);
            let z = g.corner_r * (1.0 - phi.cos());
            if z > zc {
                break;
            }
            add_pt(p, z, (g.r_flat + g.corner_r * phi.sin()).min(r));
        }
    }
    let top = g.cone_z(r).min(zc);
    if top > g.z_t {
        add_pt(p, top, (g.r0 + top * g.tan_half).min(r));
    }
    true
}

/// The cutting meridian of a flat or ball taper (`Tool::meridian`): the
/// flat face, the corner or ball as one convex arc, the cone, and the
/// straight flute side to `fl`. `None` when the tool has no such tip.
pub(crate) fn meridian_points(t: &Tool, r: f64, fl: f64) -> Option<Vec<SpunPoint>> {
    let g = tip(t)?;
    let mut p = vec![SpunPoint::line(0.0, 0.0)];
    if g.r_flat > 0.0 {
        p.push(SpunPoint::line(0.0, g.r_flat));
    }
    if g.corner_r > 0.0 {
        if let Some(last) = p.last_mut() {
            last.arc = g.corner_r;
        }
        p.push(SpunPoint::line(g.z_t, g.r_t));
    }
    let top = g.cone_z(r).min(fl);
    let mut top_r = 0.0;
    if top > g.z_t {
        top_r = (g.r0 + top * g.tan_half).min(r);
        p.push(SpunPoint::line(top, top_r));
    }
    // A flute that ends on the cone steps out to the full radius at its
    // top, as `Tool::profile` draws it (#3929 review): the straight side
    // above the cone, or the radial shoulder of a short flute.
    if fl > top || r - top_r > 1e-9 {
        p.push(SpunPoint::line(fl, r));
    }
    Some(p)
}

#[cfg(test)]
#[path = "taper_tests.rs"]
mod tests;
