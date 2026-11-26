//! The contour's per-ring depth ladder against the part solid: a ring
//! sectioned off the model at the top plane is walked only at the levels
//! where the wall it trims still exists - a blind bore's ring ends at its
//! floor (plus the stock to leave), a through feature and an outer wall
//! keep every level. Specced from the 2.5D profiling rule of published
//! machining practice (a profile pass never descends below the floor of
//! the feature it finishes) read against ALL the part material standing
//! above each plane - its section plus the shadow of what hangs over it
//! (`slice::material_overhead`'s law, read in its two halves), so a ring
//! under an overhang or a part's underside is never walked through it.
//! Touching bodies (M13.11) are read as their union: the section is
//! sliced per body and united (`slice::material_above_of`), so a seam of
//! two coincident walls between two models is interior, not a refusal.
//! No vendor material consulted.

use std::collections::HashMap;
use std::ops::Range;

use crate::geo2d::{Point2, Region};
use crate::geometry::Triangle;
use crate::geometry::slice::{material_above_of, shadow_above};

#[path = "contour_exterior.rs"]
mod exterior;
pub(super) use exterior::ladder_picked;

/// Radial slack of the ring-legality test, mm: the offset ring's round
/// joins sit a chord sag inside the exact offset (`geo2d`'s 5 um arc
/// tolerance); anything nearer the part than `r + leave - LEGAL_SLACK`
/// means the wall moved in.
///
/// Held STRICTER than the verifier's tangency law (b), which convicts
/// below `r - gouge_tol` with `gouge_tol` = 0.01 mm (M13.7): a ring the
/// ladder walks keeps every sample - each move end the law reads, plus
/// the midpoints - at least `r - 0.008`, so the generator never ends a
/// pass where its own square verdict convicts a gouge that is not real.
/// The 2 um margin covers the two readers' separate micron-quantized
/// sections; the 3 um over the arc sag keeps ordinary walls walkable.
/// (It was 0.02, wider than the verdict: a wall facet leaning out
/// 0.01-0.02 mm let a flat-end ring down where law (b) blocked the post.)
const LEGAL_SLACK: f64 = 0.008;
/// Two heights closer than this are one level (a floor that lands on an
/// existing level never doubles it).
const SAME_LEVEL: f64 = 1e-6;
/// Probe heights per ring-level beyond which the flank check stops
/// sampling vertex planes (a dense curved mesh): bounded work, and the
/// verifier's model law still judges the posted path.
const MAX_FLANK_PROBES: usize = 256;

/// The ladder the contour walks: every level any ring is cut at
/// (descending), and per ring the levels it is cut at.
pub(super) struct Ladder {
    pub zs: Vec<f64>,
    pub per_ring: Vec<Vec<f64>>,
    /// Per ring: why it stopped when the part could not be SECTIONED at
    /// the plane that failed (a slice error - fail closed), `None` when it
    /// stopped on material or never stopped. #1927's law reads it, so a
    /// ring on its legal side over an unreadable mesh is not blamed on
    /// its side.
    pub unread: Vec<Option<String>>,
}

impl Ladder {
    /// Every ring at every level - the flat-region law, byte for byte.
    pub(super) fn uniform(zs: &[f64], rings: usize) -> Self {
        Ladder { zs: zs.to_vec(), per_ring: vec![zs.to_vec(); rings], unread: vec![None; rings] }
    }

    /// Whether ring `li` is cut at level `z`.
    pub(super) fn walks(&self, li: usize, z: f64) -> bool {
        self.per_ring.get(li).is_some_and(|v| v.iter().any(|q| q.to_bits() == z.to_bits()))
    }

    /// Ring `li`'s own levels (descending).
    pub(super) fn ring(&self, li: usize) -> &[f64] {
        self.per_ring.get(li).map_or(&[], |v| v.as_slice())
    }
}

fn dist2_seg(p: Point2, a: Point2, b: Point2) -> f64 {
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    let l2 = dx * dx + dy * dy;
    let t = if l2 > 0.0 {
        (((p.x - a.x) * dx + (p.y - a.y) * dy) / l2).clamp(0.0, 1.0)
    } else {
        0.0
    };
    let (qx, qy) = (a.x + dx * t - p.x, a.y + dy * t - p.y);
    qx * qx + qy * qy
}

