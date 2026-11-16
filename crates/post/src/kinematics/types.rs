// Provenance: clean-room implementation from public 5-axis XYZAC trunnion geometry and Rodrigues rotation math.

//! Configuration, pose, path, and error types for 5-axis AC table-table kinematics.

use camcraft_cl::cl::MoveKind;
use thiserror::Error;

/// Sign convention mapping machine command angles to physical table rotations.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum RotarySignConvention {
    /// Positive command angle obeys the right-hand rule around the table rotary axis vector.
    #[default]
    TableRightHand,
    /// Positive command angle rotates the table oppositely (tool-relative right-hand rule).
    ToolRightHand,
}

impl RotarySignConvention {
    /// Returns `+1.0` for table right-hand or `-1.0` for tool-relative sign convention.
    #[inline]
    pub fn factor(self) -> f64 {
        match self {
            Self::TableRightHand => 1.0,
            Self::ToolRightHand => -1.0,
        }
    }
}

/// Section transition policy governing whether `C` may re-wrap across revolutions.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SectionBoundary {
    /// Inside a cut/feed section: unwrap `C` continuously near the previous modal angle.
    #[default]
    Continuous,
    /// At a clearance retract boundary: allow re-wrapping `C` into its primary turn within travel.
    SafeRetract,
}

/// Mechanical and travel definition for one rotary axis (`A` tilt or `C` rotary table).
#[derive(Clone, Debug, PartialEq)]
pub struct RotaryAxisConfig {
    /// Human-readable axis letter used in error messages (for example `"A"` or `"C"`).
    pub name: String,
    /// Unit (or non-zero) rotation axis vector in machine space.
    pub axis: [f64; 3],
    /// Inclusive command-angle limits `(min_deg, max_deg)`, or `None` for unlimited travel.
    pub travel: Option<(f64, f64)>,
    /// Machine zero offset subtracted before physical rotation (`phi = sign * (cmd - zero_offset_deg)`).
    pub zero_offset_deg: f64,
    /// Whether safe-retract re-wrapping targets `[0, 360)` rather than signed `[-180, 180]`.
    pub rollover: bool,
}

/// Machine geometry and solver parameters for a table-table XYZAC mill.
#[derive(Clone, Debug, PartialEq)]
pub struct AcTrunnionConfig {
    /// Tilting cradle axis (`A`), parent of the `C` table.
    pub a: RotaryAxisConfig,
    /// Rotary platter axis (`C`), carried by the `A` cradle.
    pub c: RotaryAxisConfig,
    /// Fixed spindle tool-axis vector pointing from tool tip toward spindle nose in machine space.
    pub spindle_axis: [f64; 3],
    /// Pivot point of the `A` cradle in machine coordinates (mm).
    pub a_pivot: [f64; 3],
    /// Offset from `A` pivot to `C` rotation center when `A = 0, C = 0` (mm).
    pub c_offset: [f64; 3],
    /// Offset from `C` center to part origin when `A = 0, C = 0` (mm).
    pub wcs_offset: [f64; 3],
    /// Machine work-datum shift subtracted from machine coordinates (`xyz = P_mach - work_datum`, mm).
    pub work_datum: [f64; 3],
    /// Sign convention between programmed `A/C` angles and physical right-hand rotation.
    pub sign: RotarySignConvention,
    /// Pole threshold on `||k_c x tool_axis||`; below this, `C` holds its previous modal value.
    pub pole_sin_tol: f64,
    /// Penalty weight per degree of `A` motion when ranking valid inverse-kinematics branches.
    pub weight_a: f64,
    /// Penalty weight per degree of `C` motion when ranking valid inverse-kinematics branches.
    pub weight_c: f64,
    /// Preferred initial `C` angle (deg) for the first pose or pole pose before any move is solved.
    pub initial_c_deg: f64,
}

