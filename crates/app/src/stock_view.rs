//! The document's stock as the eye sees it (M7.2 B3): the program
//! walked through the FIELD arm, for the sphere-traced viewport.
//! Provenance: the M7.1 field engine fed the same plan the column
//! verdict walks (`stock_sim::plan_program`); the subdivision is plain
//! midpoint refinement. Nothing from vendor binaries.
//!
//! ## The laws here
//!
//! - **A display field, not a verdict.** The deviation status line and
//!   the post gate still read the column engine (`simulate_document`)
//!   until M7.3 proves the two verdict-identical; this field exists so
//!   the viewport can draw the machined volume. Its cell is chosen for
//!   the frame's texel budget, never finer than the column's own.
//! - **The true stock, not a bottomless one.** The verdict field's
//!   block reaches ten spans down to mimic a semi-infinite column; the
//!   eye builds the stock entity's own mesh, so the picture shows the
//!   block the machine holds.
//! - **Refusals by name, and a target that may be absent.** A rotated
//!   stock, a multi-setup program (the flip transform is not the eye's
//!   to guess), a program that will not plan: `Err`, and the viewport
//!   keeps its sheet. A model the field cannot sign (an open shell)
//!   leaves `target` empty with its reason - pixels then paint class 3,
//!   unjudged, never clean.

use crate::geometry::{transform_point, IDENTITY_Q, Point3, Triangle};
use crate::model;
use std::sync::Arc;

use crate::stock_sim::preview::pack::MAX_TEXELS;
use crate::stock_sim::preview::{pack_field, pack_target, CutPatch, DistanceLattice, LatticeMeta};
use crate::stock_sim::{auto_cell, part_facets, plan_program, Backend, SimOp, Stock, FIELD_BAND};

/// The texels the stock's display cell is sized for by default: a
/// thirty-second of the dense pack budget. Finer cells cost the field
/// subtraction cubically (the flat tool's spanning flank sweeps the
/// whole stock height per move), and the first fixture run showed a
/// quarter of the budget taking minutes per program - the per-brick
/// cost law that would buy it back is M7.3's.
pub const DEFAULT_TEXELS: f64 = (MAX_TEXELS / 32) as f64;

/// A facet's longest edge after subdivision, in cells: well inside the
/// field builder's per-facet candidate limit with the band dilated in.
const EDGE_CELLS: f64 = 32.0;

/// Subdivision rounds are capped: 6 rounds is 4096 children a facet.
const MAX_ROUNDS: u32 = 6;

/// What the viewport traces: the stock after the program, and the
/// model it is judged against when the model can be signed.
#[derive(Debug)]
pub struct FieldView {
    pub stock: Stock,
    /// Shared, never re-cloned: the model does not move while a
    /// program plays.
    pub target: Option<Arc<Stock>>,
    /// Why `target` is empty, said so the status line can repeat it.
    pub target_note: Option<String>,
    pub cell: f64,
    /// The GPU lattice already packed off the UI thread (playback):
    /// `None` and the eye packs `stock` itself.
    pub packed: Option<Packed>,
}

/// A view's lattice, packed where the cutting ran: the session's
/// constant layers behind one `Arc` (an eye holding the same `base`
/// re-sends only `cut`), and this frame's cut layer - whole, or as a
/// patch over an earlier frame of the session (M13.1).
#[derive(Clone, Debug)]
pub struct Packed {
    pub base: Arc<PackedBase>,
    /// This frame's number in its session (ascending, from 1).
    pub seq: u64,
    pub cut: CutLayer,
}

/// A frame's cut layer over `base.meta`'s extent.
#[derive(Clone, Debug)]
pub enum CutLayer {
    /// The whole layer.
    Full(Arc<Vec<f32>>),
    /// Only what changed since frame `from`: valid on top of ANY frame
    /// `m` of the session with `from <= m < seq` (cuts only grow, and
    /// the patch covers every brick that moved from `from` to here).
    /// An eye holding anything else repacks the layer whole.
    Patch { from: u64, patch: Arc<CutPatch> },
}

impl Packed {
    /// Bytes this frame sends to the GPU as a cut-layer rewrite.
    #[must_use]
    pub fn cut_bytes(&self) -> usize {
        match &self.cut {
            CutLayer::Full(c) => c.len().saturating_mul(4),
            CutLayer::Patch { patch, .. } => patch.bytes(),
        }
    }

    /// True when the patch applies over a resident frame `held`.
    #[must_use]
    pub fn patches(&self, held: u64) -> bool {
        matches!(&self.cut, CutLayer::Patch { from, .. } if *from <= held && held < self.seq)
    }
}

/// What a playback session never re-packs: the stock's mesh layer
/// (parity-signed) and the model's distance lattice.
#[derive(Debug)]
pub struct PackedBase {
    pub meta: LatticeMeta,
    pub mesh: Vec<f32>,
    pub target: Option<DistanceLattice>,
}

