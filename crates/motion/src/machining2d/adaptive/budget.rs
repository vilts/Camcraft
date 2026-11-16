//! The growth rings' planning budget (issue #13): the ring spiral
//! (`grow`) re-walks the whole cleared outline every revolution, so its
//! work grows with the square of how far the fronts must travel - a long
//! narrow channel (the band an Outside containment leaves round a part)
//! takes hundreds of revolutions, each over a ring hundreds of mm long,
//! and planned for minutes. Two laws keep it bounded and named:
//!
//! 1. **Up front**, the least work the spiral can need: the channel's
//!    length `L = area / (2 c)` (its center area over its width, `c` the
//!    clearance at the entry), its fronts each travelling `L / 2` at no
//!    more than the step cap a revolution, over rings averaging `L`
//!    round: `L^2 / (2 cap)`. Past [`LEAST_WORK_MAX`] tool radii the
//!    area is refused at once, by name.
//! 2. **While growing**, the length of every revolution a margin attempt
//!    tries is summed; past [`WORK_MAX`] tool radii the plan stops and is
//!    refused, whatever the shape (the Prime Directive's "never hang").
//! 3. **While growing** (issue #850), the polygon geometry every step
//!    tried runs through is counted too: the vertices of the cleared area
//!    it offsets and of the ring it opens, and of each revolution's
//!    margin area. Length does not bound time - an islanded area's rings
//!    carry the island's outline through every trial, and its plan spent
//!    ~2 ms a tool radius against ~1 ms in a plain channel, running about a
//!    minute before law 2 refused it. A plan that has worked past
//!    [`GEOM_MAX`] vertices AND walked past [`GEOM_LEN`] tool radii of
//!    revolutions - many revolutions of heavy rings, a plan far from done -
//!    is refused the same way. A few revolutions round a finely
//!    tessellated outline (the user's STEP bore: 33 revolutions, 1.1 M
//!    vertices, 2.6 k r) finish as before: the vertex count alone measures
//!    outline resolution, not how far the plan is from done. The counts
//!    are deterministic (never a wall clock): the same project plans, or
//!    is refused, on every machine.
//!
//! Laws 2 and 3 hold **each margin attempt on its own** (issue #1051;
//! [`Work::attempt`]). The planner grows an area from a wall margin and,
//! where the margin is spent before the fronts are done, grows it again
//! from a wider one (`MARGIN_SCALES`, three rungs). Summed over the
//! attempts, the sums jumped by a whole attempt's work wherever the first
//! margin ran out a revolution early or late - a matter of nanometres of
//! ring geometry - and the second attempt, of a plan that fits the budget
//! from either margin, was refused for what the first had spent: the
//! rotary bar's developed level at radius 5.0 was refused at 22.5 k +
//! 7.6 k r, and planned whole in 22.3 k r with its openings 250 nm
//! deeper. Per attempt, a sum moves by under 1 % under such a change, so
//! only a plan that close to a limit can turn on it. The whole plan is
//! bounded by the rungs: at most three times the limits.
//!
//! The refused area is cut by the Offset rings, named (`pocket_hsm`).
//! Ours: the bound is plain arithmetic on the ring family's geometry;
//! the limits are calibrated on our own fixtures (the 60 x 14 slot needs
//! ~110 r up front and ~5.8 k r grown; the user's 14 mm band round a
//! 60 x 50 part ~3.4 k r up front). No vendor material consulted.

use std::cell::Cell;

use crate::geo2d::Region;

/// The least growth work (tool radii of revolution length) an area may
/// need before it is refused up front.
pub(super) const LEAST_WORK_MAX: f64 = 2_000.0;

/// The growth work (tool radii of revolutions tried) a margin attempt
/// may spend before the plan is refused.
pub(super) const WORK_MAX: f64 = 30_000.0;

/// The growth geometry (polygon vertices the trial steps and the
/// revolutions' margin areas ran through) a margin attempt may spend
/// past [`GEOM_LEN`] before the plan is refused. Calibrated on our
/// fixtures, heavy lanes included
/// (`docs/decisions/850-adaptive-geometry-budget.md`): of the plans that
/// walk past `GEOM_LEN` and finish, the heaviest (the r6.2 rotary neck)
/// spends ~358 k; the 60 x 40 pocket round a 12 x 12 island ~865 k in its
/// first margin attempt alone.
pub(super) const GEOM_MAX: u64 = 600_000;

/// The revolution length (tool radii, one margin attempt) past which
/// the geometry budget applies. The plans that finish within it - the
/// user's STEP bore, 2.6 k r over 1.1 M vertices - are law 2's alone.
pub(super) const GEOM_LEN: f64 = 5_000.0;

/// The growth plan's running work: revolution length (mm, law 2) and
/// polygon vertices (law 3), summed over one margin attempt, and the
/// law-3 limits it is held to ([`GEOM_MAX`], [`GEOM_LEN`]; a test lowers
/// them through [`Work::with_limits`]).
#[derive(Debug)]
pub(super) struct Work {
    len: Cell<f64>,
    verts: Cell<u64>,
    geom_max: u64,
    geom_len: f64,
}

