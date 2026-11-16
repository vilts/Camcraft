//! The planar level ladder (split from `machining2d.rs` by M5.0.1): cut
//! levels, the certified area, and the walk of passes level by level -
//! with M11.2's user-chosen level transitions and links (provenance in
//! `links.rs`; the descents in `descent.rs`, the links in
//! `ladder_links.rs`).

use crate::cl::{MoveKind, PathBuilder, lattice};
use crate::entry::Entry3D;
use crate::geo2d::{Point2, Region};
use crate::geometry::Point3;
use crate::links::Links;

use super::GenError;
use super::arcs::{Seg, fit_arcs, push_segs};
use super::ladder_links::{LevelWalk, LinkState, descend, level_link, pass_link};
use super::slots::chord_clears;

/// The certified legal-center area a descent is proved against (M8.2
/// C). Usually a real [`Region`]; for the slot the closed-form
/// cutter-center stadium. The tessellated form of a stadium cannot
/// certify a slot: the row law stands legitimate cutter centers
/// EXACTLY on its boundary, and an inscribed tessellation sits
/// ~1.9e-5·h inside the true arcs - a ramp landing on its own row's
/// endpoint would be refused by the polygon test, over and over. A
/// stadium is convex, so for it the endpoint test IS the chord test:
/// endpoints within h of the axis put the whole chord inside the true
/// stadium, no tessellation error in sight. The polygon rides along
/// only for `dive_room`, which reads triangle centroids - strictly
/// interior points, where an inscribed region can refuse an orbit but
/// never bless a gouge.
pub(super) enum Cert<'a> {
    Region(&'a Region),
    Stadium { a: Point2, b: Point2, h: f64, tess: &'a Region },
}

impl Cert<'_> {
    /// The region `dive_room` triangulates.
    pub(super) fn room(&self) -> &Region {
        match self {
            Cert::Region(r) => r,
            Cert::Stadium { tess, .. } => tess,
        }
    }

    /// A descent's chord, certified. `1e-7` absorbs float noise in
    /// endpoints the generators construct on the boundary; it is a
    /// tenth of a micrometer, below any cut tolerance and any gouge
    /// that could matter.
    pub(super) fn chord_ok(&self, p: Point2, q: Point2) -> bool {
        match self {
            Cert::Region(r) => chord_clears(r, p, q),
            Cert::Stadium { a, b, h, .. } => {
                dist_to_seg(p, *a, *b) <= h + 1e-7 && dist_to_seg(q, *a, *b) <= h + 1e-7
            }
        }
    }
}

/// Distance from `p` to the segment `a -> b`: the stadium membership
/// test (a point is in the stadium of half-width h iff it is within
/// h of the axis segment).
fn dist_to_seg(p: Point2, a: Point2, b: Point2) -> f64 {
    let d = Point2::new(b.x - a.x, b.y - a.y);
    let dd = d.x * d.x + d.y * d.y;
    if dd == 0.0 || !dd.is_finite() {
        return (p.x - a.x).hypot(p.y - a.y);
    }
    let t = (((p.x - a.x) * d.x + (p.y - a.y) * d.y) / dd).clamp(0.0, 1.0);
    (p.x - a.x - t * d.x).hypot(p.y - a.y - t * d.y)
}
/// Cut levels from `top` down to `bottom`, snapped so the last level is
/// `bottom` exactly. Shared by depth-stepping operations.
pub(crate) fn cut_levels(top: f64, bottom: f64, stepdown: f64) -> Result<Vec<f64>, GenError> {
    let span = top - bottom;
    if span <= 0.0 {
        return Ok(vec![bottom]);
    }
    super::face::pass_budget("stepdown", span, stepdown)?;
    let tol = span * 1e-9;
    let mut zs = Vec::new();
    let mut z = top;
    while z > bottom + tol {
        zs.push(z);
        z -= stepdown;
    }
    zs.push(bottom);
    Ok(zs)
}

