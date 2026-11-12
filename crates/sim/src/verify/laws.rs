//! Split from `verify.rs` by M5.0.1; structure only.

use super::types::{FixturePart, Warning};

use crate::cl::{is_vertical, Move, MoveKind, Toolpath};
use crate::geo2d::{Point2, Region};
use crate::geometry::Point3;
pub use crate::tools::ToolGeom;
use camcraft_tools::holder_profile::HolderStack;
pub(super) fn xy(p3: &Point3) -> Point2 {
    Point2::new(p3.x, p3.y)
}

/// Squared XY distance from `p` to segment `a`-`b`.
pub(super) fn dist2_seg(p: Point2, a: Point2, b: Point2) -> f64 {
    let dx = b.x - a.x;
    let dy = b.y - a.y;
    let dd = dx * dx + dy * dy;
    if dd <= f64::EPSILON {
        let (ex, ey) = (p.x - a.x, p.y - a.y);
        return ex * ex + ey * ey;
    }
    let t = (((p.x - a.x) * dx + (p.y - a.y) * dy) / dd).clamp(0.0, 1.0);
    let (ex, ey) = (a.x + t * dx - p.x, a.y + t * dy - p.y);
    ex * ex + ey * ey
}

/// Squared XY distance between segments `a0`-`a1` and `b0`-`b1`: zero
/// where they cross, else the least endpoint-to-segment distance (the
/// closest pair of two non-crossing segments has an endpoint in it -
/// O'Rourke, "Computational Geometry in C").
pub(super) fn dist2_segs(a0: Point2, a1: Point2, b0: Point2, b1: Point2) -> f64 {
    if proper_cross(a0, a1, b0, b1) {
        return 0.0;
    }
    dist2_seg(a0, b0, b1).min(dist2_seg(a1, b0, b1)).min(dist2_seg(b0, a0, a1)).min(dist2_seg(b1, a0, a1))
}

pub(super) fn orient(a: Point2, b: Point2, c: Point2) -> f64 {
    (b.x - a.x) * (c.y - a.y) - (b.y - a.y) * (c.x - a.x)
}

/// Proper transversal crossing of two XY segments: each endpoint pair
/// strictly straddles the other's line. Shared or touching endpoints
/// give a zero orientation and are not crossings.
pub(super) fn proper_cross(a0: Point2, a1: Point2, b0: Point2, b1: Point2) -> bool {
    let (oa, ob) = (orient(a0, a1, b0), orient(a0, a1, b1));
    let (oc, od) = (orient(b0, b1, a0), orient(b0, b1, a1));
    oa != 0.0
        && ob != 0.0
        && oc != 0.0
        && od != 0.0
        && ((oa > 0.0) != (ob > 0.0))
        && ((oc > 0.0) != (od > 0.0))
}

/// XY samples of every boundary: loop vertices plus edge midpoints, so a
/// wall is represented by more than its corners, each nudged a hair
/// toward the loop's vertex mean. The nudge keeps membership off the
/// parity knife-edge: just inside an outer loop is region material, just
/// inside a hole loop is the island - the point a reach law must classify
/// unambiguously.
pub(super) fn boundary_samples(region: &Region) -> Vec<Point2> {
    const NUDGE: f64 = 0.05;
    let mut out = Vec::new();
    for l in region.loops() {
        let pts = l.points();
        if pts.is_empty() {
            continue;
        }
        let (mx, my) = pts.iter().fold((0.0, 0.0), |s, q| (s.0 + q.x, s.1 + q.y));
        let (mx, my) = (mx / pts.len() as f64, my / pts.len() as f64);
        for i in 0..pts.len() {
            let a = pts[i];
            let b = pts[(i + 1) % pts.len()];
            for q in [a, Point2::new((a.x + b.x) * 0.5, (a.y + b.y) * 0.5)] {
                let (dx, dy) = (mx - q.x, my - q.y);
                let d = (dx * dx + dy * dy).sqrt();
                out.push(if d > f64::EPSILON {
                    Point2::new(q.x + NUDGE * dx / d, q.y + NUDGE * dy / d)
                } else {
                    q
                });
            }
        }
    }
    out
}

