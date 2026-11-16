//! The one pick rule (#692): a ray resolves to the front-most visible
//! surface, then snaps within a radius to a feature point, a round-edge
//! centre, a feature edge or the welded face - in that order, and only
//! among the kinds the asking tool accepts. Edges and points behind the
//! model are never picked unless the caller explicitly asks to pick
//! through.
//!
//! Provenance: ray casting with the Möller–Trumbore crossing test
//! (Möller & Trumbore, J. Graphics Tools 1997, `pick::ray_triangle`), the
//! closest point between a ray and a segment, and an occlusion ray back
//! toward the eye (or, orthographic, along the parallel view direction) -
//! general computer-graphics practice; our own code, no vendor material.

use std::sync::Arc;

use flecs_ecs::prelude::World;

use crate::geometry::{
    Point3, Triangle, quat_conjugate, quat_rotate_point, transform_inverse_point, transform_point,
};
use crate::model;
use crate::operation::FeatureSel;
use crate::schema::Location;

use super::feature_graph::{FeatureGraph, FeaturePoint, feature_graph_shared};
use super::ray_triangle;

/// How the view casts rays: perspective sight lines meet at the ray's
/// origin (the eye); orthographic sight lines are all parallel to `dir`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Projection {
    Perspective,
    Ortho,
}

/// One pick ray, in the frame the viewport draws in.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PickRay {
    pub origin: [f64; 3],
    pub dir: [f64; 3],
    pub projection: Projection,
}

/// Which kinds of feature the asking tool accepts.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PickKinds {
    pub faces: bool,
    pub edges: bool,
    pub points: bool,
    /// Centres of round feature edges (circles and arcs).
    pub centres: bool,
}

impl PickKinds {
    pub const FACES: PickKinds = PickKinds { faces: true, edges: false, points: false, centres: false };
    pub const EDGES: PickKinds = PickKinds { faces: false, edges: true, points: false, centres: false };
    pub const POINTS: PickKinds = PickKinds { faces: false, edges: false, points: true, centres: false };
    pub const CENTRES: PickKinds = PickKinds { faces: false, edges: false, points: false, centres: true };
    /// Faces, edges and points (centres stay opt-in).
    pub const ALL: PickKinds = PickKinds { faces: true, edges: true, points: true, centres: false };
}

/// One pick request.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PickQuery {
    pub ray: PickRay,
    /// Snap radius in world units (mm) at the surface hit. In perspective
    /// it scales with depth, so it stays one screen-space size.
    pub radius: f64,
    /// Perspective only (#692 step 2): when finite and above 0, the snap
    /// radius at depth `t` along the ray is `radius_per_depth * t` mm -
    /// one screen-space size at every depth, a miss included (the
    /// silhouette, where an edge pick belongs and no surface anchors
    /// `radius`). The viewport sets it from its pixel radius and field of
    /// view; 0 keeps `radius` anchored at the hit.
    pub radius_per_depth: f64,
    pub kinds: PickKinds,
    /// Ignore occlusion: pick edges and points behind the model. Off by
    /// default everywhere; only an explicit "pick through" mode sets it.
    pub through: bool,
}

/// What a pick landed on, keyed into the entity's [`FeatureGraph`].
#[derive(Clone, Debug, PartialEq)]
pub enum PickTarget {
    Point(FeaturePoint),
    /// Index into [`FeatureGraph::arcs`].
    Centre { arc: usize },
    /// Index into `features.edges` (one of [`FeatureGraph::edges`]).
    Edge { edge: u32 },
    /// The facet the ray hit and the welded face it belongs to (index into
    /// [`FeatureGraph::faces`]), with that face's selection key.
    Face { facet: usize, face: u32, sel: Option<FeatureSel> },
}

/// A resolved pick.
#[derive(Clone, Debug, PartialEq)]
pub struct FeaturePick {
    pub entity: String,
    pub target: PickTarget,
    /// The picked spot, world frame: the point, the centre, the nearest
    /// point of the edge to the ray, or the surface hit.
    pub at: [f64; 3],
    /// The front-most surface hit along the ray, if any.
    pub surface: Option<[f64; 3]>,
}

