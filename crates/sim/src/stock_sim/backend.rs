//! The stock seam (M7.1 C): one API, two engines.
//!
//! Provenance: the column arm is the dexel that has answered every
//! verdict since M2.1; the field arm is the brick SDF of M7.1 A/B,
//! its heights read by the ray march in `march`. Nothing from vendor
//! binaries.
//!
//! ## The laws here
//!
//! - **A verdict says the same thing either way.** `deviate`,
//!   `measure`, and `surface` answer in HEIGHTS on the stock's own XY
//!   lattice - the column engine's vocabulary - so a `Deviation` or
//!   `StockReport` can be compared value for value across arms, and
//!   every aggregate (rest, gouge, their locations, NaN for the
//!   columns that judge nothing) is the same law computed twice.
//! - **Refusals are backend-independent.** Every gate a column arm
//!   refuses through, the field arm refuses through with the same
//!   words, checked the same order before a single cell is written;
//!   a program that will not run runs nowhere. The field's two extra
//!   truths - watertightness (the parity needs it) and a chord whose
//!   window would span the grid - refuse BY NAME and are honest
//!   over-strictness, never a silent pass.
//! - **The flat tool's flank spans the verdict world** (see
//!   `spanning_flank`): the column atom is an infinite cylinder, and
//!   the field makes that law exact below the world's ceiling by
//!   giving the solid a flank that reaches past anything a verdict
//!   can stand on. Where the spanning law cannot reach (a path below
//!   the march floor, the truncated cone's chimney above its max
//!   radius) the field shows MORE rest than the column - the blessed
//!   direction: rest over-reported is noise, rest hidden is a gouge
//!   waiting.
//! - **The column arm is what ships.** App and motion still build
//!   `Dexel` directly; routing those call sites through this seam is
//!   M7.3's retirement march, with the parity suite here as its
//!   safety law. Until then the field is the verifiable second
//!   opinion, which is exactly what a new engine should be.

use super::cut::{check_ops, SimOp};
use super::dexel::Dexel;
use super::deviate::{deviate, Deviation, SimSurface};
use super::field::SdfField;
use super::field_cut::SdfTool;
use super::kernel::{measure as column_measure, part_surface, StockError, StockReport};
use super::march;
use super::march_verdicts;

use crate::geo2d::{Point2, Region};
use crate::geometry::{Point3, Triangle};

/// Which engine a stock is built on. `Column` is the dexel height
/// field; `Field` is the brick SDF, `band` cells wide on each side of
/// the shell (a memory policy, not a precision one - the march reads
/// only signs and step bounds, so `band` 1 answers the same verdicts
/// at more rebuild cost, and 4 is the shipped default).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Backend {
    Column,
    Field { band: u32 },
}

/// The shipped field band, in cells (see `Backend::Field`).
pub const FIELD_BAND: u32 = 4;

/// The field's declared verdict epsilon, in mm of column height: the
/// march's solver floor (golden-section and bisection residuals, ~1e-7
/// per column) with headroom. Two engines agree when every column's
/// deviation lands within it and every verdict word matches (M7.3).
pub const FIELD_VERDICT_EPS: f64 = 1e-5;

/// One stock, one verdict API, two engines behind it.
#[derive(Clone, Debug)]
pub enum Stock {
    Column(Dexel),
    Field(SdfField),
}

impl Stock {
    /// The engine's cell edge, in mm.
    #[must_use]
    pub fn cell(&self) -> f64 {
        match self {
            Stock::Column(d) => d.cell,
            Stock::Field(f) => f.cell(),
        }
    }

    #[must_use]
    pub fn column(&self) -> Option<&Dexel> {
        match self {
            Stock::Column(d) => Some(d),
            Stock::Field(_) => None,
        }
    }

    #[must_use]
    pub fn field(&self) -> Option<&SdfField> {
        match self {
            Stock::Field(f) => Some(f),
            Stock::Column(_) => None,
        }
    }

