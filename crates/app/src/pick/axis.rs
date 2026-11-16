//! Turn the part in the work plane from picks (#557), or align it in
//! full 3D from its two picked axes (#769): a straight edge, or two
//! points, sets the setup's +Y (and/or +X). The picked direction is
//! read in the setup frame (through the setup's current part alignment,
//! so it composes after a face-to-plane lay, #330).
//!
//! With both axes picked, the two directions define the frame outright
//! (#769, the user's law): the primary axis is kept exactly, the other
//! is made square to it, and +Z lands where the right-hand rule puts it
//! (the picks' cross) - the part is aligned in full 3D, on its head if
//! that is what the picks say, never silently adjusted. With one axis
//! picked the twist about it is free, so it is projected into the work
//! plane (the setup's XY, normal +Z) and the part is turned about Z
//! until it runs along the axis. With both, the primary one (Y by
//! default) wins and the other is only made square. Pure: the result is
//! the setup's new part alignment, handed to `AlignPartToSetup`; the
//! work offset is never touched.
//!
//! Provenance: our own design over plain fixture practice (indicate an
//! edge square to the table's Y travel); the projection onto a plane and
//! the turn about one axis are textbook vector algebra. No vendor
//! material consulted.

use flecs_ecs::prelude::World;

use crate::geometry::{Point3, feature, quat_conjugate, quat_rotate_point, transform_point};
use crate::model;
use crate::operation::{FeatureSel, GeoSel};
use crate::schema::Location;
use crate::setup_align;

use super::align::{cross, dot, gone, norm, quat_from_axes, scale, straight_edge, sub};

/// A direction must lean at least this far out of the plane's normal
/// (degrees) to give an in-plane direction worth trusting.
const MIN_TILT_FROM_NORMAL_DEG: f64 = 5.0;
/// Two picked axes closer than this to parallel (degrees) are refused.
const MIN_AXES_ANGLE_DEG: f64 = 1.0;
/// Two picked points closer than this (mm) are one point.
const COINCIDENT_MM: f64 = 1e-3;
/// The turned direction must land on its axis this closely (sine).
const LAND_TOL: f64 = 1e-9;

/// Which in-plane axis a pick sets.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum InPlaneAxis {
    #[default]
    Y,
    X,
}

impl InPlaneAxis {
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            InPlaneAxis::Y => "+Y",
            InPlaneAxis::X => "+X",
        }
    }
}

/// One axis's picks: one straight edge (start to end, its walk order),
/// or two points (first to second); `flip` reverses the sense.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct AxisPick {
    pub picks: Vec<GeoSel>,
    pub flip: bool,
}

/// What the in-plane turn is read from: +Y and/or +X, and which one wins
/// when both are given (the other is made perpendicular to it).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct InPlaneAxes {
    pub y: Option<AxisPick>,
    pub x: Option<AxisPick>,
    pub primary: InPlaneAxis,
}

/// The turn about the setup's Z (degrees, in (-180, 180]) that lays the
/// picks along their axes. Every refusal names what to re-pick.
pub fn in_plane_turn_deg(world: &World, setup: &str, axes: &InPlaneAxes) -> Result<f64, String> {
    let held = setup_align::stored(world, setup).unwrap_or_default();
    let uy = axes.y.as_ref().map(|p| in_plane(world, held, p, InPlaneAxis::Y)).transpose()?;
    let ux = axes.x.as_ref().map(|p| in_plane(world, held, p, InPlaneAxis::X)).transpose()?;
    if let (Some(y), Some(x)) = (uy, ux) {
        let s = y[0] * x[1] - y[1] * x[0];
        if s.abs() < MIN_AXES_ANGLE_DEG.to_radians().sin() {
            return Err("the +X and +Y picks are parallel - pick directions that cross".into());
        }
        // Right-handed with Z up: X = Y x Z = (y.y, -y.x), i.e. Y runs
        // counter-clockwise from X - the cross X x Y must point up.
        if s > 0.0 {
            return Err("the +X and +Y picks make a left-handed frame (X = Y x Z) - flip one of them".into());
        }
    }
    let angle = match (uy, ux, axes.primary) {
        (Some(y), Some(_), InPlaneAxis::Y) | (Some(y), None, _) => 90.0 - y[1].atan2(y[0]).to_degrees(),
        (_, Some(x), _) => -x[1].atan2(x[0]).to_degrees(),
        (None, None, _) => return Err("pick a straight edge or two points for +Y or +X".into()),
    };
    let mut deg = angle.rem_euclid(360.0);
    if deg > 180.0 {
        deg -= 360.0;
    }
    // A quarter turn within float noise is the exact quarter turn.
    let quarter = (deg / 90.0).round() * 90.0;
    if (deg - quarter).abs() < 1e-9 {
        deg = if quarter == -180.0 { 180.0 } else { quarter };
    }
    if !deg.is_finite() {
        return Err("the picks do not make a finite turn - re-pick them".into());
    }
    Ok(deg)
}

