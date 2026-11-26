//! Wire EDM 2.5D verification (#534, part of #290 D7): a vertical
//! 2-axis wire cut judged on the program plane against the part's
//! sections through the whole height the wire passes.
//!
//! - **Kerf law.** The wire burns a slot of half-width `k` (wire radius
//!   plus spark gap) along every move, through every height between its
//!   guides. A move whose kerf comes nearer the product's section than
//!   `k` - or stands in it - is a gouge, named by its move.
//! - **Threading law.** The wire is threaded at the start hole through
//!   the full height: the product must stand clear of the threading
//!   point by `k` at every judged plane. A start hole that is not
//!   stated, or that the path does not start from, is refused by name.
//! - **Slug law.** The kerf region is subtracted from the material
//!   section (stock and part); the connected pieces enclosed by the cut
//!   are what drops free when the cut closes. A die keeps the material
//!   outside the cut, so its freed piece must hold none of the product;
//!   a punch's freed piece IS the product. A cut that frees nothing, or
//!   frees the wrong side, is refused by name. A no-core burnout (#540)
//!   reads the other way: it must free nothing - a freed piece is a
//!   core left to drop, refused.
//!
//! An OPEN run (#1109, side left / right of travel) is threaded in the
//! air outside the stock and leaves it at a run-out point: it has no
//! start hole and closes no slug, so the threading and slug laws do not
//! apply and say so ([`OPEN_SKIP`]). The kerf law judges it against the
//! part as for any cut, and its ends - where the wire is threaded and
//! cut - must stand clear of the stock by the kerf offset.
//!
//! The product's section by the stated side: a punch's product is the
//! model. A die's product is the model too when the start hole stands
//! outside it (the model is a die plate, the opening a hole in it); when
//! the start hole stands IN the model, the model is the opening (the
//! slug's shape, #532's whole-model die) and the product is the stock
//! around it.
//!
//! A constant taper (#536): the moves state poses (D2: the wire's
//! direction from its lower-plane point toward the upper guide), and
//! the wire at height `z` stands at `to + pose * (z - to.z) / pose.z`,
//! both ends interpolating along a move - the ruled surface. Every law
//! reads the wire's own section at each judged height, and the kerf
//! across a leaning wire is the ellipse its disc cuts from the plane:
//! `k * sqrt(1 + (t.u)^2)` toward the product in direction `u`, `t`
//! the wire's lean per unit height. Poses are read only from a cut that
//! states a taper; any other posed move stays unreadable.
//!
//! A stated verify tolerance (#1170): the sections are the model's
//! facets, and a tessellated loft bulges past its true ruled surface by
//! up to the chord tolerance it was tessellated at. A cut that states
//! that tolerance has the kerf law pass a reading that enters the
//! facets by no more than it; the verdict says the deepest such
//! reading, and a deeper one is a gouge that names the tolerance. The
//! threading and slug laws never use it. Zero - the default - judges
//! the facets as they stand.
//!
//! A constant taper against a meshed model (#3210): the generator lays
//! the wire from the model's section at one height, and the facets at
//! the other heights stand off the true wall by up to the model's
//! chord tolerance. The caller states that chord as the verify
//! tolerance (`taper_chord`), and the undercut law (`undercut`) judges
//! the last pass under it: a move that cuts a drafted wall at one
//! height must cut it at every height.
//!
//! Arcs read from a meshed model (#3215): a vertical cut whose arcs were
//! read back from the facets passes between the facet chords and the
//! true circle. The caller states the model's chord as the verify
//! tolerance (`taper_chord`) for it too. The undercut law runs only for
//! a cut that states a taper.
//!
//! Provenance: plain 2D distance and region geometry (point-segment
//! distance, polygon booleans and the disc sweep of `geo2d`, clipper2)
//! over the public wire EDM practice the umbrella #290 cites: the kerf
//! is the wire diameter plus the gap on both sides, the wire is
//! threaded through a start hole, and a closed cut frees the enclosed
//! region (die slug or punch). Our own code; no vendor post, control
//! manual or CAM product behaviour was consulted.

mod arcs;
mod dist;
mod finding;
mod kerf;
mod passes;
mod slug;
mod undercut;
#[cfg(test)]
mod arc_tests;
#[cfg(test)]
mod depth_tests;
#[cfg(test)]
mod hold_tests;
#[cfg(test)]
mod taper_tests;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod tol_tests;

use crate::cl::Toolpath;
use crate::geo2d::{Point2, Region};

pub use arcs::ARC_SAG;
pub use finding::{slugs_said, WireFinding, WireUnder};
pub use slug::Slug;

use kerf::{gouge_at, polyline, Wire};

/// How far (mm) the kerf may come under its offset to the product
/// before the kerf law calls it a gouge: 0.1 um - the generator builds
/// the path at the offset to 10 nm, the sections are facet-exact.
pub const KERF_TOL: f64 = 1e-4;
/// The least area (mm²) of product a freed piece may hold before the
/// slug law reads it as holding product: a numerical sliver is not a
/// part.
pub const SLUG_AREA_TOL: f64 = 1e-3;

