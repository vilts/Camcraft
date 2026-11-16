//! Lay the part on a G17 / G18 / G19 plane from a pick (#330): one flat
//! face, turned so its outward normal runs along the plane's axis - +Z
//! for G17 (XY), +Y for G18 (ZX), +X for G19 (YZ), or the opposite way
//! with `flip`. The result is a setup's part alignment (#326): a rigid
//! transform from the document frame into the setup frame, handed to
//! `AlignPartToSetup::new`. Pure: nothing here writes the world, and the
//! work offset is never touched.
//!
//! #554: a face only. An edge cannot define a plane, so the #330
//! two-edge mode is gone and an edge pick is refused by name; edges set
//! an in-plane axis instead (#557, [`super::axis`]).
//!
//! #555: three picked points lay through the same [`lay_datum`]
//! ([`super::points`]), so a face and three corners of it agree.
//!
//! #738: the setup's move composes on the part as the setup holds it now
//! ([`lay_on_plane_in`]): the pick is read through the held alignment,
//! the lay is the step from there, and the step is composed onto it - so
//! a face lay keeps an earlier axis turn's heading instead of returning
//! the part to its import pose first. [`lay_on_plane`] is that step from
//! the import pose (the held alignment the identity).
//!
//! Provenance: our own design over plain fixture practice (lay a datum
//! face on the table); the shortest-arc quaternion between two unit
//! vectors is textbook rigid-body algebra (Shoemake, "Animating rotation with quaternion
//! curves", SIGGRAPH 1985). No vendor material consulted.

use flecs_ecs::prelude::World;

use crate::cl::ArcPlane;
use crate::geometry::feature::FaceKind;
use crate::geometry::{Point3, normalize_quat, quat_rotate_point, transform_point};
use crate::model;
use crate::operation::{FeatureSel, GeoSel};
use crate::schema::Location;
use crate::setup_align;

use super::align::{cross, dot, gone, norm, outward_normal, scale, sub};

/// A face with less area than this (mm^2) is degenerate.
const MIN_FACE_AREA_MM2: f64 = 1e-9;
/// The turned normal must land on the target axis this closely (rad).
const LAND_TOL_RAD: f64 = 1e-9;

/// How to lay the part: the target plane, which way its normal faces, and
/// whether the picked face is set onto the plane's
/// level (0 along the plane's axis) or the part keeps its place.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PlaneLay {
    pub plane: ArcPlane,
    /// Face the opposite way: -Z for G17, -Y for G18, -X for G19.
    pub flip: bool,
    /// Set the picked face onto the plane's level (e.g. the face to Z 0).
    pub place: bool,
}

/// The plain words for a target plane: "G17 (XY)", "G18 (ZX)", "G19 (YZ)".
#[must_use]
pub fn plane_label(plane: ArcPlane) -> &'static str {
    match plane {
        ArcPlane::Xy => "G17 (XY)",
        ArcPlane::Xz => "G18 (ZX)",
        ArcPlane::Yz => "G19 (YZ)",
    }
}

/// (first axis, normal axis index): the plane's first in-plane axis and
/// the axis its normal runs along.
pub(super) fn axes_of(plane: ArcPlane) -> ([f64; 3], usize) {
    match plane {
        ArcPlane::Xy => ([1.0, 0.0, 0.0], 2),
        ArcPlane::Xz => ([1.0, 0.0, 0.0], 1),
        ArcPlane::Yz => ([0.0, 1.0, 0.0], 0),
    }
}

/// The plane's normal as laid: its axis, negated with `flip`.
#[must_use]
pub fn target_normal(lay: PlaneLay) -> [f64; 3] {
    let (_, k) = axes_of(lay.plane);
    let mut n = [0.0; 3];
    n[k] = if lay.flip { -1.0 } else { 1.0 };
    n
}

/// What the pick fixes: a source normal and the point that goes to the
/// plane's level. A face gives its outward normal and centroid; three
/// points (#555) give their plane's away-from-the-material normal and
/// point 1 - both laid by the one [`lay_datum`].
pub(super) struct Datum {
    pub(super) normal: [f64; 3],
    pub(super) anchor: [f64; 3],
}

/// The part alignment (document frame -> setup frame) that lays the
/// picked face on `lay`'s plane, turned about the parts' bounds
/// centre like the Setups page presets. Every refusal names what to
/// re-pick. The identity comes back as an identity `Location`; the
/// command stores it as "as modelled".
pub fn lay_on_plane(world: &World, picks: &[GeoSel], lay: PlaneLay) -> Result<Location, String> {
    lay_datum(world, &datum_of(world, picks)?, lay, Location::default())
}

