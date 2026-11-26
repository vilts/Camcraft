//! Selected DXF chain on box stock for a vertical wire cut. Provenance: CamCraft's
//! stock-face placement, analytic DXF primitives and wire verification contracts (#3180).

use std::collections::HashSet;
use std::sync::Arc;

use camcraft_geometry::dxf::Primitive;
use flecs_ecs::prelude::World;

use camcraft_geometry::wire_round_profile::WireProfile;

use super::{FeatureSel, OpParams, ResolvedHeights, SrcRef, WireContourOp};
use crate::cl::Toolpath;
use crate::geo2d::{Loop, Point2, Region};
use crate::geometry::sketch_chains::{ClosedChain, SketchChainRef};
use crate::geometry::{Aabb, IDENTITY_Q, Point3, transform_point};
use crate::schema::{Location, Operation};
use crate::sketch_data::{SketchData, bounds};
use crate::{model, setup, sketch, stock::StockShape};

/// Match the stored feature kind even if its profile has since disappeared.
pub(crate) fn selected(src: &SrcRef, params: &OpParams) -> bool {
    matches!(params, OpParams::WireContour(_))
        && matches!(src, SrcRef::Selection(items)
        if items.iter().any(|g| matches!(g.feature, FeatureSel::SketchChain(_) | FeatureSel::SketchLayer(_))))
}

/// Check the pick at the command door. A curved chain can be selected and retained.
pub(crate) fn pick(world: &World, src: &SrcRef) -> Result<(String, ClosedChain), String> {
    let SrcRef::Selection(items) = src else {
        return Err("Wire Contour needs a selected DXF chain".into());
    };
    let [g] = items.as_slice() else {
        return Err("Wire Contour cuts one selected DXF chain per operation".into());
    };
    let FeatureSel::SketchChain(id) = g.feature else {
        return Err("select one closed DXF chain for Wire Contour".into());
    };
    let (_, sketch) = sketch::find(world, &g.entity)
        .ok_or_else(|| format!("DXF profile '{}' is missing", g.entity))?;
    match sketch.data.chains().find(id) {
        Some(SketchChainRef::Closed(c)) => Ok((g.entity.clone(), c.clone())),
        Some(SketchChainRef::Open(_)) => Err(format!(
            "DXF chain {id} is open; select a closed chain for Wire Contour"
        )),
        None => Err(format!(
            "DXF chain {id} is missing from '{}'; select it again",
            g.entity
        )),
    }
}

/// The chain is in drawing coordinates. Its segments retain source indices and oriented
/// analytic curves. The pose maps drawing XY to world XY on the top face of the stock.
pub(crate) struct SelectedWireChain {
    pub(crate) chain: ClosedChain,
    pub(crate) pose: Location,
    pub(crate) data: Arc<SketchData>,
    pub(crate) stock: Aabb,
    /// TEXT, MTEXT and DIMENSION entities the import skipped (#3937): not
    /// cut, and not geometry the wire can hit; said on the path.
    pub(crate) ignored: usize,
}

fn axis_aligned(rot: [f64; 4]) -> bool {
    rot == IDENTITY_Q || rot == [0.0, 0.0, 0.0, -1.0]
}

/// Collect only the conditions under which the selected chain is a through profile.
pub(crate) fn resolve(
    world: &World,
    op: &Operation,
    name: &str,
    hs: &ResolvedHeights,
) -> Result<SelectedWireChain, String> {
    resolve_geometry(world, op, hs, Some(name))
}

