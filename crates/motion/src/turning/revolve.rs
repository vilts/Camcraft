//! The revolved-blade verdict (M5.2 C): what a full-width blade
//! actually leaves on a spinning part, measured in the meridian.
//!
//! No swept sphere can judge a grooving path. A grooved part is
//! undercut BY DESIGN - material stands beside the groove, above the
//! floor, out of every ball's reach along Z - and the height-field
//! law the envelope scan runs would report the groove's own air as a
//! phantom trench (a ball riding the floor has the bar's TOP above
//! it in its column, whatever the cut did there). The blade cuts a
//! different shape: every radius OUTSIDE its tip, across the band
//! its width owns along the axis. Revolved, that footprint is
//! exactly the cut volume, and in the meridian the question is one
//! dimensional:
//!
//! - the material E(s) is the silhouette's own chord polyline (the
//!   [`super::profile::silhouette`] samples at the caller's quarter budget,
//!   interpolated on each interval);
//! - the cut C(s) is the MINIMUM tip over the moves whose open band
//!   `(start - w/2, end + w/2)` contains s - widened along an axial
//!   move's own station span; rapids cut nothing. The cuts union:
//!   everything above the deepest floor that stood here was removed
//!   by that one pass, and a shallower blade covering the same
//!   station added nothing to the surface left;
//! - where `C < E` the blade GOUCHED by `E - C`; where `C > E` the
//!   passes left a floor standing - REST by `C - E`, tolerance and
//!   the stock-to-leave the kernel was told to keep both subtracted
//!   at the gate; and a floor no band reached rests by the whole
//!   window shell - the over-report law: silence is what a missed
//!   pass must never buy.
//!
//! Strict containment is the tangency promise: a blade corner
//! standing exactly on a wall station grazes it and cuts nothing
//! there, which is how a full-width groove sits flush to its walls.
//! The promise is kept on the lattice's INTERVALS, not its points:
//! a band (or a chord) counts where it overlaps an interval FOR
//! REAL, and real is [`HAIR`]-wide - projection noise lands at
//! 1e-13 of a millimetre and no steel is that thin, so a parameter
//! edge grazing a model lip reads as the coincidence it is, at a
//! lattice station or at a band's clipped end. Between two adjacent
//! lattice events E is linear and the covering set constant, so
//! every clause takes its extremes at the interval's two ends: the
//! scan is exact, with no chord budget beyond the silhouette's.
//! A through-axis plunge (`tip <= 0`) is SEPARATION: what it removes
//! inside its kerf is what parting means to remove, and stands
//! judged silent there - and the bar's own ends need no clamp
//! beyond the lattice, which never lives outside the material.
//!
//! Provenance: grooving and cutoff as a band cut on a revolved part -
//! machine-tool handbook practice, judged the way our own
//! distance-to-part law reads a lathe; the boxcar-min-of-cuts
//! construction is our own, on the same silhouette the kernels ride.
//! No vendor strategy consulted.

use crate::verify::Warning;

type SR = (f64, f64);

/// A hair of a millimetre: the gap that decides whether an overlap
/// counts. Float operations on mm-scale values land their noise
/// below 1e-13; the thinnest real wall is far above 1e-9 - so a
/// graze (a band edge meeting a model lip to within the noise) is
/// the coincidence it looks like, and never becomes a cut, a floor
/// or a rung. The scan, the boxcar and the kernels' ladders all
/// judge by this one number, spelled once.
pub(crate) const HAIR: f64 = 1e-9;

/// The shape of a move's floor between its band's edges - the
/// footprint the steel leaves in the meridian.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Floor {
    /// The blade's boxcar: every radius outside the tip, flat.
    Flat,
    /// The threading V (M5.2 D): the flank's climb, `cot(A/2)` per
    /// mm of station from the center - a linear floor.
    Wave(f64),
    /// The form gauge's crown (M5.2 E2): the meridian arc's sagitta,
    /// `R - sqrt(R^2 - d^2)` from the center - the wave clause's
    /// curved sibling, and its own limit as `R` grows.
    Crown(f64),
}