impl PackedBase {
    /// Pack the session's constant layers; `None` when the stock is
    /// not a field or will not pack (the eye then packs, or marches
    /// on the CPU, as before).
    #[must_use]
    pub fn of(stock: &Stock, target: Option<&Stock>) -> Option<(Self, Vec<f32>)> {
        let mut lat = stock.field().and_then(pack_field)?;
        // #22: one eikonal sweep re-normalizes the band before the
        // render pass (display only; sign and zero set kept, and later
        // playback cut layers only lift cells over it). A refusal keeps
        // the packed lattice as it was - a picture, never a verdict.
        if let Err(e) = lat.renormalize() {
            tracing::warn!("stock view kept the un-normalized band: {e}");
        }
        let target = target.and_then(Stock::field).and_then(pack_target);
        Some((PackedBase { meta: lat.meta, mesh: lat.mesh, target }, lat.cut))
    }
}

/// Walk the document's program through the field arm, at the default
/// display budget.
pub fn field_view(world: &flecs_ecs::prelude::World) -> Result<FieldView, String> {
    field_view_with(world, DEFAULT_TEXELS)
}

/// The same walk with the stock's display cell sized for `texels`
/// cells of volume (a coarser picture is cheaper to cut, never less
/// honest: classes still come from the same gate).
pub fn field_view_with(world: &flecs_ecs::prelude::World, texels: f64) -> Result<FieldView, String> {
    let (mut stock, planned, cell) = display_parts(world, texels)?;
    let pose_tol = crate::stock_sim::rotary_pose_tol(cell);
    for p in &planned {
        let op = SimOp { moves: &p.moves, radius: p.radius, ball: p.ball, cone: p.cone, corner: p.corner };
        // A rotary op turns the part: its moves are chorded about the
        // fitted axis (M13.1, `stock_sim::rotary`), never cut as the
        // un-chorded posed chords they are written as.
        // A turning op's moves revolve about its fitted spindle (#19,
        // `stock_sim::lathe`).
        let cut = match (&p.rotary, &p.lathe) {
            (_, Some(l)) => stock.simulate_lathe_profile(&p.moves, &l.profile, &l.axis),
            (Some(ax), None) => crate::stock_sim::simulate_rotary_slice(&mut stock, &op, &p.moves, 0, ax, pose_tol).map(|_| ()),
            (None, None) => stock.simulate_display(std::slice::from_ref(&op)),
        };
        cut.map_err(|e| format!("the field view could not cut the program: {e}"))?;
    }
    let (target, target_note) = display_target(world, cell);
    Ok(FieldView { stock, target: target.map(Arc::new), target_note, cell, packed: None })
}

/// The uncut display stock, the plan it will be cut by, and its cell:
/// the walk `field_view_with` and the playback (`stock_play`) share.
pub(crate) fn display_parts(
    world: &flecs_ecs::prelude::World,
    texels: f64,
) -> Result<(Stock, Vec<crate::stock_sim::Planned>, f64), String> {
    let (facets, planned, cell) = display_inputs(world, texels)?;
    Ok((build_stock(&facets, cell)?, planned, cell))
}

/// `display_parts`' world half: the stock's world facets, the plan
/// and the cell - plain data a worker can build the field from.
pub(crate) fn display_inputs(
    world: &flecs_ecs::prelude::World,
    texels: f64,
) -> Result<(Vec<Triangle>, Vec<crate::stock_sim::Planned>, f64), String> {
    if !(texels.is_finite() && texels >= 1.0) {
        return Err(format!("the field view needs a positive texel budget, not {texels}"));
    }
    let planned = plan_program(world)?;
    let first = planned.first().and_then(|p| p.setup.clone());
    let eff = crate::setup::effective(world, first.as_deref());
    if planned.iter().any(|p| crate::setup::effective(world, p.setup.as_deref()) != eff) {
        return Err("the field view draws one setup's stock; a multi-setup program keeps the sheet (the flip transform is M7.3's)".into());
    }
    let stock_facets = stock_facets(world, first.as_deref())?;
    let (lo, hi) = bounds(&stock_facets).ok_or("the stock meshed empty - nothing to draw")?;
    let min_r = planned.iter().map(|p| p.radius).fold(f64::INFINITY, f64::min);
    let (sx, sy, sz) = (hi.x - lo.x, hi.y - lo.y, hi.z - lo.z);
    let budget = ((sx * sy * sz).max(0.0) / texels).cbrt();
    let cell = auto_cell(if min_r.is_finite() { min_r } else { 1.0 }, sx, sy).max(budget);
    if !(cell.is_finite() && cell > 0.0) {
        return Err(format!("the stock's extent gives no sane display cell ({cell})"));
    }
    Ok((stock_facets, planned, cell))
}

