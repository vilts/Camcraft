//! Split from `machining2d.rs` by M5.0.1; structure only.

use crate::cl::{MoveKind, PathBuilder, Toolpath};
use crate::entry::Entry3D;
use crate::geo2d::{Loop, Point2, Region};
use crate::geometry::Point3;
use crate::links::Links;

use super::face::{GenError, band};
use super::levels::{Cert, Motion, cut_levels, run_levels};
/// Slotting: a straight channel between two XY points, cleared in rows
/// parallel to its axis. The channel is the stadium swept by a disc of
/// radius `width / 2` along `a` -> `b`; shrinking a stadium is narrowing
/// one, so the cutter-center area is in closed form - the stadium of
/// half-width `h = width / 2 - r - leave` on the same endpoints, no
/// Clipper round trip. `h < 0` means no tool position keeps the edge
/// inside the walls; `h == 0` is the everyday full-width slot - one row
/// on the centerline, the tool's own edges sweeping the walls.
///
/// Rows band the width `[-h, h]` at `stepover` (same snap law as facing
/// and pocket rows), and the row at offset `d` runs `a - q u + d n` to
/// `b + q u + d n` with `q = sqrt(h^2 - d^2)`, so each row reaches the
/// round ends exactly as far as it can. The center area is convex, so
/// the serpentine joins with lateral feeds and no in-cut rapids are
/// even possible.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SlotParams {
    pub tool_diameter: f64,
    /// Width of the finished channel.
    pub width: f64,
    pub stock_to_leave: f64,
    pub clearance_z: f64,
    /// Hop plane for moves that stay inside already-cut material. None
    /// hops to `clearance_z` (the legacy shape). A slot is one continuous
    /// serpentine per level, so the hop never actually engages here; the
    /// plane is validated like everywhere else so a stale value cannot
    /// hide inside a file that later gains multi-pass rows.
    pub retract_z: Option<f64>,
    /// First cut level; material is engaged from here down.
    pub top_z: f64,
    pub bottom_z: f64,
    pub stepdown: f64,
    pub stepover: f64,
    pub feed: f64,
    pub plunge_feed: f64,
    /// The first row runs from `a` toward `b`; rows alternate from there.
    pub climb: bool,
    /// How a level crossing engages (M8.2 C): the family the pocket and
    /// contour ladders already read. The serpentine's pass start is a
    /// stadium-boundary point - a wall - so the legacy dive this ladder
    /// posts there is exactly the dive the entry law questions.
    /// `Plunge` keeps it untouched (the goldens' shape, and the shape
    /// no center is passed for); `Ramp`/`Helix` certify against the
    /// closed-form cutter-center stadium - the same region the rows
    /// band - and refuse per level, as substitutions, where it has no
    /// room the descent can stand in.
    pub entry: Entry3D,
    /// Helix centerline circle radius, mm; read only when `entry` is
    /// Helix. Fit is certified per level by `dive_room` against the
    /// stadium wall (the contour law), not by a static cap: a full-
    /// width slot (h = 0) has no room for any orbit, and says so.
    pub helix_radius: f64,
    /// Level transitions and links (M11.2 C/D), proved against the
    /// closed-form stadium; [`Links::PLANAR`] is every earlier slot.
    pub links: Links,
}

