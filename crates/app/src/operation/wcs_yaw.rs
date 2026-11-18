//! Rigid XY frame carry for a Z-rotated work offset (#760).
//!
//! Provenance: our own coordinate-frame adapter (rotates milling inputs into the
//! Z-rotated WCS axes for path generation and maps generated CL paths and
//! located refusals back into Setup coordinates before verification and workholding).

use std::sync::Arc;

use flecs_ecs::prelude::World;

use crate::cl::{CompSpan, Cycle, EdgeBreak, Move, Toolpath};
use crate::geo2d::{Loop, Point2, Region};
use crate::geometry::{Point3, Triangle};
use crate::machining2d::{self, GenError, SlotBound};

use super::boundary::{AreaCtx, OpenGeom};
use super::selection::SelectionGeometry;
use super::{Material, OpParams};

/// Rigid XY rotation `(origin, yaw_rad)` of the active setup's Z-rotated WCS.
#[derive(Clone, Copy, Debug)]
pub(super) struct WcsYaw {
    pub origin: [f64; 2],
    pub yaw_rad: f64,
}

/// Snapshot of unmodified Setup `S` inputs preserved across `W_gen` execution so
/// post-generation verification, solid checks, and workholding run in `S`.
pub(super) struct SavedS {
    pub material_facets: Vec<Triangle>,
    pub stock_facets: Option<Vec<Triangle>>,
    pub params: OpParams,
    pub keep_parts: Vec<Triangle>,
    pub neighbour_parts: Vec<Triangle>,
}

struct Rot2 {
    ox: f64,
    oy: f64,
    s: f64,
    c: f64,
}

impl Rot2 {
    fn new(origin: [f64; 2], angle_rad: f64) -> Self {
        let (s, c) = angle_rad.sin_cos();
        Self { ox: origin[0], oy: origin[1], s, c }
    }

    fn p2(&self, p: Point2) -> Point2 {
        let (dx, dy) = (p.x - self.ox, p.y - self.oy);
        Point2::new(self.ox + dx * self.c - dy * self.s, self.oy + dx * self.s + dy * self.c)
    }

    fn p3(&self, p: Point3) -> Point3 {
        let (dx, dy) = (p.x - self.ox, p.y - self.oy);
        Point3::new(self.ox + dx * self.c - dy * self.s, self.oy + dx * self.s + dy * self.c, p.z)
    }

    fn facets(&self, facets: &[Triangle]) -> Vec<Triangle> {
        facets.iter().map(|t| Triangle::new(self.p3(t.a), self.p3(t.b), self.p3(t.c))).collect()
    }

    fn aabb(&self, (lo, hi): (Point3, Point3)) -> (Point3, Point3) {
        let mut out_lo = Point3::new(f64::INFINITY, f64::INFINITY, lo.z.min(hi.z));
        let mut out_hi = Point3::new(f64::NEG_INFINITY, f64::NEG_INFINITY, lo.z.max(hi.z));
        for &x in &[lo.x, hi.x] {
            for &y in &[lo.y, hi.y] {
                let p = self.p3(Point3::new(x, y, lo.z));
                out_lo.x = out_lo.x.min(p.x);
                out_lo.y = out_lo.y.min(p.y);
                out_hi.x = out_hi.x.max(p.x);
                out_hi.y = out_hi.y.max(p.y);
            }
        }
        (out_lo, out_hi)
    }

    fn loop2(&self, l: &Loop) -> Result<Loop, GenError> {
        Ok(Loop::new(l.points().iter().copied().map(|p| self.p2(p)).collect())?)
    }

    fn region(&self, r: &Region) -> Result<Region, GenError> {
        let loops = r.loops().iter().map(|l| self.loop2(l)).collect::<Result<Vec<_>, _>>()?;
        Ok(Region::new(loops)?)
    }

    fn mv_xy(&self, m: &Move) -> Move {
        Move {
            from: self.p3(m.from),
            to: self.p3(m.to),
            arc: m.arc.map(|mut a| {
                if a.plane.is_xy() {
                    a.center = self.p2(a.center);
                }
                a
            }),
            ..*m
        }
    }

