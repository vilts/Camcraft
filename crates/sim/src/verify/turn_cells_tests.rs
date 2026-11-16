//! #1313's brute-force probe: random turns, the band sampled densely
//! in path position, turn fraction and over its own surface - every
//! position must lie inside a cell, in XY and in Z. The five-pose grid
//! the law read before is probed beside it, to keep the defect's size
//! on record. Our own fixture; no vendor material.

use super::*;
use crate::cl::angles_to_axis;

/// A small deterministic generator (Numerical Recipes' LCG constants).
struct Lcg(u64);
impl Lcg {
    fn unit(&mut self) -> f64 {
        self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        (self.0 >> 11) as f64 / (1u64 << 53) as f64
    }
    fn within(&mut self, lo: f64, hi: f64) -> f64 {
        lo + (hi - lo) * self.unit()
    }
}

/// A shank and a holder, `(above, radius, length)`.
const BANDS: [(f64, f64, f64); 2] = [(8.0, 3.0, 22.0), (30.0, 12.0, 40.0)];
/// The cutter's radius: under both bands, so neither is ever skipped.
const R: f64 = 2.0;

fn axis_at(from: [f64; 3], to: [f64; 3], g: f64) -> [f64; 3] {
    angles_to_axis([0, 1, 2].map(|k| from[k] + (to[k] - from[k]) * g)).unwrap_or([0.0, 0.0, 1.0])
}

/// Points of the true band (a cylinder about the axis) at one pose:
/// three stations along it, the axis point and twelve rim points each.
fn band_points(tip: Point3, a: [f64; 3], (above, band_r, band_len): (f64, f64, f64)) -> Vec<Point3> {
    let h = a[0].hypot(a[1]);
    // Two unit vectors square to the axis.
    let u = if h > 1e-12 { [-a[1] / h, a[0] / h, 0.0] } else { [1.0, 0.0, 0.0] };
    let v = [a[1] * u[2] - a[2] * u[1], a[2] * u[0] - a[0] * u[2], a[0] * u[1] - a[1] * u[0]];
    let mut out = Vec::new();
    for s in [above, above + band_len * 0.5, above + band_len] {
        let c = [tip.x + s * a[0], tip.y + s * a[1], tip.z + s * a[2]];
        out.push(Point3::new(c[0], c[1], c[2]));
        for k in 0..12 {
            let (sn, cs) = (std::f64::consts::TAU * k as f64 / 12.0).sin_cos();
            out.push(Point3::new(
                c[0] + band_r * (cs * u[0] + sn * v[0]),
                c[1] + band_r * (cs * u[1] + sn * v[1]),
                c[2] + band_r * (cs * u[2] + sn * v[2]),
            ));
        }
    }
    out
}

/// Is the point inside some cell: over it in XY, at or above its bottom?
fn covered(cells: &[Cell], p: Point3) -> bool {
    let q = Point2::new(p.x, p.y);
    cells.iter().any(|c| c.bottom_over(q, -1.0).is_some_and(|h| h <= p.z + 1e-9))
}

/// The grid the law read before #1313: law (c)'s column at 4 path
/// stations x 5 turn fractions. Is the point inside one of those?
fn in_the_old_grid(m: (Point3, Point3), from: [f64; 3], to: [f64; 3], band: (f64, f64, f64), p: Point3) -> bool {
    [0.2, 0.4, 0.6, 0.8].iter().any(|&t| {
        let tip = lerp(m.0, m.1, t);
        [0.0, 0.25, 0.5, 0.75, 1.0].iter().any(|&g| {
            let (o, rad, b) = column(axis_at(from, to, g), band.0, band.1, band.2);
            (p.x - tip.x - o.0).hypot(p.y - tip.y - o.1) <= rad + 1e-9 && p.z + 1e-9 >= tip.z + b
        })
    })
}

fn lerp(a: Point3, b: Point3, s: f64) -> Point3 {
    Point3::new(a.x + (b.x - a.x) * s, a.y + (b.y - a.y) * s, a.z + (b.z - a.z) * s)
}

