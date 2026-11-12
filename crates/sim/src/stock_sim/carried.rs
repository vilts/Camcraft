//! Carried stock on either engine (#637 step E3): the two-sided body a
//! program hands from setup to setup, held behind one type so the app's
//! holders (`stock_sim_carry`, `rest`, `rest/across`) carry, clip and cut
//! it without knowing which engine stands behind it.
//!
//! ## The laws here
//!
//! - **One verdict, read back.** Both arms answer through the column
//!   [`Body`]: the column arm IS one, the field arm reads its columns back
//!   onto its lattice (`FieldBody::body`, #637 step D). `deviate`,
//!   `display`, `volume` and `bounds` are then the column carry's own
//!   code on both arms - the one two-sided verdict the goldens prove.
//! - **The column arm is the old code, call for call.** `carry` is
//!   `Body::carry`, `simulate` is `cut::simulate` on its tops, the clip is
//!   the loop `stock_sim_carry` ran: every shipped verdict is unchanged.
//! - **The field never reports less than is there.** A cut-through column
//!   is not air (see `FieldBody::body`); the clip is the column law as a
//!   solid, not an intersection that would delete what stands beside the
//!   new block (see `clip_under`).
//!
//! Provenance: our own design (#637's design comments; the decision file
//! `docs/decisions/637-carried-stock-holders.md`). CSG by min/max of
//! signed distances keeps the zero set exactly (Hart 1996, sphere
//! tracing of implicit surfaces). No vendor material consulted.

use super::carry::{Body, LatticeTurn};
use super::cut::SimOp;
use super::deviate::Deviation;
use super::dexel::Dexel;
use super::field::SdfField;
use super::field_body::Lattice;
use super::field_carry::CarriedField;
use super::kernel::StockError;
use crate::geo2d::Point2;
use crate::geometry::{Point3, Triangle};

/// The carried stock, on the engine that holds it.
#[derive(Clone, Debug)]
pub enum CarriedStock {
    Column(Body),
    /// Boxed: the field body is several times the column body's size.
    Field(Box<FieldBody>),
}

/// A clip taken at a setup change: the entered setup's own stock grid (in
/// that setup's frame) and the turns the body has been carried through
/// since.
#[derive(Clone, Debug)]
pub(super) struct Ceiling {
    pub(super) grid: Dexel,
    pub(super) since: LatticeTurn,
}

/// The field arm: the carried field, the lattice its verdict columns
/// stand on in the current frame, and the clips taken on the way.
#[derive(Clone, Debug)]
pub struct FieldBody {
    pub(super) carried: CarriedField,
    pub(super) lat: Lattice,
    pub(super) ceilings: Vec<Ceiling>,
}

/// Is `lat` a finite, non-empty lattice?
pub(super) fn lattice_ok(lat: &Lattice) -> bool {
    lat.x0.is_finite() && lat.y0.is_finite() && lat.cell.is_finite() && lat.cell > 0.0 && lat.nx > 0 && lat.ny > 0
}

/// The lattice of a column grid.
#[must_use]
pub fn lattice_of_grid(d: &Dexel) -> Lattice {
    Lattice { x0: d.x0, y0: d.y0, cell: d.cell, nx: d.nx, ny: d.ny }
}

/// `lat` seen through `turn`: the image lattice `Body::carry` re-indexes
/// onto, by the same arithmetic (its corners mapped, the lower-left
/// taken, the counts swapped by a quarter turn).
pub(super) fn image(lat: &Lattice, turn: &LatticeTurn) -> Lattice {
    let (w, h) = (lat.nx as f64 * lat.cell, lat.ny as f64 * lat.cell);
    let corners = [(lat.x0, lat.y0), (lat.x0 + w, lat.y0), (lat.x0, lat.y0 + h), (lat.x0 + w, lat.y0 + h)];
    let (mut x0, mut y0) = (f64::INFINITY, f64::INFINITY);
    for (x, y) in corners {
        let p = turn.apply(Point3::new(x, y, 0.0));
        x0 = x0.min(p.x);
        y0 = y0.min(p.y);
    }
    let (nx, ny) = if turn.swaps_xy() { (lat.ny, lat.nx) } else { (lat.nx, lat.ny) };
    Lattice { x0, y0, cell: lat.cell, nx, ny }
}

/// The column clip (#317, M3.2 A): inside `grid` (the entered setup's
/// own stock, same frame) no column of `dex` may stand above that
/// stock's top; outside it, and over its air, what stands is untouched.
pub fn clip_columns_under(dex: &mut Dexel, grid: &Dexel) {
    for iy in 0..dex.ny {
        for ix in 0..dex.nx {
            let q = dex.center(ix, iy);
            let Some(k) = grid.col(q) else { continue };
            let t = grid.top_at(k);
            if t.is_finite() {
                dex.lower(ix + iy * dex.nx, t);
            }
        }
    }
}

impl FieldBody {
    /// The translation that puts the field's own frame (lattice corner at
    /// the origin, so verdict columns sit on cell centres) at `lat`.
    fn view(lat: &Lattice) -> Result<LatticeTurn, StockError> {
        if !lattice_ok(lat) {
            return Err(StockError::Invalid("the carried lattice is not a finite, non-empty grid".into()));
        }
        LatticeTurn::from_matrix([[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]], [lat.x0, lat.y0, 0.0])
            .map_err(StockError::Invalid)
    }

