//! Public modal state, program output, and line-map types for `camcraft-gcode`.
//! Provenance: modal groups and word meanings from RS-274/NGC (NISTIR 6556) and ISO 6983; output types bind to camcraft-cl (`cl::Toolpath`).

use camcraft_cl::cl::Toolpath;

pub use crate::dialect::{Dialect, DwellUnit, ModalState, ParseOptions, ReferencePositions};
pub use crate::error::GcodeError;

/// How `I`/`J`/`K` arc center words are interpreted.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ArcCenterMode {
    /// `G91.1`: `I`/`J`/`K` are incremental offsets from the arc start point.
    #[default]
    Incremental,
    /// `G90.1`: `I`/`J`/`K` are absolute coordinates in the active work offset.
    Absolute,
}

/// Modal Group 3 distance mode.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum DistanceMode {
    /// `G90`: axis words are absolute positions in the active work offset.
    #[default]
    Absolute,
    /// `G91`: axis words are increments from the current position.
    Incremental,
}

/// Modal Group 5 feed rate mode.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum FeedMode {
    /// `G94`: feed rate in units per minute.
    #[default]
    PerMinute,
}

/// Modal Group 10 canned cycle return level.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CycleReturnMode {
    /// `G98`: return to the initial Z level held before the cycle started.
    #[default]
    InitialZ,
    /// `G99`: return to the `R` retract plane.
    RPlane,
}

/// Modal Group 13 path control mode.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PathControlMode {
    /// `G61`: exact stop / exact path mode.
    ExactStop,
    /// `G64`: continuous path blending, with optional `P` tolerance in mm.
    Continuous { tolerance_mm: Option<f64> },
}

impl Default for PathControlMode {
    fn default() -> Self {
        PathControlMode::Continuous { tolerance_mm: None }
    }
}

/// Spindle rotation state (`M3`/`M4`/`M5`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SpindleDir {
    #[default]
    Off,
    Cw,
    Ccw,
}

/// Active coolant channels (`M7`, `M8`, `M9`, `M88`, `M89`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CoolantState {
    pub flood: bool,
    pub mist: bool,
    pub through_spindle: bool,
}

/// Modal Group 12 work coordinate system (`G54`..`G59`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum WorkOffset {
    #[default]
    G54 = 1,
    G55 = 2,
    G56 = 3,
    G57 = 4,
    G58 = 5,
    G59 = 6,
}

impl WorkOffset {
    /// 1-based offset number (1 for `G54` .. 6 for `G59`).
    #[must_use]
    pub const fn number(self) -> u8 {
        self as u8
    }

    /// 0-based array index (0..6).
    #[must_use]
    pub const fn index(self) -> usize {
        (self as usize) - 1
    }

    /// RS-274 G-code integer (`54`..`59`).
    #[must_use]
    pub const fn g_code(self) -> u8 {
        53 + (self as u8)
    }
}

/// Active canned cycle variant.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CycleType {
    /// `G73`: high-speed chip-breaking peck drilling.
    ChipBreakPeck,
    /// `G81`: plain drilling.
    Drill,
    /// `G82`: drilling with bottom dwell.
    DwellDrill,
    /// `G83`: deep-hole full-retract peck drilling.
    DeepPeck,
    /// `G84`: right-hand tapping.
    Tap,
    /// `G85`: reaming / boring feed-in, feed-out.
    Ream,
    /// `G86`: boring feed-in, spindle stop, rapid retract.
    Bore,
    /// `G89`: boring feed-in, bottom dwell, feed-out.
    BackBore,
}

/// Active modal Group 1 motion state.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum MotionMode {
    /// Cancelled (`G80`) or unset before the first Group 1 code.
    #[default]
    None,
    /// `G0`: rapid positioning.
    Rapid,
    /// `G1`: linear interpolation at active feed rate.
    Linear,
    /// `G2`: clockwise circular/helical arc.
    ArcCw,
    /// `G3`: counterclockwise circular/helical arc.
    ArcCcw,
    /// `G73`/`G81`..`G89`: modal canned drilling/boring/tapping cycle.
    Cycle(CycleType),
}

/// One `G4` dwell recorded in the program.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DwellEvent {
    pub line: usize,
    pub seconds: f64,
    pub section: usize,
    pub after_move: Option<usize>,
}

/// `M0` program stop or `M1` optional stop.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StopKind {
    ProgramStop,
    OptionalStop,
}

/// One `M0`/`M1` stop recorded in the program.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StopEvent {
    pub line: usize,
    pub kind: StopKind,
    pub section: usize,
    pub after_move: Option<usize>,
}

/// `M2` or `M30` program end marker.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProgramEnd {
    M2,
    M30,
}

/// One move-to-source-line mapping entry (1-based line number).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MoveLine {
    pub section: usize,
    pub move_index: usize,
    pub line: usize,
}

/// Mapping between emitted `cl::Move` indices and 1-based NC source line numbers.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LineMap {
    pub entries: Vec<MoveLine>,
}

impl LineMap {
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// 1-based line number for `(section, move_index)`.
    #[must_use]
    pub fn line_for(&self, section: usize, move_index: usize) -> Option<usize> {
        self.entries
            .iter()
            .find(|e| e.section == section && e.move_index == move_index)
            .map(|e| e.line)
    }

    /// All `(section, move_index)` pairs emitted by 1-based `line`.
    pub fn moves_on_line(&self, line: usize) -> impl Iterator<Item = (usize, usize)> + '_ {
        self.entries
            .iter()
            .filter(move |e| e.line == line)
            .map(|e| (e.section, e.move_index))
    }
}

/// One section of a parsed NC program: a run of moves with one tool, work offset,
/// length offset, spindle and coolant state. A change of any of these after a move
/// starts a new section, so consecutive sections can have the same tool.
#[derive(Clone, Debug, PartialEq)]
pub struct ParsedSection {
    pub index: usize,
    pub tool_number: Option<u32>,
    pub work_offset: WorkOffset,
    pub length_offset: Option<u32>,
    pub spindle_rpm: Option<f64>,
    pub spindle_dir: SpindleDir,
    pub coolant: CoolantState,
    pub toolpath: Toolpath,
    /// 1-based NC line number aligned 1-to-1 with `toolpath.moves`.
    pub move_lines: Vec<usize>,
}

/// Complete parsed result for a 3-axis RS-274/ISO 6983 program.
#[derive(Clone, Debug, PartialEq)]
pub struct ParsedProgram {
    pub program_number: Option<u32>,
    pub sections: Vec<ParsedSection>,
    pub line_map: LineMap,
    pub dwells: Vec<DwellEvent>,
    pub stops: Vec<StopEvent>,
    pub ended: Option<ProgramEnd>,
    pub tools: Vec<u32>,
    pub final_modal: ModalState,
}

impl ParsedProgram {
    /// Iterator over each section's `cl::Toolpath`.
    pub fn toolpaths(&self) -> impl Iterator<Item = &Toolpath> + '_ {
        self.sections.iter().map(|s| &s.toolpath)
    }

    /// 1-based NC line number for `(section, move_index)`.
    #[must_use]
    pub fn line_for_move(&self, section: usize, move_index: usize) -> Option<usize> {
        self.sections
            .get(section)
            .and_then(|s| s.move_lines.get(move_index).copied())
    }
}
