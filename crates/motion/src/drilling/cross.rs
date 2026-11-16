//! Radial (cross) drilling on the turned solid (M5.2 F2): the live
//! tooling's cycle, plunged from a turret station along X or Y
//! toward the spindle axis. The moves state their tool pose through
//! `cl`'s existing axis machinery (the multiaxis contour law), so
//! verification reads the path's own truth - and the truth is a
//! horizontal chain the vertical height field cannot judge: the
//! kernel's caller verifies it in `Mode::Radial`, whose doctrine
//! (stands (a),(b),(c),(c3) down to the construction law, reads
//! (d)'s rapids as a solid against the raw stock - #79 - and keeps
//! the fixture laws) is stated where the mode lives. There is no
//! canned `Cycle` record for a radial hole yet: the chain is the
//! single truth until the post learns G8x-style radial vocabulary.
//!
//! Provenance: cycle semantics (drill, chip-break peck, deep full-
//! retract march, through-hole exit allowance) follow the public
//! RS-274 / ISO 6983 vocabulary, shared with `cycle.rs`; the
//! traverse law - every rapid between holes running OUTSIDE the
//! stock's radius by at least the cutter's own rim, and a station
//! change inflating the hop plane so the chord's closest approach
//! (never merely the endpoints) clears the bar - is ours, from the
//! law that a bar's face is curved and a chord over it dives into
//! steel; stage G's opposite-station travel - retract axially,
//! index, swing above the bar's end face - follows the same law
//! pushed to the one pair of stations no hop radius can save.

use super::cylinder::coord;
use super::hole::{auto_kind, CrossHole, CycleChoice, CHIP_BREAK, SPOT_DEPTH, THROUGH_CLEAR};

use crate::cl::{lattice, CycleKind, MoveKind, PathBuilder, Toolpath};
use crate::geometry::Point3;
use crate::machining2d::GenError;
use crate::tools::Tool;

/// Everything the radial generator needs. The heights are
/// STANDOFFS above the mouth (a curved face has no one Z plane to
/// name), except `stock_r`: the stock's own maximum radial extent,
/// which is what makes a traverse safe between holes that open at
/// different places on the cylinder.
pub struct CrossParams<'a> {
    /// Radial clearance above the stock: where the tool stands
    /// between holes and at the path's ends.
    pub clearance_d: f64,
    /// Feed entry: this far out along the plunge axis from the
    /// hole's own mouth.
    pub r_d: f64,
    /// Retract plane between holes (a standoff); `None` retracts
    /// to the clearance plane.
    pub hop_d: Option<f64>,
    /// Depth override: an absolute coordinate plane on the plunge
    /// axis (the `bottom_z` mirror); `None` uses each hole's floor
    /// (plus the exit allowance past a through hole's far wall).
    pub bottom_d: Option<f64>,
    pub cycle: CycleChoice,
    /// Depth per peck; `<= 0` lets the Auto law pick.
    pub peck: f64,
    pub dwell: f64,
    pub feed: f64,
    pub plunge_feed: f64,
    /// Max radial extent of the stock (bar radius for a centered
    /// turned body; the mesh's max of x,y over vertices).
    pub stock_r: f64,
    /// The stock's Z top (the mesh's max of z over vertices). Known,
    /// it lets opposite stations travel by the axial DETOUR (M5.2
    /// G): lift on one station's column, swing above the end face,
    /// descend on the other's. Unknown (`None`), a 180° pair refuses
    /// as it did in F2 - no detour can be proven clear of a bar
    /// whose height is a guess.
    pub stock_z_max: Option<f64>,
    pub tool: &'a Tool,
    pub holes: &'a [CrossHole],
}

/// The position at signed travel `d` from a hole's mouth: `d > 0`
/// stands outward (air), `d < 0` is depth - the mouth convention
/// [`CrossHole`] carries.
fn along(h: &CrossHole, d: f64) -> Point3 {
    Point3::new(
        h.origin.x - h.dir.x * d,
        h.origin.y - h.dir.y * d,
        h.origin.z - h.dir.z * d,
    )
}

/// Reuse the cursor for roundoff-only travel, but still let the builder
/// record a changed station pose. Exact float equality otherwise leaves
/// an extra rapid when a measured bar radius differs by a few ULPs (#151).
/// This is local to radial rapids, not a change to the CL builder's law.
fn rapid(b: &mut PathBuilder, to: Point3) {
    let delta = to - b.at();
    let to = if delta.dot(delta) <= 1e-18 { b.at() } else { to };
    b.push(MoveKind::Rapid, to, None);
}