    /// The signed distance at `p`: NEGATIVE IN MATERIAL, positive in
    /// air, NaN where the engine cannot judge the point. The column
    /// arm answers the VERTICAL distance to its flat top (outside the
    /// grid there is no verdict); the field arm answers the true
    /// Euclidean distance to the remaining solid. Same sign law - the
    /// vocabulary differs, and callers that need a height ask `top`.
    #[must_use]
    pub fn distance(&self, p: Point3) -> f64 {
        match self {
            Stock::Column(d) => {
                if !p.is_finite() {
                    return f64::NAN;
                }
                match d.col(Point2::new(p.x, p.y)) {
                    None => f64::NAN, // outside the grid: no verdict, not air
                    Some(k) => p.z - d.top_at(k),
                }
            }
            Stock::Field(f) => f.distance(p),
        }
    }

    /// The Z of the top of remaining material at `(x, y)`: marched
    /// from the world's ceiling on the field arm, read from the tile
    /// on the column arm - the height a verdict is made of, NaN when
    /// no engine can say one (no column there, air, an unfinished
    /// march).
    #[must_use]
    pub fn top(&self, x: f64, y: f64) -> f64 {
        match self {
            Stock::Column(d) => match d.col(Point2::new(x, y)) {
                Some(k) if x.is_finite() && y.is_finite() => d.top_at(k),
                _ => f64::NAN,
            },
            Stock::Field(f) => march::column_top(f, x, y),
        }
    }

    /// Run the program. `check_ops` refuses the invalid before either
    /// arm writes a cell, so a bad op cannot leave half a stock; past
    /// that gate the column arm is `cut::simulate` unchanged and the
    /// field arm walks the same moves through the same three solids,
    /// with `spanning_flank` giving the flat tool - and the ball its
    /// shank (M7.3: the bare sphere left a waterline's stock top
    /// floating above the cut) - the column's reach, batched per
    /// brick (M7.3's cost law, `field_batch`).
    pub fn simulate(&mut self, ops: &[SimOp]) -> Result<(), StockError> {
        self.simulate_with(ops, false)
    }

    /// Run a twist-drill program (#2441): identical to [`Self::simulate`],
    /// except a `cone` op carries a cylindrical flute above its cone (`SdfTool::Drill`)
    /// rather than a truncated chamfer cone (`SdfTool::Taper`).
    pub fn simulate_drill(&mut self, ops: &[SimOp]) -> Result<(), StockError> {
        self.simulate_with(ops, true)
    }

    fn simulate_with(&mut self, ops: &[SimOp], drill: bool) -> Result<(), StockError> {
        match self {
            Stock::Column(d) => super::cut::simulate(d, ops),
            Stock::Field(f) => simulate_field_with(f, ops, drill),
        }
    }

    /// The program's verdict against the model, per column. The tol
    /// gate and every aggregate are `deviate`'s law read off the
    /// engine's own tops (the height-parity law above).
    pub fn deviate(&self, facets: &[Triangle], tol: f64) -> Result<Deviation, StockError> {
        match self {
            Stock::Column(d) => deviate(d, facets, tol),
            Stock::Field(f) => march_verdicts::deviate_field(f, facets, tol),
        }
    }

    /// One operation's report against the model within a footprint.
    /// The column arm is `kernel::measure` over its grid's part
    /// surface; the field arm marches both the stock's own top and
    /// what remains, so `removed_volume` means the same depth-times-
    /// area either way (for air columns the semi-infinite block's
    /// gross count is the column's, and the field counts only steel
    /// it found - the cylinder's corners are where that shows).
    #[must_use]
    pub fn measure(&self, model: &[Triangle], footprint: &Region, tol: f64) -> StockReport {
        match self {
            Stock::Column(d) => {
                let part = part_surface(model, d.x0, d.y0, d.cell, d.nx, d.ny);
                column_measure(d, &part, footprint, tol)
            }
            Stock::Field(f) => march_verdicts::measure_field(f, model, footprint, tol),
        }
    }

    /// The sheet the viewer paints, classed as `deviate` judged it.
    #[must_use]
    pub fn surface(&self, classes: &[f32]) -> SimSurface {
        match self {
            Stock::Column(d) => d.surface(classes),
            Stock::Field(f) => march_verdicts::sheet_field(f, classes),
        }
    }

