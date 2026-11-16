//! Wire EDM 4-axis contour (#537, part of #290): a closed upper profile
//! on the top plane and a closed lower profile on the lower programmed
//! plane, given separately and paired into rulings. Per D2 each move
//! ends on the lower plane at a ruling's lower point and its pose is
//! the wire toward the ruling's upper point; between two moves both
//! ends interpolate - the ruled surface.
//!
//! The pairing rule is stated, never guessed (`pairing`): by normalised
//! arc length from each profile's point nearest the start hole, or by
//! sync points - pairs of one point on each profile - with normalised
//! arc length between them. The rule lays the stated surface over the
//! two profiles as given, and the wire's own surface over their
//! parallels at the kerf offset.
//!
//! A straight wire cannot touch a twisted ruled surface along its whole
//! height (`ruled_laws`): where the two profiles turn against each
//! other the wire's section between the planes stands nearer the
//! surface than the offset on the planes. The generator never lets the
//! kerf in: it widens the offset until the ruled kerf law holds at
//! every height read, and says how much stands on the profile planes
//! for it. The widening is stated (#1170): per ruling (`ruled_widen` -
//! each ruling takes what its own twist and lean need, the default) or
//! uniform (one widening on both planes, the worst ruling's). More than
//! the stated tolerance is refused by name.
//!
//! Refused by name, never trimmed: an open profile or profiles of
//! different kinds, sync points that are unpaired, off their profile,
//! identical or crossed, an ambiguous whole-ring fan, a folding or
//! self-crossing pairing, a ruling past the machine's maximum taper, a
//! twist past the tolerance, and every refusal of the vertical cut on
//! either plane. The upper guide's U/V stroke and the guides' reach
//! are the machine check's (#536).
//!
//! Provenance: see `pairing` and `ruled_laws` (public CAGD and
//! differential-geometry texts), over the umbrella's decisions D2 and
//! D7 and #536's pose and kerf readings. Our own code; no vendor post,
//! control manual or CAM product behaviour was consulted.

use super::pairing::{pair, Anchor, Ring};
use super::ruled_laws::{clearance, crossing, folds, lean_law, Ruling, MAX_RULINGS};
use super::{laws, notes, ring, ruled_widen, start_side, WireParams, MIN_LEAD};
use crate::cl::{MoveKind, PathBuilder, Toolpath};
use crate::geo2d::{Loop, Point2, Region};
use crate::geometry::Point3;
use crate::machining2d::GenError;

/// How far (mm) a stated sync point may stand off its profile.
pub const SYNC_TOL: f64 = 0.01;
/// The widening passes before the offset is called unsettled.
const MAX_PASSES: usize = 16;
/// The ruled kerf law holds when need / clearance is within this of 1.
pub(super) const SETTLED: f64 = 1e-6;

/// How the wire is widened off a twisted surface (#1170) - stated.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum WireWidening {
    /// Each ruling stands off by what its own twist and lean need: a
    /// straight stretch stays at the kerf offset.
    #[default]
    PerRuling,
    /// One widening on both planes, the worst ruling's: both wire
    /// paths stay exact parallels of their profiles.
    Uniform,
}

/// One sync pair: a point of the upper profile and the point of the
/// lower profile the same ruling passes through. A side left `None` is
/// an unpaired sync point, refused by name.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WireSync {
    pub upper: Option<Point2>,
    pub lower: Option<Point2>,
}

/// The stated pairing rule.
#[derive(Clone, Debug, PartialEq)]
pub enum WirePairing {
    /// Normalised arc length, each profile walked counter-clockwise
    /// from its point nearest the start hole.
    ArcLength,
    /// The stated pairs, normalised arc length between neighbours.
    /// A shared endpoint states a fan; keep its pairs in span order.
    SyncPoints(Vec<WireSync>),
}

impl WirePairing {
    /// The rule in words, as the notes and refusals say it.
    #[must_use]
    pub fn word(&self) -> String {
        match self {
            WirePairing::ArcLength => "by normalised arc length from each profile's point nearest the start hole".into(),
            WirePairing::SyncPoints(s) => format!("by {} sync pair(s), normalised arc length between them", s.len()),
        }
    }

