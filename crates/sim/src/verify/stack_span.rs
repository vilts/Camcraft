//! The tool stack's true z span along a move, for the fixture laws
//! (c2) and (d2) (#424, #425). A band of the stack (flute, shank,
//! holder, the spindle housing) rides the tip: over a move it sweeps
//! every z from its window at the low end to its window at the high
//! end. Law (c2) used to stand every band at the move's deepest z, so
//! on a ramp or a plunge the top of the stack at the high end went
//! unjudged (#425); law (d2) swept only the flute disc along an
//! upright rapid, so the shank, holder and housing were never judged
//! on a rapid at all (#424).
//!
//! The window here, per straight piece of the tip path, is
//! `[min(floor, piece low) + lo, piece high + hi]`: the band's extent
//! at both ends, min to max, on the piece's own z (an arc's chords,
//! each at its own height - [`Move::segments3`]). `floor` is the
//! law's historical bottom (the move's lower end for (c2)), so the
//! window only ever GROWS over the one the law always read: every old
//! verdict stands, a level move's window is the old one sum for sum,
//! and the new hits are steel the stack really reaches at the high end.
//! An enclosing window over-reports a graze on a long ramp, never hides
//! a crash - the bands law's own precedent.
//!
//! An arc's chords are padded by what its circle adds to them (#491,
//! [`super::arc_bulge`]): each band's radius grows by the horizontal
//! part of the chord's deviation (the sagitta, plus the lattice pad),
//! and a vertical arc's window is its true z range between the joints.
//! A straight move pads nothing.
//!
//! The z test stays [`crate::fixture::band_hits_box`]'s: overlap must
//! be strict beyond tolerance. So the strict-dip silences hold for the
//! whole stack - a retract off a mounting face (the tip leaving
//! exactly the box top) keeps every band at or above the steel, and
//! flush contact is not a crash.
//!
//! Provenance: the swept volume of a coaxial cylinder carried along a
//! straight segment, enclosed by its end windows - elementary solid
//! geometry; the band/box predicate is the crate's own
//! (`crate::fixture`). Our own design; no vendor source consulted.

use crate::cl::Move;
use crate::fixture::{band_hits_box, FixtureBox};
use crate::tools::ToolGeom;
use camcraft_tools::holder_profile::HolderStack;

use super::arc_bulge::Bulge;
use super::types::FixturePart;

/// One straight piece of a move's tip path: its XY ends and the z the
/// tip spans over it, already floored by the law's historical bottom,
/// and how far the true path strays sideways off it (`pad`, zero on a
/// straight move). A non-finite z or pad fails closed (the window or
/// radius is NaN, which the band predicate reads as a hit).
#[derive(Clone, Copy, Debug)]
pub(super) struct Piece {
    ax: f64,
    ay: f64,
    bx: f64,
    by: f64,
    z_lo: f64,
    z_hi: f64,
    pad: f64,
}

/// The move's tip path as pieces: itself for a straight move, its 3-D
/// chords for an arc (the same XY as [`Move::chords`]), each spanning
/// `[min(floor, its low end), its high end]` in z - for a vertical arc,
/// its true z range between the joints ([`Bulge::z_span`]) - and
/// padded sideways by the arc's bulge ([`Bulge::horizontal`]).
pub(super) fn pieces(m: &Move, chord_tol: f64, floor: f64) -> Vec<Piece> {
    let segs = m.segments3(chord_tol);
    let bulge = Bulge::of(m, segs.len());
    segs.into_iter()
        .enumerate()
        .map(|(j, (p, q))| {
            let (lo, hi) = bulge.z_span(j, p, q);
            let finite = p.z.is_finite() && q.z.is_finite() && floor.is_finite() && lo.is_finite() && hi.is_finite();
            Piece {
                ax: p.x,
                ay: p.y,
                bx: q.x,
                by: q.y,
                z_lo: if finite { floor.min(lo) } else { f64::NAN },
                z_hi: if finite { hi } else { f64::NAN },
                pad: bulge.horizontal(p, q),
            }
        })
        .collect()
}

