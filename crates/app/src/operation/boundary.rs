//! The area-controls law (M11.5), pure: no world, no entity. The
//! world reads that feed it (stock outline, part material, picked
//! boundaries) are resolved into an [`AreaCtx`] by `boundary_ctx` on the
//! caller's thread; `area::apply_area_controls` is the one seam that
//! runs this law, last, on every region an operation cuts.
//!
//! Order of the law, each step named in the empty-area refusal:
//! 1. open boundary - the picked chain (extended straight past its ends)
//!    or ring splits the stock outline; the chosen side replaces the
//!    selection's area,
//! 2. offset - grow / shrink, round or sharp corners,
//! 3. extension - past the exterior edges (no part material beyond):
//!    Face runs out to the stock outline and on by the distance, the
//!    other kinds by the distance,
//! 4. include boundaries (union), then the part's own material at the
//!    cut planes is taken out again (the pocket island law),
//! 5. tool containment - inside / on / outside, as an offset relative to
//!    what the kind's generator does natively with its region,
//! 6. exclude boundaries - after containment for the clearing kinds (the
//!    tool body stays out), before it for the contour (they shape the
//!    ring it rides).
//!
//! Provenance: our own design notes; the region booleans and offsets are
//! the clipper2 bridge (`geo2d`), the side test is the plane-sweep
//! textbook construction of closing an open polyline against a far
//! circle. No vendor material consulted.

use super::*;
use super::boundary_turn::{grow_apart, sides_of_chain, snap_turn};

/// Below this (mm^2) an area is nothing - the island law's noise floor.
const AREA_EPS: f64 = 1e-4;

/// What a kind's generator does natively with its region: where the
/// TOOL stands - the shared prefix is the law's own word, and `ToolOn`
/// is the name the ROADMAP ledger gave the ride's native (M13.10).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[allow(clippy::enum_variant_names)]
pub(super) enum Native {
    /// The tool stays inside the region (pocket family, Face rows,
    /// a contour on its Inside side).
    ToolInside,
    /// The tool rides outside the region (a contour on its Outside side).
    ToolOutside,
    /// The tool CENTRE rides the region's outline (Trace / Engrave,
    /// M13.10): containment moves the ride by the whole wanted side.
    ToolOn,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Family {
    Face,
    Clearing,
    Contour,
    /// The 3D finishing family (M11.5 owed): the path is generated on
    /// the whole model and CLIPPED to the area (`finish_clip`), the
    /// tool natively reaching a radius past the silhouette - so the
    /// zones it must keep its whole body out of ride `forbidden`, like
    /// Face's.
    Finish,
    /// The outline rides, Trace and Engrave (M13.10): the region's
    /// loops ARE the path, tool centre on them. Offset, containment and
    /// the include / exclude boundaries shape the ring ridden (excludes
    /// before containment, as the contour's); an open boundary or an
    /// extension bounds no ride and refuses by name.
    Ride,
}

/// A picked boundary for step 1, resolved to XY.
#[derive(Clone, Debug)]
pub(super) enum OpenGeom {
    Chain(Vec<Point2>),
    /// Separate chains that stay apart; the area lies between them (#1727).
    Chains(Vec<Vec<Point2>>),
    Ring(Region),
}

/// Everything the law reads, owned and world-free (rides the `GenJob`).
#[derive(Clone, Debug)]
pub struct AreaCtx {
    pub(super) controls: AreaControls,
    pub(super) family: Family,
    pub(super) native: Native,
    pub(super) tool_r: f64,
    pub(super) stock: Option<Region>,
    /// Part material standing above the cut plane(s), when the op cuts a
    /// part: extension never floods it, the clearing island law takes it
    /// out again, and Face never sweeps across it.
    pub(super) blocked: Option<Region>,
    /// Re-subtract `blocked` from the final area (pocket family on a part).
    pub(super) islands: bool,
    pub(super) extra: Vec<(BoundaryMode, Region)>,
    pub(super) open: Option<(OpenGeom, ChainSide)>,
    /// #1735: the open-edge band of the selection, grown by the
    /// extension along its walls (`open_band`); the area never leaves it.
    pub(super) lane: Option<Region>,
    /// #740: the document's display unit, so the area law's refusals (on
    /// worker threads, world-free) spell their depths through this lens.
    pub(super) unit: crate::units::Unit,
}

impl AreaCtx {
    /// The contour's native side, once its Side (or Auto's verdict) is
    /// known: on the Inside the ring is ridden from inside.
    pub(super) fn contour_side(&mut self, inside: bool) {
        if self.family == Family::Contour {
            self.native = if inside { Native::ToolInside } else { Native::ToolOutside };
        }
    }