#[test]
fn no_band_position_of_a_random_turn_lies_outside_the_cells() {
    let mut rng = Lcg(1313);
    let (mut probed, mut old_xy_or_z_misses, mut old_turns_missing) = (0u64, 0u64, 0u32);
    const TURNS: u32 = 240;
    for turn in 0..TURNS {
        // A/B turns up to +-85 degrees, as the issue's probe; a third
        // also turn C, a third hold the tip still.
        let c = if turn % 3 == 0 { 60.0 } else { 0.0 };
        let from = [rng.within(-85.0, 85.0), rng.within(-85.0, 85.0), rng.within(-c, c)];
        let to = [rng.within(-85.0, 85.0), rng.within(-85.0, 85.0), rng.within(-c, c)];
        let p0 = Point3::new(rng.within(-20.0, 20.0), rng.within(-20.0, 20.0), rng.within(-10.0, 10.0));
        let run = if turn % 3 == 1 { 0.0 } else { 1.0 };
        let p1 = Point3::new(
            p0.x + run * rng.within(-30.0, 30.0),
            p0.y + run * rng.within(-30.0, 30.0),
            p0.z + rng.within(-15.0, 15.0),
        );
        let mut missed_before = false;
        for band in BANDS {
            let mut cells = Vec::new();
            push_cells(&mut cells, &[(p0, p1)], from, to, band, R, 0.0, &[]);
            assert!(!cells.is_empty() && cells.len() <= MAX_CELLS, "turn {turn}: {} cells", cells.len());
            for it in 0..=8 {
                let tip = lerp(p0, p1, it as f64 / 8.0);
                for ig in 0..=24 {
                    let a = axis_at(from, to, ig as f64 / 24.0);
                    // The true band, and law (c)'s own column for the
                    // pose (its centre and rim at its bottom): the
                    // cells hold both, so no sampled reading is lost.
                    let (o, rad, b) = column(a, band.0, band.1, band.2);
                    let mut pts = band_points(tip, a, band);
                    for k in 0..8 {
                        let (sn, cs) = (std::f64::consts::TAU * k as f64 / 8.0).sin_cos();
                        pts.push(Point3::new(tip.x + o.0 + rad * cs, tip.y + o.1 + rad * sn, tip.z + b));
                    }
                    for p in pts {
                        probed += 1;
                        assert!(
                            covered(&cells, p),
                            "turn {turn} {from:?} -> {to:?}, t {it}/8, g {ig}/24: ({}, {}, {}) is in no cell",
                            p.x,
                            p.y,
                            p.z
                        );
                        if !in_the_old_grid((p0, p1), from, to, band, p) {
                            old_xy_or_z_misses += 1;
                            missed_before = true;
                        }
                    }
                }
            }
        }
        old_turns_missing += u32::from(missed_before);
    }
    assert!(probed > 1_000_000, "{probed} positions probed");
    // The defect on record: the old grid lost band positions on most
    // turns. (Printed with --nocapture for the decision note.)
    println!("probed {probed}; old grid missed {old_xy_or_z_misses} positions on {old_turns_missing} of {TURNS} turns");
    assert!(old_turns_missing > TURNS / 2, "the old grid missed on {old_turns_missing} turns");
}

/// The enclosure is tight where it matters: a small turn AWAY from a
/// wall does not reach further toward it than the pose that started
/// the turn, by more than the stated sag.
#[test]
fn a_cell_reaches_no_further_than_its_end_columns_plus_the_sag() {
    let holder = (10.0, 10.0, 5.0);
    let tip = Point3::new(0.0, 0.0, 5.0);
    let mut cells = Vec::new();
    push_cells(&mut cells, &[(Point3::new(0.0, 0.0, 30.0), tip)], [0.0; 3], [0.0, -5.0, 0.0], holder, 3.0, 0.0, &[]);
    // Upright the rim stands at x = 10; leaning toward -x it retreats.
    let at = |x: f64| cells.iter().any(|c| c.bottom_over(Point2::new(x, 0.0), 16.0).is_some());
    assert!(at(10.0), "the upright rim is read");
    assert!(!at(10.0 + 2.0 * SAG), "{} cells reach past the rim plus the sag", cells.len());
}

