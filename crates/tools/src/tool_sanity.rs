//! Issue #1186: the tool sanity law, said by name - the first rule a tool's numbers break, in plain words, so a refusal (a project load, a tool command) can name the field instead of printing the whole tool; our own wording of our own law (`Tool::is_sane` is this function's `is_none`), no vendor material consulted.
//!
//! Every rule the law ever held is here, in the same order, and
//! `is_sane` reads only this: the words and the verdict cannot drift.

use crate::tools::{Tool, ToolKind};
use crate::turning_data::{MAX_FEED_PER_REV, MAX_RPM_CLAMP, MAX_VC, TurningData};

impl Tool {
    /// The first rule this tool's numbers break, in plain words (e.g.
    /// "corner radius 4 mm is more than half the 6 mm diameter"), or
    /// `None` when the tool is sane: finite, positive where it must be,
    /// and geometrically consistent for its kind.
    #[must_use]
    pub fn why_not_sane(&self) -> Option<String> {
        let named = [
            ("diameter", self.diameter),
            ("corner radius", self.corner_r),
            ("tip angle", self.tip_angle),
            ("pitch", self.pitch),
            ("nose radius", self.nose_radius),
            ("width", self.width),
            ("flute length", self.flute_len),
            ("shank diameter", self.shank_d),
            ("shank length", self.shank_len),
            ("holder diameter", self.holder_d),
            ("holder length", self.holder_len),
            ("spindle speed", self.rpm),
            ("feed", self.feed),
        ];
        if let Some((name, v)) = named.iter().find(|(_, v)| !v.is_finite()) {
            return Some(format!("{name} {v} is not a finite number"));
        }
        let d = self.diameter;
        if !(d > 0.0 && d <= 500.0) {
            return Some(format!("diameter {d} mm is outside 0..500 mm (it must be over 0)"));
        }
        if self.corner_r < 0.0 {
            return Some(format!("corner radius {} mm is negative", self.corner_r));
        }
        if self.corner_r > d * 0.5 {
            return Some(format!("corner radius {} mm is more than half the {d} mm diameter", self.corner_r));
        }
        if self.tip_angle >= 180.0 {
            return Some(format!("tip angle {} degrees must be under 180", self.tip_angle));
        }
        let lengths = [
            ("pitch", self.pitch),
            ("nose radius", self.nose_radius),
            ("width", self.width),
            ("shank diameter", self.shank_d),
            ("shank length", self.shank_len),
            ("holder diameter", self.holder_d),
            ("holder length", self.holder_len),
        ];
        if let Some((name, v)) = lengths.iter().find(|(_, v)| *v < 0.0) {
            return Some(format!("{name} {v} mm is negative"));
        }
        if self.flute_len <= 0.0 {
            return Some(format!("flute length {} mm must be over 0", self.flute_len));
        }
        // A torch turns no spindle (#670): its rpm is 0 by law, and
        // every spinning kind states a positive one.
        if self.kind == ToolKind::Torch {
            if self.rpm != 0.0 {
                return Some(format!("a plasma torch turns no spindle: its spindle speed must be 0, not {} rpm", self.rpm));
            }
        } else if self.rpm <= 0.0 {
            return Some(format!("spindle speed {} rpm must be over 0", self.rpm));
        }
        if self.feed <= 0.0 {
            return Some(format!("feed {} mm/min must be over 0", self.feed));
        }
        if let Some(z) = self.flutes.filter(|z| !(1..=32).contains(z)) {
            return Some(format!("flute count {z} is outside 1..32"));
        }
        if let Some(q) = self.orientation.filter(|q| !crate::orientation::is_valid(*q)) {
            return Some(format!("lathe orientation code {q} is outside 1..9"));
        }
        // #891: stated turning data is sane, and the constant rpm the
        // tool states never runs over its own max-rpm clamp.
        if let Some(t) = &self.turning {
            if let Some(why) = t.why_not_sane() {
                return Some(why);
            }
            if let Some(m) = t.max_rpm.filter(|m| self.rpm > *m) {
                return Some(format!("spindle speed {} rpm is over the tool's turning max rpm {m}", self.rpm));
            }
        }
        self.kind_rule()
    }

