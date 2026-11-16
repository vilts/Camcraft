//! The slug law (#534, #290 D7): what drops free when the wire's cut
//! closes, by region connectivity. The kerf region (the disc of the
//! kerf offset swept along the wire path) is taken out of the material
//! section; the connected pieces left that lie inside the cut's filled
//! outline are freed, the rest stay held by the stock's outside.
//! Provenance: polygon booleans and the disc sweep of `geo2d`
//! (clipper2) over the public reading of a closed wire cut (the
//! enclosed region is cut free: a die's slug, or the punch itself).
//! Our own code.

use super::passes::Pass;
use super::{CutSide, Wire, WireFinding, WireJob, WireVerdict, SLUG_AREA_TOL};
use crate::geo2d::{Loop, Point2, Region};

/// A piece the closed cut frees at one height.
#[derive(Clone, Debug, PartialEq)]
pub struct Slug {
    pub z: f64,
    /// The piece's area, mm².
    pub area: f64,
    /// The middle of the piece's bounds.
    pub at: Point2,
    /// How much of the product the piece holds, mm².
    pub product_area: f64,
}

/// Each outer loop with the holes it carries, as its own region (a hole
/// goes to the smallest outer loop that contains it).
fn components(r: &Region) -> Result<Vec<Region>, String> {
    let (outers, holes): (Vec<&Loop>, Vec<&Loop>) = r.loops().iter().partition(|l| l.area() > 0.0);
    let mut groups: Vec<Vec<Loop>> = outers.iter().map(|o| vec![(*o).clone()]).collect();
    for h in holes {
        let Some(&p) = h.points().first() else { continue };
        let mut best: Option<(usize, f64)> = None;
        for (i, o) in outers.iter().enumerate() {
            let inside = Region::new(vec![(*o).clone()]).map_err(|e| e.to_string())?.contains(p);
            if inside && best.is_none_or(|(_, a)| o.area() < a) {
                best = Some((i, o.area()));
            }
        }
        if let Some((i, _)) = best
            && let Some(g) = groups.get_mut(i)
        {
            g.push(h.clone());
        }
    }
    groups.into_iter().map(|g| Region::new(g).map_err(|e| e.to_string())).collect()
}

fn centre(r: &Region) -> Point2 {
    r.bounds().map_or(Point2::new(0.0, 0.0), |(a, b)| Point2::new((a.x + b.x) * 0.5, (a.y + b.y) * 0.5))
}

/// The pieces freed at one height, or why they cannot be read.
fn freed_at(
    material: &Region,
    product: &Region,
    kerf: &Region,
    fill: &Region,
    z: f64,
) -> Result<Vec<Slug>, String> {
    let e = |e: crate::geo2d::GeoError| e.to_string();
    let mut out = Vec::new();
    for piece in components(&material.difference(kerf).map_err(e)?)? {
        let area = piece.area();
        if area <= SLUG_AREA_TOL || piece.difference(fill).map_err(e)?.area() > SLUG_AREA_TOL {
            continue;
        }
        let product_area = piece.intersect(product).map_err(e)?.area();
        out.push(Slug { z, area, at: centre(&piece), product_area });
    }
    Ok(out)
}

/// One pass of the slug law over `wire`: the pieces freed at the
/// lowest height holding material, the first height's flag, and the
/// first piece freed at any height.
struct Scan {
    first: Vec<Slug>,
    flag: Option<WireFinding>,
    any: Option<(f64, Point2)>,
}