/// XY points a radial law must hold at for one move: exactly the move's
/// `to` for a straight one - byte-for-byte the behavior every straight
/// path always had - and the arc with its chord joints for an arc, which
/// bound it within `chord_tol`: the laws' own radial tolerance is what
/// absorbs that sampling error, so a violation past tolerance is never
/// missed.
pub(super) fn law_points(m: &Move, chord_tol: f64) -> Vec<Point2> {
    match m.arc {
        None => vec![Point2::new(m.to.x, m.to.y)],
        Some(_) => {
            let cs = m.chords(chord_tol);
            let mut v = Vec::with_capacity(cs.len() + 1);
            if let Some(&(a, _)) = cs.first() {
                v.push(a);
            }
            v.extend(cs.into_iter().map(|(_, b)| b));
            v
        }
    }
}

/// Bands against fixtures (M3.2 stage B): the shank and holder cylinders,
/// tested against the fixture box list instead of rest material - a vise
/// jaw in a band's swept path is a crash exactly the way uncut stock in it
/// is. The cylinders are SWEPT along each move's xy path (a straight move
/// is one segment, an arc its chord segments), never sampled at law
/// points: an 8 mm jaw standing between two far endpoints is exactly the
/// thing this law exists for, and erring toward HIT means not sampling
/// past it. Each band is judged over its true z span along the move
/// (#425, [`super::stack_span`]): the shank band and holder band over
/// [min(from,to).z + above, max(from,to).z + above + band_len] - their
/// windows at both ends, min to max, so a ramp's high end is read too;
/// above the holder, the spindle housing band when the machine file
/// states its body geometry; below them all, the flute column at the
/// tool radius over [min(from,to).z, max(from,to).z + flute_len].
///
/// The law (c) annulus is empty over the flute because the flute cuts its
/// own kerf in STOCK - but a fixture standing in that window was never
/// meant to be cut, and nothing else looks at it. Z overlap is strict
/// beyond tolerance (the swept-cylinder rule, `fixture::band_hits_box`:
/// a jaw milled flush is contact without penetration, not a crash); a
/// band reaching within the radius of the box counts, XY touching
/// included. One report per move, first band struck, worst-case order
/// flute-free: shank, holder, then the spindle housing. Region-free by
/// construction - the post's live gate runs it against a cached path
/// without re-deriving any cut geometry (the M2.3 law: workholding is
/// checked here and now, never baked into a stamp). `holder` is what
/// stands above the shank (#368): one cylinder, or a body's bands.
pub fn bands_vs_fixtures_held(
    path: &Toolpath,
    tool: &ToolGeom,
    holder: &HolderStack,
    fixtures: &[crate::fixture::FixtureBox],
    spindle: Option<(f64, f64)>,
    chord_tol: f64,
) -> Vec<Warning> {
    let mut warns = Vec::new();
    if fixtures.is_empty() {
        return warns;
    }
    // The posed sweep (M13.8): a leaning move's real stack along its
    // real pose - [`super::posed_steel`]; it only ever ADDS a verdict.
    let posed = super::posed_steel::posed_hits(path, tool, holder, fixtures, spindle, chord_tol, false);
    // The spindle housing band rides on the holder's top: its bottom
    // offset is the holder's offset plus the holder length, so the stack
    // can never double-count a slice of air or gap over the tool. It is a
    // fixture law only - against stock the housing stands down by the kerf
    // reasoning that already excuses the holder band there.
    let spindle = spindle.map(|(sr, sl)| (FixturePart::Spindle, holder.top, sr, sl));
    // #852: the shank as swept - a drawn neck's bands, or the cylinder.
    let mut bands: Vec<_> = holder.shank_bands(tool).iter().map(|b| (FixturePart::Shank, b.above, b.r, b.len)).collect();
    bands.extend(holder.bands.iter().map(|b| (FixturePart::Holder, b.above, b.r, b.len)));
    for (i, m) in path.moves.iter().enumerate() {
        if m.kind == MoveKind::Rapid {
            continue;
        }
        let z_deep = m.from.z.min(m.to.z);
        // The move's trace as pieces: one for a straight, the chords
        // for an arc - what the cylinders actually sweep - each with
        // its true z span, floored at the move's deepest end (#425:
        // a ramp's high end lifts the whole stack with it).
        let segs = super::stack_span::pieces(m, chord_tol, z_deep);
        // Pose (M5.1 stage A): a leaning move turns each band from a
        // vertical column into a tilted cylinder. This law encloses
        // the lean in a vertical column that is never smaller: the
        // swept path shifts to the band's mid-height point on the
        // leaning axis, the radius gains the half-length's horizontal
        // spread, and the z planes gain the half-length's vertical
        // reach plus the disc's own radius. An ENCLOSING model can
        // over-report a graze; it cannot hide a crash - the tilted
        // truth (a swept tilted cylinder against a box) is worth
        // measuring exactly when stage B's generators start leaning
        // real stock paths. The prismatic branch is bit-identical to
        // the law's own history: same sums, same comparisons.
        let lean = m.axis.filter(|a| !is_vertical(a));
        let mut struck = None;
        for (part, above, band_r, band_len) in bands.iter().copied().chain(spindle) {
            // Unlike law (c), a band THINNER than the flute still
            // collides here: (c) skips it because the flute's kerf in
            // stock covers that radius, and steel has no such kerf.
            if band_len <= 0.0 {
                continue;
            }
            if super::stack_span::band_struck(&segs, lean, above, band_r, band_len, fixtures) {
                struck = Some(part);
                break; // once per move, first (worst) element struck
            }
        }
        if struck.is_none()
            && super::stack_span::band_struck(&segs, lean, 0.0, tool.radius, tool.flute_len, fixtures)
        {
            struck = Some(FixturePart::Flute);
        }
        if let Some(part) = struck.or(posed.get(i).copied().flatten()) {
            warns.push(Warning::FixtureHit {
                at: i,
                fixture_part: part,
            });
        }
    }
    warns
}