impl SlotParams {
    pub(super) fn validate(&self) -> Result<(), GenError> {
        let mut finite = [
            self.tool_diameter,
            self.width,
            self.stock_to_leave,
            self.clearance_z,
            self.top_z,
            self.bottom_z,
            self.stepdown,
            self.stepover,
            self.feed,
            self.plunge_feed,
        ]
        .into_iter()
        .all(f64::is_finite);
        finite &= self.retract_z.is_none_or(f64::is_finite);
        if !finite {
            return Err(GenError::Invalid("parameters must be finite".into()));
        }
        if self.tool_diameter <= 0.0 || self.stepdown <= 0.0 {
            return Err(GenError::Invalid("diameter and stepdown must be positive".into()));
        }
        if self.stepover <= 0.0 {
            return Err(GenError::Invalid("stepover must be positive".into()));
        }
        if self.stepover > self.tool_diameter {
            return Err(GenError::Invalid(
                "stepover leaves bands the tool cannot reach".into(),
            ));
        }
        if self.feed <= 0.0 || self.plunge_feed <= 0.0 {
            return Err(GenError::Invalid("feeds must be positive".into()));
        }
        if self.width <= 0.0 {
            return Err(GenError::Invalid("slot width must be positive".into()));
        }
        self.links.validate().map_err(|e| GenError::Invalid(e.into()))?;
        let helix = self.entry == Entry3D::Helix || self.links.transition == Some(Entry3D::Helix);
        if helix && self.helix_radius <= 0.0 {
            // Only positivity (the contour law): whether the corridor
            // can HOLD the orbit is certified per level by `dive_room`
            // against the stadium, where too wide is refused loudly.
            return Err(GenError::Invalid("helix radius must be positive".into()));
        }
        if self.stock_to_leave < 0.0 {
            return Err(GenError::Invalid("stock to leave must not be negative".into()));
        }
        if self.top_z < self.bottom_z {
            return Err(GenError::Invalid("cut top below cut bottom".into()));
        }
        if self.clearance_z < self.top_z {
            return Err(GenError::Invalid("clearance plane below the first cut level".into()));
        }
        if self.retract_z.is_some_and(|rz| rz < self.top_z) {
            return Err(GenError::Invalid("retract plane sits inside the material".into()));
        }
        Ok(())
    }
}

/// Mill the slot between `a` and `b` (see [`SlotParams`] for the shape
/// and the row law). One serpentine pass per level; `run_levels` lifts
/// and plunges only between levels.
pub fn slot(a: Point2, b: Point2, p: &SlotParams) -> Result<Toolpath, GenError> {
    p.validate()?;
    let dx = b.x - a.x;
    let dy = b.y - a.y;
    let len = (dx * dx + dy * dy).sqrt();
    if len == 0.0 || !len.is_finite() {
        return Err(GenError::Invalid("slot endpoints must be distinct".into()));
    }
    let r = p.tool_diameter * 0.5;
    let h = p.width * 0.5 - r - p.stock_to_leave;
    if h < 0.0 {
        return Err(GenError::Invalid("slot is narrower than the tool".into()));
    }
    let u = Point2::new(dx / len, dy / len);
    let n = Point2::new(-u.y, u.x);
    let ds = band(-h, h, 0.0, p.stepover)?;
    let mut pass: Vec<Point2> = Vec::with_capacity(2 * ds.len());
    for (i, &d) in ds.iter().enumerate() {
        let q = (h * h - d * d).max(0.0).sqrt();
        let s = Point2::new(a.x - u.x * q + n.x * d, a.y - u.y * q + n.y * d);
        let e = Point2::new(b.x + u.x * q + n.x * d, b.y + u.y * q + n.y * d);
        // Even rows run a -> b under climb; odd rows return. The join is
        // then a short lateral step at the far end, never a diagonal.
        let (s, e) = if p.climb == (i % 2 == 0) { (s, e) } else { (e, s) };
        pass.push(s);
        pass.push(e);
    }

    let zs = cut_levels(p.top_z, p.bottom_z, p.stepdown)?;
    let mut bld = PathBuilder::approached(Point3::new(0.0, 0.0, p.clearance_z));
    let mut motion = Motion::new(p.clearance_z, p.retract_z, p.feed, p.plunge_feed);
    // `Plunge` passes no center and the ladder dives at the pass start
    // as it always did - the goldens' shape. The ramped families certify
    // against the cutter-center stadium, the same band the rows walk:
    // chords by closed form (convex - the row law's own boundary points
    // certify clean), orbits by the inscribed tessellation (centroids
    // are strictly interior, so a tessellation error can only refuse,
    // never bless). A channel with no width to certify (h = 0) hands
    // the ladder nothing, and says so once - the descent it then posts
    // is the correct one (the starter hole IS the plunge at h=0).
    // A tool that is not centre-cutting needs the stadium under a Plunge
    // entry too: the helix or ramp that stands in proves against it.
    let tess = if p.entry == Entry3D::Plunge && p.links.approach.center_cutting {
        None
    } else {
        motion = motion
            .with_entry(p.entry, p.helix_radius)
            .with_arc_radius(p.tool_diameter);
        let t = stadium_center(a, b, h);
        if t.is_none() {
            bld.note(
                "entry: the channel has no room the descent could stand in - axial dive at the starter end, unchanged",
            );
        }
        t
    };
    let center = tess.as_ref().map(|t| Cert::Stadium { a, b, h, tess: t });
    // The links' area (M11.2): the same stadium, asked for even under a
    // Plunge entry - only when the user chose something to prove.
    let link_tess = match &tess {
        Some(_) => None,
        None if p.links == Links::PLANAR => None,
        None => stadium_center(a, b, h),
    };
    motion = motion.with_arc_radius(p.tool_diameter).with_links(p.links, p.helix_radius).with_top(p.top_z);
    let link_cert = match (&tess, &link_tess) {
        (Some(t), _) | (None, Some(t)) => Some(Cert::Stadium { a, b, h, tess: t }),
        (None, None) => None,
    };
    run_levels(&mut bld, &[pass], &zs, &motion, None, center.as_ref(), link_cert.as_ref());
    let cur = bld.at();
    bld.push(MoveKind::Rapid, Point3::new(cur.x, cur.y, p.clearance_z), None);

    let path = bld.finish();
    if let Some(why) = crate::approach::refusal(&path) {
        // #261 part 3: the moves built so far ride the refusal, display-only.
        return Err(GenError::Invalid(why).with_ghost(&path));
    }
    if !path.all_finite() {
        return Err(GenError::Invalid("path left the finite horizon".into()));
    }
    Ok(path)
}