/// `display_parts`' build half: the uncut display field (no world).
pub(crate) fn build_stock(stock_facets: &[Triangle], cell: f64) -> Result<Stock, String> {
    let backend = Backend::Field { band: FIELD_BAND };
    Stock::from_facets(backend, &subdivide(stock_facets, cell), cell)
        .map_err(|e| format!("the field view could not build the stock: {e}"))
}

/// The model as a field at `cell`, or why it cannot be one.
pub(crate) fn display_target(world: &flecs_ecs::prelude::World, cell: f64) -> (Option<Stock>, Option<String>) {
    match part_facets(world) {
        Ok(part) => build_target(&part, cell),
        Err(why) => (None, Some(why)),
    }
}

/// `display_target`'s build half, from the model's world facets.
pub(crate) fn build_target(part: &[Triangle], cell: f64) -> (Option<Stock>, Option<String>) {
    if part.is_empty() {
        return (None, Some("no model to judge against".to_string()));
    }
    match Stock::from_facets(Backend::Field { band: FIELD_BAND }, &subdivide(part, cell), cell) {
        Ok(t) => (Some(t), None),
        Err(e) => (None, Some(format!("the model cannot be signed as a field: {e}"))),
    }
}

/// The setup's stock entity as world triangles (the mesh the viewport
/// already draws), refusing a rotation the axis-aligned field
/// lattice would silently ignore.
fn stock_facets(world: &flecs_ecs::prelude::World, setup: Option<&str>) -> Result<Vec<Triangle>, String> {
    let name = crate::setup::stock_entity_name(world, setup);
    let e = model::find_by_name(world, &name)
        .ok_or_else(|| format!("no stock '{name}' - the eye draws material, not air"))?;
    let loc = model::transform_of(world, &e).unwrap_or_default();
    if loc.rot != IDENTITY_Q {
        return Err("the field view models an axis-aligned stock; this stock carries a rotation".into());
    }
    let tris = model::facets_of(world, &e).ok_or("the stock entity carries no mesh")?;
    Ok(tris
        .iter()
        .map(|t| {
            Triangle::new(
                transform_point(t.a, loc.pos, loc.rot),
                transform_point(t.b, loc.pos, loc.rot),
                transform_point(t.c, loc.pos, loc.rot),
            )
        })
        .collect())
}

fn bounds(f: &[Triangle]) -> Option<(Point3, Point3)> {
    let mut lo = Point3::new(f64::INFINITY, f64::INFINITY, f64::INFINITY);
    let mut hi = Point3::new(f64::NEG_INFINITY, f64::NEG_INFINITY, f64::NEG_INFINITY);
    for t in f {
        for v in [t.a, t.b, t.c] {
            lo = Point3::new(lo.x.min(v.x), lo.y.min(v.y), lo.z.min(v.z));
            hi = Point3::new(hi.x.max(v.x), hi.y.max(v.y), hi.z.max(v.z));
        }
    }
    (lo.is_finite() && hi.is_finite()).then_some((lo, hi))
}

/// Uniform 4-way midpoint subdivision until the longest edge spans
/// at most `EDGE_CELLS` cells. Every shared edge splits at the same
/// midpoint on both sides, so a watertight shell stays watertight -
/// the field's parity law needs exactly that.
pub fn subdivide(facets: &[Triangle], cell: f64) -> Vec<Triangle> {
    let longest = facets
        .iter()
        .flat_map(|t| [(t.a, t.b), (t.b, t.c), (t.c, t.a)])
        .map(|(p, q)| dist(p, q))
        .fold(0.0f64, f64::max);
    let ratio = longest / (EDGE_CELLS * cell);
    let rounds = if ratio.is_finite() && ratio > 1.0 { ratio.log2().ceil() as u32 } else { 0 };
    let mut out = facets.to_vec();
    for _ in 0..rounds.min(MAX_ROUNDS) {
        let mut next = Vec::with_capacity(out.len() * 4);
        for t in &out {
            let (ab, bc, ca) = (mid(t.a, t.b), mid(t.b, t.c), mid(t.c, t.a));
            next.push(Triangle::new(t.a, ab, ca));
            next.push(Triangle::new(ab, t.b, bc));
            next.push(Triangle::new(ca, bc, t.c));
            next.push(Triangle::new(ab, bc, ca));
        }
        out = next;
    }
    out
}

fn mid(p: Point3, q: Point3) -> Point3 {
    Point3::new((p.x + q.x) * 0.5, (p.y + q.y) * 0.5, (p.z + q.z) * 0.5)
}

fn dist(p: Point3, q: Point3) -> f64 {
    ((p.x - q.x).powi(2) + (p.y - q.y).powi(2) + (p.z - q.z).powi(2)).sqrt()
}