    /// A block stock covering the whole of `lat`, from `floor` up to
    /// `top`: a closed box, not the semi-infinite column (the decision
    /// file: a phantom floor makes every cut after a flip sweep it).
    ///
    /// # Errors
    /// A non-finite or empty lattice or height range; the field's own
    /// construction refusals.
    pub fn block(lat: Lattice, floor: f64, top: f64, band: u32) -> Result<Self, StockError> {
        let turn = Self::view(&lat)?;
        if !(floor.is_finite() && top.is_finite() && floor < top) {
            return Err(StockError::Invalid("the carried block needs a finite floor below a finite top".into()));
        }
        let (w, h) = (lat.nx as f64 * lat.cell, lat.ny as f64 * lat.cell);
        let field = SdfField::from_box(Point3::new(0.0, 0.0, floor), Point3::new(w, h, top), lat.cell, band)?;
        Ok(FieldBody { carried: CarriedField::new(field, turn), lat, ceilings: Vec::new() })
    }

    /// A stock that is a closed mesh (`facets` in the frame of `lat`).
    ///
    /// # Errors
    /// As [`FieldBody::block`]; a mesh the field refuses (open, empty,
    /// non-finite) is refused by its own name.
    pub fn mesh(lat: Lattice, facets: &[Triangle], band: u32) -> Result<Self, StockError> {
        let turn = Self::view(&lat)?;
        let back = |p: Point3| Point3::new(p.x - lat.x0, p.y - lat.y0, p.z);
        let own: Vec<Triangle> = facets.iter().map(|t| Triangle::new(back(t.a), back(t.b), back(t.c))).collect();
        let field = SdfField::from_mesh(&own, lat.cell, band)?;
        Ok(FieldBody { carried: CarriedField::new(field, turn), lat, ceilings: Vec::new() })
    }

    /// The lattice the verdict columns stand on, in the current frame.
    #[must_use]
    pub fn lattice(&self) -> Lattice {
        self.lat
    }

    /// The same body seen through `turn` (no value moves: `CarriedField`).
    #[must_use]
    pub fn carry(self, turn: &LatticeTurn) -> FieldBody {
        let lat = image(&self.lat, turn);
        let ceilings = self.ceilings.into_iter().map(|c| Ceiling { grid: c.grid, since: c.since.then(*turn) }).collect();
        FieldBody { carried: self.carried.then(*turn), lat, ceilings }
    }

    /// Clip under the entered setup's own stock `grid` (current frame):
    /// the column law as a solid - the body minus the half-infinite prism
    /// above that stock's top over its footprint, `max(d_body, -d_prism)`.
    /// The verdicts read membership along columns, so the prism is kept
    /// and applied where the columns are read (`body`); it commutes with
    /// every later cut, and a later flip turns the ceiling into a floor
    /// bound.
    pub fn clip_under(&mut self, grid: &Dexel) {
        self.ceilings.push(Ceiling { grid: grid.clone(), since: LatticeTurn::IDENTITY });
    }

    /// Cut a program stated in the current frame (`CarriedField::simulate`).
    ///
    /// # Errors
    /// Every refusal `Stock::simulate` makes, before a cell is written.
    pub fn simulate(&mut self, ops: &[SimOp]) -> Result<(), StockError> {
        self.carried.simulate(ops)
    }
}

impl CarriedStock {
    /// The same stock seen through `turn`.
    ///
    /// # Errors
    /// `Body::carry`'s (a lattice the image cannot hold).
    pub fn carry(self, turn: &LatticeTurn) -> Result<CarriedStock, String> {
        Ok(match self {
            CarriedStock::Column(b) => CarriedStock::Column(b.carry(turn)?),
            CarriedStock::Field(f) => CarriedStock::Field(Box::new((*f).carry(turn))),
        })
    }

    /// Run a program stated in the current frame.
    ///
    /// # Errors
    /// The engine's refusals, before a cell is written.
    pub fn simulate(&mut self, ops: &[SimOp]) -> Result<(), StockError> {
        match self {
            CarriedStock::Column(b) => super::cut::simulate(&mut b.dex, ops),
            CarriedStock::Field(f) => f.simulate(ops),
        }
    }

    /// Clip under the entered setup's own stock `grid` (current frame).
    pub fn clip_under(&mut self, grid: &Dexel) {
        match self {
            CarriedStock::Column(b) => clip_columns_under(&mut b.dex, grid),
            CarriedStock::Field(f) => f.clip_under(grid),
        }
    }

    /// The stock as a column body in the current frame: the column arm's
    /// own, the field arm's read-back.
    ///
    /// # Errors
    /// A column the field cannot read (refused by name, never guessed).
    pub fn into_body(self) -> Result<Body, String> {
        match self {
            CarriedStock::Column(b) => Ok(b),
            CarriedStock::Field(f) => f.body(),
        }
    }

    /// The two-sided verdict against the part (`Body::deviate`).
    ///
    /// # Errors
    /// A non-finite tolerance; a column the field cannot read.
    pub fn deviate(&self, facets: &[Triangle], tol: f64) -> Result<Deviation, String> {
        match self {
            CarriedStock::Column(b) => b.deviate(facets, tol).map_err(|e| e.to_string()),
            CarriedStock::Field(f) => f.body()?.deviate(facets, tol).map_err(|e| e.to_string()),
        }
    }
}

/// A point of the current frame's XY, as `Ceiling::since` saw it when the
/// clip was taken.
pub(super) fn clip_xy(since: &LatticeTurn, x: f64, y: f64) -> Point2 {
    let p = since.unapply(Point3::new(x, y, 0.0));
    Point2::new(p.x, p.y)
}
