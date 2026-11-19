//! In-pass chip-break dips (issue #419): inside one roughing pass the
//! insert lifts radially by a dip height and comes back, a few times
//! along the thread, so the chip breaks. The lead is never touched.
//! Every half-turn of a dipped pass still advances exactly half a lead,
//! so the station a spindle angle meets (station mod lead, the helix
//! phase) is the plain pass's. Only the radius at each half-turn end
//! changes. A half-turn whose two ends stand at different radii is the
//! CL's variable-radius synchronized segment: on a continuous-threading
//! control it is one `G32 X Z F` block, a taper segment of the chain.
//!
//! Safety laws:
//! - **Never deeper than the pass.** Every radius is the pass's tip
//!   plus a lift of 0 or more. The lifted V stands on the pass's own
//!   station, and its tip is higher, so it lies inside the V the pass
//!   cuts. A dip can only leave material, never take more.
//! - **Never steeper than 45 degrees.** Each segment's diameter change
//!   is smaller than its axial travel. Past 45 degrees a Fanuc control
//!   reads F as the X lead (B-64304EN-1 §3.2), which would change the
//!   Z feed and move the helix. Checking the diameter keeps the rule
//!   whether a control measures the angle in radius or in diameter.
//! - **Never in the first or the last lead, never on a pass at the
//!   root.** The first lead is where the pass comes up to speed, and
//!   the last is where it leaves. The finishing and spring passes cut
//!   the finished flank, so a dip there would leave its wedge on the
//!   part. Repeats of a depth (#418) clean that depth, so they cut
//!   whole too.
//! - **No more than the pass's own depth of cut.** The flanks lift
//!   onto the surface the pass before left, not into air above it (the
//!   #363 research's rule).
//!
//! The verdict ([`dip_warnings`]) re-reads each laid chain against
//! these laws, so a path that breaks one is convicted. A convicted
//! path holds the post.
//!
//! Provenance: continuous thread cutting as FANUC Series 0i-TD USER'S
//! MANUAL (For Lathe System) B-64304EN-1 states it: §3.4 consecutive
//! threading blocks keep spindle synchronism at the block joints; §3.2
//! WARNING 6, a threading block that follows a threading block starts
//! at once, without waiting for the one-rotation signal; §3.2, a G32
//! may cut a taper, with the lead along Z up to 45 degrees. Also the
//! Haas lathe G32 reference (sync holds for each following G32 line).
//! In-pass interruption for chip control as tooling makers describe it
//! (behaviour only). The layout, the containment proof and the verdict
//! are our own. No vendor post, output or CAM strategy consulted.

use crate::cl::{MoveKind, Toolpath};
use crate::machining2d::GenError;
use crate::verify::Warning;

use super::infeed::Pass;
use super::profile::Frame;
use super::revolve::HAIR;

/// The smallest dip a post can tell from lattice noise, mm radial (the
/// stream writes whole microns).
pub const MIN_DIP_HEIGHT: f64 = 0.01;

/// Half-turns in a lead: the first and the last lead stay whole.
const LEAD_HALVES: usize = 2;

/// How far under 45 degrees a dip ramp stays, mm of diameter change
/// per half-turn below its axial travel.
const SLOPE_MARGIN: f64 = 0.01;

/// How close two lengths must be to read as the same, mm. This is
/// float noise on the half-micron stream lattice, far below any real
/// dip.
const EPS: f64 = 1e-6;

/// The chip-break dips of a thread's roughing passes. `interval` 0
/// (the default) lays none.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ThreadDips {
    /// Axial distance from one dip's start to the next, mm. It is
    /// rounded up to whole half-turns, at least two leads, and at least
    /// one lead longer than the dip. 0 means no dips.
    pub interval: f64,
    /// Radial lift, mm. At most the depth of cut of every pass that
    /// dips.
    pub height: f64,
    /// Axial length of one dip, out-ramp to in-ramp, mm. It is rounded
    /// up to whole half-turns, and is never shorter than its two ramps.
    /// 0 means the ramps alone.
    pub length: f64,
}

impl Default for ThreadDips {
    fn default() -> Self {
        ThreadDips { interval: 0.0, height: 0.05, length: 0.0 }
    }
}

/// Where the dips stand in a pass of `n` half-turns, in half-turns.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Layout {
    /// Half-turns in each ramp.
    ramp: usize,
    /// Half-turns in a whole dip, both ramps included.
    span: usize,
    /// Half-turns from one dip's start to the next.
    every: usize,
}

impl ThreadDips {
    /// Does the plan lay dips at all?
    #[must_use]
    pub fn is_on(&self) -> bool {
        self.interval > 0.0
    }