/// The ring's samples: its vertices and segment midpoints.
fn samples(ring: &[Point2]) -> Vec<Point2> {
    let mut v = Vec::with_capacity(ring.len() * 2);
    for w in ring.windows(2) {
        v.push(w[0]);
        v.push(Point2::new((w[0].x + w[1].x) * 0.5, (w[0].y + w[1].y) * 0.5));
    }
    if let Some(&l) = ring.last() {
        v.push(l);
    }
    v
}

/// The part material standing above a plane, in its two halves: the
/// cross-section and the shadow of the down-facing faces above it.
/// Kept apart (not united) so a ring clear of a part solid down to the
/// plane reads exactly as before - the shadow only ever adds a veto.
struct Above {
    section: Region,
    shadow: Region,
}

/// Material above each plane, cached by plane; `Err` = a slice failed
/// (its reason kept), which every reader treats as "not legal" (fail
/// closed). `bodies` are the per-solid ranges of `facets` (fewer than
/// two: one body).
struct Sections<'a> {
    facets: &'a [Triangle],
    bodies: &'a [Range<usize>],
    cache: HashMap<u64, Result<Above, String>>,
}

/// Every sample keeps `need2` (squared) off the region and out of it.
fn clear_of(s: &Region, pts: &[Point2], need2: f64) -> bool {
    if s.is_empty() {
        return true;
    }
    pts.iter().all(|&p| {
        !s.contains(p)
            && s.loops().iter().all(|l| {
                let q = l.points();
                (0..q.len()).all(|k| dist2_seg(p, q[k], q[(k + 1) % q.len()]) >= need2)
            })
    })
}

impl Sections<'_> {
    fn at(&mut self, z: f64) -> Result<&Above, &str> {
        let (f, bodies) = (self.facets, self.bodies);
        self.cache
            .entry(z.to_bits())
            .or_insert_with(|| {
                let section = material_above_of(f, bodies, z).map_err(|e| e.to_string())?;
                let shadow = shadow_above(f, z).map_err(|e| e.to_string())?;
                Ok(Above { section, shadow })
            })
            .as_ref()
            .map_err(String::as_str)
    }

    /// The ring keeps `need` off the part's material strictly above `z`:
    /// out of its section and out of the shadow of what hangs above.
    /// (Clear of both halves is clear of their union: outside each, the
    /// union's boundary distance is the nearer of the two.)
    fn clear(&mut self, pts: &[Point2], z: f64, need: f64) -> bool {
        let Ok(a) = self.at(z) else { return false };
        let need2 = need.max(0.0) * need.max(0.0);
        clear_of(&a.section, pts, need2) && clear_of(&a.shadow, pts, need2)
    }
}

/// The part's distinct vertex heights, ascending.
fn vertex_heights(facets: &[Triangle]) -> Vec<f64> {
    let mut v: Vec<f64> = facets
        .iter()
        .flat_map(|t| [t.a.z, t.b.z, t.c.z])
        .filter(|z| z.is_finite())
        .collect();
    v.sort_by(f64::total_cmp);
    v.dedup_by(|a, b| (*a - *b).abs() <= 1e-9);
    v
}

/// Is the ring legal with its tip at `z` (the floor probed `leave` below
/// it) and its flank up to `above` (every vertex plane in between)?
fn legal_at(
    sec: &mut Sections,
    pts: &[Point2],
    heights: &[f64],
    z: f64,
    above: f64,
    leave: f64,
    need: f64,
) -> bool {
    if !sec.clear(pts, z - leave, need) {
        return false;
    }
    let flank: Vec<f64> = heights
        .iter()
        .copied()
        .filter(|&v| v > z - leave && v < above)
        .take(MAX_FLANK_PROBES)
        .collect();
    flank.into_iter().all(|v| sec.clear(pts, v, need))
}

/// Why [`legal_at`] failed, when it failed on a plane the part could not
/// be sectioned at: the planes in `legal_at`'s own order, the first one
/// that is unreadable or not clear decides. `None` = material (or legal).
fn unread_at(
    sec: &mut Sections,
    pts: &[Point2],
    heights: &[f64],
    z: f64,
    above: f64,
    leave: f64,
    need: f64,
) -> Option<String> {
    let flank = heights.iter().copied().filter(|&v| v > z - leave && v < above).take(MAX_FLANK_PROBES);
    for v in std::iter::once(z - leave).chain(flank) {
        if let Err(why) = sec.at(v) {
            return Some(why.to_string());
        }
        if !sec.clear(pts, v, need) {
            return None;
        }
    }
    None
}

