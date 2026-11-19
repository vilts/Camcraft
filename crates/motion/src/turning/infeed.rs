//! The threading infeed plan (issue #416): how the passes of a
//! single-point thread grow the groove from the crest to the root -
//! which way each pass enters (radial, along one flank, or
//! alternating flanks), how deep each pass goes (constant depth, or
//! constant chip area by the square root of the pass number with a
//! minimum per pass), the finish allowance the finishing pass takes,
//! and the spring passes at full depth after it.
//!
//! The safety law is containment: every pass's V must stand inside the
//! finished V, or a roughing pass cuts flank the finishing pass was
//! meant to leave. A pass `h` above the root, shifted `h * tan(theta)`
//! along the axis, keeps both its flanks inside the finished V exactly
//! when `theta` is no more than the insert's half-angle - so a flank
//! angle past it is refused by name, and the finishing and spring
//! passes always stand on the lattice centre itself (no shift). The
//! shift moves where a pass STARTS on the lattice, never its lead: the
//! helix phase (spindle angle at a station) is the lattice's for every
//! pass.
//!
//! Provenance: the infeed methods (radial, modified flank at a compound
//! angle a little under half the included angle, alternating flank),
//! the decreasing-depth (constant-area, first depth times the square
//! root of the pass number) schedule with a minimum depth per pass, the
//! finish allowance and spring passes, as public tooling-maker guides
//! (Sandvik Coromant "How to choose infeed method" and thread-turning
//! application tips; ISCAR threading handbook) and the public G76
//! references (GSK988TA manual G76 section, Fanuc-compatible; the
//! LinuxCNC G-code reference, G76) describe them - behaviour only. The
//! containment proof and the pass layout are our own. No vendor
//! strategy, post or output consulted.

use crate::machining2d::GenError;

use super::blade;
pub use super::thread_dips::ThreadDips;

/// The most spring passes a plan may ask for - a two-digit finishing
/// count is what the lathe cycles carry (99 finishing passes, one of
/// them the finish itself).
pub const MAX_SPRING_PASSES: u32 = 98;

/// How each pass enters the groove.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum InfeedMethod {
    /// Every pass plunges square to the axis on the lattice centre.
    #[default]
    Radial,
    /// Each deeper pass steps along the axis by its depth times
    /// tan(flank angle), in the direction the pass travels: the
    /// leading flank cuts, the trailing flank rides the cut flank.
    Flank,
    /// As `Flank`, the side flipping every roughing pass.
    Alternating,
}

/// How deep each pass goes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum DepthSchedule {
    /// Every pass `first depth` deeper than the last (the last roughing
    /// pass the remainder).
    #[default]
    ConstantDepth,
    /// Pass `n` reaches `first depth * sqrt(n)` below the crest, but at
    /// least `min_depth` deeper than the pass before it.
    ConstantArea,
}

/// The infeed plan of a thread. The first-pass depth is the operation's
/// own `infeed` number; the default plan is the radial constant-depth
/// ladder every thread before #416 was cut with.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ThreadInfeed {
    pub method: InfeedMethod,
    /// Compound angle off the radial, degrees (flank and alternating).
    pub flank_angle: f64,
    pub schedule: DepthSchedule,
    /// The smallest depth a constant-area pass may take, radial mm.
    pub min_depth: f64,
    /// Radial stock the finishing pass removes; 0 - the last roughing
    /// pass reaches the root itself.
    pub finish_allowance: f64,
    /// Extra passes at full depth after the finishing pass.
    pub spring_passes: u32,
    /// Cut every roughing depth twice before stepping deeper (issue
    /// #418, "spring at each depth"): the repeat is the very same pass,
    /// so it can only clean what the first cut left. The finishing and
    /// spring passes at the root are not repeated. Off by default -
    /// tooling makers advise against routine spring passes.
    pub repeat_each_depth: bool,
    /// In-pass chip-break dips on the roughing passes (issue #419), off
    /// by default. G32 passes on a post that declares continuous
    /// threading only.
    pub dips: ThreadDips,
}

