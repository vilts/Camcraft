//! Workholding in a carried setup (#321): a setup entered with the body
//! the earlier setups machined (#317) holds THAT body, not its declared
//! block - so its fixtures are read against the carried stock. A fixture
//! standing inside the material the program really carries in (a vise
//! jaw modelled into a wall setup 1 left standing, a clamp on a face that
//! is still there) is workholding that cannot be where it is modelled:
//! every fixture verdict of that setup judged steel the machine will not
//! hold there, and the post is refused by name.
//!
//! Provenance: our own design per #308's research decision (the carried
//! body is the incoming stock, read through each setup's transform);
//! plain re-clamp practice from machine-tool handbooks. No vendor
//! material consulted.
//!
//! ## The laws here
//!
//! - **The carried body, not the declared block.** Only the body the
//!   setup is ENTERED with is read (`rest::entered_body`, the carried-body
//!   cache's entry): a clamp seated in a pocket setup 1 cut, inside the
//!   declared block but clear of the real material, is no conflict.
//! - **Penetration, not contact.** A column counts when its centre lies
//!   strictly inside the fixture's footprint and its material
//!   (`floor..top`) overlaps the fixture's steel by more than the
//!   contact tolerance - jaws gripping a side face and a part seated on
//!   a plate or in a nest are contact by design. A footprint narrower
//!   than a cell (holding no centre) is judged by the column under its
//!   own centre, by the box. The column sampling resolves to the
//!   carried grid's pitch: an overlap narrower than half a cell beside a
//!   face can pass, and is stated so in the decision record.
//! - **The shape, not the box (#1530).** The box is the broad phase; a
//!   fixture it flags is then judged by its closed world mesh, in two
//!   readings, and refused when either convicts:
//!   - **On each column** whose centre the box holds: the steel on the
//!     column's vertical line (and on four lines [`CARRIED_CONTACT_TOL`]
//!     beside it - a face at the column centre is contact) against the
//!     column's material.
//!   - **Between the columns**: every rectangle spanned by four
//!     neighbouring material columns (trimmed by the tolerance) is read
//!     WHOLE against the material all four agree on
//!     (`FixtureMesh::steel_in_prism`) - a post or a lip narrower than
//!     the pitch standing in the part cannot pass between the lines.
//!     Where the four disagree (a side face or a step of the carried
//!     part runs through the rectangle) only what they share is judged:
//!     a material boundary is known to one pitch, as the box law always
//!     read it (steel at most one pitch into the material at a side face
//!     or step, short of the next column centre, passes; the decision
//!     record states it, #1580 tracks it).
//!
//!   A vise is its jaws, not the box that spans the part they grip; a
//!   nest is its walls and floor. The mesh only acquits: an open,
//!   refused or coarse-fidelity mesh, a line or rectangle it cannot read
//!   (budget spent, inconsistent winding), keeps the box verdict.
//! - **Live, like every workholding law.** Read at the post gate against
//!   the steel as it stands now, never baked into a stamp (`fixture`'s
//!   live-gate law). The machine table is not read: the table is what the
//!   setup's stock stands on, not steel placed against it.
//! - **Unreadable is not clear.** A carry the gate cannot read leaves the
//!   ops of that setup refused by their own generation (#318/#319); this
//!   clause adds nothing for it, it never reads the declared block instead.

use flecs_ecs::prelude::{Entity, World};

use crate::fixture::{FIXTURE_HIT_TOL, FixtureBox};
use crate::geo2d::Point2;
use crate::model;
use crate::stock_sim::Body;
use camcraft_sim::fixture_mesh::{FixtureMesh, PrismReading};

/// The phrase every carried-workholding refusal carries.
pub const FIXTURE_IN_CARRIED: &str = "stands inside the stock carried from";

/// How far a fixture's MESH may reach into the carried material and
/// still be contact, in millimetres (#1530): a jaw face on a side face,
/// a part seated on a nest floor. Ten microns - below any fit a jaw or
/// nest is modelled to, above the float dust of a carried body.
pub const CARRIED_CONTACT_TOL: f64 = 0.01;

