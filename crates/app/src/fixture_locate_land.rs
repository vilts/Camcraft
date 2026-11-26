//! Does a located stock land on its mates (#2173)? A stock locate moves
//! the part and lets its stock follow; a square stock re-fits round a
//! turned part, so the follow can leave the picked stock faces off the
//! fixture planes they were mated to. Each picked face is judged on the
//! body the follow builds: facets that face the way the picked face must
//! (1 - cos of their angle within [`NORMAL_TOL`]) and lie on the mate plane (within
//! [`LEN_TOL_MM`]) must cover the face's whole footprint there. A
//! picked vertex must be a corner of the followed stock. The box of the
//! two bodies must also agree. Anything else is refused by name.
//!
//! Provenance: plane and point datums of 3-2-1 fixturing (machine-tool
//! and fixture-design handbooks); triangle area and barycentric
//! containment are standard computational geometry. Our own code, no
//! vendor material.

use flecs_ecs::prelude::World;

use super::{LEN_TOL_MM, LocateSpec, NORMAL_TOL, Plan, stock_carried};
use crate::geometry::{Point3, Triangle, feature, transform_point};
use crate::model;
use crate::operation::{FeatureSel, GeoSel};

/// The share of a picked face's area that the followed stock may lack on
/// the mate plane (rounding of the re-fit sizes only).
pub const FACE_AREA_REL_TOL: f64 = 1e-3;

type V = [f64; 3];

