//! #3747: the link law of the rings round a part (2D Offset "Pass starts
//! at" Inside, Outside and Auto). Each ring stays at depth. The link to
//! the next ring stays in the cut plane while it is clear of the part and
//! the fixtures: a straight chord, or a walk along the edge of the area
//! where that edge is in air. A lift is taken only where the in-plane link
//! fails, and the tally names the reason on the path. Provenance: our own
//! law over the keep-down proof of `ladder_links` (the maintainer's rule
//! in #3741, the shared design of #3743); no vendor material consulted.

use crate::cl::{MoveKind, PathBuilder};
use crate::entry::chord_hits_plan;
use crate::geo2d::{Point2, Region};
use crate::geometry::Point3;
use crate::links::TOO_LONG;
use crate::plunge_law;

use super::face::GenError;
use super::levels::{Cert, Motion};
use super::pass_start::StartSide;
use super::PocketStrategy;

/// How far (in tool radii) outside the stock a centre stands in air: its
/// disc then cuts at most [`SWATH`] radii into the stock, as a centre
/// [`SWATH`] radii from a cut already walked cuts into uncut stock.
const AIR: f64 = 0.1;
/// How near (in tool radii) a link sample stays to a cut already walked
/// (the join law of `level_order`).
pub(super) const SWATH: f64 = 0.9;
/// A point this near (mm) to an edge of the area stands on that edge.
const ON_EDGE: f64 = 1e-6;

/// The in-plane link would leave the area that keeps the tool off the part.
pub(crate) const CUTS_PART: &str =
    "the in-plane link would leave the area that keeps the tool off the part";
/// The in-plane link would run into a fixture's keep-out.
pub(crate) const HITS_FIXTURE: &str = "the in-plane link would hit a fixture (a Do not touch item)";
/// The in-plane link would cross a cut planned on that plane.
pub(crate) const CROSSES_CUT: &str = "the in-plane link would cross a cut planned on that plane";
/// The in-plane link would run through stock that no cut has reached.
pub(crate) const UNCUT_STOCK: &str =
    "the in-plane link would cut uncut stock with the full tool width";
/// No area was certified to prove the link against.
pub(crate) const NO_AREA: &str = "no area was certified to prove an in-plane link";

/// What the links of a ring area read beyond the motion: where the air is
/// (outside the stock footprint), and the level's fixture keep-out.
#[derive(Clone, Default)]
pub(super) struct RingLinks {
    air: Option<Region>,
    fixtures: Option<Region>,
}

impl RingLinks {
    /// The law for a tool of radius `tool_r` over `stock` (no footprint: no
    /// air), with the keep-out of the fixtures at the level.
    pub(super) fn new(
        stock: Option<&Region>,
        tool_r: f64,
        fixtures: Option<Region>,
    ) -> Result<Self, GenError> {
        if !(tool_r.is_finite() && tool_r > 0.0) {
            return Err(GenError::Invalid(
                "the tool radius must be positive and finite".into(),
            ));
        }
        let air = match stock {
            Some(s) if !s.is_empty() => Some(s.offset(AIR * tool_r)?),
            _ => None,
        };
        Ok(Self {
            air,
            fixtures: fixtures.filter(|f| !f.is_empty()),
        })
    }

    /// Does a centre at `q` stand in air (its disc mostly off the stock)?
    pub(super) fn in_air(&self, q: Point2) -> bool {
        self.air.as_ref().is_some_and(|a| !a.contains(q))
    }
}

/// The ring law of a start side (Inside or Outside; Auto once resolved):
/// only the rings of an area round a part link by it. A pocket keeps its
/// own links, and so do the passes that are not rings (#3822: the Adaptive
/// spiral, the Zigzag rows), whatever the side.
pub(super) fn for_side(
    side: &StartSide,
    strategy: PocketStrategy,
    tool_r: f64,
    fixtures: Option<Region>,
) -> Result<Option<RingLinks>, GenError> {
    if !side.around_part || !rings(strategy) {
        return Ok(None);
    }
    RingLinks::new(side.stock.as_ref(), tool_r, fixtures).map(Some)
}

/// #3822: the strategies whose passes are the rings of one ring set, the
/// only passes the ring law links. Read on the strategy of the passes
/// posted: an Adaptive plan that falls back to Offset rings posts rings,
/// and the planner joined them by the same law (`level_order`).
pub(super) fn rings(strategy: PocketStrategy) -> bool {
    matches!(strategy, PocketStrategy::Waterline | PocketStrategy::OffsetRings)
}

impl Motion {
    /// #3747: link the rings of this ladder by the ring law.
    pub(super) fn with_ring_links(mut self, rl: Option<RingLinks>) -> Self {
        self.ring_links = rl;
        self
    }
}

