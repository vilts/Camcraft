//! Issue #369: a lathe-held tool body judged on every move of a turning op against the revolved in-process stock - the shop rule that only a tool's edge meets the work, its holder never (machine-tool handbooks), read about the spindle as a body of revolution: a body point (x, y, z) from the tip at (station S, signed radius X) stands sqrt((X + x)^2 + y^2) from the axis at station S + z, and rubs when material stands there; our own design, no vendor material consulted.
//!
//! ## The laws here
//!
//! - **Every move, along its length.** Feeds and rapids alike are walked
//!   at [`PITCH`] steps (and every arc chorded in its own plane); at each
//!   instant the body's clearance bands ([`LatheClearance`]) are read
//!   against the material standing then. A move that leaves the turning
//!   plane cannot be read in the meridian, so a bodied tool refuses it by
//!   name rather than pass it unjudged.
//! - **Exact on the bodies' skins, sampled on the cuts.** The part and
//!   stock are read on a station table holding every knot of their
//!   outlines, every facet vertex station and a fine grid between, so
//!   between two table stations a skin runs straight. Each body band is
//!   read at its two ends (the skins interpolated there) and at every
//!   table station inside it, so its worst reading against a skin is
//!   never missed. The cuts are read exactly at each of those stations;
//!   between them they are sampled, as every meridian law samples.
//! - **What stands.** The part, always (never cut, a rub by name), and
//!   the stock less every cut before the instant: the ops and pilots
//!   ahead, the program ahead's facing, this path's earlier cutting
//!   moves, and this move's own sweep up to the instant.
//! - **The insert's region is its footprint.** The excluded zone is the
//!   tool's own modelled cutting footprint (the nose disc, the blade's
//!   band, the thread's V, the form crown) swept from the move's start to
//!   the instant: whatever the cut model says the insert removes is air
//!   to the body, so the holder over the insert's flank never rubs the
//!   chip the insert is cutting. Beyond it, only the body's own insert
//!   zone (#396, a box about the tip one insert could fill) is left out,
//!   and that already in its clearance bands. Nothing else is excluded -
//!   a body part ahead of the edge in uncut stock, or anywhere in the
//!   part, rubs.
//! - **One finding per move**, blocking the post, naming the move and
//!   the clearance shortfall: the radial depth the body reaches into the
//!   material (the part's when it touches the part, else the stock's).

use camcraft_tools::lathe_holder::LatheClearance;

use crate::cl::{Move, MoveKind, Toolpath};
use crate::stock_sim::LatheProfile;
use crate::tools::Tool;
use crate::turning::profile::Frame;
use crate::verify::Warning;

use super::drill_prior::PriorCell;
use super::kinds::Section;
use super::turning_credit::{pilots, Pilot};
use super::turning_holder_scene::{material, pilot_removed, standing, Annuli, Kerf, Table, PITCH};
use super::turning_outline::Revolved;
use super::turning_rapids::{faced_end, kerf_of, Meridian, SignedMeridian};
use super::OpParams;

/// Most instants one straight piece is read at.
const MAX_SAMPLES: f64 = 4000.0;
/// The sagitta a meridian arc is chorded at (mm).
const ARC_TOL: f64 = 1e-3;
/// Width of one kerf bucket along the spindle (mm).
const BUCKET: f64 = 2.0;
/// Most kerf buckets.
const MAX_BUCKETS: f64 = 100_000.0;

/// A meridian segment, (station, signed radius) at each end.
type Seg = ((f64, f64), (f64, f64));

/// The path with the "holder not judged" note, once: a turning op whose
/// turning steel carries no lathe body.
pub(super) fn noted(mut path: Toolpath, params: &OpParams, tool: &Tool, has_body: bool) -> Toolpath {
    if has_body || params.kind().info().section != Section::Turning || crate::stock_sim::lathe::profile_of(tool).is_none() {
        return path;
    }
    let n = crate::tool_body_lathe::HOLDER_NOTE.to_owned();
    if !path.notes.contains(&n) {
        path.notes.push(n);
    }
    path
}