/// The setup's new part alignment: its current one, then the in-plane
/// turn about the parts' centre (as the setup holds them), so the part
/// stays on its spot of the table. An identity result comes back as an
/// identity `Location`; the command stores it as "as modelled".
pub fn turn_in_plane(world: &World, setup: &str, axes: &InPlaneAxes) -> Result<Location, String> {
    let deg = in_plane_turn_deg(world, setup, axes)?;
    let held = setup_align::stored(world, setup).unwrap_or_default();
    let rot = setup_align::axis_quat(2, deg).ok_or("the picks do not make a finite turn - re-pick them")?;
    let c = setup_align::part_centre(world).ok_or("there is no part to align - import a model first")?;
    let c = transform_point(c, held.pos, held.rot);
    let rc = quat_rotate_point(rot, c);
    let turn = Location { pos: Point3::new(c.x - rc.x, c.y - rc.y, c.z - rc.z), rot };
    let out = setup_align::compose(turn, held);
    // Prove the landing: the primary pick runs along its axis now.
    let primary = match (&axes.y, &axes.x, axes.primary) {
        (Some(y), Some(_), InPlaneAxis::Y) | (Some(y), None, _) => Some((y, InPlaneAxis::Y)),
        (_, Some(x), _) => Some((x, InPlaneAxis::X)),
        _ => None,
    };
    if let Some((p, axis)) = primary {
        let d = in_plane(world, out, p, axis)?;
        let want = if axis == InPlaneAxis::Y { [0.0, 1.0] } else { [1.0, 0.0] };
        let miss = d[0] * want[1] - d[1] * want[0];
        if !(d[0] * want[0] + d[1] * want[1] > 0.0 && miss.abs() <= LAND_TOL) {
            return Err("the picks do not make a usable turn - re-pick them".into());
        }
    }
    let finite = [out.pos.x, out.pos.y, out.pos.z].iter().chain(&out.rot).all(|v| v.is_finite());
    if !finite {
        return Err("the picks do not make a finite turn - re-pick them".into());
    }
    Ok(out)
}