/// The motion envelope a depth-stepping generator shares with
/// [`run_levels`]: the two lift planes (clearance for a level change,
/// hop between passes of one level), the two feeds, the entry family the
/// ladder first engages with (M5.1 H), and the level transitions and
/// links the user chose (M11.2 C/D, [`Links`]).
pub(super) struct Motion {
    pub(super) clearance_z: f64,
    pub(super) hop_z: f64,
    pub(super) feed: f64,
    pub(super) plunge_feed: f64,
    /// How the first engagement goes. `Plunge` is the legacy climb-and-
    /// dive every depth-stepping operation has always posted; the
    /// pocket's validation already proved the other arms' radii against
    /// the tool, the way `machining3d`'s does.
    pub(super) entry: Entry3D,
    pub(super) helix_radius: f64,
    /// The arc entry's nominal radius (M11.1); 0 asks the quarter turn.
    pub(super) arc_radius: f64,
    /// Level transitions and links; [`Links::PLANAR`] is the legacy
    /// ladder byte for byte.
    pub(super) links: Links,
    /// The top of the cut (the material's top on the first level), read
    /// by the approach's plunge height and ramp-to-depth (M11.3 B);
    /// clearance until a ladder states it, which leaves both inert.
    pub(super) top_z: f64,
    /// Per pass: reached from the last by a microlift return (the HSM
    /// plan's, `microlift`) instead of the pass link; empty: none is.
    pub(super) micro: Vec<Option<super::adaptive::Return>>,
    /// The pocket's horizontal arc entry (M11.3, `descent_xy`): only a
    /// ladder that opts in reads `Approach::arc_xy`.
    pub(super) xy_arc: bool,
    /// Holes the caller proved open (#1449, `predrilled`): read only
    /// when the approach asks for pre-drilled entry.
    pub(super) holes: Vec<super::predrilled::PreHole>,
    /// Every pass is a ring walked as arcs about one centre (#1558, the
    /// circular kernel; #1665, the bore's ring and its radial sweep): the
    /// ramp entry zig-zags along the first cut at the ramp angle, and a
    /// zig-zag ramp rides the ring's arcs.
    pub(super) ring_zig: bool,
}

impl Motion {
    /// `retract_z: None` hops at clearance - the legacy shape, every
    /// lift a full climb. A set plane is clamped at clearance, so a
    /// pre-validation file whose retract sits above clearance still
    /// crosses at the plane the machine already obeyed. The entry
    /// family starts at `Plunge`: the pocket (M8.1), the contour
    /// (M8.2 A) and the slot (M8.2 C) opt into [`with_entry`]; trace
    /// never does - its zero-width kerf admits no arm (the verdict
    /// comment in `slots.rs`).
    pub(super) fn new(clearance_z: f64, retract_z: Option<f64>, feed: f64, plunge_feed: f64) -> Self {
        Self {
            clearance_z,
            hop_z: retract_z.unwrap_or(clearance_z).min(clearance_z),
            feed,
            plunge_feed,
            entry: Entry3D::default(),
            helix_radius: 0.0,
            arc_radius: 0.0,
            links: Links::PLANAR,
            top_z: clearance_z,
            micro: Vec::new(),
            xy_arc: false,
            holes: Vec::new(),
            ring_zig: false,
        }
    }

    /// Every pass is a ring of arcs (#1558): ramps ride them.
    pub(super) fn with_ring_zig(mut self) -> Self {
        self.ring_zig = true;
        self
    }

    /// The proved pre-drilled holes (#1449).
    pub(super) fn with_holes(mut self, holes: &[super::predrilled::PreHole]) -> Self {
        self.holes = holes.to_vec();
        self
    }

    /// Opt this ladder into the horizontal arc entry (the pocket, when
    /// its approach asks for it).
    pub(super) fn with_xy_arc(mut self, on: bool) -> Self {
        self.xy_arc = on;
        self
    }

    /// The HSM plan's microlift returns, per pass (M11.4 C round 4).
    pub(super) fn with_microlift(mut self, joins: Vec<Option<super::adaptive::Return>>) -> Self {
        self.micro = joins;
        self
    }

