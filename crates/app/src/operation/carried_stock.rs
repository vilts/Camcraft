//! The stock a later carried setup's verdicts judge against (#319): when
//! the body machined in earlier setups is carried into this one (#317),
//! the rapid, entry, reach and linking laws must never see less material
//! than really stands - so where the carried body stands outside the
//! setup's DECLARED stock, the job's stock becomes the box enclosing both.
//!
//! Provenance: our own design per #308's research decision (the carried
//! body is the incoming stock; a law that cannot read it judges a
//! superset or refuses by name); plain re-clamp practice, no vendor
//! material consulted.
//!
//! ## The laws here
//!
//! - **Never less than reality.** The box encloses the declared stock and
//!   the incoming body's bounds (themselves clamped to the uncut origin
//!   block carried through the turns - both supersets of the body). Every
//!   law reading the job's stock (`stock_facets`, `stock_aabb`, the top)
//!   then judges at least the real material; verify's prior acquittal
//!   (#96, carried by #318) still clears a dive into what was really cut.
//! - **Unchanged where the declared block already covers.** When the
//!   declared stock is a box enclosing the carried block (or the incoming
//!   body), nothing is substituted: the verdict is the one before #319,
//!   byte for byte - and so is every single-setup and clip-entered job,
//!   whose chain crosses no carried boundary.
//! - **A mismatch is said by name, and blocks where it is cut.** The op's
//!   machining region is still the declared stock, so a FEED move (a lead,
//!   the side offset, a plunge, the tool's body under the carried top)
//!   reaching carried material outside it engages stock nothing verified.
//!   Then the path carries [`CARRIED_OUTSIDE_DECLARED`], naming both
//!   setups, the overhang and the move, and the post gate refuses on it
//!   ([`carried_problems`]). Where no feed move reaches it the overhang
//!   is only noted ([`CARRIED_OUTSIDE_NOTED`]): the rapids, entries and
//!   links were judged against the enclosing box. The reach test reads
//!   the carried grid's columns under each feed chord's footprint box, so
//!   it refuses on a near miss, never passes a cut.
//! - **Declared material beyond the carried block is present.** Where the
//!   declared stock reaches past the uncut carried block in XY, the
//!   carried grid cannot speak for that material: the rest mask and
//!   verify's prior are not read, and the raw stock judges.

use crate::geometry::{Point3, Triangle, cube};
use crate::machining2d::GenError;
use crate::model;
use crate::stock::StockShape;
use flecs_ecs::prelude::World;

/// The refusal's head: a feed move reaches carried material outside the
/// declared stock (the post gate refuses on it).
pub const CARRIED_OUTSIDE_DECLARED: &str =
    "post refused: a feed move cuts stock carried from an earlier setup that stands outside this setup's declared stock";

/// The note's head when the overhang exists but no feed move reaches it.
pub const CARRIED_OUTSIDE_NOTED: &str =
    "the stock carried from an earlier setup stands outside this setup's declared stock, clear of every feed move";

/// Containment slack: float noise of a carried corner, never material.
const TOL: f64 = 1e-6;

/// What a carried setup changes in the job; the default changes nothing.
#[derive(Default)]
pub(super) struct Judged {
    /// The box the stock-reading laws judge instead of the declared stock.
    pub stock: Option<(Vec<Triangle>, (Point3, Point3))>,
    /// Carried material outside the declared stock, judged against the
    /// path once it exists ([`Overhang::note`]).
    pub overhang: Option<Overhang>,
    /// The declared stock reaches beyond the carried block in XY: that
    /// material is present (it is declared), and the carried grid - the
    /// rest mask and verify's prior - does not cover it, so neither is
    /// read; the raw stock judges (more material, never less).
    pub distrust_prior: bool,
}

/// Does `outer` hold `inner` in XY and below its top? The bottom is not
/// read: nothing judged from above meets material under the floor.
fn holds(outer: (Point3, Point3), inner: (Point3, Point3)) -> bool {
    let ((o0, o1), (i0, i1)) = (outer, inner);
    i0.x >= o0.x - TOL && i0.y >= o0.y - TOL && i1.x <= o1.x + TOL && i1.y <= o1.y + TOL && i1.z <= o1.z + TOL
}