impl Default for Work {
    fn default() -> Self {
        Self { len: Cell::new(0.0), verts: Cell::new(0), geom_max: GEOM_MAX, geom_len: GEOM_LEN }
    }
}

impl Work {
    /// A plan's work held to law 3 at `geom_max` vertices over `geom_len`
    /// tool radii (tests: a refusal reached in a few revolutions).
    #[cfg(test)]
    pub(super) fn with_limits(geom_max: u64, geom_len: f64) -> Self {
        Self { geom_max, geom_len, ..Self::default() }
    }

    /// A margin attempt begins: its work is counted from nothing (issue
    /// #1051 - what an attempt that ran out of margin spent is no measure
    /// of the next one).
    pub(super) fn attempt(&self) {
        self.len.set(0.0);
        self.verts.set(0);
    }

    /// A revolution `mm` long was tried. A NaN or non-positive length
    /// (degenerate input) is not counted: it would poison the sum and
    /// switch law 2 off for every later revolution.
    pub(super) fn add_len(&self, mm: f64) {
        if mm.is_finite() && mm > 0.0 {
            self.len.set(self.len.get() + mm);
            #[cfg(test)]
            PEAK_LEN.with(|p| p.set(p.get().max(self.len.get())));
        }
    }

    /// The step ran through `regions`' vertices.
    pub(super) fn add_geometry(&self, regions: &[&Region]) {
        let n = regions.iter().flat_map(|r| r.loops()).fold(0u64, |a, l| a.saturating_add(u64::try_from(l.points().len()).unwrap_or(u64::MAX)));
        self.verts.set(self.verts.get().saturating_add(n));
        #[cfg(test)]
        PEAK.with(|p| p.set(p.get().max(self.verts.get())));
    }

    /// The vertices counted so far.
    #[cfg(test)]
    pub(super) fn verts(&self) -> u64 {
        self.verts.get()
    }

    /// Why the plan is refused after `revs` revolutions, for a tool of
    /// radius `r`: past [`WORK_MAX`], or past both [`GEOM_MAX`] and
    /// [`GEOM_LEN`]; `None` otherwise.
    pub(super) fn over(&self, r: f64, revs: usize) -> Option<String> {
        let len = self.len.get();
        if len > WORK_MAX * r {
            Some(over_work(revs))
        } else if self.verts.get() > self.geom_max && len > self.geom_len * r {
            Some(over_geometry(revs, (self.verts.get(), self.geom_max), (len / r.max(1e-9), self.geom_len)))
        } else {
            None
        }
    }
}

#[cfg(test)]
thread_local! {
    /// The most geometry any margin attempt on this thread spent (tests).
    pub(super) static PEAK: Cell<u64> = const { Cell::new(0) };
    /// ... and the most revolution length (mm).
    pub(super) static PEAK_LEN: Cell<f64> = const { Cell::new(0.0) };
}

/// The channel an area reads as: its length (mm) from its center
/// `area` and the clearance `clear` at the entry.
pub(super) fn channel_len(area: f64, clear: f64) -> f64 {
    if clear > 1e-9 { area / (2.0 * clear) } else { f64::INFINITY }
}

/// The least growth work (tool radii) for a channel `len` long, fronts
/// stepping at most `cap` mm a revolution, for a tool of radius `r`;
/// with the least revolutions it takes.
pub(super) fn least_work(len: f64, cap: f64, r: f64) -> (f64, f64) {
    let revs = 0.5 * len / cap.max(1e-9);
    (revs * len / r.max(1e-9), revs)
}

/// The refusal for a channel `width` mm wide (cutter reach included)
/// and `len` long that would need `revs` revolutions at least.
pub(super) fn too_long(width: f64, len: f64, revs: f64) -> String {
    format!(
        "is refused: the area is a channel {width:.1} mm wide and ~{len:.0} mm long (the band round a part?) - one adaptive spiral would need at least {revs:.0} revolutions along it, each round a ring up to ~{:.0} mm long: too narrow and long to plan", 2.0 * len
    )
}

/// The refusal once the growth work ran past [`WORK_MAX`].
pub(super) fn over_work(revs: usize) -> String {
    format!("is refused: growing the cleared area ran past its planning budget after {revs} revolutions")
}

/// The refusal once the growth geometry ran past both law-3 limits:
/// `verts` outline vertices over its limit and `len` tool radii of
/// revolutions over its limit.
pub(super) fn over_geometry(revs: usize, (verts, verts_max): (u64, u64), (len, len_max): (f64, f64)) -> String {
    format!(
        "is refused: growing the cleared area ran past its planning budget after {revs} revolutions - its outline is too detailed and its path too long to plan in seconds (an island?): both limits passed, ~{:.1} k outline vertices processed (limit {} k) and ~{:.1} k tool radii of path (limit {:.1} k)",
        verts as f64 / 1000.0,
        verts_max / 1000,
        len / 1000.0,
        len_max / 1000.0
    )
}

#[cfg(test)]
#[path = "budget_fixtures.rs"]
mod fixtures;