/// The flutes' kerf excuses a column only while the tip stands at it:
/// a band trailing over the kerf the tip has LEFT is read.
#[test]
fn the_kerf_excuses_only_where_the_tip_stands() {
    let band = (5.0, 6.0, 10.0);
    let mut cells = Vec::new();
    let (a, b) = (Point3::new(0.0, 0.0, 0.0), Point3::new(40.0, 0.0, -8.0));
    push_cells(&mut cells, &[(a, b)], [0.0; 3], [0.0, 1.0, 0.0], band, 4.0, 0.0, &[]);
    let lowest = |q: Point2| cells.iter().filter_map(|c| c.bottom_over(q, 16.0)).fold(f64::INFINITY, f64::min);
    // On the tip's line the band covers q from 6 before to 6 after the
    // tip; the kerf excuses the middle 4. The lowest bottom is read
    // where the tip has run 4 past q (x 20 -> tip at 24, z -4.8).
    let h = lowest(Point2::new(20.0, 0.0));
    // The rim dips by R·sin θ (#1303): under 6·sin 1° at this turn.
    let dip = 6.0 * 1f64.to_radians().sin();
    assert!(h.is_finite() && h < 5.0 - 4.8 + 0.2 && h > 5.0 - dip - 5.3, "bottom {h}");
    // A plunge never leaves its kerf: nothing inside it is read.
    let mut still = Vec::new();
    push_cells(&mut still, &[(a, Point3::new(0.0, 0.0, -8.0))], [0.0; 3], [0.0, 1.0, 0.0], band, 4.0, 0.0, &[]);
    assert!(still.iter().all(|c| c.bottom_over(Point2::new(1.0, 0.0), 16.0).is_none()));
}

/// Nonsense in, no panic and no silent shrink: a dead band yields no
/// cell, and non-finite letters still yield cells (the widest sag).
#[test]
fn degenerate_bands_and_letters_do_not_panic() {
    let piece = [(Point3::new(0.0, 0.0, 0.0), Point3::new(1.0, 0.0, 0.0))];
    let mut cells = Vec::new();
    push_cells(&mut cells, &piece, [0.0; 3], [0.0, 10.0, 0.0], (5.0, 0.0, 10.0), 2.0, 0.0, &[]);
    push_cells(&mut cells, &piece, [0.0; 3], [0.0, 10.0, 0.0], (5.0, 4.0, -1.0), 2.0, 0.0, &[]);
    assert!(cells.is_empty());
    push_cells(&mut cells, &piece, [0.0; 3], [f64::NAN, 10.0, 0.0], (5.0, 4.0, 10.0), 2.0, 0.0, &[]);
    assert_eq!(cells.len(), MAX_CELLS);
    assert!(cells.iter().all(|c| c.bottom_over(Point2::new(0.5, 0.0), -1.0).is_some()));
    assert_eq!(cell_count(100.0, 0.0), 1);
    assert_eq!(cell_count(f64::INFINITY, 1.0), MAX_CELLS);
}