/// The cutter-center stadium of half-width `h` on `a` -> `b`,
/// tessellated for `dive_room`'s orbit test (see `Cert::Stadium` in
/// `levels.rs` for why the chords are certified by closed form and
/// not here). The round ends ride inscribed 512-segment semicircles:
/// a centroid is strictly interior, so this tessellation can refuse
/// an orbit the true stadium would allow (sagitta `h·1.9e-5`,
/// sub-micron for any real slot), never bless a gouge. `h <= 0` is
/// the degenerate channel - a line has no area an orbit could stand
/// in - and answers `None`.
fn stadium_center(a: Point2, b: Point2, h: f64) -> Option<Region> {
    if h <= 0.0 || !h.is_finite() {
        return None;
    }
    let dx = b.x - a.x;
    let dy = b.y - a.y;
    let len = (dx * dx + dy * dy).sqrt();
    if len == 0.0 || !len.is_finite() {
        return None;
    }
    let u = Point2::new(dx / len, dy / len);
    let n = Point2::new(-u.y, u.x);
    let seg = std::f64::consts::PI / 512.0; // 512 chords per cap
    let mut pts = Vec::with_capacity(2 + 2 * 512);
    // Boundary order: far cap of `b` from its +n tangent through the
    // tip `b + h u` to the -n tangent, the straight -n side to `a`,
    // then `a`'s cap mirrored the same way back to the start.
    pts.push(Point2::new(b.x + n.x * h, b.y + n.y * h));
    for k in 1..=512 {
        let t = seg * k as f64;
        let (c, s) = (t.cos(), t.sin());
        pts.push(Point2::new(
            b.x + h * (n.x * c + u.x * s),
            b.y + h * (n.y * c + u.y * s),
        ));
    }
    for k in 1..=512 {
        let t = seg * k as f64;
        let (c, s) = (t.cos(), t.sin());
        pts.push(Point2::new(
            a.x - h * (n.x * c + u.x * s),
            a.y - h * (n.y * c + u.y * s),
        ));
    }
    Loop::new(pts).ok().and_then(|l| Region::new(vec![l]).ok())
}

/// 2D tracing: the cutter center runs exactly on the region's loops at
/// successive depths - the drawn line itself, engraved (the plan's
/// "buffer 0"). Wall finishing is contour's job; how the tool EDGE gets
/// onto the line (G41/G42, or an r-offset at the post) is the same
/// recorded-compensation question contour already defers to M1.5.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TraceParams {
    pub clearance_z: f64,
    /// Hop plane between loops of the same level (engraved lines already
    /// cut their own groove, so a low hop is safe there). None hops to
    /// `clearance_z` - the legacy shape, byte-stable goldens.
    pub retract_z: Option<f64>,
    /// First cut level; material is engaged from here down.
    pub top_z: f64,
    pub bottom_z: f64,
    pub stepdown: f64,
    pub feed: f64,
    pub plunge_feed: f64,
    /// Canonical (material-on-the-left) travel; conventional reverses
    /// each loop.
    pub climb: bool,
}

