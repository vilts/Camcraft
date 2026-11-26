//! Ray-to-curve hit testing for DXF sketch profiles (#1864).
//!
//! Provenance: analytic 3D ray-to-segment distance and planar projection refined onto exact
//! circular arcs and full circles; reuses `feature_pick` sight-line occlusion so solid bodies
//! still hide curves behind them while stock stays translucent. Original code; no vendor
//! material consulted.

use flecs_ecs::prelude::World;

use crate::geometry::dxf::Primitive;
use crate::geometry::geo2d::Point2;
use crate::geometry::sketch_chains::{ChainId, nearest_point_on_primitive};
use crate::geometry::{Point3, quat_conjugate, quat_rotate_point, transform_inverse_point, transform_point};
use crate::schema::Location;

use super::feature_pick::{
    FeaturePick, PickBody, PickQuery, PickTarget, Projection, occluded, ray_measure, segment_near_ray,
};

/// Number of coarse chord samples across a circular segment before analytic refinement.
const CURVE_SAMPLES: usize = 48;

fn world_pt(pose: Location, p: Point2) -> [f64; 3] {
    let w = transform_point(Point3::new(p.x, p.y, 0.0), pose.pos, pose.rot);
    [w.x, w.y, w.z]
}

/// Closest world point on a 2D sketch `curve` lying on `z = 0` under `pose` to the normalized
/// world ray `(o, d)`, paired with `(perp, depth)`.
fn curve_near_ray(
    o: [f64; 3],
    d: [f64; 3],
    pose: Location,
    curve: &Primitive,
) -> Option<([f64; 3], f64, f64)> {
    let pt_measure = |local: Point2| -> Option<([f64; 3], f64, f64)> {
        let w = world_pt(pose, local);
        ray_measure(o, d, w).map(|(depth, perp)| (w, perp, depth))
    };
    match *curve {
        Primitive::Line { start, end } => {
            let wa = world_pt(pose, start);
            let wb = world_pt(pose, end);
            let w = segment_near_ray(o, d, wa, wb);
            ray_measure(o, d, w).map(|(depth, perp)| (w, perp, depth))
        }
        Primitive::Arc { center, radius, start_angle, sweep_angle } => {
            let mut best: Option<([f64; 3], f64, f64)> = None;
            let mut consider = |cand: Option<([f64; 3], f64, f64)>| {
                if let Some((w, perp, depth)) = cand
                    && best.is_none_or(|(_, bp, bd)| (perp, depth) < (bp, bd))
                {
                    best = Some((w, perp, depth));
                }
            };
            // Chord pass covers grazing / oblique camera angles cleanly.
            let mut prev = Point2::new(
                center.x + radius * start_angle.cos(),
                center.y + radius * start_angle.sin(),
            );
            for k in 1..=CURVE_SAMPLES {
                let a = start_angle + sweep_angle * (k as f64 / CURVE_SAMPLES as f64);
                let cur = Point2::new(center.x + radius * a.cos(), center.y + radius * a.sin());
                let wa = world_pt(pose, prev);
                let wb = world_pt(pose, cur);
                let w = segment_near_ray(o, d, wa, wb);
                let w_l = transform_inverse_point(Point3::new(w[0], w[1], w[2]), pose.pos, pose.rot);
                consider(pt_measure(nearest_point_on_primitive(Point2::new(w_l.x, w_l.y), curve)));
                prev = cur;
            }
            consider(plane_seed_measure(o, d, pose, curve, &pt_measure));
            refine_curve(o, d, pose, curve, best)
        }
        Primitive::Circle { center, radius } => {
            let mut best: Option<([f64; 3], f64, f64)> = None;
            let mut consider = |cand: Option<([f64; 3], f64, f64)>| {
                if let Some((w, perp, depth)) = cand
                    && best.is_none_or(|(_, bp, bd)| (perp, depth) < (bp, bd))
                {
                    best = Some((w, perp, depth));
                }
            };
            let mut prev = Point2::new(center.x + radius, center.y);
            for k in 1..=CURVE_SAMPLES {
                let a = std::f64::consts::TAU * (k as f64 / CURVE_SAMPLES as f64);
                let cur = Point2::new(center.x + radius * a.cos(), center.y + radius * a.sin());
                let wa = world_pt(pose, prev);
                let wb = world_pt(pose, cur);
                let w = segment_near_ray(o, d, wa, wb);
                let w_l = transform_inverse_point(Point3::new(w[0], w[1], w[2]), pose.pos, pose.rot);
                consider(pt_measure(nearest_point_on_primitive(Point2::new(w_l.x, w_l.y), curve)));
                prev = cur;
            }
            consider(plane_seed_measure(o, d, pose, curve, &pt_measure));
            refine_curve(o, d, pose, curve, best)
        }
    }
}