/// Why an open run's verdict holds no threading or slug finding: said
/// on the report line, never silent.
pub const OPEN_SKIP: &str = "start-hole and slug laws not applied (an open run is threaded in the air outside the stock and closes no slug)";

/// Which side of the cut is kept - stated by the operation, never read
/// from the geometry.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CutSide {
    /// The opening is the product: the material outside the cut stays.
    Die,
    /// The enclosed piece is the product: it drops when the cut closes.
    Punch,
    /// An open run (#1109): the kerf on the stated left or right of
    /// travel, the model is the product, nothing is enclosed.
    Open,
}

impl CutSide {
    #[must_use]
    pub fn word(self) -> &'static str {
        match self {
            CutSide::Die => "die",
            CutSide::Punch => "punch",
            CutSide::Open => "open",
        }
    }
}

/// The material at one judged height: the part model's section and the
/// stock's (when the setup has one).
#[derive(Clone, Debug)]
pub struct WireSection {
    pub z: f64,
    pub part: Region,
    pub stock: Option<Region>,
}

/// One wire cut to judge.
#[derive(Clone, Debug)]
pub struct WireJob<'a> {
    pub path: &'a Toolpath,
    /// Wire radius plus spark gap (mm) - the rough (first) pass's.
    pub kerf_offset: f64,
    /// Each skim pass's own kerf offset (its wire radius plus its own
    /// spark gap, mm), in cut order (#538); empty is the one pass at
    /// `kerf_offset`. Every pass starts and ends at the threading
    /// point, and each move is judged at its own pass's offset.
    pub skim_offsets: Vec<f64>,
    pub side: CutSide,
    /// The operation's stated start hole; `None` is not stated.
    pub start: Option<Point2>,
    /// The sections through the height the wire passes, low to high.
    pub sections: Vec<WireSection>,
    /// A tabbed cut (#539): the first move of the tab cut-off. The main
    /// cut before it must free nothing (the tabs hold the slug); the
    /// whole path, cut-off included, is judged by the slug law.
    pub cutoff_from: Option<usize>,
    /// The operator states the freed piece is caught (#539): an untabbed
    /// cut may then free it; without it a freed piece is refused.
    pub capture: bool,
    /// The operation states a constant taper (#536): its moves' poses
    /// are read as the wire's lean. False: a posed move is unreadable.
    pub taper: bool,
    /// A no-core burnout (#540): the cut must free nothing - any piece
    /// it frees is a core that drops, refused.
    pub burnout: bool,
    /// The stated verify tolerance (#1170, mm; module head): how far the
    /// kerf may enter the model's facets and still pass the kerf law.
    /// 0 judges the facets as they stand.
    pub verify_tol: f64,
    /// `verify_tol` is the model's chord tolerance (module head): a
    /// constant-taper cut (#3210), whose undercut law runs under it, or a
    /// vertical cut of arcs read from the facets (#3215).
    pub taper_chord: bool,
}

/// The verdict: every refusal, and the pieces the cut frees at the
/// lowest judged height that holds material (the report line's slug).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct WireVerdict {
    pub findings: Vec<WireFinding>,
    pub slugs: Vec<Slug>,
    /// The tabs hold the slug through the main cut; the cut-off frees
    /// `slugs` (#539).
    pub held: bool,
    /// The freed piece drops into a stated capture (#539).
    pub captured: bool,
    /// The laws not applied to this cut and why ([`OPEN_SKIP`] for an
    /// open run); `None` when every law ran.
    pub skipped: Option<&'static str>,
    /// The verify tolerance the kerf law judged under (#1170, mm).
    pub verify_tol: f64,
    /// The deepest kerf reading that tolerance let pass; `None` when no
    /// reading needed it.
    pub under: Option<WireUnder>,
    /// `verify_tol` is the model's chord tolerance (#3210), not stated.
    pub chord: bool,
}

/// The product's section at one height (module head): the model, or
/// the stock around it when the model is a die's opening.
pub(crate) enum Product<'a> {
    Part(&'a Region),
    /// Everything outside `opening`, within `stock` when there is one.
    Around { opening: &'a Region, stock: Option<&'a Region> },
}

impl Product<'_> {
    fn contains(&self, p: Point2) -> bool {
        match self {
            Product::Part(r) => r.contains(p),
            Product::Around { opening, stock } => !opening.contains(p) && stock.is_none_or(|s| s.contains(p)),
        }
    }

    fn boundary(&self) -> &Region {
        match self {
            Product::Part(r) => r,
            Product::Around { opening, .. } => opening,
        }
    }
}