fn v(p: Point3) -> V {
    [p.x, p.y, p.z]
}
fn sub(a: V, b: V) -> V {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
fn dot(a: V, b: V) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
fn cross(a: V, b: V) -> V {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}
fn norm(a: V) -> f64 {
    dot(a, a).sqrt()
}
/// Twice the triangle's vector area (its normal times twice its area).
fn area2(t: &Triangle) -> V {
    cross(sub(v(t.b), v(t.a)), sub(v(t.c), v(t.a)))
}

/// A stock locate lands only where the followed stock meets every mate.
/// `stocks` are the moved stock bodies (setup frame) of the part write;
/// the rigid carry of the stock by the plan is where the mates put it.
pub fn stock_lands(world: &World, spec: &LocateSpec, plan: &Plan, stocks: &[(String, Vec<Triangle>)]) -> Result<(), String> {
    let name = crate::setup::stock_entity_name(world, plan.setup.as_deref());
    let carried = stock_carried(world, plan);
    let want = carried.iter().find(|(n, _)| *n == name).map(|(_, t)| t.as_slice());
    let got = stocks.iter().find(|(n, _)| *n == name).map(|(_, t)| t.as_slice());
    let not_following =
        || format!("the stock '{name}' does not follow the part - set the stock round the part, or locate the part");
    let (Some(want), Some(got)) = (want, got) else {
        return Err(not_following());
    };
    let (Some(wb), Some(gb)) = (box_of(want), box_of(got)) else {
        return Err(not_following());
    };
    let off = (0..3).any(|k| (wb.0[k] - gb.0[k]).abs() > LEN_TOL_MM || (wb.1[k] - gb.1[k]).abs() > LEN_TOL_MM);
    if off {
        return Err(format!(
            "the stock '{name}' stays square round the part and would not land on the mates - locate it without a turn, or locate the part"
        ));
    }
    // A rigid carry (a stock that does not re-fit) lands by construction.
    let same = want.len() == got.len()
        && want.iter().zip(got).all(|(a, b)| [(a.a, b.a), (a.b, b.b), (a.c, b.c)].into_iter().all(|(p, q)| norm(sub(v(p), v(q))) <= LEN_TOL_MM));
    if same {
        return Ok(());
    }
    let e = model::find_by_name(world, &name).ok_or_else(not_following)?;
    let features = model::features_of(world, &e).ok_or_else(not_following)?;
    let pose = model::transform_of(world, &e).unwrap_or_default();
    for (i, m) in spec.mates.iter().enumerate() {
        let k = i + 1;
        let missed = |what: &str| {
            format!(
                "mate {k}: {} of the stock '{name}' {what} {} - the stock keeps its upright shape round the turned part, so that face does not meet the fixture; locate it without a turn, or locate the part",
                crate::pick::feature_name_or_kind(world, &name, &m.part.feature),
                crate::pick::geo_label(world, &m.fixture),
            )
        };
        match &m.part.feature {
            FeatureSel::Face(fid) => {
                let f = features.faces.iter().find(|f| f.id == *fid).ok_or_else(|| gone(world, &m.part))?;
                let face: Vec<Triangle> = f.facets.iter().filter_map(|&i| want.get(i as usize).copied()).collect();
                if face.len() != f.facets.len() {
                    return Err(gone(world, &m.part));
                }
                if !face_lands(&face, got) {
                    return Err(missed("would stand across the plane of, not flat on,"));
                }
            }
            FeatureSel::Vertex(id) => {
                let p = feature::resolve_vertex(&features, *id)
                    .and_then(|vi| features.verts.get(vi as usize))
                    .map(|q| transform_point(Point3::new(q[0], q[1], q[2]), pose.pos, pose.rot))
                    .map(|q| v(transform_point(q, plan.delta.pos, plan.delta.rot)))
                    .ok_or_else(|| gone(world, &m.part))?;
                let corner = got.iter().flat_map(|t| [t.a, t.b, t.c]).any(|q| norm(sub(v(q), p)) <= LEN_TOL_MM);
                if !corner {
                    return Err(missed("would not touch"));
                }
            }
            // `plan` refuses edges, curved patches and DXF sketch picks before this runs.
            FeatureSel::Patch(_)
            | FeatureSel::Chain(_)
            | FeatureSel::SketchChain(_)
            | FeatureSel::SketchLayer(_) => {}
        }
    }
    Ok(())
}

fn gone(world: &World, g: &GeoSel) -> String {
    format!("{} is gone - re-pick it", crate::pick::geo_label(world, g))
}

/// Does the face `face` (where the mates put it) lie on the followed
/// body `got`? The face's plane is its
/// area-weighted normal through its first corner. `got` must hold
/// facets on that plane that face the same way (parallel within
/// [`NORMAL_TOL`]), covering the face's area and each of its facet
/// centres. Both bodies come from one mesher, so a facet's winding says
/// which way it faces.
fn face_lands(face: &[Triangle], got: &[Triangle]) -> bool {
    let sum = face.iter().map(area2).fold([0.0; 3], |a, b| [a[0] + b[0], a[1] + b[1], a[2] + b[2]]);
    let (len, area) = (norm(sum), face.iter().map(|t| 0.5 * norm(area2(t))).sum::<f64>());
    let Some(first) = face.first() else { return false };
    if !(len.is_finite() && len > 0.0 && area > 0.0) {
        return false;
    }
    let n = [sum[0] / len, sum[1] / len, sum[2] / len];
    let at = v(first.a);
    let d = |p: Point3| dot(sub(v(p), at), n);
    let on: Vec<&Triangle> = got
        .iter()
        .filter(|t| [t.a, t.b, t.c].into_iter().all(|p| d(p).abs() <= LEN_TOL_MM))
        .filter(|t| {
            let a = area2(t);
            let l = norm(a);
            l > 0.0 && (1.0 - dot(a, n) / l) <= NORMAL_TOL
        })
        .collect();
    let covered: f64 = on.iter().map(|t| 0.5 * norm(area2(t))).sum();
    if covered < area * (1.0 - FACE_AREA_REL_TOL) {
        return false;
    }
    face.iter().all(|t| {
        let c = [(t.a.x + t.b.x + t.c.x) / 3.0, (t.a.y + t.b.y + t.c.y) / 3.0, (t.a.z + t.b.z + t.c.z) / 3.0];
        on.iter().any(|g| inside(g, c))
    })
}

/// Is `p` (on the triangle's plane) inside `t`, edges included?
fn inside(t: &Triangle, p: V) -> bool {
    let (a, b, c) = (v(t.a), v(t.b), v(t.c));
    let n = cross(sub(b, a), sub(c, a));
    let nn = dot(n, n);
    if nn.is_nan() || nn <= 0.0 {
        return false;
    }
    let tol = LEN_TOL_MM * nn.sqrt();
    [(a, b), (b, c), (c, a)].into_iter().all(|(s, e)| dot(cross(sub(e, s), sub(p, s)), n) >= -tol * norm(sub(e, s)))
}

/// The (min, max) box of a body; `None` when empty or not finite.
fn box_of(tris: &[Triangle]) -> Option<(V, V)> {
    let (mut lo, mut hi) = ([f64::INFINITY; 3], [f64::NEG_INFINITY; 3]);
    for p in tris.iter().flat_map(|t| [t.a, t.b, t.c]) {
        for (k, x) in v(p).into_iter().enumerate() {
            if !x.is_finite() {
                return None;
            }
            lo[k] = lo[k].min(x);
            hi[k] = hi[k].max(x);
        }
    }
    (lo[0] <= hi[0]).then_some((lo, hi))
}
