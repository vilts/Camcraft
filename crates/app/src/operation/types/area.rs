//! The area controls of an operation (M11.5): the selection says WHERE,
//! these say HOW FAR - tool containment against the boundary, a grow /
//! shrink offset, an extension past exterior edges, extra include /
//! exclude boundaries, and an open (or closed) boundary with a side.
//! Plain data only; the law that applies them is
//! `operation::boundary`, the world reads are `operation::boundary_ctx`.
//!
//! Provenance: our own design notes on 2.5D machining boundaries (tool
//! inside / on / outside a boundary and offset-with-corner-style are the
//! textbook vocabulary of any machining handbook's 2D toolpath chapter);
//! no vendor material consulted.

use super::params::OpParams;
use super::sel::{GeoSel, OpType};

/// Where the tool may stand relative to the boundary. `None` on
/// [`AreaControls::containment`] is the kind's own law, unchanged.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Containment {
    /// The whole tool stays inside the boundary (its edge touches it).
    Inside,
    /// The tool centre rides the boundary.
    On,
    /// The tool passes the boundary until its edge touches it from
    /// outside - the area is cleared past its own edge.
    Outside,
}

/// Corner style of a grow / shrink offset.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CornerStyle {
    /// Arcs round the convex corners - the offset's exact law.
    #[default]
    Round,
    /// Corners stay sharp (mitred), as a rectangle grows into a rectangle.
    Sharp,
}

/// Whether an extra boundary adds to the area or keeps the tool out.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BoundaryMode {
    Include,
    Exclude,
}

/// What an extra boundary is drawn from.
#[derive(Clone, Debug, PartialEq)]
pub enum BoundaryShape {
    /// Picked faces / closed edge chains (their XY loops).
    Picks(Vec<GeoSel>),
    /// The setup stock's outline.
    Stock,
    /// The silhouette of the operation's source part.
    Silhouette,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ExtraBoundary {
    pub mode: BoundaryMode,
    pub shape: BoundaryShape,
}

/// Which side of an open (or closed) boundary is machined. Inside is the
/// side that holds more of the operation's own area (what the selection
/// or source encloses) - the part's side; Outside the other one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChainSide {
    Inside,
    Outside,
}

/// A boundary the user draws instead of the selection's own: an open
/// chain (closed off by the stock outline) or a closed ring.
#[derive(Clone, Debug, PartialEq)]
pub struct OpenBoundary {
    pub picks: Vec<GeoSel>,
    pub side: ChainSide,
}

/// The area controls. `Default` is the identity: every region, every
/// stamp byte and every saved word is exactly what it was before M11.5.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct AreaControls {
    pub containment: Option<Containment>,
    /// Grow (+) or shrink (-) the boundary, mm.
    pub offset: f64,
    pub corner: CornerStyle,
    /// Extend past exterior edges (edges with no part material beyond
    /// them), mm; 0 is off.
    pub extension: f64,
    pub extra: Vec<ExtraBoundary>,
    pub open: Option<OpenBoundary>,
}

/// Largest offset / extension the door accepts, mm: far past any
/// machine table, far inside the region kernel's horizon.
pub const AREA_DISTANCE_MAX: f64 = 10_000.0;

impl AreaControls {
    /// The identity - nothing stamped, nothing saved, nothing changed.
    #[must_use]
    pub fn is_default(&self) -> bool {
        *self == AreaControls::default()
    }

    /// Door sanity, no world: finite in-range distances, no empty pick
    /// list, and nothing but the identity on a kind without an area.
    #[must_use]
    pub fn is_sane_for(&self, params: &OpParams) -> bool {
        if self.is_default() {
            return true;
        }
        if !area_applies(params.kind()) {
            return false;
        }
        let dist = |v: f64| v.is_finite() && v.abs() <= AREA_DISTANCE_MAX;
        dist(self.offset)
            && dist(self.extension)
            && self.extension >= 0.0
            && self.extra.iter().all(|b| match &b.shape {
                BoundaryShape::Picks(p) => !p.is_empty(),
                BoundaryShape::Stock | BoundaryShape::Silhouette => true,
            })
            && self.open.as_ref().is_none_or(|o| !o.picks.is_empty())
    }

