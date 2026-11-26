//! Public input, error, and report types for 2D true-shape nesting.
//! Provenance: our own domain types specced for #2158 from published nesting literature (Bennell & Oliveira 2008; Burke et al. 2006).

use crate::geo2d::{GeoError, Loop, Point2, Region};
use std::collections::BTreeMap;
use thiserror::Error;

/// Spacing rule between adjacent cuts on a sheet.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ClearanceMode {
    /// Separate kerf per part: uncut material between adjacent cut edges is at least `part_gap`.
    #[default]
    SeparateCuts,
    /// Router single-slot mode (`part_gap >= cut_width`): adjacent `Outside` profiles share a single cutter-width channel.
    RouterSingleSlot,
}

/// Default side on which the cutter runs relative to a profile loop.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CutSide {
    #[default]
    Outside,
    OnLine,
    Inside,
}

/// Allowed rotation angles for placing a part.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum RotationMode {
    /// Keep 0 degrees only (e.g. grain direction or printed face).
    Locked,
    /// Allow 0, 90, 180, and 270 degree orientations.
    #[default]
    Orthogonal,
    /// Allow increments of `deg` degrees in `[0, 360)`.
    FixedStep(f64),
}

/// Rectangular sheet specification and cut-clearance parameters (millimetres).
#[derive(Clone, Debug, PartialEq)]
pub struct SheetInput {
    pub width: f64,
    pub height: f64,
    pub edge_margin: f64,
    /// Kerf width (waterjet/plasma/laser) or cutter diameter (router).
    pub cut_width: f64,
    pub part_gap: f64,
    pub allow_parts_in_holes: bool,
    pub clearance_mode: ClearanceMode,
}

impl Default for SheetInput {
    fn default() -> Self {
        SheetInput {
            width: 0.0,
            height: 0.0,
            edge_margin: 0.0,
            cut_width: 0.0,
            part_gap: 0.0,
            allow_parts_in_holes: true,
            clearance_mode: ClearanceMode::SeparateCuts,
        }
    }
}

/// One 2D part definition with quantity, geometry, cut side, and orientation rules.
#[derive(Clone, Debug, PartialEq)]
pub struct PartInput {
    pub id: String,
    pub quantity: usize,
    pub outer: Option<Loop>,
    pub holes: Vec<Loop>,
    pub open_chains: Vec<Vec<Point2>>,
    pub cut_side: CutSide,
    pub rotation_mode: RotationMode,
    pub allow_mirror: bool,
}

impl PartInput {
    /// Create a closed part with an outer loop and optional hole loops.
    pub fn closed(id: impl Into<String>, quantity: usize, outer: Loop, holes: Vec<Loop>) -> Self {
        PartInput {
            id: id.into(),
            quantity,
            outer: Some(outer),
            holes,
            open_chains: Vec::new(),
            cut_side: CutSide::Outside,
            rotation_mode: RotationMode::Orthogonal,
            allow_mirror: false,
        }
    }

    /// Create a standalone open-chain part with no closed outer loop.
    pub fn open(id: impl Into<String>, quantity: usize, open_chains: Vec<Vec<Point2>>) -> Self {
        PartInput {
            id: id.into(),
            quantity,
            outer: None,
            holes: Vec::new(),
            open_chains,
            cut_side: CutSide::OnLine,
            rotation_mode: RotationMode::Orthogonal,
            allow_mirror: false,
        }
    }

    pub fn with_cut_side(mut self, cut_side: CutSide) -> Self {
        self.cut_side = cut_side;
        self
    }

    pub fn with_rotation(mut self, rotation_mode: RotationMode) -> Self {
        self.rotation_mode = rotation_mode;
        self
    }

    pub fn with_mirror(mut self, allow_mirror: bool) -> Self {
        self.allow_mirror = allow_mirror;
        self
    }

    pub fn with_open_chains(mut self, open_chains: Vec<Vec<Point2>>) -> Self {
        self.open_chains = open_chains;
        self
    }
}

#[derive(Clone, Error, Debug, PartialEq)]
pub enum NestError {
    #[error("invalid nesting input: {0}")]
    InvalidInput(String),
    #[error("nesting input out of machinable range: {0}")]
    OutOfRange(String),
    #[error("invalid part '{part_id}': {reason}")]
    InvalidPart { part_id: String, reason: String },
    #[error("nesting geometry error: {0}")]
    Geometry(#[from] GeoError),
    #[error("unplaced parts remain: {0}")]
    Unplaced(String),
}

/// One placed instance on a sheet.
#[derive(Clone, Debug, PartialEq)]
pub struct PlacedPart {
    pub part_id: String,
    pub instance_index: usize,
    pub tx: f64,
    pub ty: f64,
    pub angle_deg: f64,
    pub mirrored: bool,
    /// Set when placed inside a closed hole of another placed part `(host_id, host_instance)`.
    pub container_part: Option<(String, usize)>,
    pub net_area: f64,
    pub outer: Option<Loop>,
    pub holes: Vec<Loop>,
    pub open_chains: Vec<Vec<Point2>>,
    pub cut_footprint: Region,
    pub inflated_footprint: Region,
}

/// Nesting output for one physical sheet.
#[derive(Clone, Debug, PartialEq)]
pub struct SheetPlacement {
    pub sheet_index: usize,
    pub placements: Vec<PlacedPart>,
    pub placed_counts: BTreeMap<String, usize>,
    pub utilization_pct: f64,
    pub waste_region: Region,
    pub used_bounds: Option<(Point2, Point2)>,
    pub usable_sheet: Region,
}

/// Summary of parts that could not fit on any sheet, with named reasons.
#[derive(Clone, Debug, PartialEq)]
pub struct UnplacedPart {
    pub part_id: String,
    pub quantity: usize,
    pub reason: String,
}

/// Full deterministic nesting report across all allocated sheets.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct NestReport {
    pub sheet_count: usize,
    pub sheets: Vec<SheetPlacement>,
    pub total_requested: usize,
    pub total_placed: usize,
    pub requested_counts: BTreeMap<String, usize>,
    pub placed_counts: BTreeMap<String, usize>,
    pub overall_utilization_pct: f64,
    pub unplaced: Vec<UnplacedPart>,
}

impl NestReport {
    /// Return `Err(NestError::Unplaced)` naming the first unplaced part if any remain.
    pub fn strict(self) -> Result<Self, NestError> {
        if let Some(first) = self.unplaced.first() {
            return Err(NestError::Unplaced(first.reason.clone()));
        }
        Ok(self)
    }
}