/// Align the part from its two picked axes (#769): with both +Y and +X
/// picked, their directions - read through the setup's current part
/// alignment - define the full 3D frame. The primary axis is kept
/// exactly; the other is made square to it (it lands within the skew
/// it came with); and +Z lands where the right-hand rule puts it, the
/// picks' cross - a left-handed pair is honoured literally, not
/// refused: the part lands as picked, on its head if that is what the
/// picks say. The part turns about the centre of its bounds, so it
/// stays on its spot of the table; the origin is NOT set here (the
/// work offset is placed separately). With one axis picked the twist
/// about it is free, so the work plane carries the turn instead
/// ([`turn_in_plane`]). Every refusal names what to re-pick.
pub fn align_from_axes(world: &World, setup: &str, axes: &InPlaneAxes) -> Result<Location, String> {
    let (Some(y), Some(x)) = (&axes.y, &axes.x) else {
        return turn_in_plane(world, setup, axes);
    };
    if y.picks.is_empty() || x.picks.is_empty() {
        return turn_in_plane(world, setup, axes);
    }
    let held = setup_align::stored(world, setup).unwrap_or_default();
    let dy = dir_through(world, held.rot, y, InPlaneAxis::Y)?;
    let dx = dir_through(world, held.rot, x, InPlaneAxis::X)?;
    let (p, s) = if axes.primary == InPlaneAxis::Y { (dy, dx) } else { (dx, dy) };
    // The picks must cross: running along each other, either sense,
    // leaves no frame to read.
    let along = dot(p, s);
    if along.abs() > MIN_AXES_ANGLE_DEG.to_radians().cos() {
        return Err("the +X and +Y picks run (nearly) along each other - pick directions that cross".into());
    }
    // The secondary, made square to the primary. The primary lands
    // exactly; the secondary lands within the skew it came with
    // (|along|, the part the squaring removes).
    let s2 = sub(s, scale(p, along));
    let l2 = norm(s2);
    if !l2.is_finite() || l2 <= 0.0 {
        return Err("the picks do not make a usable frame - re-pick them".into());
    }
    let s2 = scale(s2, 1.0 / l2);
    // The picked frame, columns in axis order; their cross closes it
    // right-handed. +Z lands where the right-hand rule puts it.
    let (colx, coly) = if axes.primary == InPlaneAxis::Y { (s2, p) } else { (p, s2) };
    let colz = cross(colx, coly);
    let q = quat_from_axes(colx, coly, colz).ok_or("the picks do not make a finite frame - re-pick them")?;
    // The rotation that takes the picked frame onto the world axes.
    let rot = quat_conjugate(q);
    // #1347: picks the held alignment already lands make no turn - the
    // held alignment comes back as it is (the command then refuses
    // "already holds the part that way"), never re-composed with float
    // noise into a no-op change. Only a turn below the landing tolerance
    // counts as none; the landing proof below still checks it.
    let out = if is_no_turn(rot) {
        held
    } else {
        // Turn about the centre of the parts' bounds, as the setup holds
        // them, so the part stays on its spot of the table.
        let c = setup_align::part_centre(world).ok_or("there is no part to align - import a model first")?;
        let c = transform_point(c, held.pos, held.rot);
        let rc = quat_rotate_point(rot, c);
        let turn = Location { pos: Point3::new(c.x - rc.x, c.y - rc.y, c.z - rc.z), rot };
        setup_align::compose(turn, held)
    };
    // Prove the landing before the command stores anything: through
    // the new alignment the primary runs along its axis exactly, the
    // secondary within the skew it came with - never a silently wrong
    // frame.
    let ly = dir_through(world, out.rot, y, InPlaneAxis::Y)?;
    let lx = dir_through(world, out.rot, x, InPlaneAxis::X)?;
    let (lp, ls) = if axes.primary == InPlaneAxis::Y { (ly, lx) } else { (lx, ly) };
    let (want_p, want_s) = if axes.primary == InPlaneAxis::Y {
        ([0.0, 1.0, 0.0], [1.0, 0.0, 0.0])
    } else {
        ([1.0, 0.0, 0.0], [0.0, 1.0, 0.0])
    };
    let miss = |v: [f64; 3], want: [f64; 3]| norm(cross(v, want));
    if miss(lp, want_p) > LAND_TOL || miss(ls, want_s) > along.abs() + LAND_TOL {
        return Err("the picks do not make a usable frame - re-pick them".into());
    }
    let finite = [out.pos.x, out.pos.y, out.pos.z].iter().chain(&out.rot).all(|v| v.is_finite());
    if !finite {
        return Err("the picks do not make a finite alignment - re-pick them".into());
    }
    Ok(out)
}

/// Is the unit quaternion `rot` ([x, y, z, w]) a turn of less than a
/// quarter of [`LAND_TOL`] (radians), either sign? Its vector part is
/// sin(angle / 2); a NaN reads as a turn.
fn is_no_turn(rot: [f64; 4]) -> bool {
    let half_sin = norm([rot[0], rot[1], rot[2]]);
    half_sin <= LAND_TOL / 8.0
}

/// One axis pick's direction (unit, the flip honored), read through
/// the rotation `rot` - the setup's held alignment while the frame is
/// built, the resulting alignment while the landing is proved.
fn dir_through(world: &World, rot: [f64; 4], pick: &AxisPick, axis: InPlaneAxis) -> Result<[f64; 3], String> {
    let name = axis.label();
    let (a, b) = ends(world, pick, name)?;
    let d = sub(b, a);
    let d = quat_rotate_point(rot, Point3::new(d[0], d[1], d[2]));
    let d = if pick.flip { [-d.x, -d.y, -d.z] } else { [d.x, d.y, d.z] };
    let len = norm(d);
    if !len.is_finite() || len <= 0.0 {
        return Err(format!("the {name} pick has no length - re-pick it"));
    }
    Ok(scale(d, 1.0 / len))
}

