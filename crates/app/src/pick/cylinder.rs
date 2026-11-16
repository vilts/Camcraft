//! The cylinder-angle alignment (#771): one curved-wall pick (a bore's
//! or cylinder's welded wall) names the wall's axis, and the axis
//! lands on a chosen world axis, turned by an angle about it.
//!
//! The axis reading is the wall's own geometry, not a guess: every
//! facet normal of a cylinder wall is perpendicular to the axis, so
//! any two non-parallel wall normals cross exactly along it. The
//! first facet normal and the wall normal most perpendicular to it
//! give a well-conditioned pair (magnitude sin of their angle, never
//! zero for a curved weld); every other normal is then verified
//! against that axis - a mixed patch that is not one cylinder wall is
//! refused by name, never silently misread. The axis direction is
//! read in the setup frame (through the part's placement and the
//! setup's current part alignment, so it composes after a
//! face-to-plane lay, #330); a flip reverses its sense (which way
//! the angle turns about it). The frame is built with the axis in
//! the chosen world axis's slot, the other columns from the world
//! axis least aligned with it (so the frame is never degenerate),
//! and the typed angle turns the result about the world axis it
//! landed on. The part turns about the centre of its bounds, as the
//! setup holds them, so it stays on its spot of the table. Pure:
//! the result is the setup's new part alignment, handed to
//! `AlignPartToSetup`; the work offset is never touched.
//!
//! Provenance: our own design over plain fixture practice (indicate
//! a bore's wall and swing the angle about its axis); the
//! normals-perpendicular-to-the-axis property of a cylinder is
//! textbook surface geometry. No vendor material consulted.

use flecs_ecs::prelude::World;

use crate::geometry::{Point3, Triangle, feature, quat_conjugate, quat_mul, quat_rotate_point, transform_point};
use crate::model;
use crate::operation::{FeatureSel, GeoSel};
use crate::pick::align::{cross, norm, quat_from_axes, scale, sub};
use crate::schema::Location;
use crate::setup_align;

/// How far the landed axis may miss the chosen world axis (float
/// round-off only; the math is closed-form).
const LAND_TOL: f64 = 1e-9;
/// How far a facet normal may lean off perpendicular to the axis
/// before the wall is not one cylinder wall (a tessellation's facets
/// are exactly on the wall; anything above this is a mixed patch).
const NORMAL_TOL: f64 = 1e-6;

/// The world axis a picked cylinder lands on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TargetAxis {
    X,
    Y,
    Z,
}

impl TargetAxis {
    /// The unit world vector this target names.
    #[must_use]
    pub fn vec(self) -> [f64; 3] {
        match self {
            TargetAxis::X => [1.0, 0.0, 0.0],
            TargetAxis::Y => [0.0, 1.0, 0.0],
            TargetAxis::Z => [0.0, 0.0, 1.0],
        }
    }

    /// The slot's index in a frame's columns.
    #[must_use]
    pub fn slot(self) -> usize {
        match self {
            TargetAxis::X => 0,
            TargetAxis::Y => 1,
            TargetAxis::Z => 2,
        }
    }

    /// The short label the UI shows for this target.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            TargetAxis::X => "+X",
            TargetAxis::Y => "+Y",
            TargetAxis::Z => "+Z",
        }
    }
}