/// The dominant axis of an axis-aligned unit plunge direction.
fn plunge_axis(dir: Point3) -> Option<usize> {
    let mut hit = None;
    for i in 0..3 {
        let a = coord(dir, i).abs();
        if (a - 1.0).abs() <= 1e-9 {
            if hit.is_some() {
                return None; // two unit components: not a coordinate axis
            }
            hit = Some(i);
        } else if a > 1e-9 {
            return None; // an off-axis component: not axis-aligned
        }
    }
    hit
}

/// Machine a list of cross-drilled holes: one pose-tagged chain,
/// every rapid running clear of the stock's radius, every cut
/// along the hole's own axis. Nonsense in, `GenError` out; a
/// silent wrong answer never leaves this function (Prime
/// Directive - a radial path that cannot be proven clear refuses
/// by name instead).
pub fn cross_drill(p: &CrossParams) -> Result<Toolpath, GenError> {
    let d = p.tool.diameter;
    if !(d.is_finite() && d > 0.0)
        || !p.clearance_d.is_finite()
        || p.clearance_d < 0.0
        || !p.r_d.is_finite()
        || p.r_d <= 0.0
        || !p.stock_r.is_finite()
        || p.stock_r <= 0.0
    {
        return Err(GenError::Invalid("cross-drill tool or heights not usable".into()));
    }
    if p.holes.is_empty() {
        return Err(GenError::Empty("no cross holes to drill".into()));
    }
    if !p.holes.iter().all(|h| h.is_usable()) {
        return Err(GenError::Invalid("malformed cross hole list".into()));
    }
    for h in p.holes {
        let at = |msg: String| GenError::invalid_at(msg, h.origin); // #677: marks the mouth
        match plunge_axis(h.dir) {
            None => {
                return Err(at(format!(
                    "cross hole at ({}, {}, {}) plunges along ({}, {}, {}): only axis-aligned live tooling (M5.2 v1)",
                    h.origin.x, h.origin.y, h.origin.z, h.dir.x, h.dir.y, h.dir.z
                )));
            }
            Some(2) => {
                return Err(at(format!(
                    "cross hole at ({}, {}, {}) plunges along Z: this cycle plunges radially - the mill-side drill owns Z",
                    h.origin.x, h.origin.y, h.origin.z
                )));
            }
            _ => {}
        }
    }
    if !(p.feed.is_finite() && p.plunge_feed.is_finite() && p.dwell.is_finite() && p.dwell >= 0.0) {
        return Err(GenError::Invalid("cycle parameters not usable".into()));
    }
    if p.stock_z_max.is_some_and(|z| !z.is_finite()) {
        return Err(GenError::Invalid("cross-drill stock Z top not usable".into()));
    }
    if matches!(p.cycle, CycleChoice::Tap | CycleChoice::FloatingTap | CycleChoice::Bore | CycleChoice::BackBore | CycleChoice::Ream) {
        return Err(GenError::Invalid(
            "radial tapping, boring and reaming are not this cycle's word (M5.2 v1)".into(),
        ));
    }

    // Snap once; every move reads these numbers.
    let hs: Vec<CrossHole> = p
        .holes
        .iter()
        .map(|h| CrossHole {
            origin: Point3::new(lattice(h.origin.x), lattice(h.origin.y), lattice(h.origin.z)),
            dir: h.dir,
            radius: lattice(h.radius),
            entry_d: 0.0,
            floor_d: lattice(h.floor_d),
            through: h.through,
        })
        .collect();

    // Absolute standoffs on the plunge coordinates: the traverse
    // planes sit OUTSIDE the stock's radius BY AT LEAST THE TOOL
    // RADIUS - a station's hop point is clear only if the cutter's
    // own rim, trailing the tip toward the station, misses the
    // bar's face too - and a station CHANGE inflates that plane
    // further (see `traverse_s`), so no chord between two hop
    // points can dive into the bar's face.
    let margin = p.r_d.max(0.5 * d + 0.1);
    let end_s = p.stock_r + p.clearance_d.max(margin);
    let hop_s = p.stock_r + p.hop_d.unwrap_or(p.clearance_d).max(margin);
    let mut b = PathBuilder::new(along(&hs[0], start_d(&hs[0], end_s)));
    for (i, h) in hs.iter().enumerate() {
        if i > 0 {
            // The travel rapid rides the OLD station's pose: the
            // tool swings to the new station after it arrives -
            // except on the detour, where the index happens above
            // the bar's end face (stage G).
            match traverse_s(&hs[i - 1], h, hop_s) {
                Some(s) => rapid(&mut b, along(h, start_d(h, s))),
                None => {
                    let z_top = p.stock_z_max.map(|z| z + p.clearance_d.max(margin));
                    let Some(z_top) = z_top else {
                        return Err(GenError::Invalid(format!(
                            "cross holes {} and {} stand on opposite stations: a straight traverse would cross the spindle, and the stock Z top is unknown so the over-the-end-face detour cannot be proven clear (M5.2 G)",
                            i - 1,
                            i
                        )));
                    };
                    detour(&mut b, &hs[i - 1], h, hop_s, z_top);
                }
            }
        }
        // The pose tag states the tool's axis from tip toward
        // shank (the CL law `moves.rs` records) - the live tool
        // plunges ALONG `dir`, so its shank stands BEHIND the tip,
        // on the negated side. The station hop's rapids ride the
        // previous hole's pose, so this sign is what makes the
        // traverse leave the way the tool came in.
        b.set_axis(Some([-h.dir.x, -h.dir.y, -h.dir.z]));
        rapid(&mut b, along(h, p.r_d));
        let floor_bottom = h.floor_d - if h.through { THROUGH_CLEAR } else { 0.0 };
        // In d-space the entry plane is d = 0 (the mouth; `r_d` is
        // only where the rapids stop outside), so the dimple law
        // reads -SPOT_DEPTH, and a bottom at or above 0 cuts air.
        let bottom = match p.bottom_d {
            Some(s) => start_d(h, s),
            None if matches!(p.cycle, CycleChoice::Spot) => (-SPOT_DEPTH).max(h.floor_d),
            None => floor_bottom,
        };
        if bottom >= 0.0 {
            return Err(GenError::invalid_at(
                format!("cross hole at ({}, {}, {}) bottoms at {bottom}, above its entry", h.origin.x, h.origin.y, h.origin.z),
                h.origin,
            ));
        }
        let depth = p.r_d - bottom;
        let kind = match p.cycle {
            CycleChoice::Auto => auto_kind(p.tool.kind, depth, d),
            CycleChoice::Spot => CycleKind::Spot,
            CycleChoice::Drill => CycleKind::Drill,
            CycleChoice::Peck => CycleKind::Peck,
            _ => unreachable!("rejected by the v1 cycle door above"),
        };
        if matches!(kind, CycleKind::Tap | CycleKind::FloatingTap | CycleKind::Bore | CycleKind::BackBore | CycleKind::Ream) {
            return Err(GenError::Invalid(format!(
                "a {:?} cycle is not radial-capable (M5.2 v1)",
                kind
            )));
        }
        super::cycle::refuse_solid_cycle(p.tool, kind, h.origin)?;
        match kind {
            CycleKind::Spot | CycleKind::Drill => {
                b.push(MoveKind::Plunge, along(h, bottom), Some(p.plunge_feed));
                rapid(&mut b, along(h, p.r_d));
            }
            CycleKind::Peck | CycleKind::Deep => {
                let auto = if kind == CycleKind::Peck { 3.0 * d } else { d };
                let step = if p.peck > 0.0 { p.peck } else { auto }.max(0.25 * d);
                if !(step.is_finite() && step > 0.0) || depth / step > 4096.0 {
                    return Err(GenError::Invalid("peck plan unusable".into()));
                }
                let mut t = (p.r_d - step).max(bottom);
                loop {
                    b.push(MoveKind::Plunge, along(h, t), Some(p.plunge_feed));
                    if t <= bottom {
                        break;
                    }
                    let up = if kind == CycleKind::Peck {
                        (t + CHIP_BREAK).min(p.r_d)
                    } else {
                        p.r_d
                    };
                    rapid(&mut b, along(h, up));
                    t = (t - step).max(bottom);
                }
                rapid(&mut b, along(h, p.r_d));
            }
            _ => {}
        }
        if i + 1 < hs.len() {
            // Lift to the (station-pair inflated) hop plane before
            // the turret indexes to the next hole. An opposite pair
            // has no inflation factor - stand on the plain plane;
            // the next round's traverse branch takes the axial
            // detour, or refuses if the bar's Z top is unknown.
            let s = traverse_s(h, &hs[i + 1], hop_s).unwrap_or(hop_s);
            rapid(&mut b, along(h, start_d(h, s)));
        }
    }
    let last = hs[hs.len() - 1];
    rapid(&mut b, along(&last, start_d(&last, end_s)));
    let t = b.finish();
    if !t.is_chained() || !t.all_finite() {
        return Err(GenError::Invalid("cross-drill chain self-check failed".into()));
    }
    Ok(t)
}