    pub(super) fn hint(&self) -> &'static str {
        match self {
            WirePairing::ArcLength => "state sync points that pair the profiles' matching corners",
            WirePairing::SyncPoints(_) => "restate the sync points so each pairs matching points of the two profiles",
        }
    }
}

/// One profile as picked: closed rings and open runs.
#[derive(Clone, Copy, Debug)]
pub struct WireProfile<'a> {
    pub loops: &'a [Loop],
    pub chains: &'a [Vec<Point2>],
}

/// The 4-axis cut's own numbers.
#[derive(Clone, Debug, PartialEq)]
pub struct WireRuled {
    /// The top plane, where the upper profile lies.
    pub top_z: f64,
    pub pairing: WirePairing,
    /// The most (mm) the wire may stand off the profiles past the kerf
    /// offset to keep clear of a twisted surface.
    pub tolerance: f64,
    /// Where the widening stands: per ruling, or uniform.
    pub widening: WireWidening,
    /// The machine's stated maximum taper, degrees; `None` = not stated.
    pub max_deg: Option<f64>,
    /// The machine's name, as the refusals say it.
    pub machine: String,
}

pub(super) fn invalid(why: String) -> GenError {
    GenError::Invalid(format!("wire contour: {why}"))
}

/// One profile's points as given, its region and its ring - or the
/// vertical cut's refusal, naming which profile.
fn one(which: &str, p: WireProfile<'_>) -> Result<(Vec<Point2>, Region, Ring), GenError> {
    let (given, region) = ring::profile(p.loops, p.chains).map_err(|e| match e {
        GenError::Invalid(w) => GenError::Invalid(format!("{w} (the {which} profile)")),
        GenError::Empty(w) => GenError::Empty(format!("{w} (the {which} profile)")),
        e => e,
    })?;
    let r = Ring::new(&given).ok_or_else(|| invalid(format!("the {which} profile has no area")))?;
    Ok((given, region, r))
}

/// The anchors as points of the two profiles `(upper, lower, number)`.
fn anchors(r: &WireRuled, gu: &Ring, gl: &Ring, start: Point2) -> Result<Vec<(Point2, Point2, usize)>, GenError> {
    let pairs = match &r.pairing {
        WirePairing::ArcLength => return Ok(vec![(gu.at(gu.nearest(start, false).0), gl.at(gl.nearest(start, false).0), 0)]),
        WirePairing::SyncPoints(p) => p,
    };
    if pairs.is_empty() {
        return Err(invalid(
            "the pairing is by sync points and none are stated - state at least one pair, or pair by arc length".into(),
        ));
    }
    let mut out = Vec::with_capacity(pairs.len());
    for (i, s) in pairs.iter().enumerate() {
        let no = i + 1;
        let (u, l) = match (s.upper, s.lower) {
            (Some(u), Some(l)) => (u, l),
            (Some(q), None) | (None, Some(q)) => {
                let (has, lacks) = if s.upper.is_some() { ("upper", "lower") } else { ("lower", "upper") };
                return Err(invalid(format!(
                    "sync pair #{no} is unpaired: it states its {has} point (X{:.3} Y{:.3}) and no {lacks} point - a sync point pairs one point of each profile",
                    q.x, q.y
                )));
            }
            (None, None) => return Err(invalid(format!("sync pair #{no} is unpaired: it states neither point"))),
        };
        let snap = |q: Point2, g: &Ring, which: &str| {
            if !(q.x.is_finite() && q.y.is_finite()) {
                return Err(invalid(format!("sync pair #{no}'s {which} point is not a finite point")));
            }
            let (arc, d) = g.nearest(q, false);
            if d.is_nan() || d > SYNC_TOL {
                return Err(invalid(format!(
                    "sync pair #{no}'s {which} point X{:.3} Y{:.3} is not on the {which} profile (it stands {d:.4} mm off it)",
                    q.x, q.y
                )));
            }
            Ok(g.at(arc))
        };
        out.push((snap(u, gu, "upper")?, snap(l, gl, "lower")?, no));
    }
    Ok(out)
}

pub(super) fn arcs(pts: &[(Point2, Point2, usize)], up: &Ring, lo: &Ring, corner: bool) -> Vec<Anchor> {
    pts.iter().map(|(u, l, no)| Anchor { upper: up.nearest(*u, corner).0, lower: lo.nearest(*l, corner).0, no: *no }).collect()
}

