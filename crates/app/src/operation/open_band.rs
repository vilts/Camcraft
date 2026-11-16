//! The open-edge band (#1735): two picked open edges of a part are the
//! walls of a region with open ends, and the stock outline closes it.
//! The pocket family (2D Pocket, 2D Adaptive, 2D Offset) takes such a
//! selection as its boundary instead of refusing it as "an open run".
//!
//! The law, exactly. Picked open runs that share an end are welded
//! into one wall; exactly two walls A and B must remain, level and at
//! one height. Each wall is run on straight along its end tangents.
//! With S the stock outline grown by the extension `e` (sharp corners):
//!
//!   band(e) = (side of A that holds B) ∩ (side of B that holds A) ∩ S
//!
//! `band(0)` is the selection's region; the pocket region law then
//! takes the part's standing material out of it at every plane, as it
//! does for a closed pick. `band(e)` is the lane the area law's
//! extension is clipped to (`boundary::resolve`), so "Extend past"
//! moves the open ends out of the stock along the edges and never
//! lets the tool round the end of a wall. Every selection the law
//! cannot read one way - no stock, a wall not level, walls at two
//! heights, walls that do not face each other, a band the stock
//! closes in several pieces - is refused by name, never guessed.
//!
//! Provenance: our own design notes on the open pocket / open slot of
//! general 2.5D machining practice (walls bound the cut, the stock
//! bounds the open ends); the half-plane split is `sides_of_chain`.
//! No vendor material consulted.

use super::boundary_turn::sides_of_chain;
use super::*;

#[path = "open_slot.rs"]
mod open_slot;

/// Past this angle between the two walls' chords they are not "facing
/// each other": the band would be a wedge, and which wedge is a guess.
pub(super) const FACING_MAX_DEG: f64 = 45.0;
/// Height spread, mm, inside which a wall is level and two walls share
/// a height (the model's own tessellation noise is far below it).
const LEVEL_TOL: f64 = 1e-3;
/// Area, mm², under which a band is nothing.
const AREA_MIN: f64 = 1e-4;

/// A band's two walls, or a blind slot's single U-shaped wall, in XY.
type Walls = (Vec<Point2>, Option<Vec<Point2>>);

fn refuse(msg: String) -> GenError {
    GenError::Invalid(msg)
}

/// The refusal of record for ONE open run in a pocket-family selection
/// (the text every kind has always given; a band needs two).
pub(super) fn mid_edge(label: &str, params: &OpParams) -> GenError {
    refuse(format!(
        "{label} is an open run - a {} cannot end mid-edge; select a closed chain - Ctrl+click the remaining edges, or Shift+click to take the whole loop",
        params.kind().info().label
    ))
}

fn band_kind(params: &OpParams) -> bool {
    matches!(params, OpParams::Pocket { .. } | OpParams::Adaptive2D { .. } | OpParams::Offset2D { .. })
}

/// The setup's stock outline in world XY.
fn stock_outline(world: &World, setup: Option<&str>) -> Option<Region> {
    let name = crate::setup::stock_entity_name(world, setup);
    model::find_by_name(world, &name)
        .and_then(|e| model::world_facets(world, &e))
        .and_then(|f| slice::project(&f, FOOTPRINT_SAMPLES).ok())
        .filter(|r| !r.is_empty())
}

