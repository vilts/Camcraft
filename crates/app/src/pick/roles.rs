//! WCS-from-features roles (#701): every pick carries a chosen role -
//! Origin, X, Y or Z - over flat faces, straight edges, vertices and
//! curved walls. The first axis pick in Z, X, Y priority reads its axis
//! exactly, the second is projected perpendicular to it, and the third
//! axis follows right-handed - the third pick, if any, is overridden
//! wholesale (#187's rule generalized). A curved wall reads through
//! `wall_fit`, the unit that fits a wall's axis and centre from its
//! facets, so a tilted bore reads true.
//! Pure: the UI reads the result and sends the existing `SetWcs`
//! command; nothing here writes the world. `align_321` is the old fixed
//! role table; this is the general one it grew out of.
//!
//! Provenance: datum-plane/line/point alignment as taught in coordinate
//! metrology. Our own code, no vendor material consulted.

use flecs_ecs::prelude::World;

use crate::geometry::{Triangle, feature, quat_to_euler, transform_point};
use crate::model;
use crate::operation::{FeatureSel, GeoSel};

use super::align::{
    Alignment, MIN_EDGE_TILT_DEG, cross, dot, gone, norm, outward_normal, quat_from_axes, scale,
    sub, straight_edge, vertex,
};
use super::face::face_origin;
use super::wall_fit::fit_wall;

/// The part of the frame a pick sets: the work origin, or one of the
/// three axes. At most one pick per role; the third axis (and any pick
/// standing where it would land) is derived, never read.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Role {
    Origin,
    X,
    Y,
    Z,
}

impl Role {
    fn label(self) -> &'static str {
        match self {
            Role::Origin => "Origin",
            Role::X => "X",
            Role::Y => "Y",
            Role::Z => "Z",
        }
    }
}

/// What one pick contributes, already placed in the viewer's frame: a
/// direction (axes only - a straight edge's is its raw walk, so
/// `align_roles` normalises it before use), an anchor point (origin, or
/// the point an axis stands at), and a datum plane ONLY when the pick is
/// a flat face - a line or a fitted wall is no plane, so it projects nothing.
struct Datum {
    dir: Option<[f64; 3]>,
    anchor: [f64; 3],
    plane: Option<([f64; 3], [f64; 3])>,
    /// A straight edge's two ends, so an origin pick can side the
    /// direction, exactly as #187 did.
    ends: Option<([f64; 3], [f64; 3])>,
}

/// The first two axis roles, in the order the frame is built: the first
/// present reads exactly, the second is projected. The third axis always
/// follows right-handed - its pick, if any, is never resolved.
const AXIS_PRIORITY: [Role; 3] = [Role::Z, Role::X, Role::Y];