    /// Why these dips cannot ride a thread whose first pass is
    /// `first_depth` deep (`None`: they can, as far as the numbers tell
    /// before the passes are laid). Off is always fine.
    #[must_use]
    pub fn refusal(&self, first_depth: f64) -> Option<String> {
        if !self.is_on() {
            return None;
        }
        if !self.interval.is_finite() {
            return Some(format!("a chip-break dip interval of {} is not a length", self.interval));
        }
        if !(self.height.is_finite() && self.height >= MIN_DIP_HEIGHT) {
            return Some(format!(
                "a chip-break dip {} high is too small for a post to tell apart from the \
                 micron grid - lift at least {MIN_DIP_HEIGHT}",
                self.height
            ));
        }
        if !(self.length.is_finite() && self.length >= 0.0) {
            return Some(format!("a chip-break dip length of {} is not a length - give 0 or more", self.length));
        }
        if first_depth.is_finite() && self.height > first_depth + EPS {
            return Some(format!(
                "a chip-break dip {:.3} high lifts past the first pass's own {:.3} depth of cut - \
                 a dip lifts the flanks onto the last pass's surface, never above it",
                self.height, first_depth
            ));
        }
        None
    }

    /// The tool-free door: every number finite and in range.
    #[must_use]
    pub fn is_sane(&self) -> bool {
        self.interval.is_finite()
            && self.interval >= 0.0
            && self.height.is_finite()
            && self.height >= 0.0
            && self.length.is_finite()
            && self.length >= 0.0
    }

    fn layout(&self, pitch: f64, n: usize) -> Result<Layout, GenError> {
        let h = pitch * 0.5;
        let halves = |v: f64| -> usize {
            let q = (v / h - 1e-9).ceil();
            if q.is_finite() && q > 0.0 {
                q.min(usize::MAX as f64 / 4.0) as usize
            } else {
                0
            }
        };
        let too_long = || {
            GenError::Invalid(format!(
                "the thread is too short for a chip-break dip every {:.3} - the first and the \
                 last lead stay whole, so the dip has nowhere to stand; shorten the interval \
                 or turn the dips off",
                self.interval
            ))
        };
        // The diameter change of a ramp half-turn under its axial
        // travel by at least SLOPE_MARGIN: 2 * height / ramp <= h -
        // margin, the fewest half-turns that keep it so. The margin
        // keeps the post's own 45-degree check (on the micron grid)
        // on the same side as the kernel's.
        let room = h - SLOPE_MARGIN;
        let steep = (2.0 * self.height / room).floor() + 1.0;
        if !(room > 0.0 && steep.is_finite() && steep >= 1.0 && steep <= n as f64) {
            return Err(too_long());
        }
        let ramp = steep as usize;
        let span = halves(self.length).max(ramp.saturating_mul(2));
        let least = span.saturating_add(LEAD_HALVES).max(2 * LEAD_HALVES);
        let every = halves(self.interval);
        if every < least {
            return Err(GenError::Invalid(format!(
                "chip-break dips every {:.3} leave less than a lead of whole thread between \
                 dips {:.3} long - the shortest interval here is {:.3}",
                self.interval,
                span as f64 * h,
                least as f64 * h
            )));
        }
        if every.saturating_add(span).saturating_add(LEAD_HALVES) > n {
            return Err(too_long());
        }
        Ok(Layout { ramp, span, every })
    }
}

