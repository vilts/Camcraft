//! Helical thread milling: 3-axis interpolation of internal and external threads with a single-form or multi-form thread mill (M2.4 C, reworked by #894).
//!
//! Provenance: helical-interpolation thread milling as the public
//! machine-tool handbooks describe it (Machinery's Handbook, "Thread
//! milling"; Smid, "CNC Programming Handbook", the helical milling and
//! thread milling chapters): the cutter orbits the thread axis on a
//! helix whose lead is the pitch; a single-form cutter rides one turn
//! per thread, a multi-form cutter finishes every thread its teeth span
//! in one revolution; the tool enters and leaves on a tangential arc;
//! the depth may be taken in several radial passes; the feed is stated
//! at the cutting edge and programmed at the tool centre, scaled by
//! centre-circle over edge-circle. Thread depths are the ISO metric
//! profile's (ISO 724: internal 0.5413 x pitch; ISO 965-1 / DIN 13:
//! external 0.6134 x pitch). Screw handedness and the G2/G3 sense are
//! RS-274 / ISO 6983's ([`ThreadHand`]). Written clean-room from those
//! statements; no CAM product's output was consulted.
//!
//! The law in one paragraph. The model's wall is the thread's MINOR
//! cylinder on a hole (what CAD draws of a tapped bore) and its MAJOR
//! cylinder on a boss. Radial pass `i` of `n` puts the tooth crests
//! `depth x i / n` into that wall, so the last pass lands on the full
//! thread depth and no pass goes past it. With the spindle turning
//! clockwise seen from above (M3), climb milling orbits an internal
//! thread counter-clockwise and an external one clockwise; the hand then
//! fixes the axial direction (right-hand: counter-clockwise rises), so a
//! right-hand internal climb cut starts at the bottom and winds up. Every
//! pass rides the SAME helix - it meets angle zero at the same height -
//! so later passes deepen the groove the first one opened. The lead arcs
//! travel axially at the helix's own rate per unit of path, so the tooth
//! stays in the groove while it engages (on a hole the lead-in is the
//! semicircle from the axis, which by the inscribed-angle theorem is on
//! the helix at every point).
//!
//! Prime Directive: the tool tip never goes below the feature's floor
//! (a through hole's floor is one pitch below its exit); no rapid runs
//! below the feature's top - every move in the bore or beside the boss
//! is a feed; a mill too wide for the hole, a stepped hole, an unstated
//! pitch, an infeed past the thread depth, a run-out above the link
//! plane or an R plane under a feature's top (#1212: the rapid stops at
//! the R plane and the feed starts there, as on a drill cycle) is
//! refused by name.

use crate::cl::ThreadHand;
use crate::geo2d::Point2;
use crate::geometry::Point3;

use super::face::GenError;
pub use super::thread_passes::{PassSchedule, ThreadPasses, MAX_SPRING_PASSES};

pub use camcraft_tools::thread_mill::{
    EXTERNAL_DEPTH_PER_PITCH as EXTERNAL_THREAD_DEPTH_PER_PITCH, INTERNAL_DEPTH_PER_PITCH as THREAD_DEPTH_PER_PITCH,
};

/// One feature to thread, depths in world Z. On a hole `radius` is the
/// minor radius (the drilled wall); on a boss it is the major radius.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ThreadHole {
    pub xy: Point2,
    pub radius: f64,
    pub top_z: f64,
    /// The lowest height the tool tip may reach: a blind hole's floor,
    /// a boss's shoulder.
    pub floor_z: f64,
    pub stepped: bool,
    /// A through hole: the tip may run one pitch past `floor_z`, so the
    /// thread is cut whole at the exit.
    pub through: bool,
}