/// Resolve nominal geometry. A machining operation also requires fresh stock at its place in the sequence.
pub(super) fn resolve_geometry(
    world: &World,
    op: &Operation,
    hs: &ResolvedHeights,
    machining_operation: Option<&str>,
) -> Result<SelectedWireChain, String> {
    let (profile, chain) = pick(world, &op.src)?;
    let (e, sk) = sketch::find(world, &profile).ok_or("the selected DXF profile is missing")?;
    let declared = op.setup.as_deref();
    if setup::effective(world, declared) != setup::effective(world, sk.setup.as_deref()) {
        return Err(format!(
            "DXF profile '{profile}' is on another setup's stock"
        ));
    }
    if let Some(why) = crate::setup_align::generation_problem(world, op) {
        return Err(why);
    }
    if !op.area.is_default() {
        return Err("selected DXF wire cuts require the default boundary settings".into());
    }
    if let Some(wcs) =
        setup::wcs_entity(world, declared).and_then(|e| model::transform_of(world, &e))
        && !axis_aligned(wcs.rot)
    {
        return Err(
            "selected DXF wire cuts require a work offset aligned with the machine axes".into(),
        );
    }
    if !model::model_entities(world).is_empty()
        || !crate::fixture::boxes_for(world, declared).is_empty()
    {
        return Err("selected DXF wire cuts cannot verify a separate part, fixture or machine table in the wire span".into());
    }
    if sketch::placed_in(world, declared).len() != 1 {
        return Err("selected DXF wire cuts can check only one placed drawing in the setup".into());
    }
    if let Some(name) = machining_operation
        && (crate::rest::carried_stock_block(world, name, declared)?.is_some()
            || has_predecessor(world, name, declared))
    {
        return Err("selected DXF wire cuts require fresh stock; an earlier operation or setup changes its material".into());
    }
    let stock_e =
        setup::stock_entity(world, declared).ok_or("the selected DXF profile needs box stock")?;
    let Some(StockShape::Box { .. }) = model::stock_def_of(world, &stock_e) else {
        return Err(
            "the selected DXF profile needs box stock for through-height verification".into(),
        );
    };
    let at = model::transform_of(world, &stock_e).unwrap_or_default();
    let local = model::transform_of(world, &e).unwrap_or_default();
    if !axis_aligned(at.rot)
        || sketch::face_placement(local).is_none()
        || local.pos.z != 0.0
        || local.rot[0] != 0.0
        || local.rot[1] != 0.0
    {
        return Err("the DXF chain must lie on the top face of axis-aligned box stock".into());
    }
    let stock =
        model::stock_world_aabb_for(world, declared).ok_or("the stock has no finite extent")?;
    let pose = sketch::world_pose(world, &e)
        .ok_or("the selected DXF profile has no stock-face placement")?;
    if !stock.min.is_finite()
        || !stock.max.is_finite()
        || stock.max.z <= stock.min.z
        || pose.pos.z != stock.max.z
        || hs.top != Some(stock.max.z)
        || hs.bottom != Some(stock.min.z)
    {
        return Err("set the wire top and bottom to the top and bottom of the stock for a selected DXF through cut".into());
    }
    let ignored = super::wire_sketch_arcs::annotations(&sk.data)?;
    Ok(SelectedWireChain {
        chain,
        pose,
        data: sk.data,
        stock,
        ignored,
    })
}

pub(super) fn single_pass(w: &WireContourOp) -> Result<(), String> {
    if !w.capture
        || w.tabs.is_some()
        || w.burnout.is_some()
        || w.taper_deg != 0.0
        || w.four_axis.is_some()
        || !w.skims.is_empty()
    {
        return Err("selected DXF wire cuts require a single vertical pass with slug capture and no tabs".into());
    }
    Ok(())
}

fn has_predecessor(world: &World, name: &str, declared: Option<&str>) -> bool {
    setup::operation_order(world)
        .into_iter()
        .take_while(|(n, _)| n != name)
        .any(|(_, entity)| {
            model::operation_of(world, &entity).is_some_and(|other| {
                !other.suppressed
                    && setup::effective(world, other.setup.as_deref())
                        == setup::effective(world, declared)
            })
        })
}

/// Stable chain IDs quantize coordinates. A sub-quantum edit still changes the
/// nominal or protected geometry the kerf verdict reads.
pub(super) fn stamp(
    world: &World,
    setup: Option<&str>,
    name: &str,
    sketch: &crate::schema::Sketch,
    h: &mut impl std::hash::Hasher,
) {
    use std::hash::Hash;
    h.write(b"selected-wire-chain");
    sketch::placed_in(world, setup).len().hash(h);
    has_predecessor(world, name, setup).hash(h);
    let wcs = setup::wcs_entity(world, setup)
        .and_then(|e| model::transform_of(world, &e))
        .unwrap_or_default();
    wcs.rot.map(f64::to_bits).hash(h);
    sketch.setup.hash(h);
    let data = &sketch.data;
    data.skipped_total().hash(h);
    data.join_tolerance().to_bits().hash(h);
    h.write_u64(data.primitive_signature());
}

impl SelectedWireChain {
    pub(super) fn world_xy(&self, p: Point2) -> Point2 {
        let q = transform_point(Point3::new(p.x, p.y, 0.0), self.pose.pos, self.pose.rot);
        Point2::new(q.x, q.y)
    }

    /// Every span a line, joined exactly: the polygon path of #3180, kept
    /// byte for byte. Any other chain is cut on its own lines and arcs
    /// (#3937, `wire_sketch_arcs`).
    pub(super) fn exact_polygon(&self) -> bool {
        self.chain.max_gap == 0.0
            && self
                .chain
                .segments
                .iter()
                .all(|s| matches!(s.curve, Primitive::Line { .. }))
    }

