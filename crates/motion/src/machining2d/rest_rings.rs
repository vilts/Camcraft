//! Rest-machining ring clipping and chaining (#1816): trims waterline rings
//! to the cutter reach of the rest material so cleared-floor inner legs and
//! hole laps are omitted while preserving interior-out ordering and
//! crossing-free chains.
//!
//! Provenance: geometric polyline clipping against the morphological rest zone
//! (O'Rourke, "Computational Geometry in C", ch. 7) and the ring-chaining laws
//! of `waterscan.rs`; clean-room, no vendor material consulted.

use crate::geo2d::{Loop, Point2, Region};

use super::face::GenError;
use super::rest2d::{REST_REACH, RestPlan};
use super::ring::{BOUNDARY_TOL, dist, on_boundary};
use super::slots::chord_clears;
use super::waterscan::chord_crosses_path;

/// Parameter slack along a clipped edge.
const EPS: f64 = 1e-9;

/// Minimum length (mm) for a surviving clipped open pass.
const MIN_PIECE_LEN: f64 = 1e-6;

struct Piece {
    k: u32,
    pts: Vec<Point2>,
    closed: bool,
}

impl Piece {
    fn start(&self) -> Point2 {
        self.pts[0]
    }

    fn end(&self) -> Point2 {
        if self.closed {
            self.pts[0]
        } else {
            self.pts.last().copied().unwrap_or(self.pts[0])
        }
    }
}

/// Plan concentric waterline passes over `plan.center`, clipped to the
/// cutter's reach around `plan.rest` (`r + REST_REACH * 0.5`), chained
/// interior-out within the clipped center area. Returns `(passes, cut_center)`.
pub(super) fn rest_passes(
    plan: &RestPlan,
    tool_diameter: f64,
    stepover: f64,
    climb: bool,
) -> Result<(Vec<Vec<Point2>>, Region), GenError> {
    let r = tool_diameter * 0.5;
    let raw_zone = plan.rest.offset(r + REST_REACH * 0.5)?;
    let cut_zone = if raw_zone.is_empty() { plan.center.clone() } else { raw_zone };
    let cut_center_raw = cut_zone.intersect(&plan.center)?;
    let cut_center = if cut_center_raw.is_empty() {
        plan.center.clone()
    } else {
        cut_center_raw
    };

    let Some((lo, hi)) = plan.center.bounds() else {
        return Err(GenError::Empty("empty pocket region".into()));
    };
    let cap = (dist(lo, hi) / stepover + 10.0).max(16.0) as u32;
    let mut pieces: Vec<Piece> = Vec::new();
    let mut converged = false;
    for k in 0..=cap {
        let ring = if k == 0 {
            plan.center.clone()
        } else {
            plan.center.offset(-f64::from(k) * stepover)?
        };
        if ring.is_empty() {
            converged = true;
            break;
        }
        let mut ls: Vec<&Loop> = ring.loops().iter().collect();
        ls.sort_by(|a, b| b.area().abs().total_cmp(&a.area().abs()));
        for l in ls {
            let pts: Vec<Point2> = if climb {
                l.points().to_vec()
            } else {
                l.points().iter().rev().copied().collect()
            };
            if pts.len() < 3 {
                continue;
            }
            for (clipped, closed) in clip_loop(&pts, &cut_zone) {
                pieces.push(Piece { k, pts: clipped, closed });
            }
        }
    }
    if !converged {
        return Err(GenError::Invalid(
            "waterline rings did not converge; check the stepover".into(),
        ));
    }
    if pieces.is_empty() {
        return Err(GenError::Empty("no reachable rest material".into()));
    }

    // Interior-out ordering, stable within each ring offset k.
    pieces.sort_by_key(|p| std::cmp::Reverse(p.k));
    let chains = chain_pieces(&pieces, &cut_center);
    let mut passes: Vec<Vec<Point2>> = Vec::with_capacity(chains.len());
    for chain in &chains {
        let mut cur: Vec<Point2> = Vec::new();
        for &idx in chain {
            for &pt in &pieces[idx].pts {
                if cur.last().is_none_or(|&prev| prev != pt) {
                    cur.push(pt);
                }
            }
            if pieces[idx].closed {
                let first = pieces[idx].pts[0];
                if cur.last().is_none_or(|&prev| prev != first) {
                    cur.push(first);
                }
            }
        }
        if cur.len() >= 2 {
            passes.push(cur);
        }
    }
    Ok((passes, cut_center))
}

/// Chain clipped pieces interior-out without leaving `cut_center` or crossing
/// any planned piece or earlier join chord.
fn chain_pieces(pieces: &[Piece], cut_center: &Region) -> Vec<Vec<usize>> {
    let mut used = vec![false; pieces.len()];
    let mut chains: Vec<Vec<usize>> = Vec::new();
    for i in 0..pieces.len() {
        if used[i] {
            continue;
        }
        used[i] = true;
        let mut chain = vec![i];
        while let Some(&last) = chain.last() {
            let prev = pieces[last].end();
            let last_k = pieces[last].k;
            let mine = chain_chords(pieces, &chain);
            let mut next = None;
            for j in 0..pieces.len() {
                if used[j] || pieces[j].k.abs_diff(last_k) > 1 {
                    continue;
                }
                let entry = pieces[j].start();
                if !chord_clears(cut_center, prev, entry)
                    || pieces
                        .iter()
                        .any(|p| chord_crosses_path(prev, entry, &p.pts, p.closed))
                    || mine
                        .iter()
                        .any(|&(a, b)| chord_crosses_path(prev, entry, &[a, b], false))
                {
                    continue;
                }
                next = Some(j);
                break;
            }
            let Some(j) = next else { break };
            used[j] = true;
            chain.push(j);
        }
        chains.push(chain);
    }
    loop {
        let mut victim = None;
        let mut chords: Vec<(usize, usize, Point2, Point2)> = Vec::new();
        for (ci, ch) in chains.iter().enumerate() {
            for (w, &(a, b)) in chain_chords(pieces, ch).iter().enumerate() {
                chords.push((ci, w, a, b));
            }
        }
        'sweep: for x in 0..chords.len() {
            for y in x + 1..chords.len() {
                if chords[x].0 == chords[y].0 {
                    continue;
                }
                if chord_crosses_path(
                    chords[x].2,
                    chords[x].3,
                    &[chords[y].2, chords[y].3],
                    false,
                ) {
                    victim = Some((chords[y].0, chords[y].1));
                    break 'sweep;
                }
            }
        }
        let Some((ci, w)) = victim else { break };
        let tail = chains[ci].split_off(w + 1);
        chains.push(tail);
    }
    chains
}

