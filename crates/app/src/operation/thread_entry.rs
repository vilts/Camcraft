//! The thread mill's way in (#992): a thread mill is not centre-cutting,
//! and the thread ride feeds it down the hole's axis and out along its
//! lead-in arc. That is air only when an earlier operation opened the
//! hole, so each threaded hole is read in the stock the program ahead
//! leaves standing (the prior grid the drilling laws read,
//! [`super::drill_prior`]) and a hole not open to the thread's depth is
//! refused by name - never a thread mill fed into solid metal. An
//! external thread (#985) has the same law the other way out: the mill
//! feeds down BESIDE the boss, so the stock around the boss must be
//! machined away first ([`judge_bosses`]).
//!
//! Provenance: public machine-tool handbook thread milling (the tap-drill
//! hole is made first, to the minor diameter, deeper than the thread);
//! the prior-grid reading is our own design. No vendor material consulted.

use crate::cl::Toolpath;
use crate::drilling::Hole;
use crate::geo2d::Point2;
use crate::machining2d::GenError;
use crate::stock_sim::{Columns, Dexel};

/// Float slack on the grid's stored heights, mm: a column cut exactly to
/// the thread mill's lowest point is air there.
const Z_SLACK: f64 = 1e-6;

/// The lowest point the path takes the tool to inside `hole` (its moves
/// whose ends stand within the hole's minor circle - every thread move
/// does, the door keeps the mill inside it), or `None` when the path
/// never enters the hole.
fn lowest_in(path: &Toolpath, hole: &Hole) -> Option<f64> {
    let inside = |p: crate::geometry::Point3| (p.x - hole.xy.x).hypot(p.y - hole.xy.y) <= hole.radius + 1e-6;
    path.moves
        .iter()
        .flat_map(|m| [m.from, m.to])
        .filter(|p| inside(*p))
        .map(|p| p.z)
        .fold(None, |lo: Option<f64>, z| Some(lo.map_or(z, |l| l.min(z))))
}

/// Why the program ahead has not opened every threaded hole, by name, or
/// `Ok` when each is air under the thread mill. `prior` is the grid the
/// setup's earlier operations leave standing; `None` (nothing ahead cut,
/// or what is ahead cannot be read, or a posed frame no column grid
/// reads) is unknown, which is refused - an unjudged hole is never air.
///
/// The disk read is the hole's minor circle less one grid cell (the
/// lattice's own resolution: a drill of exactly the minor diameter cuts
/// every column whose centre falls inside it), never narrower than the
/// mill itself, from the stock top down to the mill's lowest point.
pub(super) fn judge(holes: &[Hole], path: &Toolpath, tool_r: f64, prior: Option<&Dexel>) -> Result<(), GenError> {
    for hole in holes {
        let Some(low) = lowest_in(path, hole) else {
            continue;
        };
        let (x, y, d) = (hole.xy.x, hole.xy.y, hole.radius * 2.0);
        let advice = format!(
            "a thread mill does not cut at its centre - add a drilling operation of at least the {d:.3} mm minor diameter, deep enough for its point, before the thread"
        );
        let Some(dex) = prior else {
            return Err(GenError::Invalid(format!(
                "the hole at ({x:.3}, {y:.3}) is not drilled yet: nothing ahead of the thread in this setup's program opens it, or the program ahead cannot be read - {advice}"
            )));
        };
        let r = tool_r.max(hole.radius - dex.cell);
        let top = if r.is_finite() && r >= 0.0 {
            crate::verify::prior_surface(&Columns::of_grid(dex), hole.xy, r)
        } else {
            None
        };
        match top {
            Some(top) if top <= low + Z_SLACK => {}
            Some(top) => {
                return Err(GenError::Invalid(format!(
                    "the hole at ({x:.3}, {y:.3}) is not drilled yet: the stock the program ahead leaves stands at z {top:.3} inside it, above the thread mill's lowest point at z {low:.3} - {advice}"
                )));
            }
            None => {
                return Err(GenError::Invalid(format!(
                    "the hole at ({x:.3}, {y:.3}) cannot be judged: the stock the program ahead leaves is unreadable there - {advice}"
                )));
            }
        }
    }
    Ok(())
}

