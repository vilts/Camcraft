//! A modelled tool body's band stack, carried with the cutter (#835): the
//! revolved envelope (#368) or drawn spun holder (#93) the fixture laws
//! sweep and the simulation draws, handed to every law that reads the
//! cutter's stack - so the reach, solid and rotary laws judge the same
//! shape, not the single cylinder enclosing it. A band stack is the
//! stepped-cylinder reading of a solid of revolution (machine-tool
//! handbooks); our own design, no vendor source or file consulted.
//!
//! The laws here:
//! - **Never larger than the cylinder it replaces, never smaller than
//!   the body.** The bands are the envelope's own (each never smaller
//!   than the body over its span, `HolderProfile`'s law); a stack that
//!   reaches past the tool's enclosing holder or shank (the shank carried
//!   up to a raised foot may stand wider than the holder - it is steel
//!   the cylinder left out, so reading it can only add), starts in the
//!   flutes, or tops out elsewhere is not this tool's and is refused
//!   (`None`): the enclosing cylinder stands in, which can only
//!   over-report.
//! - **No body, no change.** The plain holder cylinder's stack is not
//!   carried at all, so a tool with no body reads bit-identically.
//! - **#852: a drawn neck rides the same way** ([`NeckBands`]): the
//!   shank bands a necked tool's drawn cutter states, never wider than
//!   the stated shank; no neck carries nothing.

use camcraft_tools::holder_profile::{HolderBand, HolderStack, MAX_BANDS};

use crate::tools::ToolGeom;

/// Most bands one carried stack holds: the envelope's own cap, plus the
/// shank carried up to the body's foot (`HolderProfile::stack`).
pub const BODY_BANDS_CAP: usize = MAX_BANDS + 1;
/// Float dust on the fit checks (mm).
const EPS: f64 = 1e-6;

/// The body's bands above the shank's top, bottom-up - fixed capacity,
/// so the cutter stays `Copy`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BodyBands {
    n: usize,
    b: [HolderBand; BODY_BANDS_CAP],
}

impl BodyBands {
    /// The bands of `stack` standing above the shank of `g` (the tool as
    /// the laws read it, its holder the cylinder enclosing the body), or
    /// `None` - the enclosing cylinder stands in - when `stack` is the
    /// plain holder cylinder or does not fit `g`: non-finite or negative
    /// numbers, more bands than [`BODY_BANDS_CAP`], a band reaching
    /// below the flutes' top, past the shank beside it or past the
    /// holder above it, bands that are not contiguous from the shank's
    /// top (a gap, a late start, an overlap), or a top other than the
    /// holder's.
    #[must_use]
    pub fn of(g: &ToolGeom, stack: &HolderStack) -> Option<Self> {
        // A neck (#852) rides apart (`NeckBands`): only the holder counts.
        let cyl = HolderStack::cylinder(g);
        if (stack.bands == cyl.bands && stack.top == cyl.top) || stack.bands.len() > BODY_BANDS_CAP {
            return None;
        }
        let fl = g.flute_len;
        let shank_top = fl + g.shank_len;
        let ok = |v: f64| v.is_finite() && v >= 0.0;
        if !(ok(fl) && ok(shank_top) && ok(g.shank_radius) && ok(g.holder_radius) && ok(g.holder_len)) {
            return None;
        }
        let mut out = Self { n: 0, b: [HolderBand { above: 0.0, len: 0.0, r: 0.0 }; BODY_BANDS_CAP] };
        let mut top = shank_top;
        for band in &stack.bands {
            if !(ok(band.above) && ok(band.len) && ok(band.r)) || band.above < fl - EPS {
                return None;
            }
            let hi = band.above + band.len;
            // Beside the shank the enclosing shank stands for it: the
            // part there must sit inside it.
            if band.above < shank_top && band.r > g.shank_radius + EPS {
                return None;
            }
            if hi <= shank_top + EPS {
                continue;
            }
            // Above it, the enclosing holder - or the shank carried up
            // to a body's raised foot (the stack's own gap band).
            if band.r > g.holder_radius.max(g.shank_radius) + EPS {
                return None;
            }
            // Contiguous from the shank's top: a band that starts past
            // where the last one ended (or the first past the shank's
            // top) leaves a span no band judges, one that starts below it
            // runs back down the axis. Either way the stack is not a
            // profile, and the enclosing cylinder stands.
            let above = band.above.max(shank_top);
            if (above - top).abs() > EPS {
                return None;
            }
            out.b[out.n] = HolderBand { above, len: hi - above, r: band.r };
            out.n += 1;
            top = hi;
        }
        ((top - (shank_top + g.holder_len)).abs() <= EPS && out.n > 0).then_some(out)
    }

    /// The bands, bottom-up.
    #[must_use]
    pub fn bands(&self) -> &[HolderBand] {
        &self.b[..self.n]
    }
}

/// A necked tool's shank bands (#852), carried with the cutter the way
/// [`BodyBands`] carries the holder - fixed capacity, so the cutter stays
/// `Copy`. Only ever a neck that fits its tool and narrows somewhere
/// (`HolderStack::shank_bands`): otherwise `None`, the stated shank
/// cylinder, bit-identical to a tool with no drawn cutter.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NeckBands {
    n: usize,
    b: [HolderBand; MAX_BANDS],
}

impl NeckBands {
    /// The neck `stack` carries for the shank of `g`, or `None` when the
    /// laws read the stated shank cylinder there.
    #[must_use]
    pub fn of(g: &ToolGeom, stack: &HolderStack) -> Option<Self> {
        if stack.neck.is_empty() {
            return None;
        }
        let bands = stack.shank_bands(g);
        if bands.len() > MAX_BANDS || bands == HolderStack::cylinder(g).shank_bands(g) {
            return None;
        }
        let mut out = Self { n: 0, b: [HolderBand { above: 0.0, len: 0.0, r: 0.0 }; MAX_BANDS] };
        for (slot, band) in out.b.iter_mut().zip(&bands) {
            *slot = *band;
        }
        out.n = bands.len();
        Some(out)
    }

    /// The bands, bottom-up.
    #[must_use]
    pub fn bands(&self) -> &[HolderBand] {
        &self.b[..self.n]
    }

    /// Does this neck still fit the shank of `g` exactly as carried?
    #[must_use]
    pub fn fits(&self, g: &ToolGeom) -> bool {
        let stack = HolderStack { neck: self.bands().to_vec(), ..HolderStack::cylinder(g) };
        stack.shank_bands(g) == self.bands()
    }
}