/// The two walls a pocket-family selection spells, or `None` where the
/// band law does not apply (another kind, a tilted plane, fewer than
/// two open runs left by the stitcher - the old refusals stand).
fn walls(
    world: &World,
    setup: Option<&str>,
    items: &[GeoSel],
    params: &OpParams,
    frame: Option<&crate::cl::TiltedFrame>,
    stitched: &HashMap<usize, usize>,
) -> Result<Option<Walls>, GenError> {
    if !band_kind(params) || frame.is_some() {
        return Ok(None);
    }
    let runs: Vec<Run> = gather_runs(world, setup, items, None).into_iter().filter(|r| !run_used(r, stitched)).collect();
    if runs.is_empty() || (runs.len() == 1 && runs[0].pts.len() < 3) {
        return Ok(None);
    }
    let kind = params.kind().info().label;
    let stock_name = crate::setup::stock_entity_name(world, setup);
    if let Some(r) = runs.iter().find(|r| r.entity == stock_name) {
        return Err(refuse(format!(
            "an open edge of the stock '{}' is picked - an open-edge band needs two wall edges of the part; pick the part's edges",
            r.entity
        )));
    }
    if runs.iter().map(|r| r.items.len()).sum::<usize>() != items.len() {
        return Err(refuse(format!(
            "the selection mixes {} open edges with closed boundaries - a {kind} takes closed boundaries, or two opposite open edges alone; remove one kind",
            runs.len()
        )));
    }
    let unit = crate::units::of(world);
    let z = |v: f64| crate::units::len(v, unit, 3);
    let mut level: Option<f64> = None;
    for r in &runs {
        let lo = r.zs.iter().copied().fold(f64::INFINITY, f64::min);
        let hi = r.zs.iter().copied().fold(f64::NEG_INFINITY, f64::max);
        if !(hi - lo).is_finite() || hi - lo > LEVEL_TOL {
            return Err(refuse(format!(
                "a picked open edge of '{}' is not level (it runs from Z {} to Z {}) - an open-edge band needs two level edges at one height",
                r.entity,
                z(lo),
                z(hi)
            )));
        }
        match level {
            Some(l) if (l - lo).abs() > LEVEL_TOL => {
                return Err(refuse(format!(
                    "the picked open edges are at different heights (Z {} and Z {}) - an open-edge band needs both walls at one height; pick the two top edges of the slot",
                    z(l),
                    z(lo)
                )));
            }
            _ => level = Some(lo),
        }
    }
    let mut welded = weld(runs.into_iter().map(|r| r.pts).collect())?;
    match (welded.pop(), welded.pop(), welded.len()) {
        (Some(b), Some(a), 0) => Ok(Some((a, Some(b)))),
        (Some(a), None, _) => Ok(Some((a, None))),
        (_, _, more) => Err(refuse(format!(
            "the picked open edges spell {} separate runs - an open-edge band has exactly two walls; pick two edges that face each other",
            more + 2
        ))),
    }
}

/// Weld runs that share an end into single walls, in pick order.
fn weld(runs: Vec<Vec<Point2>>) -> Result<Vec<Vec<Point2>>, GenError> {
    let mut ends: HashMap<(i64, i64), usize> = HashMap::new();
    for r in &runs {
        for p in [r.first(), r.last()].into_iter().flatten() {
            *ends.entry(qkey(p)).or_insert(0) += 1;
        }
    }
    if ends.values().any(|n| *n > 2) {
        return Err(refuse(
            "three picked open edges meet at one point - a band's wall is one run; remove the stray edge".into(),
        ));
    }
    let key = |p: Option<&Point2>| p.map(qkey);
    let mut out: Vec<Vec<Point2>> = Vec::new();
    for mut cur in runs {
        loop {
            let (cf, cl) = (key(cur.first()), key(cur.last()));
            let Some(i) = out.iter().position(|o| {
                let (of, ol) = (key(o.first()), key(o.last()));
                of == cf || of == cl || ol == cf || ol == cl
            }) else {
                break;
            };
            let mut o = out.remove(i);
            // Orient both so `o` ends where `cur` starts.
            if key(o.first()) == cf || key(o.first()) == cl {
                o.reverse();
            }
            if key(o.last()) == key(cur.last()) {
                cur.reverse();
            }
            o.extend(cur.into_iter().skip(1));
            cur = o;
            if cur.len() > 2 && key(cur.first()) == key(cur.last()) {
                return Err(refuse(
                    "the picked open edges join into a loop that is no closed chain of the model - pick two separate edges that face each other".into(),
                ));
            }
        }
        out.push(cur);
    }
    Ok(out)
}

/// A wall's chord: start, unit direction, length.
fn chord(p: &[Point2]) -> Result<(Point2, (f64, f64), f64), GenError> {
    let (Some(a), Some(b)) = (p.first(), p.last()) else {
        return Err(refuse("a picked open edge holds no points - re-pick it".into()));
    };
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    let len = dx.hypot(dy);
    if len.is_nan() || len <= 1e-6 {
        return Err(refuse(
            "a picked open edge ends where it starts - it gives the band no direction; pick a straighter edge".into(),
        ));
    }
    Ok((*a, (dx / len, dy / len), len))
}

/// `p` run on straight past both ends by `far`.
fn run_on(p: &[Point2], far: f64) -> Vec<Point2> {
    let mut out = Vec::with_capacity(p.len() + 2);
    let ray = |a: &Point2, b: &Point2| {
        let (dx, dy) = (a.x - b.x, a.y - b.y);
        let n = dx.hypot(dy).max(1e-12);
        Point2::new(a.x + dx / n * far, a.y + dy / n * far)
    };
    if let [a, b, ..] = p {
        out.push(ray(a, b));
    }
    out.extend(p.iter().copied());
    if let [.., a, b] = p {
        out.push(ray(b, a));
    }
    out
}

