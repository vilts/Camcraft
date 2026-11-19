//! Split from `drilling.rs` by M5.0.1; structure only.


use crate::cl::CycleKind;
use crate::geo2d::Point2;
use crate::geometry::Point3;
use crate::tools::{Tool, ToolKind};
/// One recognized (or hand-entered) hole, in world coordinates. The
/// cutter's own shape makes the diameter when drilling - `radius` is
/// the model's hole, kept for display, the operation region's rim
/// loops, and the gouge law's tangency test.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Hole {
    pub xy: Point2,
    pub radius: f64,
    /// The material's entry surface at the hole (highest wall vertex).
    pub entry_z: f64,
    /// The hole's floor (lowest wall vertex); a through hole's floor is
    /// the mesh's bottom.
    pub floor_z: f64,
    pub through: bool,
    /// Set when recognition folded Z-stepped members (a counterbore, a
    /// chamfer) into this hole: `radius` is then the NARROW bore and
    /// the fold left a wider ring no field records (M8.3). A drilled
    /// hole owes nothing to that ring - it is another operation's
    /// promise - but a bore cycle finishing "the hole" would silently
    /// miss it, so [`crate::drilling::cycle::drill`] declines stepped
    /// holes for `Bore | BackBore` by name.
    pub stepped: bool,
}

impl Hole {
    pub(super) fn all_finite(&self) -> bool {
        self.xy.x.is_finite()
            && self.xy.y.is_finite()
            && self.radius.is_finite()
            && self.entry_z.is_finite()
            && self.floor_z.is_finite()
    }

    /// Structural sanity for command construction (the params mirror of
    /// the generator's gate): finite numbers, a positive radius, and a
    /// floor below the entry - a hole that cuts upward is not a hole.
    #[must_use]
    pub fn is_usable(&self) -> bool {
        self.all_finite() && self.radius > 0.0 && self.floor_z < self.entry_z
    }
}

/// A cross-drilled hole: the same round closed wall, standing on a
/// RADIAL axis. Live tooling (M5.2 F2) points a drill from the +X
/// or +Y turret position in toward the spindle axis, so the plunge
/// runs along `dir` (the inward unit of that axis) and depth
/// DECREASES the axis coordinate. `origin` is the MOUTH's center -
/// the wall's highest coordinate along the axis, where the tool
/// first meets steel - and the scalars ride `dir` from there:
/// `entry_d = 0.0` at the mouth (kept for a future angled entry),
/// `floor_d = -depth` at the floor. Recognition fits the circle in
/// the plane the wall's vertices actually live in (YZ for an X
/// hole, XZ for a Y one); the cycle plunges along `dir` with the
/// move's tool pose stated, not down Z.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CrossHole {
    pub origin: Point3,
    pub dir: Point3,
    pub radius: f64,
    /// Distance from `origin` along `dir` to the material's entry;
    /// 0.0 for a mouth-on-the-face hole.
    pub entry_d: f64,
    /// Distance from `origin` along `dir` to the floor: negative,
    /// the drill's travel end. A through hole's floor is the far
    /// wall plus the exit allowance the cycle adds.
    pub floor_d: f64,
    pub through: bool,
}

impl CrossHole {
    /// Structural sanity for command construction, mirroring
    /// [`Hole::is_usable`]: finite numbers, a unit plunge direction,
    /// a positive radius, and a floor beyond the entry along `dir`
    /// (a smaller `floor_d` is deeper travel).
    #[must_use]
    pub fn is_usable(&self) -> bool {
        let n2 = self.dir.x * self.dir.x + self.dir.y * self.dir.y + self.dir.z * self.dir.z;
        self.origin.is_finite()
            && n2.is_finite()
            && (n2 - 1.0).abs() <= 1e-9
            && self.radius.is_finite()
            && self.radius > 0.0
            && self.entry_d.is_finite()
            && self.floor_d.is_finite()
            && self.floor_d < self.entry_d
    }
}

/// Which cycle runs. `Auto` lets the tool's kind and the hole's depth
/// decide (see [`auto_kind`]); the others are the user's explicit word.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum CycleChoice {
    #[default]
    Auto,
    Spot,
    Drill,
    Peck,
    Tap,

    Bore,
    BackBore,
    /// Explicit floating-holder intent; Auto and legacy Tap remain rigid.
    FloatingTap,
    /// Reaming (#1445): feed in, feed out ([`CycleKind::Ream`]). Never
    /// automatic - a reamer is the user's word, like Spot.
    Ream,
}

/// Which reference point on a twist drill the through-hole `break_through`
/// allowance is measured from (#2441).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum BreakThroughFrom {
    /// The full cutting diameter breaks through the floor by `through_clear`;
    /// the tip travels an extra `(diameter / 2) / tan(tip_angle / 2)` for a
    /// twist drill so no cone lip remains on exit.
    #[default]
    FullDiameter,
    /// The drill tip itself stops `through_clear` past `floor_z` (no point
    /// cone added). Useful to keep the tip clear of a fixture below the part.
    Tip,
}