    /// The tool radius the centre-cutting plunge law reads (`plunge_law`):
    /// half the arc radius every entry-capable ladder sets to the tool
    /// diameter; `0` where none is set (chamfer, trace), which clears no
    /// column - the law errs to solid.
    pub(super) fn tool_r(&self) -> f64 {
        (self.arc_radius * 0.5).max(0.0)
    }

    /// The top of the cut (M11.3 B), clamped under clearance.
    pub(super) fn with_top(mut self, top_z: f64) -> Self {
        if top_z.is_finite() {
            self.top_z = top_z.min(self.clearance_z);
        }
        self
    }

    /// Opt this ladder into the entry family (the pocket's `entry`
    /// parameter); `helix_radius` is the dive circle, ignored by the
    /// other arms.
    pub(super) fn with_entry(mut self, entry: Entry3D, helix_radius: f64) -> Self {
        self.entry = entry;
        self.helix_radius = helix_radius;
        self
    }

    /// The arc entry's first-asked radius (callers: the tool diameter).
    pub(super) fn with_arc_radius(mut self, r: f64) -> Self {
        self.arc_radius = r;
        self
    }

    /// The user's level transitions and links (M11.2 C/D).
    pub(super) fn with_links(mut self, links: Links, helix_radius: f64) -> Self {
        self.links = links;
        if links.transition == Some(Entry3D::Helix) {
            self.helix_radius = helix_radius;
        }
        self
    }
}

/// Run XY passes at each of the cut levels. `center` is the level plan's
/// cutter-center area - the same region at every level, because a
/// planar pocket's passes are one tiling walked at each plane. `Some`
/// lets the entry family prove its descents against it, at the first
/// level and at every crossing (M8.1); `None` keeps the climb-and-dive
/// slot and trace have always posted. `links_cert` is the area the
/// level transitions and keep-down links (M11.2) are proved against -
/// the same area, handed over even where the entry stays the plain dive
/// (a slot or a bore whose entry is Plunge still has a certified band).
/// `fit = Some(tol)` runs each pass through [`fit_arcs`] once.
pub(super) fn run_levels(
    b: &mut PathBuilder,
    passes: &[Vec<Point2>],
    zs: &[f64],
    m: &Motion,
    fit: Option<f64>,
    center: Option<&Cert<'_>>,
    links_cert: Option<&Cert<'_>>,
) {
    let runs: Option<Vec<Vec<Seg>>> =
        fit.map(|tol| passes.iter().map(|p| fit_arcs(p, tol)).collect());
    run_level_walks(b, passes, zs, m, runs.as_deref(), center, links_cert);
}

