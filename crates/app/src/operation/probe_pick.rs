//! The Probe operation seeded from one picked feature (#171): a planar
//! face seeds a Face touch against its outward normal, a straight edge
//! an Edge touch on the side face under it, a vertex an outside Corner
//! (one X and one Y touch, off the two walls meeting there), and a
//! round wall or rim a Bore or Boss centre (see `probe_pick_round`).
//! Anything the probe cannot touch honestly is refused by name - never
//! guessed. The seed is written into the op's params by the pick
//! command itself (`SetOperationGeometry`), so one undo takes it back.
//!
//! Provenance: touch-probe practice from machine-tool handbooks (a
//! touch travels against the surface normal; an outside corner takes
//! one touch per wall); outward normals from facet winding (standard
//! mesh geometry). Our own law and code; no vendor material consulted.

use std::sync::Arc;

use flecs_ecs::prelude::World;

use crate::geometry::feature::{self, FaceKind, Features};
use crate::geometry::{Point3, Triangle, quat_rotate_point, transform_point};
use crate::model;
use crate::probing::{ProbeDir, ProbeMode};
use crate::schema::Location;

use super::{FeatureSel, GeoSel, OpParams};

/// How far (degrees) a face normal may stand off an axis and still be
/// touched along it: a probe travels along X, Y or down Z only, and
/// an exact CAD face is exact - this is tessellation slack, not a lean.
pub const PROBE_AXIS_TOL_DEG: f64 = 1.0;

/// Positional slack (mm) for "straight", "level" and "below": the facet
/// lattice's own, like the circle fit's tolerance.
pub(super) const PICK_TOL: f64 = 0.01;

/// What one pick seeds into a Probe op: the mode, the touch directions,
/// the expected contact, and - for a round pick - the nominal radius.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ProbeSeed {
    pub mode: ProbeMode,
    pub dir: ProbeDir,
    pub dir_y: ProbeDir,
    pub target: [f64; 3],
    pub radius: Option<f64>,
}

fn dir_word(d: ProbeDir) -> &'static str {
    match d {
        ProbeDir::XPos => "+X",
        ProbeDir::XNeg => "-X",
        ProbeDir::YPos => "+Y",
        ProbeDir::YNeg => "-Y",
        ProbeDir::ZNeg => "-Z",
    }
}

impl ProbeSeed {
    /// One line for the op page: what the pick made of the probe.
    #[must_use]
    pub fn describe(&self) -> String {
        let [x, y, z] = self.target;
        let at = format!("({x:.3}, {y:.3}, {z:.3})");
        match self.mode {
            ProbeMode::Face => format!("Face touch toward {} at {at}", dir_word(self.dir)),
            ProbeMode::Edge => format!("Edge touch toward {} under {at}", dir_word(self.dir)),
            ProbeMode::Corner => format!(
                "Outside corner, touches toward {} and {} at {at}",
                dir_word(self.dir),
                dir_word(self.dir_y)
            ),
            ProbeMode::Bore | ProbeMode::Boss => format!(
                "{} centre, radius {:.3} mm, rim centre {at}",
                if self.mode == ProbeMode::Bore { "Bore" } else { "Boss" },
                self.radius.unwrap_or(0.0)
            ),
        }
    }
}

/// The picked entity's topology, facets and placement: every query
/// below answers in WORLD coordinates, the frame the probe's target and
/// its stylus law (`probe_op`, over world facets) both live in.
pub(super) struct PickCtx {
    pub features: Arc<Features>,
    pub tris: Arc<[Triangle]>,
    pub loc: Location,
}

impl PickCtx {
    pub(super) fn of(world: &World, entity: &str) -> Result<PickCtx, String> {
        let e = model::find_by_name(world, entity)
            .ok_or_else(|| format!("'{entity}' is gone - re-pick the feature"))?;
        let none = || format!("'{entity}' carries no geometry to probe");
        let features = model::features_of(world, &e).ok_or_else(none)?;
        let tris = model::facets_of(world, &e).ok_or_else(none)?;
        let loc = model::transform_of(world, &e).unwrap_or_default();
        Ok(PickCtx { features, tris, loc })
    }

    pub(super) fn point(&self, v: u32) -> Option<Point3> {
        let p = self.features.verts.get(v as usize)?;
        let w = transform_point(Point3::new(p[0], p[1], p[2]), self.loc.pos, self.loc.rot);
        w.is_finite().then_some(w)
    }