impl TraceParams {
    pub(super) fn validate(&self) -> Result<(), GenError> {
        let mut finite = [
            self.clearance_z,
            self.top_z,
            self.bottom_z,
            self.stepdown,
            self.feed,
            self.plunge_feed,
        ]
        .into_iter()
        .all(f64::is_finite);
        finite &= self.retract_z.is_none_or(f64::is_finite);
        if !finite {
            return Err(GenError::Invalid("parameters must be finite".into()));
        }
        if self.stepdown <= 0.0 {
            return Err(GenError::Invalid("stepdown must be positive".into()));
        }
        if self.feed <= 0.0 || self.plunge_feed <= 0.0 {
            return Err(GenError::Invalid("feeds must be positive".into()));
        }
        if self.top_z < self.bottom_z {
            return Err(GenError::Invalid("cut top below cut bottom".into()));
        }
        if self.clearance_z < self.top_z {
            return Err(GenError::Invalid("clearance plane below the first cut level".into()));
        }
        if self.retract_z.is_some_and(|rz| rz < self.top_z) {
            return Err(GenError::Invalid("retract plane sits inside the material".into()));
        }
        Ok(())
    }
}

/// Trace every loop of `region` - outer loops and holes alike are just
/// lines to engrave - ordered by `|area|` descending for determinism,
/// each traversal closed so the ring's cut joins at its start.
pub fn trace(region: &Region, p: &TraceParams) -> Result<Toolpath, GenError> {
    p.validate()?;
    if region.is_empty() {
        return Err(GenError::Empty("empty trace region".into()));
    }
    let mut loops: Vec<&Loop> = region.loops().iter().collect();
    loops.sort_by(|x, y| y.area().abs().total_cmp(&x.area().abs()));
    let mut passes: Vec<Vec<Point2>> = Vec::with_capacity(loops.len());
    for l in &loops {
        let mut pass: Vec<Point2> = if p.climb {
            l.points().to_vec()
        } else {
            l.points().iter().rev().copied().collect()
        };
        if let Some(&start) = pass.first() {
            pass.push(start);
        }
        passes.push(pass);
    }

    let zs = cut_levels(p.top_z, p.bottom_z, p.stepdown)?;
    let mut b = PathBuilder::approached(Point3::new(0.0, 0.0, p.clearance_z));
    // Trace engraves the region's own vertices - the point is to ride
    // the boundary as picked, so no arc fitter runs here.
    //
    // The M8.2 C entry verdict for this generator: NO knob, and that is
    // a theorem, not a gap. The cutter center rides the design line
    // itself - a zero-width kerf. A ramp or a helix certifies against
    // bounded air around the pass start; here every neighborhood of the
    // start straddles the line, so no region exists the descent laws
    // could read, and any "entry angle" control would promise a
    // certification the geometry can never supply. The vertical plunge
    // onto the engraved point is the only legal first engagement (and
    // for an engraved line the whole cut is one kerf deep - nothing
    // above the level is material to avoid). M11.3: it still rapids to
    // the plunge height over the top before it feeds.
    let motion = Motion::new(p.clearance_z, p.retract_z, p.feed, p.plunge_feed).with_top(p.top_z);
    run_levels(&mut b, &passes, &zs, &motion, None, None, None);
    let cur = b.at();
    b.push(MoveKind::Rapid, Point3::new(cur.x, cur.y, p.clearance_z), None);

    let path = b.finish();
    if let Some(why) = crate::approach::refusal(&path) {
        // #261 part 3: the moves built so far ride the refusal, display-only.
        return Err(GenError::Invalid(why).with_ghost(&path));
    }
    if !path.all_finite() {
        return Err(GenError::Invalid("path left the finite horizon".into()));
    }
    Ok(path)
}

/// The chord law lives in `chord_clear` (#1620); re-exported here, where
/// its callers have always found it.
pub use super::chord_clear::chord_clears;