/// The blade moves the verdict reads: a station span (`a` and `b` -
/// the same point for a straight plunge) and the tip radius the move
/// reaches. Feed moves only: a rapids' band moves air, not edges.
///
/// A threading insert is a blade cut to a V, and a form gauge is one
/// cut to an arc, and both say so in `floor`: the floor a band holds
/// is not its tip alone but the shape the steel sweeps around its
/// center (M5.2 D, E2). The scan's laws do not change - the boxcar
/// still unions, the interval still judges at its two ends - only
/// the floor now BENDS between the centers the lattice already
/// holds, linearly or along the arc.
pub struct BandMove {
    pub a: f64,
    pub b: f64,
    pub tip: f64,
    /// Index into the judged path, for the warning's attribution. A
    /// composed program scan (`blade_program_warnings`) keeps the
    /// index in the PROGRAM's move space - the caller's law for
    /// naming which leg's move stands deepest here.
    pub at: usize,
    /// The floor's shape around the center `(a + b) * 0.5`;
    /// [`Floor::Flat`] for the plain-tipped blade.
    pub floor: Floor,
    /// The half-width of the blade making this move. A single-tool
    /// scan stores the same number on every move; a program scan
    /// judges passes of different blades together (M5.3 D), and the
    /// tangency promise reads per move exactly as it did per call.
    pub half: f64,
}

/// The floor a move holds at a station: its tip, plus the steel's
/// climb where the move carries a form. A crown reads only inside
/// its own reach - past `R` from the center the arc has no steel,
/// and the door's band law says no scan asks; the clamp is the
/// rounding's, not a license for an over-wide band.
fn floor_at(m: &BandMove, s: f64) -> f64 {
    let d = (s - (m.a + m.b) * 0.5).abs();
    m.tip + match m.floor {
        Floor::Flat => 0.0,
        Floor::Wave(cot) => d * cot,
        Floor::Crown(r) => r - (r * r - (d.min(r) * d.min(r))).max(0.0).sqrt(),
    }
}

/// The silhouette read as chord windows: (s0, s1, r0, slope) per
/// interval of ascending stations - the pieces of E, the same
/// partition the offset and the kernels ride.
pub(crate) type Segs = Vec<(f64, f64, f64, f64)>;

/// Window-partition the silhouette samples; verticals (equal
/// stations) drop out exactly as the offset's windows do.
pub(crate) fn segs(pts: &[SR]) -> Segs {
    pts.windows(2)
        .filter(|w| w[0].0 < w[1].0)
        .map(|w| {
            (
                w[0].0,
                w[1].0,
                w[0].1,
                (w[1].1 - w[0].1) / (w[1].0 - w[0].0),
            )
        })
        .collect()
}

/// The material at a station: every interval's chord VALUE at it,
/// as a maximum - so a step's shoulder stands AT its station where
/// the model says material does, and reads as air beyond the
/// domain. The kernels' column reads use it (a parting plane on a
/// lip: the taller claim, more rungs, never a gouge); the scan
/// measures its intervals with the chord-overlap law instead,
/// where a point's both-sided vote would judge a graze a cut.
pub(crate) fn material(sg: &Segs, s: f64) -> f64 {
    let mut r: f64 = 0.0;
    let i = sg.partition_point(|&(s0, ..)| s0 <= s);
    for &(s0, s1, a, b) in &sg[i.saturating_sub(1)..(i + 2).min(sg.len())] {
        if s0 <= s && s <= s1 {
            r = r.max(a + b * (s - s0));
        }
    }
    r
}