    /// Whether this runs `Containment::Outside` clearing (no open boundary), and whether stock exists.
    pub(crate) fn outside_clearing_stock(&self) -> Option<bool> {
        let out = self.family == Family::Clearing && self.controls.containment == Some(Containment::Outside);
        (out && self.open.is_none()).then_some(self.stock.is_some())
    }
}

/// The law's answer: the area, plus what Face needs to stay honest - the
/// zones its bounding band must not cross (named) and the turn that lays
/// its rows parallel to an open chain.
#[derive(Clone, Debug)]
pub(super) struct AreaApplied {
    pub(super) region: Region,
    pub(super) forbidden: Vec<(&'static str, Region)>,
    pub(super) turn: f64,
}

fn empty_after(step: &str) -> GenError {
    GenError::Empty(format!(
        "the Boundary settings leave no area to machine after the {step} - relax them"
    ))
}

fn check(r: Region, step: &str) -> Result<Region, GenError> {
    if r.area().abs() < AREA_EPS {
        return Err(empty_after(step));
    }
    Ok(r)
}

pub(super) fn grow(r: &Region, d: f64, corner: CornerStyle) -> Result<Region, GenError> {
    if d == 0.0 {
        return Ok(r.clone());
    }
    Ok(match corner {
        CornerStyle::Round => r.offset(d)?,
        CornerStyle::Sharp => r.offset_sharp(d)?,
    })
}

/// The region offset that puts the tool where `want` says, given what the
/// generator does natively (the tool-edge / tool-centre geometry).
pub(super) fn containment_delta(native: Native, want: Containment, r: f64) -> f64 {
    let t = match want {
        Containment::Inside => -r,
        Containment::On => 0.0,
        Containment::Outside => r,
    };
    let n = match native {
        Native::ToolInside => -r,
        Native::ToolOutside => r,
        Native::ToolOn => 0.0,
    };
    t - n
}

/// Containment past the edge (user hand test 2026-11-06; #2232): let a
/// clearing tool pass the outer edge only (keeping selection holes out
/// and dropping unreached voids). With setup stock (`stock_reach`),
/// `Outside` clears out to `stock + 2r` unless walls enclose `pre`.
fn reach_past_edge(
    pre: &Region,
    d: f64,
    stock_reach: Option<&Region>,
    islands: &dyn Fn(Region) -> Result<Region, GenError>,
) -> Result<Region, GenError> {
    let shells: Vec<Loop> = pre.loops().iter().filter(|l| l.is_ccw()).cloned().collect();
    let filled = Region::union_all(shells)?;
    let holes = filled.difference(pre)?;
    let local_grown = grow(&filled, d, CornerStyle::Round)?.difference(&holes)?;
    let band = local_grown.difference(pre)?;
    let pick = |cand: Region| -> Result<(Region, Region), GenError> {
        let area = islands(cand)?;
        Ok((connected_to(&area, &band)?, area))
    };
    if let Some(sr) = stock_reach {
        let pre_free = islands(pre.clone())?;
        let rim = islands(grow(&filled, 1e-2, CornerStyle::Round)?.difference(&filled)?)?;
        if pre_free.area().abs() > AREA_EPS && rim.area().abs() <= AREA_EPS {
            return Ok(pre_free);
        }
        let (reached, _) = pick(local_grown.union(sr)?.difference(&holes)?)?;
        if reached.area().abs() > AREA_EPS { return Ok(reached); }
    }
    let (reached, area) = pick(local_grown)?;
    Ok(if reached.area().abs() > AREA_EPS { reached } else { area })
}

/// The families whose exclude boundaries shape the ring they ride,
/// before containment: the contour and the outline rides.
fn rides(f: Family) -> bool {
    matches!(f, Family::Contour | Family::Ride)
}

/// Run the law on `base` (the kind's own region).
pub(super) fn resolve(base: Region, ctx: &AreaCtx) -> Result<AreaApplied, GenError> {
    let c = &ctx.controls;
    let r = ctx.tool_r;
    let mut forbidden: Vec<(&'static str, Region)> = Vec::new();
    let mut turn = 0.0;
    // 1. Open (or closed) user boundary.
    let mut far_side: Option<Region> = None;
    let mut outer: Option<Region> = None;
    let mut area = match &ctx.open {
        None => base,
        Some((geom, side)) => {
            if ctx.family == Family::Contour {
                // The generator's Contour arm rides an open boundary
                // itself (`contour_open`); it never reaches the area law.
                return Err(GenError::Invalid(
                    "a Contour rides its open boundary as one open pass - the boundary bounds no area for it".into(),
                ));
            }
            if ctx.family == Family::Ride {
                return Err(GenError::Invalid(
                    "an outline ride follows the picked outline itself - an open boundary bounds no ride for it; pick the chain as the outline instead".into(),
                ));
            }
            let Some(stock) = &ctx.stock else {
                return Err(GenError::Invalid(
                    "an open boundary is closed off by the stock outline - this setup has no stock".into(),
                ));
            };
            // Face's own overrun is its air start; the clearing kinds
            // reach a tool diameter past the stock so their outer
            // passes run in air.
            let reach = match ctx.family {
                Family::Face => stock.clone(),
                _ => stock.offset_sharp(2.0 * r)?,
            };
            let (left, right, dir, laid) = match geom {
                OpenGeom::Chain(pts) => {
                    let (l, rt, d) = sides_of_chain(pts, &reach)?;
                    (l, rt, Some(d), None)
                }
                OpenGeom::Chains(cs) => {
                    let (between, beyond, t) = super::boundary_chains::sides_of_chains(cs, &reach, &base).map_err(super::boundary_chain_error)?;
                    (between, beyond, None, Some(t))
                }
                OpenGeom::Ring(ring) => (reach.intersect(ring)?, reach.difference(ring)?, None, None),
            };
            outer = Some(reach);
            // Separate chains name their Inside themselves (between them).
            let left_inside = laid.is_some() || pick_side_left(&base, &left, &right)?;
            let keep_left = (*side == ChainSide::Inside) == left_inside;
            let (keep, drop) = if keep_left { (left, right) } else { (right, left) };
            if let Some(theta) = dir {
                // Rows parallel to the chain, the machined side below it:
                // the band starts at the far edge and works in.
                turn = snap_turn(if keep_left { std::f64::consts::PI - theta } else { -theta });
            }
            if let Some(t) = laid {
                turn = t;
            }
            far_side = Some(drop);
            check(keep, "open boundary (the chosen side holds no stock)")?
        }
    };
    // 2. Offset: the picked line by `offset`, an open boundary's outer
    //    line by `outer_offset` (#3338); one value is the old law.
    area = match &outer {
        Some(reach) if c.outer_offset != c.offset => check(grow_apart(&area, reach, c.offset, c.outer_offset, c.corner)?, "outer boundary offset")?,
        _ => check(grow(&area, c.offset, c.corner)?, "boundary offset")?,
    };
    if let Some(f) = &mut far_side {
        *f = grow(f, -c.offset, c.corner)?;
    }
    // 3. Extension past the exterior edges.
    if c.extension > 0.0 {
        if ctx.family == Family::Ride {
            return Err(GenError::Invalid(
                "an outline ride has no exterior edge to extend past - use the offset to move the ride instead".into(),
            ));
        }
        let reach = match (&ctx.family, &ctx.stock) {
            (Family::Face, Some(s)) => s.union(&area)?.offset_sharp(c.extension)?,
            _ => area.offset_sharp(c.extension)?,
        };
        let reach = match &ctx.blocked {
            Some(b) => reach.difference(b)?,
            None => reach,
        };
        area = connected_to(&area.union(&reach)?, &area)?;
    }
    if let Some(f) = &far_side {
        area = check(area.difference(f)?, "open boundary")?;
    }
    if let Some(l) = &ctx.lane {
        area = check(area.intersect(l)?, "open-edge band")?;
    }
    // 4. Includes, then the part's own material back out.
    for (mode, b) in &ctx.extra {
        if *mode == BoundaryMode::Include {
            area = area.union(b)?;
        }
    }
    let islands = |a: Region| -> Result<Region, GenError> {
        match (&ctx.blocked, ctx.islands) {
            (Some(b), true) => Ok(a.difference(b)?),
            _ => Ok(a),
        }
    };
    // Containment that lets a clearing tool past the outer edge grows
    // the selection's OWN outline (before the island law), so a part
    // standing on the picked face leaves the band around it to clear.
    let grow_d = c
        .containment
        .map(|want| containment_delta(ctx.native, want, r))
        .filter(|d| *d > 0.0 && !rides(ctx.family));
    let pre_islands = area.clone();
    area = islands(area)?;
    if grow_d.is_none() {
        area = check(area, "include boundaries (the part fills them)")?;
    }
    let excludes = |mut a: Region| -> Result<Region, GenError> {
        for (mode, b) in &ctx.extra {
            if *mode == BoundaryMode::Exclude {
                a = a.difference(b)?;
            }
        }
        Ok(a)
    };
    if rides(ctx.family) {
        area = check(excludes(area)?, "exclude boundaries")?;
    }
    // 5. Containment.
    if let Some(want) = c.containment {
        let d = containment_delta(ctx.native, want, r);
        // The rides shape the ring they ride, holes included; the
        // clearing kinds keep a hole's protection (`contain`).
        if grow_d.is_some() {
            let sr = (ctx.outside_clearing_stock() == Some(true))
                .then(|| ctx.stock.as_ref().map(|s| s.offset(2.0 * r)))
                .flatten()
                .transpose()?;
            area = check(reach_past_edge(&pre_islands, d, sr.as_ref(), &islands)?, "tool containment (the part fills the area)")?;
        } else {
            area = check(grow(&area, d, CornerStyle::Round)?, "tool containment")?;
            if !rides(ctx.family) {
                area = check(islands(area)?, "tool containment (the part fills the area)")?;
            }
        }
        if let Some(f) = &mut far_side {
            *f = grow(f, -d, CornerStyle::Round)?;
        }
    }
    // 6. Excludes for the clearing kinds: the tool body stays out.
    if !rides(ctx.family) {
        area = check(excludes(area)?, "exclude boundaries")?;
    }
    if matches!(ctx.family, Family::Face | Family::Finish | Family::Clearing) {
        if let Some(f) = far_side {
            forbidden.push(("the far side of the open boundary", f));
        }
        for (mode, b) in &ctx.extra {
            if *mode == BoundaryMode::Exclude {
                forbidden.push(("an exclude boundary", b.clone()));
            }
        }
        if matches!(ctx.family, Family::Face | Family::Finish)
            && let Some(b) = &ctx.blocked
        {
            forbidden.push(("part material standing above Cut Z", b.clone()));
        }
    }
    Ok(AreaApplied { region: area, forbidden, turn })
}

/// Whether the left side of an open boundary is its Inside: the side
/// holding more of the operation's own area `base` (a near tie, within
/// 1 %, reads as left).
pub(super) fn pick_side_left(base: &Region, left: &Region, right: &Region) -> Result<bool, GenError> {
    let in_l = left.intersect(base)?.area().abs();
    let in_r = right.intersect(base)?.area().abs();
    Ok((in_l - in_r).abs() <= 0.01 * in_l.max(in_r) || in_l > in_r)
}

/// The pieces of `all` that overlap `seed` - extension never jumps to a
/// pocket of reach the original area does not touch.
fn connected_to(all: &Region, seed: &Region) -> Result<Region, GenError> {
    let outers: Vec<&Loop> = all.loops().iter().filter(|l| l.is_ccw()).collect();
    let mut kept: Vec<Loop> = Vec::new();
    for o in outers {
        let one = Region::new(vec![o.clone()])?;
        if one.intersect(seed)?.area().abs() > AREA_EPS {
            kept.push(o.clone());
        }
    }
    let shells = Region::new(kept.clone())?;
    for h in all.loops().iter().filter(|l| !l.is_ccw()) {
        if h.points().first().is_some_and(|p| shells.contains(*p)) {
            kept.push(h.clone());
        }
    }
    Ok(Region::new(kept)?)
}
