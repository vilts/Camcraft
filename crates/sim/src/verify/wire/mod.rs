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
//! Provenance: plain 2D distance and region geometry (point-segment
//! distance, polygon booleans and the disc sweep of `geo2d`, clipper2)
//! over the public wire EDM practice the umbrella #290 cites: the kerf
//! is the wire diameter plus the gap on both sides, the wire is
//! threaded through a start hole, and a closed cut frees the enclosed
//! region (die slug or punch). Our own code; no vendor post, control
//! manual or CAM product behaviour was consulted.

mod dist;
mod kerf;
mod passes;
mod slug;
#[cfg(test)]
mod hold_tests;
#[cfg(test)]
mod taper_tests;
#[cfg(test)]
mod tests;

use crate::cl::Toolpath;
use crate::geo2d::{Point2, Region};

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
}

/// Every finding refuses: the wire laws have no warning tier.
#[derive(Clone, Debug, PartialEq)]
pub enum WireFinding {
    /// The path cannot be read by these laws (an arc, a posed or
    /// non-planar move, no moves, a bad offset).
    Unreadable(String),
    NoStart,
    /// The path does not start at the stated start hole.
    StartMoved { stated: Point2, path: Point2 },
    /// The product stands nearer the threading point than the kerf.
    Threading { z: f64, at: Point2, clear: f64, need: f64 },
    /// An open run's end (where the wire is threaded or cut) stands in
    /// the stock or nearer it than the kerf.
    OpenEndInStock { z: f64, at: Point2, clear: f64, need: f64 },
    /// The kerf of move `move_no` (1-based) enters the product.
    Gouge { move_no: usize, z: f64, at: Point2, depth: f64, moves: usize },
    /// The closed cut frees no material.
    NothingFreed { z: f64 },
    /// The piece freed is not the one the stated side drops.
    WrongSlug { z: f64, side: CutSide, at: Point2, product_area: f64 },
    /// The cut frees a piece and nothing holds it: no tabs, no capture
    /// stated (#290 D6, #539).
    Unsupported { z: f64, side: CutSide, at: Point2 },
    /// The main cut of a tabbed path already frees a piece: the tabs do
    /// not hold it (#539).
    TabsDoNotHold { z: f64, at: Point2 },
    /// A burnout (#540) frees a piece: a core is left to drop.
    Core { z: f64, at: Point2, area: f64 },
}

impl WireFinding {
    /// The finding in words, mm.
    #[must_use]
    pub fn describe(&self) -> String {
        match self {
            WireFinding::Unreadable(why) => format!("the wire laws cannot read the path - {why}"),
            WireFinding::NoStart => {
                "no start hole is stated - the wire cannot be threaded, nothing is judged clear".into()
            }
            WireFinding::StartMoved { stated, path } => format!(
                "the path starts at X{:.3} Y{:.3}, not at the stated start hole X{:.3} Y{:.3} - regenerate it",
                path.x, path.y, stated.x, stated.y
            ),
            WireFinding::Threading { z, at, clear, need } => format!(
                "the start hole at X{:.3} Y{:.3} is not clear for threading at Z{z:.3}: the part stands {} - the wire and its gap need {need:.4} mm all round",
                at.x,
                at.y,
                if *clear <= 0.0 { "at the threading point itself".to_owned() } else { format!("{clear:.4} mm from it") }
            ),
            WireFinding::OpenEndInStock { z, at, clear, need } => format!(
                "the open run's end at X{:.3} Y{:.3} is not clear of the stock at Z{z:.3}: the stock stands {} - the wire is threaded and cut in the air, {need:.4} mm clear all round (regenerate the run against the stock as it stands)",
                at.x,
                at.y,
                if *clear <= 0.0 { "at the point itself".to_owned() } else { format!("{clear:.4} mm from it") }
            ),
            WireFinding::Gouge { move_no, z, at, depth, moves } => format!(
                "gouge: the kerf of move {move_no} enters the part by {depth:.4} mm near X{:.3} Y{:.3} at Z{z:.3} ({moves} move(s) gouge)",
                at.x, at.y
            ),
            WireFinding::NothingFreed { z } => format!(
                "the cut frees no material at Z{z:.3} - nothing drops when it closes (no closed cut, or no stock around the opening to judge the slug)"
            ),
            WireFinding::WrongSlug { z, side: CutSide::Die, at, product_area } => format!(
                "slug: the piece freed near X{:.3} Y{:.3} at Z{z:.3} holds {product_area:.3} mm² of the part - a die keeps the material outside the cut, but here the part drops",
                at.x, at.y
            ),
            WireFinding::Core { z, at, area } => format!(
                "core: the burnout frees a piece of {area:.3} mm² near X{:.3} Y{:.3} at Z{z:.3} - a burnout must leave no core to drop",
                at.x, at.y
            ),
            WireFinding::WrongSlug { z, side: CutSide::Punch, at, .. } => format!(
                "slug: the piece freed near X{:.3} Y{:.3} at Z{z:.3} holds none of the part - a punch's cut frees the punch, but here only waste drops and the part stays in the skeleton",
                at.x, at.y
            ),
            WireFinding::Unsupported { z, side, at } => format!(
                "slug: the {} freed near X{:.3} Y{:.3} at Z{z:.3} drops unsupported when the cut closes - state tabs to hold it until a tab cut-off, or state that the slug is captured",
                match side {
                    CutSide::Die => "slug",
                    CutSide::Punch => "punch",
                    CutSide::Open => "piece",
                },
                at.x,
                at.y
            ),
            WireFinding::TabsDoNotHold { z, at } => format!(
                "slug: the main cut already frees the piece near X{:.3} Y{:.3} at Z{z:.3} - the tabs do not hold it; place them on the cut that encloses it",
                at.x, at.y
            ),
            WireFinding::WrongSlug { z, side: CutSide::Open, at, .. } => format!(
                "slug: an open run frees a piece near X{:.3} Y{:.3} at Z{z:.3} - an open run closes no slug",
                at.x, at.y
            ),
        }
    }
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
    // The threading point's worst plane, and the first plane that gouges.
    let mut worst: Option<(f64, f64)> = None;
    let mut gouge: Option<WireFinding> = None;
    for s in &job.sections {
        let p = product(s, opening);
        let (clear, _) = dist::clearance(&p, start, start);
        if worst.is_none_or(|(c, _)| clear < c) {
            worst = Some((clear, s.z));
        }
        if gouge.is_none() {
            gouge = passes::gouge_at(&p, &wire, &passes, s.z);
        }
    }
    if let Some((clear, z)) = worst.filter(|(c, _)| *c < k - KERF_TOL) {
        v.findings.push(WireFinding::Threading { z, at: start, clear, need: k });
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
        if gouge.is_none() {
            gouge = gouge_at(&Product::Part(&s.part), wire, k, s.z);
        }
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
    v.findings.extend(gouge);
}

fn product(s: &WireSection, opening: bool) -> Product<'_> {
    if opening {
        Product::Around { opening: &s.part, stock: s.stock.as_ref() }
    } else {
        Product::Part(&s.part)
    }
}