impl Default for ThreadInfeed {
    fn default() -> Self {
        ThreadInfeed {
            method: InfeedMethod::Radial,
            // The modified-flank angle the tooling guides give for a
            // 60-degree form; read only when the method is not radial.
            flank_angle: 29.5,
            schedule: DepthSchedule::ConstantDepth,
            // The smallest infeed the guides recommend; read only by
            // the constant-area schedule.
            min_depth: 0.05,
            finish_allowance: 0.0,
            spring_passes: 0,
            repeat_each_depth: false,
            dips: ThreadDips::default(),
        }
    }
}

/// One pass of the plan: where its tip stands and how far its start
/// moves along the pass's travel from the lattice centre.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Pass {
    /// Tip radius of the pass.
    pub tip: f64,
    /// Axial offset from the lattice station, along the pass's travel
    /// (negative: behind it).
    pub shift: f64,
}

impl ThreadInfeed {
    /// Why this plan cannot cut a thread `depth` deep with a first pass
    /// of `first_depth` on an insert of `form_angle` included degrees
    /// (`None`: it can). Every refusal names its number - the door the
    /// operation commands and the kernel both ask.
    #[must_use]
    pub fn refusal(&self, depth: f64, first_depth: f64, form_angle: f64) -> Option<String> {
        let a = self.allowance_refusal(depth);
        if a.is_some() {
            return a;
        }
        if let Some(why) = self.dips.refusal(first_depth) {
            return Some(why);
        }
        if self.spring_passes > MAX_SPRING_PASSES {
            return Some(format!(
                "{} spring passes is more than the {MAX_SPRING_PASSES} a thread may repeat at full depth",
                self.spring_passes
            ));
        }
        if self.schedule == DepthSchedule::ConstantArea {
            if !(self.min_depth.is_finite() && self.min_depth > 0.0) {
                return Some(format!(
                    "a constant-area thread needs a positive minimum depth per pass, not {} - \
                     without it the passes shrink toward nothing",
                    self.min_depth
                ));
            }
            if first_depth.is_finite() && self.min_depth > first_depth {
                return Some(format!(
                    "the minimum depth per pass {:.3} is deeper than the first-pass depth {:.3} - \
                     the first pass is the deepest a constant-area thread takes",
                    self.min_depth, first_depth
                ));
            }
        }
        if self.method != InfeedMethod::Radial {
            let half = form_angle * 0.5;
            if !(self.flank_angle.is_finite() && self.flank_angle > 0.0) {
                return Some(format!(
                    "a flank infeed angle of {} degrees is not a flank - give an angle above 0 \
                     (0 is radial infeed)",
                    self.flank_angle
                ));
            }
            if !(half.is_finite() && half > 0.0) || self.flank_angle > half + 1e-9 {
                return Some(format!(
                    "a flank infeed angle of {:.2} degrees does not fit a {:.1} degree insert - past \
                     its {:.2} degree half-angle the trailing flank would cut the finished thread's \
                     flank; use at most {:.2}",
                    self.flank_angle, form_angle, half, half
                ));
            }
        }
        None
    }

    fn allowance_refusal(&self, depth: f64) -> Option<String> {
        if !(self.finish_allowance.is_finite() && self.finish_allowance >= 0.0) {
            return Some(format!(
                "a finish allowance of {} is not a depth - give 0 or more",
                self.finish_allowance
            ));
        }
        if depth.is_finite() && self.finish_allowance >= depth {
            return Some(format!(
                "a finish allowance of {:.3} leaves nothing to rough on a thread {:.3} deep - \
                 keep it under the thread depth",
                self.finish_allowance, depth
            ));
        }
        None
    }

