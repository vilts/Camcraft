//! The wire laws' findings in words (#534 and its children), and the
//! reading a stated verify tolerance let pass (#1170). Split from the
//! module head for the 400-line law.
//!
//! Provenance: our own words over our own laws; no vendor material.

use super::{CutSide, Slug, WireVerdict, SLUG_AREA_TOL};
use crate::geo2d::Point2;

/// The deepest kerf reading a stated verify tolerance let pass (#1170):
/// the kerf enters the model's facets by `depth` mm at `at` on the
/// plane `z`, within the tolerance.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WireUnder {
    pub depth: f64,
    pub z: f64,
    pub at: Point2,
}

impl WireUnder {
    /// The deeper of two readings.
    pub(super) fn deeper(a: Option<Self>, b: Option<Self>) -> Option<Self> {
        match (a, b) {
            (Some(a), Some(b)) => Some(if b.depth > a.depth { b } else { a }),
            (a, b) => a.or(b),
        }
    }
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
    /// The first move of the first plane that gouges. `tol` is the
    /// stated verify tolerance the reading is past (0 when none is
    /// stated) and `deepest` the deepest reading of any move at any
    /// plane - what a verify tolerance would have to cover (#1170).
    Gouge { move_no: usize, z: f64, at: Point2, depth: f64, moves: usize, tol: f64, deepest: f64 },
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
            WireFinding::Gouge { move_no, z, at, depth, moves, tol, deepest } => format!(
                "gouge: the kerf of move {move_no} enters the part by {depth:.4} mm near X{:.3} Y{:.3} at Z{z:.3}{} ({moves} move(s) gouge{})",
                at.x,
                at.y,
                if *tol > 0.0 { format!(", past the stated verify tolerance {tol:.4} mm") } else { String::new() },
                if *deepest > *depth + 5e-5 { format!("; the deepest reading at any height is {deepest:.4} mm") } else { String::new() }
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


impl WireFinding {
    /// The first plane's gouge, its `deepest` raised to a later plane's.
    pub(super) fn first_gouge(first: Option<Self>, next: Option<Self>) -> Option<Self> {
        match (first, next) {
            (Some(WireFinding::Gouge { move_no, z, at, depth, moves, tol, deepest }), Some(WireFinding::Gouge { deepest: more, .. })) => {
                Some(WireFinding::Gouge { move_no, z, at, depth, moves, tol, deepest: deepest.max(more) })
            }
            (first, next) => first.or(next),
        }
    }

    /// What a gouge with no verify tolerance stated may be (#1170): the
    /// caller says it for a cut whose surface the model tessellates.
    #[must_use]
    pub fn facets_hint(&self) -> Option<&'static str> {
        match self {
            WireFinding::Gouge { tol, .. } if *tol <= 0.0 => Some(
                "a tessellated loft's facets stand past its ruled surface by up to the chord tolerance of the model: when the deepest reading is within that, state that tolerance as the cut's verify tolerance",
            ),
            _ => None,
        }
    }
}

impl WireVerdict {
    /// The stated verify tolerance the kerf law judged under, and the
    /// deepest reading it let pass (#1170); `None` when none is stated.
    #[must_use]
    pub fn tolerance_said(&self) -> Option<String> {
        if self.verify_tol <= 0.0 {
            return None;
        }
        Some(match &self.under {
            Some(u) => format!(
                "kerf judged under the stated verify tolerance {:.4} mm: the kerf enters the model's facets by at most {:.4} mm, near X{:.3} Y{:.3} at Z{:.3}",
                self.verify_tol, u.depth, u.at.x, u.at.y, u.z
            ),
            None => format!(
                "kerf judged under the stated verify tolerance {:.4} mm: no reading needed it",
                self.verify_tol
            ),
        })
    }
}

/// The pieces a cut frees, in the report line's words.
#[must_use]
pub fn slugs_said(s: &[Slug]) -> String {
    if s.is_empty() {
        return "frees nothing".into();
    }
    s.iter()
        .map(|g| {
            format!(
                "frees {:.2} mm² near X{:.3} Y{:.3} ({})",
                g.area,
                g.at.x,
                g.at.y,
                if g.product_area > SLUG_AREA_TOL { "the part" } else { "waste" }
            )
        })
        .collect::<Vec<_>>()
        .join(", ")
}