    /// The kind-scoped rules: each kind's own fields (see the notes on
    /// [`ToolKind`]); an inert field may carry any sane number.
    fn kind_rule(&self) -> Option<String> {
        let (d, a, w, r) = (self.diameter, self.tip_angle, self.width, self.nose_radius);
        match self.kind {
            // A point that cannot self-center is not a drill, and a tap
            // with no thread pitch has no feed the cycle could derive.
            ToolKind::Drill if a <= 60.0 => Some(format!("a drill's tip angle {a} degrees must be over 60")),
            ToolKind::Tap if self.pitch <= 0.0 => Some(format!("a tap's thread pitch {} mm must be over 0", self.pitch)),
            // An insert with no nose is a mathematical point, and a
            // nose wider than half the shank is no insert body's.
            ToolKind::Turning if !(r > 0.0 && r <= d * 0.5) => {
                Some(format!("a turning insert's nose radius {r} mm must be over 0 and at most half the {d} mm diameter"))
            }
            // A blade with no width is a plane; one as wide as its body
            // is no edge a holder grounds.
            ToolKind::Groove if !(w > 0.0 && w < d) => {
                Some(format!("a grooving blade's width {w} mm must be over 0 and under the {d} mm diameter"))
            }
            // A thread V needs an opening and a width under the body.
            ToolKind::Thread if !(w > 0.0 && w < d) => {
                Some(format!("a threading insert's width {w} mm must be over 0 and under the {d} mm diameter"))
            }
            ToolKind::Thread if !(a > 0.0 && a < 180.0) => {
                Some(format!("a threading insert's included angle {a} degrees must be over 0 and under 180"))
            }
            // A gauge needs a crown, and a band no wider than its arc.
            ToolKind::Form if !(w > 0.0 && w < d) => {
                Some(format!("a form gauge's width {w} mm must be over 0 and under the {d} mm diameter"))
            }
            ToolKind::Form if r <= 0.0 => Some(format!("a form gauge's crown radius {r} mm must be over 0")),
            ToolKind::Form if w > 2.0 * r => {
                Some(format!("a form gauge's width {w} mm is more than twice its {r} mm crown radius"))
            }
            // A cone that opens no angle is a cylinder.
            ToolKind::Taper if a <= 0.0 => Some(format!("a chamfer mill's cone angle {a} degrees must be over 0")),
            // A probe tip is a sphere, on a stylus at least its ball long.
            ToolKind::Probe if (self.corner_r - d * 0.5).abs() >= 1e-9 => Some(format!(
                "a probe's ball radius (corner radius) {} mm must be half its {d} mm ball diameter",
                self.corner_r
            )),
            ToolKind::Probe if self.flute_len < d => {
                Some(format!("a probe's stylus length {} mm is shorter than its {d} mm ball", self.flute_len))
            }
            // A torch's kerf is a cut width, never a corner (#670).
            ToolKind::Torch if self.corner_r != 0.0 => {
                Some(format!("a plasma torch carries no corner radius, not {} mm", self.corner_r))
            }
            _ => None,
        }
    }
}

impl TurningData {
    /// The first bound this turning data breaks, in plain words, or
    /// `None` when it is sane (`is_sane` is this function's `is_none`).
    #[must_use]
    pub fn why_not_sane(&self) -> Option<String> {
        if !(self.vc.is_finite() && self.vc > 0.0 && self.vc <= MAX_VC) {
            return Some(format!("turning surface speed {} m/min is outside 0..{MAX_VC} m/min (it must be over 0)", self.vc));
        }
        if !(self.feed_per_rev.is_finite() && self.feed_per_rev > 0.0 && self.feed_per_rev <= MAX_FEED_PER_REV) {
            return Some(format!(
                "turning feed {} mm/rev is outside 0..{MAX_FEED_PER_REV} mm/rev (it must be over 0)",
                self.feed_per_rev
            ));
        }
        if let Some(m) = self.max_rpm.filter(|m| !(m.is_finite() && *m > 0.0 && *m <= MAX_RPM_CLAMP)) {
            return Some(format!("turning max rpm {m} is outside 0..{MAX_RPM_CLAMP} (it must be over 0)"));
        }
        None
    }
}

#[cfg(test)]
#[path = "tool_sanity_tests.rs"]
mod tests;
