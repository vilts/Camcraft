//! Circle centre from a picked edge (#739): the shared pick primitive the
//! WCS page anchors on, and setup alignment (#738), WCS roles (#701),
//! probing and hole features reuse. An edge pick resolves to its run (one
//! straight stretch or one arc of the tangent chain); a run whose every
//! vertex lies on one circle, in one plane, within the feature graph's
//! arc tolerance gives back its centre, axis and radius. Anything else -
//! a straight edge, a run too short to call round, a curve that is not a
//! circle (an ellipse, a spline, an open polyline) - is refused by name.
//! Never a guessed centre.
//!
//! Provenance: the circumcircle of three points checked against every
//! vertex (`feature_arc::fit_arc`, textbook analytic geometry) and the
//! distance of a point from a line (standard vector form); our own code
//! over this repo's feature runs, no vendor material consulted.

use flecs_ecs::prelude::{Entity, World};

use crate::geometry::feature::{self, Chain, Features};
use crate::geometry::{Point3, quat_rotate_point, transform_point};
use crate::model;
use crate::operation::{FeatureSel, GeoSel};
use crate::schema::Location;

use super::feature_arc::{ARC_ABS_TOL, ARC_MIN_EDGES, ARC_REL_TOL, fit_arc};
use super::feature_graph::FeatureGraph;
use super::feature_sel::graph_of;

/// A circle (or circular arc) read off a picked edge.
#[derive(Clone, Debug, PartialEq)]
pub struct Circle {
    /// The circle's centre, at the edge's own height (world frame for
    /// [`circle_centre`], the mesh's local frame for [`circle_of_run`]).
    pub centre: [f64; 3],
    /// Unit axis (the normal of the circle's plane). Its sign is
    /// canonical, not the walk's: the largest component is positive, so a
    /// hole in a flat top face reads +Z.
    pub axis: [f64; 3],
    pub radius: f64,
    /// The run closes on itself: a full circle, not an arc.
    pub closed: bool,
}

impl Circle {
    /// Twice the radius.
    #[must_use]
    pub fn diameter(&self) -> f64 {
        2.0 * self.radius
    }
}

/// Why a pick gives no circle centre - each one named, never a guess.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CircleRefusal {
    /// The picked entity or feature no longer resolves.
    Gone(String),
    /// The pick is a face or a vertex, not an edge.
    NotAnEdge(String),
    /// The edge is straight.
    Straight,
    /// The run has too few segments to call it round.
    TooShort { edges: usize },
    /// The run curves but its vertices do not lie on one planar circle.
    NotCircular,
}

impl std::fmt::Display for CircleRefusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CircleRefusal::Gone(what) => write!(f, "{what} is no longer in the model - re-pick it"),
            CircleRefusal::NotAnEdge(what) => write!(f, "{what} is not an edge - pick a circular edge"),
            CircleRefusal::Straight => f.write_str("the picked edge is straight - pick a circular edge"),
            CircleRefusal::TooShort { edges } => write!(
                f,
                "the picked edge has {edges} segment(s) - at least {ARC_MIN_EDGES} are needed to call it round"
            ),
            CircleRefusal::NotCircular => f.write_str(
                "the picked edge is not circular within tolerance (an ellipse, a spline or an uneven polyline)",
            ),
        }
    }
}

impl std::error::Error for CircleRefusal {}