    fn toolpath(&self, path: &Toolpath, chord_tol: f64) -> Toolpath {
        let mut moves = Vec::with_capacity(path.moves.len());
        let mut starts = Vec::with_capacity(path.moves.len());
        for m in &path.moves {
            starts.push(moves.len());
            if let Some(arc) = m.arc
                && !arc.plane.is_xy()
            {
                for (a, b) in m.segments3(chord_tol) {
                    let (a, b) = (self.p3(a), self.p3(b));
                    if a != b {
                        moves.push(Move { from: a, to: b, arc: None, ..*m });
                    }
                }
            } else {
                moves.push(self.mv_xy(m));
            }
        }
        let remap = |i: usize| starts.get(i).copied().unwrap_or(moves.len());
        let cycles = path
            .cycles
            .iter()
            .map(|c| Cycle { xy: self.p2(c.xy), first_move: remap(c.first_move), ..*c })
            .collect();
        let comps = path
            .comps
            .iter()
            .map(|cs| CompSpan {
                first: remap(cs.first),
                last: remap(cs.last.saturating_add(1)).saturating_sub(1),
                programmed: cs.programmed.iter().map(|m| self.mv_xy(m)).collect(),
                ..*cs
            })
            .collect();
        let breaks = path
            .breaks
            .iter()
            .map(|b| {
                let mut rotated = EdgeBreak {
                    edge: b.edge.iter().copied().map(|p| self.p3(p)).collect(),
                    ..b.clone()
                };
                if let Some(volume) = &mut rotated.volume {
                    // Cap frames are directions, so rotate without translating
                    // by the work-offset origin. Width and profile stay intact.
                    let dir = |v: [f64; 3]| [v[0] * self.c - v[1] * self.s, v[0] * self.s + v[1] * self.c, v[2]];
                    for frame in &mut volume.frames {
                        frame.outward = dir(frame.outward);
                        frame.across = dir(frame.across);
                    }
                }
                rotated
            })
            .collect();
        Toolpath {
            moves,
            cycles,
            notes: path.notes.clone(),
            comps,
            threads: path.threads.clone(),
            probes: path.probes.clone(),
            breaks,
            plasma: path.plasma.clone(),
        }
    }
}

/// True when `params` is an upright 3-axis milling operation that aligns to a
/// pure Z-rotated WCS (`params.tilt()` is `None`).
pub(crate) fn op_applies(kind: crate::machine::Kind, params: &OpParams) -> bool {
    if !matches!(kind, crate::machine::Kind::Mill | crate::machine::Kind::MillTurn) || params.tilt().is_some() {
        return false;
    }
    match params {
        OpParams::Face { .. }
        | OpParams::Contour { .. }
        | OpParams::Pocket { .. }
        | OpParams::Adaptive2D { .. }
        | OpParams::Offset2D { .. }
        | OpParams::Engrave { .. }
        | OpParams::Chamfer2D { .. }
        | OpParams::Circular { .. }
        | OpParams::Slot { .. }
        | OpParams::TrochoidalSlot { .. }
        | OpParams::Trace { .. }
        | OpParams::Pocket3D { .. }
        | OpParams::Finish3D { .. }
        | OpParams::Deburr { .. }
        | OpParams::Bore { .. }
        | OpParams::Thread { .. } => true,
        OpParams::Drill { cross, .. } => cross.is_empty(),
        _ => false,
    }
}

impl WcsYaw {
    /// Read active Z-yaw from the setup's WCS when eligible for this operation.
    pub(super) fn from_world(world: &World, setup: Option<&str>, kind: crate::machine::Kind, params: &OpParams) -> Option<Self> {
        if !op_applies(kind, params) {
            return None;
        }
        let deg = crate::posting::wcs_yaw(world, setup)?;
        let (_, origin) = crate::posting::wcs_context(world, setup);
        Some(Self { origin: [origin[0], origin[1]], yaw_rad: deg.to_radians() })
    }