/// Rapids against fixtures (M3.2 stage B): the tool disc swept along a
/// rapid's whole segment against every box of the setup's fixtures,
/// then the rest of the upright stack (#424: shank, holder, the flute
/// column above the tip - each over the rapid's true z span, a band
/// struck reported by name, the flute column as the rapid). No
/// clearance gate and no vertical exemption - law (d) stands those down
/// because the air under the clearance plane and the kerf below a retract
/// are REMOVED MATERIAL; a fixture is steel that was never meant to move,
/// and a rapid dipping through a jaw or a clamp arm is a crash wherever it
/// runs. The z law is the strict-dip rule of `fixture::rapid_hits_box`: a
/// retract lifting off exactly the mounting surface stays silent (mating
/// surfaces contact by design); anything deeper than float dust reports.
/// The strict-dip rule holds for every band: each stands at or above
/// the tip. First hit per move, once. Region-free - see
/// [`bands_vs_fixtures`] for why that matters. No spindle housing: see
/// [`rapids_vs_fixtures_spindle`] for the stack with it.
pub fn rapids_vs_fixtures(
    path: &Toolpath,
    tool: &ToolGeom,
    fixtures: &[crate::fixture::FixtureBox],
) -> Vec<Warning> {
    rapids_vs_fixtures_spindle(path, tool, fixtures, None)
}

/// [`bands_vs_fixtures_held`] with the tool's own holder cylinder.
pub fn bands_vs_fixtures(
    path: &Toolpath,
    tool: &ToolGeom,
    fixtures: &[crate::fixture::FixtureBox],
    spindle: Option<(f64, f64)>,
    chord_tol: f64,
) -> Vec<Warning> {
    bands_vs_fixtures_held(path, tool, &HolderStack::cylinder(tool), fixtures, spindle, chord_tol)
}

/// [`rapids_vs_fixtures_held`] with the tool's own holder cylinder.
pub fn rapids_vs_fixtures_spindle(
    path: &Toolpath,
    tool: &ToolGeom,
    fixtures: &[crate::fixture::FixtureBox],
    spindle: Option<(f64, f64)>,
) -> Vec<Warning> {
    rapids_vs_fixtures_held(path, tool, &HolderStack::cylinder(tool), fixtures, spindle)
}

