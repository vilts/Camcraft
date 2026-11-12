//! Threading (M5.2 D): the V a synchronized spindle walks along a
//! helix, judged as the wave it leaves in the meridian.
//!
//! A threading insert is a blade cut to a V. Its footprint is no
//! longer a boxcar - at `distance` along the axis from where its tip
//! last stood, its flank stands `distance * cot(A/2)` deeper out -
//! and [`revolve`]'s scan reads exactly that bend: the same lattice,
//! the same interval law, floors now evaluated by [`revolve`]'s
//! wave clause. The centers are the LATTICE THE HELIX IMPOSED:
//! every `pitch` of axis travel the tool returns to its own
//! meridian, so the cut at station `s` is the V centered at the
//! lattice point nearest it - and a pass is one full turn per
//! pitch of synchronized spindle, spelled in CL as the helix the
//! M2.1 law already owns: XY half-turn arcs about the axis line,
//! falling `pitch / 2` of station as they cross
//! (toward the chuck, issue #629). The deepest pass
//! is the finishing one: a full-form insert (whose reach,
//! `width / 2 * cot(A/2)`, stands at or above the blank's crest -
//! the door's first question) tiles the window's whole cut with
//! that one wave, and the shallower passes before it only widen
//! the V it already owns.
//!
//! The phase is the model's, not a number's: the lattice must sit
//! so its V's land IN the model's grooves, and the model's own
//! first valley within the window states where. That makes the cut
//! require a MODELED thread - a revolved V section. A real
//! helical thread's silhouette is a flat crest (its flanks twist
//! through every phase on their way around), so it refuses by
//! name, and the verdict then judges the thread as its drawing
//! states it - profile-representation practice, the same law the
//! turned profile rides.
//!
//! Beside the passes the path carries one [`ThreadCycle`] record
//! (issue #34): the same thread as a lathe control's threading cycle
//! states it, for a post that writes the cycle instead of the passes.
//!
//! Hand and direction (issue #629): every pass runs TOWARD the chuck
//! (the free end is +Z; a pass enters at the top lattice point, in free
//! air, and falls); a right-hand thread turns clockwise seen from +Z
//! (M03's sense), a left-hand one counter-clockwise; the record says so.
//!
//! Provenance: single-point and V-form thread whirling as a
//! synchronized helix, full-form and multi-pass infeed ladders,
//! and the included-angle relation `width = 2 * depth * tan(A/2)`,
//! all machine-tool handbook threading, chapter material wherever
//! it is spelled. The wave floor as a boxcar's bent child and the
//! valley-run phase law are our own, on the same silhouette the
//! blade kinds ride. No vendor strategy consulted.

use crate::cl::{MoveKind, PathBuilder, ThreadCycle, ThreadHand, Toolpath};
use crate::geo2d::Point2;
use crate::geometry::Triangle;
use crate::machining2d::GenError;

use super::blade;
use super::revolve;
use super::stock::StockEnv;

/// The lean an XY-circle helix can live with: the circles must stand
/// in planes square to the axis, or the pass stops being one turn
/// per pitch. [`profile`]'s own door (LEAN_MIN) admits about a
/// degree for the facing kinds; a helix spends its phase on the
/// axis, so it asks for plumb steel - and says what owns the
/// tilted case.
const LEAN_MAX: f64 = 1e-6;

/// Thread cutting: a V-form insert, a modeled thread's pitch and
/// depth, and the infeed ladder that grows one groove from the
/// blank.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ThreadParams {
    pub tolerance: f64,
    pub clearance_z: f64,
    pub top_z: f64,
    pub bottom_z: f64,
    pub pitch: f64,
    pub depth: f64,
    pub infeed: f64,
    pub feed: f64,
    pub plunge_feed: f64,
    /// The insert's V opening width at its crest line.
    pub insert_width: f64,
    /// The flanks' INCLUDED angle, degrees.
    pub form_angle: f64,
    /// The hand the helix winds (issue #629); the passes always run
    /// toward the chuck (-Z), so this alone sets the arcs' sense.
    pub hand: ThreadHand,
}

/// Threading: ladder the V from a hair under the blank's crest
/// down to the root, one helical pass per rung, each pass a full
/// turn per pitch across the window's lattice of turns.
pub fn thread(facets: &[Triangle], p: &ThreadParams) -> Result<Toolpath, GenError> {
    thread_in(facets, p, None)
}