/// The world directions a lathe body's tool frame stands in on `path`
/// (#1038): +x the path's signed radial, +z the spindle - the very
/// frame [`judge`] reads a body point (x, y, z) in, fitted from the same
/// part, so the drawn holder is the judged one. Refuses by name when no
/// spindle fits the part.
pub(crate) fn body_axes(part: &[crate::geometry::Triangle], path: &Toolpath) -> Result<([f64; 3], [f64; 3]), String> {
    let frame = Frame::about(part).map_err(|e| format!("no spindle fits the part ({e})"))?;
    let (dir, radial) = SignedMeridian::of(&frame, path).axes();
    Ok(([radial.x, radial.y, radial.z], [dir.x, dir.y, dir.z]))
}

/// The holder law (#369) on `path`, for a tool whose lathe body is
/// `body`: one [`Warning::HolderHit`] per rubbing move. Nothing for an op
/// outside the turning section or a tool with no body. Refuses by name
/// when a body cannot be judged at all.
pub(super) fn judge(
    params: &OpParams,
    path: &Toolpath,
    cx: &Meridian,
    body: Option<&LatheClearance>,
    tool: &Tool,
    prior: &mut PriorCell,
    warns: &mut Vec<Warning>,
) -> Result<(), String> {
    let Some(body) = body else { return Ok(()) };
    if params.kind().info().section != Section::Turning {
        return Ok(());
    }
    let unjudged = |why: String| format!("the lathe holder body cannot be judged: {why}");
    let profile = crate::stock_sim::lathe::profile_of(tool).ok_or_else(|| unjudged(format!("a {} cuts with no turning footprint", tool.kind)))?;
    profile.validate().map_err(|e| unjudged(e.to_string()))?;
    let frame = Frame::about(cx.part).map_err(|e| unjudged(format!("no spindle fits the part ({e})")))?;
    let part = Revolved::of(cx.part, &frame).ok_or_else(|| unjudged("the part has a non-finite vertex".into()))?;
    let stock = match cx.stock {
        Some(s) => Some(Revolved::of(s, &frame).ok_or_else(|| unjudged("the stock has a non-finite vertex".into()))?),
        None => None,
    };
    let sm = SignedMeridian::of(&frame, path);
    let mut pieces = Vec::with_capacity(path.moves.len());
    for (i, m) in path.moves.iter().enumerate() {
        pieces.push(pieces_of(m, &sm, cx.z_tol).ok_or_else(|| unjudged(format!("move {i} leaves the turning plane")))?);
    }
    let margin = if cx.margin.is_finite() { cx.margin.max(0.0) } else { 0.0 };
    let mut kerfs: Vec<Kerf> = Vec::new();
    for (i, _) in path.moves.iter().enumerate().filter(|(_, m)| m.kind != MoveKind::Rapid) {
        kerfs.extend(pieces[i].iter().filter_map(|&(p, q)| sweep(&profile, p, q, margin, Some(i))));
    }
    for a in cx.ahead {
        for m in a.moves.iter().filter(|m| m.kind != MoveKind::Rapid) {
            if let Some((p, q)) = kerf_of(m, &sm, cx.z_tol) {
                kerfs.extend(Kerf::new(vec![[p.1, p.0], [q.1, q.0]], a.nose + margin, None));
            }
        }
    }
    let (s_min, s_max) = pieces.iter().flatten().flat_map(|&(p, q)| [p.0, q.0]).fold((f64::INFINITY, f64::NEG_INFINITY), |(l, h), s| (l.min(s), h.max(s)));
    if s_min > s_max {
        return Ok(()); // no move, nothing to judge
    }
    let (lo, hi) = (s_min + body.z_lo() - 1.0, s_max + body.z_hi() + 1.0);
    // Where the skins bend (their outlines' knots) and where the hole
    // walls can (every facet vertex's station).
    let mut vertices: Vec<f64> = cx.part.iter().chain(cx.stock.unwrap_or(&[]).iter()).flat_map(|t| [t.a, t.b, t.c]).map(|q| frame.sr(q).0).collect();
    vertices.extend(part.knots());
    vertices.extend(stock.iter().flat_map(Revolved::knots));
    let table = Table::new(&part, stock.as_ref(), lo, hi, &vertices).ok_or_else(|| unjudged("the path's stations are not finite".into()))?;
    let scene = Scene { table, buckets: Buckets::new(&kerfs, lo, hi), kerfs, pilots: pilots(cx.drills, &sm), spans: body.spans(), profile, margin };
    let mut cap: Option<Option<f64>> = None; // the faced end, read once
    for (i, piece) in pieces.iter().enumerate() {
        let verdict = match scene.judge_move(i, piece, None) {
            // Only a stock conviction pays for the program ahead's
            // simulation: the part is never faced away.
            Some(h) if !h.part => {
                let c = *cap.get_or_insert_with(|| faced_end(prior, &frame, &sm, cx));
                if c.is_some() { scene.judge_move(i, piece, c) } else { Some(h) }
            }
            v => v,
        };
        if let Some(h) = verdict {
            warns.push(Warning::HolderHit { at: i, part: h.part, short_mm: h.depth });
        }
    }
    Ok(())
}

