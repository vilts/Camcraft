//! Two or more separate open chains as one open boundary (#1727): the
//! picked runs grouped into chains joined end to end, chains that touch
//! or cross refused by name, and the area kept between them - each chain
//! splits the stock as the single open chain does (`sides_of_chain`),
//! each keeps the side holding the operation's own area, and the kept
//! area is where every chain's side agrees.
//!
//! Provenance: our own design notes; the side construction is
//! `split`'s, the rest is region intersection. No vendor
//! material consulted.

use crate::geo2d::{Point2, Region};

pub mod open_slot;
mod split;
pub use split::{BoundaryError, sides_of_chain, snap_turn};

fn qkey(p: &Point2) -> (i64, i64) {
    (
        (p.x * 1e6).round().clamp(i64::MIN as f64, i64::MAX as f64) as i64,
        (p.y * 1e6).round().clamp(i64::MIN as f64, i64::MAX as f64) as i64,
    )
}

fn dist(a: Point2, b: Point2) -> f64 {
    (a.x - b.x).hypot(a.y - b.y)
}

fn cross(o: Point2, a: Point2, b: Point2) -> f64 {
    (a.x - o.x) * (b.y - o.y) - (a.y - o.y) * (b.x - o.x)
}

/// Closest distance between segments ab and cd (zero when they cross).
fn seg_seg(a: Point2, b: Point2, c: Point2, d: Point2) -> f64 {
    let (d1, d2) = (cross(a, b, c), cross(a, b, d));
    let (d3, d4) = (cross(c, d, a), cross(c, d, b));
    if d1 * d2 < 0.0 && d3 * d4 < 0.0 {
        return 0.0;
    }
    let pd = |p: Point2, a: Point2, b: Point2| foot(p, a, b).1;
    pd(a, c, d).min(pd(b, c, d)).min(pd(c, a, b)).min(pd(d, a, b))
}

/// The unclamped parameter of `p`'s foot on ab and the clamped distance.
fn foot(p: Point2, a: Point2, b: Point2) -> (f64, f64) {
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    let l2 = dx * dx + dy * dy;
    if l2 <= 0.0 {
        return (0.0, dist(p, a));
    }
    let t = ((p.x - a.x) * dx + (p.y - a.y) * dy) / l2;
    let tc = t.clamp(0.0, 1.0);
    (t, dist(p, Point2::new(a.x + dx * tc, a.y + dy * tc)))
}

/// Closer than this, two chains touch, mm.
const TOUCH: f64 = 1e-3;

/// Group open runs into chains joined end to end (either direction), in
/// pick order: a run that meets neither end of the chain being grown
/// starts the next one.
pub fn group_runs(mut runs: Vec<Vec<Point2>>) -> Vec<Vec<Point2>> {
    let mut out: Vec<Vec<Point2>> = Vec::new();
    while !runs.is_empty() {
        let mut chain = runs.remove(0);
        while let (Some(h), Some(t)) = (chain.first().map(qkey), chain.last().map(qkey)) {
            let Some(j) = runs.iter().position(|r| {
                let (a, b) = (r.first().map(qkey), r.last().map(qkey));
                a == Some(t) || b == Some(t) || a == Some(h) || b == Some(h)
            }) else {
                break;
            };
            let mut r = runs.remove(j);
            let (a, b) = (r.first().map(qkey), r.last().map(qkey));
            if a == Some(t) {
                chain.extend(r.into_iter().skip(1));
            } else if b == Some(t) {
                r.reverse();
                chain.extend(r.into_iter().skip(1));
            } else {
                if a == Some(h) {
                    r.reverse();
                }
                r.pop();
                r.extend(chain);
                chain = r;
            }
        }
        out.push(chain);
    }
    out
}