/// One pickable mesh: local facets, their graph and the placement the
/// viewport draws them at.
#[derive(Clone, Debug)]
pub struct PickBody {
    pub name: String,
    pub tris: Arc<[Triangle]>,
    pub graph: Arc<FeatureGraph>,
    pub pose: Location,
    /// Stock is translucent: it never hides a model, and a model hit
    /// beats it at any depth (the `first_hit_f` rule).
    pub stock: bool,
}

impl PickBody {
    /// A model body at `pose`, its graph from the shared cache (keyed by
    /// the facet allocation, so a per-frame call never re-hashes a mesh).
    #[must_use]
    pub fn new(name: &str, tris: Arc<[Triangle]>, pose: Location) -> PickBody {
        let graph = feature_graph_shared(&tris);
        PickBody { name: name.to_string(), tris, graph, pose, stock: false }
    }
}

/// At most this many candidates per tier pay for a sight ray (#692 step
/// 2): a dense run of hidden ones - the far rim of a finely tessellated
/// bore - would otherwise cost one full ray per chord. Past the cap the
/// tier yields to the next (an edge to the face); the loss is a visible
/// feature ranked below 32 hidden ones, never a hidden pick.
const OCCLUSION_TRIES: usize = 32;

/// Visibility slack of the occlusion ray, mm: the facets that meet AT an
/// edge or corner cross the sight line at its own distance.
fn sight_tol(len: f64) -> f64 {
    (len.abs() * 1e-4).max(1e-3)
}

fn p3(p: [f64; 3]) -> Point3 {
    Point3::new(p[0], p[1], p[2])
}

fn a3(p: Point3) -> [f64; 3] {
    [p.x, p.y, p.z]
}