/// #738: the setup's new part alignment from a face pick - the lay read
/// on the part as `setup` holds it NOW, composed onto that alignment (one
/// more move, not a fresh start from the import pose). The same pick
/// again is a zero step and gives back the held alignment exactly.
pub fn lay_on_plane_in(world: &World, setup: &str, picks: &[GeoSel], lay: PlaneLay) -> Result<Location, String> {
    let held = setup_align::stored(world, setup).unwrap_or_default();
    let step = lay_datum(world, &datum_of(world, picks)?.through(held), lay, held)?;
    Ok(onto_held(step, held))
}

/// #738: a step (setup frame -> setup frame) composed onto the held
/// alignment. A step within float noise of nothing is nothing: the held
/// alignment comes back bit for bit, so "already holds the part that way"
/// still reads for a re-applied pick.
pub(super) fn onto_held(step: Location, held: Location) -> Location {
    let still = |v: f64| v.abs() <= STILL_MM;
    if setup_align::is_identity_rot(step.rot) && still(step.pos.x) && still(step.pos.y) && still(step.pos.z) {
        return held;
    }
    setup_align::compose(step, held)
}

/// A step that moves the part less than this (mm) is no move (#738).
const STILL_MM: f64 = 1e-9;

impl Datum {
    /// The datum as the alignment `held` (document -> setup frame) sees it.
    pub(super) fn through(&self, held: Location) -> Datum {
        let n = quat_rotate_point(held.rot, p3(self.normal));
        let a = transform_point(p3(self.anchor), held.pos, held.rot);
        Datum { normal: [n.x, n.y, n.z], anchor: [a.x, a.y, a.z] }
    }
}

/// The one apply path (#330, #555): turn `datum`'s normal onto `lay`'s
/// axis about the parts' centre, then (with `place`) set its anchor on
/// the plane's level. A face and three points both land here, so the two
/// modes cannot disagree on the same plane. `datum` and the result are in
/// the frame `held` puts the part in (#738): the centre is the parts'
/// bounds centre as held there.
pub(super) fn lay_datum(world: &World, datum: &Datum, lay: PlaneLay, held: Location) -> Result<Location, String> {
    let t3 = target_normal(lay);
    let rot = shortest_arc(datum.normal, t3, lay.plane).ok_or("the picks do not make a usable turn - re-pick them")?;
    // Prove the landing: a turn that misses the axis is refused, never
    // stored as a subtly tilted part.
    // The sine of the miss (the cross product), not a cosine: cos(1e-9)
    // rounds to 1.0 in f64.
    let landed = quat_rotate_point(rot, p3(datum.normal));
    let landed = [landed.x, landed.y, landed.z];
    if !(dot(landed, t3) > 0.0 && norm(cross(landed, t3)) <= LAND_TOL_RAD) {
        return Err("the picks do not make a usable turn - re-pick them".into());
    }
    let c = crate::setup_turn::held_centre(world, held).ok_or("there is no part to align - import a model first")?;
    let rc = quat_rotate_point(rot, c);
    let mut pos = Point3::new(c.x - rc.x, c.y - rc.y, c.z - rc.z);
    if lay.place {
        let a = transform_point(p3(datum.anchor), pos, rot);
        match axes_of(lay.plane).1 {
            0 => pos.x -= a.x,
            1 => pos.y -= a.y,
            _ => pos.z -= a.z,
        }
    }
    let finite = [pos.x, pos.y, pos.z].iter().chain(&rot).all(|v| v.is_finite());
    if !finite {
        return Err("the picks do not make a finite turn - re-pick them".into());
    }
    Ok(Location { pos, rot })
}

/// Read the pick as a datum, in the DOCUMENT frame ([`Datum::through`]
/// carries it into a setup's frame, #738). Exactly one flat face; an edge is refused by name (#554).
fn datum_of(world: &World, picks: &[GeoSel]) -> Result<Datum, String> {
    let mut faces = Vec::new();
    for g in picks {
        let e = model::find_by_name(world, &g.entity).ok_or_else(|| gone(g))?;
        if !setup_align::is_part(world, &e) {
            return Err(format!("'{}' is not the part - pick a face of the model", g.entity));
        }
        match g.feature {
            FeatureSel::Face(_) => faces.push(g),
            FeatureSel::Chain(_) => {
                return Err(format!(
                    "{} of '{}' is an edge - an edge cannot set a plane; pick a flat face \
                     (edges set an axis: Part axes)",
                    g.feature, g.entity
                ));
            }
            FeatureSel::Patch(_) => {
                return Err(format!("{} of '{}' is curved - pick a flat face", g.feature, g.entity));
            }
            FeatureSel::Vertex(_) => {
                return Err(format!("{} of '{}' is a vertex - pick a flat face", g.feature, g.entity));
            }
        }
    }
    match faces.as_slice() {
        [g] => face_datum(world, g),
        _ => Err("pick one flat face".into()),
    }
}