/// Where segments `a0-a1` and `b0-b1` cross, if they do.
fn crossing(a0: Point2, a1: Point2, b0: Point2, b1: Point2) -> Option<Point2> {
    let (rx, ry, sx, sy) = (a1.x - a0.x, a1.y - a0.y, b1.x - b0.x, b1.y - b0.y);
    let den = rx * sy - ry * sx;
    if den.abs() < 1e-12 {
        return None;
    }
    let (qx, qy) = (b0.x - a0.x, b0.y - a0.y);
    let (t, u) = ((qx * sy - qy * sx) / den, (qx * ry - qy * rx) / den);
    ((0.0..=1.0).contains(&t) && (0.0..=1.0).contains(&u)).then(|| Point2::new(a0.x + t * rx, a0.y + t * ry))
}

/// Whether a point of `pts` lies on the chain `line` (within 1e-6 mm).
fn touches(pts: &[Point2], line: &[Point2]) -> bool {
    pts.iter().any(|p| {
        line.windows(2).any(|s| {
            let (dx, dy) = (s[1].x - s[0].x, s[1].y - s[0].y);
            let t = (((p.x - s[0].x) * dx + (p.y - s[0].y) * dy) / (dx * dx + dy * dy).max(1e-24)).clamp(0.0, 1.0);
            (p.x - s[0].x - t * dx).hypot(p.y - s[0].y - t * dy) < 1e-6
        })
    })
}

/// The side of `wall`'s run-on line that holds every point of `other`,
/// inside `world_box`.
fn side_holding(wall: &[Point2], other: &[Point2], world_box: &Region) -> Result<Region, GenError> {
    let (left, right, _) = sides_of_chain(wall, world_box)?;
    let all_in = |r: &Region| other.iter().all(|p| r.contains(*p));
    match (all_in(&left), all_in(&right)) {
        (true, false) => Ok(left),
        (false, true) => Ok(right),
        _ => Err(refuse(
            "the two picked open edges cross or touch once run on straight - no band lies between them; pick two opposite edges".into(),
        )),
    }
}

/// The band between walls `a` and `b`, closed by `stock` grown by
/// `extend` (the module's law). World-free.
pub(super) fn band(a: &[Point2], b: &[Point2], stock: &Region, extend: f64) -> Result<Region, GenError> {
    let (ca, cb) = (chord(a)?, chord(b)?);
    let (cross, dot) = (ca.1.0 * cb.1.1 - ca.1.1 * cb.1.0, ca.1.0 * cb.1.0 + ca.1.1 * cb.1.1);
    let angle = cross.abs().atan2(dot.abs()).to_degrees();
    if angle.is_nan() || angle > FACING_MAX_DEG {
        return Err(refuse(format!(
            "the two picked open edges do not face each other (they stand at {angle:.0} degrees to each other; the limit is {FACING_MAX_DEG:.0}) - pick two opposite edges"
        )));
    }
    // Each wall must run alongside the other for some of its length.
    let alongside = |c: &(Point2, (f64, f64), f64), other: &[Point2]| {
        let t = |p: &Point2| (p.x - c.0.x) * c.1.0 + (p.y - c.0.y) * c.1.1;
        let lo = other.iter().map(t).fold(f64::INFINITY, f64::min);
        let hi = other.iter().map(t).fold(f64::NEG_INFINITY, f64::max);
        hi.min(c.2) - lo.max(0.0) > 1e-6
    };
    if !(alongside(&ca, b) && alongside(&cb, a)) {
        return Err(refuse(
            "the two picked open edges do not face each other (neither runs alongside the other) - pick two opposite edges".into(),
        ));
    }
    let reach = if extend > 0.0 { stock.offset_sharp(extend)? } else { stock.clone() };
    let Some((mut lo, mut hi)) = reach.bounds() else {
        return Err(GenError::Empty("the stock outline is empty".into()));
    };
    for p in a.iter().chain(b) {
        lo = Point2::new(lo.x.min(p.x), lo.y.min(p.y));
        hi = Point2::new(hi.x.max(p.x), hi.y.max(p.y));
    }
    let pad = 10.0;
    let world_box = Region::new(vec![Loop::new(vec![
        Point2::new(lo.x - pad, lo.y - pad),
        Point2::new(hi.x + pad, lo.y - pad),
        Point2::new(hi.x + pad, hi.y + pad),
        Point2::new(lo.x - pad, hi.y + pad),
    ])?])?;
    // Walls that run together inside the stock leave a wedge with one
    // open end: not the band the picks ask for.
    let far = 4.0 * ((hi.x - lo.x).hypot(hi.y - lo.y) + pad);
    let (ea, eb) = (run_on(a, far), run_on(b, far));
    for sa in ea.windows(2) {
        for sb in eb.windows(2) {
            if crossing(sa[0], sa[1], sb[0], sb[1]).is_some_and(|x| reach.contains(x)) {
                return Err(refuse(
                    "the two picked open edges run together inside the stock - the band between them has one open end only; pick two edges that stay apart up to the stock edge".into(),
                ));
            }
        }
    }
    // A wall with a point on the other's line has no side of it: the
    // side test would take a whole half of the stock for the band.
    if touches(a, &eb) || touches(b, &ea) {
        return Err(refuse(
            "the two picked open edges cross or touch once run on straight - no band lies between them; pick two opposite edges".into(),
        ));
    }
    let between = side_holding(a, b, &world_box)?.intersect(&side_holding(b, a, &world_box)?)?;
    let band = between.intersect(&reach)?;
    let mid = |c: &(Point2, (f64, f64), f64)| (c.0.x + c.1.0 * c.2 * 0.5, c.0.y + c.1.1 * c.2 * 0.5);
    let ((ax, ay), (bx, by)) = (mid(&ca), mid(&cb));
    if band.area().abs() < AREA_MIN || !band.contains(Point2::new((ax + bx) * 0.5, (ay + by) * 0.5)) {
        return Err(refuse(
            "the stock does not cover the space between the two picked open edges - nothing closes the band; check the stock, or close the pick into a chain".into(),
        ));
    }
    let pieces = band.loops().iter().filter(|l| l.is_ccw()).count();
    if pieces != 1 {
        return Err(refuse(format!(
            "the stock outline closes the band between the two picked open edges in {pieces} separate pieces - there is more than one way to close it; pick a closed chain around the area"
        )));
    }
    Ok(band)
}

