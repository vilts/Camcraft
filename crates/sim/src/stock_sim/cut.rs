//! Split from `stock_sim.rs` by M5.0.1; structure only.

use super::kernel::{MAX_CELLS_PER_AXIS, StockError, part_surface};
use super::dexel::Dexel;

use crate::cl::{Move, MoveKind};
use crate::geometry::Triangle;
/// One operation's claim on the stock: its cached moves and the tool
/// that cut them. `ball` picks the exact sphere atom for a ball nose
/// and leaves the flat-disc atom for everything whose final hole or
/// groove IS its cylinder (end mills, drills, taps). `corner` picks
/// the exact bull-nose atom (M13.1): the corner radius of a
/// torus-cornered end mill, `0 < corner <= radius`. Before it the
/// bull nose swept the flat disc, over-reporting removal beside every
/// fillet wall; now the corner leaves what the real tool leaves.
/// `cone` picks the exact cone atom for a chamfer mill (M2.4 B): the
/// cotangent of the cone's half-angle from the axis, moves read as the
/// path of the virtual sharp on the axis. `ball`, `cone` and `corner`
/// are the kind's exclusive choice upstream, and `simulate` refuses
/// any contradictory pair rather than silently preferring one atom.
#[derive(Clone, Copy, Debug)]
pub struct SimOp<'a> {
    pub moves: &'a [Move],
    pub radius: f64,
    pub ball: bool,
    pub cone: Option<f64>,
    pub corner: Option<f64>,
}

/// The lattice law, stated once (and the same numbers `envelope_scan`
/// has always used): an eighth of the smallest tool radius sees every
/// scallop the tolerance must keep, and the span growth keeps the
/// grid honest on dust motes and stadiums alike.
#[must_use]
pub fn auto_cell(min_radius: f64, span_x: f64, span_y: f64) -> f64 {
    (min_radius * 0.125)
        .max(1e-4)
        .max(span_x / 1023.0)
        .max(span_y / 1023.0)
}

impl Dexel {
    /// The stock solid as a grid. Box and cylinder are analytic - the
    /// cylinder's columns outside its own circle start as air, so a
    /// sweep never finds material in the machine's world beside the
    /// round. `FromModel` carries its facets in the world, not the
    /// enum, so it builds through `from_facets` - a refusal with the
    /// right answer in its words, never a silent box.
    pub fn from_shape(shape: &crate::stock::StockShape, cell: f64) -> Result<Self, StockError> {
        use crate::stock::StockShape as S;
        match shape {
            S::Box { dx, dy, dz } => {
                if !(dx.is_finite() && dy.is_finite() && dz.is_finite() && *dx > 0.0 && *dy > 0.0 && *dz > 0.0)
                {
                    return Err(StockError::Invalid("box stock needs finite positive dx, dy, dz".into()));
                }
                Self::block(-dx / 2.0, -dy / 2.0, dx / 2.0, dy / 2.0, *dz, cell)
            }
            S::Cylinder { diameter, height } => {
                if !(diameter.is_finite() && height.is_finite() && *diameter > 0.0 && *height > 0.0)
                {
                    return Err(StockError::Invalid("cylinder stock needs finite positive diameter, height".into()));
                }
                let mut dex = Self::block(
                    -diameter / 2.0,
                    -diameter / 2.0,
                    diameter / 2.0,
                    diameter / 2.0,
                    *height,
                    cell,
                )?;
                // Tile-aware air (M5.3 E2): the same `hypot` cell law
                // on the ring the circle crosses, one number for the
                // tiles wholly outside it.
                dex.air_outside_circle(*diameter / 2.0);
                Ok(dex)
            }
            S::FromModel { .. } => Err(StockError::Invalid(
                "model stock builds from its facets: Dexel::from_facets".into(),
            )),
        }
    }

    /// Stock that IS a mesh (stock-from-model): every column starts at
    /// the mesh's own top, air where no facet covers - `part_surface`
    /// read once as an initial state instead of a verdict. The one
    /// cell of margin keeps a sweep that starts exactly on the bbox
    /// edge inside the grid that must judge it.
    pub fn from_facets(facets: &[Triangle], cell: f64) -> Result<Self, StockError> {
        if !(cell.is_finite() && cell > 0.0) {
            return Err(StockError::Invalid(format!("cell {cell} is not finite-positive")));
        }
        let (mut bx0, mut by0, mut bz1) = (f64::INFINITY, f64::INFINITY, f64::NEG_INFINITY);
        let (mut bx1, mut by1) = (f64::NEG_INFINITY, f64::NEG_INFINITY);
        for t in facets {
            for v in [t.a, t.b, t.c] {
                if !(v.x.is_finite() && v.y.is_finite() && v.z.is_finite()) {
                    return Err(StockError::Invalid("stock mesh has a non-finite vertex".into()));
                }
                bx0 = bx0.min(v.x);
                bx1 = bx1.max(v.x);
                by0 = by0.min(v.y);
                by1 = by1.max(v.y);
                bz1 = bz1.max(v.z);
            }
        }
        if facets.is_empty() || bx1 <= bx0 || by1 <= by0 {
            return Err(StockError::Invalid(
                "stock mesh is empty or flat in XY: nothing to simulate against".into(),
            ));
        }
        let (x0, y0) = (bx0 - cell, by0 - cell);
        let nx = (((bx1 - bx0) / cell).ceil() as usize + 2).max(1);
        let ny = (((by1 - by0) / cell).ceil() as usize + 2).max(1);
        if nx > MAX_CELLS_PER_AXIS || ny > MAX_CELLS_PER_AXIS {
            return Err(StockError::Invalid(format!(
                "grid would be {nx}x{ny} cells; at most {MAX_CELLS_PER_AXIS} per axis"
            )));
        }
        // The mesh's own surface, wrapped into tiles: most of a
        // model's bounding box is air or full height, and those runs
        // cost one number each, not a cell apiece (M5.3 E2).
        Ok(Dexel::from_grid(
            x0,
            y0,
            cell,
            nx,
            ny,
            if bz1.is_finite() { bz1 } else { 0.0 },
            part_surface(facets, x0, y0, cell, nx, ny),
        ))
    }
}