fn face_datum(world: &World, g: &GeoSel) -> Result<Datum, String> {
    let FeatureSel::Face(fid) = g.feature else { return Err(gone(g)) };
    let e = model::find_by_name(world, &g.entity).ok_or_else(|| gone(g))?;
    let features = model::features_of(world, &e).ok_or_else(|| gone(g))?;
    let tris = model::facets_of(world, &e).ok_or_else(|| gone(g))?;
    let f = features.faces.iter().find(|f| f.id == fid).ok_or_else(|| gone(g))?;
    if f.kind == FaceKind::Degenerate {
        return Err(format!("{} of '{}' is degenerate (no usable plane) - pick another face", g.feature, g.entity));
    }
    let normal = outward_normal(world, g, model::transform_of)?
        .ok_or_else(|| format!("{} of '{}' is not flat - pick a flat face", g.feature, g.entity))?;
    let loc = model::transform_of(world, &e).unwrap_or_default();
    // Area-weighted centroid, placed: the point that goes to the level.
    let (mut area, mut sum) = (0.0_f64, [0.0_f64; 3]);
    for t in f.facets.iter().filter_map(|&i| tris.get(i as usize)) {
        let n = (t.b - t.a).cross(t.c - t.a);
        let a = 0.5 * (n.x * n.x + n.y * n.y + n.z * n.z).sqrt();
        if !a.is_finite() {
            continue;
        }
        let m = [(t.a.x + t.b.x + t.c.x) / 3.0, (t.a.y + t.b.y + t.c.y) / 3.0, (t.a.z + t.b.z + t.c.z) / 3.0];
        area += a;
        sum = [sum[0] + a * m[0], sum[1] + a * m[1], sum[2] + a * m[2]];
    }
    if area.is_nan() || area <= MIN_FACE_AREA_MM2 {
        return Err(format!("{} of '{}' has no area - pick a face with a surface", g.feature, g.entity));
    }
    let local = Point3::new(sum[0] / area, sum[1] / area, sum[2] / area);
    let w = transform_point(local, loc.pos, loc.rot);
    Ok(Datum { normal, anchor: [w.x, w.y, w.z] })
}

/// Which way the picks' outside faces before Apply, in plain words, e.g.
/// "outside: +Z in the model of the picked face (away from the
/// material)" - never the face's internal id (#738). Read in the
/// document frame, like [`lay_on_plane`]; refuses as it does.
pub fn plane_side(world: &World, picks: &[GeoSel]) -> Result<String, String> {
    let d = datum_of(world, picks)?;
    let dir = axis_words(d.normal);
    Ok(match picks.first() {
        // #738: the kind, never the welded face's internal id.
        Some(_) => format!("outside: {dir} of the picked face (away from the material)"),
        None => format!("outside: {dir}"),
    })
}

/// A unit direction as "+Z" when it runs along an axis, else its parts.
pub(super) fn axis_words(n: [f64; 3]) -> String {
    for (k, name) in ["X", "Y", "Z"].iter().enumerate() {
        let v = n.get(k).copied().unwrap_or(0.0);
        if (v.abs() - 1.0).abs() <= 1e-9 {
            return format!("{}{name} in the model", if v < 0.0 { "-" } else { "+" });
        }
    }
    format!("({:.3}, {:.3}, {:.3}) in the model", n[0], n[1], n[2])
}

/// The shortest turn taking unit `from` onto unit `to`. Antiparallel: a
/// half turn about the plane's first axis when it lies square to `from`
/// (a G17 flip is "flip over X"), else about any axis square to `from`.
fn shortest_arc(from: [f64; 3], to: [f64; 3], plane: ArcPlane) -> Option<[f64; 4]> {
    let w = 1.0 + dot(from, to);
    if w > 1e-12 {
        let v = cross(from, to);
        return normalize_quat([v[0], v[1], v[2], w]);
    }
    let (t1, _) = axes_of(plane);
    let axis = [t1, [1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]]
        .into_iter()
        .map(|a| sub(a, scale(from, dot(a, from))))
        .find(|a| norm(*a) > 0.5)?;
    let axis = unit(axis)?;
    Some([axis[0], axis[1], axis[2], 0.0])
}

fn unit(a: [f64; 3]) -> Option<[f64; 3]> {
    let l = norm(a);
    (l.is_finite() && l > 0.0).then(|| scale(a, 1.0 / l))
}

fn p3(a: [f64; 3]) -> Point3 {
    Point3::new(a[0], a[1], a[2])
}
