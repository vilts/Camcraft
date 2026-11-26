//! Caller dialect presets, machine reference positions, and initial modal state.
//! Provenance: controller dwell/arc presets follow public Fanuc, Haas, LinuxCNC/PathPilot, grblHAL, TinyG, Carvera docs and CamCraft post contracts (`assets/posts/*.cps`).

use camcraft_cl::cl::ArcPlane;
use camcraft_geometry::geometry::Point3;
use camcraft_geometry::geometry::units::Unit;

use crate::types::{
    ArcCenterMode, CoolantState, CycleReturnMode, DistanceMode, FeedMode, MotionMode,
    PathControlMode, SpindleDir, WorkOffset,
};

/// How `P` dwell values are scaled to seconds.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum DwellUnit {
    /// `P` is in seconds (RS-274/NGC, LinuxCNC, Tormach, grblHAL, TinyG, Carvera).
    #[default]
    Seconds,
    /// `P` is in milliseconds (Fanuc).
    Milliseconds,
    /// `P` with a decimal point is in seconds, `P` without one is in milliseconds
    /// (Haas: `G04 P10` is 10 ms, `G04 P10.` is 10 s).
    PointSecondsElseMilliseconds,
}

impl DwellUnit {
    /// Scale a `P` value to seconds. `has_point` says whether the word had a decimal point.
    #[must_use]
    pub fn to_seconds(self, value: f64, has_point: bool) -> f64 {
        match self {
            DwellUnit::Seconds => value,
            DwellUnit::Milliseconds => value / 1000.0,
            DwellUnit::PointSecondsElseMilliseconds if has_point => value,
            DwellUnit::PointSecondsElseMilliseconds => value / 1000.0,
        }
    }
}

/// Dialect configuration passed by the caller.
#[derive(Clone, Debug, PartialEq)]
pub struct Dialect {
    pub name: String,
    pub default_arc_center: ArcCenterMode,
    pub default_unit: Option<Unit>,
    pub cycle_dwell_unit: DwellUnit,
    pub g4_dwell_unit: DwellUnit,
    pub arc_radius_tolerance_mm: f64,
    pub chip_break_retract_mm: f64,
    pub initial_position: Point3,
}

impl Default for Dialect {
    fn default() -> Self {
        Self::rs274()
    }
}

impl Dialect {
    /// Default RS-274/NGC dialect: `G91.1`, no fallback unit, dwell `P` in seconds.
    #[must_use]
    pub fn rs274() -> Self {
        Self {
            name: "rs274".to_string(),
            default_arc_center: ArcCenterMode::Incremental,
            default_unit: None,
            cycle_dwell_unit: DwellUnit::Seconds,
            g4_dwell_unit: DwellUnit::Seconds,
            arc_radius_tolerance_mm: 0.02,
            chip_break_retract_mm: 1.0,
            initial_position: Point3::ZERO,
        }
    }

    /// Fanuc 3-axis milling profile (`G91.1`, `P` in milliseconds).
    #[must_use]
    pub fn fanuc() -> Self {
        Self {
            name: "fanuc".to_string(),
            cycle_dwell_unit: DwellUnit::Milliseconds,
            g4_dwell_unit: DwellUnit::Milliseconds,
            ..Self::rs274()
        }
    }

    /// Haas 3-axis milling profile (`G91.1`; `P` with a decimal point in seconds,
    /// without one in milliseconds).
    #[must_use]
    pub fn haas() -> Self {
        Self {
            name: "haas".to_string(),
            cycle_dwell_unit: DwellUnit::PointSecondsElseMilliseconds,
            g4_dwell_unit: DwellUnit::PointSecondsElseMilliseconds,
            ..Self::rs274()
        }
    }

    /// Tormach PathPilot 3-axis profile (`G91.1`, `P` in seconds).
    #[must_use]
    pub fn tormach() -> Self {
        Self {
            name: "tormach".to_string(),
            ..Self::rs274()
        }
    }

    /// grblHAL 3-axis profile (`G91.1`, `P` in seconds).
    #[must_use]
    pub fn grblhal() -> Self {
        Self {
            name: "grblhal".to_string(),
            ..Self::rs274()
        }
    }

    /// Synthetos TinyG 3-axis profile (`G91.1`, `P` in seconds).
    #[must_use]
    pub fn tinyg() -> Self {
        Self {
            name: "tinyg".to_string(),
            ..Self::rs274()
        }
    }

    /// Makera Carvera 3-axis profile (`G91.1`, `P` in seconds).
    #[must_use]
    pub fn carvera() -> Self {
        Self {
            name: "carvera".to_string(),
            ..Self::rs274()
        }
    }
}

/// Optional machine reference points and work-offset origins (in machine mm).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ReferencePositions {
    /// Machine-coordinate `G28` home position (mm).
    pub g28: Option<Point3>,
    /// Machine-coordinate `G30` secondary reference position (mm).
    pub g30: Option<Point3>,
    /// Default machine-coordinate origin of the active work offset for `G53` moves (mm).
    pub g53_origin: Option<Point3>,
    /// Optional per-WCS origins in machine coordinates (`G54`..`G59`), overriding `g53_origin`.
    pub wcs_origins: [Option<Point3>; 6],
}

impl ReferencePositions {
    /// Convenience preset with `(0, 0, 0)` reference points for `G28`, `G30`, and `G53`.
    #[must_use]
    pub fn zero() -> Self {
        Self {
            g28: Some(Point3::ZERO),
            g30: Some(Point3::ZERO),
            g53_origin: Some(Point3::ZERO),
            wcs_origins: [None; 6],
        }
    }

    /// Machine-coordinate origin for `wcs`, falling back to `g53_origin`.
    #[must_use]
    pub fn wcs_origin(&self, wcs: WorkOffset) -> Option<Point3> {
        self.wcs_origins[wcs.index()].or(self.g53_origin)
    }
}

/// Caller configuration for parsing a 3-axis NC program.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ParseOptions {
    pub dialect: Dialect,
    pub references: ReferencePositions,
}

/// Snapshot of modal groups tracked during parsing.
#[derive(Clone, Debug, PartialEq)]
pub struct ModalState {
    pub motion: MotionMode,
    pub plane: ArcPlane,
    pub distance: DistanceMode,
    pub arc_center: ArcCenterMode,
    pub feed_mode: FeedMode,
    pub units: Option<Unit>,
    pub length_offset: Option<u32>,
    pub work_offset: WorkOffset,
    pub cycle_return: CycleReturnMode,
    pub path_control: PathControlMode,
    pub feed_rate_mm_min: Option<f64>,
    pub spindle_rpm: Option<f64>,
    pub spindle_dir: SpindleDir,
    pub coolant: CoolantState,
    pub active_tool: Option<u32>,
    pub pending_tool: Option<u32>,
}

impl ModalState {
    #[must_use]
    pub fn from_dialect(dialect: &Dialect) -> Self {
        Self {
            motion: MotionMode::None,
            plane: ArcPlane::Xy,
            distance: DistanceMode::Absolute,
            arc_center: dialect.default_arc_center,
            feed_mode: FeedMode::PerMinute,
            units: dialect.default_unit,
            length_offset: None,
            work_offset: WorkOffset::G54,
            cycle_return: CycleReturnMode::InitialZ,
            path_control: PathControlMode::default(),
            feed_rate_mm_min: None,
            spindle_rpm: None,
            spindle_dir: SpindleDir::Off,
            coolant: CoolantState::default(),
            active_tool: None,
            pending_tool: None,
        }
    }
}
