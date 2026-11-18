//! Open-run stitching: the picked open runs of a selection, welded end to
//! end into the rings they spell (M1.5.1 issue C), the rim dedupe every
//! boundary passes, and the toolpath joint. The per-entity walk lives
//! here; the walk across model entities is `stitch_cross` (M13.11), the
//! run-out of an unclosed run to the stock is `run_out`.
//!
//! Provenance: our own design notes; the weld is the feature extractor's
//! own lattice. No vendor material consulted.

use super::*;

/// The 1e6 lattice `push_loop` keys rims on, in one place so the
/// stitcher's endpoint matches and the dedupe agree to the nanometre.
pub(super) fn qkey(p: &Point2) -> (i64, i64) {
    (
        (p.x * 1e6).round().clamp(i64::MIN as f64, i64::MAX as f64) as i64,
        (p.y * 1e6).round().clamp(i64::MIN as f64, i64::MAX as f64) as i64,
    )
}

/// One step of the [`qkey`] lattice: the weld tolerance every endpoint
/// match in this module uses, and the plane tolerance the cross-entity
/// law (`stitch_cross`) holds a composed ring to - one number, so "the
/// ends meet" and "the ring lies on one plane" are the same nanometre.
pub(super) const WELD_TOL: f64 = 1e-6;

/// A world point in the walk's frame, all three axes: [`plane_xy`]'s
/// door with the height kept (the cross-entity plane law reads it).
fn plane_local(w: Point3, frame: Option<&crate::cl::TiltedFrame>) -> Point3 {
    match frame {
        None => w,
        Some(f) => f.to_local(w),
    }
}

/// One distinct picked open run in the shape the stitcher works in:
/// world XY points with the entity's transform applied (where
/// `push_loop` expects every boundary to live), its quantized endpoints,
/// and every item index that picked it - the same chain twice is one
/// run picked twice.
pub(super) struct Run {
    pub(super) items: Vec<usize>,
    pub(super) entity: String,
    cid: u64,
    pub(super) pts: Vec<Point2>,
    pub(super) start: (i64, i64),
    pub(super) end: (i64, i64),
    /// The height of every point in the walk's frame. Only the
    /// cross-entity law reads it: one entity's ring is a ring however
    /// it stands, exactly as it always was.
    pub(super) zs: Vec<f64>,
}

/// Rings composed from picked runs: label, points, run count.
pub(super) type Rings = Vec<(String, Vec<Point2>, usize)>;

/// What the stitcher hands back: the rings, and for every item the rings
/// consumed, which ring (index into that vec) consumed it - the set form
/// answers "was this stitched?", the map form lets the UI's chips say
/// "→ ring k" from the very walk the region takes.
type Stitched = (Rings, HashMap<usize, usize>);

/// Every distinct picked open run, in the walk's frame.
pub(super) fn gather_runs(
    world: &World,
    setup: Option<&str>,
    items: &[GeoSel],
    frame: Option<&crate::cl::TiltedFrame>,
) -> Vec<Run> {
    let opframe = OpFrame::new(setup.map(str::to_owned), None);
    let mut runs: Vec<Run> = Vec::new();
    for (i, g) in items.iter().enumerate() {
        let FeatureSel::Chain(cid) = g.feature else {
            continue;
        };
        if let Some(r) = runs.iter_mut().find(|r| r.entity == g.entity && r.cid == cid) {
            r.items.push(i);
            continue;
        }
        let Some(entity) = model::find_by_name(world, &g.entity) else {
            continue;
        };
        let Some(features) = model::features_of(world, &entity) else {
            continue;
        };
        let Some(c) = crate::model::resolve_chain(&features, cid) else {
            continue;
        };
        if c.closed {
            continue;
        }
        let loc = opframe.pose_in_setup(world, &entity).unwrap_or_default();
        let local: Vec<Point3> = feature::chain_points(&features, &c)
            .iter()
            .map(|p| plane_local(transform_point(Point3::new(p[0], p[1], p[2]), loc.pos, loc.rot), frame))
            .collect();
        let pts: Vec<Point2> = local.iter().map(|l| Point2::new(l.x, l.y)).collect();
        let (Some(first), Some(last)) = (pts.first().copied(), pts.last().copied()) else {
            continue;
        };
        runs.push(Run {
            items: vec![i],
            entity: g.entity.clone(),
            cid,
            pts,
            start: qkey(&first),
            end: qkey(&last),
            zs: local.iter().map(|l| l.z).collect(),
        });
    }
    runs
}

/// Whether any pick of `r` already went into a ring.
pub(super) fn run_used(r: &Run, consumed: &HashMap<usize, usize>) -> bool {
    r.items.iter().any(|it| consumed.contains_key(it))
}