/// A move's meridian pieces: its chord, a thread's helix read at its
/// constant radius, or a meridian arc's chords; `None` when it leaves
/// the turning plane.
fn pieces_of(m: &Move, sm: &SignedMeridian, tol: f64) -> Option<Vec<Seg>> {
    if !sm.in_plane(m.from, tol) || !sm.in_plane(m.to, tol) {
        return None;
    }
    if m.arc.is_none() {
        return Some(vec![(sm.sr(m.from), sm.sr(m.to))]);
    }
    if let Some(pq) = kerf_of(m, sm, tol) {
        return Some(vec![pq]);
    }
    let chords = m.segments3(ARC_TOL);
    if chords.iter().any(|(p, q)| !sm.in_plane(*p, tol) || !sm.in_plane(*q, tol)) {
        return None;
    }
    Some(chords.into_iter().map(|(p, q)| (sm.sr(p), sm.sr(q))).collect())
}

/// The tool's footprint swept from `p` to `q` (meridian (s, rho)), its
/// reach grown by the margin - the meridian laws' kerf credit.
fn sweep(profile: &LatheProfile, p: (f64, f64), q: (f64, f64), margin: f64, at: Option<usize>) -> Option<Kerf> {
    let (hull, round) = profile.swept_outline(p.1, p.0, q.1, q.0).ok()?;
    Kerf::new(hull, round + margin, at)
}

/// A move's verdict: on the part or only on stock, and the worst depth.
#[derive(Clone, Copy)]
struct Hit {
    part: bool,
    depth: f64,
}

impl Hit {
    fn worse(a: Option<Self>, b: Self) -> Self {
        match a {
            None => b,
            Some(a) if a.part != b.part => if a.part { a } else { b },
            Some(a) => Hit { part: a.part, depth: a.depth.max(b.depth) },
        }
    }
}

/// The kerfs by station, so an instant reads only the cuts near it.
struct Buckets {
    lo: f64,
    width: f64,
    lists: Vec<Vec<usize>>,
}

impl Buckets {
    fn new(kerfs: &[Kerf], lo: f64, hi: f64) -> Self {
        let width = BUCKET.max((hi - lo) / MAX_BUCKETS);
        let n = ((hi - lo) / width).ceil().clamp(1.0, MAX_BUCKETS) as usize;
        let mut lists = vec![Vec::new(); n];
        let at = |s: f64| ((s - lo) / width).floor().clamp(0.0, (n - 1) as f64) as usize;
        for (i, k) in kerfs.iter().enumerate() {
            for list in lists.iter_mut().take(at(k.s_hi) + 1).skip(at(k.s_lo)) {
                list.push(i);
            }
        }
        Self { lo, width, lists }
    }

    fn near(&self, s: f64) -> &[usize] {
        let n = self.lists.len();
        let k = ((s - self.lo) / self.width).floor().clamp(0.0, (n.max(1) - 1) as f64) as usize;
        self.lists.get(k).map_or(&[], Vec::as_slice)
    }
}