/// The external analogue (#985): the mill enters BESIDE a boss, from
/// outside, and cuts only the thread's own depth - so every column of
/// stock its body passes over, the boss itself aside, must already be
/// machined away down to its tip. `prior` is the grid the setup's
/// earlier operations leave standing; `None` is unknown, which is
/// refused. Read at every position the feeds take the tool through
/// (half a cell apart): each column whose centre the tool's full disc
/// covers and that stands more than one cell outside the boss's drawn
/// wall (the lattice's own resolution - an operation that finished the
/// boss cut every column whose centre lies outside it) must stand at or
/// below the tip there. Columns off the grid are outside the stock:
/// air. Rapids are the rapid law's, re-read against the same grid.
pub(super) fn judge_bosses(bosses: &[Hole], path: &Toolpath, tool_r: f64, prior: Option<&Dexel>) -> Result<(), GenError> {
    let Some(first) = bosses.first() else {
        return Ok(());
    };
    let advice = "a thread mill cuts only the thread's own depth";
    let Some(dex) = prior else {
        return Err(GenError::Invalid(format!(
            "the boss at ({:.3}, {:.3}) is not cleared: nothing ahead of the thread in this setup's program machines the stock around it, or the program ahead cannot be read - {advice} - clear the stock around the boss down to the thread's bottom before the thread",
            first.xy.x, first.xy.y
        )));
    };
    let cols = Columns::of_grid(dex);
    let cell = cols.cell();
    let unreadable = |x: &Hole| {
        GenError::Invalid(format!(
            "the boss at ({:.3}, {:.3}) cannot be judged: the stock the program ahead leaves is unreadable around it - {advice}",
            x.xy.x, x.xy.y
        ))
    };
    if !(cell.is_finite() && cell > 0.0 && tool_r.is_finite() && tool_r > 0.0) {
        return Err(unreadable(first));
    }
    if let Some(x) = bosses.iter().find(|x| cols.col(x.xy).is_none()) {
        return Err(unreadable(x));
    }
    // Where the feeds stand, each XY once at its lowest tip: a helix
    // revisits the same circle on every turn. Each stand keeps the boss
    // its run of feeds threads ([`super::thread_boss::owners`]).
    let owned = super::thread_boss::owners(path, bosses, tool_r);
    let mut low: std::collections::HashMap<(i64, i64, Option<usize>), crate::geometry::Point3> = std::collections::HashMap::new();
    let mut budget = 4_000_000usize;
    for (i, m) in path.moves.iter().enumerate().filter(|(_, m)| m.kind != crate::cl::MoveKind::Rapid) {
        let pts = super::thread_boss::samples(m, cell * 0.5);
        if pts.len() > budget || pts.iter().any(|p| !p.is_finite()) {
            return Err(unreadable(first));
        }
        budget -= pts.len();
        let own = owned.get(i).copied().flatten();
        for p in pts {
            let key = ((p.x * 1e3).round() as i64, (p.y * 1e3).round() as i64, own);
            low.entry(key).and_modify(|q| q.z = q.z.min(p.z)).or_insert(p);
        }
    }
    let (o, r2) = (cols.origin(), tool_r * tool_r);
    let span = |v: f64, o: f64, n: usize| {
        let lo = ((v - tool_r - o) / cell).floor().clamp(0.0, n as f64) as usize;
        let hi = ((v + tool_r - o) / cell).ceil().clamp(0.0, n as f64) as usize;
        lo..hi
    };
    // The worst standing column, so the refusal names one place and not the scan's first.
    let mut worst: Option<(f64, Point2, f64, Option<usize>)> = None;
    for ((_, _, own), p) in &low {
        // The boss being threaded keeps its own columns; feeds that orbit no listed boss keep none.
        let keep = own.and_then(|k| bosses.get(k)).map(|x| (x.xy, x.radius + cell));
        for iy in span(p.y, o.y, cols.ny()) {
            for ix in span(p.x, o.x, cols.nx()) {
                let c = cols.center(ix, iy);
                let (ex, ey) = (c.x - p.x, c.y - p.y);
                if ex * ex + ey * ey > r2 || keep.is_some_and(|(xy, out)| (c.x - xy.x).hypot(c.y - xy.y) <= out) {
                    continue;
                }
                let top = cols.top_at(ix, iy);
                if top.is_nan() {
                    return Err(unreadable(own.and_then(|k| bosses.get(k)).unwrap_or(first)));
                }
                let over = top - p.z;
                if over > Z_SLACK && worst.is_none_or(|w| over > w.0 || (over == w.0 && (c.x, c.y) < (w.1.x, w.1.y))) {
                    worst = Some((over, c, p.z, *own));
                }
            }
        }
    }
    let Some((over, c, tip, own)) = worst else {
        return Ok(());
    };
    let x = own.and_then(|k| bosses.get(k)).unwrap_or(first);
    // The ring the refusal names: how far out the tool's rim reaches round this boss, and how low.
    let (far, lowest) = low
        .iter()
        .filter(|((_, _, k), _)| *k == own)
        .fold((0.0f64, f64::INFINITY), |(far, lo), (_, p)| (far.max((p.x - x.xy.x).hypot(p.y - x.xy.y) + tool_r), lo.min(p.z)));
    Err(GenError::Invalid(format!(
        "the boss at ({:.3}, {:.3}) is not cleared: the stock the program ahead leaves stands at z {:.3} at ({:.3}, {:.3}), {:.3} mm from the boss's axis, above the thread mill's tip at z {tip:.3} there - {advice} - machine the stock around the boss away, out to {far:.3} mm from its axis and down to z {lowest:.3}, before the thread",
        x.xy.x,
        x.xy.y,
        tip + over,
        c.x,
        c.y,
        (c.x - x.xy.x).hypot(c.y - x.xy.y)
    )))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cl::{Move, MoveKind};
    use crate::geo2d::Point2;
    use crate::geometry::Point3;

    fn hole() -> Hole {
        Hole { xy: Point2::new(0.0, 0.0), radius: 3.3, entry_z: 20.0, floor_z: 8.0, through: false, stepped: false }
    }

    /// A plunge down the axis to z 8 and a step out to the wall.
    fn path() -> Toolpath {
        let mv = |from: Point3, to: Point3| Move::new(MoveKind::Feed, from, to, Some(100.0));
        let mut t = Toolpath::default();
        t.moves.push(mv(Point3::new(0.0, 0.0, 25.0), Point3::new(0.0, 0.0, 8.0)));
        t.moves.push(mv(Point3::new(0.0, 0.0, 8.0), Point3::new(0.3, 0.0, 8.0)));
        t
    }

    /// A 20 mm block over a 20 x 20 square, the hole's column cut to `cut_z` out to `cut_r`.
    fn block(cut_r: f64, cut_z: f64) -> Dexel {
        let mut d = Dexel::block(-10.0, -10.0, 10.0, 10.0, 20.0, 0.25).expect("block");
        d.cut_chord(Point3::new(0.0, 0.0, cut_z), Point3::new(0.0, 0.0, cut_z), cut_r);
        d
    }

    fn refusal(prior: Option<&Dexel>) -> String {
        judge(&[hole()], &path(), 3.0, prior).err().map(|e| e.to_string()).unwrap_or_default()
    }

    #[test]
    fn a_hole_drilled_to_the_minor_diameter_below_the_thread_is_air() {
        assert!(judge(&[hole()], &path(), 3.0, Some(&block(3.3, 5.0))).is_ok());
        // Cut exactly to the mill's lowest point is still air.
        assert!(judge(&[hole()], &path(), 3.0, Some(&block(3.3, 8.0))).is_ok());
    }

    #[test]
    fn solid_shallow_or_narrow_stock_under_the_mill_is_refused_by_name() {
        let solid = refusal(Some(&block(0.0, 20.0)));
        assert!(solid.contains("the hole at (0.000, 0.000) is not drilled yet") && solid.contains("6.600 mm minor diameter"), "{solid}");
        let shallow = refusal(Some(&block(3.3, 9.0)));
        assert!(shallow.contains("stands at z 9.000") && shallow.contains("lowest point at z 8.000"), "{shallow}");
        // Opened only to the mill's own width: the minor circle (less a cell) still holds stock.
        assert!(refusal(Some(&block(2.5, 5.0))).contains("not drilled yet"));
        assert!(refusal(Some(&block(3.0, 5.0))).contains("not drilled yet"));
    }

    #[test]
    fn an_unknown_prior_is_refused_never_read_as_air() {
        let none = refusal(None);
        assert!(none.contains("nothing ahead of the thread"), "{none}");
        // A hole off the grid has no reading at all.
        let mut far = hole();
        far.xy = Point2::new(50.0, 50.0);
        let mut p = path();
        for m in &mut p.moves {
            m.from.x += 50.0;
            m.from.y += 50.0;
            m.to.x += 50.0;
            m.to.y += 50.0;
        }
        let msg = judge(&[far], &p, 3.0, Some(&block(3.3, 5.0))).err().map(|e| e.to_string()).unwrap_or_default();
        assert!(msg.contains("cannot be judged"), "{msg}");
    }

    #[test]
    fn a_hole_the_path_never_enters_is_not_read() {
        let mut other = hole();
        other.xy = Point2::new(5.0, 5.0);
        other.radius = 1.0;
        assert!(judge(&[other], &path(), 0.5, None).is_ok());
    }

    /// An M8 boss (R 4) from z 10 to 20 at the origin, the Ø6 mill's
    /// first pass round it: down beside it, out and once around.
    fn boss() -> Hole {
        Hole { xy: Point2::new(0.0, 0.0), radius: 4.0, entry_z: 20.0, floor_z: 10.0, through: false, stepped: false }
    }

    fn orbit() -> Toolpath {
        let mut b = crate::cl::PathBuilder::approached(Point3::new(9.75, -3.0, 21.0));
        b.push(MoveKind::Feed, Point3::new(9.75, -3.0, 10.0), Some(100.0));
        b.push(MoveKind::Feed, Point3::new(6.75, 0.0, 10.0), Some(100.0));
        b.push_arc(MoveKind::Feed, Point3::new(-6.75, 0.0, 10.0), Some(100.0), Point2::new(0.0, 0.0), false);
        b.push_arc(MoveKind::Feed, Point3::new(6.75, 0.0, 10.0), Some(100.0), Point2::new(0.0, 0.0), false);
        b.finish()
    }

    /// A 22 mm block with everything further than `keep` from the axis milled down to `floor`.
    fn cleared(keep: f64, out_to: f64, floor: f64) -> Dexel {
        let mut d = Dexel::block(-20.0, -20.0, 20.0, 20.0, 22.0, 0.25).expect("block");
        for iy in 0..d.ny {
            for ix in 0..d.nx {
                let c = d.center(ix, iy);
                let rho = c.x.hypot(c.y);
                if rho > keep
                    && rho <= out_to
                    && let Some(k) = d.col(c)
                {
                    d.lower(k, floor);
                }
            }
        }
        d
    }

    fn boss_refusal(prior: Option<&Dexel>) -> String {
        judge_bosses(&[boss()], &orbit(), 3.0, prior).err().map(|e| e.to_string()).unwrap_or_default()
    }

    #[test]
    fn a_boss_standing_in_cleared_air_is_accepted() {
        assert_eq!(boss_refusal(Some(&cleared(4.0, 40.0, 10.0))), "");
        // Stock left on the boss inside one cell is the lattice's own resolution.
        assert_eq!(boss_refusal(Some(&cleared(4.2, 40.0, 10.0))), "");
        // No bosses, nothing to read.
        assert!(judge_bosses(&[], &orbit(), 3.0, None).is_ok());
    }

    #[test]
    fn stock_standing_round_the_boss_is_refused_by_name() {
        // Nothing machined round it at all.
        let solid = boss_refusal(Some(&Dexel::block(-20.0, -20.0, 20.0, 20.0, 22.0, 0.25).expect("block")));
        assert!(solid.contains("the boss at (0.000, 0.000) is not cleared") && solid.contains("stands at z 22.000"), "{solid}");
        assert!(solid.contains("thread mill's tip at z 10.000") && solid.contains("down to z 10.000"), "{solid}");
        // Cleared, but not as far out as the lead-in swings (the station's rim reaches 13.2 mm).
        let narrow = boss_refusal(Some(&cleared(4.0, 11.0, 10.0)));
        assert!(narrow.contains("is not cleared") && narrow.contains("out to 13.2"), "{narrow}");
        // Cleared, but not as deep as the thread runs.
        let shallow = boss_refusal(Some(&cleared(4.0, 40.0, 12.0)));
        assert!(shallow.contains("stands at z 12.000") && shallow.contains("tip at z 10.000"), "{shallow}");
        // The boss left a millimetre oversize: more than the thread mill may take.
        let fat = boss_refusal(Some(&cleared(5.0, 40.0, 10.0)));
        assert!(fat.contains("is not cleared"), "{fat}");
    }

    #[test]
    fn an_unknown_prior_round_a_boss_is_refused_never_read_as_air() {
        let none = boss_refusal(None);
        assert!(none.contains("the boss at (0.000, 0.000) is not cleared: nothing ahead of the thread"), "{none}");
        let mut off = boss();
        off.xy = Point2::new(80.0, 0.0);
        let msg = judge_bosses(&[off], &orbit(), 3.0, Some(&cleared(4.0, 40.0, 10.0))).err().map(|e| e.to_string()).unwrap_or_default();
        assert!(msg.contains("cannot be judged"), "{msg}");
        assert!(judge_bosses(&[boss()], &orbit(), f64::NAN, Some(&cleared(4.0, 40.0, 10.0))).is_err());
    }
}