impl Default for AcTrunnionConfig {
    fn default() -> Self {
        Self {
            a: RotaryAxisConfig {
                name: "A".to_string(),
                axis: [1.0, 0.0, 0.0],
                travel: Some((-110.0, 30.0)),
                zero_offset_deg: 0.0,
                rollover: false,
            },
            c: RotaryAxisConfig {
                name: "C".to_string(),
                axis: [0.0, 0.0, 1.0],
                travel: None,
                zero_offset_deg: 0.0,
                rollover: false,
            },
            spindle_axis: [0.0, 0.0, 1.0],
            a_pivot: [0.0, 0.0, 0.0],
            c_offset: [0.0, 0.0, 0.0],
            wcs_offset: [0.0, 0.0, 0.0],
            work_datum: [0.0, 0.0, 0.0],
            sign: RotarySignConvention::TableRightHand,
            pole_sin_tol: 1e-6,
            weight_a: 1.0,
            weight_c: 1.0,
            initial_c_deg: 0.0,
        }
    }
}

/// Validated and normalized trunnion configuration ready for fast repeated evaluations.
#[derive(Clone, Debug)]
pub struct ValidatedTrunnion {
    /// Original `A` axis metadata.
    pub a: RotaryAxisConfig,
    /// Original `C` axis metadata.
    pub c: RotaryAxisConfig,
    /// Normalized unit vector of the `A` axis in machine space.
    pub a_unit: [f64; 3],
    /// Normalized unit vector of the `C` axis in machine space.
    pub c_unit: [f64; 3],
    /// Normalized unit spindle vector in machine space.
    pub spindle_unit: [f64; 3],
    /// `A` cradle pivot in machine space (mm).
    pub a_pivot: [f64; 3],
    /// `C` center offset from `A` pivot at home (mm).
    pub c_offset: [f64; 3],
    /// Part origin offset from `C` center at home (mm).
    pub wcs_offset: [f64; 3],
    /// Machine work-datum origin shift (mm).
    pub work_datum: [f64; 3],
    /// Sign convention between programmed and physical rotary angles.
    pub sign: RotarySignConvention,
    /// Normalized sine tolerance identifying a `C` pole.
    pub pole_sin_tol: f64,
    /// Cost weight per degree of `A` motion.
    pub weight_a: f64,
    /// Cost weight per degree of `C` motion.
    pub weight_c: f64,
    /// Initial fallback `C` angle (deg).
    pub initial_c_deg: f64,
}

/// Tool-tip position and tool-axis vector in part coordinates.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PartPose {
    /// Tool tip coordinates in the part frame (mm).
    pub tip: [f64; 3],
    /// Tool axis vector from tip toward shank in the part frame.
    pub axis: [f64; 3],
}

/// Solved machine position (`X/Y/Z` linear coordinates and `A/C` rotary command angles).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MachinePose {
    /// Linear machine axes `[X, Y, Z]` (mm) relative to `work_datum`.
    pub xyz: [f64; 3],
    /// Commanded `A` rotary angle (deg).
    pub a_deg: f64,
    /// Commanded `C` rotary angle (deg).
    pub c_deg: f64,
}

/// One analytical inverse-kinematics solution branch in command angles before travel filtering.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AcCandidate {
    /// Analytical `A` angle reduced into `(-180, 180]` (deg).
    pub a_deg: f64,
    /// Analytical `C` angle reduced into `(-180, 180]` (deg, or modal `C` at the pole).
    pub c_deg: f64,
    /// Whether the tool axis lies inside `pole_sin_tol` of the `C` axis.
    pub at_pole: bool,
}

/// Input motion segment in part coordinates for `solve_path`.
#[derive(Clone, Debug, PartialEq)]
pub struct PathMove {
    /// Linear motion kind (`Rapid`, `Feed`, or `Plunge`).
    pub kind: MoveKind,
    /// Segment start tool tip in part coordinates (mm).
    pub from_tip: [f64; 3],
    /// Segment end tool tip in part coordinates (mm).
    pub to_tip: [f64; 3],
    /// Segment start tool axis vector in part coordinates.
    pub from_axis: [f64; 3],
    /// Segment end tool axis vector in part coordinates.
    pub to_axis: [f64; 3],
    /// Optional feed rate (mm/min) preserved on subdivided output segments.
    pub feed: Option<f64>,
    /// Marks a clearance lift, checked with continuous C winding.
    /// Does not authorize an unwind; exhausted rotary travel is refused.
    pub safe_retract: bool,
}