/// The highest point of the chords over the interval (lo, hi):
/// each window's share of a chord is linear, so its maximum is at
/// that share's ends - but a chord counts only where it overlaps
/// for real, by more than a [`HAIR`]. A wall the window merely
/// grazes at its edge stands AT the edge, not inside the band:
/// read the graze as a claim and a projection hair would raise a
/// groove's floor to the shell, plunging the blade in air. This is
/// the floor a blade whose band is (lo, hi) can descend to and
/// touch nothing the model keeps - the boxcar maximum, the ball
/// offset's no-gouge law read radially.
pub fn span_max(sg: &Segs, lo: f64, hi: f64) -> f64 {
    if hi - lo <= HAIR {
        return 0.0;
    }
    let mut r: f64 = 0.0;
    let i = sg.partition_point(|&(s0, ..)| s0 < hi);
    for &(s0, s1, a, b) in &sg[..i] {
        if s1 <= lo {
            continue;
        }
        let x0 = s0.max(lo);
        let x1 = s1.min(hi);
        if x1 - x0 > HAIR {
            r = r.max(a + b * (x0 - s0)).max(a + b * (x1 - s0));
        }
    }
    r
}

/// The band a move owns: (start - w/2, end + w/2) - the tangency
/// promise spelled once. The bar's ends are the lattice's domain
/// law's business, not the band's: a band may stand in end air and
/// simply own no interval there.
fn raw_band(m: &BandMove) -> (f64, f64) {
    (m.a.min(m.b) - m.half, m.a.max(m.b) + m.half)
}

/// The window the passes can reach (their bands' union, kept where
/// the tip bites) and the lattice to judge: E's breakpoints and the
/// bands' edges - kerfs' too - retained inside the bar's own
/// stations, which is the whole of the domain law.
pub fn lattice(pts: &[SR], moves: &[BandMove]) -> (Option<(f64, f64)>, Vec<f64>) {
    let (d_lo, d_hi) = (pts[0].0, pts[pts.len() - 1].0);
    let mut lat: Vec<f64> = Vec::with_capacity(pts.len() + 4 * moves.len() + 2);
    let mut window = None;
    for m in moves {
        let (lo, hi) = raw_band(m);
        lat.push(lo);
        lat.push(hi);
        lat.push(m.a.min(m.b));
        lat.push(m.a.max(m.b));
        if m.tip > 0.0 && hi - lo > HAIR {
            window = match window {
                None => Some((lo, hi)),
                Some((w0, w1)) => Some((w0.min(lo), w1.max(hi))),
            };
        }
    }
    for &(sv, _) in pts {
        lat.push(sv);
    }
    lat.retain(|s| (d_lo..=d_hi).contains(s));
    if let Some((w0, w1)) = window {
        lat.push(w0);
        lat.push(w1);
    }
    lat.sort_unstable_by(f64::total_cmp);
    lat.dedup();
    (window, lat)
}

/// What the scan reports: the worst gouge and worst rest found,
/// each with its station, radius and blamed move.
pub struct Alert {
    pub mm: f64,
    pub at: SR,
    pub move_i: usize,
}