fn sub(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn along(o: [f64; 3], d: [f64; 3], t: f64) -> [f64; 3] {
    [o[0] + d[0] * t, o[1] + d[1] * t, o[2] + d[2] * t]
}

pub(super) fn world_of(b: &PickBody, p: [f64; 3]) -> [f64; 3] {
    a3(transform_point(p3(p), b.pose.pos, b.pose.rot))
}

/// (depth along the unit ray, perpendicular distance) of world point `w`.
fn ray_measure(o: [f64; 3], d: [f64; 3], w: [f64; 3]) -> Option<(f64, f64)> {
    let rel = sub(w, o);
    let depth = dot(rel, d);
    let foot = sub(rel, [d[0] * depth, d[1] * depth, d[2] * depth]);
    let perp = dot(foot, foot).sqrt();
    (depth.is_finite() && perp.is_finite() && depth > 0.0).then_some((depth, perp))
}

/// The point of segment `a`-`b` nearest the ray's line (world frame).
fn segment_near_ray(o: [f64; 3], d: [f64; 3], a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    let e = sub(b, a);
    let w0 = sub(a, o);
    let (ee, ed) = (dot(e, e), dot(e, d));
    let denom = ee - ed * ed; // |d| = 1
    let u = if denom > 1e-12 * ee.max(1e-300) { (ed * dot(d, w0) - dot(e, w0)) / denom } else { 0.0 };
    let u = if u.is_finite() { u.clamp(0.0, 1.0) } else { 0.0 };
    along(a, e, u)
}

/// Does any occluding facet cross the sight segment from `p` (world)
/// back to the viewer before reaching `p`?
pub(super) fn occluded(bodies: &[PickBody], own: usize, ray: &PickRay, d: [f64; 3], p: [f64; 3]) -> bool {
    let eye = match ray.projection {
        Projection::Perspective => ray.origin,
        Projection::Ortho => along(p, d, -dot(sub(p, ray.origin), d)),
    };
    let seg = sub(p, eye);
    let len = dot(seg, seg).sqrt();
    if !(len.is_finite() && len > 1e-12) {
        return false; // the eye sits on the point: nothing in between
    }
    let cut = 1.0 - sight_tol(len) / len; // t along the segment: 1 is at p
    bodies.iter().enumerate().filter(|(i, b)| !b.stock || *i == own).any(|(_, b)| {
        let e_l = transform_inverse_point(p3(eye), b.pose.pos, b.pose.rot);
        let p_l = transform_inverse_point(p3(p), b.pose.pos, b.pose.rot);
        let dir = p_l - e_l;
        e_l.is_finite()
            && dir.is_finite()
            && b.tris.iter().any(|t| ray_triangle(*t, e_l, dir).is_some_and(|hit| hit < cut))
    })
}

/// At most this many samples of a round edge pay for a sight ray.
const ARC_SAMPLES: usize = 8;

/// Is any of the round edge's segments (sampled at their midpoints, up to
/// [`ARC_SAMPLES`] spread around the run) in sight? A hidden rim offers no
/// centre unless the caller picks through.
fn arc_seen(bodies: &[PickBody], own: usize, arc: usize, ray: &PickRay, d: [f64; 3]) -> bool {
    let Some(b) = bodies.get(own) else { return false };
    let Some(a) = b.graph.arcs.get(arc) else { return false };
    let step = a.edges.len().div_ceil(ARC_SAMPLES).max(1);
    a.edges.iter().step_by(step).filter_map(|&e| b.graph.edge_ends(e)).any(|(p, r)| {
        let mid = [(p[0] + r[0]) * 0.5, (p[1] + r[1]) * 0.5, (p[2] + r[2]) * 0.5];
        !occluded(bodies, own, ray, d, world_of(b, mid))
    })
}

/// The front-most surface hit: (body, facet, t along the unit ray).
/// Models beat stock at any depth.
fn front_hit(bodies: &[PickBody], o: [f64; 3], d: [f64; 3]) -> Option<(usize, usize, f64)> {
    let mut model: Option<(usize, usize, f64)> = None;
    let mut stock: Option<(usize, usize, f64)> = None;
    for (bi, b) in bodies.iter().enumerate() {
        let o_l = transform_inverse_point(p3(o), b.pose.pos, b.pose.rot);
        let d_l = quat_rotate_point(quat_conjugate(b.pose.rot), p3(d));
        if !(o_l.is_finite() && d_l.is_finite()) {
            continue;
        }
        let best = if b.stock { &mut stock } else { &mut model };
        for (fi, t) in b.tris.iter().enumerate() {
            if let Some(hit) = ray_triangle(*t, o_l, d_l)
                && best.is_none_or(|(_, _, bt)| hit < bt)
            {
                *best = Some((bi, fi, hit));
            }
        }
    }
    model.or(stock)
}

/// A snap candidate: (perp, depth, body, target, world position).
type Cand = (f64, f64, usize, PickTarget, [f64; 3]);

/// The pure rule over explicit bodies (what [`pick_feature`] runs on the
/// world's). `None` on a miss or on any degenerate input - never a panic.
#[must_use]
pub fn pick_bodies(bodies: &[PickBody], q: &PickQuery) -> Option<FeaturePick> {
    let (o, d0) = (q.ray.origin, q.ray.dir);
    let len = dot(d0, d0).sqrt();
    if !(o.iter().all(|v| v.is_finite()) && len.is_finite() && len > 1e-12) {
        return None;
    }
    let d = [d0[0] / len, d0[1] / len, d0[2] / len];
    let radius = if q.radius.is_finite() && q.radius > 0.0 { q.radius } else { 0.0 };
    let hit = front_hit(bodies, o, d);
    let surface = hit.map(|(_, _, t)| along(o, d, t));
    // Perspective: one screen-space radius means a world radius growing
    // with depth, anchored at the surface hit.
    let per_depth = if q.radius_per_depth.is_finite() && q.radius_per_depth > 0.0 { q.radius_per_depth } else { 0.0 };
    let reach = |depth: f64| match (q.ray.projection, hit) {
        (Projection::Perspective, _) if per_depth > 0.0 => per_depth * depth,
        (Projection::Perspective, Some((_, _, t))) if t > 0.0 => radius * depth / t,
        _ => radius,
    };
    let keep = |w: [f64; 3]| ray_measure(o, d, w).filter(|(depth, perp)| *perp <= reach(*depth));

    let mut tiers: [Vec<Cand>; 3] = [Vec::new(), Vec::new(), Vec::new()];
    for (bi, b) in bodies.iter().enumerate() {
        let g = &b.graph;
        if q.kinds.points {
            for p in &g.points {
                let w = world_of(b, p.at);
                if let Some((depth, perp)) = keep(w) {
                    tiers[0].push((perp, depth, bi, PickTarget::Point(*p), w));
                }
            }
        }
        if q.kinds.centres {
            for (k, a) in g.arcs.iter().enumerate() {
                let w = world_of(b, a.centre);
                if let Some((depth, perp)) = keep(w) {
                    tiers[1].push((perp, depth, bi, PickTarget::Centre { arc: k }, w));
                }
            }
        }
        if q.kinds.edges {
            for &ei in &g.edges {
                let Some((a, z)) = g.edge_ends(ei) else { continue };
                let w = segment_near_ray(o, d, world_of(b, a), world_of(b, z));
                if let Some((depth, perp)) = keep(w) {
                    tiers[2].push((perp, depth, bi, PickTarget::Edge { edge: ei }, w));
                }
            }
        }
    }
    for mut tier in tiers {
        // Nearest the ray first (to the nanometre: two centres stacked on
        // one axis tie, never split by float noise), then nearest the eye;
        // only the winners pay for the occlusion test.
        let nm = |perp: f64| (perp * 1e6).round();
        tier.sort_by(|a, b| nm(a.0).total_cmp(&nm(b.0)).then(a.1.total_cmp(&b.1)));
        let tries = if q.through { usize::MAX } else { OCCLUSION_TRIES };
        let seen = |c: &Cand| match c.3 {
            // A centre floats in the air of its hole: it is offered when
            // its round EDGE is in sight (#692), not by its own position.
            PickTarget::Centre { arc } => arc_seen(bodies, c.2, arc, &q.ray, d),
            _ => !occluded(bodies, c.2, &q.ray, d, c.4),
        };
        let found = tier.into_iter().take(tries).find(|c| q.through || seen(c));
        if let Some((_, _, bi, target, at)) = found {
            let entity = bodies.get(bi)?.name.clone();
            return Some(FeaturePick { entity, target, at, surface });
        }
    }
    if !q.kinds.faces {
        return None;
    }
    let (bi, facet, _) = hit?;
    let b = bodies.get(bi)?;
    let face = *b.graph.face_of_facet.get(facet)?;
    let sel = b.graph.faces.get(face as usize)?.sel;
    Some(FeaturePick { entity: b.name.clone(), target: PickTarget::Face { facet, face, sel }, at: surface?, surface })
}

/// The world's pickable bodies - what the viewport draws: every named
/// entity with facets except those `skip` names (hidden in the view) and
/// the stocks of setups other than the active one (only the active
/// setup's stock stands in the viewport), each at its view pose (#326).
#[must_use]
pub fn pick_bodies_of(world: &World, skip: impl Fn(&str) -> bool) -> Vec<PickBody> {
    let shown_stock = crate::setup::stock_entity_name(world, crate::setup::active(world).as_deref());
    let mut out = Vec::new();
    for (name, entity) in model::named_entities(world) {
        if skip(&name) {
            continue;
        }
        let stock = model::is_stock(world, &entity);
        if stock && name != shown_stock {
            continue;
        }
        let Some(tris) = model::facets_of(world, &entity) else { continue };
        let pose = crate::setup_align::view_pose(world, &entity).unwrap_or_default();
        let mut body = PickBody::new(&name, tris, pose);
        body.stock = stock;
        out.push(body);
    }
    out
}

/// The app-wide pick: [`pick_bodies`] over the world's entities.
#[must_use]
pub fn pick_feature(world: &World, q: &PickQuery, skip: impl Fn(&str) -> bool) -> Option<FeaturePick> {
    pick_bodies(&pick_bodies_of(world, skip), q)
}
