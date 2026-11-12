//! Horizontal-plane slicing of a triangle soup, and the XY silhouette.
//!
//! Provenance: plane/triangle intersection with the half-open edge rule and
//! cycle assembly by welding chord endpoints is the standard marching-slices
//! formulation from published computational geometry; the welding trick
//! itself (bucket by quantized position, then weld within a micrometre so
//! duplicated STL corners count as shared even when not bit-identical)
//! lives in `slice_weld`.
//! Region semantics (orientation, overlap fusion, crossing decomposition)
//! come from `geo2d::Region::from_slices` - this module only produces the
//! cycles and the diagnostics.
//!
//! Contract: facets are in whatever frame is to be sliced (transform to
//! world before calling; STEP-imported meshes arrive tessellate-only, which
//! is why boundaries come from slicing at all). A triangle straddling the
//! plane contributes exactly one chord; chords welded at their ends assemble
//! into closed cycles. Half-open in z (an edge counts when one endpoint is
//! strictly below and the other at-or-above): a slice at a mesh's z-max
//! returns its boundary, one at z-min returns nothing - callers sample
//! strictly inside.
//!
//! Prime Directive: a mesh whose slice leaves chord ends dangling is open -
//! reported, never half-used. Tangential slices (grazing an edge or vertex)
//! error cleanly too; `project` nudges once before propagating.


use thiserror::Error;

use crate::geo2d::{COORD_LIMIT, GeoError, Loop, Point2, Region};
use crate::geometry::{Point3, Triangle};

/// Cap on `project`'s Z samples - a silhouette converges on far fewer.
pub const MAX_SAMPLES: usize = 64;

// The editor reads the same source and stock silhouette several times per
// frame (selection notes, outline, fill). Keep a few exact-input results so
// an unchanged edge pick does not repeat the full multi-slice union.
const PROJECT_CACHE_ENTRIES: usize = 4;
const PROJECT_CACHE_FACET_LIMIT: usize = 50_000;
type ProjectEntry = (usize, Vec<Triangle>, Result<Region, SliceError>);
thread_local! {
    static PROJECT_CACHE: std::cell::RefCell<Vec<ProjectEntry>> =
        const { std::cell::RefCell::new(Vec::new()) };
    static PROJECT_CACHE_HITS: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
}

fn profile_projection() -> bool {
    static ENABLED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ENABLED.get_or_init(|| std::env::var_os("CAMCRAFT_PROFILE_PROJECTION").is_some())
}

fn project_timed(facets: &[Triangle], samples: usize, cacheable: bool) -> Result<Region, SliceError> {
    // The clock is read only when profiling is on (#31): std's
    // `Instant::now()` panics on wasm32-unknown-unknown, where the
    // environment variable can never be set, so the browser build never
    // touches it.
    let started = profile_projection().then(std::time::Instant::now);
    let result = project_uncached(facets, samples);
    if let Some(started) = started {
        eprintln!(
            "profile: projection {} facets={} samples={} elapsed={} ms result={}",
            if cacheable { "cache miss" } else { "uncached large mesh" },
            facets.len(),
            samples,
            started.elapsed().as_millis(),
            if result.is_ok() { "ok" } else { "error" },
        );
    }
    result
}