/// Build the ladder for `rings` (closed center polylines, contour order)
/// over the flat ladder `zs` against the part `facets`. A ring stops at
/// the first level it is not legal at; the deepest legal vertex plane
/// above that level (plus `leave`) becomes its floor level. Each dropped
/// range is named in `notes`. `bodies`: the per-solid ranges of
/// `facets` when the part is a union of touching models (empty or one
/// range reads `facets` as one body).
///
/// `top` is Top Z (#2004: never a level): a ring whose first level is under
/// its floor searches its floor up to `top`, so a blind feature shallower
/// than one stepdown is cut at its floor, never dropped.
#[allow(clippy::too_many_arguments)]
pub(super) fn ladder(
    rings: &[Vec<Point2>],
    facets: &[Triangle],
    bodies: &[Range<usize>],
    zs: &[f64],
    top: f64,
    r: f64,
    leave: f64,
    notes: &mut Vec<String>,
) -> Ladder {
    ladder_in(rings, facets, bodies, zs, top, r, leave, notes, None)
}

#[allow(clippy::too_many_arguments)]
fn ladder_in(
    rings: &[Vec<Point2>],
    facets: &[Triangle],
    bodies: &[Range<usize>],
    zs: &[f64],
    top: f64,
    r: f64,
    leave: f64,
    notes: &mut Vec<String>,
    hull: Option<Region>,
) -> Ladder {
    let need = r + leave - LEGAL_SLACK;
    let heights = vertex_heights(facets);
    let mut sec = Sections { facets, bodies, cache: HashMap::new() };
    let mut per_ring: Vec<Vec<f64>> = Vec::with_capacity(rings.len());
    let mut unread: Vec<Option<String>> = Vec::with_capacity(rings.len());
    for ring in rings {
        // The full XY hull contains every section and overhead shadow.
        // A cutter clear of it stays clear at every height, even when an
        // unrelated interior hole cannot be sectioned at a vertex plane.
        if hull.as_ref().is_some_and(|h| exterior::clear(h, ring, need)) {
            per_ring.push(zs.to_vec());
            unread.push(None);
            continue;
        }
        let pts = samples(ring);
        let mut mine: Vec<f64> = Vec::new();
        let mut above = f64::INFINITY;
        let mut stop: Option<usize> = None;
        for (k, &z) in zs.iter().enumerate() {
            if legal_at(&mut sec, &pts, &heights, z, above, leave, need) {
                mine.push(z);
                above = z;
            } else {
                stop = Some(k);
                break;
            }
        }
        let mut why: Option<String> = None;
        if let Some(k) = stop {
            let z_bad = zs[k];
            why = unread_at(&mut sec, &pts, &heights, z_bad, above, leave, need);
            // The floor: the deepest legal vertex plane + leave strictly
            // between the failed level and the last walked one - or Top Z,
            // when the first level failed and the ring is clear above Top Z
            // (#2004: the old ladder's first level, at Top Z, in air).
            let ceiling = if !mine.is_empty() {
                Some(above)
            } else if top.is_finite()
                && top > z_bad + SAME_LEVEL
                && legal_at(&mut sec, &pts, &heights, top, f64::INFINITY, leave, need)
            {
                Some(top)
            } else {
                None
            };
            let mut floor: Option<f64> = None;
            if let Some(top) = ceiling {
                for &v in heights.iter().rev() {
                    let c = v + leave;
                    if c >= top - SAME_LEVEL || c <= z_bad + SAME_LEVEL {
                        continue;
                    }
                    if legal_at(&mut sec, &pts, &heights, c, top, leave, need) {
                        floor = Some(c);
                    } else {
                        break;
                    }
                }
            }
            if let Some(c) = floor {
                mine.push(c);
            }
            let skipped: Vec<String> = zs[k..].iter().map(|z| format!("{z:.3}")).collect();
            let at = ring.first().copied().unwrap_or(Point2::new(0.0, 0.0));
            notes.push(match mine.last() {
                Some(end) => format!(
                    "depth: the ring at ({:.3}, {:.3}) ends at its floor Z {end:.3} - the part is \
                     solid below it; levels below skipped ({})",
                    at.x,
                    at.y,
                    skipped.join(", ")
                ),
                None => format!(
                    "depth: the ring at ({:.3}, {:.3}) meets the part at its first level - not \
                     walked at any level ({})",
                    at.x,
                    at.y,
                    skipped.join(", ")
                ),
            });
        }
        per_ring.push(mine);
        unread.push(why);
    }
    let mut all: Vec<f64> = per_ring.iter().flatten().copied().collect();
    all.sort_by(|a, b| b.total_cmp(a));
    all.dedup_by(|a, b| a.to_bits() == b.to_bits());
    Ladder { zs: all, per_ring, unread }
}