/// The scan: walk the lattice's INTERVALS, keep the covers whose
/// bands overlap each interval for real (a move appears at its
/// band's left edge and stands to its right - the tangency promise
/// read on the interval, not the point), and judge every clause at
/// the interval's two ends off the DEEPEST standing FLOOR (a form
/// move's floor read AT that end - [`floor_at`]). E is linear and
/// the cover set constant between adjacent events, and so is every
/// floor between a center and the next, which is why the lattice
/// holds the centers: the ends are still the interval's extremes.
/// Intervals thinner than a [`HAIR`] own no surface and are skipped.
/// `clip_shell` is the crest law: a form floor standing above the
/// blank's own surface measures nothing - the crest is where the
/// passes never reach, and there the shell, not the floor, is the
/// part. The blank level is [`span_max`] over the passes' window -
/// the whole thread, where crest and window are the same steel -
/// unless `crest` says better: a lone crown's band sits INSIDE the
/// blank it cannot reach past, and clipping that floor to its own
/// window would hide the rest it stands off the modeled surface.
/// The gouge clause answers to `gouge_tol`, the rest clause to `tol`:
/// one number on a model that met its import promise, the smaller
/// [`gouge_tol_on`] on a coarser one (#904).
#[allow(clippy::too_many_arguments)] // the scan's whole meridian frame, threaded once
pub fn scan(
    sg: &Segs,
    moves: &[BandMove],
    window: Option<(f64, f64)>,
    lat: &[f64],
    (tol, gouge_tol): (f64, f64),
    leave: f64,
    clip_shell: bool,
    crest: Option<f64>,
) -> (Option<Alert>, Option<Alert>) {
    let mut order: Vec<(f64, usize)> = Vec::new();
    let mut kerfs: Vec<(f64, f64)> = Vec::new();
    for (i, m) in moves.iter().enumerate() {
        let (lo, hi) = raw_band(m);
        if m.tip <= 0.0 {
            if hi > lo {
                kerfs.push((lo, hi));
            }
        } else if hi > lo {
            order.push((lo, i));
        }
    }
    order.sort_unstable_by(|x, y| x.0.total_cmp(&y.0));
    kerfs.sort_unstable_by(|x, y| x.0.total_cmp(&y.0));
    let ref_top = window.map(|(w0, w1)| span_max(sg, w0, w1)).unwrap_or(0.0);

    // Standing covers as (right edge, move index), sorted by right
    // edge descending, so expiration pops from the back; the floor
    // each one holds is read off the move itself at each end.
    let mut heap: Vec<(f64, usize)> = Vec::new();
    let mut next = 0usize;
    let mut ki = 0usize;
    let mut gouge: Option<Alert> = None;
    let mut rest: Option<Alert> = None;
    let gouge_gate = gouge_tol + 1e-9;
    let rest_gate = tol + leave + 1e-9;
    for w in lat.windows(2) {
        let (u, v) = (w[0], w[1]);
        if v - u <= HAIR {
            continue; // no surface lives in a hair of lattice
        }
        while next < order.len() && order[next].0 < v {
            let mi = order[next].1;
            let hi = raw_band(&moves[mi]).1;
            let k = heap.partition_point(|e| e.0.total_cmp(&hi) == std::cmp::Ordering::Greater);
            heap.insert(k, (hi, mi));
            next += 1;
        }
        while heap.last().is_some_and(|e| e.0 <= u) {
            heap.pop();
        }
        while ki < kerfs.len() && kerfs[ki].0 < v {
            ki += 1;
        }
        if kerfs[..ki].iter().any(|&(_, b)| b > u) {
            continue; // inside a cutoff: what stands here was meant to go
        }
        // The interval's own shell: the chords overlapping it for
        // real, read at both ends. A chord endpoint strictly inside
        // a long interval would be a lattice point - adjacency
        // makes every overlap here either real or the interval a
        // hair, already skipped.
        let j = sg.partition_point(|&(s0, ..)| s0 < v);
        let k = sg.partition_point(|&(_, s1, _, _)| s1 <= u);
        let (mut e_u, mut e_v) = (0.0f64, 0.0f64);
        for &(s0, s1, a, b) in &sg[k.min(j)..j] {
            e_u = e_u.max(a + b * (u.max(s0) - s0));
            e_v = e_v.max(a + b * (v.min(s1) - s0));
        }
        if !heap.is_empty() {
            let (mut floor_u, mut at_u) = (f64::INFINITY, usize::MAX);
            let (mut floor_v, mut at_v) = (f64::INFINITY, usize::MAX);
            for &(_, mi) in &heap {
                let m = &moves[mi];
                let cu = floor_at(m, u);
                if cu < floor_u || (cu == floor_u && m.at < at_u) {
                    floor_u = cu;
                    at_u = m.at;
                }
                let cv = floor_at(m, v);
                if cv < floor_v || (cv == floor_v && m.at < at_v) {
                    floor_v = cv;
                    at_v = m.at;
                }
            }
            if clip_shell {
                let cap = crest.unwrap_or(ref_top);
                floor_u = floor_u.min(cap);
                floor_v = floor_v.min(cap);
            }
            let g = (e_u - floor_u).max(e_v - floor_v);
            let r = (floor_u - e_u).max(floor_v - e_v);
            if g > gouge_gate {
                let (x, e, move_i) =
                    if e_u - floor_u >= e_v - floor_v { (u, e_u, at_u) } else { (v, e_v, at_v) };
                keep(&mut gouge, Alert { mm: g, at: (x, e), move_i });
            } else if r > rest_gate {
                let (x, e, move_i) =
                    if floor_u - e_u >= floor_v - e_v { (u, e_u, at_u) } else { (v, e_v, at_v) };
                keep(&mut rest, Alert { mm: r, at: (x, e), move_i });
            }
        } else if window.is_some_and(|(w0, w1)| w0 < v && u < w1) {
            // A floor no band reached rests by the window shell -
            // the over-report law: a pass missing entirely must not
            // buy silence.
            let e = e_u.min(e_v);
            if ref_top - e > rest_gate {
                keep(&mut rest, Alert {
                    mm: ref_top - e,
                    at: (if e_u <= e_v { u } else { v }, e),
                    move_i: nearest(moves, (u + v) * 0.5),
                });
            }
        }
    }
    (gouge, rest)
}