/// What `op_name`'s job judges in `setup`: the default when the declared
/// stock stands (no carried boundary, or it already covers).
///
/// # Errors
/// A carry the lattice refuses or an origin setup with no stock, named:
/// the stock this setup starts from is unknown, so nothing judges it.
pub(super) fn judged(world: &World, op_name: &str, setup: Option<&str>, tool_r: f64) -> Result<Judged, GenError> {
    let named = |e: String| GenError::Invalid(format!("{op_name}: {e}"));
    let Some((origin, block)) = crate::rest::carried_stock_block(world, op_name, setup).map_err(named)? else {
        return Ok(Judged::default());
    };
    let declared = model::stock_world_aabb_for(world, setup).map(|b| (b.min, b.max));
    let distrust_prior = declared.is_some_and(|(d0, d1)| {
        d0.x < block.0.x - TOL || d0.y < block.0.y - TOL || d1.x > block.1.x + TOL || d1.y > block.1.y + TOL
    });
    let unchanged = Judged { distrust_prior, ..Judged::default() };
    let boxed = crate::setup::stock_entity(world, setup)
        .and_then(|e| model::stock_def_of(world, &e))
        .is_some_and(|s| matches!(s, StockShape::Box { .. }));
    let covers = |b: (Point3, Point3)| boxed && declared.is_some_and(|d| holds(d, b));
    if covers(block) {
        return Ok(unchanged);
    }
    // The real body, when the grid can cut it; the uncut block otherwise
    // (a superset either way - an unreadable body is never read as air).
    let (body, grid) = match crate::rest::carried_stock_body(world, op_name, setup, tool_r, block) {
        Ok(Some((b, g))) => (b, Some(g)),
        // No material carried at all: any declared stock covers it.
        Ok(None) => return Ok(unchanged),
        Err(e) => {
            tracing::warn!(op = %op_name, "the carried body is unreadable, its uncut block is judged: {e}");
            (block, None)
        }
    };
    if covers(body) {
        return Ok(unchanged);
    }
    let (lo, hi) = match declared {
        Some((d0, d1)) => (
            Point3::new(d0.x.min(body.0.x), d0.y.min(body.0.y), d0.z.min(block.0.z)),
            Point3::new(d1.x.max(body.1.x), d1.y.max(body.1.y), d1.z.max(body.1.z)),
        ),
        None => (Point3::new(body.0.x, body.0.y, block.0.z), body.1),
    };
    let overhang = declared.filter(|d| !holds(*d, body)).map(|declared| Overhang {
        declared,
        body,
        grid,
        origin,
        to: setup.unwrap_or_default().to_owned(),
    });
    Ok(Judged { stock: Some((cube(lo, hi).facets.to_vec(), (lo, hi))), overhang, distrust_prior })
}

/// Carried material standing outside the declared stock (#319): the
/// declared box, the carried body's bounds, and the two setups.
pub(super) struct Overhang {
    declared: (Point3, Point3),
    body: (Point3, Point3),
    /// The incoming body's columns; `None` (unreadable) reads the whole
    /// body box as standing to its top.
    grid: Option<crate::stock_sim::Dexel>,
    origin: String,
    to: String,
}

impl Overhang {
    /// The first feed move whose cutter (radius `r`, its body standing up
    /// from the tip) may reach carried material outside the declared stock.
    fn engaged_by(&self, path: &crate::cl::Toolpath, r: f64) -> Option<usize> {
        let ((d0, d1), (b0, b1)) = (self.declared, self.body);
        let above = b1.z > d1.z + TOL;
        path.moves.iter().enumerate().filter(|(_, m)| m.kind != crate::cl::MoveKind::Rapid).find_map(|(i, m)| {
            if m.from.z.min(m.to.z) >= b1.z - TOL {
                return None; // the tip rides at or above the carried top
            }
            let tip = m.from.z.min(m.to.z);
            let reach = m.chords(0.01).into_iter().any(|(a, b)| {
                // The chord's footprint box, clipped to the carried body.
                let (x0, x1) = ((a.x.min(b.x) - r).max(b0.x), (a.x.max(b.x) + r).min(b1.x));
                let (y0, y1) = ((a.y.min(b.y) - r).max(b0.y), (a.y.max(b.y) + r).min(b1.y));
                if !(x1 > x0 + TOL && y1 > y0 + TOL) {
                    return false;
                }
                match &self.grid {
                    Some(g) => self.columns_reach(&crate::stock_sim::Columns::of_grid(g), (x0, x1, y0, y1), tip),
                    None => x0 < d0.x - TOL || y0 < d0.y - TOL || x1 > d1.x + TOL || y1 > d1.y + TOL || above,
                }
            });
            reach.then_some(i)
        })
    }