#[cfg(test)]
#[path = "budget_twins.rs"]
mod twins;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_long_band_needs_far_more_work_than_a_short_slot() {
        // The 60 x 14 slot (center 50 x 4) and the 14 mm band round a
        // 60 x 50 part (center ~275 x 4), D10 at the default load.
        let cap = 1.5 * 2.0 * 5.0 * crate::load::DEFAULT_OPTIMAL_LOAD;
        let (slot, _) = least_work(channel_len(200.0, 2.0), cap, 5.0);
        let (band, revs) = least_work(channel_len(1100.0, 2.0), cap, 5.0);
        assert!(slot < 0.1 * LEAST_WORK_MAX && band > 1.5 * LEAST_WORK_MAX, "slot {slot}, band {band}");
        assert!(too_long(14.0, 275.0, revs).contains("14.0 mm wide"));
        assert_eq!(channel_len(10.0, 0.0), f64::INFINITY);
    }

    #[test]
    fn the_geometry_budget_refuses_by_name_and_saturates() {
        let Ok(sq) = crate::geo2d::Loop::new(vec![
            crate::geo2d::Point2::new(0.0, 0.0),
            crate::geo2d::Point2::new(1.0, 0.0),
            crate::geo2d::Point2::new(1.0, 1.0),
            crate::geo2d::Point2::new(0.0, 1.0),
        ]) else {
            panic!("square")
        };
        let Ok(reg) = Region::new(vec![sq]) else { panic!("region") };
        let w = Work::default();
        w.add_len(f64::NAN);
        w.add_geometry(&[&reg, &reg]);
        assert_eq!(w.verts(), 8);
        assert!(w.over(5.0, 3).is_none());
        w.add_len(WORK_MAX * 5.0 + 1.0);
        assert!(w.over(5.0, 3).is_some_and(|m| m.contains("planning budget after 3 revolutions")));
        // Heavy geometry over a short walk (a finely tessellated outline
        // nearly done) is law 2's alone; past `GEOM_LEN` it is refused.
        let w = Work::default();
        w.verts.set(GEOM_MAX);
        w.add_len(GEOM_LEN * 5.0 - 1.0);
        assert!(w.over(5.0, 9).is_none());
        w.add_geometry(&[&reg]);
        assert!(w.over(5.0, 9).is_none(), "the bore's kind: heavy, short");
        w.add_len(2.0);
        let Some(m) = w.over(5.0, 9) else { panic!("over the geometry budget") };
        assert!(m.contains("planning budget after 9 revolutions") && m.contains("too detailed and its path too long") && m.contains("both limits passed"), "{m}");
        assert!(m.contains("(limit 600 k)") && m.contains("(limit 5.0 k)"), "{m}");
        w.verts.set(u64::MAX);
        w.add_geometry(&[&reg]);
        assert_eq!(w.verts(), u64::MAX);
    }

    /// Issue #1051: a margin attempt is held to the budget on its own.
    /// Summed over the attempts, a plan whose first margin ran out after
    /// 22.5 k r was refused 7.6 k r into its second; the same plan
    /// finishing inside its first margin (22.3 k r) was kept.
    #[test]
    fn each_margin_attempt_is_held_to_the_budget_on_its_own() {
        let (r, w) = (3.0, Work::default());
        w.add_len(22_455.0 * r);
        w.verts.set(380_448);
        assert!(w.over(r, 144).is_none(), "the first attempt is within both laws");
        w.attempt();
        assert_eq!((w.verts(), w.len.get()), (0, 0.0), "the next attempt counts from nothing");
        w.add_len(24_639.0 * r);
        w.verts.set(398_169);
        assert!(w.over(r, 150).is_none(), "the second attempt is not refused for what the first spent");
        // ... and each attempt is still refused past its own limits.
        w.add_len((WORK_MAX - 24_639.0) * r + 1.0);
        assert!(w.over(r, 151).is_some_and(|m| m.contains("planning budget after 151 revolutions")));
        w.attempt();
        w.verts.set(GEOM_MAX + 1);
        w.add_len(GEOM_LEN * r + 1.0);
        assert!(w.over(r, 9).is_some_and(|m| m.contains("both limits passed")));
    }

    /// A NaN (or non-positive) revolution length is degenerate input: it
    /// neither refuses the plan nor switches law 2 off. Before #850 a NaN
    /// poisoned the sum and no later revolution was ever refused.
    #[test]
    fn a_nan_length_neither_refuses_nor_disables_the_budget() {
        let w = Work::default();
        for bad in [f64::NAN, f64::INFINITY, -1.0, 0.0] {
            w.add_len(bad);
        }
        assert!(w.over(5.0, 1).is_none(), "nothing real was walked");
        w.add_len(WORK_MAX * 5.0 - 1.0);
        assert!(w.over(5.0, 2).is_none());
        w.add_len(f64::NAN);
        w.add_len(2.0);
        assert!(w.over(5.0, 3).is_some_and(|m| m.contains("planning budget after 3 revolutions")), "law 2 still refuses");
    }
}