    /// Every pick the controls reference (boundaries and the open one),
    /// for the stamp and the stale-reason walk.
    pub fn picks(&self) -> impl Iterator<Item = &GeoSel> {
        self.extra
            .iter()
            .filter_map(|b| match &b.shape {
                BoundaryShape::Picks(p) => Some(p.iter()),
                _ => None,
            })
            .flatten()
            .chain(self.open.iter().flat_map(|o| o.picks.iter()))
    }
}

/// The kinds whose region IS the area the tool works in, so the area
/// controls mean something: the facing and clearing kinds, the contour
/// ride, the 3D pocket ladder, the 3D finishing family (every
/// pattern of `Finish3D` - its path is clipped to the area) and the
/// outline rides Trace / Engrave (tool centre on the chain). Every
/// other kind refuses non-default controls by name: [`area_unsupported`]
/// is the one list, and this is its `is_none`.
#[must_use]
pub fn area_applies(kind: OpType) -> bool {
    area_unsupported(kind).is_none()
}

/// Why the Boundary settings cannot apply to `kind` - the reason the
/// page shows, the Apply door and the generator refuse with - or `None`
/// for a kind with a machining area the boundary law can clip. One
/// source of truth for app and UI alike (M11.5 owed).
#[must_use]
pub fn area_unsupported(kind: OpType) -> Option<String> {
    Some(match kind {
        OpType::Face
        | OpType::Contour
        | OpType::Pocket
        | OpType::Adaptive2D
        | OpType::Offset2D
        | OpType::Pocket3D
        | OpType::Finish3D
        // The outline rides (M13.10): the tool centre ON the chain is
        // their native, so containment, offset and the extra boundaries
        // move or shape the ring they ride (`Native::ToolOn`).
        | OpType::Trace
        | OpType::Engrave => return None,
        // Authored geometry: the numbers ARE the area.
        // #1504: a Slot's picks (two opposite edges or a face) seed its
        // endpoints and width and bound where it may cut.
        OpType::Slot => format!("a {kind} is authored by its two endpoints and width, seeded from two picked opposite edges or a face and kept inside them - move those instead of a boundary"),
        OpType::TrochoidalSlot => format!("a {kind} is authored by its two endpoints and width - move those instead of a boundary"),
        OpType::Circular => format!("a {kind} is authored by its centre and radius, typed or fitted to a picked round rim or face - set those instead of a boundary"),
        // The cone's ride is the wall itself: an offset or clipped
        // outline would be ridden where nothing was picked.
        OpType::Chamfer2D => {
            format!("a {kind}'s cone lies on the picked wall - its ride cannot be offset or clipped; pick other edges instead")
        }
        // Hole lists.
        OpType::Drill | OpType::Bore | OpType::Thread => {
            format!("{kind} rides its hole list - edit the holes instead of a boundary")
        }
        OpType::Probe => format!("a {kind} touches its picked feature - there is no area to bound"),
        OpType::PlasmaProfile => {
            format!("a {kind} cuts its picked contours through the plate - pick the contours instead of a boundary")
        }
        OpType::WireContour => {
            format!("a {kind} cuts its picked profile through the workpiece - pick the profile instead of a boundary")
        }
        OpType::Deburr => format!("{kind} breaks the model's own exposed edges - there is no area to bound"),
        OpType::SinkerBurn | OpType::SinkerSideBurn => format!("a {kind} burns its electrode's own face - there is no area to bound"),
        // Mesh-driven with a moving tool axis: the model's own shadow.
        OpType::Contour5D
        | OpType::Parallel5D
        | OpType::Swarf
        | OpType::AdvancedSwarf
        | OpType::RotaryContour
        | OpType::RotaryParallel
        | OpType::RotaryOffset
        | OpType::MultiAxisRough
        | OpType::Flow5D
        | OpType::MultiAxisContour => {
            format!("{kind} is mesh-driven with a moving tool axis - its area is the model's own shadow, not a boundary")
        }
        // The turning family: the limit is the turned silhouette.
        OpType::Facing
        | OpType::ProfileRough
        | OpType::ProfileFinish
        | OpType::GrooveRough
        | OpType::GrooveFinish
        | OpType::Part
        | OpType::Threading
        | OpType::Chamfering
        | OpType::TaperTurning
        | OpType::MultiPassTurning
        | OpType::FormTurning
        | OpType::BoringTurning => {
            format!("{kind} is a turning ride - its limit is the turned profile's own silhouette, not a boundary")
        }
    })
}