/// The highest part vertex near `ring`: the facets whose XY box comes
/// within `reach` of the ring's XY box. `None` = no part stands near it.
fn local_top(ring: &[Point2], facets: &[Triangle], reach: f64) -> Option<f64> {
    let (mut x0, mut y0, mut x1, mut y1) = (f64::INFINITY, f64::INFINITY, f64::NEG_INFINITY, f64::NEG_INFINITY);
    for p in ring {
        (x0, y0, x1, y1) = (x0.min(p.x), y0.min(p.y), x1.max(p.x), y1.max(p.y));
    }
    let (x0, y0, x1, y1) = (x0 - reach, y0 - reach, x1 + reach, y1 + reach);
    facets
        .iter()
        .filter(|t| {
            let xs = [t.a.x, t.b.x, t.c.x];
            let ys = [t.a.y, t.b.y, t.c.y];
            let lo = |v: [f64; 3]| v.iter().copied().fold(f64::INFINITY, f64::min);
            let hi = |v: [f64; 3]| v.iter().copied().fold(f64::NEG_INFINITY, f64::max);
            lo(xs) <= x1 && hi(xs) >= x0 && lo(ys) <= y1 && hi(ys) >= y0
        })
        .flat_map(|t| [t.a.z, t.b.z, t.c.z])
        .filter(|z| z.is_finite())
        .reduce(f64::max)
}

/// #1927, the picked-ring law: a ring the user PICKED must trim its wall
/// at one level at least. The ladder above stops a ring where it stands
/// in the part, but a level at or above the part's top is legal for any
/// ring - nothing stands there - so a ring on the wrong side of its wall
/// (Outside of a hole, Inside of a boss) kept only the top plane and cut
/// air with no word. When the flat ladder asks for a level under the
/// part's top near the ring and the ring walks none, the side the user
/// chose lies in the part at every level: refused by name. When the ring
/// stopped because the part could not be SECTIONED there (the ladder's
/// fail-closed slice error), the refusal names that error and the skipped
/// levels instead - the side is not at fault. A whole-model
/// section never calls this - its rings come from the part itself.
pub(super) fn picked_law(
    rings: &[Vec<Point2>],
    facets: &[Triangle],
    zs: &[f64],
    lad: &Ladder,
    reach: f64,
    side: super::contour_params::Side,
) -> Result<(), super::face::GenError> {
    use super::contour_params::Side;
    for (li, ring) in rings.iter().enumerate() {
        let Some(top) = local_top(ring, facets, reach) else { continue };
        let asks_below = zs.iter().any(|&z| z < top - SAME_LEVEL);
        let trims = lad.ring(li).iter().any(|&z| z < top - SAME_LEVEL);
        if !asks_below {
            continue;
        }
        let at = ring.first().copied().unwrap_or(Point2::new(0.0, 0.0));
        if let Some(Some(why)) = lad.unread.get(li) {
            // A required plane could not be sectioned under the ring.
            // Deeper levels remain unproven even if earlier ones were
            // clear: name the mesh error, not a floor or the chosen side.
            let skipped: Vec<String> =
                zs.iter().filter(|&&z| z < top - SAME_LEVEL && !lad.walks(li, z)).map(|z| format!("{z:.3}")).collect();
            return Err(super::face::GenError::Invalid(format!(
                "the picked ring at ({:.3}, {:.3}) cannot reach every requested level below the part top Z {top:.3}: \
                 the part cannot be sectioned under it, so no deeper level is proven clear - levels below \
                 skipped ({}): {why}",
                at.x,
                at.y,
                skipped.join(", ")
            )));
        }
        if trims {
            continue;
        }
        let what = match side {
            // The side here is the RESOLVED one (the app turns Auto into
            // Inside or Outside first), and a tool containment moves the
            // ring too - so the advice names both, never "use Auto".
            Side::Inside => {
                "the cutter Inside this ring stands in the part material - if the ring bounds material, set Side to Outside; else check the tool containment"
            }
            Side::Outside => {
                "the cutter Outside this ring stands in the part material - if the ring bounds a hole, set Side to Inside; else check the tool containment"
            }
            _ => "the cutter stands in the part material - check Side and the tool containment",
        };
        return Err(super::face::GenError::Invalid(format!(
            "the picked ring at ({:.3}, {:.3}) meets its wall at no level below the part top Z {top:.3}: {what}",
            at.x, at.y
        )));
    }
    Ok(())
}