fn scan(job: &WireJob<'_>, wire: &Wire, passes: &[Pass], opening: bool) -> Result<Scan, WireFinding> {
    let unreadable = |why: String| WireFinding::Unreadable(format!("the slug cannot be judged - {why}"));
    // The kerf at one height: the wire's own section there (#536 - a
    // vertical wire's is the same at every height), each pass swept by
    // its own kerf offset and joined (#538).
    let kerf_at = |z: f64| -> Result<(Region, Region), String> {
        let pts = wire.at(z);
        let mut kerf: Option<Region> = None;
        for p in passes {
            let Some(run) = p.of(&pts) else { continue };
            let r = Region::sweep(&[run.to_vec()], p.k).map_err(|e| e.to_string())?;
            kerf = Some(match kerf {
                None => r,
                Some(acc) => acc.union(&r).map_err(|e| e.to_string())?,
            });
        }
        let kerf = kerf.ok_or_else(|| "the path has no pass".to_owned())?;
        let fill = Region::new(kerf.loops().iter().filter(|l| l.area() > 0.0).cloned().collect()).map_err(|e| e.to_string())?;
        Ok((kerf, fill))
    };
    let mut out = Scan { first: Vec::new(), flag: None, any: None };
    let mut reported = false;
    for s in &job.sections {
        let material = match &s.stock {
            Some(st) => s.part.union(st),
            None => Ok(s.part.clone()),
        };
        let product = match (opening, &s.stock) {
            (false, _) => Ok(s.part.clone()),
            (true, Some(st)) => st.difference(&s.part),
            (true, None) => Ok(Region::empty()),
        };
        if material.as_ref().is_ok_and(Region::is_empty) {
            continue;
        }
        let (kerf, fill) = kerf_at(s.z).map_err(unreadable)?;
        let freed = match (material, product) {
            (Ok(m), Ok(p)) => freed_at(&m, &p, &kerf, &fill, s.z),
            (Err(e), _) | (_, Err(e)) => Err(e.to_string()),
        }
        .map_err(unreadable)?;
        if !reported {
            out.first.clone_from(&freed);
            reported = true;
        }
        if out.any.is_none() {
            out.any = freed.first().map(|f| (s.z, f.at));
        }
        if out.flag.is_some() {
            continue;
        }
        out.flag = match job.side {
            // An open run closes no slug (#1109): `judge` never asks.
            CutSide::Open => None,
            _ if job.burnout => freed.first().map(|f| WireFinding::Core { z: s.z, at: f.at, area: f.area }),
            _ if freed.is_empty() => Some(WireFinding::NothingFreed { z: s.z }),
            CutSide::Die => freed.iter().find(|f| f.product_area > SLUG_AREA_TOL).map(|f| WireFinding::WrongSlug {
                z: s.z,
                side: CutSide::Die,
                at: f.at,
                product_area: f.product_area,
            }),
            CutSide::Punch => freed.iter().all(|f| f.product_area <= SLUG_AREA_TOL).then(|| WireFinding::WrongSlug {
                z: s.z,
                side: CutSide::Punch,
                at: freed.first().map_or(Point2::new(0.0, 0.0), |f| f.at),
                product_area: 0.0,
            }),
        };
    }
    Ok(out)
}

/// Run the slug law (#534) with the hold law (#539): a tabbed cut's
/// main cut must free nothing and its whole path frees the stated
/// piece; an untabbed cut's freed piece must be captured.
pub(super) fn judge(job: &WireJob<'_>, wire: &Wire, passes: &[Pass], opening: bool, v: &mut WireVerdict) {
    if let Some(m) = job.cutoff_from {
        let Some(main) = wire.prefix(m + 1) else {
            return v.findings.push(WireFinding::Unreadable(format!("the tab cut-off's start (move {m}) is past the path")));
        };
        match scan(job, &main, passes, opening) {
            Err(f) => return v.findings.push(f),
            Ok(s) => {
                if let Some((z, at)) = s.any {
                    return v.findings.push(WireFinding::TabsDoNotHold { z, at });
                }
            }
        }
        v.held = true;
    }
    let s = match scan(job, wire, passes, opening) {
        Ok(s) => s,
        Err(f) => return v.findings.push(f),
    };
    v.slugs = s.first;
    if let Some(f) = s.flag {
        return v.findings.push(f);
    }
    if job.cutoff_from.is_none() && !job.burnout {
        if job.capture {
            v.captured = true;
        } else if let Some((z, at)) = s.any {
            v.findings.push(WireFinding::Unsupported { z, side: job.side, at });
        }
    }
}