/// Path-level discretization and chord tolerances for `solve_path`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PathOptions {
    /// Maximum allowed tool-tip deviation from joint linear interpolation (mm).
    pub chord_tol_mm: f64,
    /// Optional cap on per-segment `|dA|` or `|dC|` step during cutting moves (deg).
    pub max_angular_step_deg: Option<f64>,
    /// When true, non-retract rapids with rotary motion are also chord-subdivided.
    pub subdivide_rapids: bool,
}

impl Default for PathOptions {
    fn default() -> Self {
        Self {
            chord_tol_mm: 0.01,
            max_angular_step_deg: Some(5.0),
            subdivide_rapids: false,
        }
    }
}

/// One output machine move emitted by `solve_path` or `solve_toolpath`.
#[derive(Clone, Debug, PartialEq)]
pub struct SolvedMove {
    /// Zero-based index of the originating input move.
    pub source_index: usize,
    /// Motion kind (`Rapid`, `Feed`, or `Plunge`).
    pub kind: MoveKind,
    /// Preserved feed rate from the input move (mm/min).
    pub feed: Option<f64>,
    /// Machine pose at segment start.
    pub from: MachinePose,
    /// Machine pose at segment end.
    pub to: MachinePose,
    /// Part-frame tool tip at segment start (mm).
    pub tip_from: [f64; 3],
    /// Part-frame tool tip at segment end (mm).
    pub tip_to: [f64; 3],
    /// Unit part-frame tool axis at segment start.
    pub axis_from: [f64; 3],
    /// Unit part-frame tool axis at segment end.
    pub axis_to: [f64; 3],
}

/// Refusals returned when trunnion geometry, tool orientation, or travel limits fail validation.
#[derive(Clone, Debug, Error, PartialEq)]
pub enum KinematicsError {
    /// CL arcs cannot be represented by the linear path adapter without losing geometry.
    #[error("unsupported arc at move {source_index}: linearize within tolerance before AC solving")]
    UnsupportedArc {
        /// Index of the input move carrying arc geometry.
        source_index: usize,
    },
    /// Adjacent moves do not share a part pose; no implicit transition is safe.
    #[error("discontinuous path at move {source_index}: an explicit pose transition is required")]
    DiscontinuousPath {
        /// Index of the move whose start differs from the preceding end.
        source_index: usize,
    },
    /// The bounded subdivision budget cannot satisfy the requested path tolerances.
    #[error("subdivision limit exceeded at move {source_index} during {stage}; tolerance unmet")]
    SubdivisionLimitExceeded {
        /// Index of the input move that cannot be discretized within tolerance.
        source_index: usize,
        /// Subdivision procedure that exhausted its budget.
        stage: &'static str,
    },
    /// Machine configuration is malformed (zero vectors, parallel axes, inverted bounds, bad weights).
    #[error("{message}")]
    InvalidConfig {
        /// Detailed reason naming the offending axis or parameter.
        message: String,
    },
    /// Non-finite (`NaN` or `inf`) coordinate or angle supplied by the caller.
    #[error("non-finite input value for {field}")]
    NonFiniteInput {
        /// Name of the field holding a non-finite scalar.
        field: &'static str,
    },
    /// Direction vector has negligible magnitude and cannot define an axis.
    #[error("zero-length vector for {field}")]
    ZeroVector {
        /// Name of the vector field that has zero length.
        field: &'static str,
    },
    /// Geometry of the `A/C/spindle` axes makes the requested tool vector unreachable at any angle.
    #[error("unreachable tool orientation for axis {axis_name}: {reason}")]
    UnreachableOrientation {
        /// Rotary axis whose geometric projection cannot achieve the target vector.
        axis_name: String,
        /// Quantitative geometric shortfall.
        reason: String,
    },
    /// All analytical branches fall outside rotary travel bounds.
    #[error(
        "axis {axis_name} travel limit [{min_deg}, {max_deg}] deg exceeded (needs {required_deg:.3} deg); {hint}"
    )]
    TravelLimitExceeded {
        /// Name of the blocking rotary axis (`"A"` or `"C"`).
        axis_name: String,
        /// Nearest required command angle (deg).
        required_deg: f64,
        /// Configured lower travel bound (deg).
        min_deg: f64,
        /// Configured upper travel bound (deg).
        max_deg: f64,
        /// Actionable explanation (for example inserting a safe retract boundary).
        hint: String,
    },
}