/// Keep the worst alert of a clause.
fn keep(slot: &mut Option<Alert>, a: Alert) {
    if slot.as_ref().is_none_or(|w| a.mm > w.mm) {
        *slot = Some(a);
    }
}

/// The move whose band reaches nearest a station - the attribution
/// for a floor no band covered at all.
fn nearest(moves: &[BandMove], s: f64) -> usize {
    let mut best = (f64::INFINITY, 0usize);
    for m in moves {
        let d = ((m.a + m.b) * 0.5 - s).abs();
        if d < best.0 {
            best = (d, m.at);
        }
    }
    best.1
}

/// The gouge clause's tolerance on a model imported coarser than
/// promised (#864, #904): `tol` less the model's `excess` chord error,
/// while that leaves a positive tolerance. The silhouette E(s) is read
/// off the facets, and a facet of a convex meridian stands inside the
/// true surface by up to its chord error - so a floor that far below
/// the real skin reads as flush. Past the bound the clause reads `tol`
/// untouched and the program gate refuses the operation by name
/// ([`crate::verify::excess_floor`] is the one test both sides read).
/// The rest clause and the silhouette's own chord budget keep `tol`.
#[must_use]
pub fn gouge_tol_on(tol: f64, excess: f64) -> f64 {
    crate::verify::excess_floor(tol, excess).unwrap_or(tol)
}

/// The shared close: scan what the moves leave and speak it in the
/// vocabulary the verifier already owns. `clip_shell` belongs to the
/// form tools (the thread's crest law, M5.2 D); the blade judges
/// flat floors and passes false. `crest` names the blank level the
/// clip reads - None takes the window's own span (the thread's
/// crest), Some the model's tallest claim (a lone crown's steel
/// cannot reach past the bar, M5.2 E2). `excess` is the judged
/// model's chord error past the import promise ([`gouge_tol_on`]).
pub(crate) fn judge(
    pts: &[SR],
    moves: Vec<BandMove>,
    (tol, excess): (f64, f64),
    leave: f64,
    clip_shell: bool,
    crest: Option<f64>,
) -> Vec<Warning> {
    let sg = segs(pts);
    let (window, lat) = lattice(pts, &moves);
    let (gouge, rest) = scan(&sg, &moves, window, &lat, (tol, gouge_tol_on(tol, excess)), leave, clip_shell, crest);
    let g = gouge.as_ref().map(|a| a.mm).unwrap_or(0.0);
    let r = rest.as_ref().map(|a| a.mm).unwrap_or(0.0);
    if g <= 0.0 && r <= 0.0 {
        return vec![];
    }
    let at = gouge
        .as_ref()
        .map(|a| a.move_i)
        .or_else(|| rest.as_ref().map(|a| a.move_i))
        .unwrap_or(0);
    vec![Warning::Envelope {
        at,
        gouge_mm: g,
        rest_mm: r,
    }]
}