    /// The machined volume as the eye sees it (M7.2): a sphere-traced
    /// frame of this stock, classed per pixel against `target` - the
    /// model's own field, so the deviation is located wherever the
    /// ray lands, not only at column tops. The field arm only: the
    /// column's vertical distance would let a lateral ray slip past
    /// its walls, so that arm keeps the sheet until M7.3 retires it,
    /// and says so by name.
    pub fn preview(
        &self,
        target: Option<&Stock>,
        cam: &super::preview::PreviewCamera,
        w: u32,
        h: u32,
        tol: f64,
    ) -> Result<super::preview::Preview, StockError> {
        let f = self
            .field()
            .ok_or_else(|| StockError::Invalid("the column arm previews through its sheet mesh; the ray march needs the field arm".into()))?;
        let mut tf = None;
        if let Some(t) = target {
            tf = Some(
                t.field()
                    .ok_or_else(|| StockError::Invalid("the deviation target must be a field: the model needs a signed distance, not a height field".into()))?,
            );
        }
        super::preview::preview(f, tf, cam, w, h, tol)
    }
}

/// The column engine's refusal of a turning program, said once.
pub const LATHE_COLUMN_REFUSAL: &str = "a revolved path is not judgeable in the mill's dexel frame - the column engine has no annular-sector sweep; the field arm cuts it (Stock::simulate_lathe)";

/// The flank that makes the field's flat tool reach what the column
/// atom reaches: every height a verdict can stand on sits within the
/// shell's diagonal plus two caps of the deepest tip position the
/// program can hold, so a solid that tall claims exactly the column's
/// removal below that world. Above it the two engines were never
/// going to agree (the column removes everything to the sky), and no
/// verdict lives there.
pub(crate) fn spanning_flank(f: &SdfField) -> f64 {
    let b = f.bounds();
    let (dx, dy, dz) = (b.max.x - b.min.x, b.max.y - b.min.y, b.max.z - b.min.z);
    (dx * dx + dy * dy + dz * dz).sqrt() + 2.0 * f.cap() + 4.0 * f.cell()
}

/// Map a `SimOp` to the field engine's solid (#2441 shared across `backend`
/// and `display`). `drill` switches `op.cone` from a truncated `Taper` to
/// a twist `Drill` with a cylindrical flute of length `flank`.
pub(crate) fn op_sdf_tool(op: &SimOp, flank: f64, shanked_ball: bool, drill: bool) -> SdfTool {
    if op.ball {
        if shanked_ball {
            SdfTool::Shanked { r: op.radius, shank: flank }
        } else {
            SdfTool::Ball { r: op.radius }
        }
    } else if let Some(cot) = op.cone {
        if drill {
            SdfTool::Drill { r: op.radius, cot, flank: flank.max(op.radius * cot) }
        } else {
            SdfTool::Taper { r: op.radius, cot }
        }
    } else if let Some(corner) = op.corner {
        SdfTool::BullNose { r: op.radius, corner, flank: flank.max(corner) }
    } else {
        SdfTool::Flat { r: op.radius, flank }
    }
}

/// The field arm of [`Stock::simulate`], stated once so a carried field
/// (`CarriedField::simulate`, #637 E2c) cuts through the very same tool
/// construction: `check_ops` first, then each op's moves batched per brick
/// against the solid `spanning_flank` gives the column's reach.
pub(crate) fn simulate_field_with(f: &mut SdfField, ops: &[SimOp], drill: bool) -> Result<(), StockError> {
    check_ops(ops)?;
    let chord_tol = (f.cell() * 0.25).max(1e-9);
    let flank = spanning_flank(f);
    for (i, op) in ops.iter().enumerate() {
        let tool = op_sdf_tool(op, flank, true, drill);
        for chunk in op.moves.chunks(super::field_batch::BATCH_MOVES) {
            f.subtract_batch(chunk, &tool, chord_tol).map_err(|e| named_op(i, e))?;
        }
    }
    Ok(())
}

/// A field refusal, said with the op that caused it: the column
/// arm's gates are per-op by construction, and an error that cannot
/// say which op raised its hand is the opaque failure AGENTS.md
/// refuses.
pub(crate) fn named_op(i: usize, e: StockError) -> StockError {
    match e {
        StockError::Invalid(s) => StockError::Invalid(format!("op {i}: {s}")),
        other => other,
    }
}
