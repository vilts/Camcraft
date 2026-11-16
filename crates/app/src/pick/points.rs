//! Lay the part on a G17 / G18 / G19 plane through three picked points
//! (#555): three model corners span a plane, whose normal - taken on the
//! side away from the part's material - is laid on the target plane's
//! axis through the face pick's own apply path ([`super::plane`]'s
//! `lay_datum`), so a face and three points on it cannot disagree. Point
//! 1 is the anchor set onto the plane's level. The optional 3-2-1 frame
//! also turns the part about the plane's normal until point 1 -> 2 runs
//! along the plane's first axis (+X for G17 / G18, +Y for G19) and moves
//! point 1 to the setup frame's origin. Pure: the result is a setup's
//! part alignment, handed to `AlignPartToSetup::new`; the work offset is
//! never touched.
//!
//! #738: the setup's move composes on the part as held now
//! ([`lay_on_points_in`]), like the face pick's [`super::plane`].
//!
//! Provenance: our own design over the 3-2-1 (three-point primary datum)
//! fixture practice taught in fixture-design and coordinate-metrology
//! handbooks; the plane through three points is the cross product of two
//! spanning vectors, textbook vector algebra. No vendor material
//! consulted.

use flecs_ecs::prelude::World;

use crate::geometry::{Point3, normalize_quat, quat_rotate_point, transform_point};
use crate::model;
use crate::operation::{FeatureSel, GeoSel};
use crate::schema::Location;
use crate::setup_align;

use super::align::{cross, dot, gone, norm, scale, sub};
use super::plane::{Datum, PlaneLay, axes_of, axis_words, lay_datum, onto_held, target_normal};

/// Two picked points closer than this (mm) are one point.
pub const COINCIDENT_MM: f64 = 1e-3;
/// Three points are (nearly) collinear when the triangle they span is
/// thinner than this angle: its height over its longest side is under
/// `sin(1°)` (about 1.75 % of that side). Thinner, and a micron of pick
/// noise tilts the plane by more than the user can see.
pub const MIN_SPREAD_DEG: f64 = 1.0;
/// The frame's 1 -> 2 must land on the axis this closely (sine).
const LAND_TOL: f64 = 1e-9;
/// Point 1 must land on the origin this closely (mm per mm of its reach).
const ORIGIN_TOL_MM: f64 = 1e-9;

/// How to lay the part on three points: the plane, flip and level of the
/// face mode, plus the 3-2-1 frame (point 1 the origin, 1 -> 2 along the
/// plane's first axis). The frame always sets the plane on the level.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PointsLay {
    pub lay: PlaneLay,
    pub frame: bool,
}

/// The part alignment (document frame -> setup frame) that lays the
/// plane through the three picked points on `pl`'s plane. Every refusal
/// names what to re-pick. Absolute, like the face pick: the same points
/// in an aligned setup give the same answer.
pub fn lay_on_points(world: &World, picks: &[GeoSel], pl: PointsLay) -> Result<Location, String> {
    lay_step(world, picks, pl, Location::default())
}

/// #738: the setup's new part alignment from three points - the lay read
/// on the part as `setup` holds it NOW, composed onto that alignment. The
/// same points again are a zero step: the held alignment comes back.
pub fn lay_on_points_in(world: &World, setup: &str, picks: &[GeoSel], pl: PointsLay) -> Result<Location, String> {
    let held = setup_align::stored(world, setup).unwrap_or_default();
    Ok(onto_held(lay_step(world, picks, pl, held)?, held))
}

/// The lay as a step in the frame `held` puts the part in (the identity:
/// the document frame, and the step is the alignment itself).
fn lay_step(world: &World, picks: &[GeoSel], pl: PointsLay, held: Location) -> Result<Location, String> {
    let (datum, [p1, p2, _]) = datum_of(world, picks)?;
    let datum = datum.through(held);
    let at = |p: [f64; 3]| {
        let q = transform_point(p3(p), held.pos, held.rot);
        [q.x, q.y, q.z]
    };
    let (p1, p2) = (at(p1), at(p2));
    if !pl.frame {
        return lay_datum(world, &datum, pl.lay, held);
    }
    // 3-2-1: lay first (the shared path), then turn about the plane's
    // normal so 1 -> 2 runs along its first axis, then point 1 to 0.
    let laid = lay_datum(world, &datum, PlaneLay { place: false, ..pl.lay }, held)?;
    let t3 = target_normal(pl.lay);
    let (x_t, _) = axes_of(pl.lay.plane);
    let v_t = cross(t3, x_t);
    let d = quat_rotate_point(laid.rot, p3(sub(p2, p1)));
    let d = [d.x, d.y, d.z];
    let theta = -dot(d, v_t).atan2(dot(d, x_t));
    let (s, c) = (0.5 * theta).sin_cos();
    let rot = normalize_quat([t3[0] * s, t3[1] * s, t3[2] * s, c]).ok_or(UNUSABLE)?;
    let turned = setup_align::compose(Location { pos: Point3::ZERO, rot }, laid);
    let o = transform_point(p3(p1), turned.pos, turned.rot);
    let out = Location { pos: Point3::new(turned.pos.x - o.x, turned.pos.y - o.y, turned.pos.z - o.z), rot: turned.rot };
    // Prove the landings, never store a subtly-off frame.
    let o = transform_point(p3(p1), out.pos, out.rot);
    let x = quat_rotate_point(out.rot, p3(sub(p2, p1)));
    let x = [x.x, x.y, x.z];
    let len = norm(x);
    let at_origin = norm([o.x, o.y, o.z]) <= ORIGIN_TOL_MM * (1.0 + norm(p1));
    if !(len > 0.0 && dot(x, x_t) > 0.0 && norm(cross(x, x_t)) <= LAND_TOL * len && at_origin) {
        return Err(UNUSABLE.into());
    }
    let finite = [out.pos.x, out.pos.y, out.pos.z].iter().chain(&out.rot).all(|v| v.is_finite());
    if !finite {
        return Err("the points do not make a finite turn - re-pick them".into());
    }
    Ok(out)
}