/// M1.5.1 issue C: tessellation - STEP adaptive chords especially - can
/// break a geometrically closed rim into several chains, each stopping
/// where a chord's turn exceeds the merge angle. The user picks them
/// all, and to them it is obviously one loop. So before routing, stitch
/// the picked open runs of each entity end to end and return every walk
/// that closes back onto its start as one ring, with its contributing
/// picks marked consumed so routing treats them as the closed chain
/// they describe.
///
/// Junctions match either endpoint of a candidate because a loop's runs
/// are stored with whatever direction their edge orientations allow -
/// across the rim of a split circle the two halves meet end-to-end on
/// one side and start-to-start on the other, so a flipped continuation
/// IS the ring, not a corruption of it. Committed rings are shoelace-
/// canonicalized to CCW so pick order can never flip the winding. A
/// doubled pick is deduped by chain id - the same segments twice close
/// no ring, they close a zero-area spike.
///
/// Deliberately conservative: only picked runs take part (nothing
/// unpicked is pulled in), a greedy fork that dead-ends commits nothing,
/// and runs that do not close route exactly as they always did. Runs
/// left over by this per-entity pass go to the cross-entity walk
/// ([`stitch_across`], M13.11) when the picks span more than one model -
/// the one place this walk refuses, by name. Other failed lookups are
/// not errors here; the routing loop owns those refusals and will name
/// them.
pub(super) fn stitch_open_runs(
    world: &World,
    setup: Option<&str>,
    items: &[GeoSel],
    frame: Option<&crate::cl::TiltedFrame>,
) -> Result<Stitched, GenError> {
    let runs = gather_runs(world, setup, items, frame);
    // Each committed ring carries the count of runs that spelled it, so
    // `selection_notes` can say what the stitching did.
    let mut rings: Rings = Vec::new();
    let mut consumed: HashMap<usize, usize> = HashMap::new();
    for s in 0..runs.len() {
        if run_used(&runs[s], &consumed) {
            continue;
        }
        let Some(seq) = walk_ring(&runs, s, &consumed, false)? else {
            continue;
        };
        let pts = compose_ring(&runs, &seq, rings.len(), &mut consumed);
        rings.push((format!("{STITCHED_RIM} '{}'", runs[s].entity), pts, seq.len()));
    }
    stitch_across(&runs, &mut rings, &mut consumed)?;
    Ok((rings, consumed))
}

/// One ring walk from run `s`: the `(run, reversed)` sequence that
/// closes back onto `s`'s start, or `None` when it dead-ends. `anchor`
/// is where the walk started, `far` the junction it extends from, and
/// `in_seq` is why a walk can never loop.
///
/// Per entity (`cross` false) the walk is the greedy first-match law it
/// always was, confined to `s`'s own entity. Across entities (`cross`
/// true, M13.11) every unconsumed run of every model is a candidate, and
/// a junction with more than one way on - or a closing point a third
/// run also leaves from - is the named ambiguity, never a guess: the
/// picks spell two rings, and a wrong one gouges.
pub(super) fn walk_ring(
    runs: &[Run],
    s: usize,
    consumed: &HashMap<usize, usize>,
    cross: bool,
) -> Result<Option<Vec<(usize, bool)>>, GenError> {
    let anchor = runs[s].start;
    let mut far = runs[s].end;
    let mut seq: Vec<(usize, bool)> = vec![(s, false)];
    let mut in_seq = vec![false; runs.len()];
    in_seq[s] = true;
    loop {
        let closes = far == anchor;
        if closes && !cross {
            return Ok(Some(seq));
        }
        let mut next: Option<(usize, bool)> = None;
        let mut at_junction: Vec<usize> = Vec::new();
        for (j, r) in runs.iter().enumerate() {
            if in_seq[j] || run_used(r, consumed) || (!cross && r.entity != runs[s].entity) {
                continue;
            }
            let hit = if r.start == far {
                Some((j, false))
            } else if r.end == far {
                Some((j, true))
            } else {
                None
            };
            let Some(h) = hit else { continue };
            if next.is_none() {
                next = Some(h);
            }
            at_junction.push(j);
            if !cross {
                break;
            }
        }
        if cross && (at_junction.len() > 1 || (closes && !at_junction.is_empty())) {
            let Some(&(last, _)) = seq.last() else {
                return Ok(None);
            };
            at_junction.push(last);
            if closes {
                at_junction.push(s);
            }
            return Err(fork_refusal(runs, &at_junction, far));
        }
        if closes {
            return Ok(Some(seq));
        }
        let Some((j, rev)) = next else {
            return Ok(None);
        };
        in_seq[j] = true;
        seq.push((j, rev));
        far = if rev { runs[j].start } else { runs[j].end };
    }
}

/// Concatenate a closed walk into one CCW ring, marking every pick of
/// its runs as consumed by ring `ring_ix`. Shared seam vertices arrive as
/// consecutive repeats and closing wraps - `push_loop` and `Loop::new`
/// drop exactly those, so the ring needs no special handling here.
pub(super) fn compose_ring(
    runs: &[Run],
    seq: &[(usize, bool)],
    ring_ix: usize,
    consumed: &mut HashMap<usize, usize>,
) -> Vec<Point2> {
    let mut pts: Vec<Point2> = Vec::new();
    for (j, rev) in seq {
        let Some(run) = runs.get(*j) else { continue };
        for &it in &run.items {
            consumed.insert(it, ring_ix);
        }
        if *rev {
            pts.extend(run.pts.iter().rev().copied());
        } else {
            pts.extend(run.pts.iter().copied());
        }
    }
    let mut twice = 0.0;
    let n = pts.len();
    for i in 0..n.saturating_sub(1) {
        let (a, b) = (pts[i], pts[i + 1]);
        twice += a.x * b.y - a.y * b.x;
    }
    if twice < 0.0 {
        pts.reverse();
    }
    pts
}