/// Does the band `above..above + len` (radius `r`) strike any box
/// anywhere along the pieces? `lean` (a non-vertical axis) turns the
/// band's column into the leaning enclosure of law (c2): shifted to the
/// band's mid-height point on the axis, widened by the half-length's
/// spread, its z planes the axis reach at both window ends plus the
/// disc rim's exact dip R·sin θ (#1303). Upright, the window is the bare z span. Either way
/// the radius takes the piece's arc pad. A level straight piece
/// reproduces the law's historical sums exactly.
pub(super) fn band_struck(
    pieces: &[Piece],
    lean: Option<[f64; 3]>,
    above: f64,
    r: f64,
    len: f64,
    fixtures: &[FixtureBox],
) -> bool {
    fixtures.iter().any(|b| {
        pieces.iter().any(|pc| {
            let (offx, offy, r_use, bz0, bz1) = match lean {
                None => (0.0, 0.0, r + pc.pad, pc.z_lo + above, pc.z_hi + above + len),
                Some(a) => {
                    // The one leaning column ([`super::tilt_band`],
                    // #1303): the piece's z span (`z_lo <= z_hi`) takes
                    // the band's axis reach at its low and high end,
                    // then the rim's exact dip R·sin θ either way.
                    let col = super::tilt_band::lean(&a, above, r, len);
                    (
                        col.off.0,
                        col.off.1,
                        col.r + pc.pad,
                        pc.z_lo + col.lo - col.dip,
                        pc.z_hi + col.hi + col.dip,
                    )
                }
            };
            band_hits_box(
                pc.ax + offx,
                pc.ay + offy,
                pc.bx + offx,
                pc.by + offy,
                bz0,
                bz1,
                r_use,
                b,
            )
        })
    })
}

/// The bands above the flute, worst first: shank, the holder cylinder,
/// then the spindle housing (radius, length) when the machine states
/// it, riding on the holder's top - each `(part, above, r, len)`, the
/// shape law (c2) builds. A modelled holder body (#368's `HolderStack`)
/// replaces the one holder cylinder here exactly as it does in (c2).
pub(super) fn upper_bands(
    tool: &ToolGeom,
    holder: &HolderStack,
    spindle: Option<(f64, f64)>,
) -> Vec<(FixturePart, f64, f64, f64)> {
    // #852: the shank as swept - a drawn neck's bands, or the cylinder.
    let mut out: Vec<_> = holder.shank_bands(tool).iter().map(|b| (FixturePart::Shank, b.above, b.r, b.len)).collect();
    out.extend(holder.bands.iter().map(|b| (FixturePart::Holder, b.above, b.r, b.len)));
    if let Some((sr, sl)) = spindle {
        out.push((FixturePart::Spindle, holder.top, sr, sl));
    }
    out
}

/// Law (d2)'s whole-stack reading of an UPRIGHT rapid (#424), after
/// its flute disc came back clear: the upper bands (`bands`, worst
/// first, named when struck), then the flute column above the tip
/// (reported as [`FixturePart::Rapid`], the tool body on a rapid) -
/// each over the rapid's true z span. Band-free tools and bands of no
/// length are no steel.
pub(super) fn upright_rapid_struck(
    m: &Move,
    tool: &ToolGeom,
    bands: &[(FixturePart, f64, f64, f64)],
    fixtures: &[FixtureBox],
) -> Option<FixturePart> {
    let pcs = pieces(m, 0.01, m.from.z.min(m.to.z));
    let upper = bands.iter().copied();
    let flute = std::iter::once((FixturePart::Rapid, 0.0, tool.radius, tool.flute_len));
    upper
        .chain(flute)
        .filter(|&(_, _, _, len)| len > 0.0 || len.is_nan())
        .find(|&(_, above, r, len)| band_struck(&pcs, None, above, r, len, fixtures))
        .map(|(part, ..)| part)
}