fn sub(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

/// Every point within `tol` of the line through the run's two farthest-
/// apart samples (its first point and the one farthest from it).
fn is_straight(pts: &[[f64; 3]]) -> bool {
    let Some(&a) = pts.first() else { return true };
    let far = pts.iter().copied().max_by(|p, q| dot(sub(*p, a), sub(*p, a)).total_cmp(&dot(sub(*q, a), sub(*q, a))));
    let Some(b) = far else { return true };
    let d = sub(b, a);
    let len2 = dot(d, d);
    if !(len2.is_finite() && len2 > 0.0) {
        return true; // every point coincides: no curvature to speak of
    }
    let tol = (len2.sqrt() * ARC_REL_TOL).max(ARC_ABS_TOL);
    pts.iter().all(|p| {
        let r = sub(*p, a);
        let t = dot(r, d) / len2;
        let foot = sub(r, [d[0] * t, d[1] * t, d[2] * t]);
        dot(foot, foot).sqrt() <= tol
    })
}

/// Canonical axis sign: the largest-magnitude component positive.
fn canonical(n: [f64; 3]) -> [f64; 3] {
    let k = (0..3).max_by(|&i, &j| n[i].abs().total_cmp(&n[j].abs())).unwrap_or(2);
    if n[k] < 0.0 { [-n[0], -n[1], -n[2]] } else { n }
}

/// The circle a run of `features` lies on, local frame.
pub fn circle_of_run(features: &Features, run: &Chain) -> Result<Circle, CircleRefusal> {
    let mut pts = feature::chain_points(features, run);
    if pts.len() < 2 {
        return Err(CircleRefusal::Gone(String::from("the picked edge")));
    }
    if run.closed
        && let Some(&first) = pts.first()
    {
        pts.push(first); // `fit_arc` walks a closed run back to its start
    }
    if is_straight(&pts) {
        return Err(CircleRefusal::Straight);
    }
    if run.edges.len() < ARC_MIN_EDGES {
        return Err(CircleRefusal::TooShort { edges: run.edges.len() });
    }
    let arc = fit_arc(&pts, run.closed, run.edges.clone()).ok_or(CircleRefusal::NotCircular)?;
    Ok(Circle { centre: arc.centre, axis: canonical(arc.normal), radius: arc.radius, closed: arc.closed })
}

/// The circle of the run holding feature edge `edge` (an index into
/// `features.edges`, as [`super::PickTarget::Edge`] carries), local frame.
pub fn circle_of_edge(graph: &FeatureGraph, edge: u32) -> Result<Circle, CircleRefusal> {
    let run = graph
        .runs()
        .iter()
        .find(|r| r.edges.contains(&edge))
        .ok_or_else(|| CircleRefusal::Gone(String::from("the picked edge")))?;
    circle_of_run(&graph.features, run)
}

/// The circle centre of a stored edge pick (a `Chain` selection: a run or
/// a whole tangent chain), in the frame the viewport draws and picks in
/// (the active setup's, #326) - the frame the WCS origin is set in.
pub fn circle_centre(world: &World, pick: &GeoSel) -> Result<Circle, CircleRefusal> {
    circle_centre_in(world, pick, crate::setup_align::view_pose)
}

/// [`circle_centre`] placed by `pose` instead of the view's: the document
/// frame (`model::transform_of`) for the alignment picks that read their
/// points as modelled (#692 centre points).
pub fn circle_centre_in(
    world: &World,
    pick: &GeoSel,
    pose: impl Fn(&World, &Entity) -> Option<Location>,
) -> Result<Circle, CircleRefusal> {
    let FeatureSel::Chain(id) = pick.feature else {
        return Err(CircleRefusal::NotAnEdge(format!("{} of '{}'", pick.feature, pick.entity)));
    };
    let gone = || CircleRefusal::Gone(format!("{} of '{}'", pick.feature, pick.entity));
    let e = model::find_by_name(world, &pick.entity).ok_or_else(gone)?;
    let graph = graph_of(world, &pick.entity).ok_or_else(gone)?;
    let run = feature::resolve_chain(&graph.features, id).ok_or_else(gone)?;
    let local = circle_of_run(&graph.features, &run)?;
    let loc = pose(world, &e).unwrap_or_default();
    let c = transform_point(Point3::new(local.centre[0], local.centre[1], local.centre[2]), loc.pos, loc.rot);
    let n = quat_rotate_point(loc.rot, Point3::new(local.axis[0], local.axis[1], local.axis[2]));
    let len = n.dot(n).sqrt();
    let centre = [c.x, c.y, c.z];
    if !(centre.iter().all(|v| v.is_finite()) && len.is_finite() && len > 0.0) {
        return Err(CircleRefusal::NotCircular);
    }
    Ok(Circle { centre, axis: canonical([n.x / len, n.y / len, n.z / len]), radius: local.radius, closed: local.closed })
}