/// Judge one wire cut by the three laws.
#[must_use]
pub fn judge(job: &WireJob<'_>) -> WireVerdict {
    let mut v = WireVerdict::default();
    let k = job.kerf_offset;
    if !(k.is_finite() && k > 0.0) {
        v.findings.push(WireFinding::Unreadable(format!("the kerf offset {k} is not a positive number")));
        return v;
    }
    let tol = job.verify_tol;
    if !(tol.is_finite() && tol >= 0.0 && tol < k) {
        v.findings.push(WireFinding::Unreadable(format!(
            "the verify tolerance {tol} mm is not zero or a positive number below the kerf offset {k:.4} mm"
        )));
        return v;
    }
    v.verify_tol = tol;
    v.chord = job.taper_chord;
    let wire = match polyline(job.path, job.taper) {
        Ok(p) => p,
        Err(why) => {
            v.findings.push(WireFinding::Unreadable(why));
            return v;
        }
    };
    if job.side == CutSide::Open {
        if !job.skim_offsets.is_empty() {
            v.findings.push(WireFinding::Unreadable(
                "an open run states skim passes - an open run is cut once, from the air to its run-out".into(),
            ));
            return v;
        }
        open(job, &wire, k, &mut v);
        return v;
    }
    // A tabbed cut's skims (#538 x #539) are not cut: the hold law
    // reads one pass's main cut and its cut-off.
    if job.cutoff_from.is_some() && wire.has_arcs() {
        v.findings.push(WireFinding::Unreadable("a tabbed cut holds arc moves - tabs are read on straight moves only".into()));
        return v;
    }
    if job.cutoff_from.is_some() && !job.skim_offsets.is_empty() {
        v.findings.push(WireFinding::Unreadable("a tabbed cut states skim passes - skims are cut on an untabbed profile only".into()));
        return v;
    }
    let passes = match passes::split(&wire.pts, k, &job.skim_offsets) {
        Ok(p) => p,
        Err(why) => {
            v.findings.push(WireFinding::Unreadable(why));
            return v;
        }
    };
    // The threading point clears the widest kerf any pass burns.
    let k = passes::widest(&passes);
    let Some(start) = job.start else {
        v.findings.push(WireFinding::NoStart);
        return v;
    };
    let Some(&first) = wire.pts.first() else {
        v.findings.push(WireFinding::Unreadable("the path has no moves".into()));
        return v;
    };
    if dist::pp(first, start) > 1e-6 {
        v.findings.push(WireFinding::StartMoved { stated: start, path: first });
    }
    // The die's reading of the model (module head), decided once for
    // the whole height: the start hole stands in the model at some plane.
    let opening = job.side == CutSide::Die && job.sections.iter().any(|s| s.part.contains(start));
    // The threading point's worst plane, and the first plane that gouges
    // (with the deepest reading of any plane, #1170).
    let mut worst: Option<(f64, f64)> = None;
    let mut gouge: Option<WireFinding> = None;
    for s in &job.sections {
        let p = product(s, opening);
        let (clear, _) = dist::clearance(&p, start, start);
        if worst.is_none_or(|(c, _)| clear < c) {
            worst = Some((clear, s.z));
        }
        let (found, under) = passes::gouge_at(&p, &wire, &passes, s.z, tol);
        gouge = WireFinding::first_gouge(gouge, found);
        v.under = WireUnder::deeper(v.under, under);
    }
    if let Some((clear, z)) = worst.filter(|(c, _)| *c < k - KERF_TOL) {
        v.findings.push(WireFinding::Threading { z, at: start, clear, need: k });
    }
    if job.taper_chord {
        gouge = gouge.map(WireFinding::under_chord);
    }
    // A burnout is cut vertical and never states a taper; nor does a
    // vertical cut of arcs read from the facets (#3215).
    if job.taper_chord && job.taper {
        v.findings.extend(undercut::judge(&job.sections, |s| product(s, opening), &wire, &passes, tol));
    }
    v.findings.extend(gouge);
    slug::judge(job, &wire, &passes, opening, &mut v);
    v
}

/// An open run (#1109, module head): the kerf law against the part at
/// every height, both ends clear of the stock; no threading, no slug.
fn open(job: &WireJob<'_>, wire: &Wire, k: f64, v: &mut WireVerdict) {
    v.skipped = Some(OPEN_SKIP);
    let mut worst: Option<(f64, f64, Point2)> = None;
    let mut gouge: Option<WireFinding> = None;
    for s in &job.sections {
        let (found, under) = gouge_at(&Product::Part(&s.part), wire, k, s.z, job.verify_tol);
        gouge = WireFinding::first_gouge(gouge, found);
        v.under = WireUnder::deeper(v.under, under);
        let Some(stock) = s.stock.as_ref() else { continue };
        // The ends where the wire stands at this height (a leaning
        // wire's ends move with the plane).
        let at = wire.at(s.z);
        for e in [at.first(), at.last()].into_iter().flatten() {
            let (clear, _) = dist::clearance(&Product::Part(stock), *e, *e);
            if worst.is_none_or(|(c, _, _)| clear < c) {
                worst = Some((clear, s.z, *e));
            }
        }
    }
    if let Some((clear, z, at)) = worst.filter(|(c, _, _)| *c < k - KERF_TOL) {
        v.findings.push(WireFinding::OpenEndInStock { z, at, clear, need: k });
    }
    v.findings.extend(if job.taper_chord { gouge.map(WireFinding::under_chord) } else { gouge });
}

fn product(s: &WireSection, opening: bool) -> Product<'_> {
    if opening {
        Product::Around { opening: &s.part, stock: s.stock.as_ref() }
    } else {
        Product::Part(&s.part)
    }
}