/// The in-plane link from the tool to `e` on the plane `z` (#3747): the
/// straight chord, else (#3824) the walk along the edge of the area that
/// [`edge_route`] finds, each of its chords proved as the straight one is
/// ([`chord_proof`]) and the whole walk within the stay-down limit. `Ok`
/// posts the feeds and returns the walk from the tool to `e`; `Err` is the
/// straight chord's reason, which the pass link's lift names.
pub(super) fn link(
    b: &mut PathBuilder,
    rl: &RingLinks,
    m: &Motion,
    cert: Option<&Cert<'_>>,
    cuts: &[&[Vec<Point2>]],
    e: Point2,
    z: f64,
) -> Result<Vec<Point2>, &'static str> {
    let cur = b.at();
    let at = Point2::new(cur.x, cur.y);
    if !m.links.within_stay_down(dist(at, e)) {
        return Err(TOO_LONG);
    }
    if at == e {
        return Ok(vec![at, e]);
    }
    let r = m.tool_r();
    let Some(cen) = cert.filter(|_| r > 0.0) else {
        return Err(NO_AREA);
    };
    let walk = match chord_proof(b, rl, cen, cuts, at, e, z, r) {
        Ok(()) => vec![at, e],
        Err(why) => {
            let walks: Vec<&[Point2]> = cuts.iter().flat_map(|c| c.iter().map(Vec::as_slice)).collect();
            let ok = |p: Point2, q: Point2| chord_proof(b, rl, cen, cuts, p, q, z, r).is_ok();
            let Some(via) = edge_route(at, e, cen.room(), SWATH * r, &walks, ok) else {
                return Err(why);
            };
            let mut w = vec![at];
            w.extend(via);
            if !m.links.within_stay_down(length(&w)) {
                return Err(why);
            }
            w
        }
    };
    for q in walk.iter().skip(1) {
        b.push(MoveKind::Feed, Point3::new(q.x, q.y, z), Some(m.feed));
    }
    Ok(walk)
}

/// The proof of one in-plane chord `a -> s` on the plane `z`, in this
/// order: the certified area (else the part or a fixture is named), no
/// crossing of `cuts`, and every sample in air or over a column the path
/// has cut. `Err` is the reason.
#[allow(clippy::too_many_arguments)]
fn chord_proof(
    b: &PathBuilder,
    rl: &RingLinks,
    cen: &Cert<'_>,
    cuts: &[&[Vec<Point2>]],
    a: Point2,
    s: Point2,
    z: f64,
    r: f64,
) -> Result<(), &'static str> {
    if a == s {
        return Ok(());
    }
    let pts = samples(&[a, s], 0.25 * r);
    if !cen.chord_ok(a, s) {
        let fixture = rl
            .fixtures
            .as_ref()
            .is_some_and(|f| pts.iter().any(|&q| f.contains(q)));
        return Err(if fixture { HITS_FIXTURE } else { CUTS_PART });
    }
    if cuts.iter().any(|c| chord_hits_plan(a, s, c)) {
        return Err(CROSSES_CUT);
    }
    // `column_cleared` reads a centre within (1 - CORE) of its radius from
    // a cut; scaled, the reach is the planner's swath.
    let reach = r * SWATH / (1.0 - plunge_law::CORE);
    if !pts
        .iter()
        .all(|&q| rl.in_air(q) || plunge_law::column_cleared(b.moves(), q, z, reach))
    {
        return Err(UNCUT_STOCK);
    }
    Ok(())
}

/// The walk from `e` to `s` along a loop of `center` that both stand on
/// or within `gap` of (then a chord to a point of the loop: the nearest,
/// or a vertex of `walks` on the loop, a shared vertex), the shorter way
/// first, through each vertex of `walks` that stands on the walk (a
/// touch, never a crossing). Returns the walk after `e` (it ends on `s`)
/// when `ok` proves each of its chords.
pub(super) fn edge_route(
    e: Point2,
    s: Point2,
    center: &Region,
    gap: f64,
    walks: &[&[Point2]],
    ok: impl Fn(Point2, Point2) -> bool,
) -> Option<Vec<Point2>> {
    for l in center.loops() {
        let pts = l.points();
        let n = pts.len();
        if n < 3 {
            continue;
        }
        let (from, to) = (attaches(e, pts, gap, walks), attaches(s, pts, gap, walks));
        let mut ways: Vec<Vec<Point2>> = Vec::new();
        for &(ie, fe) in &from {
            for &(is, fs) in &to {
                // Forward: the vertices after e's edge up to s's edge;
                // back: the vertices of e's edge down to the one after s's
                // edge. On one edge: along that edge only.
                let fwd: Vec<Point2> = (1..=(is + n - ie) % n).map(|k| pts[(ie + k) % n]).collect();
                let back: Vec<Point2> = (0..(ie + n - is) % n)
                    .map(|k| pts[(ie + n - k) % n])
                    .collect();
                let mids = if ie == is {
                    vec![Vec::new()]
                } else {
                    vec![fwd, back]
                };
                for mid in mids {
                    let mut w = vec![e, fe];
                    w.extend(mid);
                    w.extend([fs, s]);
                    w.dedup();
                    ways.push(through(&w, walks));
                }
            }
        }
        ways.sort_by(|a, b| length(a).total_cmp(&length(b)));
        for w in ways {
            if w.windows(2).all(|c| ok(c[0], c[1])) {
                return Some(w.into_iter().skip(1).collect());
            }
        }
    }
    None
}

