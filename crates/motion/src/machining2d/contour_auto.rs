//! Side Auto for picked contour rings (#1933): the side of the rings
//! that holds the material, read from the part's section at the rings'
//! own level - never from the full XY silhouette, which a blind bore's
//! floor fills. Provenance: the 2.5D profiling rule of published
//! machining practice (the cutter rides the air side of the wall it
//! trims); our own design notes. No vendor material consulted.

use std::ops::Range;

use crate::geo2d::Region;
use crate::geometry::Triangle;
use crate::geometry::slice::material_above_of;

use super::face::GenError;

/// How far from the rings' level each probe section is taken, mm: just
/// below the highest ring point and just above the lowest one. Far
/// under any wall a cutter trims, far over the slice's own nudge.
pub const AUTO_PROBE_DZ: f64 = 0.01;
/// Width of the band just outside the rings that stands for the other
/// side of the wall, mm.
pub const AUTO_BAND: f64 = 0.5;
/// A side "holds material" when at least this fraction of its probe
/// area is part section (the old fill law's threshold).
const HOLDS: f64 = 0.5;

/// What Side Auto resolves to for a set of picked rings.
#[derive(Debug, Clone, PartialEq)]
pub enum AutoSide {
    /// The material is inside the rings: the cutter goes outside.
    Outside,
    /// The material is outside the rings: the cutter goes inside.
    Inside,
    /// The section cannot tell (material on both sides, or on neither,
    /// or the probes disagree): the sentence asks for a named side.
    Undecided(String),
}

/// What one probe section reads on the two sides of the rings.
#[derive(Clone, Copy, PartialEq)]
enum Read {
    /// Material inside only: the cutter goes outside.
    Outside,
    /// Material outside only: the cutter goes inside.
    Inside,
    /// Material on both sides.
    Both,
    /// No material on either side, or no section to read.
    Neither,
}

fn probe(rings: &Region, band: &Region, facets: &[Triangle], bodies: &[Range<usize>], z: f64) -> Result<Read, GenError> {
    let Ok(sec) = material_above_of(facets, bodies, z) else {
        return Ok(Read::Neither);
    };
    let (whole, ring_band) = (rings.area().abs(), band.area().abs());
    if whole <= 0.0 || ring_band <= 0.0 || sec.is_empty() {
        return Ok(Read::Neither);
    }
    let inside = rings.intersect(&sec)?.area().abs() / whole >= HOLDS;
    let outside = band.intersect(&sec)?.area().abs() / ring_band >= HOLDS;
    Ok(match (inside, outside) {
        (true, false) => Read::Outside,
        (false, true) => Read::Inside,
        (true, true) => Read::Both,
        (false, false) => Read::Neither,
    })
}

/// Side Auto for `rings`, picked between Z `z_lo` and `z_hi` (one level
/// for a flat chain or face; a wall patch spans its height). The part
/// section is read just below the top of that span, then just above its
/// bottom: the first probe that finds material on exactly one side
/// decides. A blind bore rim reads air inside and metal outside at its
/// mouth (Inside), as a through hole does; a boss or an outer wall reads
/// metal inside and air outside (Outside). A floor rim reads metal on
/// both sides below it and decides from above. When no probe sees any
/// material near the rings (a sheet part with no volume, a ring that
/// floats over the part), the part's XY outline (`plan`, read only
/// then: it costs a full projection) decides as it
/// always did: rings that bound material are cut Outside, rings that
/// bound air Inside. When the probes see material but cannot decide, or
/// the two disagree, the answer is [`AutoSide::Undecided`] with the
/// sentence to show - never a guess.
pub fn auto_side(
    rings: &Region,
    facets: &[Triangle],
    bodies: &[Range<usize>],
    plan: impl FnOnce() -> Option<Region>,
    z_lo: f64,
    z_hi: f64,
) -> Result<AutoSide, GenError> {
    if facets.is_empty() || rings.is_empty() || !z_lo.is_finite() || !z_hi.is_finite() {
        // No part to read: the ring bounds what it bounds - Outside, the
        // side a contour of a sketch outline always took.
        return Ok(AutoSide::Outside);
    }
    let band = rings.offset(AUTO_BAND)?.difference(rings)?;
    let first = probe(rings, &band, facets, bodies, z_hi - AUTO_PROBE_DZ)?;
    let second = probe(rings, &band, facets, bodies, z_lo + AUTO_PROBE_DZ)?;
    let undecided = |why: &str| {
        AutoSide::Undecided(format!(
            "Side Auto cannot tell which side of the picked ring holds the material at Z {z_hi:.3}: {why} - set Side to Inside or Outside"
        ))
    };
    let decisive = |r: Read| matches!(r, Read::Inside | Read::Outside);
    Ok(match (first, second) {
        (a, b) if decisive(a) && decisive(b) && a != b => {
            undecided("the part stands inside the ring on one side of that level and outside it on the other")
        }
        (Read::Inside, _) | (_, Read::Inside) => AutoSide::Inside,
        (Read::Outside, _) | (_, Read::Outside) => AutoSide::Outside,
        (Read::Neither, Read::Neither) => from_plan(rings, plan().as_ref())?,
        _ => undecided("the part stands on both sides of the ring there"),
    })
}

/// The outline law for rings with no material at their level: the share
/// of the rings' area that the part's XY outline covers.
fn from_plan(rings: &Region, plan: Option<&Region>) -> Result<AutoSide, GenError> {
    let whole = rings.area().abs();
    let Some(plan) = plan.filter(|_| whole > 0.0) else {
        return Ok(AutoSide::Outside);
    };
    let fill = rings.intersect(plan)?.area().abs() / whole;
    Ok(if fill < HOLDS { AutoSide::Inside } else { AutoSide::Outside })
}