/// One axis's pick as a unit direction in the work plane ([x, y]),
/// read through the alignment `held` (document frame -> setup frame).
fn in_plane(world: &World, held: Location, pick: &AxisPick, axis: InPlaneAxis) -> Result<[f64; 2], String> {
    let name = axis.label();
    let (a, b) = ends(world, pick, name)?;
    let d = sub(b, a);
    let d = quat_rotate_point(held.rot, Point3::new(d[0], d[1], d[2]));
    let d = if pick.flip { [-d.x, -d.y, -d.z] } else { [d.x, d.y, d.z] };
    let len = norm(d);
    let flat = d[0].hypot(d[1]);
    if !(len.is_finite() && flat.is_finite()) || len <= 0.0 {
        return Err(format!("the {name} pick has no length - re-pick it"));
    }
    if flat < len * MIN_TILT_FROM_NORMAL_DEG.to_radians().sin() {
        return Err(format!(
            "the {name} pick runs (nearly) along the work plane's normal (Z) - pick an edge or two points \
             that run across the plane"
        ));
    }
    Ok([d[0] / flat, d[1] / flat])
}

/// The pick's (start, end) in the DOCUMENT frame: one straight edge, or
/// two points.
fn ends(world: &World, pick: &AxisPick, name: &str) -> Result<([f64; 3], [f64; 3]), String> {
    for g in &pick.picks {
        let e = model::find_by_name(world, &g.entity).ok_or_else(|| gone(g))?;
        if !setup_align::is_part(world, &e) {
            return Err(format!("'{}' is not the part - pick an edge or points of the model for {name}", g.entity));
        }
        if matches!(g.feature, FeatureSel::Face(_) | FeatureSel::Patch(_)) {
            return Err(format!(
                "{} of '{}' is a face - a face sets no direction; pick a straight edge or two points for {name}",
                g.feature, g.entity
            ));
        }
    }
    let role = format!("for {name}");
    match pick.picks.as_slice() {
        [] => Err(format!("pick a straight edge or two points for {name}")),
        [g] if matches!(g.feature, FeatureSel::Chain(_)) => straight_edge(world, g, model::transform_of, &role),
        [g] => Err(format!("{} of '{}' is one point - pick a second point for {name}, or a straight edge", g.feature, g.entity)),
        // #692: either point may be a round edge's centre.
        [p, q] => {
            let pt = |g: &GeoSel| {
                point(world, g).map_err(|why| match g.feature {
                    FeatureSel::Chain(_) => format!("pick one straight edge, or two points, for {name}: {why}"),
                    _ => why,
                })
            };
            let (a, b) = (pt(p)?, pt(q)?);
            let gap = norm(sub(b, a));
            if p == q || !gap.is_finite() || gap <= COINCIDENT_MM {
                return Err(format!("the two points for {name} coincide - pick two points apart"));
            }
            Ok((a, b))
        }
        _ => Err(format!("pick one straight edge, or two points, for {name}")),
    }
}

/// Where one axis pick sits, in the DOCUMENT frame (mm), for readouts:
/// a vertex's point, a straight edge's (start, end), or a round edge's
/// centre (#692). `None` for a face, a curve that is no circle, or a pick
/// no longer in the model.
#[must_use]
pub fn pick_coords(world: &World, g: &GeoSel) -> Option<Vec<[f64; 3]>> {
    match g.feature {
        FeatureSel::Vertex(_) => point(world, g).ok().map(|p| vec![p]),
        FeatureSel::Chain(_) => straight_edge(world, g, model::transform_of, "")
            .ok()
            .map(|(a, b)| vec![a, b])
            .or_else(|| super::centre::centre_of(world, g).ok().map(|c| vec![c])),
        FeatureSel::Face(_) | FeatureSel::Patch(_) => None,
    }
}

/// A picked point in the document frame: a vertex, or (#692) the centre
/// of a round edge - a straight or non-circular edge refused by name.
pub(super) fn point(world: &World, g: &GeoSel) -> Result<[f64; 3], String> {
    let id = match g.feature {
        FeatureSel::Vertex(id) => id,
        FeatureSel::Chain(_) => return super::centre::centre_point(world, g),
        FeatureSel::Face(_) | FeatureSel::Patch(_) => return Err(gone(g)),
    };
    let e = model::find_by_name(world, &g.entity).ok_or_else(|| gone(g))?;
    let features = model::features_of(world, &e).ok_or_else(|| gone(g))?;
    let vi = feature::resolve_vertex(&features, id).ok_or_else(|| gone(g))?;
    let v = features.verts.get(vi as usize).ok_or_else(|| gone(g))?;
    let loc = model::transform_of(world, &e).unwrap_or_default();
    let w = transform_point(Point3::new(v[0], v[1], v[2]), loc.pos, loc.rot);
    let out = [w.x, w.y, w.z];
    if !out.iter().all(|c| c.is_finite()) {
        return Err(format!("{} of '{}' is not a finite point - re-pick it", g.feature, g.entity));
    }
    Ok(out)
}