impl ThreadHole {
    fn all_finite(&self) -> bool {
        [self.xy.x, self.xy.y, self.radius, self.top_z, self.floor_z].into_iter().all(f64::is_finite)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct ThreadParams {
    pub holes: Vec<ThreadHole>,
    /// The thread's lead per turn, mm.
    pub pitch: f64,
    /// Across the tooth crests.
    pub tool_diameter: f64,
    /// Rows of teeth along the axis: 1 rides one turn per thread (a
    /// single-form mill), more finish `rows` threads per revolution.
    pub rows: u32,
    /// Radial depth per roughing pass (the first, largest one on a
    /// constant-area schedule); the last pass lands on the thread depth
    /// less the allowance.
    pub infeed: f64,
    /// A boss (external thread) instead of a hole.
    pub external: bool,
    pub hand: ThreadHand,
    /// Climb milling (the orbit sense follows the spindle, #988).
    pub climb: bool,
    /// Pass schedule, finish and spring passes, allowance, spindle and
    /// cutter hand (#988); the default is the #894 ride.
    pub passes: ThreadPasses,
    pub clearance_z: f64,
    /// The R plane (#1212): where every feature's rapid stops and its
    /// feed begins, as on a drill cycle. `None` stops at each feature's
    /// own top (the #894 ride). Never below a feature's top, never above
    /// the link plane - both refused by name.
    pub r_z: Option<f64>,
    /// The plane links between features travel at; `None` = clearance.
    pub retract_z: Option<f64>,
    /// Feed at the cutting edge (the thread's own circle), mm/min.
    pub feed: f64,
    pub plunge_feed: f64,
}

impl ThreadParams {
    /// The radial thread depth this ride cuts, mm.
    #[must_use]
    pub fn depth(&self) -> f64 {
        self.pitch * if self.external { EXTERNAL_THREAD_DEPTH_PER_PITCH } else { THREAD_DEPTH_PER_PITCH }
    }

    /// Orbit sense: counter-clockwise seen from +Z when true. Climb
    /// under M3 orbits a hole counter-clockwise and a boss clockwise;
    /// an M4 spindle (a left-hand-cut mill, #988) flips both.
    #[must_use]
    pub fn ccw(&self) -> bool {
        (self.climb != self.external) != self.passes.spindle_reverse
    }

    /// Does the helix rise (+Z) as it winds?
    #[must_use]
    pub fn rising(&self) -> bool {
        ThreadHand::of_turn(self.ccw(), 1.0) == Some(self.hand)
    }

    pub(super) fn link_z(&self) -> f64 {
        self.retract_z.unwrap_or(self.clearance_z)
    }

    pub(super) fn validate(&self) -> Result<(), GenError> {
        let bad = |m: &str| Err(GenError::Invalid(m.into()));
        let mut finite = [self.pitch, self.tool_diameter, self.infeed, self.clearance_z, self.feed, self.plunge_feed]
            .into_iter()
            .all(f64::is_finite);
        finite &= self.retract_z.is_none_or(f64::is_finite);
        finite &= self.r_z.is_none_or(f64::is_finite);
        finite &= self.holes.iter().all(ThreadHole::all_finite);
        if !finite {
            return bad("parameters must be finite");
        }
        if self.holes.is_empty() {
            return bad(if self.external {
                "thread milling needs at least one boss"
            } else {
                "thread milling needs at least one hole"
            });
        }
        if self.pitch <= 0.0 {
            return bad("a thread mill needs a positive pitch to time its helix against");
        }
        if self.tool_diameter <= 0.0 {
            return bad("tool diameter must be positive");
        }
        if self.rows == 0 {
            return bad("a thread mill needs at least one row of teeth");
        }
        let h = self.depth();
        if self.tool_diameter * 0.5 <= h {
            return Err(GenError::Invalid(format!(
                "a {} mm thread mill is no wider than twice the {h} mm thread depth - its teeth leave no core",
                self.tool_diameter
            )));
        }
        if self.infeed <= 0.0 || self.infeed > h + 1e-9 {
            return Err(GenError::Invalid(format!(
                "infeed must sit between 0 and the {h} mm thread depth this pitch stands at"
            )));
        }
        if let Some(why) = self.passes.refusal(Some(h), self.infeed) {
            return Err(GenError::Invalid(why));
        }
        if self.feed <= 0.0 || self.plunge_feed <= 0.0 {
            return bad("feeds must be positive");
        }
        let top = self.holes.iter().map(|x| x.top_z).fold(f64::NEG_INFINITY, f64::max);
        if self.clearance_z < top {
            return bad("clearance plane below a thread's entry");
        }
        if self.link_z() < top || self.link_z() > self.clearance_z {
            return bad("retract plane must stand between the threads' entries and the clearance plane");
        }
        if let Some(r) = self.r_z {
            if r > self.link_z() + 1e-9 {
                return Err(GenError::Invalid(format!(
                    "the R plane at z {r} stands above the {} mm retract plane the threads link at - lower the R plane or raise the retract plane",
                    self.link_z()
                )));
            }
            // #1212: the rapid stops at R, so R under a feature's top
            // would rapid into it. Refused by name, never clamped.
            if let Some(x) = self.holes.iter().find(|x| r < x.top_z - 1e-9) {
                let what = if self.external { "boss" } else { "hole" };
                return Err(GenError::invalid_at(
                    format!(
                        "the R plane at z {r} is below the top of the {what} at ({}, {}) at z {} - the rapid down to R would run into it; raise the R plane (feed starts here) to z {} or above",
                        x.xy.x, x.xy.y, x.top_z, x.top_z
                    ),
                    Point3::new(x.xy.x, x.xy.y, x.top_z),
                ));
            }
        }
        self.holes.iter().try_for_each(|x| self.validate_hole(x, h))
    }

    fn validate_hole(&self, x: &ThreadHole, h: f64) -> Result<(), GenError> {
        if x.radius <= 0.0 {
            return Err(GenError::Invalid("thread radius must be positive".into()));
        }
        if x.floor_z >= x.top_z {
            return Err(GenError::Invalid("a thread must have its floor below its entry".into()));
        }
        // #261 part 2: a refusal about this feature points at its mouth.
        let mouth = Point3::new(x.xy.x, x.xy.y, x.top_z);
        let at = |m: String| Err(GenError::invalid_at(m, mouth));
        if self.external {
            if x.radius <= h {
                return at(format!(
                    "the {} mm boss at ({}, {}) is thinner than the {h} mm thread is deep",
                    x.radius * 2.0,
                    x.xy.x,
                    x.xy.y
                ));
            }
            return Ok(());
        }
        if x.stepped {
            return at(format!(
                "hole at ({}, {}) carries a counterbore fold - thread milling cuts the \
                 wall it can see and would silently miss the ring it cannot",
                x.xy.x, x.xy.y
            ));
        }
        // The air door: the whole cutter must stand inside the minor
        // cylinder to travel the axis, and the first pass's centre
        // circle must be a real circle.
        if self.tool_diameter * 0.5 >= x.radius {
            return at(format!(
                "a {} mm thread mill has no air to enter the {} mm drill hole at ({}, {})",
                self.tool_diameter,
                x.radius * 2.0,
                x.xy.x,
                x.xy.y
            ));
        }
        Ok(())
    }
}

/// One radial pass: its centre-circle radius, the axial travel of its
/// lead arcs and the feed programmed at the tool centre.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Pass {
    pub rho: f64,
    pub lead_z: f64,
    pub feed: f64,
}

/// What one feature's ride is made of: the passes, the low tip height
/// of each axial level, the turns per level and the heights that bound
/// the ride.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct HolePlan {
    pub passes: Vec<Pass>,
    /// The largest lead-arc axial travel: every pass's helix starts
    /// this far above its level's low height, so all passes share one
    /// helix.
    pub lead_max: f64,
    pub lows: Vec<f64>,
    pub turns: u32,
    /// Where the rapid stops and the feed begins: the R plane (or the
    /// feature's top when none is stated), or the ride's own highest tip
    /// height when that stands higher.
    pub rapid_z: f64,
}