/// Build a work offset from role-tagged picks: every pick says what it
/// sets - `Origin`, `X`, `Y` or `Z`. The first two axis roles present, in
/// `AXIS_PRIORITY` order, build the frame: the first reads its axis
/// exactly, the second is projected square to it; the third follows
/// right-handed and its pick, if any, is overridden wholesale (#187's
/// rule for any pair). The origin is the Origin pick, else the second
/// axis pick's anchor; a flat face's plane projects it (#187 again).
///
/// Refusals come in order: too few picks, a role set twice, too few
/// axes, then each datum by name, then how square the two axes stand.
pub fn align_roles(world: &World, picks: &[(GeoSel, Role)]) -> Result<Alignment, String> {
    if picks.len() < 2 {
        return Err("set the WCS from two axes, plus an optional origin".into());
    }
    for r in [Role::Origin, Role::X, Role::Y, Role::Z] {
        if picks.iter().filter(|(_, role)| *role == r).count() > 1 {
            return Err(format!("only one pick may set {} - re-pick one of them", r.label()));
        }
    }
    // The two picks that carry the frame's axes, in priority order. A
    // third axis pick sits outside this list unread: that axis is
    // derived, so its pick is never resolved and never checked - even a
    // pick standing parallel to the first rides along.
    let frame: Vec<(usize, Role)> = AXIS_PRIORITY
        .iter()
        .filter_map(|r| picks.iter().position(|(_, role)| *role == *r).map(|i| (i, *r)))
        .collect();
    if frame.len() < 2 {
        return Err("set the WCS from two axes - a face, a straight edge or a wall each set one".into());
    }
    let (g1, g2) = (&picks[frame[0].0].0, &picks[frame[1].0].0);
    let mut d1 = datum_for(world, g1, frame[0].1)?;
    let mut d2 = datum_for(world, g2, frame[1].1)?;
    let origin = picks.iter().find(|(_, role)| *role == Role::Origin).map(|(g, _)| g.clone());
    let raw = match &origin {
        Some(g) => datum_for(world, g, Role::Origin)?.anchor,
        None => d2.anchor,
    };
    if origin.is_some() {
        // #187's siding rule, for either axis pick: one backed by a
        // straight edge runs from the end nearer the origin toward the
        // other, so the frame points away from the work, not at it.
        for d in [&mut d1, &mut d2] {
            if let (Some(dir), Some((a, b))) = (d.dir, d.ends)
                && norm(sub(a, raw)) > norm(sub(b, raw))
            {
                d.dir = Some(scale(dir, -1.0));
            }
        }
    }
    let a = d1.dir.ok_or_else(|| format!("{} of '{}' sets no axis - re-pick it", g1.feature, g1.entity))?;
    // Normalise right after reading: a Chain datum's direction is the
    // raw edge walk, so it carries the edge's length. Reading it raw left
    // the frame scaled and `quat_from_axes` returned a silently wrong
    // rotation for a part tipped off the table.
    let a_len = norm(a);
    if !(a_len.is_finite() && a_len > 0.0) {
        return Err(format!("{} of '{}' has no length - re-pick it", g1.feature, g1.entity));
    }
    let a = scale(a, 1.0 / a_len);
    // The projection maths is #187's step for step, so a role table with
    // the old fixed roles agrees with `align_321` to the last digit.
    let tilt = MIN_EDGE_TILT_DEG.to_radians().sin();
    // Only the ordered-by-priority pairs are reachable: (Z, X), (Z, Y)
    // and (X, Y). Each arm places every pick's direction on the axis its
    // role names and derives the third right-handed; the derived axis is
    // never the projected one.
    let (x, y, z) = match (frame[0].1, frame[1].1) {
        (Role::Z, Role::X) => {
            let x = square_to(g2, d2.dir, a, tilt)?;
            (x, cross(a, x), a)
        }
        (Role::Z, Role::Y) => {
            let y = square_to(g2, d2.dir, a, tilt)?;
            (cross(y, a), y, a)
        }
        (Role::X, Role::Y) => {
            let y = square_to(g2, d2.dir, a, tilt)?;
            (a, y, cross(a, y))
        }
        _ => return Err("two different axes set the frame - re-pick one of them".into()),
    };
    // #187's projection rule: the origin lands on the datum plane only
    // when the exact axis read it from a flat face - a line or a fitted
    // wall is no plane, so a bore's centre stays in its bore.
    let pos = match d1.plane {
        Some((p0, n)) => sub(raw, scale(n, dot(sub(raw, p0), n))),
        None => raw,
    };
    let q = quat_from_axes(x, y, z).ok_or("the picks do not make a usable frame - re-pick them")?;
    let (r, p, w) = quat_to_euler(q);
    let out = Alignment {
        pos,
        roll_deg: r.to_degrees(),
        pitch_deg: p.to_degrees(),
        yaw_deg: w.to_degrees(),
        x,
        y,
        z,
    };
    if out.pos.iter().chain(&[out.roll_deg, out.pitch_deg, out.yaw_deg]).all(|v| v.is_finite()) {
        Ok(out)
    } else {
        Err("the picks do not make a finite frame - re-pick them".into())
    }
}

/// The second axis pick, projected square to the first: #187's maths -
/// project, check the lean against `tilt` (already a sine), normalize.
/// Refused by name when the two picks stand too near parallel.
fn square_to(g: &GeoSel, d: Option<[f64; 3]>, axis: [f64; 3], tilt: f64) -> Result<[f64; 3], String> {
    let d = d.ok_or_else(|| format!("{} of '{}' sets no axis - re-pick it", g.feature, g.entity))?;
    let len = norm(d);
    let ex = sub(d, scale(axis, dot(d, axis)));
    let ex_len = norm(ex);
    if !len.is_finite() || len <= 0.0 || ex_len < len * tilt {
        return Err(format!(
            "{} of '{}' is too near parallel to the first axis - pick picks that lean apart",
            g.feature, g.entity
        ));
    }
    Ok(scale(ex, 1.0 / ex_len))
}