/// Accept one selected boundary: refuse what collapses to a line seen
/// from above, and count a rim already in the region once - two faces
/// meeting at an edge share it, and a coincident double loop would be a
/// zero-wide sliver.
pub(super) fn push_loop(
    label: &str,
    pts: Vec<Point2>,
    loops: &mut Vec<Loop>,
    rims: &mut Vec<Vec<(i64, i64)>>,
) -> Result<(), GenError> {
    let mut key: Vec<(i64, i64)> = pts.iter().map(qkey).collect();
    // Same drop Loop::new does - consecutive repeats and the closing
    // wrap - so the collapse check below sees the ring the loop would
    // actually be.
    key.dedup();
    if key.len() > 1 && key.first() == key.last() {
        key.pop();
    }
    if key.len() < 3 {
        // A vertical wall seen from above, or a chain that runs up one:
        // nothing encloses.
        return Err(line_from_above(label));
    }
    let canon = canon_ring(&key);
    if rims.contains(&canon) {
        return Ok(());
    }
    let loop_ = Loop::new(pts)?;
    if loop_.area().abs() < DEGENERATE_AREA {
        return Err(line_from_above(label));
    }
    rims.push(canon);
    loops.push(loop_);
    Ok(())
}

/// Rotate- and mirror-canonical form of a quantized ring: the
/// lexicographic minimum over its 2n walks. Compared exactly, never by
/// hash - a colliding dedupe key would silently drop a real boundary.
pub(super) fn canon_ring(ring: &[(i64, i64)]) -> Vec<(i64, i64)> {
    let n = ring.len();
    if n == 0 {
        return Vec::new();
    }
    // Booth's minimum-rotation scan is linear. The old walk allocated 2n
    // whole rings and compared them, quadratic work on every face pick and
    // every Geometry-panel redraw for a finely tessellated boundary.
    let forward = minimum_rotation(ring);
    let reversed: Vec<_> = ring.iter().rev().copied().collect();
    let backward = minimum_rotation(&reversed);
    let a: Vec<_> = (0..n).map(|k| ring[(forward + k) % n]).collect();
    let b: Vec<_> = (0..n).map(|k| reversed[(backward + k) % n]).collect();
    a.min(b)
}

fn minimum_rotation(ring: &[(i64, i64)]) -> usize {
    let n = ring.len();
    let (mut i, mut j, mut k) = (0, 1, 0);
    while i < n && j < n && k < n {
        match ring[(i + k) % n].cmp(&ring[(j + k) % n]) {
            std::cmp::Ordering::Equal => k += 1,
            std::cmp::Ordering::Greater => {
                i += k + 1;
                if i <= j {
                    i = j + 1;
                }
                k = 0;
            }
            std::cmp::Ordering::Less => {
                j += k + 1;
                if j <= i {
                    j = i + 1;
                }
                k = 0;
            }
        }
    }
    i.min(j) % n
}

/// Run one generated path after another. Both start and end on the
/// clearance plane, so the joint is a clearance rapid: append and re-anchor
/// the added path's opening `from` to the previous move's `to` - a stale
/// `(0, 0)` there would read as a teleport to the origin.
pub(super) fn stitch(mut a: Toolpath, mut b: Toolpath) -> Toolpath {
    if let (Some(to), Some(first)) = (a.moves.last().map(|m| m.to), b.moves.first_mut()) {
        first.from = to;
    }
    a.moves.append(&mut b.moves);
    a.cycles.append(&mut b.cycles);
    a
}

#[cfg(test)]
mod canonical_ring_tests {
    use super::canon_ring;

    fn reference(ring: &[(i64, i64)]) -> Vec<(i64, i64)> {
        let n = ring.len();
        if n == 0 {
            return Vec::new();
        }
        let mut best = None;
        for start in 0..n {
            for step in [1, n - 1] {
                let walk: Vec<_> = (0..n).map(|k| ring[(start + k * step) % n]).collect();
                if best.as_ref().is_none_or(|old| walk < *old) {
                    best = Some(walk);
                }
            }
        }
        best.unwrap_or_default()
    }

    #[test]
    fn linear_canonical_ring_matches_all_short_rotations_and_mirrors() {
        assert_eq!(canon_ring(&[]), Vec::new());
        for len in 1..=7u32 {
            for mut code in 0..3usize.pow(len) {
                let ring: Vec<_> = (0..len)
                    .map(|_| {
                        let digit = code % 3;
                        code /= 3;
                        (digit as i64, 0)
                    })
                    .collect();
                assert_eq!(canon_ring(&ring), reference(&ring), "{ring:?}");
            }
        }
    }
}