    /// The nominal ring of the polygon path: the exact straight chain.
    pub(super) fn nominal_ring(&self, w: &WireContourOp) -> Result<Loop, String> {
        if !self.exact_polygon() {
            return Err("the selected DXF chain holds an arc or a joint gap; it is cut on its own lines and arcs".into());
        }
        single_pass(w)?;
        Loop::new(
            self.chain
                .segments
                .iter()
                .map(|s| self.world_xy(s.start))
                .collect(),
        )
        .map_err(|e| e.to_string())
    }

    /// The selected circle, racetrack or convex tangent loop, in world XY
    /// (the native Agie punch, #2022 / #2983).
    pub(crate) fn wire_profile(&self) -> Result<WireProfile, String> {
        let mut shape = WireProfile::from_chain(&self.chain)?;
        shape.place(|p| self.world_xy(p));
        Ok(shape)
    }

    /// Prove the entire swept kerf and the freed piece fit inside the raw stock
    /// and clear every unselected primitive, including rejected chain fragments.
    /// The caller supplies the full world-XY envelope including leads and kerf.
    pub(crate) fn check_envelope(&self, lo: Point2, hi: Point2) -> Result<(), String> {
        if !(lo.x > self.stock.min.x
            && hi.x < self.stock.max.x
            && lo.y > self.stock.min.y
            && hi.y < self.stock.max.y
            && lo.x <= hi.x
            && lo.y <= hi.y)
        {
            return Err(
                "the threading point, cut and full kerf must fit inside the box stock".into(),
            );
        }
        let used: HashSet<_> = self.chain.segments.iter().map(|s| s.primitive).collect();
        for (index, item) in self
            .data
            .primitives()
            .iter()
            .enumerate()
            .filter(|(i, _)| !used.contains(i))
        {
            let (a, b) = bounds(std::slice::from_ref(item))
                .ok_or("an unselected DXF primitive has no bounds")?;
            let corners =
                [a, b, Point2::new(a.x, b.y), Point2::new(b.x, a.y)].map(|p| self.world_xy(p));
            if !(corners.iter().all(|p| p.x < lo.x)
                || corners.iter().all(|p| p.x > hi.x)
                || corners.iter().all(|p| p.y < lo.y)
                || corners.iter().all(|p| p.y > hi.y))
            {
                return Err(format!(
                    "the wire cut or freed piece cannot clear unselected DXF primitive {} on layer '{}'",
                    index + 1,
                    item.layer
                ));
            }
        }
        Ok(())
    }

    pub(super) fn check_path(&self, path: &Toolpath, kerf_radius: f64) -> Result<(), String> {
        if !(kerf_radius.is_finite() && kerf_radius > 0.0) || path.moves.is_empty() {
            return Err("the selected DXF wire cut has no verifiable kerf".into());
        }
        let mut lo = Point2::new(f64::INFINITY, f64::INFINITY);
        let mut hi = Point2::new(f64::NEG_INFINITY, f64::NEG_INFINITY);
        for m in &path.moves {
            if m.arc.is_some_and(|a| !a.plane.is_xy())
                || m.rotary.is_some()
                || m.axis.is_some()
                || m.kind != crate::cl::MoveKind::Feed
            {
                return Err(
                    "the selected DXF wire cut requires vertical-wire feed moves (lines and XY arcs)".into(),
                );
            }
            for p in [m.from, m.to] {
                if !p.is_finite() || p.z != self.stock.min.z {
                    return Err("the wire path leaves the programmed stock bottom plane".into());
                }
            }
            for p in super::wire_sketch_arcs::extremes(m) {
                lo.x = lo.x.min(p.x - kerf_radius);
                lo.y = lo.y.min(p.y - kerf_radius);
                hi.x = hi.x.max(p.x + kerf_radius);
                hi.y = hi.y.max(p.y + kerf_radius);
            }
        }
        self.check_envelope(lo, hi)
    }

    pub(super) fn stock_region(&self) -> Result<Region, String> {
        let (a, b) = (self.stock.min, self.stock.max);
        let pts = vec![
            Point2::new(a.x, a.y),
            Point2::new(b.x, a.y),
            Point2::new(b.x, b.y),
            Point2::new(a.x, b.y),
        ];
        Loop::new(pts)
            .and_then(|l| Region::new(vec![l]))
            .map_err(|e| e.to_string())
    }
}