/// The setup's new part alignment from a picked cylinder wall: the
/// wall's axis lands on `target`, turned `angle_deg` about it
/// (applied after the landing, so the landed axis stays put). The
/// pick is one curved wall (a welded patch); anything else is
/// refused by name.
pub fn align_from_cylinder(
    world: &World,
    setup: &str,
    pick: &GeoSel,
    target: TargetAxis,
    angle_deg: f64,
    flip: bool,
) -> Result<Location, String> {
    if !angle_deg.is_finite() {
        return Err("the angle about the axis is not a number - retype it".into());
    }
    let held = setup_align::stored(world, setup).unwrap_or_default();
    let d = axis_through(world, held.rot, pick)?;
    let d = if flip { [-d[0], -d[1], -d[2]] } else { d };
    // Fill the frame: the axis in the target's slot, the other
    // columns from the world axis least aligned with it, closed
    // right-handed by the cross in column order.
    let mut fill = [0.0_f64; 3];
    let mut best = f64::INFINITY;
    for e in [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]] {
        let along = (d[0] * e[0] + d[1] * e[1] + d[2] * e[2]).abs();
        if along < best {
            best = along;
            fill = e;
        }
    }
    let (colx, coly, colz) = frame_cols(d, fill, target);
    let q = quat_from_axes(colx, coly, colz).ok_or("the wall does not make a usable frame - re-pick it")?;
    let rot = quat_conjugate(q);
    // The typed angle, about the world axis it landed on, after the
    // landing: the landed axis stays put, the rest swings by it.
    let rot = swing_about(target, angle_deg, rot);
    // Turn about the centre of the parts' bounds, as the setup holds
    // them, so the part stays on its spot of the table.
    let c = setup_align::part_centre(world).ok_or("there is no part to align - import a model first")?;
    let c = transform_point(c, held.pos, held.rot);
    let rc = quat_rotate_point(rot, c);
    let turn = Location { pos: Point3::new(c.x - rc.x, c.y - rc.y, c.z - rc.z), rot };
    let out = setup_align::compose(turn, held);
    // Prove the landing before the command stores anything: through
    // the new alignment the wall's axis runs along the chosen world
    // axis exactly - never a silently wrong frame.
    let d2 = axis_through(world, out.rot, pick)?;
    let t = target.vec();
    let miss = norm(cross(d2, t));
    if miss > LAND_TOL {
        return Err("the wall does not make a usable frame - re-pick it".into());
    }
    let finite = [out.pos.x, out.pos.y, out.pos.z].iter().chain(&out.rot).all(|v| v.is_finite());
    if !finite {
        return Err("the wall does not make a finite alignment - re-pick it".into());
    }
    Ok(out)
}

/// The picked wall's axis direction (unit), read in the frame `rot`
/// turns things into (the held alignment while the frame is built,
/// the resulting alignment while the landing is proved). A flat face
/// or a non-wall pick is refused by name.
fn axis_through(world: &World, rot: [f64; 4], g: &GeoSel) -> Result<[f64; 3], String> {
    let e = model::find_by_name(world, &g.entity)
        .ok_or_else(|| format!("{} of '{}' is no longer in the model - re-pick it", g.feature, g.entity))?;
    let features = model::features_of(world, &e)
        .ok_or_else(|| format!("'{}' carries no geometry - re-pick it", g.entity))?;
    let tris = model::facets_of(world, &e)
        .ok_or_else(|| format!("'{}' carries no geometry - re-pick it", g.entity))?;
    let FeatureSel::Patch(pid) = g.feature else {
        return Err(format!(
            "{} of '{}' is not a welded wall - pick the bore's or cylinder's curved wall",
            g.feature, g.entity
        ));
    };
    let _patch = feature::patches(&features).into_iter().find(|p| p.id == pid);
    if _patch.is_none() {
        return Err(format!(
            "{} of '{}' is no longer in the model - re-pick it",
            g.feature, g.entity
        ));
    }
    let patch_faces: Vec<_> = _patch.into_iter().flat_map(|p| p.faces.clone()).collect();
    // The wall's facet normals, in the model's own frame.
    let mut normals: Vec<[f64; 3]> = Vec::new();
    for face in features.faces.iter().filter(|f| patch_faces.contains(&f.id)) {
        for &i in &face.facets {
            if let Some(t) = tris.get(i as usize)
                && let Some(n) = unit_normal(t)
            {
                normals.push(n);
            }
        }
    }
    let first = *normals.first().ok_or("the picked wall has no area - re-pick it")?;
    // The wall normal most perpendicular to the first: their cross is
    // exactly along the axis (both are perpendicular to it), and the
    // widest choice keeps the cross well away from zero.
    let mut best_n = None;
    let mut best_dot = f64::INFINITY;
    for n in &normals {
        let dot = (n[0] * first[0] + n[1] * first[1] + n[2] * first[2]).abs();
        if dot < best_dot {
            best_dot = dot;
            best_n = Some(*n);
        }
    }
    let other = best_n.ok_or("the picked wall has no area - re-pick it")?;
    let a = cross(first, other);
    let l = norm(a);
    if !l.is_finite() || l <= 0.0 {
        return Err(format!(
            "{} of '{}' does not wrap a curve - every facet of the pick lies one way; pick a fuller band of the bore's or cylinder's wall",
            g.feature, g.entity
        ));
    }
    let d = scale(a, 1.0 / l);
    // Every facet normal must be perpendicular to the axis: a mixed
    // patch that is not one cylinder wall is refused, never misread.
    for n in &normals {
        let dot = (n[0] * d[0] + n[1] * d[1] + n[2] * d[2]).abs();
        if dot > NORMAL_TOL {
            return Err(format!(
                "{} of '{}' is not one cylinder wall - pick just the bore's or cylinder's curved wall",
                g.feature, g.entity
            ));
        }
    }
    // Into the world (the part's placement), then the reading frame.
    // The supplied frame already includes the setup alignment; view_pose
    // would apply the active setup alignment a second time.
    let loc = model::transform_of(world, &e).unwrap_or_default();
    let w = quat_rotate_point(loc.rot, Point3::new(d[0], d[1], d[2]));
    let r = quat_rotate_point(rot, Point3::new(w.x, w.y, w.z));
    Ok([r.x, r.y, r.z])
}