/// Cut the ruled surface between the `upper` and `lower` profiles.
///
/// # Errors
/// Every refusal the module head lists, by name; none is a panic.
pub fn wire_contour_ruled(upper: WireProfile<'_>, lower: WireProfile<'_>, p: &WireParams, r: &WireRuled) -> Result<Toolpath, GenError> {
    p.door()?;
    let (z0, h) = (p.plane_z, r.top_z - p.plane_z);
    if !(h.is_finite() && h > 0.0) {
        return Err(invalid(format!("the top plane ({:.3}) does not stand above the programmed plane ({z0:.3})", r.top_z)));
    }
    if !(r.tolerance.is_finite() && r.tolerance >= 0.0) {
        return Err(invalid(format!("the 4-axis tolerance {} is not zero or more", r.tolerance)));
    }
    let Some(max) = r.max_deg else {
        return Err(invalid(format!(
            "a 4-axis cut leans the wire, and machine {} states no edm.max_taper_deg - a taper is cut only on a machine that states how far it leans",
            r.machine
        )));
    };
    // Profiles of one kind: both closed.
    let open = |q: WireProfile<'_>| q.loops.is_empty() && !q.chains.is_empty();
    match (open(upper), open(lower)) {
        (true, false) | (false, true) => {
            let (o, c) = if open(upper) { ("upper", "lower") } else { ("lower", "upper") };
            return Err(invalid(format!(
                "the {o} profile is open and the {c} profile is closed - a 4-axis cut pairs two profiles of one kind; pick a closed {o} profile"
            )));
        }
        (true, true) => {
            return Err(invalid("both profiles are open - this operation cuts closed profiles; an open profile has no punch or die side".into()));
        }
        (false, false) => {}
    }
    let (given_u, region_u, gu) = one("upper", upper)?;
    let (given_l, region_l, gl) = one("lower", lower)?;
    let Some(start) = p.start else {
        return Err(invalid("no start hole is stated - a closed profile is cut from a threading point; the wire cannot be threaded through solid material".into()));
    };
    let (rule, hint) = (r.pairing.word(), r.pairing.hint());
    // The stated surface: the profiles as given, under the rule.
    let pts = anchors(r, &gu, &gl, start)?;
    let product = pair(&gu, &gl, &arcs(&pts, &gu, &gl, false), None)?;
    folds(&product, z0, h, &rule, hint)?;
    let k = p.kerf_offset();
    if r.widening == WireWidening::PerRuling {
        let stated = ruled_widen::Stated { p, r, start, max, product: &product, anchors: &pts };
        return ruled_widen::per_ruling(&stated, [(&given_l, &region_l), (&given_u, &region_u)]);
    }
    let mut s = 1.0f64;
    for _ in 0..MAX_PASSES {
        let scaled = |z: f64| WireParams { wire_diameter: p.wire_diameter * s, gap: p.gap * s, plane_z: z, ..p.clone() };
        let (pu, pl) = (scaled(r.top_z), scaled(z0));
        let ring_u = ring::wire_ring(&region_u, p.side, k * s)?;
        let ring_l = ring::wire_ring(&region_l, p.side, k * s)?;
        laws::cover(&given_u, &ring_u, &pu)?;
        laws::cover(&given_l, &ring_l, &pl)?;
        let (Some(wu), Some(wl)) = (Ring::new(&ring_u), Ring::new(&ring_l)) else {
            return Err(invalid("the wire path has collapsed - the profile is too small for the wire".into()));
        };
        let (seam, lead) = wl.nearest(start, false);
        let wire = pair(&wu, &wl, &arcs(&pts, &wu, &wl, true), Some(seam))?;
        if wire.len() > MAX_RULINGS {
            return Err(invalid(format!("the pairing lays {} rulings, more than {MAX_RULINGS} - simplify the profiles", wire.len())));
        }
        folds(&wire, z0, h, &rule, hint)?;
        let steepest = lean_law(&wire, z0, h, max, &rule, &r.machine)?;
        let read = clearance(&product, &wire, start, k, z0, h);
        let standing = k * (s * read.ratio.max(1.0) - 1.0);
        if !read.ratio.is_finite() || standing > r.tolerance + 1e-9 {
            return Err(too_far(r, read.at, standing));
        }
        if read.ratio > 1.0 + SETTLED {
            s *= read.ratio * (1.0 + SETTLED * 0.5);
            continue;
        }
        crossing(&wire, z0, h)?;
        if !(lead.is_finite() && lead > MIN_LEAD) {
            return Err(GenError::invalid_at(
                format!(
                    "wire contour: the start hole at X{:.3} Y{:.3} stands on the wire path itself - move it off the lower profile by more than the kerf offset ({:.4} mm)",
                    start.x,
                    start.y,
                    k * s
                ),
                Point3::new(start.x, start.y, z0),
            ));
        }
        start_side(&region_u, &ring_u, start, &pu)?;
        start_side(&region_l, &ring_l, start, &pl)?;
        if let Some(lead) = read.lead {
            return Err(lead_in(lead));
        }
        let held = format!(
            "wire EDM: 4-axis - the wire is held {:.4} mm off both profiles: the kerf offset {k:.4} and {:.4} mm more (tolerance {:.4}), which stands on the profile planes - a straight leaning wire cannot touch a twisted ruled surface along its whole height, and the kerf is kept out of it at every height",
            k * s,
            k * (s - 1.0),
            r.tolerance
        );
        return emit(&wire, p, r, start, lead, steepest, held);
    }
    Err(invalid(format!("the wire's offset from the paired surface ({rule}) did not settle - refused")))
}

