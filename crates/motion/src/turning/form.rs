//! Form turning (M5.2 E2): the gauge's crown sunk once per modeled
//! formed surface, judged as the arc floor it leaves in the meridian.
//!
//! A form tool is a blade whose edge is a curve: plunge it and the
//! cut standing in the meridian is that curve revolved - a crown of
//! radius `R` leaves the sagitta `R - sqrt(R^2 - d^2)`, the thread's
//! V leaves its linear [`revolve::Floor::Wave`], and stage D's
//! generalization said the scan reads both the same way. This kernel
//! adds no verdict law: it states where the crown stands and how
//! deep, and [`revolve`]'s lattice judges it with the CROWN floor
//! and the crest law (a floor standing above the blank measures
//! nothing - the shell owns what the gauge never reaches).
//!
//! Like threading, a form op is a promise over a MODELED formed
//! surface: the valley the gauge forms is in the model, and the
//! kernel centers each crown on the model's own valley run. The
//! plunge depth is not authored at all - it is the deepest position
//! that cuts nothing the model keeps (`tip = max(E - sag)` over the
//! chords the band overlaps, read at their ends - the scan's own
//! chord-overlap law, and since a chord minus a convex arc is
//! concave the maximum lives AT those ends): gouge-free by
//! construction, never by luck. A gauge that cannot match the
//! valley's shape stands shallow and leaves REST, which is the
//! verdict's to report - a model with no valley at all has nothing
//! to state the center, and that refusal is this door's namesake.
//!
//! Provenance: radius-form and shank-form turning as a single
//! plunge per formed annulus, and the sagitta as a form tool's
//! meridian footprint, are machine-tool handbook chapter material.
//! The deepest-legal-plunge depth law and the crown clause on
//! stage D's bendable floor are our own, on the same silhouette the
//! blade kinds ride. No vendor strategy consulted.

use crate::cl::{MoveKind, PathBuilder, Toolpath};
use crate::geometry::Triangle;
use crate::machining2d::GenError;
use crate::verify::Warning;

use super::blade;
use super::profile;
use super::revolve;
use super::stock::StockEnv;

/// Form turning on a crown gauge: the window the formed surfaces
/// stand in, the feeds, and the gauge's ground profile - its crown
/// radius `R` and band width `band_width` - which is TOOL steel and
/// rides here from the tool row, the blade's width law again.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FormParams {
    pub tolerance: f64,
    pub clearance_z: f64,
    pub top_z: f64,
    pub bottom_z: f64,
    pub feed: f64,
    pub plunge_feed: f64,
    pub band_width: f64,
    pub crown_radius: f64,
}

/// The crown's rise at distance `d` from its center: the sagitta,
/// clamped to the arc's own reach (past `R` there is no steel, and
/// the door below says no band stands there).
fn sag(r: f64, d: f64) -> f64 {
    let d = d.min(r);
    r - (r * r - d * d).max(0.0).sqrt()
}

/// Form turning: one radial plunge per modeled valley in the
/// window, each crown centered on its valley's own midpoint and
/// sunk to the deepest position whose floor stands at or above
/// every chord the band passes over.
pub fn form(facets: &[Triangle], p: &FormParams) -> Result<Toolpath, GenError> {
    form_in(facets, p, None)
}