    /// The tool-free half of [`Self::refusal`], for the parameter door
    /// that holds no tool: every number finite and in range.
    #[must_use]
    pub fn is_sane(&self) -> bool {
        self.flank_angle.is_finite()
            && self.flank_angle >= 0.0
            && self.flank_angle < 90.0
            && (self.method == InfeedMethod::Radial || self.flank_angle > 0.0)
            && self.min_depth.is_finite()
            && self.min_depth >= 0.0
            && (self.schedule == DepthSchedule::ConstantDepth || self.min_depth > 0.0)
            && self.finish_allowance.is_finite()
            && self.finish_allowance >= 0.0
            && self.spring_passes <= MAX_SPRING_PASSES
            && self.dips.is_sane()
    }

    /// The pass list from the crest `ref_top` down to the `root`, the
    /// first pass `first_depth` deep: roughing to `root +
    /// finish_allowance`, the finishing pass at the root when an
    /// allowance stands, then the spring passes at the root. With
    /// `repeat_each_depth` every pass above the root follows itself
    /// once more, identical (#418). The radial constant-depth plan with
    /// no allowance is exactly the ladder [`blade::rungs`] lays.
    pub fn passes(&self, ref_top: f64, root: f64, first_depth: f64) -> Result<Vec<Pass>, GenError> {
        let height = ref_top - root;
        if !(height > 0.0 && first_depth > 0.0 && first_depth.is_finite()) {
            return Err(GenError::Invalid("threading needs a positive thread height and first-pass depth".into()));
        }
        if let Some(why) = self.allowance_refusal(height) {
            return Err(GenError::Invalid(why));
        }
        let leave = self.finish_allowance;
        let mut tips = match self.schedule {
            DepthSchedule::ConstantDepth => blade::rungs(root, ref_top, first_depth, leave)?,
            DepthSchedule::ConstantArea => self.area_rungs(ref_top, root + leave, first_depth)?,
        };
        if leave > 0.0 {
            tips.push(root);
        }
        tips.extend(std::iter::repeat_n(root, self.spring_passes.min(MAX_SPRING_PASSES) as usize));
        let tan = match self.method {
            InfeedMethod::Radial => 0.0,
            _ => self.flank_angle.to_radians().tan(),
        };
        let mut out = Vec::with_capacity(tips.len().saturating_mul(2));
        // The alternation counts DEPTHS, not cuts: a repeated depth
        // keeps its own side, so its repeat is the same pass.
        for (j, &tip) in tips.iter().enumerate() {
            let back = (tip - root).max(0.0) * tan;
            let shift = match self.method {
                InfeedMethod::Radial => 0.0,
                InfeedMethod::Flank => -back,
                InfeedMethod::Alternating if j % 2 == 0 => -back,
                InfeedMethod::Alternating => back,
            };
            let pass = Pass { tip, shift };
            out.push(pass);
            if self.repeat_each_depth && tip > root + super::revolve::HAIR {
                out.push(pass);
            }
        }
        Ok(out)
    }

    /// The constant-area roughing ladder down to `floor`: cumulative
    /// depth `first * sqrt(n)`, each step at least `min_depth`, the
    /// last clamped to the floor.
    fn area_rungs(&self, ref_top: f64, floor: f64, first: f64) -> Result<Vec<f64>, GenError> {
        let target = ref_top - floor;
        let step = self.min_depth;
        if !(step > 0.0 && step.is_finite()) {
            return Err(GenError::Invalid(format!(
                "a constant-area thread needs a positive minimum depth per pass, not {step}"
            )));
        }
        if target > step * blade::MAX_RUNGS {
            return Err(GenError::Invalid(format!(
                "the ladder would run more than {:.0} passes - the minimum depth is far too fine \
                 for the thread to eat",
                blade::MAX_RUNGS
            )));
        }
        let mut out = Vec::new();
        let mut cum = 0.0_f64;
        let mut n = 1.0_f64;
        loop {
            let next = (first * n.sqrt()).max(cum + step);
            if next >= target - super::revolve::HAIR {
                out.push(floor);
                return Ok(out);
            }
            out.push(ref_top - next);
            cum = next;
            n += 1.0;
        }
    }
}