/// The most points of a loop that one end of an edge route tries.
const ATTACHES: usize = 6;

/// Where `q` may join loop `pts` (closed): `q` itself when it stands on
/// an edge; else, within `gap`, the nearest point of the loop and the
/// vertices of `walks` that stand on the loop, nearest first. Each with
/// the index of its edge.
fn attaches(q: Point2, pts: &[Point2], gap: f64, walks: &[&[Point2]]) -> Vec<(usize, Point2)> {
    let n = pts.len();
    let edge_of = |v: Point2| (0..n).find(|&i| seg_dist(v, pts[i], pts[(i + 1) % n]) <= ON_EDGE);
    if let Some(i) = edge_of(q) {
        return vec![(i, q)];
    }
    let mut out: Vec<(f64, usize, Point2)> = Vec::new();
    if let Some((i, d)) = (0..n)
        .map(|i| (i, seg_dist(q, pts[i], pts[(i + 1) % n])))
        .min_by(|a, b| a.1.total_cmp(&b.1))
        && d <= gap
    {
        let (a, b) = (pts[i], pts[(i + 1) % n]);
        let (dx, dy) = (b.x - a.x, b.y - a.y);
        let dd = dx * dx + dy * dy;
        let t = if dd > 0.0 {
            (((q.x - a.x) * dx + (q.y - a.y) * dy) / dd).clamp(0.0, 1.0)
        } else {
            0.0
        };
        out.push((d, i, Point2::new(a.x + t * dx, a.y + t * dy)));
    }
    for v in walks.iter().flat_map(|w| w.iter().copied()) {
        let d = dist(q, v);
        if d <= gap
            && d > 0.0
            && let Some(i) = edge_of(v)
        {
            out.push((d, i, v));
        }
    }
    out.sort_by(|a, b| a.0.total_cmp(&b.0));
    out.dedup_by(|a, b| a.2 == b.2);
    out.into_iter()
        .take(ATTACHES)
        .map(|(_, i, v)| (i, v))
        .collect()
}

/// The walk `w` with each vertex of `walks` that stands on one of its
/// chords put in, in order along that chord.
fn through(w: &[Point2], walks: &[&[Point2]]) -> Vec<Point2> {
    let mut out: Vec<Point2> = w.first().copied().into_iter().collect();
    for c in w.windows(2) {
        let (a, b) = (c[0], c[1]);
        let (lo, hi) = (
            Point2::new(a.x.min(b.x) - ON_EDGE, a.y.min(b.y) - ON_EDGE),
            Point2::new(a.x.max(b.x) + ON_EDGE, a.y.max(b.y) + ON_EDGE),
        );
        let dd = (b.x - a.x).powi(2) + (b.y - a.y).powi(2);
        let mut on: Vec<(f64, Point2)> = walks
            .iter()
            .flat_map(|p| p.iter().copied())
            .filter(|v| v.x >= lo.x && v.x <= hi.x && v.y >= lo.y && v.y <= hi.y)
            .filter(|&v| v != a && v != b && seg_dist(v, a, b) <= ON_EDGE)
            .map(|v| {
                (
                    if dd > 0.0 {
                        ((v.x - a.x) * (b.x - a.x) + (v.y - a.y) * (b.y - a.y)) / dd
                    } else {
                        0.0
                    },
                    v,
                )
            })
            .collect();
        on.sort_by(|x, y| x.0.total_cmp(&y.0));
        out.extend(on.into_iter().map(|(_, v)| v));
        out.push(b);
    }
    out.dedup();
    out
}

fn length(w: &[Point2]) -> f64 {
    w.windows(2).map(|c| dist(c[0], c[1])).sum()
}

fn dist(a: Point2, b: Point2) -> f64 {
    (b.x - a.x).hypot(b.y - a.y)
}

/// Points along the walk `pts` no farther apart than `step`, both ends
/// included.
pub(super) fn samples(pts: &[Point2], step: f64) -> Vec<Point2> {
    let mut out: Vec<Point2> = pts.first().copied().into_iter().collect();
    for w in pts.windows(2) {
        let n = (dist(w[0], w[1]) / step.max(1e-3)).ceil().clamp(1.0, 1e5) as usize;
        for k in 1..=n {
            let t = k as f64 / n as f64;
            out.push(Point2::new(
                w[0].x + t * (w[1].x - w[0].x),
                w[0].y + t * (w[1].y - w[0].y),
            ));
        }
    }
    out
}

fn seg_dist(p: Point2, a: Point2, b: Point2) -> f64 {
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    let dd = dx * dx + dy * dy;
    let t = if dd > 0.0 && dd.is_finite() {
        (((p.x - a.x) * dx + (p.y - a.y) * dy) / dd).clamp(0.0, 1.0)
    } else {
        0.0
    };
    (p.x - a.x - t * dx).hypot(p.y - a.y - t * dy)
}