/// [`form`] inside a setup stock (#130): every rapid clears the
/// bar's OD, not only the part's.
pub fn form_in(facets: &[Triangle], p: &FormParams, stock: Option<StockEnv>) -> Result<Toolpath, GenError> {
    if !p.crown_radius.is_finite() || p.crown_radius <= 0.0 {
        return Err(GenError::Invalid(format!(
            "a crown of radius {:.3} is no arc at all - a flat floor is a wide blade, \
             and an infinite one is the cylinder a profile ride already turns; mount a \
             form GAUGE with a ground crown",
            p.crown_radius
        )));
    }
    let (f, pts, sg, win, _dom, rho_clear) = blade::setup(
        facets,
        p.clearance_z,
        p.top_z,
        p.bottom_z,
        p.band_width,
        p.tolerance,
        "form",
        "form gauge",
        stock,
    )?;
    let half = p.band_width * 0.5;
    if half > p.crown_radius {
        return Err(GenError::Invalid(format!(
            "a {:.3} wide band is ground past the reach of its own {:.3} crown - the arc \
             runs out {:.3} off center, so the band's corners are no steel at all; grind \
             narrower or crown shallower",
            p.band_width,
            p.crown_radius,
            p.crown_radius
        )));
    }
    let (s_lo, s_hi) = win;
    let ref_top = revolve::span_max(&sg, s_lo, s_hi);
    // The valleys are the model's, never a number's: every run of
    // silhouette samples standing at the window's own low floor
    // (threading's phase law, one center per run instead of one
    // lattice) is a formed surface this gauge is asked to stamp.
    let model_lo = pts
        .iter()
        .filter(|(s, _)| *s >= s_lo && *s <= s_hi)
        .map(|&(_, r)| r)
        .fold(ref_top, f64::min);
    if model_lo > ref_top - p.tolerance {
        return Err(GenError::Invalid(format!(
            "nothing to form - no surface stands {:.3} down from the bar in this window: \
             a form op promises a MODELED formed surface (a revolved crown section); a \
             smooth bar has none to find",
            p.tolerance
        )));
    }
    let mut valleys: Vec<(f64, f64)> = Vec::new();
    let mut run: Option<(f64, f64)> = None;
    for &(s, r) in &pts {
        if s < s_lo || s > s_hi {
            continue;
        }
        if r <= model_lo + p.tolerance {
            match &mut run {
                None => run = Some((s, s)),
                Some(v) => v.1 = s,
            }
        } else if let Some(v) = run.take() {
            valleys.push(v);
        }
    }
    if let Some(v) = run {
        valleys.push(v);
    }
    if valleys.len() as f64 > blade::MAX_RUNGS {
        return Err(GenError::Invalid(format!(
            "the window holds {} formed surfaces - more than this kernel will program in \
             one operation; split the window",
            valleys.len()
        )));
    }
    let s_out = f.station_of(p.clearance_z);
    let mut b = PathBuilder::new(f.point(rho_clear, s_out));
    for (v_lo, v_hi) in valleys {
        let c = (v_lo + v_hi) * 0.5;
        let (lo, hi) = (c - half, c + half);
        // Deepest legal plunge: the floor must stand at or above
        // the material at every event the SCAN judges - the chords
        // overlapping the band for real (a graze of HAIR or less
        // owns no surface, the tangency promise) and at the crown's
        // own center, which the lattice judges too. One law with
        // the verdict, so the gauge may
        // sink as deep as the verdict will call flush and not a
        // micron deeper: tip >= chord - sag everywhere, so
        // floor = tip + sag never dips under a chord, and E - sag
        // is concave on a chord, so its maximum lives AT the ends.
        let mut tip = f64::NEG_INFINITY;
        for &(s0, s1, a, b) in &sg {
            let (x0, x1) = (s0.max(lo), s1.min(hi));
            if x1 - x0 <= revolve::HAIR {
                continue;
            }
            for x in [x0, x1, if x0 < c && c < x1 { c } else { x0 }] {
                tip = tip.max(a + b * (x - s0) - sag(p.crown_radius, x - c));
            }
        }
        if !(tip.is_finite() && tip > 0.0) {
            return Err(GenError::Invalid(format!(
                "the crown that fits the valley at station {c:.3} stands on the axis or \
                 through it - that is a hole, and holes are boring work"
            ))
            // #871: a part-way refusal hands back the valleys so far.
            .with_ghost_moves(b.moves()));
        }
        b.push(MoveKind::Rapid, f.point(rho_clear, c), None);
        b.push(MoveKind::Feed, f.point(tip, c), Some(p.plunge_feed));
        b.push(MoveKind::Rapid, f.point(rho_clear, c), None);
    }
    b.push(MoveKind::Rapid, f.point(rho_clear, s_out), None);
    Ok(b.finish())
}

/// The form verdict over a generated path: the same meridian scan
/// the blade and the thread ride, with the CROWN floor (the gauge's
/// arc, not a boxcar, says what stands between the band's edges)
/// and the crest law (a crown floor above the blank measures
/// nothing - the shell owns what the gauge never reaches). A model
/// this cannot even frame reports as a gouge at the first cut - a
/// judge that cannot measure never reports silence.
pub fn form_warnings(facets: &[Triangle], path: &Toolpath, band_width: f64, crown_radius: f64, tol: f64) -> Vec<Warning> {
    form_warnings_excess(facets, path, band_width, crown_radius, tol, 0.0)
}

/// [`form_warnings`] on a model imported coarser than promised (#904):
/// the crown scan's gouge clause gives the model's `excess` chord
/// error back out of `tol` while a positive tolerance is left
/// ([`revolve::gouge_tol_on`]). An `excess` of 0 is [`form_warnings`]
/// exactly.
pub fn form_warnings_excess(
    facets: &[Triangle],
    path: &Toolpath,
    band_width: f64,
    crown_radius: f64,
    tol: f64,
    excess: f64,
) -> Vec<Warning> {
    let first_cut = || Warning::Gouge {
        at: path
            .moves
            .iter()
            .position(|m| m.kind != MoveKind::Rapid)
            .unwrap_or(0),
    };
    if facets.is_empty()
        || !(band_width > 0.0 && band_width.is_finite())
        || !(crown_radius > 0.0 && crown_radius.is_finite())
        || !(tol > 0.0 && tol.is_finite())
    {
        return vec![first_cut()];
    }
    let f = match profile::Frame::about(facets) {
        Ok(f) => f,
        Err(_) => return vec![first_cut()],
    };
    let pts = match profile::silhouette(facets, &f, tol * 0.25) {
        Ok(p) => p,
        Err(_) => return vec![first_cut()],
    };
    let moves: Vec<revolve::BandMove> = path
        .moves
        .iter()
        .enumerate()
        .filter(|(_, m)| m.kind == MoveKind::Feed)
        .map(|(i, m)| {
            let (sa, ra) = f.sr(m.from);
            let (sb, rb) = f.sr(m.to);
            revolve::BandMove {
                a: sa,
                b: sb,
                tip: ra.min(rb),
                at: i,
                floor: revolve::Floor::Crown(crown_radius),
                half: band_width * 0.5,
            }
        })
        .collect();
    // The clip's blank level is the model's own tallest claim, not
    // the window's span_max: this band sits INSIDE the bar, and a
    // crown standing off the floor's edges must rest out loud even
    // where the window never sees the skin it cannot reach.
    let crest = pts.iter().map(|&(_, r)| r).fold(0.0f64, f64::max);
    revolve::judge(&pts, moves, (tol, excess), 0.0, true, Some(crest))
}