fn chain_chords(pieces: &[Piece], chain: &[usize]) -> Vec<(Point2, Point2)> {
    chain
        .windows(2)
        .map(|w| (pieces[w[0]].end(), pieces[w[1]].start()))
        .collect()
}

/// Clip a closed polyline `pts` against `zone`, returning `(points, closed)`
/// pieces in travel order.
fn clip_loop(pts: &[Point2], zone: &Region) -> Vec<(Vec<Point2>, bool)> {
    if zone.is_empty() || pts.len() < 3 {
        return Vec::new();
    }
    let n = pts.len();
    let mut runs: Vec<Vec<Point2>> = Vec::new();
    let mut cur: Vec<Point2> = Vec::new();
    let mut dropped_any = false;

    for i in 0..n {
        let a = pts[i];
        let b = pts[(i + 1) % n];
        let d = Point2::new(b.x - a.x, b.y - a.y);
        let cuts = edge_cuts(a, d, zone);
        for w in cuts.windows(2) {
            let p0 = point_on_edge(a, b, d, w[0]);
            let p1 = point_on_edge(a, b, d, w[1]);
            if dist(p0, p1) <= EPS {
                continue;
            }
            let mid = Point2::new((p0.x + p1.x) * 0.5, (p0.y + p1.y) * 0.5);
            if zone.contains(mid) || on_boundary(zone, mid) {
                if cur.last().is_some_and(|&prev| dist(prev, p0) <= EPS) {
                    if cur.last().is_some_and(|&prev| dist(prev, p1) > EPS) {
                        cur.push(p1);
                    }
                } else {
                    if cur.len() >= 2 {
                        runs.push(std::mem::take(&mut cur));
                    }
                    cur = vec![p0, p1];
                }
            } else {
                dropped_any = true;
                if cur.len() >= 2 {
                    runs.push(std::mem::take(&mut cur));
                } else {
                    cur.clear();
                }
            }
        }
    }
    if cur.len() >= 2 {
        runs.push(cur);
    }
    if !dropped_any && !runs.is_empty() {
        return vec![(pts.to_vec(), true)];
    }
    if runs.len() >= 2
        && let Some(last_end) = runs.last().and_then(|r| r.last().copied())
        && dist(runs[0][0], last_end) <= EPS
        && let Some(mut tail) = runs.pop()
    {
        tail.pop();
        tail.extend_from_slice(&runs[0]);
        runs[0] = tail;
    }
    runs.into_iter()
        .filter_map(|mut r| {
            if r.len() >= 4
                && let Some(&end) = r.last()
                && dist(r[0], end) <= EPS
            {
                r.pop();
                return (polyline_len(&r) > MIN_PIECE_LEN).then_some((r, true));
            }
            (r.len() >= 2 && polyline_len(&r) > MIN_PIECE_LEN).then_some((r, false))
        })
        .collect()
}

fn point_on_edge(a: Point2, b: Point2, d: Point2, t: f64) -> Point2 {
    if t == 0.0 {
        a
    } else if t == 1.0 {
        b
    } else {
        Point2::new(a.x + d.x * t, a.y + d.y * t)
    }
}

fn edge_cuts(a: Point2, d: Point2, zone: &Region) -> Vec<f64> {
    let mut cuts = vec![0.0, 1.0];
    let dd = d.x * d.x + d.y * d.y;
    if !dd.is_finite() || dd <= 0.0 {
        return cuts;
    }
    for l in zone.loops() {
        let zpts = l.points();
        let zn = zpts.len();
        for j in 0..zn {
            let c = zpts[j];
            let (vx, vy) = (c.x - a.x, c.y - a.y);
            let h = vx * d.y - vy * d.x;
            let tv = (vx * d.x + vy * d.y) / dd;
            if h * h <= BOUNDARY_TOL * BOUNDARY_TOL * dd && tv > EPS && tv < 1.0 - EPS {
                cuts.push(tv);
            }
            let e = zpts[(j + 1) % zn];
            let g = Point2::new(e.x - c.x, e.y - c.y);
            let den = d.x * g.y - d.y * g.x;
            if den == 0.0 {
                continue;
            }
            let ca = Point2::new(c.x - a.x, c.y - a.y);
            let t = (ca.x * g.y - ca.y * g.x) / den;
            let u = (ca.x * d.y - ca.y * d.x) / den;
            if t > EPS && t < 1.0 - EPS && (-EPS..=1.0 + EPS).contains(&u) {
                cuts.push(t);
            }
        }
    }
    cuts.sort_by(f64::total_cmp);
    cuts.dedup_by(|x, y| (*x - *y).abs() <= EPS);
    cuts
}

fn polyline_len(pts: &[Point2]) -> f64 {
    pts.windows(2).map(|w| dist(w[0], w[1])).sum()
}