const UNUSABLE: &str = "the points do not make a usable turn - re-pick them";

/// Which way the points' plane takes as its outside before Apply, in
/// plain words. Refuses as [`lay_on_points`] does.
pub fn points_side(world: &World, picks: &[GeoSel]) -> Result<String, String> {
    let (d, _) = datum_of(world, picks)?;
    Ok(format!("outside: {} (away from the material)", axis_words(d.normal)))
}

/// Read three picked corners (document frame) as a datum: their plane's
/// unit normal on the side away from the part's material, and point 1.
fn datum_of(world: &World, picks: &[GeoSel]) -> Result<(Datum, [[f64; 3]; 3]), String> {
    if setup_align::part_centre(world).is_none() {
        return Err("there is no part to align - import a model first".into());
    }
    for g in picks {
        let e = model::find_by_name(world, &g.entity).ok_or_else(|| gone(g))?;
        if !setup_align::is_part(world, &e) {
            return Err(format!("'{}' is not the part - pick points of the model", g.entity));
        }
        match g.feature {
            FeatureSel::Vertex(_) => continue,
            // #692: a round edge stands for its centre; any other edge is
            // refused by name.
            FeatureSel::Chain(_) => {
                super::centre::centre_point(world, g)?;
                continue;
            }
            FeatureSel::Face(_) | FeatureSel::Patch(_) => {}
        }
        return Err(format!(
            "{} of '{}' is a face - pick three points (corners of the model or circle centres)",
            g.feature, g.entity
        ));
    }
    let [g1, g2, g3] = picks else {
        return Err(format!("pick three points - {} picked", picks.len()));
    };
    let pts = [super::axis::point(world, g1)?, super::axis::point(world, g2)?, super::axis::point(world, g3)?];
    for (i, j) in [(0, 1), (0, 2), (1, 2)] {
        let gap = norm(sub(pts[j], pts[i]));
        if picks[i] == picks[j] || !gap.is_finite() || gap <= COINCIDENT_MM {
            return Err(format!(
                "points {} and {} coincide (closer than {COINCIDENT_MM} mm) - pick three points apart",
                i + 1,
                j + 1
            ));
        }
    }
    let [a, b, c] = pts;
    let n = cross(sub(b, a), sub(c, a));
    let longest = [norm(sub(b, a)), norm(sub(c, a)), norm(sub(c, b))].into_iter().fold(0.0_f64, f64::max);
    // Twice the area over the longest side is the triangle's least height.
    let height = norm(n) / longest;
    if !(height.is_finite() && height >= longest * MIN_SPREAD_DEG.to_radians().sin()) {
        return Err(format!(
            "the three points are (nearly) collinear - they span less than {MIN_SPREAD_DEG}° across; \
             pick points spread across the plane"
        ));
    }
    let mut normal = scale(n, 1.0 / norm(n));
    if material_side(world, a, normal) > 0.0 {
        normal = scale(normal, -1.0);
    }
    Ok((Datum { normal, anchor: a }, pts))
}

/// Which side of the plane (through `a`, normal `n`) the part's material
/// reaches further into: > 0 along `n`, < 0 against it, 0 for a tie (a
/// plane through the middle) - then the pick order's right-hand normal
/// (1 -> 2 -> 3) stands, and Flip turns it.
fn material_side(world: &World, a: [f64; 3], n: [f64; 3]) -> f64 {
    let (mut above, mut below) = (0.0_f64, 0.0_f64);
    for (_, e) in model::model_entities(world) {
        if !setup_align::is_part(world, &e) {
            continue;
        }
        for t in model::world_facets(world, &e).unwrap_or_default().iter() {
            for p in [t.a, t.b, t.c] {
                let s = dot(sub([p.x, p.y, p.z], a), n);
                if s.is_finite() {
                    above = above.max(s);
                    below = below.max(-s);
                }
            }
        }
    }
    let tie = 1e-6 * (above + below) + 1e-9;
    if (above - below).abs() <= tie { 0.0 } else { above - below }
}

fn p3(a: [f64; 3]) -> Point3 {
    Point3::new(a[0], a[1], a[2])
}
