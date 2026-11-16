//! Thread-milling pass control (#988): the radial pass schedule (equal or constant chip area), a finishing pass of its own depth, spring passes, a fit allowance on the thread depth, and the spindle direction and cutter hand the climb sense follows.
//!
//! Provenance: public machine-tool handbooks. Peter Smid, "CNC
//! Programming Handbook", thread cutting chapter: the depth of each
//! threading pass decreases so that every pass removes about the same
//! chip area, the cumulative depth after pass `i` of `n` being
//! `D x sqrt(i / n)` (the first pass `D / sqrt(n)`); a final spring
//! pass repeats the last depth to take the deflection out. Machinery's
//! Handbook, "Thread milling" and "Thread cutting": a finishing pass
//! lighter than the roughing passes; climb direction follows the
//! cutter's rotation; a right-hand-cut tool cuts only under a clockwise
//! (M3) spindle and a left-hand-cut tool only under M4. The area law is
//! elementary geometry: the cross-section of a V form grows with the
//! square of its depth. ISO 965-1 for the fit (the pitch-diameter
//! tolerance is a small fraction of the thread depth). Written
//! clean-room from those statements; no CAM product consulted.
//!
//! Every default is the #894 ride: equal passes, no finish or spring
//! pass, no allowance, M3 with a right-hand-cut mill.

/// How the radial passes divide the roughing depth.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PassSchedule {
    /// Equal radial steps of at most `infeed` (#894).
    #[default]
    Equal,
    /// Decreasing steps of constant chip area: the first is at most
    /// `infeed`, the cumulative depth after pass `i` of `n` is
    /// `D x sqrt(i / n)`.
    ConstantArea,
}

/// The pass controls of one thread-milling operation.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ThreadPasses {
    pub schedule: PassSchedule,
    /// Radial depth of a finishing pass of its own after the roughing
    /// passes, mm; 0 = none (the last roughing pass finishes).
    pub finish: f64,
    /// Spring passes: the final radius ridden again, this many times.
    pub spring: u32,
    /// Radial depth left uncut short of the standard thread depth, mm
    /// (a fit adjustment: the last pass lands on depth - allowance).
    pub allowance: f64,
    /// The spindle runs counter-clockwise seen from above (M4).
    pub spindle_reverse: bool,
    /// The mill is left-hand cut (its teeth cut turning M4).
    pub cutter_left: bool,
}

/// At most this many spring passes: more polishes nothing a fourth did not.
pub const MAX_SPRING_PASSES: u32 = 4;
/// The allowance may leave at most this fraction of the thread depth.
pub const MAX_ALLOWANCE_PER_DEPTH: f64 = 0.25;
/// A constant-area schedule may ask at most this many roughing passes.
pub const MAX_TAPERED_PASSES: u32 = 64;

impl ThreadPasses {
    /// Why these controls cannot cut a thread of radial `depth` with a
    /// roughing step of `infeed`, by name; `None` when they can. With
    /// `depth` unknown (an unresolved pitch) only the depth-free laws
    /// are judged.
    #[must_use]
    pub fn refusal(&self, depth: Option<f64>, infeed: f64) -> Option<String> {
        if !(self.finish.is_finite() && self.allowance.is_finite()) {
            return Some("the thread's finishing depth and allowance must be finite".into());
        }
        if self.spindle_reverse != self.cutter_left {
            let (cutter, spindle) =
                if self.cutter_left { ("left-hand-cut", "M3 (clockwise)") } else { ("right-hand-cut", "M4 (counter-clockwise)") };
            return Some(format!(
                "a {cutter} thread mill under an {spindle} spindle turns its teeth backwards - it rubs and cannot cut; state the spindle the cutter's hand needs"
            ));
        }
        if self.spring > MAX_SPRING_PASSES {
            return Some(format!(
                "{} spring passes asked - at most {MAX_SPRING_PASSES} repeat the final radius",
                self.spring
            ));
        }
        if self.finish < 0.0 {
            return Some("the finishing pass depth cannot be negative".into());
        }
        if self.allowance < 0.0 {
            return Some(format!(
                "a negative allowance of {} mm would cut past the thread's standard depth - past the major diameter of a hole, the design root of a boss",
                self.allowance
            ));
        }
        let h = depth?;
        let cap = h * MAX_ALLOWANCE_PER_DEPTH;
        if self.allowance > cap + 1e-12 {
            return Some(format!(
                "an allowance of {} mm leaves more than a quarter of the {h} mm thread depth ({cap} mm) uncut - the thread would not gauge",
                self.allowance
            ));
        }
        let full = h - self.allowance;
        if self.finish > 0.0 && self.finish >= full - 1e-12 {
            return Some(format!(
                "a finishing pass {} mm deep leaves nothing to rough of the {full} mm the thread is cut to",
                self.finish
            ));
        }
        if self.schedule == PassSchedule::ConstantArea && infeed > 0.0 {
            let rough = full - self.finish;
            let n = (rough / infeed).powi(2);
            if !(n.is_finite() && n <= f64::from(MAX_TAPERED_PASSES) + 1e-9) {
                return Some(format!(
                    "constant-chip-area passes from a first pass of {infeed} mm need more than {MAX_TAPERED_PASSES} passes to reach {rough} mm - raise the infeed"
                ));
            }
        }
        None
    }

    /// The cumulative radial crest depth of every pass, in order, for a
    /// thread `depth` deep and a roughing step of at most `infeed`. The
    /// last roughing pass, the finishing pass and every spring pass
    /// land on `depth - allowance` exactly. Call after [`Self::refusal`].
    #[must_use]
    pub fn depths(&self, depth: f64, infeed: f64) -> Vec<f64> {
        let full = depth - self.allowance;
        let rough = full - self.finish;
        let mut out: Vec<f64> = match self.schedule {
            PassSchedule::Equal => {
                let n = ((rough / infeed) - 1e-9).ceil().clamp(1.0, 4096.0) as u32;
                (1..=n).map(|i| rough * f64::from(i) / f64::from(n)).collect()
            }
            PassSchedule::ConstantArea => {
                let n = ((rough / infeed).powi(2) - 1e-9).ceil().clamp(1.0, f64::from(MAX_TAPERED_PASSES)) as u32;
                (1..=n).map(|i| rough * (f64::from(i) / f64::from(n)).sqrt()).collect()
            }
        };
        if self.finish > 0.0 {
            out.push(full);
        }
        for _ in 0..self.spring {
            out.push(full);
        }
        out
    }
}