#[derive(Clone, Error, Debug, PartialEq)]
pub enum SliceError {
    #[error("invalid slice input: {0}")]
    Invalid(String),
    #[error("the mesh is open at z = {z}: {dangling} dangling chord ends")]
    Open { z: f64, dangling: usize },
    #[error("chords meet 3+ ways at z = {z}: non-manifold contact")]
    NonManifold { z: f64 },
    #[error("flat model: zero Z extent between {min_z} and {max_z}")]
    Flat { min_z: f64, max_z: f64 },
    #[error(transparent)]
    Geo(#[from] GeoError),
}

/// Cross `facets` with the plane z = `z` and return the material region.
/// An empty intersection is an empty region, not an error (z missed the
/// model); bad geometry is always an error, never a partial region.
///
/// Chord ends weld on their exact 1 um key first - every mesh that slices
/// that way keeps exactly the answer it always had. Only a slice that
/// leaves ends dangling is retried with the micrometre tolerance weld
/// (#241: an imported mesh's shared corners need not be bit-identical);
/// a mesh still open under the tolerance is refused as before.
pub fn slice(facets: &[Triangle], z: f64) -> Result<Region, SliceError> {
    match slice_with(facets, z, false) {
        Err(SliceError::Open { .. }) => slice_with(facets, z, true),
        other => other,
    }
}

fn slice_with(facets: &[Triangle], z: f64, tolerant: bool) -> Result<Region, SliceError> {
    if !z.is_finite() {
        return Err(SliceError::Invalid("non-finite slice height".into()));
    }
    for t in facets {
        for p in [t.a, t.b, t.c] {
            if !p.is_finite() {
                return Err(SliceError::Invalid("non-finite vertex".into()));
            }
            if beyond_horizon(p) {
                return Err(SliceError::Geo(GeoError::OutOfRange));
            }
        }
    }

    // 1. Chords. Edges are canonicalized low-z-first so the two triangles
    //    sharing an edge compute their crossing point with identical float
    //    arithmetic and it welds exactly; the zero-length guard drops
    //    vertex grazes (a plane touching a triangle at one point encloses
    //    nothing).
    let mut welder = super::slice_weld::Welder::new(tolerant);
    let mut segs: Vec<[u32; 2]> = Vec::new();
    for t in facets {
        let mut hits: [Point2; 2] = [Point2::default(); 2];
        let mut n = 0usize;
        for (p, q) in [(t.a, t.b), (t.b, t.c), (t.c, t.a)] {
            let (lo, hi) = if p.z <= q.z { (p, q) } else { (q, p) };
            if lo.z < z && hi.z >= z {
                if n == 2 {
                    // Three crossings of one triangle boundary with one
                    // plane is impossible for finite input; refuse loudly
                    // rather than index out of bounds.
                    return Err(SliceError::Invalid("triangle meets plane 3x".into()));
                }
                // lo.z < z <= hi.z keeps the denominator positive.
                let s = (z - lo.z) / (hi.z - lo.z);
                hits[n] = Point2::new(
                    lo.x + s * (hi.x - lo.x),
                    lo.y + s * (hi.y - lo.y),
                );
                n += 1;
            }
        }
        match n {
            2 => {
                let i = welder.weld(hits[0]);
                let j = welder.weld(hits[1]);
                if i != j {
                    segs.push([i, j]);
                }
            }
            0 => {}
            _ => return Err(SliceError::Invalid(format!("odd chord count at z = {z}"))),
        }
    }

    // 2. Every chord end must be shared by exactly two chords.
    let pts = welder.points();
    let mut adj: Vec<Vec<u32>> = vec![Vec::new(); pts.len()];
    for (k, [i, j]) in segs.iter().enumerate() {
        adj[*i as usize].push(k as u32);
        adj[*j as usize].push(k as u32);
    }
    let mut dangling = 0usize;
    let mut meeting = false;
    for a in &adj {
        match a.len() {
            1 => dangling += 1,
            2 => {}
            0 => {} // grazed vertex, chord skipped: carries no boundary
            _ => meeting = true,
        }
    }
    if dangling > 0 {
        return Err(SliceError::Open { z, dangling });
    }
    if meeting {
        return Err(SliceError::NonManifold { z });
    }

    // 3. Walk the 2-regular graph into closed cycles.
    let mut used = vec![false; segs.len()];
    let mut loops = Vec::new();
    for s in 0..segs.len() {
        if used[s] {
            continue;
        }
        used[s] = true;
        let [tail, head_start] = segs[s];
        let mut head = head_start;
        let mut chain: Vec<Point2> = vec![pts[tail as usize], pts[head_start as usize]];
        for _ in 0..segs.len() {
            if head == tail {
                break;
            }
            let next = adj[head as usize]
                .iter()
                .copied()
                .find(|k| !used[*k as usize]);
            let ns = match next {
                Some(k) => k as usize,
                None => return Err(SliceError::Invalid("cycle walk stalled".into())),
            };
            used[ns] = true;
            let [a, b] = segs[ns];
            head = if a == head { b } else { a };
            chain.push(pts[head as usize]);
        }
        if head != tail {
            return Err(SliceError::Invalid("cycle walk did not close".into()));
        }
        loops.push(Loop::new(chain)?);
    }

    // 4. Normalize: self-touching section outlines decompose into lobes,
    //    nesting decides hole orientation, the non-zero fuse unites shells.
    Ok(Region::from_slices(loops)?)
}

/// The XY silhouette: material at *any* height, as a union of slices
/// sampled evenly strictly inside the Z extent. This is the footprint a
/// contour path or a collision check asks of a solid. `samples` is clamped
/// to [`MAX_SAMPLES`]; a slice that fails on a tangential coincidence is
/// retried once at a nudge - if it fails again the error is real.
pub fn project(facets: &[Triangle], samples: usize) -> Result<Region, SliceError> {
    if facets.len() > PROJECT_CACHE_FACET_LIMIT {
        return project_timed(facets, samples, false);
    }
    if let Some(hit) = PROJECT_CACHE.with(|cache| {
        let mut cache = cache.borrow_mut();
        let i = cache.iter().position(|(n, old, _)| *n == samples && old.as_slice() == facets)?;
        let entry = cache.remove(i);
        let result = entry.2.clone();
        cache.push(entry);
        Some(result)
    }) {
        if profile_projection() {
            PROJECT_CACHE_HITS.with(|hits| {
                let count = hits.get().saturating_add(1);
                hits.set(count);
                if count == 1 || count % 100 == 0 {
                    eprintln!("profile: projection cache hits={count}");
                }
            });
        }
        return hit;
    }
    let result = project_timed(facets, samples, true);
    PROJECT_CACHE.with(|cache| {
        let mut cache = cache.borrow_mut();
        if cache.len() == PROJECT_CACHE_ENTRIES {
            drop(cache.remove(0));
        }
        cache.push((samples, facets.to_vec(), result.clone()));
    });
    result
}

fn project_uncached(facets: &[Triangle], samples: usize) -> Result<Region, SliceError> {
    if facets.is_empty() {
        return Ok(Region::empty());
    }
    let (mut zmin, mut zmax) = (f64::INFINITY, f64::NEG_INFINITY);
    for t in facets {
        for p in [t.a, t.b, t.c] {
            if !p.is_finite() {
                return Err(SliceError::Invalid("non-finite vertex".into()));
            }
            zmin = zmin.min(p.z);
            zmax = zmax.max(p.z);
        }
    }
    let span = zmax - zmin;
    if !span.is_finite() {
        return Err(SliceError::Invalid("non-finite Z extent".into()));
    }
    if span <= 0.0 {
        return Err(SliceError::Flat { min_z: zmin, max_z: zmax });
    }
    let n = samples.clamp(1, MAX_SAMPLES);
    let nudge = span.max(1.0) * 1e-3;
    let mut acc: Option<Region> = None;
    for k in 0..n {
        let z = zmin + (k as f64 + 0.5) * (span / n as f64);
        let region = match slice(facets, z) {
            Ok(r) => r,
            Err(first_err) => match slice(facets, z + nudge) {
                Ok(r) => r,
                Err(_) => return Err(first_err),
            },
        };
        if region.is_empty() {
            continue;
        }
        acc = Some(match acc {
            None => region,
            Some(a) => a.union(&region)?,
        });
    }
    Ok(acc.unwrap_or_default())
}

fn beyond_horizon(p: Point3) -> bool {
    p.x.abs() >= COORD_LIMIT || p.y.abs() >= COORD_LIMIT || p.z.abs() >= COORD_LIMIT
}

/// The model's cross-section strictly above `z`, nudge-retried when a
/// plane grazes a tessellated shelf (reading an errored slice as
/// "no material" would be the silent wrong answer; the retry lands
/// above the shelf instead, failing safe). Three probes, one
/// millionth-of-a-millimetre apart, first survivor wins. The plain
/// section: the verifier's island cache reads it at each move's plane,
/// and it is the first half of the generators' islands law,
/// [`material_overhead`] (section plus [`shadow_above`]) - one law at
/// the layer that owns the mesh (M2.1a, M4.1 R4).
pub fn material_above(facets: &[Triangle], z: f64) -> Result<Region, SliceError> {
    const EPS: f64 = 1e-6;
    let mut err = None;
    for k in 0..3 {
        let probe = z + f64::from(k + 1) * EPS;
        match slice(facets, probe) {
            Ok(r) => return Ok(r),
            Err(e) => err = Some(e),
        }
    }
    Err(err.unwrap_or(SliceError::Invalid("cannot section the model".into())))
}

/// The XY shadow of the model's down-facing faces strictly above `z`
/// (each clipped to the half-space over the plane). Together with the
/// cross-section it is the projection of all material standing above
/// the plane: a column the plane finds in air but material crosses
/// higher up meets that material first at a face whose outward normal
/// points down (a blind floor's underside seen from below, an overhang,
/// an undercut lip). Facet winding is the outward right-hand rule every
/// importer and primitive keeps (`Triangle::normal`). Provenance: the
/// projection identity of a closed solid's boundary (computational
/// geometry textbook material); no vendor source.
pub fn shadow_above(facets: &[Triangle], z: f64) -> Result<Region, SliceError> {
    const EPS: f64 = 1e-6;
    if !z.is_finite() {
        return Err(SliceError::Invalid("non-finite level plane".into()));
    }
    let c = z + EPS;
    let mut loops = Vec::new();
    for t in facets {
        if t.a.z.max(t.b.z).max(t.c.z) <= c {
            continue; // wholly at or under the plane
        }
        let n = (t.b - t.a).cross(t.c - t.a);
        if n.z.is_nan() || n.z >= 0.0 {
            continue; // up-facing, vertical, degenerate or non-finite
        }
        let poly = clip_above([t.a, t.b, t.c], c);
        // Down-facing projects clockwise; the union wants material CCW.
        let pts: Vec<Point2> = poly.iter().rev().map(|p| Point2::new(p.x, p.y)).collect();
        if pts.len() < 3 {
            continue;
        }
        if let Ok(l) = Loop::new(pts)
            && l.area() > 0.0
        {
            loops.push(l);
        }
    }
    if loops.is_empty() {
        return Ok(Region::empty());
    }
    Ok(Region::union_all(loops)?)
}

/// ALL the part material standing above `z`, projected to XY: the
/// cross-section strictly above the plane ([`material_above`]) united
/// with the shadow of what hangs over it ([`shadow_above`]) - what a
/// vertical cutter standing at `z` must stay out of, shank included.
/// The generators' islands law. [`material_above`] keeps its plain
/// section meaning (the verifier's per-plane reading); the two agree
/// only for a part solid all the way down to the plane.
pub fn material_overhead(facets: &[Triangle], z: f64) -> Result<Region, SliceError> {
    let cut = material_above(facets, z)?;
    let hang = shadow_above(facets, z)?;
    if hang.is_empty() {
        return Ok(cut);
    }
    Ok(cut.union(&hang)?)
}

/// [`material_above`] of several bodies that TOUCH, read as the union
/// they stand for (M13.11): `bodies` are index ranges of `facets`, one
/// per closed solid. Two bodies butted face to face put two coincident
/// walls of opposite normal into one soup, which the chord graph of
/// [`slice`] meets as a non-manifold contact and refuses; sliced each on
/// its own they are two clean sections, and the 2D boolean union of
/// those sections is the section of the union solid - the same law the
/// region builders apply to picks across entities. Fewer than two
/// bodies is the plain call, byte for byte; a body that will not slice
/// on its own (a self-intersecting shell) still refuses with its own
/// error - the seam read never widens what counts as manifold. A range
/// outside `facets` is an input error, never a silent partial read.
pub fn material_above_of(
    facets: &[Triangle],
    bodies: &[std::ops::Range<usize>],
    z: f64,
) -> Result<Region, SliceError> {
    if bodies.len() < 2 {
        return material_above(facets, z);
    }
    let mut acc: Option<Region> = None;
    for b in bodies {
        let Some(part) = facets.get(b.clone()) else {
            return Err(SliceError::Invalid(format!(
                "body range {}..{} is outside the {} facets",
                b.start,
                b.end,
                facets.len()
            )));
        };
        let r = material_above(part, z)?;
        if r.is_empty() {
            continue;
        }
        acc = Some(match acc {
            None => r,
            Some(a) => a.union(&r)?,
        });
    }
    Ok(acc.unwrap_or_default())
}

/// [`material_overhead`] over touching bodies ([`material_above_of`]'s
/// section united with the shadow of the whole soup - the shadow is a
/// projection of down-facing faces, which a vertical seam never has).
pub fn material_overhead_of(
    facets: &[Triangle],
    bodies: &[std::ops::Range<usize>],
    z: f64,
) -> Result<Region, SliceError> {
    if bodies.len() < 2 {
        return material_overhead(facets, z);
    }
    let cut = material_above_of(facets, bodies, z)?;
    let hang = shadow_above(facets, z)?;
    if hang.is_empty() {
        return Ok(cut);
    }
    Ok(cut.union(&hang)?)
}

/// The part of a triangle at or above the plane `c` (Sutherland-Hodgman
/// against one half-space): 0, 3 or 4 vertices, in the input's order.
fn clip_above(v: [Point3; 3], c: f64) -> Vec<Point3> {
    let mut out = Vec::with_capacity(4);
    for i in 0..3 {
        let (p, q) = (v[i], v[(i + 1) % 3]);
        let (pin, qin) = (p.z >= c, q.z >= c);
        if pin {
            out.push(p);
        }
        if pin != qin {
            let t = (c - p.z) / (q.z - p.z);
            out.push(Point3::new(p.x + (q.x - p.x) * t, p.y + (q.y - p.y) * t, c));
        }
    }
    out
}