/// #1303 composed with #1313: a span whose letters lerp past a right
/// angle (A 100 -> 260 passes the tool straight down) reads #1303's
/// span floor at the sampled poses it ends; a turn cell holding such a
/// pose reads no higher. Here a column under the far rim of the A 100
/// column, reached by no more tilted pose, is convicted by the floor
/// alone - the cells' own bound stands above it - so the merged law
/// convicts what the sampled sweep did.
#[test]
fn a_cell_holding_a_sampled_pose_reads_its_span_floor() {
    let (from, to) = ([100.0, 0.0, 0.0], [260.0, 0.0, 0.0]);
    let band = (20.0, 10.0, 50.0);
    let floors = super::super::sweep::band_floors(from, to, band);
    let floor0 = floors[0].1;
    assert!(floor0.is_finite(), "past the cap the span floor reads: {floors:?}");
    let tip = Point3::new(0.0, 0.0, 0.0);
    let (mut with, mut without) = (Vec::new(), Vec::new());
    push_cells(&mut with, &[(tip, tip)], from, to, band, R, 0.0, &floors);
    push_cells(&mut without, &[(tip, tip)], from, to, band, R, 0.0, &[]);
    // The far rim of the A 100 column, a millimetre in.
    let (o, rad, _) = column(axis_at(from, to, 0.0), band.0, band.1, band.2);
    let l = o.0.hypot(o.1);
    assert!(l > 1.0, "the A 100 column leans off the tip: {o:?}");
    let q = Point2::new(o.0 / l * (l + rad - 1.0), o.1 / l * (l + rad - 1.0));
    let lowest = |cells: &[Cell]| cells.iter().filter_map(|c| c.bottom_over(q, R * R)).fold(f64::INFINITY, f64::min);
    let (lo_with, lo_without) = (lowest(&with), lowest(&without));
    assert!(lo_with <= floor0 + 1e-9, "the cell reads {lo_with}, the span floor {floor0}");
    assert!(lo_without > floor0 + 5.0, "the cells alone read {lo_without}: the floor must be what convicts");
    // A stock column standing between the two readings: convicted only
    // with the floor riding the cell.
    let zq = 0.5 * (floor0 + lo_without);
    assert!(lo_with < zq && lo_without >= zq);
}

/// Pins the `+ kappa` in the cell's rim dip (#1313 review). B 60 -> 120
/// with a wide, short band (radius 100, 0.5 long, at the tip): the cell
/// holding 90 degrees ends at 86.7 and 93.3, so both its end poses dip
/// the rim by 100·sin 86.7 = 99.83 while the pose between them lies flat
/// and dips the whole 100. The sag (reach · kappa, half a micrometre on
/// this reach) cannot cover that; only the rim's own `R · kappa` can.
/// Every cell's bottom must stand at or under the band's true bottom at
/// every pose it holds (a dense scan), and a stock post 1.25 µm under the
/// flat band's rim must be convicted. Without `+ kappa` the cells read
/// the post's column at -99.92, 0.08 mm clear of it.
#[test]
fn a_cell_straddling_a_flat_pose_reads_the_flat_rim() {
    let (from, to) = ([0.0, 60.0, 0.0], [0.0, 120.0, 0.0]);
    let band = (0.0, 100.0, 0.5);
    let tip = Point3::new(0.0, 0.0, 0.0);
    let mut cells = Vec::new();
    push_cells(&mut cells, &[(tip, tip)], from, to, band, R, 0.0, &[]);
    let n = cells.len();
    assert!(n > 2, "{n} cells");
    // A stock post over the tip whose top the flat band dips 1.25 µm into.
    let post = Point2::new(0.0, 0.0);
    let read = cells.iter().filter_map(|c| c.bottom_over(post, -1.0)).fold(f64::INFINITY, f64::min);
    assert!(read < -100.0, "the cells read the post's column bottom at {read}: the flat rim is missed");
    let mut true_lowest = f64::INFINITY;
    for (j, c) in cells.iter().enumerate() {
        // The band's true bottom over the tip at a dense scan of the
        // cell's own turn interval.
        let lowest = (0..=400)
            .map(|i| (j as f64 + i as f64 / 400.0) / n as f64)
            .map(|g| column(axis_at(from, to, g), band.0, band.1, band.2).2)
            .fold(f64::INFINITY, f64::min);
        assert!(c.z0 <= lowest + 1e-12, "cell {j}/{n} reads {} over a band reaching {lowest}", c.z0);
        true_lowest = true_lowest.min(lowest);
    }
    // The flat pose dips the whole radius, and the station a hair past
    // 90 degrees dips under the tip: the band reaches just under -100.
    assert!(true_lowest < -100.0 && true_lowest > -100.01, "{true_lowest}");
}