/// The cell index span `lo..hi` covers (one cell of slack each side),
/// clamped to the grid: float -> index through a clamp, never an
/// unchecked cast.
fn cell_span(lo: f64, hi: f64, o: f64, cell: f64, n: usize) -> (usize, usize) {
    let top = n as f64;
    let i0 = ((lo - o) / cell - 1.0).floor().clamp(0.0, top);
    let i1 = ((hi - o) / cell + 1.0).ceil().clamp(0.0, top);
    (i0 as usize, i1 as usize)
}

/// `b`'s footprint, ordered, when every bound of it is finite.
fn footprint(b: &FixtureBox) -> Option<(f64, f64, f64, f64)> {
    let (bx0, bx1) = (b.min.x.min(b.max.x), b.max.x.max(b.min.x));
    let (by0, by1) = (b.min.y.min(b.max.y), b.max.y.max(b.min.y));
    let all = [bx0, bx1, by0, by1, b.min.z, b.max.z];
    all.iter().all(|v| v.is_finite()).then_some((bx0, bx1, by0, by1))
}

/// Walk the columns `b`'s footprint holds (centre strictly inside), and
/// the column under the footprint's centre when it holds none, taking
/// the worst depth `judge(k, centre, inside, worst so far)` reports.
fn scan(
    body: &Body,
    b: &FixtureBox,
    mut judge: impl FnMut(usize, Point2, bool, Option<f64>) -> Option<f64>,
) -> Option<f64> {
    let dex = &body.dex;
    let (bx0, bx1, by0, by1) = footprint(b)?;
    if !(dex.cell.is_finite() && dex.cell > 0.0) {
        return None;
    }
    let (ix0, ix1) = cell_span(bx0, bx1, dex.x0, dex.cell, dex.nx);
    let (iy0, iy1) = cell_span(by0, by1, dex.y0, dex.cell, dex.ny);
    let mut worst: Option<f64> = None;
    let mut any_inside = false;
    let take = |worst: &mut Option<f64>, d: Option<f64>| {
        if let Some(d) = d {
            *worst = Some(worst.map_or(d, |w: f64| w.max(d)));
        }
    };
    for iy in iy0..iy1 {
        for ix in ix0..ix1 {
            let c = dex.center(ix, iy);
            let inside = c.x > bx0 + FIXTURE_HIT_TOL
                && c.x < bx1 - FIXTURE_HIT_TOL
                && c.y > by0 + FIXTURE_HIT_TOL
                && c.y < by1 - FIXTURE_HIT_TOL;
            if inside {
                any_inside = true;
                let k = ix + iy * dex.nx;
                if !body.empty_at(k) {
                    let d = judge(k, c, true, worst);
                    take(&mut worst, d);
                }
            }
        }
    }
    // A footprint narrower than a cell holds no column centre: the column
    // under its own centre speaks for it, never nothing.
    let mid = Point2::new((bx0 + bx1) * 0.5, (by0 + by1) * 0.5);
    if !any_inside && let Some(k) = dex.col(mid) && !body.empty_at(k) {
        let d = judge(k, mid, false, worst);
        take(&mut worst, d);
    }
    worst
}

/// Column `k`'s material overlap with the box's Z span.
fn box_depth(body: &Body, b: &FixtureBox, k: usize) -> f64 {
    let (bz0, bz1) = (b.min.z.min(b.max.z), b.max.z.max(b.min.z));
    body.dex.top_at(k).min(bz1) - body.floor_at(k).max(bz0)
}

/// How deep `b` stands in `body`'s material: the largest Z overlap of a
/// column whose centre lies strictly inside the box's footprint, or
/// `None` when no column overlaps beyond [`FIXTURE_HIT_TOL`].
#[must_use]
pub fn penetration(body: &Body, b: &FixtureBox) -> Option<f64> {
    scan(body, b, |k, _, _, _| Some(box_depth(body, b, k)).filter(|d| d.is_finite() && *d > FIXTURE_HIT_TOL))
}

/// Material `lo..hi` overlapped by `spans`, in millimetres.
fn overlap(spans: &[(f64, f64)], lo: f64, hi: f64) -> f64 {
    spans.iter().map(|&(a, b)| (b.min(hi) - a.max(lo)).max(0.0)).sum()
}

/// The facets one fixture's mesh judgement may visit (#1530): past it
/// every column not yet read keeps its box depth. Bounds the gate on a
/// mesh every line meets many facets of (a fine fan, a dense nest): a
/// few seconds at most, and the answer stays on the safe side.
pub const MESH_VISIT_BUDGET: u64 = 50_000_000;