    /// Transform generation inputs into `W_gen` (`-yaw` about `origin`) and
    /// return the unmodified `S` copies for post-generation checks.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn enter(
        self,
        material: &mut Material,
        stock_facets: &mut Option<Vec<Triangle>>,
        stock_aabb: &mut Option<(Point3, Point3)>,
        picked_region: &mut Option<Result<Region, GenError>>,
        contour_sel: &mut Option<Result<SelectionGeometry, GenError>>,
        drive_chains: &mut Option<Result<Vec<Vec<Point3>>, GenError>>,
        slot_bound: &mut Option<Result<SlotBound, GenError>>,
        prior: &mut Option<Result<Arc<crate::rest::PriorState>, GenError>>,
        drills_ahead: &mut Vec<crate::rest::DrillAhead>,
        keep_parts: &mut Vec<Triangle>,
        neighbour_parts: &mut Vec<Triangle>,
        area: &mut Option<Result<AreaCtx, GenError>>,
        params: &mut OpParams,
    ) -> Result<SavedS, GenError> {
        let saved = SavedS {
            material_facets: material.facets.clone(),
            stock_facets: stock_facets.clone(),
            params: params.clone(),
            keep_parts: keep_parts.clone(),
            neighbour_parts: neighbour_parts.clone(),
        };
        let r = Rot2::new(self.origin, -self.yaw_rad);
        let o2 = Point2::new(self.origin[0], self.origin[1]);
        material.facets = r.facets(&material.facets);
        if let Some(sf) = stock_facets {
            *sf = r.facets(sf);
        }
        if let Some(a) = stock_aabb {
            *a = r.aabb(*a);
        }
        *keep_parts = r.facets(keep_parts);
        *neighbour_parts = r.facets(neighbour_parts);
        if let Some(Ok(reg)) = picked_region {
            *reg = r.region(reg)?;
        }
        if let Some(Ok((loops, runs, _))) = contour_sel {
            *loops = loops.iter().map(|l| r.loop2(l)).collect::<Result<Vec<_>, _>>()?;
            for run in runs {
                for p in run {
                    *p = r.p2(*p);
                }
            }
        }
        if let Some(Ok(chains)) = drive_chains {
            for ch in chains {
                for p in ch {
                    *p = r.p3(*p);
                }
            }
        }
        if let Some(Ok(sb)) = slot_bound {
            *sb = sb.rotated_xy(o2, -self.yaw_rad)?;
        }
        if let Some(Ok(ps)) = prior
            && let Some(dex) = &ps.grid
        {
            let rot_dex = dex.rotated_z_conservative(o2, -self.yaw_rad)?;
            *ps = Arc::new(crate::rest::PriorState { grid: Some(rot_dex), digest: ps.digest });
        }
        for d in drills_ahead {
            for m in &mut d.moves {
                *m = r.mv_xy(m);
            }
        }
        if let Some(Ok(ctx)) = area {
            if let Some(s) = &mut ctx.stock {
                *s = r.region(s)?;
            }
            if let Some(b) = &mut ctx.blocked {
                *b = r.region(b)?;
            }
            for (_, reg) in &mut ctx.extra {
                *reg = r.region(reg)?;
            }
            if let Some((geom, _)) = &mut ctx.open {
                match geom {
                    OpenGeom::Chain(pts) => pts.iter_mut().for_each(|p| *p = r.p2(*p)),
                    OpenGeom::Chains(cs) => cs.iter_mut().for_each(|pts| pts.iter_mut().for_each(|p| *p = r.p2(*p))),
                    OpenGeom::Ring(reg) => *reg = r.region(reg)?,
                }
            }
            if let Some(l) = &mut ctx.lane {
                *l = r.region(l)?;
            }
        }
        rotate_params(&r, params);
        Ok(saved)
    }

    /// Map a generated location back into Setup `S`, including break-through notes.
    pub(super) fn exit_point(self, point: Point3) -> Point3 {
        Rot2::new(self.origin, self.yaw_rad).p3(point)
    }

    /// Map a generated `W_gen` region back into Setup `S` (`+yaw` about `origin`).
    pub(super) fn exit_region(self, reg: &Region) -> Result<Region, GenError> {
        Rot2::new(self.origin, self.yaw_rad).region(reg)
    }

    /// Map a generated `W_gen` toolpath back into Setup `S` (`+yaw` about `origin`).
    pub(super) fn exit_path(self, path: &Toolpath, chord_tol: f64) -> Toolpath {
        Rot2::new(self.origin, self.yaw_rad).toolpath(path, chord_tol)
    }

    /// Map a located refusal or ghost back into Setup `S` (`+yaw` about `origin`).
    pub(super) fn exit_err(self, e: GenError) -> GenError {
        let r = Rot2::new(self.origin, self.yaw_rad);
        e.map_location(|p| r.p3(p)).map_ghost(|t| r.toolpath(t, machining2d::ARC_FIT_TOL))
    }
}

fn rotate_params(r: &Rot2, params: &mut OpParams) {
    match params {
        OpParams::Circular { center, .. } => *center = r.p2(*center),
        OpParams::Slot { a, b, .. } | OpParams::TrochoidalSlot { a, b, .. } => {
            *a = r.p2(*a);
            *b = r.p2(*b);
        }
        OpParams::Drill { holes, .. } | OpParams::Bore { holes, .. } | OpParams::Thread { holes, .. } => {
            for h in holes {
                h.xy = r.p2(h.xy);
            }
        }
        _ => {}
    }
}

#[cfg(test)]
#[path = "wcs_yaw_tests.rs"]
mod tests;