impl HolePlan {
    pub(super) fn of(p: &ThreadParams, x: &ThreadHole) -> Result<Self, GenError> {
        let r = p.tool_diameter * 0.5;
        let h = p.depth();
        let passes: Vec<Pass> = p
            .passes
            .depths(h, p.infeed)
            .into_iter()
            .map(|d| {
                let (rho, lead_z, scale) = if p.external {
                    let rho = x.radius + r - d;
                    // A quarter arc of the tool's own radius: its axial
                    // travel at the helix's rate per unit of path.
                    (rho, p.pitch * r / (4.0 * rho), rho / (rho - r))
                } else {
                    let rho = x.radius - r + d;
                    // The semicircle from the axis: a quarter pitch.
                    (rho, p.pitch * 0.25, rho / (rho + r))
                };
                Pass { rho, lead_z, feed: (p.feed * scale * 1000.0).round() / 1000.0 }
            })
            .collect();
        let lead_max = passes.iter().map(|q| q.lead_z).fold(0.0, f64::max);
        let floor = if x.through { x.floor_z - p.pitch } else { x.floor_z };
        let span = x.top_z - floor - lead_max;
        let per_level = f64::from(p.rows) * p.pitch;
        let (turns, levels) = if p.rows == 1 {
            (((span / p.pitch) - 1e-9).ceil().clamp(1.0, 100_000.0) as u32, 1)
        } else {
            (1, ((span / per_level) - 1e-9).ceil().clamp(1.0, 100_000.0) as u32)
        };
        let lows: Vec<f64> = (0..levels).map(|k| floor + f64::from(k) * per_level).collect();
        let last = lows.last().copied().unwrap_or(floor);
        let run_out = last + 2.0 * lead_max + f64::from(turns) * p.pitch;
        if !(run_out.is_finite() && passes.iter().all(|q| q.rho.is_finite() && q.rho > 0.0 && q.feed > 0.0)) {
            return Err(GenError::Invalid("thread ride left the finite horizon".into()));
        }
        if run_out > p.link_z() + 1e-9 {
            return Err(GenError::invalid_at(
                format!(
                    "the thread at ({}, {}) runs out {run_out} mm high, above the {} mm retract plane - raise the plane",
                    x.xy.x,
                    x.xy.y,
                    p.link_z()
                ),
                Point3::new(x.xy.x, x.xy.y, x.top_z),
            ));
        }
        Ok(Self { passes, lead_max, lows, turns, rapid_z: p.r_z.unwrap_or(x.top_z).max(x.top_z).max(run_out) })
    }
}

pub use super::thread_ride::thread_mill;
