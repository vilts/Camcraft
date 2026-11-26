//! Provenance: Jordan-curve containment and post-order traversal; CamCraft plasma nesting and issue #2159 hold-down rules.
//!
//! Inputs are already placed, polygonal features in sheet millimeters. This
//! module orders cuts; placement clearance and offset toolpaths remain the
//! caller's responsibility. Diagnostics describe ideal sharp corners that
//! a circular cutter cannot reach. They do not change the picked geometry.

use crate::geo2d::{COORD_LIMIT, GeoError, Loop, Point2, SCALE};

mod geometry;
mod validate;

/// A side is relative to the contour interior, or to the directed open chain.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CutSide {
    Auto,
    Outside,
    OnLine,
    Inside,
    Left,
    Right,
}

/// Global sheet phases. Marking and RoughProfile retain stock. Internal may
/// contain blind pockets or through holes; release_phase distinguishes them.
/// Breakthrough is always a through cut on a closed contour.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum CutPhase {
    Marking,
    Internal,
    RoughProfile,
    Breakthrough,
}

impl CutPhase {
    const ALL: [Self; 4] = [
        Self::Marking,
        Self::Internal,
        Self::RoughProfile,
        Self::Breakthrough,
    ];
}

#[derive(Clone, Debug, PartialEq)]
pub enum CutGeometry {
    Closed(Loop),
    Open(Vec<Point2>),
}

/// Stable IDs must be unique across the sheet. Request each phase at most once.
/// Multiple physical passes within a phase remain the caller's responsibility.
#[derive(Clone, Debug, PartialEq)]
pub struct CutFeature {
    pub id: u64,
    pub geometry: CutGeometry,
    pub side: CutSide,
    pub passes: Vec<CutPhase>,
    /// The through pass that frees material. None means all passes retain
    /// stock. Only a closed Internal or Breakthrough pass can release stock.
    pub release_phase: Option<CutPhase>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CutRole {
    PartOutline,
    Hole,
    OpenFeature,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResolvedFeature {
    pub id: u64,
    pub parent: Option<u64>,
    pub depth: usize,
    pub role: CutRole,
    /// Always explicit: the planner never returns Auto.
    pub side: CutSide,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlannedCut {
    pub feature_id: u64,
    pub phase: CutPhase,
}

/// A sharp corner needs at least tool_radius of rounding on the cutter side.
#[derive(Clone, Debug, PartialEq)]
pub struct CornerClearance {
    pub feature_id: u64,
    /// Index into the original input vertices (no winding reversal occurs).
    pub vertex: usize,
    pub tool_radius: f64,
    /// Angle of the wedge on the cutter side, between zero and pi.
    pub angle_radians: f64,
    /// Distance left uncut from the ideal corner along its angle bisector.
    pub min_unreachable_distance: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SheetCutPlan {
    /// Sorted by stable ID, independent of input enumeration.
    pub features: Vec<ResolvedFeature>,
    pub cuts: Vec<PlannedCut>,
    pub corner_limits: Vec<CornerClearance>,
}

fn invalid(message: impl Into<String>) -> GeoError {
    GeoError::Invalid(message.into())
}

fn resolve(
    features: &[CutFeature],
    nodes: &[geometry::Node],
) -> Result<Vec<ResolvedFeature>, GeoError> {
    let mut resolved = Vec::with_capacity(features.len());
    for (feature, node) in features.iter().zip(nodes) {
        let role = if !node.closed {
            CutRole::OpenFeature
        } else if node.depth % 2 == 0 {
            CutRole::PartOutline
        } else {
            CutRole::Hole
        };
        let side = match (node.closed, feature.side) {
            (false, CutSide::Inside | CutSide::Outside) => {
                return Err(invalid(format!(
                    "open chain {} cannot use {:?}",
                    feature.id, feature.side
                )));
            }
            (true, CutSide::Left | CutSide::Right) => {
                return Err(invalid(format!(
                    "closed contour {} cannot use {:?}",
                    feature.id, feature.side
                )));
            }
            (false, CutSide::Auto) => CutSide::OnLine,
            (true, CutSide::Auto) if role == CutRole::Hole => CutSide::Inside,
            (true, CutSide::Auto) => CutSide::Outside,
            (_, side) => side,
        };
        resolved.push(ResolvedFeature {
            id: feature.id,
            parent: node.parent.map(|p| features[p].id),
            depth: node.depth,
            role,
            side,
        });
    }
    Ok(resolved)
}

/// Construct the global forest, then visit siblings by increasing area with
/// stable IDs breaking ties. Traversal uses a stack, including for deep nesting.
fn post_order(features: &[CutFeature], nodes: &[geometry::Node]) -> Vec<usize> {
    let mut roots = Vec::new();
    let mut children = vec![Vec::new(); nodes.len()];
    for (i, node) in nodes.iter().enumerate() {
        if let Some(parent) = node.parent {
            children[parent].push(i);
        } else {
            roots.push(i);
        }
    }
    let sort = |list: &mut Vec<usize>| {
        list.sort_by_key(|&i| (nodes[i].area2.unsigned_abs(), features[i].id));
    };
    sort(&mut roots);
    for list in &mut children {
        sort(list);
    }
    let mut stack: Vec<_> = roots.into_iter().rev().map(|i| (i, false)).collect();
    let mut out = Vec::with_capacity(nodes.len());
    while let Some((i, visited)) = stack.pop() {
        if visited {
            out.push(i);
        } else {
            stack.push((i, true));
            stack.extend(children[i].iter().rev().map(|&child| (child, false)));
        }
    }
    out
}

/// Plan one sheet. Refuse ambiguous geometry and phase requests that would cut
/// a feature after its enclosing contour has freed the material.
pub fn plan_sheet(features: &[CutFeature], cut_width: f64) -> Result<SheetCutPlan, GeoError> {
    if !cut_width.is_finite() || !(1.0 / SCALE..COORD_LIMIT).contains(&cut_width) {
        return Err(invalid(
            "cut_width must be finite, at least one geometry grid unit, and within range",
        ));
    }
    // Resolve IDs and pass requests before the geometric pair checks so duplicate
    // IDs always get their own diagnostic, even when their geometry also overlaps.
    validate::requests(features)?;
    let nodes = geometry::forest(features)?;
    let mut resolved = resolve(features, &nodes)?;
    let order = post_order(features, &nodes);
    let mut cuts = Vec::new();
    for phase in CutPhase::ALL {
        for &i in &order {
            if features[i].passes.contains(&phase) {
                cuts.push(PlannedCut {
                    feature_id: features[i].id,
                    phase,
                });
            }
        }
    }
    validate::order(features, &nodes, &cuts)?;
    let mut corner_limits = Vec::new();
    for (i, node) in nodes.iter().enumerate() {
        corner_limits.extend(geometry::corners(
            features[i].id,
            node,
            resolved[i].side,
            cut_width / 2.0,
        ));
    }
    resolved.sort_by_key(|feature| feature.id);
    corner_limits.sort_by_key(|corner| (corner.feature_id, corner.vertex));
    Ok(SheetCutPlan {
        features: resolved,
        cuts,
        corner_limits,
    })
}

/// Check a caller-supplied schedule, including completeness and global phases.
/// Safe sibling permutations are accepted; descendant cuts after release are not.
pub fn validate_cut_order(features: &[CutFeature], cuts: &[PlannedCut]) -> Result<(), GeoError> {
    validate::requests(features)?;
    let nodes = geometry::forest(features)?;
    resolve(features, &nodes)?;
    validate::order(features, &nodes, cuts)
}