    /// Does a carried column under the footprint box stand above the tip
    /// where the declared stock does not hold it - beside the declared box,
    /// or above its top? A column cut through still reads its top (more
    /// material, never less); a column that cannot be read reaches.
    fn columns_reach(&self, g: &crate::stock_sim::Columns<'_>, (x0, x1, y0, y1): (f64, f64, f64, f64), tip: f64) -> bool {
        let (d0, d1) = self.declared;
        let (cell, o) = (g.cell(), g.origin());
        if !(cell > 0.0 && cell.is_finite()) {
            return true;
        }
        let h = cell * 0.5;
        let span = |lo: f64, hi: f64, o: f64, n: usize| {
            let a = ((lo - o) / cell).floor().max(0.0).min(n as f64) as usize;
            let b = ((hi - o) / cell).ceil().max(0.0).min(n as f64) as usize;
            a..b
        };
        for iy in span(y0, y1, o.y, g.ny()) {
            for ix in span(x0, x1, o.x, g.nx()) {
                let t = g.top_at(ix, iy);
                if t.is_nan() {
                    return true;
                }
                if !(t.is_finite() && t > tip + TOL) {
                    continue;
                }
                let c = g.center(ix, iy);
                let beside = c.x - h < d0.x - TOL || c.y - h < d0.y - TOL || c.x + h > d1.x + TOL || c.y + h > d1.y + TOL;
                if beside || t > d1.z + TOL {
                    return true;
                }
            }
        }
        false
    }

    /// The path's note: the refusal when a feed move reaches the
    /// overhang, the plain note otherwise.
    pub(super) fn note(&self, path: &crate::cl::Toolpath, r: f64) -> String {
        let ((d0, d1), (b0, b1)) = (self.declared, self.body);
        let side = [d0.x - b0.x, d0.y - b0.y, b1.x - d1.x, b1.y - d1.y].into_iter().fold(0.0, f64::max);
        let above = (b1.z - d1.z).max(0.0);
        let (from, to) = (&self.origin, &self.to);
        let overhang = format!("from setup '{from}' into setup '{to}': up to {side:.2} mm beside it, {above:.2} mm above its top");
        match self.engaged_by(path, r) {
            Some(i) => format!(
                "{CARRIED_OUTSIDE_DECLARED} ({overhang}; feed move {i} reaches it) - enlarge setup '{to}'s stock to cover what setup '{from}' leaves"
            ),
            None => format!("{CARRIED_OUTSIDE_NOTED} ({overhang}) - rapids, entries and links were judged against both"),
        }
    }
}

/// The post gate's carried-stock clause (#319): every operation in the
/// program whose cached path carries [`CARRIED_OUTSIDE_DECLARED`] refuses
/// the post by name. The note is part of the stamped cache, so a fresh
/// op's note is the verdict on the world as it stands.
#[must_use]
pub fn carried_problems(world: &World) -> Vec<String> {
    let Ok(ops) = crate::posting::program_ops(world) else {
        return Vec::new(); // program_ops' own refusal holds the post
    };
    ops.iter()
        .filter_map(|(name, e)| model::toolpath_of(world, e).map(|t| (name, t)))
        .flat_map(|(name, t)| {
            t.path
                .notes
                .iter()
                .filter(|n| n.starts_with(CARRIED_OUTSIDE_DECLARED))
                .map(|n| format!("operation {name}: {n}"))
                .collect::<Vec<_>>()
        })
        .collect()
}