/// [`run_levels`] with the walks already built: `runs[i]` is pass `i`
/// past its first point as [`Seg`]s (fitted arcs, M11.1's corner
/// fillets); `None` walks the points.
///
/// The ladder's three moments: the path's first entry (the `entry`
/// family from clearance - M8.1's level-0 law); a level change (the
/// transition - `links.transition`, else the entry - reached by the
/// level link: from clearance, from the retract plane, or staying at
/// the level above and feeding to the descent's start where that link
/// is proved); and a pass change within a level (the pass link: the
/// retract-plane hop every planar ladder has posted, the clearance
/// climb, or a keep-down feed where proved). The z the tool last
/// ENGAGED decides which moment is which: a level whose earlier passes
/// came out degenerate still crosses in over new material.
pub(super) fn run_level_walks(
    b: &mut PathBuilder,
    passes: &[Vec<Point2>],
    zs: &[f64],
    m: &Motion,
    runs: Option<&[Vec<Seg>]>,
    center: Option<&Cert<'_>>,
    links_cert: Option<&Cert<'_>>,
) {
    let mut first = true;
    let mut last_cut_z: Option<f64> = None;
    let mut above = m.top_z;
    let mut st = LinkState::default();
    for &z in zs {
        for (pi, pass) in passes.iter().enumerate() {
            let Some((_, rest)) = pass.split_first() else {
                continue;
            };
            if rest.is_empty() {
                continue;
            }
            let walk = runs.and_then(|r| r.get(pi)).map(Vec::as_slice);
            let prev_z = last_cut_z.unwrap_or(m.clearance_z);
            // The material's top over this level (M11.3's plunge height):
            // the plane above it - the last level cut when this level is
            // entered, kept for its later passes - or the top of the cut.
            if last_cut_z != Some(z) {
                above = last_cut_z.unwrap_or(m.top_z);
            }
            let mat_top = above;
            let lv = LevelWalk { passes, pass, walk, z, prev_z, mat_top };
            if first {
                // The first entry: the entry family, from clearance.
                let dove = descend(b, m.entry, false, center, &lv, m.clearance_z, m);
                st.begin_level(dove);
            } else if last_cut_z == Some(z) {
                if let Some(Some(ret)) = m.micro.get(pi) {
                    super::microlift::link(b, ret, z, m);
                } else {
                    pass_link(b, &lv, m, links_cert, &mut st);
                }
            } else {
                // A level change. The transition, when the user set one,
                // proves against the link area; "same as entry" keeps
                // the entry's own certification, byte for byte.
                let (arm, zig, cert) = match m.links.transition {
                    Some(t) => (t, true, links_cert),
                    None => (m.entry, false, center),
                };
                level_link(b, arm, zig, cert, &lv, m, &mut st);
            }
            // The engagement limit (M11.3 D, owed): the cut out of a
            // level's descent stays slowed for its first tool diameter,
            // until the full-width engagement of the bore settles.
            let entered = last_cut_z != Some(z);
            let scale = m.links.approach.engagement_scale();
            first = false;
            if entered && scale < 1.0 && m.arc_radius > 0.0 {
                let segs = match walk {
                    Some(w) => w.to_vec(),
                    None => vec![Seg::Line(rest.to_vec())],
                };
                push_settling(b, &segs, z, m.feed, m.feed * scale, m.arc_radius);
            } else {
                match walk {
                    Some(w) => push_segs(b, w, z, m.feed),
                    None => {
                        for q in rest {
                            b.push(MoveKind::Feed, Point3::new(q.x, q.y, z), Some(m.feed));
                        }
                    }
                }
            }
            last_cut_z = Some(z);
        }
    }
    st.tally.say(b, "level/pass");
    if let Some(n) = m.links.approach.engagement_note()
        && !first
    {
        b.note_once(&n);
    }
}

/// Walk `segs` at plane `z`, the first `stretch` mm (XY path length) at
/// `slow` and the rest at `feed`: a straight crossing the stretch's end
/// is split there (lattice-snapped); an arc that starts inside it is
/// slowed whole - never faster than asked.
fn push_settling(b: &mut PathBuilder, segs: &[Seg], z: f64, feed: f64, slow: f64, stretch: f64) {
    let mut run = 0.0;
    for seg in segs {
        let at = b.at();
        match seg {
            Seg::Line(qs) => {
                let mut cur = Point2::new(at.x, at.y);
                for q in qs {
                    let d = (q.x - cur.x).hypot(q.y - cur.y);
                    if run < stretch && run + d > stretch && d.is_finite() && d > 0.0 {
                        let f = (stretch - run) / d;
                        let s = Point2::new(lattice(cur.x + (q.x - cur.x) * f), lattice(cur.y + (q.y - cur.y) * f));
                        b.push(MoveKind::Feed, Point3::new(s.x, s.y, z), Some(slow));
                        b.push(MoveKind::Feed, Point3::new(q.x, q.y, z), Some(feed));
                    } else {
                        let f = if run < stretch { slow } else { feed };
                        b.push(MoveKind::Feed, Point3::new(q.x, q.y, z), Some(f));
                    }
                    run += if d.is_finite() { d } else { 0.0 };
                    cur = *q;
                }
            }
            Seg::Arc { center, ccw, to } => {
                let f = if run < stretch { slow } else { feed };
                b.push_arc(MoveKind::Feed, Point3::new(to.x, to.y, z), Some(f), *center, *ccw);
                run += (to.x - at.x).hypot(to.y - at.y);
            }
        }
    }
}