/// Everything the generator needs, with heights already resolved to
/// world Z by the operation layer.
pub struct CycleParams<'a> {
    /// The plane the tool starts and ends every hole above.
    pub clearance_z: f64,
    /// The cycle's R plane: the feed entry height (Top of Part).
    pub r_z: f64,
    /// Retract plane between holes; `None` retracts to clearance.
    pub hop_z: Option<f64>,
    /// Depth override for every hole; `None` uses each hole's floor
    /// (plus exit allowance past a through hole's far wall).
    pub bottom_z: Option<f64>,
    pub cycle: CycleChoice,
    /// Depth per peck; `<= 0` lets the Auto law pick (see [`drill`]).
    pub peck: f64,
    /// Seconds at bottom; recorded on the cycles that honor a dwell.
    pub dwell: f64,
    pub feed: f64,
    pub plunge_feed: f64,
    pub tool: &'a Tool,
    pub holes: &'a [Hole],
    /// How far past a through hole's floor the break-through runs (#1557):
    /// the operation's stated break-through, [`THROUGH_CLEAR`] by default.
    /// Read only where a hole is through and no `bottom_z` overrides it.
    pub through_clear: f64,
    /// Whether `through_clear` measures from the full cutting diameter
    /// (adding [`drill_point_length`]) or from the drill tip (#2441).
    pub break_through_from: BreakThroughFrom,
}

/// A spot drill's dimple depth below the entry plane, unless the floor
/// is in the way. General spotting practice.
pub const SPOT_DEPTH: f64 = 2.0;

/// How far past a through hole's far wall the drill must exit so the
/// full diameter is clean through. Standard through-drilling allowance.
pub const THROUGH_CLEAR: f64 = 1.0;

/// Conical point length `r / tan(tip_angle / 2)` of a twist drill (#2441).
/// Flat/non-drill tools (`EndMill`, `Tap`, `SpotDrill`, `Reamer`, `BoringBar`,
/// or a `180°` flat-bottom drill) add `0.0`. Refuses an invalid or
/// out-of-range (`<= 0` or `> 180` degrees) `Drill` tip angle by name.
pub fn drill_point_length(tool: &Tool) -> Result<f64, crate::machining2d::GenError> {
    if tool.kind != crate::tools::ToolKind::Drill {
        return Ok(0.0);
    }
    let r = tool.diameter * 0.5;
    if !(r.is_finite() && r > 0.0) {
        return Err(crate::machining2d::GenError::Invalid(format!(
            "drill diameter {} is not a positive finite number",
            tool.diameter
        )));
    }
    let angle = tool.tip_angle;
    if !angle.is_finite() || angle <= 0.0 || angle > 180.0 {
        return Err(crate::machining2d::GenError::Invalid(format!(
            "drill tip angle {angle} must be finite and between 0 and 180 degrees"
        )));
    }
    if (angle - 180.0).abs() <= 1e-9 {
        return Ok(0.0);
    }
    let tan_half = (angle.to_radians() * 0.5).tan();
    if !tan_half.is_finite() || tan_half <= 0.0 {
        return Err(crate::machining2d::GenError::Invalid(format!(
            "drill tip angle {angle} produces no usable point cone"
        )));
    }
    Ok(r / tan_half)
}

/// Tip travel past a through hole's floor (`floor_z`) for `through_clear`
/// and `from` (#2441): adds [`drill_point_length`] only when `from` is
/// [`BreakThroughFrom::FullDiameter`] and the cycle/tool is a twist drill
/// (not `Spot`, `Tap`, `FloatingTap`, or `Ream`).
pub fn through_tip_run(
    tool: &Tool,
    choice: CycleChoice,
    through_clear: f64,
    from: BreakThroughFrom,
) -> Result<f64, crate::machining2d::GenError> {
    let clear = if through_clear.is_finite() { through_clear.max(0.0) } else { 0.0 };
    let non_drill_cycle = matches!(
        choice,
        CycleChoice::Spot | CycleChoice::Tap | CycleChoice::FloatingTap | CycleChoice::Ream
    );
    let extra = match from {
        BreakThroughFrom::Tip => 0.0,
        BreakThroughFrom::FullDiameter if non_drill_cycle => 0.0,
        BreakThroughFrom::FullDiameter => drill_point_length(tool)?,
    };
    Ok(clear + extra)
}

/// G73 chip-break hop: up about a millimeter, snap the chip, continue.
/// Published general practice for chip-breaking peck cycles.
pub const CHIP_BREAK: f64 = 1.0;

/// How far a RECOGNIZED radius may sit from the truth: the fitted
/// circle's own tolerance, 20 um - over our tessellation's chord noise,
/// under any real taper. It is one physical quantity wearing three
/// laws (M8.3): the vertex residual a cylinder must meet to be one
/// (`recognize.rs`), the radius gap that makes a folded wall pair a
/// real step rather than the same wall twice, and the bore door's band
/// in `cycle.rs` - a slide this much over the hole is snap noise, not
/// a wrong tool; beyond it, refuse by name. A hand-entered radius is
/// exact, so the band only ever gives the fit's benefit of the doubt
/// to geometry the machine could not measure better.
pub const FIT_TOL: f64 = 0.02;

/// The Auto cycle law: the tool's kind speaks first (a tap taps, a
/// boring bar bores); otherwise it is depth against the diameter -
/// one length or less is a plain drill, up to four diameters a
/// chip-break peck, deeper gets full-retract pecks and the dwell word
/// (G83's shape). Spot is never automatic: it is what the user says it
/// is.
#[must_use]
pub fn auto_kind(tool_kind: ToolKind, depth: f64, diameter: f64) -> CycleKind {
    match tool_kind {
        ToolKind::Tap => CycleKind::Tap,
        ToolKind::Bore => CycleKind::Bore,
        _ => {
            if depth <= diameter {
                CycleKind::Drill
            } else if depth <= 4.0 * diameter {
                CycleKind::Peck
            } else {
                CycleKind::Deep
            }
        }
    }
}

// ---------------------------------------------------------------- recognition