/// Deduplicate XY-coincident runs (direction-free) and weld touching
/// open runs end to end into continuous open chains. Single-point or
/// non-finite runs stay apart so generation's own refusal names them.
pub fn join_open_runs(runs: Vec<Vec<Point2>>) -> Vec<Vec<Point2>> {
    if runs.len() < 2 {
        return runs;
    }
    let mut seen: Vec<Vec<(i64, i64)>> = Vec::with_capacity(runs.len());
    let mut valid: Vec<Vec<Point2>> = Vec::with_capacity(runs.len());
    let mut passthrough: Vec<Vec<Point2>> = Vec::new();
    for pts in runs {
        if !pts.iter().all(|p| p.x.is_finite() && p.y.is_finite()) {
            passthrough.push(pts);
            continue;
        }
        let mut keys: Vec<(i64, i64)> = pts.iter().map(qkey).collect();
        keys.dedup();
        if keys.len() < 2 {
            passthrough.push(pts);
            continue;
        }
        let rev: Vec<(i64, i64)> = keys.iter().rev().copied().collect();
        let canon = keys.min(rev);
        if !seen.contains(&canon) {
            seen.push(canon);
            valid.push(pts);
        }
    }
    let mut out = if valid.len() > 1 { group_runs(valid) } else { valid };
    out.extend(passthrough);
    out
}

/// Whether two polylines touch or cross anywhere.
fn chains_touch(a: &[Point2], b: &[Point2]) -> bool {
    a.windows(2).any(|s| b.windows(2).any(|q| seg_seg(s[0], s[1], q[0], q[1]) <= TOUCH))
}

/// The refusal for chains that touch or cross: which side of either lies
/// between them is a guess, so it is never taken.
pub fn touching_refusal(chains: &[Vec<Point2>]) -> Option<BoundaryError> {
    for i in 0..chains.len() {
        for j in (i + 1)..chains.len() {
            if chains_touch(&chains[i], &chains[j]) {
                return Some(BoundaryError::Invalid(format!(
                    "the boundary picks spell {} open chains and chains {} and {} touch or cross - separate chains bound the area between them only when they stay apart; pick the edges between them to make one chain, or remove one",
                    chains.len(),
                    i + 1,
                    j + 1
                )));
            }
        }
    }
    None
}