/// The hop radius for a traverse from one station's hole to
/// another's: the smallest plane `s >= hop_s` whose chord between the
/// two hop points (`along(h, start_d(h, s))`, one on each hole's own
/// column) never comes closer than `hop_s` to the spindle axis. For
/// mouths ON their stations' centre lines a quarter turn apart this is
/// the closed form `hop_s * sqrt(2 / (1 + cos dtheta))` (the chord's
/// closest approach is `s * cos(dtheta/2)`); a mouth OFF its centre
/// line (#81) moves the columns, so the chord is measured, not
/// assumed: the plane grows from that closed form until the chord
/// clears, then bisects back to the smallest clear plane. Same
/// station: the chord is the slide between two mouths on one column,
/// clear at `hop_s`. Opposite stations: no finite plane saves the
/// chord - it crosses the spindle at ANY radius - so `None` says
/// "take the detour". `from.dot(to)` on unit XY plunges is cos(dtheta).
fn traverse_s(from: &CrossHole, to: &CrossHole, hop_s: f64) -> Option<f64> {
    let dot = from.dir.x * to.dir.x + from.dir.y * to.dir.y;
    if dot < -0.9 {
        return None;
    }
    let guess = hop_s * (2.0 / (1.0 + dot)).sqrt();
    let clear = |s: f64| chord_reach(from, to, s) >= hop_s - 1e-9;
    if clear(guess) {
        // Never below the closed form: on-centre holes keep their
        // proven plane exactly (no drift in existing chains).
        return Some(guess);
    }
    let mut lo = guess;
    let mut hi = guess;
    for _ in 0..64 {
        hi *= 2.0;
        if clear(hi) {
            break;
        }
        lo = hi;
    }
    if !clear(hi) {
        return None; // no plane clears it: the detour (or its refusal) speaks
    }
    for _ in 0..60 {
        let mid = 0.5 * (lo + hi);
        if clear(mid) {
            hi = mid;
        } else {
            lo = mid;
        }
    }
    Some(hi)
}