fn plane_seed_measure(
    o: [f64; 3],
    d: [f64; 3],
    pose: Location,
    curve: &Primitive,
    pt_measure: &impl Fn(Point2) -> Option<([f64; 3], f64, f64)>,
) -> Option<([f64; 3], f64, f64)> {
    let o_l = transform_inverse_point(Point3::new(o[0], o[1], o[2]), pose.pos, pose.rot);
    let d_l = quat_rotate_point(quat_conjugate(pose.rot), Point3::new(d[0], d[1], d[2]));
    if !(o_l.is_finite() && d_l.is_finite()) || d_l.z.abs() <= 1e-9 {
        return None;
    }
    let t = -o_l.z / d_l.z;
    if !(t.is_finite() && t > 0.0) {
        return None;
    }
    let p = Point2::new(o_l.x + d_l.x * t, o_l.y + d_l.y * t);
    pt_measure(nearest_point_on_primitive(p, curve))
}

fn refine_curve(
    o: [f64; 3],
    d: [f64; 3],
    pose: Location,
    curve: &Primitive,
    mut best: Option<([f64; 3], f64, f64)>,
) -> Option<([f64; 3], f64, f64)> {
    for _ in 0..4 {
        let Some((w, bp, bd)) = best else { break };
        let on_ray = [o[0] + d[0] * bd, o[1] + d[1] * bd, o[2] + d[2] * bd];
        let _ = w;
        let rl = transform_inverse_point(Point3::new(on_ray[0], on_ray[1], on_ray[2]), pose.pos, pose.rot);
        let q = nearest_point_on_primitive(Point2::new(rl.x, rl.y), curve);
        let nw = world_pt(pose, q);
        if let Some((nd, np)) = ray_measure(o, d, nw)
            && np < bp
        {
            best = Some((nw, np, nd));
        } else {
            break;
        }
    }
    best
}

pub(super) struct SketchCand {
    pub perp: f64,
    pub depth: f64,
    pub pick: FeaturePick,
}

/// Nearest visible sketch chain under `q`, restricted to sketches in the active setup (matching
/// `pick_bodies_of`'s shown-stock rule).
pub(super) fn pick_sketches(
    world: &World,
    bodies: &[PickBody],
    q: &PickQuery,
    d: [f64; 3],
    surface: Option<[f64; 3]>,
    reach: impl Fn(f64) -> f64,
    skip: &dyn Fn(&str) -> bool,
) -> Option<SketchCand> {
    if !q.kinds.edges {
        return None;
    }
    let active = crate::setup::active(world);
    let mut cands: Vec<(f64, f64, String, ChainId, [f64; 3])> = Vec::new();
    for name in crate::sketch::placed_in(world, active.as_deref()) {
        if skip(&name) {
            continue;
        }
        let Some((entity, sketch)) = crate::sketch::find(world, &name) else {
            continue;
        };
        let Some(pose) = crate::sketch::world_pose(world, &entity) else {
            continue;
        };
        for chain in sketch.data.chains().all() {
            let mut best_chain: Option<(f64, f64, [f64; 3])> = None;
            for seg in chain.segments() {
                if let Some((w, perp, depth)) = curve_near_ray(q.ray.origin, d, pose, &seg.curve)
                    && perp <= reach(depth)
                    && best_chain.is_none_or(|(bp, bd, _)| (perp, depth) < (bp, bd))
                {
                    best_chain = Some((perp, depth, w));
                }
            }
            if let Some((perp, depth, w)) = best_chain {
                cands.push((perp, depth, name.clone(), chain.id(), w));
            }
        }
    }
    let nm = |perp: f64| (perp * 1e6).round();
    cands.sort_by(|a, b| nm(a.0).total_cmp(&nm(b.0)).then(a.1.total_cmp(&b.1)));
    let _ = Projection::Perspective;
    for (perp, depth, entity, id, at) in cands {
        if !q.through && occluded(bodies, usize::MAX, &q.ray, d, at) {
            continue;
        }
        return Some(SketchCand {
            perp,
            depth,
            pick: FeaturePick {
                entity,
                target: PickTarget::SketchChain { id },
                at,
                surface,
            },
        });
    }
    None
}