/// Everything one path's holder is judged against.
struct Scene<'a> {
    table: Table<'a>,
    kerfs: Vec<Kerf>,
    buckets: Buckets,
    pilots: Vec<Pilot>,
    /// The body's runs of identical bands (`LatheClearance::spans`).
    spans: Vec<(f64, f64, camcraft_tools::lathe_holder::Reach)>,
    profile: LatheProfile,
    margin: f64,
}

impl Scene<'_> {
    /// Move `i` walked along its pieces; `cap` the station the program
    /// ahead faced to.
    fn judge_move(&self, i: usize, pieces: &[Seg], cap: Option<f64>) -> Option<Hit> {
        let mut worst: Option<Hit> = None;
        let mut done: Vec<Kerf> = Vec::new();
        for &(p, q) in pieces {
            let len = (q.0 - p.0).hypot(q.1 - p.1);
            if !len.is_finite() {
                continue; // the NonFinite law names it
            }
            let n = (len / PITCH.max(len / MAX_SAMPLES)).ceil().max(1.0) as usize;
            for k in 0..=n {
                let t = k as f64 / n as f64;
                let now = (p.0 + (q.0 - p.0) * t, p.1 + (q.1 - p.1) * t);
                let trail = sweep(&self.profile, p, now, self.margin, None);
                if let Some(h) = self.instant(i, now, &done, trail.as_ref(), cap) {
                    worst = Some(Hit::worse(worst, h));
                }
            }
            done.extend(sweep(&self.profile, p, q, self.margin, None));
        }
        worst
    }

    /// The body with its tip at `now` (station, signed radius) during
    /// move `i`, this move's cut so far `done` and `trail`.
    fn instant(&self, i: usize, now: (f64, f64), done: &[Kerf], trail: Option<&Kerf>, cap: Option<f64>) -> Option<Hit> {
        let (st, x) = now;
        let mut worst: Option<Hit> = None;
        let mut cuts: Vec<(f64, f64)> = Vec::new();
        // Each band at its two ends and at every table station inside:
        // between two of those the skins run straight, so the band's
        // worst reading is at one of them.
        for &(z0, z1, reach) in &self.spans {
            let (a, b) = (st + z0, st + z1);
            let radii = reach.radii(x);
            let inner = (self.table.first_at(a)..self.table.first_at(b)).filter(|&k| self.table.station(k) > a);
            let points = std::iter::once((a, self.table.annuli_at(a)))
                .chain(inner.map(|k| (self.table.station(k), self.table.annuli(k))))
                .chain(std::iter::once((b, self.table.annuli_at(b))));
            for (s, an) in points {
                if let Some(h) = self.reading(i, s, an, radii, done, trail, cap, &mut cuts) {
                    worst = Some(Hit::worse(worst, h));
                }
            }
        }
        worst
    }

    /// The body's radial span `radii` at station `s` against the material
    /// `an` standing there before move `i`'s instant.
    #[allow(clippy::too_many_arguments)]
    fn reading(
        &self,
        i: usize,
        s: f64,
        an: Annuli,
        (r_lo, r_hi): (f64, f64),
        done: &[Kerf],
        trail: Option<&Kerf>,
        cap: Option<f64>,
        cuts: &mut Vec<(f64, f64)>,
    ) -> Option<Hit> {
        let overlap = |m: (f64, f64)| {
            let (l, u) = (m.0.max(r_lo), m.1.min(r_hi));
            (u > l).then_some((l, u))
        };
        if let Some((l, u)) = material(an.part, self.margin).and_then(overlap) {
            return Some(Hit { part: true, depth: u - l });
        }
        if cap.is_some_and(|c| s >= c - self.margin) {
            return None; // the program ahead faced the stock off here
        }
        let (l, u) = an.stock.and_then(|a| material(a, self.margin)).and_then(overlap)?;
        cuts.clear();
        for &j in self.buckets.near(s) {
            if let Some(kf) = self.kerfs.get(j).filter(|kf| kf.at.is_none_or(|a| a < i)) {
                kf.removed(s, cuts);
            }
        }
        for kf in done.iter().chain(trail) {
            kf.removed(s, cuts);
        }
        for p in &self.pilots {
            pilot_removed(p, s, cuts);
        }
        let d = standing(l, u, cuts);
        (d > 0.0).then_some(Hit { part: false, depth: d })
    }
}