/// The area between `chains` inside `reach`, what lies beyond them, and
/// the turn that lays Face's rows along the first chain with the kept
/// side below it. Each chain keeps the side that holds more of `base`;
/// a chain with `base` evenly on both sides is refused by name.
pub fn sides_of_chains(
    chains: &[Vec<Point2>],
    reach: &Region,
    base: &Region,
) -> Result<(Region, Region, f64), BoundaryError> {
    let mut between = reach.clone();
    let mut turn = 0.0;
    for (i, c) in chains.iter().enumerate() {
        let (left, right, theta) = sides_of_chain(c, reach)?;
        let in_l = left.intersect(base)?.area().abs();
        let in_r = right.intersect(base)?.area().abs();
        if (in_l - in_r).abs() <= 0.01 * in_l.max(in_r) {
            return Err(BoundaryError::Invalid(format!(
                "open chain {} of {} has the operation's area evenly on both sides - which side lies between the chains is a guess; pick edges that bound the area from one side",
                i + 1,
                chains.len()
            )));
        }
        let keep_left = in_l > in_r;
        if i == 0 {
            turn = snap_turn(if keep_left { std::f64::consts::PI - theta } else { -theta });
        }
        between = between.intersect(if keep_left { &left } else { &right })?;
    }
    let beyond = reach.difference(&between)?;
    Ok((between, beyond, turn))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geo2d::Loop;

    fn p(x: f64, y: f64) -> Point2 {
        Point2::new(x, y)
    }

    fn square(h: f64) -> Region {
        let ring = vec![p(-h, -h), p(h, -h), p(h, h), p(-h, h)];
        Loop::new(ring).and_then(|l| Region::new(vec![l])).unwrap_or_else(|_| Region::empty())
    }

    #[test]
    fn runs_group_into_chains_and_touching_chains_are_refused() {
        // Two runs meeting end to end are one chain; a third apart is a second.
        let runs = vec![
            vec![p(0.0, 0.0), p(5.0, 0.0)],
            vec![p(0.0, 9.0), p(5.0, 9.0)],
            vec![p(5.0, 0.0), p(9.0, 0.0)],
        ];
        let chains = group_runs(runs);
        assert_eq!(chains.len(), 2, "{chains:?}");
        assert!(touching_refusal(&chains).is_none(), "parallel chains stay apart");
        // A run that meets a chain mid-way (a T) is a second chain that touches it.
        let t = group_runs(vec![vec![p(0.0, 0.0), p(10.0, 0.0)], vec![p(5.0, 0.0), p(5.0, 8.0)]]);
        assert_eq!(t.len(), 2);
        let why = touching_refusal(&t).map(|e| e.to_string()).unwrap_or_default();
        assert!(why.contains("chains 1 and 2 touch or cross"), "{why}");
        // Two chains that cross.
        let x = vec![vec![p(-5.0, 0.0), p(5.0, 0.0)], vec![p(0.0, -5.0), p(0.0, 5.0)]];
        assert!(touching_refusal(&x).is_some(), "crossing chains are refused");
    }

    #[test]
    fn the_area_lies_between_the_chains_and_a_tie_is_refused() {
        let (reach, base) = (square(30.0), square(10.0));
        let chains = vec![vec![p(-10.0, 10.0), p(10.0, 10.0)], vec![p(-10.0, -10.0), p(10.0, -10.0)]];
        let (between, beyond, _) = sides_of_chains(&chains, &reach, &base).unwrap_or_else(|e| panic!("{e}"));
        assert!(
            (between.area().abs() - 60.0 * 20.0).abs() < 1.0,
            "the strip across the reach: {}",
            between.area()
        );
        assert!((beyond.area().abs() - 60.0 * 40.0).abs() < 1.0, "{}", beyond.area());
        // A chain through the middle of the area: which side is between is a guess.
        let mid = vec![vec![p(-10.0, 10.0), p(10.0, 10.0)], vec![p(-10.0, 0.0), p(10.0, 0.0)]];
        let why = sides_of_chains(&mid, &reach, &base).err().map(|e| e.to_string()).unwrap_or_default();
        assert!(why.contains("open chain 2 of 2 has the operation's area evenly on both sides"), "{why}");
    }

    #[test]
    fn join_open_runs_welds_out_of_order_and_deduplicates_coincident_runs() {
        let edges = vec![
            vec![p(42.0, 10.0), p(42.0, 0.0)],
            vec![p(22.0, 10.0), p(42.0, 10.0)],
            vec![p(22.0, 0.0), p(22.0, 10.0)],
            vec![p(0.0, 0.0), p(22.0, 0.0)],
            vec![p(42.0, 0.0), p(64.0, 0.0)],
        ];
        let joined = join_open_runs(edges);
        assert_eq!(joined.len(), 1, "{joined:?}");
        assert_eq!(joined[0].len(), 6, "{joined:?}");

        let mixed = vec![
            vec![p(0.0, 0.0), p(10.0, 0.0)],
            vec![p(10.0, 0.0), p(0.0, 0.0)],
            vec![p(10.0, 0.0), p(10.0, 5.0)],
            vec![p(30.0, 0.0), p(40.0, 0.0)],
            vec![p(5.0, 5.0), p(5.0, 5.0)],
        ];
        let out = join_open_runs(mixed);
        assert_eq!(out.len(), 3, "{out:?}");
        assert_eq!(out[0], vec![p(0.0, 0.0), p(10.0, 0.0), p(10.0, 5.0)]);
        assert_eq!(out[1], vec![p(30.0, 0.0), p(40.0, 0.0)]);
        assert_eq!(out[2], vec![p(5.0, 5.0), p(5.0, 5.0)]);
    }
}
