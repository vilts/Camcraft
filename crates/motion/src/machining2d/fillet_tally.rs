//! The fillet law's books (M11.1 B/C): each corner verdict counted by
//! reason and spoken as status-line notes, so every refusal is named and
//! every rounding states the corner rest it leaves. Provenance: ours,
//! the reporting half of `fillet.rs` / `fillet_arc.rs` (tangent circle
//! to two lines, to a line and a circle, to two circles).

use crate::geo2d::Point2;

use super::blend::MIN_BLEND_RADIUS;

/// One joint's verdict: `Round(t1, t2, center, ccw, rest)`, no corner
/// at all, or the refusal bucket it lands in.
pub(super) enum Verdict {
    Round(Point2, Point2, Point2, bool, f64),
    Straight,
    Curved,
    Reversal,
    TowardPart,
    Tiny,
    Blocked,
}

/// What a fillet pass did, counted by reason. Every corner is in
/// exactly one of the buckets after `corners`.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct FilletTally {
    pub corners: usize,
    pub rounded: usize,
    /// Turned toward the part: a fillet would cut into it.
    pub toward_part: usize,
    /// A full reversal.
    pub reversal: usize,
    /// Leg room or tool radius gave less than the tolerance/resolution.
    pub tiny: usize,
    /// Live geometry refused every radius tried.
    pub blocked: usize,
    /// A neighbour is an arc and no tangent fillet fits it (too short a
    /// sweep or leg for the radius, too tight a curve, a cusp).
    pub curved: usize,
    /// A closed walk's seam: the corner where it starts and ends (the
    /// plunge, the lead join) - rounding it would move the entry.
    pub seam: usize,
    /// Worst corner rest left against the sharp path, mm (the fillet's
    /// bisector offset `e`).
    pub max_rest: f64,
}

impl FilletTally {
    /// Book one joint's verdict; the fillet to emit, if it rounds.
    pub(super) fn count(&mut self, v: Verdict) -> Option<(Point2, Point2, Point2, bool)> {
        let bucket = match v {
            Verdict::Straight => return None,
            Verdict::Round(t1, t2, c, ccw, rest) => {
                self.corners += 1;
                self.rounded += 1;
                self.max_rest = self.max_rest.max(rest);
                return Some((t1, t2, c, ccw));
            }
            Verdict::Curved => &mut self.curved,
            Verdict::Reversal => &mut self.reversal,
            Verdict::TowardPart => &mut self.toward_part,
            Verdict::Tiny => &mut self.tiny,
            Verdict::Blocked => &mut self.blocked,
        };
        *bucket += 1;
        self.corners += 1;
        None
    }

    pub fn add(&mut self, o: FilletTally) {
        self.corners += o.corners;
        self.rounded += o.rounded;
        self.toward_part += o.toward_part;
        self.reversal += o.reversal;
        self.tiny += o.tiny;
        self.blocked += o.blocked;
        self.curved += o.curved;
        self.seam += o.seam;
        self.max_rest = self.max_rest.max(o.max_rest);
    }

    /// The status-line words: what rounded (with its rest) and every
    /// refusal by name. Empty when no corner was seen.
    pub fn notes(&self, what: &str, floor: f64) -> Vec<String> {
        let n = self.corners;
        let mut v = Vec::new();
        if self.rounded > 0 {
            v.push(format!(
                "turn blends: {} of {n} {what} corner(s) rounded - up to {:.4} mm of corner rest left against the sharp path, for the stock sim to measure",
                self.rounded, self.max_rest
            ));
        }
        let mut say = |k: usize, why: &str| {
            if k > 0 {
                v.push(format!("turn blends: {k} of {n} {what} corner(s) kept sharp - {why}"));
            }
        };
        say(self.toward_part, "the turn is toward the part, a fillet there would cut into it");
        say(self.reversal, "a full reversal has no tangent fillet");
        say(
            self.tiny,
            &format!("the legs leave room for no arc of at least {floor:.3} mm (tolerance / arc resolution)"),
        );
        say(self.blocked, "live geometry blocks every arc down to the resolution");
        say(self.curved, "a neighbour is an arc and no tangent fillet fits it (its sweep, its leg or its curve leave no room)");
        say(self.seam, "the walk starts and ends there; rounding it would move the entry");
        v
    }
}

/// The corner-arc radius a pass walk asks of `fillet_corners` (#1733):
/// the user's `radius` or, with none, the tool radius (every file before
/// the option). `Err` names the door: the radius must be finite, at
/// least [`MIN_BLEND_RADIUS`] and at most the tool radius - past it the
/// arc leaves more corner rest than the passes are spaced to clear.
pub fn corner_arc_radius(radius: Option<f64>, tool_diameter: f64) -> Result<f64, String> {
    let tool_r = tool_diameter * 0.5;
    let Some(rho) = radius else { return Ok(tool_r) };
    if rho.is_finite() && rho >= MIN_BLEND_RADIUS && rho <= tool_r {
        Ok(rho)
    } else {
        Err(format!(
            "corner arc radius must be between {MIN_BLEND_RADIUS} mm and the tool radius ({tool_r:.3} mm)"
        ))
    }
}

/// The radius floor a fillet must clear for tolerance `tol`.
pub fn fillet_floor(tol: f64) -> f64 {
    if tol.is_finite() && tol > MIN_BLEND_RADIUS {
        tol
    } else {
        MIN_BLEND_RADIUS
    }
}