/// Run the program: each op's moves, in order, through the same grid.
/// Refusals happen BEFORE any subtraction, so a bad op cannot leave a
/// half-run grid that looks like a sim.
pub fn simulate(dex: &mut Dexel, ops: &[SimOp]) -> Result<(), StockError> {
    check_ops(ops)?;
    // The column atom is an UPRIGHT solid: every subtract below reads
    // the tool as standing on +Z whatever `Move::axis` says. A posed
    // move cut that way is the tilted program judged as an upright
    // one - a silent wrong verdict - so the column arm refuses it by
    // name, before a single column is lowered. The field arm reads the
    // pose (`field_cut`); a caller that wants posed stock asks it.
    for (i, op) in ops.iter().enumerate() {
        if let Some(k) = first_posed(op.moves) {
            return Err(StockError::Invalid(format!(
                "op {i} move {k}: {POSED_REFUSAL}"
            )));
        }
    }
    // A display lattice cut: the chord budget follows the cell the
    // viewer chose, which is honest for what this is - a picture of
    // stock being removed. The VERDICT scan (`envelope_scan`) keeps
    // the stricter quarter-tolerance law, because a measurement may
    // never be coarser than the promise it measures.
    let mut history = dex.history.as_deref().cloned();
    if let Some(h) = &mut history {
        h.add_ops(ops).map_err(StockError::Invalid)?;
        if h.active {
            *dex = h.read(false).map_err(StockError::Invalid)?.dex;
            return Ok(());
        }
    }
    let chord_tol = (dex.cell * 0.25).max(1e-9);
    for op in ops {
        for m in op.moves {
            match (op.cone, op.corner) {
                (Some(cot), _) => dex.subtract_cone(m, op.radius, cot, chord_tol),
                (None, Some(c)) => dex.subtract_bull(m, op.radius, c, chord_tol),
                (None, None) if op.ball => dex.subtract_ball(m, op.radius, chord_tol),
                (None, None) => dex.subtract(m, op.radius, chord_tol),
            }
        }
    }
    dex.history = history.map(std::sync::Arc::new);
    Ok(())
}

/// The column engine's refusal of a tilted tool, said once so every
/// caller (the post gate's not-judged line included) quotes one text.
pub const POSED_REFUSAL: &str = "posed moves are not modelled by the column engine (its atoms stand upright on +Z; a tilted tool cut upright is a wrong verdict)";

/// The first CUTTING move whose tool axis is not upright: `Some(a)`
/// with `a` anything but a positive multiple of +Z (a non-finite axis
/// counts - it states no upright pose either). `None` axes and an
/// explicit +Z stamp are the prismatic case the column atoms model
/// exactly; rapids touch nothing on either arm (RS-274 G0), so a
/// posed rapid changes no verdict and is not refused.
#[must_use]
pub fn first_posed(moves: &[Move]) -> Option<usize> {
    moves.iter().position(|m| {
        m.kind != MoveKind::Rapid
            && m.axis.is_some_and(|a| {
                !(a[0] == 0.0 && a[1] == 0.0 && a[2].is_finite() && a[2] > 0.0)
            })
    })
}

/// The program's input gate, shared by both sim arms (M7.1 C): a
/// refusal the seam states identically whichever engine was asked,
/// so a verdict never depends on which backend a call happened to
/// build. Checked BEFORE any subtraction, so a bad op cannot leave
/// a half-run stock that looks like a sim.
pub(crate) fn check_ops(ops: &[SimOp]) -> Result<(), StockError> {
    for (i, op) in ops.iter().enumerate() {
        if !(op.radius.is_finite() && op.radius > 0.0) {
            return Err(StockError::Invalid(format!(
                "op {i} tool radius {} is not finite-positive",
                op.radius
            )));
        }
        if !op.moves.iter().all(|m| m.is_finite()) {
            return Err(StockError::Invalid(format!(
                "op {i} carries a non-finite move - the sim refuses instead of cutting nothing silently"
            )));
        }
        if op.ball && op.cone.is_some() {
            return Err(StockError::Invalid(format!(
                "op {i} asks for a ball and a cone at once - no tool is both"
            )));
        }
        if let Some(c) = op.corner {
            if op.ball || op.cone.is_some() {
                return Err(StockError::Invalid(format!(
                    "op {i} asks for a bull-nose corner on a ball or a cone - no tool is both"
                )));
            }
            if !(c.is_finite() && c > 0.0 && c <= op.radius) {
                return Err(StockError::Invalid(format!(
                    "op {i} bull-nose corner radius {c} is not finite-positive within the tool radius {}",
                    op.radius
                )));
            }
        }
        if op.cone.is_some_and(|c| !(c.is_finite() && c > 0.0)) {
            return Err(StockError::Invalid(format!(
                "op {i} cone slope {:?} is not finite-positive",
                op.cone
            )));
        }
    }
    Ok(())
}