/// Read one pick into the datum it contributes, placed in the viewer's
/// frame (#326). An axis role reads a direction - a flat face its
/// outward normal, a straight edge its walk, a curved wall its fitted
/// axis; `Origin` reads a point - a vertex, a flat face's centroid or a
/// wall's centre. Only a flat face carries a plane to project onto. What
/// a pick cannot set for its role is refused by name, never guessed.
fn datum_for(world: &World, g: &GeoSel, role: Role) -> Result<Datum, String> {
    let pose = crate::setup_align::view_pose;
    match (g.feature, role) {
        (FeatureSel::Vertex(_), Role::Origin) => Ok(Datum {
            dir: None,
            anchor: vertex(world, g)?,
            plane: None,
            ends: None,
        }),
        (FeatureSel::Vertex(_), _) => Err(format!(
            "{} of '{}' sets no axis - pick a face, a straight edge or a wall for it",
            g.feature, g.entity
        )),
        (FeatureSel::Chain(_), Role::Origin) => Err(format!(
            "{} of '{}' sets an axis - pick a vertex or a wall for the origin",
            g.feature, g.entity
        )),
        (FeatureSel::Chain(_), _) => {
            let (a, b) = straight_edge(world, g, pose, &format!("for {}", role.label()))?;
            // The raw walk vector is the direction: `align_roles` squares
            // and normalizes it against the first axis, #187's maths, and
            // the ends are what sides it.
            Ok(Datum { dir: Some(sub(b, a)), anchor: a, plane: None, ends: Some((a, b)) })
        }
        (FeatureSel::Face(_), _) => match outward_normal(world, g, pose)? {
            Some(_) if role == Role::Origin => {
                let c = face_origin(world, &g.entity, g.feature).map_err(|why| why.to_owned())?;
                Ok(Datum { dir: None, anchor: c.centroid, plane: None, ends: None })
            }
            Some(n) => {
                let c = face_origin(world, &g.entity, g.feature).map_err(|why| why.to_owned())?;
                Ok(Datum { dir: Some(n), anchor: c.centroid, plane: Some((c.centroid, n)), ends: None })
            }
            // Not flat: a curved face is a wall, and a wall fits its own
            // axis and centre for either role.
            None => wall_datum(world, g, role),
        },
        (FeatureSel::Patch(_), _) => wall_datum(world, g, role),
    }
}

/// A curved wall's datum, for either role: an axis fitted through its
/// facets, anchored at the wall's centre. No plane - a wall is no flat
/// face, so the origin is never projected onto it.
fn wall_datum(world: &World, g: &GeoSel, role: Role) -> Result<Datum, String> {
    let e = model::find_by_name(world, &g.entity)
        .ok_or_else(|| gone(g))?;
    let feats = model::features_of(world, &e)
        .ok_or_else(|| gone(g))?;
    // Facet ids straight from the feature, and a patch's faces' facets:
    // one list either way, then placed where the part sits (#326).
    let sel = match g.feature {
        FeatureSel::Face(fid) => feats.faces.iter().find(|f| f.id == fid).map(|f| f.facets.clone()),
        FeatureSel::Patch(pid) => feature::patches(&feats)
            .into_iter()
            .find(|p| p.id == pid)
            .map(|p| {
                p.faces
                    .iter()
                    .flat_map(|fid| {
                        feats.faces.iter().find(|f| f.id == *fid).map(|f| f.facets.clone()).unwrap_or_default()
                    })
                    .collect()
            }),
        _ => None,
    };
    let ids = sel.ok_or_else(|| gone(g))?;
    let tris = model::facets_of(world, &e).ok_or_else(|| gone(g))?;
    let loc = crate::setup_align::view_pose(world, &e).unwrap_or_default();
    let wall: Vec<Triangle> = ids
        .iter()
        .filter_map(|i| tris.get(*i as usize))
        .map(|t| Triangle::new(
            transform_point(t.a, loc.pos, loc.rot),
            transform_point(t.b, loc.pos, loc.rot),
            transform_point(t.c, loc.pos, loc.rot),
        ))
        .collect();
    let (axis, centre) = fit_wall(&g.entity, &wall)?;
    Ok(Datum {
        dir: (role != Role::Origin).then_some(axis),
        anchor: centre,
        plane: None,
        ends: None,
    })
}
