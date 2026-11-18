//! Provenance: the public values of sketch chaining - options, identities, chains, rejections
//! and the named errors. Our own design for issue 1861; no vendor material consulted.

use crate::dxf::Primitive;
use crate::geo2d::Point2;
use std::fmt;

/// Default join tolerance in millimetres.
pub const DEFAULT_JOIN_TOLERANCE: f64 = 0.01;
/// The smallest join tolerance: above the identity quantum (1e-6 mm), so two joints of one
/// chain never share a quantized position.
pub const MIN_JOIN_TOLERANCE: f64 = 1e-5;
/// The largest join tolerance, in millimetres.
pub const MAX_JOIN_TOLERANCE: f64 = 10.0;
/// Input size limit; above it the call is refused.
pub const MAX_PRIMITIVES: usize = 2_000_000;

#[derive(Clone, Debug, PartialEq, thiserror::Error)]
pub enum ChainError {
    #[error("sketch chaining: join tolerance must be between 1e-5 mm and 10 mm")]
    InvalidTolerance,
    #[error("sketch chaining: primitive {primitive} has a non-finite value")]
    NonFinite { primitive: usize },
    #[error("sketch chaining: primitive {primitive} lies outside the supported coordinate range")]
    OutOfRange { primitive: usize },
    #[error("sketch chaining: primitive {primitive} is invalid: {reason}")]
    InvalidPrimitive {
        primitive: usize,
        reason: &'static str,
    },
    #[error(
        "sketch chaining: the drawing is too large or too dense to chain within the work limit"
    )]
    TooComplex,
    #[error("sketch chaining: primitive {primitive} repeats primitive {of} on the same layer")]
    DuplicatePrimitive { primitive: usize, of: usize },
    #[error("sketch chaining: primitive {primitive} is shorter than the join tolerance")]
    DegenerateSegment { primitive: usize },
    #[error("sketch chaining: {degree} curve ends meet at ({}, {}); a chain cannot branch", at.x, at.y)]
    Branch { at: Point2, degree: usize },
    #[error("sketch chaining: closed loop crosses or touches itself at ({}, {})", at.x, at.y)]
    SelfCrossing { at: Point2 },
    #[error("sketch chaining: closed loop encloses no area")]
    ZeroArea,
    #[error("sketch chaining: chain repeats chain {of}")]
    DuplicateChain { of: ChainId },
    #[error("sketch chaining: nesting is ambiguous: chain {other} crosses or touches a loop at ({}, {})", at.x, at.y)]
    NestingAmbiguous { other: ChainId, at: Point2 },
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ChainOptions {
    /// Ends closer than this (mm) are joined; a wider gap is never bridged.
    pub join_tolerance: f64,
}

impl Default for ChainOptions {
    fn default() -> Self {
        ChainOptions {
            join_tolerance: DEFAULT_JOIN_TOLERANCE,
        }
    }
}

/// Identity of a chain: a hash of its layer name and its quantized curves in canonical order.
/// It does not depend on the order or direction the primitives were drawn in.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ChainId(pub u128);

impl fmt::Display for ChainId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:032x}", self.0)
    }
}

/// One input primitive in walk direction.
#[derive(Clone, Debug, PartialEq)]
pub struct ChainSegment {
    /// Index of the primitive in the input slice.
    pub primitive: usize,
    /// The analytic curve, turned to the walk direction (an arc keeps centre and radius; its
    /// sweep sign says clockwise or counterclockwise).
    pub curve: Primitive,
    /// The walk runs against the drawn direction.
    pub reversed: bool,
    pub start: Point2,
    pub end: Point2,
}

/// A chain with two free ends. It starts at its lower end in (x, y).
#[derive(Clone, Debug, PartialEq)]
pub struct OpenChain {
    pub id: ChainId,
    pub layer: String,
    pub segments: Vec<ChainSegment>,
    pub start: Point2,
    pub end: Point2,
    /// The widest joint gap inside the chain, at most the join tolerance.
    pub max_gap: f64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LoopRole {
    /// Inside an even number of other loops: material lies inside it.
    Outer,
    /// Inside an odd number of other loops: a hole in its parent.
    Hole,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Nesting {
    pub role: LoopRole,
    /// How many closed chains contain this one.
    pub depth: usize,
    /// The innermost chain that contains this one.
    pub parent: Option<ChainId>,
}

/// A closed loop, walked counterclockwise from its lowest joint in (x, y).
#[derive(Clone, Debug, PartialEq)]
pub struct ClosedChain {
    pub id: ChainId,
    pub layer: String,
    pub segments: Vec<ChainSegment>,
    /// Enclosed area in mm^2, arcs included exactly.
    pub area: f64,
    /// The widest joint gap inside the loop, at most the join tolerance.
    pub max_gap: f64,
    /// Outer boundary or hole; `NestingAmbiguous` when this loop or one around it crosses or
    /// touches another closed chain, so no reading is safe.
    pub nesting: Result<Nesting, ChainError>,
}

/// Primitives that form no usable chain, and why.
#[derive(Clone, Debug, PartialEq)]
pub struct Rejected {
    pub layer: String,
    /// Input indices, ascending.
    pub primitives: Vec<usize>,
    pub error: ChainError,
}

/// Every chain of the input, each list sorted by layer name, then start position, then id.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SketchChains {
    pub closed: Vec<ClosedChain>,
    pub open: Vec<OpenChain>,
    pub rejected: Vec<Rejected>,
}

impl SketchChains {
    /// The chains, or the first rejection or ambiguous nesting as an error.
    pub fn strict(self) -> Result<SketchChains, ChainError> {
        if let Some(rejected) = self.rejected.first() {
            return Err(rejected.error.clone());
        }
        if let Some(error) = self
            .closed
            .iter()
            .find_map(|chain| chain.nesting.as_ref().err())
        {
            return Err(error.clone());
        }
        Ok(self)
    }
}