/// The radius at every half-turn end of every pass: `out[k][0]` is pass
/// `k`'s tip (the plunge), `out[k][i]` the end of half-turn `i` of `n`.
/// A pass that does not dip keeps its tip throughout, bit for bit, so
/// a plan with dips off lays the very path it laid before #419.
/// Refused by name: dips on with no roughing pass to carry them, a dip
/// higher than a dipped pass's depth of cut, a thread too short for a
/// dip outside its first and last lead, and an interval that leaves no
/// whole lead between dips.
pub(super) fn radii(
    dips: &ThreadDips,
    passes: &[Pass],
    ref_top: f64,
    root: f64,
    pitch: f64,
    n: usize,
) -> Result<Vec<Vec<f64>>, GenError> {
    let plain = || passes.iter().map(|p| vec![p.tip; n + 1]).collect();
    if !dips.is_on() {
        return Ok(plain());
    }
    if let Some(why) = dips.refusal(f64::INFINITY) {
        return Err(GenError::Invalid(why));
    }
    let mut out: Vec<Vec<f64>> = plain();
    let dipped: Vec<(usize, f64)> = passes
        .iter()
        .enumerate()
        .filter(|&(k, p)| p.tip > root + HAIR && (k == 0 || passes.get(k - 1) != Some(p)))
        .map(|(k, p)| {
            let above = k.checked_sub(1).and_then(|j| passes.get(j)).map_or(ref_top, |q| q.tip);
            (k, above - p.tip)
        })
        .collect();
    if dipped.is_empty() {
        return Err(GenError::Invalid(
            "chip-break dips ride the roughing passes, and this thread has none above the root - \
             the first pass is the finishing pass; turn the dips off or take a shallower first pass"
                .into(),
        ));
    }
    let shallowest = dipped.iter().map(|&(_, d)| d).fold(f64::INFINITY, f64::min);
    if dips.height > shallowest + EPS {
        return Err(GenError::Invalid(format!(
            "a chip-break dip {:.3} high lifts past a roughing pass's own {:.3} depth of cut - a \
             dip lifts the flanks onto the last pass's surface, never above it; keep the dip at \
             most {:.3}",
            dips.height, shallowest, shallowest
        )));
    }
    let lay = dips.layout(pitch, n)?;
    for &(k, _) in &dipped {
        let (Some(row), Some(p)) = (out.get_mut(k), passes.get(k)) else {
            continue;
        };
        let mut j = lay.every;
        while j + lay.span + LEAD_HALVES <= n {
            for q in 1..=lay.span {
                let up = if q <= lay.ramp {
                    q
                } else if q > lay.span - lay.ramp {
                    lay.span - q
                } else {
                    lay.ramp
                };
                if let Some(r) = row.get_mut(j + q) {
                    *r = p.tip + dips.height * (up as f64) / (lay.ramp as f64);
                }
            }
            j += lay.every;
        }
    }
    Ok(out)
}

/// A dip is posted only where the record marks it (the stream's
/// ChipBreakDips gate): dips on and no cycle record attached is refused
/// by name, so a dip is never laid unmarked.
pub(super) fn require_record(dips: &ThreadDips, path: &Toolpath) -> Result<(), GenError> {
    if dips.is_on() && path.threads.is_empty() {
        return Err(GenError::Invalid(
            "chip-break dips need the thread cycle record that marks them, and the passes did \
             not lay as planned - turn the dips off"
                .into(),
        ));
    }
    Ok(())
}

/// One synchronized chain of a laid path: the plunge that opens it and
/// the half-turns that follow it unbroken.
struct Chain {
    tip: f64,
    /// (move index, station and radius at the start, then at the end).
    turns: Vec<(usize, f64, f64, f64, f64)>,
}

/// The dip laws over a laid thread path, read off its own geometry. A
/// chain opens at a straight feed (the plunge) and runs through the
/// half-turns after it. Every half-turn must advance the station the
/// chain's first one does (the phase law). No half-turn may stand
/// deeper than the plunge. A half-turn whose radius changes must not
/// be steeper than 45 degrees, must stand outside the chain's first and
/// last lead, and must not stand on a pass at the deepest tip (the
/// finishing and spring passes). Each breach is a gouge at its move:
/// it holds the post.
pub(super) fn dip_warnings(f: &Frame, path: &Toolpath) -> Vec<Warning> {
    let mut chains: Vec<Chain> = Vec::new();
    let mut open: Option<Chain> = None;
    for (i, m) in path.moves.iter().enumerate() {
        if m.kind == MoveKind::Rapid {
            chains.extend(open.take());
            continue;
        }
        let (sa, ra) = f.sr(m.from);
        let (sb, rb) = f.sr(m.to);
        if m.arc.is_some() {
            if let Some(c) = open.as_mut() {
                c.turns.push((i, sa, ra, sb, rb));
            }
        } else {
            chains.extend(open.take());
            open = Some(Chain { tip: rb, turns: Vec::new() });
        }
    }
    chains.extend(open);
    let deepest = chains
        .iter()
        .filter(|c| !c.turns.is_empty())
        .map(|c| c.tip)
        .fold(f64::INFINITY, f64::min);
    let mut out = Vec::new();
    for c in &chains {
        let n = c.turns.len();
        let Some(&(_, s0, _, s1, _)) = c.turns.first() else {
            continue;
        };
        let step = s1 - s0;
        for (k, &(at, sa, ra, sb, rb)) in c.turns.iter().enumerate() {
            let k = k + 1;
            let phase_moved = ((sb - sa) - step).abs() > EPS;
            let deeper = ra.min(rb) < c.tip - EPS;
            let dip = (rb - ra).abs() > EPS;
            let bad_dip = dip
                && (2.0 * (rb - ra).abs() >= (sb - sa).abs()
                    || k <= LEAD_HALVES
                    || k + LEAD_HALVES > n
                    || c.tip <= deepest + EPS);
            if phase_moved || deeper || bad_dip {
                out.push(Warning::Gouge { at });
            }
        }
    }
    out
}