/// The closest the straight chord between the two holes' hop points at
/// plane `s` comes to the spindle (the world Z axis), in XY - the
/// segment-to-origin distance, endpoints included.
fn chord_reach(from: &CrossHole, to: &CrossHole, s: f64) -> f64 {
    let a = along(from, start_d(from, s));
    let b = along(to, start_d(to, s));
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    let l2 = dx * dx + dy * dy;
    let t = if l2 > 0.0 { (-(a.x * dx + a.y * dy) / l2).clamp(0.0, 1.0) } else { 0.0 };
    let (px, py) = (a.x + t * dx, a.y + t * dy);
    let r = (px * px + py * py).sqrt();
    if r.is_finite() { r } else { 0.0 }
}

/// The axial detour between opposite stations (M5.2 G): lift on the
/// old column (the caller has the tool standing there, outside
/// `hop_s`), index the turret in the clear, rapid above the bar's
/// END FACE - a plane no radial chord could reach - then descend on
/// the new column. Every leg is provably stock-free: the columns
/// stand outside the bar's radius by the traverse law, and the
/// swing plane clears the top by `clearance.max(margin)` - the
/// margin half is what keeps the horizontal body's own rim
/// (`d/2` below the tip line) off the end face.
fn detour(b: &mut PathBuilder, prev: &CrossHole, h: &CrossHole, hop_s: f64, z_top: f64) {
    let p0 = along(prev, start_d(prev, hop_s));
    rapid(b, Point3::new(p0.x, p0.y, z_top));
    // The index: the pose flips to the NEW plunge, shank behind
    // the tip, while the tool hangs above the bar.
    b.set_axis(Some([-h.dir.x, -h.dir.y, -h.dir.z]));
    let p1 = along(h, start_d(h, hop_s));
    rapid(b, Point3::new(p1.x, p1.y, z_top));
    rapid(b, p1);
}

/// The travel `d` at which the hole's point reaches radial distance
/// `s` from the spindle axis, standing on the mouth's OWN side of
/// it - a station's hop plane is its column, never the opposite
/// one (the old signed-coordinate reading parked a negative-side
/// station on the far column, straight through the spindle). `dir`
/// is an axis-aligned unit, so this is exact, not projected; the
/// side is the mouth's coordinate sign (a radial mouth on a centered
/// stock never sits at 0; if it did, the station stands behind the
/// tip, `-dir`).
fn start_d(h: &CrossHole, s: f64) -> f64 {
    let a = plunge_axis(h.dir).unwrap_or(2);
    let dcomp = coord(h.dir, a);
    let oc = coord(h.origin, a);
    let side = if oc < 0.0 {
        -1.0
    } else if oc > 0.0 {
        1.0
    } else {
        -dcomp.signum()
    };
    (oc - side * s) / dcomp
}