    pub(super) fn world(&self, p: [f64; 3]) -> Point3 {
        transform_point(Point3::new(p[0], p[1], p[2]), self.loc.pos, self.loc.rot)
    }

    /// The face's outward unit normal in world, from its facets' winding
    /// (the stored plane normal is winding-blind); `None` when the
    /// facets cancel or carry no area.
    pub(super) fn outward(&self, face: usize) -> Option<Point3> {
        let f = self.features.faces.get(face)?;
        let mut n = Point3::new(0.0, 0.0, 0.0);
        for &i in &f.facets {
            let Some(t) = self.tris.get(i as usize) else { continue };
            let c = (t.b - t.a).cross(t.c - t.a);
            if c.is_finite() {
                n = Point3::new(n.x + c.x, n.y + c.y, n.z + c.z);
            }
        }
        let w = quat_rotate_point(self.loc.rot, n);
        let len = w.dot(w).sqrt();
        (len.is_finite() && len > 1e-12).then(|| Point3::new(w.x / len, w.y / len, w.z / len))
    }

    /// The face's area centroid in world.
    pub(super) fn centroid(&self, face: usize) -> Option<Point3> {
        let f = self.features.faces.get(face)?;
        let c = crate::pick::facets_centroid(&self.tris, f.facets.iter().map(|&i| i as usize))?;
        Some(self.world(c))
    }

    /// The distinct faces (indices into `features.faces`) bounding the
    /// given feature edges.
    pub(super) fn faces_of_edges(&self, edges: impl IntoIterator<Item = u32>) -> Vec<usize> {
        let mut out: Vec<usize> = Vec::new();
        for ei in edges {
            let Some(e) = self.features.edges.get(ei as usize) else { continue };
            for &t in e.facets.iter().take(e.n_facets.min(3) as usize) {
                if let Some(&fi) = self.features.facet_face.get(t as usize)
                    && !out.contains(&(fi as usize))
                {
                    out.push(fi as usize);
                }
            }
        }
        out
    }
}

/// Which axis a unit normal stands along, within the tolerance: 0, 1
/// or 2 for X, Y, Z, and its sign.
pub(super) fn axis_of(n: Point3) -> Option<(usize, f64)> {
    let cos = PROBE_AXIS_TOL_DEG.to_radians().cos();
    [n.x, n.y, n.z]
        .into_iter()
        .enumerate()
        .find(|(_, c)| c.abs() >= cos)
        .map(|(k, c)| (k, c.signum()))
}

/// The touch that travels AGAINST an outward normal: `None` for a face
/// looking down (the stylus only descends) or off every axis.
fn against(n: Point3) -> Option<ProbeDir> {
    match axis_of(n)? {
        (0, s) => Some(if s > 0.0 { ProbeDir::XNeg } else { ProbeDir::XPos }),
        (1, s) => Some(if s > 0.0 { ProbeDir::YNeg } else { ProbeDir::YPos }),
        (2, s) if s > 0.0 => Some(ProbeDir::ZNeg),
        _ => None,
    }
}

fn off_axis(n: Point3) -> String {
    let worst = n.x.abs().max(n.y.abs()).max(n.z.abs()).clamp(-1.0, 1.0);
    format!("{:.1} deg", worst.acos().to_degrees())
}

/// The seed one pick makes, or the named reason it cannot make one.
pub fn probe_seed(world: &World, sel: &GeoSel) -> Result<ProbeSeed, String> {
    let cx = PickCtx::of(world, &sel.entity)?;
    match sel.feature {
        FeatureSel::Face(id) => face_seed(&cx, id),
        FeatureSel::Chain(id) => {
            let chain = crate::model::resolve_chain(&cx.features, id)
                .ok_or("the picked edge is no longer in the model - re-pick it")?;
            if chain.closed {
                super::probe_pick_round::rim_seed(&cx, &chain)
            } else {
                edge_seed(&cx, &chain)
            }
        }
        FeatureSel::Patch(id) => super::probe_pick_round::wall_seed(&cx, id),
        FeatureSel::Vertex(id) => corner_seed(&cx, id),
    }
}