/// How deep the closed `mesh` (whose world box is `b`) stands in
/// `body`'s material (#1530): the worst of [`on_columns`] and
/// [`between_columns`]. `None` when neither finds steel beyond
/// [`CARRIED_CONTACT_TOL`]. `budget` is the facet visits left
/// ([`MESH_VISIT_BUDGET`] for one fixture), shared by both; whatever it
/// leaves unread keeps the box's depth.
#[must_use]
pub fn mesh_penetration(body: &Body, b: &FixtureBox, mesh: &FixtureMesh, budget: &mut u64) -> Option<f64> {
    let worst = on_columns(body, b, mesh, budget);
    let between = between_columns(body, b, mesh, budget, worst);
    match (worst, between) {
        (Some(w), Some(d)) => Some(w.max(d)),
        (w, d) => w.or(d),
    }
}

/// The steel ON the material columns the box holds: per column, the
/// least of its centre line and four lines [`CARRIED_CONTACT_TOL`]
/// beside it (a face at the centre is contact). A line no nudge reads, a
/// spent budget, and a footprint narrower than a cell keep the box's
/// depth. A column whose box depth cannot beat the worst depth already
/// found is not read: the verdict and its depth stand.
fn on_columns(body: &Body, b: &FixtureBox, mesh: &FixtureMesh, budget: &mut u64) -> Option<f64> {
    let t = CARRIED_CONTACT_TOL;
    scan(body, b, |k, c, inside, worst| {
        let boxed = box_depth(body, b, k);
        if !(boxed.is_finite() && boxed > t) || worst.is_some_and(|w| boxed <= w) {
            return None; // the box acquits, or cannot deepen the verdict
        }
        if !inside {
            return Some(boxed);
        }
        let (lo, hi) = (body.floor_at(k), body.dex.top_at(k));
        let mut depth = f64::INFINITY;
        for (dx, dy) in [(0.0, 0.0), (t, 0.0), (-t, 0.0), (0.0, t), (0.0, -t)] {
            let d = mesh.steel_on_line(c.x + dx, c.y + dy, budget).map_or(boxed, |s| overlap(&s, lo, hi));
            depth = depth.min(d);
            if depth <= t {
                return None; // one line clear within the tolerance: contact
            }
        }
        Some(depth).filter(|d| d.is_finite())
    })
}

/// The steel BETWEEN the material columns (#1530 review): every
/// rectangle spanned by four neighbouring columns that all hold
/// material, trimmed by [`CARRIED_CONTACT_TOL`], is read whole against
/// the material the four share (the highest floor to the lowest top) -
/// conservative over the whole rectangle, so no feature narrower than
/// the pitch slips between the column lines. An unread rectangle keeps
/// the box's depth over that shared material. `worst` is the depth
/// already found; a rectangle that cannot beat it is not read.
fn between_columns(body: &Body, b: &FixtureBox, mesh: &FixtureMesh, budget: &mut u64, mut worst: Option<f64>) -> Option<f64> {
    let t = CARRIED_CONTACT_TOL;
    let dex = &body.dex;
    let (bx0, bx1, by0, by1) = footprint(b)?;
    let (bz0, bz1) = (b.min.z.min(b.max.z), b.max.z.max(b.min.z));
    let (ix0, ix1) = cell_span(bx0, bx1, dex.x0, dex.cell, dex.nx);
    let (iy0, iy1) = cell_span(by0, by1, dex.y0, dex.cell, dex.ny);
    for iy in iy0..iy1.min(dex.ny.saturating_sub(1)) {
        for ix in ix0..ix1.min(dex.nx.saturating_sub(1)) {
            let ks = [ix + iy * dex.nx, ix + 1 + iy * dex.nx, ix + (iy + 1) * dex.nx, ix + 1 + (iy + 1) * dex.nx];
            if ks.iter().any(|&k| body.empty_at(k)) {
                continue; // a boundary of the material: judged on its columns
            }
            let lo = ks.iter().map(|&k| body.floor_at(k)).fold(f64::NEG_INFINITY, f64::max);
            let hi = ks.iter().map(|&k| dex.top_at(k)).fold(f64::INFINITY, f64::min);
            let reach = hi.min(bz1) - lo.max(bz0);
            if !(reach.is_finite() && reach > t) || worst.is_some_and(|w| reach <= w) {
                continue; // the box acquits, or cannot deepen the verdict
            }
            let (c0, c1) = (dex.center(ix, iy), dex.center(ix + 1, iy + 1));
            let (x0, x1, y0, y1) = (c0.x + t, c1.x - t, c0.y + t, c1.y - t);
            // A rectangle too slim to read once trimmed (a pitch under
            // ~0.02 mm inverts it, x0 > x1) is not read: the box speaks.
            // Tested BEFORE the off-footprint test, which an inverted
            // rectangle always passes (#1530 review: it cleared a post
            // the box refuses). `!(..>=..)` also keeps the box on NaN.
            let slim = !(x1 - x0 >= 1e-3 && y1 - y0 >= 1e-3);
            if !slim && (x0.max(bx0) > x1.min(bx1) || y0.max(by0) > y1.min(by1)) {
                continue; // the rectangle is off the footprint: no steel
            }
            let reading = if slim { PrismReading::Unread } else { mesh.steel_in_prism((x0, x1), (y0, y1), (lo, hi), t, budget) };
            let d = match reading {
                PrismReading::Clear => continue,
                PrismReading::Steel(d) => d.min(reach).max(f64::MIN_POSITIVE),
                PrismReading::Unread => reach,
            };
            worst = Some(worst.map_or(d, |w| w.max(d)));
        }
    }
    worst
}