/// The refusal of a twist past the tolerance: `standing` mm past the
/// kerf offset is what the kerf law needs at `at`.
pub(super) fn too_far(r: &WireRuled, at: Point3, standing: f64) -> GenError {
    GenError::invalid_at(
        format!(
            "wire contour: the pairing ({}) twists the surface too far for a straight wire near X{:.3} Y{:.3} Z{:.3}: keeping the kerf out of it needs the wire {} off the profiles past the kerf offset, and the 4-axis tolerance is {:.4} mm - {}, or raise the tolerance",
            r.pairing.word(),
            at.x,
            at.y,
            at.z,
            if standing.is_finite() { format!("{standing:.4} mm") } else { "an unbounded distance".into() },
            r.tolerance,
            r.pairing.hint()
        ),
        at,
    )
}

/// The refusal of a lead whose leaning kerf enters the surface.
pub(super) fn lead_in((short, at): (f64, Point3)) -> GenError {
    GenError::invalid_at(
        format!(
            "wire contour: the lead from the start hole would bring the leaning wire's kerf {short:.4} mm into the paired surface near X{:.3} Y{:.3} Z{:.3} - move the start hole",
            at.x, at.y, at.z
        ),
        at,
    )
}

/// The moves: upright at the start hole, each ruling's lower point with
/// its pose, the first ruling again, and upright back at the hole.
/// `held` is the note of where the wire stands off the profiles.
pub(super) fn emit(
    wire: &[Ruling],
    p: &WireParams,
    r: &WireRuled,
    start: Point2,
    lead: f64,
    steepest: f64,
    held: String,
) -> Result<Toolpath, GenError> {
    let (z, h) = (p.plane_z, r.top_z - p.plane_z);
    let mut b = PathBuilder::new(Point3::new(start.x, start.y, z));
    for (l, u) in wire.iter().chain(wire.first()) {
        let (ux, uy) = (u.x - l.x, u.y - l.y);
        let len = (ux * ux + uy * uy + h * h).sqrt();
        b.set_axis(Some([ux / len, uy / len, h / len]));
        b.push(MoveKind::Feed, Point3::new(l.x, l.y, z), None);
    }
    b.set_axis(Some([0.0, 0.0, 1.0]));
    b.push(MoveKind::Feed, Point3::new(start.x, start.y, z), None);
    let mut path = b.finish();
    if path.moves.len() < 4 {
        return Err(invalid("the wire path lays down no cut".into()));
    }
    let mut said = notes(p, start, lead);
    said.pop();
    said.push(format!(
        "wire EDM: 4-axis - the upper profile on Z{:.3}, the lower on Z{z:.3}, paired {}; {} rulings, the wire leaning at most {steepest:.3} degrees, the moves' poses the wire toward the upper profile; vertical at the start hole; no feed (the control's generator sets the rate)",
        r.top_z,
        r.pairing.word(),
        wire.len()
    ));
    said.push(held);
    path.notes.extend(said);
    Ok(path)
}