fn face_seed(cx: &PickCtx, id: u64) -> Result<ProbeSeed, String> {
    let gone = "the picked face is no longer in the model - re-pick it";
    let fi = cx.features.faces.iter().position(|f| f.id == id).ok_or(gone)?;
    let f = cx.features.faces.get(fi).ok_or(gone)?;
    if f.kind != FaceKind::Planar {
        return Err("the picked face is not planar - a face touch needs a flat face".into());
    }
    let n = cx.outward(fi).ok_or("the picked face has no area to touch")?;
    let dir = match against(n) {
        Some(d) => d,
        None if axis_of(n) == Some((2, -1.0)) => {
            return Err("the picked face looks down - the stylus only descends in Z; probe it from a side".into());
        }
        None => {
            return Err(format!(
                "the picked face stands {} off X, Y and Z - a probe touches along an axis",
                off_axis(n)
            ));
        }
    };
    let target = on_face(cx, fi).ok_or("the picked face has no area to touch")?;
    Ok(ProbeSeed { mode: ProbeMode::Face, dir, dir_y: ProbeDir::YPos, target: [target.x, target.y, target.z], radius: None })
}

/// A point ON the face to touch: its area centroid when that lands on
/// one of its facets, else the nearest facet's own centroid (a face
/// round a hole has its centroid in the hole - air, not the face). The
/// pick names a feature, not a click point, so this stands for it.
fn on_face(cx: &PickCtx, fi: usize) -> Option<Point3> {
    let f = cx.features.faces.get(fi)?;
    let c = cx.centroid(fi)?;
    let mut best: Option<(f64, Point3)> = None;
    for &i in &f.facets {
        let Some(t) = cx.tris.get(i as usize) else { continue };
        let (a, b, d) = (cx.world([t.a.x, t.a.y, t.a.z]), cx.world([t.b.x, t.b.y, t.b.z]), cx.world([t.c.x, t.c.y, t.c.z]));
        if inside(c, a, b, d) {
            return Some(c);
        }
        let m = Point3::new((a.x + b.x + d.x) / 3.0, (a.y + b.y + d.y) / 3.0, (a.z + b.z + d.z) / 3.0);
        let dist = (m - c).dot(m - c);
        if m.is_finite() && best.is_none_or(|(bd, _)| dist < bd) {
            best = Some((dist, m));
        }
    }
    best.map(|(_, m)| m)
}

/// Is `p` (in the triangle's plane) inside triangle `abc`?
fn inside(p: Point3, a: Point3, b: Point3, c: Point3) -> bool {
    let n = (b - a).cross(c - a);
    let s = |u: Point3, v: Point3| (v - u).cross(p - u).dot(n);
    let (x, y, z) = (s(a, b), s(b, c), s(c, a));
    n.dot(n) > 0.0 && ((x >= 0.0 && y >= 0.0 && z >= 0.0) || (x <= 0.0 && y <= 0.0 && z <= 0.0))
}

/// The one vertical, axis-facing face among `faces` that hangs below
/// height `z`: the wall an edge or corner touch lands on.
fn walls_below(cx: &PickCtx, faces: &[usize], z: f64) -> Vec<(usize, ProbeDir, Point3)> {
    faces
        .iter()
        .filter_map(|&fi| {
            let n = cx.outward(fi)?;
            let dir = against(n).filter(|d| d.is_x() || d.is_y())?;
            let c = cx.centroid(fi)?;
            (c.z < z - PICK_TOL).then_some((fi, dir, c))
        })
        .collect()
}

fn edge_seed(cx: &PickCtx, chain: &feature::Chain) -> Result<ProbeSeed, String> {
    let pts: Vec<Point3> = feature::chain_points(&cx.features, chain).iter().map(|p| cx.world(*p)).collect();
    let (Some(&a), Some(&b)) = (pts.first(), pts.last()) else {
        return Err("the picked edge has no length".into());
    };
    let d = b - a;
    let len = d.dot(d).sqrt();
    if !(len.is_finite() && len > PICK_TOL) {
        return Err("the picked edge has no length".into());
    }
    let u = Point3::new(d.x / len, d.y / len, d.z / len);
    let bend = pts.iter().map(|p| (*p - a).cross(u)).map(|v| v.dot(v).sqrt()).fold(0.0, f64::max);
    if bend.is_nan() || bend > PICK_TOL {
        return Err(format!(
            "the picked edge is not straight (it bends {bend:.3} mm off its chord) - an edge touch needs a straight edge; pick a vertex or a round rim instead"
        ));
    }
    if (b.z - a.z).abs() > PICK_TOL {
        return Err("the picked edge is not level - an edge touch runs under a horizontal top edge".into());
    }
    let walls = walls_below(cx, &cx.faces_of_edges(chain.edges.iter().copied()), a.z);
    let [(_, dir, _)] = walls.as_slice() else {
        return Err(format!(
            "the picked edge has {} vertical side faces facing X or Y under it - an edge touch needs exactly one",
            walls.len()
        ));
    };
    let m = Point3::new((a.x + b.x) * 0.5, (a.y + b.y) * 0.5, (a.z + b.z) * 0.5);
    Ok(ProbeSeed { mode: ProbeMode::Edge, dir: *dir, dir_y: ProbeDir::YPos, target: [m.x, m.y, m.z], radius: None })
}