/// [`thread`] inside a setup stock (#130): every rapid clears the
/// bar's OD, not only the part's.
pub fn thread_in(facets: &[Triangle], p: &ThreadParams, stock: Option<StockEnv>) -> Result<Toolpath, GenError> {
    if p.pitch <= 0.0 {
        return Err(GenError::Invalid("threading needs a positive pitch".into()));
    }
    if p.depth <= 0.0 {
        return Err(GenError::Invalid("threading needs a positive thread depth".into()));
    }
    if p.infeed <= 0.0 {
        return Err(GenError::Invalid("threading needs a positive infeed".into()));
    }
    if p.depth <= p.tolerance {
        return Err(GenError::Invalid(format!(
            "a thread {:.3} deep within its own tolerance {:.3} has no valley that can \
             be told from its crest - deepen the thread or tighten the tolerance",
            p.depth, p.tolerance
        )));
    }
    if !(0.0 < p.form_angle && p.form_angle < 180.0) {
        return Err(GenError::Invalid(format!(
            "a thread form of {:.1} degrees is not a V an insert can carry - threading \
             wants the flanks' included angle, between 0 and 180",
            p.form_angle
        )));
    }
    let (f, pts, sg, win, _dom, rho_clear) = blade::setup(
        facets,
        p.clearance_z,
        p.top_z,
        p.bottom_z,
        p.insert_width,
        p.tolerance,
        "thread",
        "threading insert",
        stock,
    )?;
    if f.lean() > LEAN_MAX {
        // #677: names no spot - the bar's axis tilt, true along its whole length.
        return Err(GenError::Invalid(format!(
            "the bar's axis stands {:.1} degrees off the spindle (that is {:.0} mm over \
             100 mm of length) - threading rides XY circles square to the axis; a tilted \
             spindle is mill-turn work (M5.2 G)",
            f.lean().min(1.0).asin().to_degrees(),
            f.lean() * 100.0
        )));
    }
    let (s_lo, s_hi) = win;
    let half = p.insert_width * 0.5;
    let cot = 1.0 / (p.form_angle * 0.5).to_radians().tan();
    let ref_top = revolve::span_max(&sg, s_lo, s_hi);
    let root = ref_top - p.depth;
    if root <= 0.0 {
        return Err(GenError::Invalid(format!(
            "a thread {:.3} deep on a bar of {:.3} crest radius runs past the axis - no \
             core is left to stand between the flanks; that is a hole, and holes are \
             boring work",
            p.depth, ref_top
        )));
    }
    // The V must FIT the pitch it cuts: flanks wider than the
    // turn's own spacing would stand in the neighbouring groove's
    // steel. Narrower, or a whirling head - not this kernel.
    if p.insert_width >= p.pitch - 2.0 * p.tolerance {
        return Err(GenError::Invalid(format!(
            "a {:.3} wide insert does not fit a {:.3} pitch - the V is wider than the \
             groove it was asked to cut; its flanks would stand in the neighbouring \
             turn. Grind narrower steel for a single-point thread",
            p.insert_width, p.pitch
        )));
    }
    // The full-form door: a wave whose flanks stop short of the
    // crest leaves crests standing NO pass can touch - the verdict
    // would spend the whole thread length reporting the same rest.
    if half * cot + p.tolerance < p.depth {
        return Err(GenError::Invalid(format!(
            "a {:.3} wide insert at {:.0} degrees reaches {:.3} of the {:.3} this thread \
             is deep - a full-form insert must stand at or above the crest, or the crests \
             never get cut; single-flank passes are not this kernel's to speak",
            p.insert_width, p.form_angle, half * cot, p.depth
        )));
    }
    // The phase the MODEL states - never the requested depth's.
    // The scan is the first CONTIGUOUS run of silhouette samples
    // standing at or inside the MODEL'S OWN FLOOR within the
    // window: a ground root reads as its flat's center, a sharp V
    // as its one point; either is the groove the lattice must land
    // in. A cut too deep or too shallow for the model is the
    // verdict's to convict, not a reason the phase cannot be read;
    // a bar with no dip at all has no phase to state, and that
    // refusal is this door's namesake.
    let model_lo = pts
        .iter()
        .filter(|(s, _)| *s >= s_lo && *s <= s_hi)
        .map(|&(_, r)| r)
        .fold(ref_top, f64::min);
    let mut valley: Option<(f64, f64)> = None;
    if model_lo <= ref_top - p.tolerance {
        for &(s, r) in &pts {
            if s < s_lo || s > s_hi {
                continue;
            }
            if r <= model_lo + p.tolerance {
                match valley {
                    None => valley = Some((s, s)),
                    Some(v) => valley = Some((v.0, s)),
                }
            } else if valley.is_some() {
                break;
            }
        }
    }
    let (v_lo, v_hi) = match valley {
        Some(v) => v,
        None => {
            return Err(GenError::Invalid(format!(
                "nothing to thread - no valley {:.3} down from the bar's surface stands in \
                 the window: threading cuts a MODELED thread (a revolved V section); a \
                 smooth bar has none to find",
                p.depth
            )));
        }
    };
    let c0 = (v_lo + v_hi) * 0.5;
    // The lattice whose footprints reach the window: every center
    // whose band (center +- half) overlaps it. The valley itself is
    // such a center, so the run never comes up empty.
    let k_lo = ((s_lo - half - c0) / p.pitch).ceil();
    let k_hi = ((s_hi + half - c0) / p.pitch).floor();
    if k_hi - k_lo > blade::MAX_RUNGS {
        return Err(GenError::Invalid(format!(
            "the thread would run more than {:.0} turns - the pitch is far too fine for \
             the window, or the window is far too long for one operation",
            blade::MAX_RUNGS
        )));
    }
    let c_first = c0 + k_lo * p.pitch;
    let c_last = c0 + k_hi * p.pitch;
    let n = ((k_hi - k_lo) * 2.0) as usize;
    let tips = blade::rungs(root, ref_top, p.infeed, 0.0)?;
    let s_out = f.station_of(p.clearance_z);
    let ax = f.axis_point(c_first);
    let center = Point2::new(ax.x, ax.y);
    // Toward the chuck (#629): each pass enters at the top lattice
    // point and falls half a pitch per half-turn to the bottom one;
    // the hand alone picks the turn's sense.
    let ccw = p.hand.ccw_for(true);
    let mut b = PathBuilder::new(f.point(rho_clear, s_out));
    let first = b.moves_len();
    for &t in &tips {
        b.push(MoveKind::Rapid, f.point(rho_clear, c_last), None);
        b.push(MoveKind::Feed, f.point(t, c_last), Some(p.plunge_feed));
        for i in 1..=n {
            let s = c_last - (i as f64) * (p.pitch * 0.5);
            let to = if i % 2 == 0 {
                f.point(t, s)
            } else {
                f.antipode(t, s)
            };
            b.push_arc(MoveKind::Feed, to, Some(p.feed), center, ccw);
        }
        b.push(MoveKind::Rapid, f.point(rho_clear, c_first), None);
    }
    let last = b.moves_len().saturating_sub(1);
    b.push(MoveKind::Rapid, f.point(rho_clear, s_out), None);
    let mut path = b.finish();
    let first_depth = tips.first().map_or(p.depth, |&t| ref_top - t);
    attach_cycle(&mut path, first, last, ref_top, root, p, first_depth);
    Ok(path)
}