fn band_of(world: &World, setup: Option<&str>, w: &Walls, extend: f64) -> Result<Region, GenError> {
    let Some(stock) = stock_outline(world, setup) else {
        return Err(refuse(
            "open slot edges are closed by the stock outline, and this setup has no stock - set up stock, or close the pick into a chain".into(),
        ));
    };
    match &w.1 {
        Some(b) => band(&w.0, b, &stock, extend),
        None => open_slot::region(&w.0, &stock, extend),
    }
}

/// The selection walk's door: when `items` spell a band, its loops go
/// into `loops` and the answer is `true` (the walk then skips the open
/// runs); `false` leaves every old law standing.
#[allow(clippy::too_many_arguments)]
pub(super) fn of_selection(
    world: &World,
    setup: Option<&str>,
    items: &[GeoSel],
    params: &OpParams,
    frame: Option<&crate::cl::TiltedFrame>,
    stitched: &HashMap<usize, usize>,
    loops: &mut Vec<Loop>,
    notes: Option<&mut Vec<String>>,
) -> Result<bool, GenError> {
    let Some(w) = walls(world, setup, items, params, frame, stitched)? else {
        return Ok(false);
    };
    loops.extend(band_of(world, setup, &w, 0.0)?.loops().iter().cloned());
    if let Some(n) = notes {
        let note = if w.1.is_some() {
            "2 open edges closed by the stock outline into a band"
        } else {
            "1 U-shaped open run closed by the stock outline into a blind slot"
        };
        n.push(note.into());
    }
    Ok(true)
}

/// The area law's lane for `op`: `band(extension)` when its selection
/// spells a band, else `None`. A band keeps the tool between its walls,
/// so a containment that lets the tool past the boundary is refused.
pub(super) fn lane(world: &World, op: &Operation) -> Result<Option<Region>, GenError> {
    let SrcRef::Selection(items) = &op.src else {
        return Ok(None);
    };
    if !band_kind(&op.params) {
        return Ok(None);
    }
    let setup = op.setup.as_deref();
    let (_, stitched) = stitch_open_runs(world, setup, items, None)?;
    let Some(w) = walls(world, setup, items, &op.params, None, &stitched)? else {
        return Ok(None);
    };
    if matches!(op.area.containment, Some(Containment::On | Containment::Outside)) {
        return Err(refuse(
            "an open-edge band keeps the tool between its two edges - tool containment On / Outside does not apply to it; set containment to Inside and use the extension to pass the stock edge".into(),
        ));
    }
    band_of(world, setup, &w, op.area.extension.max(0.0)).map(Some)
}

#[cfg(test)]
#[path = "open_band_tests.rs"]
mod tests;