/// [`rapids_vs_fixtures`] with the machine's spindle housing band
/// (radius, length - the bands law's own `spindle`) riding on the
/// holder's top, and what stands above the shank (`holder`, #368): a
/// posed rapid sweeps the whole stack, the housing nose included
/// (M13.8 (1)), and an upright rapid carries every band of the stack -
/// shank, the holder's own bands, the housing - over its true z span
/// (#424).
pub fn rapids_vs_fixtures_held(
    path: &Toolpath,
    tool: &ToolGeom,
    holder: &HolderStack,
    fixtures: &[crate::fixture::FixtureBox],
    spindle: Option<(f64, f64)>,
) -> Vec<Warning> {
    let mut warns = Vec::new();
    if fixtures.is_empty() {
        return warns;
    }
    // The posed sweep (M13.8): a leaning rapid's whole stack - not
    // only its flute - along its real pose ([`super::posed_steel`]).
    let posed = super::posed_steel::posed_hits(path, tool, holder, fixtures, spindle, 0.01, true);
    // The bands above the flute an upright rapid carries (#424), read
    // from the same `HolderStack` the posed sweep and (c2) take (#368).
    let upper = super::stack_span::upper_bands(tool, holder, spindle);
    for (i, m) in path.moves.iter().enumerate() {
        if m.kind != MoveKind::Rapid {
            continue;
        }
        if posed.get(i).copied().flatten().is_some() {
            warns.push(Warning::FixtureHit {
                at: i,
                fixture_part: FixturePart::Rapid,
            });
            continue;
        }
        // Pose (M5.1 stage B, paying stage A's rapids debt): a rapid
        // whose tool leans sweeps a TILTED disc column along the chord,
        // not the upright pill the prismatic law measures. This law
        // encloses the swept parallelogram - the chord's XY shifted
        // along the whole lean run - in a capsule that is never
        // smaller: the centre chord moved half the lean aside, and the
        // radius grown by the parallelogram's own half-diagonal. The
        // z window spans both tips' axis reach plus the disc. Over-
        // report a graze, never hide a crash - the bands law's exact
        // precedent. The prismatic branch stays bit-identical to the
        // law's own history: same `rapid_hits_box`, same sums.
        match m.axis.filter(|a| !is_vertical(a)) {
            None => {
                // The flute disc first, its verdict as ever; then the
                // rest of the stack (#424) - shank, holder, the housing
                // when stated, the flute column - each over the rapid's
                // true z span ([`super::stack_span`]), under the same
                // strict z rule: a retract off a mounting face keeps
                // every band at or above the steel.
                let part = if fixtures
                    .iter()
                    .any(|b| crate::fixture::rapid_hits_box(&m.from, &m.to, tool.radius, b))
                {
                    Some(FixturePart::Rapid)
                } else {
                    super::stack_span::upright_rapid_struck(m, tool, &upper, fixtures)
                };
                if let Some(fixture_part) = part {
                    warns.push(Warning::FixtureHit { at: i, fixture_part });
                }
            }
            Some(a) => {
                let lean_xy = (a[0] * a[0] + a[1] * a[1]).sqrt();
                // |Q|/2: the parallelogram (chord ⊕ lean run) is exactly
                // contained in the capsule of radius half the run about
                // its centre line - every point sits |Q|/2 or less off it.
                let run2 = 0.5 * tool.flute_len * lean_xy;
                let r_use = tool.radius + run2;
                let (ox, oy) = (0.5 * tool.flute_len * a[0], 0.5 * tool.flute_len * a[1]);
                let (x0, y0) = (m.from.x + ox, m.from.y + oy);
                let (x1, y1) = (m.to.x + ox, m.to.y + oy);
                let za = m.from.z + tool.flute_len * a[2];
                let zb = m.to.z + tool.flute_len * a[2];
                let z_lo = m.from.z.min(m.to.z).min(za).min(zb) - tool.radius;
                let z_hi = m.from.z.max(m.to.z).max(za).max(zb) + tool.radius;
                if fixtures.iter().any(|b| {
                    crate::fixture::band_hits_box(x0, y0, x1, y1, z_lo, z_hi, r_use, b)
                }) {
                    warns.push(Warning::FixtureHit {
                        at: i,
                        fixture_part: FixturePart::Rapid,
                    });
                }
            }
        }
    }
    warns
}