/// The thread-cycle record over the passes just laid (issue #34): the
/// numbers a control's threading cycle needs, read off the built path
/// so its points are the lattice the moves keep. This ladder plunges
/// radially (infeed angle 0) in equal rungs of `infeed` to the root -
/// first depth and minimum depth both the rung, no finish allowance,
/// no spring pass, a square retract. A span the builder did not lay
/// as planned gets no record and says so; the passes still post.
fn attach_cycle(
    path: &mut Toolpath,
    first: usize,
    last: usize,
    ref_top: f64,
    root: f64,
    p: &ThreadParams,
    first_depth: f64,
) {
    let (Some(m0), Some(end)) = (path.moves.get(first), last.checked_sub(1).and_then(|i| path.moves.get(i)))
    else {
        path.notes.push("threading: the passes did not lay as planned - no thread cycle record".into());
        return;
    };
    let rec = ThreadCycle {
        start: m0.to,
        end: end.to,
        major_diameter: 2.0 * ref_top,
        minor_diameter: 2.0 * root,
        lead: p.pitch,
        height: ref_top - root,
        first_depth,
        min_depth: p.infeed.min(ref_top - root),
        finish_allowance: 0.0,
        infeed_angle: 0.0,
        spring_passes: 0,
        pull_out: 0.0,
        first,
        last,
        hand: p.hand,
    };
    if m0.kind == MoveKind::Rapid && rec.is_sane(&path.moves) {
        path.threads.push(rec);
    } else {
        path.notes.push("threading: the passes did not lay as planned - no thread cycle record".into());
    }
}