/// One fixture's depth in `body`: the box flags, the mesh judges what it
/// flagged (#1530). A coarse-fidelity fixture (#864) or a mesh that
/// refuses keeps the box verdict, logged.
fn fixture_depth(world: &World, fname: &str, fe: &Entity, body: &Body) -> Option<f64> {
    let a = model::entity_world_aabb(world, fe)?;
    let b = FixtureBox { min: a.min, max: a.max };
    let boxed = penetration(body, &b)?;
    if crate::model_fidelity::chord_of(world, fe).is_some() {
        return Some(boxed);
    }
    let Some(tris) = model::world_facets(world, fe) else { return Some(boxed) };
    match FixtureMesh::build(&tris) {
        Ok(mesh) => {
            let mut budget = MESH_VISIT_BUDGET;
            let d = mesh_penetration(body, &b, &mesh, &mut budget);
            if budget == 0 {
                tracing::info!(fixture = %fname, "carried-stock hold partly judged by the bounding box: the mesh visit budget ran out");
            }
            d
        }
        Err(why) => {
            tracing::info!(fixture = %fname, "carried-stock hold judged by the bounding box: {why}");
            Some(boxed)
        }
    }
}

/// The gate's carried-workholding clause: per setup entered with a
/// carried body, one problem per tagged fixture standing in it. The
/// setup's first op in program order reads the chain (every op of a
/// setup is entered with the same body).
#[must_use]
pub fn carried_fixture_problems(world: &World) -> Vec<String> {
    let mut out = Vec::new();
    let mut seen: Vec<Option<String>> = Vec::new();
    for (name, e) in crate::setup::operation_order(world) {
        let Some(op) = model::operation_of(world, &e) else { continue };
        let setup = crate::setup::effective(world, op.setup.as_deref());
        if seen.contains(&setup) {
            continue;
        }
        seen.push(setup.clone());
        let fixtures = crate::fixture::tagged(world, setup.as_deref());
        if fixtures.is_empty() {
            continue;
        }
        let tool_r = model::find_by_name(world, &op.tool)
            .and_then(|t| model::tool_of(world, &t))
            .map_or(0.0, |t| t.diameter * 0.5);
        // A refused carry is refused by every op's own generation.
        let Ok(Some((origin, body))) = crate::rest::entered_body(world, &name, setup.as_deref(), tool_r) else {
            continue;
        };
        let to = setup.as_deref().unwrap_or_default();
        for (fname, fe) in fixtures {
            if let Some(d) = fixture_depth(world, &fname, &fe, &body) {
                out.push(format!(
                    "setup '{to}': fixture '{fname}' {FIXTURE_IN_CARRIED} setup '{origin}' ({d:.2} mm deep) - \
                     the part that setup leaves cannot be held as modelled; move the fixture or re-check the hold"
                ));
            }
        }
    }
    out
}