fn corner_seed(cx: &PickCtx, id: u64) -> Result<ProbeSeed, String> {
    let vi = feature::resolve_vertex(&cx.features, id)
        .ok_or("the picked vertex is no longer in the model - re-pick it")?;
    let v = cx.point(vi).ok_or("the picked vertex is not finite")?;
    let edges = cx.features.edges.iter().enumerate().filter(|(_, e)| e.a == vi || e.b == vi);
    let faces = cx.faces_of_edges(edges.map(|(i, _)| i as u32));
    // Every face meeting the corner is a wall facing X or Y, or level
    // (the top): a chamfer or a draft there is no outside corner.
    for &fi in &faces {
        let n = cx.outward(fi).ok_or("a face at the picked vertex has no area")?;
        if axis_of(n).is_none() {
            return Err(format!(
                "a face at the picked vertex stands {} off the axes - an outside corner takes two vertical walls facing X and Y",
                off_axis(n)
            ));
        }
    }
    let walls = walls_below(cx, &faces, v.z);
    let xs: Vec<_> = walls.iter().filter(|w| w.1.is_x()).collect();
    let ys: Vec<_> = walls.iter().filter(|w| w.1.is_y()).collect();
    let ([(_, dx, cxw)], [(_, dy, cyw)]) = (xs.as_slice(), ys.as_slice()) else {
        return Err(format!(
            "the picked vertex tops {} wall(s) facing X and {} facing Y - an outside corner takes exactly one of each below it",
            xs.len(),
            ys.len()
        ));
    };
    // Outside, not inside: each wall runs away from the vertex the way
    // the OTHER touch travels (`probing::contacts` insets each touch
    // along that wall) - an inside corner's walls run the other way.
    let along_y = (cxw.y - v.y) * dy.unit().y;
    let along_x = (cyw.x - v.x) * dx.unit().x;
    if !(along_y > PICK_TOL && along_x > PICK_TOL) {
        return Err("the picked vertex is an inside corner - the corner probe finds an outside one".into());
    }
    Ok(ProbeSeed { mode: ProbeMode::Corner, dir: *dx, dir_y: *dy, target: [v.x, v.y, v.z], radius: None })
}

/// Write a seed into a Probe op's params; `None` for any other kind.
/// The numbers the pick does not speak to (feeds, distances, result)
/// stay the author's.
#[must_use]
pub fn seeded_params(params: &OpParams, seed: &ProbeSeed) -> Option<OpParams> {
    let mut out = params.clone();
    let OpParams::Probe { mode, dir, dir_y, target, radius, .. } = &mut out else {
        return None;
    };
    *mode = seed.mode;
    *dir = seed.dir;
    *dir_y = seed.dir_y;
    *target = seed.target;
    if let Some(r) = seed.radius {
        *radius = r;
    }
    Some(out)
}

/// A Probe op's params re-seeded from its pick list - what the pick
/// command writes. `None` when `params` is not a probe (every other
/// kind's picks bound a region instead). One pick only.
#[must_use]
pub fn probe_from_picks(world: &World, params: &OpParams, items: &[GeoSel]) -> Option<Result<OpParams, String>> {
    if !matches!(params, OpParams::Probe { .. }) {
        return None;
    }
    let [sel] = items else {
        return Some(Err(format!(
            "a probe is seeded from one pick, not {} - pick one face, edge, vertex or round wall",
            items.len()
        )));
    };
    Some(probe_seed(world, sel).and_then(|s| seeded_params(params, &s).ok_or_else(|| "not a probe".to_owned())))
}

/// The op page's account of a probe's pick: the seed it made, or why
/// it makes none. `None` for other kinds.
#[must_use]
pub fn probe_account(world: &World, items: &[GeoSel], params: &OpParams) -> Option<(Vec<String>, Option<String>)> {
    if !matches!(params, OpParams::Probe { .. }) || items.is_empty() {
        return None;
    }
    Some(match items {
        [sel] => match probe_seed(world, sel) {
            Ok(s) => (vec![format!("Probe from the pick: {}", s.describe())], None),
            Err(e) => (Vec::new(), Some(e)),
        },
        _ => (Vec::new(), Some("a probe is seeded from one pick - remove the others".into())),
    })
}