/// One facet's unit normal, `None` when it carries no area or is not
/// finite (it weighs nothing either way).
fn unit_normal(t: &Triangle) -> Option<[f64; 3]> {
    let n = (t.b - t.a).cross(t.c - t.a);
    let l = n.dot(n).sqrt();
    (l.is_finite() && l > 0.0).then(|| [n.x / l, n.y / l, n.z / l])
}

/// The frame columns for axis `d` landing on `target`, with `fill` (a
/// world axis, orthogonalized against `d`) closing the other slots;
/// the cross in column order makes it right-handed.
fn frame_cols(d: [f64; 3], fill: [f64; 3], target: TargetAxis) -> ([f64; 3], [f64; 3], [f64; 3]) {
    // Make the fill square to the axis.
    let along = fill[0] * d[0] + fill[1] * d[1] + fill[2] * d[2];
    let f = sub(fill, scale(d, along));
    let l = norm(f);
    let f = if l.is_finite() && l > 0.0 { scale(f, 1.0 / l) } else { [1.0, 0.0, 0.0] };
    match target.slot() {
        0 => (d, f, cross(d, f)),
        1 => (f, d, cross(f, d)),
        _ => (f, cross(d, f), d),
    }
}

/// Rotate `rot` by `angle_deg` about the world axis `target`, applied
/// after it: the landed axis stays put, the rest swings by the angle.
fn swing_about(target: TargetAxis, angle_deg: f64, rot: [f64; 4]) -> [f64; 4] {
    let a = angle_deg.to_radians();
    let (c, s) = (a.cos(), a.sin());
    let cols = match target {
        TargetAxis::X => ([1.0, 0.0, 0.0], [0.0, c, s], [0.0, -s, c]),
        TargetAxis::Y => ([c, 0.0, -s], [0.0, 1.0, 0.0], [s, 0.0, c]),
        TargetAxis::Z => ([c, s, 0.0], [-s, c, 0.0], [0.0, 0.0, 1.0]),
    };
    match quat_from_axes(cols.0, cols.1, cols.2) {
        Some(q) => quat_mul(q, rot),
        None => rot,
    }
}
