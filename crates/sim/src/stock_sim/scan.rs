//! Split from `stock_sim.rs` by M5.0.1; structure only.

use super::kernel::{MAX_CELLS_PER_AXIS, StockError, part_surface};
use super::scan_arc::ScanArc;
use super::field_lathe::LatheAxis;
use super::scan_hidden::{hidden_rest, HiddenRest, ScanBudget};
use super::scan_turned::TurnedBand;

use crate::cl::{Move, MoveKind};
use crate::geo2d::Point2;
use crate::geometry::{Point3, Triangle};

/// The centre path: segments, the XY arc piece each one stands for
/// (`scan_arc`; `None` for a straight one), and their ring ids.
pub(super) type CenterPath = (Vec<(Point3, Point3)>, Vec<Option<ScanArc>>, Vec<u32>);

/// The envelope verdict for a finishing path (M2.2a), on a stated
/// lattice, measured not assumed. Two truths, each read by the law
/// that is exact for it. Gouge is facet truth (M5.2 E): the exact
/// distance from each model triangle to each instant's ball center,
/// and `r` minus it where positive - a surface standing inside the
/// swept envelope. A column floor cannot say that honestly (it is
/// inflated by 1/cos on tilts, blind on walls, and convicts a pass
/// running legitimately under an overhang); two convex sets can.
/// Rest is distance truth at RIDGES: a
/// model surface point that stands within the pass's promise band
/// (`r + tol + floor` of at least two chords the center path ran
/// over) or that the pass FLANKS (the nearest feet of two passes on
/// opposite sides of it within one ball diameter, a ball able to touch
/// it - M13.8, #212, `ridge_scan`), and how far outside every ball it
/// stood. Two band
/// contacts is exactly what "this op rolled a stepdown ladder over
/// you" promises, so what stands outside both is scallop left on the
/// pass's own word; the band alone caps that word at `tol` (a point
/// inside it is within `tol` of a ball), so the flank is what lets
/// a too-wide spacing convict. A point with one graze (a lone ring's annulus,
/// the box's footprint square) or none (a flat shelf, the crown
/// above the top ring, untouched walls) was never promised anything
/// by this pass: its leftover is rest machining's jurisdiction
/// (M2.3), law there, not a silence here. Gating rest by "this pass
/// cut this column" instead was tried and is dead code - a ball
/// reaching a column's surface is the same event as a center standing
/// within `r + floor` of that point (prove it: `cut <= t + floor`
/// rearranges to `dist^2 <= (r + floor)^2`), so it can never report
/// above tolerance. Proved, not observed.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct EnvelopeReport {
    /// Worst depth by which the model surface stood INSIDE the swept
    /// envelope (`r - distance`, and a path that buried its centers in
    /// material reads the saturation value `r` - as deep as a distance
    /// measurement can say). 0.0 when the envelope never reached the
    /// surface. Not a trench depth: the column's "how far below the
    /// wanted surface" phrasing died with the column law (M5.2 E).
    pub gouge_mm: f64,
    /// Worst distance the model surface stood OUTSIDE the swept
    /// envelope at a ridge the pass promised to roll over (two band
    /// contacts; single grazes and untouched columns are law, not
    /// this number). M2.2d: the union with the solid truth - material
    /// standing hidden under an occluding top grows this number too,
    /// measured by the same distance law. The union is in the FIELD
    /// only; the ridge predicate above is untouched by it.
    pub rest_mm: f64,
    /// Worst distance the model surface stood outside every ball at a
    /// point the pass FLANKED (M13.8 B / M13.9 / #212, `ridge_scan`):
    /// the nearest feet of two passes on opposite sides within one
    /// ball diameter, a tangent ball able to touch it, no band contact. The spacing's own scallop -
    /// a quality reading, kept apart from `rest_mm` (which `Envelope`
    /// convicts on) so it can be reported without blocking. 0.0 when
    /// no flanked point stood outside the band.
    pub scallop_mm: f64,
    /// XY of the worst flanked point, `scallop_mm`'s place.
    pub scallop_at: Point2,
    /// The lattice's cell size, so a caller can state the
    /// quantization floor of these numbers honestly.
    pub cell: f64,
    /// XY of the largest single deviation.
    pub at: Point2,
    /// Whether the hidden-rest half was judged (M13.1): over its
    /// budget the scan answers `OverBudget` and `rest_mm` speaks for
    /// the ridge law alone - "not judged", never a zero that reads
    /// like "no rest".
    pub hidden: HiddenRest,
}

/// The envelope verdict for a ball-mill path: the moves carry TIP
/// positions (the whole codebase's convention) and a ball's tip sits
/// `r` under its center, so the centers are the tips lifted by `r`.
pub fn envelope_scan(
    facets: &[Triangle],
    moves: &[Move],
    r: f64,
    tol: f64,
) -> Result<EnvelopeReport, StockError> {
    envelope_scan_lift(facets, moves, r, tol, r, ScanBudget::SHIPPED, (None, 0.0))
}

/// `envelope_scan` under a stated hidden-rest budget (M13.1): the
/// same verdict, with the budget a caller can name - and a test can
/// shrink to watch the scan say "not judged".
pub fn envelope_scan_within(
    facets: &[Triangle],
    moves: &[Move],
    r: f64,
    tol: f64,
    budget: ScanBudget,
) -> Result<EnvelopeReport, StockError> {
    envelope_scan_lift(facets, moves, r, tol, r, budget, (None, 0.0))
}

/// The same law for a path whose positions ARE the envelope centers
/// (M5.2 E): a turning generator walks its nose CENTER along the
/// offset, so the mill's `+r` tip lift would tilt every ball off the
/// skin it rode - exactly `r·(1 - 1/cos n̂_z)` INTO a face whose
/// material stands above it (the chamfer's phantom gouge) and the same
/// depth OUT of every face tilting the other way (a blindness no
/// passing test showed, because a vacuous clean passes). One
/// convention per kind, chosen by the caller that knows the kind.
pub fn envelope_scan_centers(
    facets: &[Triangle],
    moves: &[Move],
    r: f64,
    tol: f64,
) -> Result<EnvelopeReport, StockError> {
    envelope_scan_turned(facets, moves, r, tol, (None, 0.0))
}

/// `envelope_scan_centers` for a turning path whose spindle is known
/// (#1408, `scan_turned`): the hidden-rest half reads each hidden top
/// on its orbit about `axis`, excuses one outside the axial band the
/// nose centres travel or one the nose (grown by the op's `leave`)
/// cannot touch, and lets `leave` stand. `(None, _)` is
/// `envelope_scan_centers`, bit for bit.
pub fn envelope_scan_turned(
    facets: &[Triangle],
    moves: &[Move],
    r: f64,
    tol: f64,
    (axis, leave): (Option<LatheAxis>, f64),
) -> Result<EnvelopeReport, StockError> {
    envelope_scan_lift(facets, moves, r, tol, 0.0, ScanBudget::SHIPPED, (axis, leave))
}

fn envelope_scan_lift(
    facets: &[Triangle],
    moves: &[Move],
    r: f64,
    tol: f64,
    center_lift: f64,
    budget: ScanBudget,
    (axis, leave): (Option<LatheAxis>, f64),
) -> Result<EnvelopeReport, StockError> {
    if !(r.is_finite() && r > 0.0) {
        return Err(StockError::Invalid(format!(
            "envelope ball radius {r} is not finite-positive"
        )));
    }
    if !tol.is_finite() {
        return Err(StockError::Invalid(format!(
            "envelope tolerance {tol} is not finite"
        )));
    }
    let tol = tol.max(0.0);
    let (mut bx0, mut by0, mut bx1, mut by1) =
        (f64::INFINITY, f64::INFINITY, f64::NEG_INFINITY, f64::NEG_INFINITY);
    for t in facets {
        for v in [t.a, t.b, t.c] {
            if !(v.x.is_finite() && v.y.is_finite() && v.z.is_finite()) {
                return Err(StockError::Invalid("model has a non-finite vertex".into()));
            }
            bx0 = bx0.min(v.x);
            bx1 = bx1.max(v.x);
            by0 = by0.min(v.y);
            by1 = by1.max(v.y);
        }
    }
    if facets.is_empty() || bx1 <= bx0 || by1 <= by0 {
        return Ok(EnvelopeReport::default()); // no footprint to measure on
    }
    // One eighth of the tool radius sees a scallop the tolerance must
    // keep; the floor and the span growth keep the grid honest on
    // dust motes and stadiums alike (MAX_CELLS discipline, applied
    // before allocation, never after).
    let mut cell = (r * 0.125).max(1e-4);
    cell = cell.max((bx1 - bx0) / 1023.0).max((by1 - by0) / 1023.0);
    let margin = r + cell;
    let (x0, y0) = (bx0 - margin, by0 - margin);
    let nx = (((bx1 - bx0 + 2.0 * margin) / cell).ceil() as usize).max(1);
    let ny = (((by1 - by0 + 2.0 * margin) / cell).ceil() as usize).max(1);
    if nx > MAX_CELLS_PER_AXIS || ny > MAX_CELLS_PER_AXIS {
        return Err(StockError::Invalid(format!(
            "envelope grid would be {nx}x{ny} cells; at most {MAX_CELLS_PER_AXIS} per axis"
        )));
    }
    // The arc tessellation of the measurement: the generator's own
    // chord law - every XY shortcut keeps tolerance/4 (the march law,
    // the ring law) - so the scan's discretization error never
    // outweighs the promise it is measuring. A cell-derived budget
    // was tried: at the r/8 lattice it shortens an arc by a sagitta
    // of cell/4, which exceeds the tol + floor gate itself, and a
    // tangential ring's chords then dig a phantom trench the emitted
    // G2 arc never cut. This budget only ever tightens the yardstick:
    // the machine follows the true arc, chord or not. Since #413 the
    // chords only index: every distance is read off the arc piece each
    // one stands for (`scan_arc`), so this budget sizes pieces, not
    // error.
    let chord_tol = (tol * 0.25).max(1e-9);
    let (segs, arcs, rings) = center_segments_tagged(moves, center_lift, chord_tol);
    let part = part_surface(facets, x0, y0, cell, nx, ny);
    let floor = cell * cell / (2.0 * r); // the sagitta one cell can hide
    let mut rep = EnvelopeReport {
        cell,
        at: Point2::new(0.0, 0.0),
        ..Default::default()
    };
    let mut worst = 0.0f64;
    // Gouge: the facet truth (M5.2 E). Every model triangle against
    // every ball-center instant, by the exact distance of two convex
    // sets - no column, no floor, no height field in the verdict. A
    // flush ride reads 0 by construction (distance == r); a tangent
    // chamfer ride under its own top cap reads 0, because no ball
    // ever reached inside the cap's facets; and the r/2 mixup still
    // reads its ~3 mm, because distance is the whole story. The Dexel
    // sweep that answered this question before is dead in the scan -
    // it inflated tilts by 1/cos, saw nothing on walls, and convicted
    // legitimate under-passes, and no column reading can be fixed
    // into honesty from those.
    let (gouge, gouge_at) = super::gouge::facet_gouge(facets, &segs, &arcs, r, x0, y0, cell, nx, ny)?;
    rep.gouge_mm = gouge;
    if gouge > worst {
        worst = gouge;
        rep.at = gouge_at;
    }
    // Rest: the distance truth at ridges - surface points the center
    // path passed by twice inside the promise band, or FLANKED within
    // one ball diameter (M13.8, #212, `ridge_scan`). The band is what the
    // stepdown law swore, so a point outside every ball there is this
    // path's own failure, while a once-grazed or never-touched point
    // is another op's job (the law above). The band alone could never
    // report above tolerance (a point inside it is within `tol` of a
    // ball by construction - the same proof as the dead column gate);
    // the flank reads past it: a point with feet on opposite sides is
    // between two passes, and what stands outside every ball there is
    // the spacing's own scallop - unless no ball could touch the point
    // at all (a crease narrower than the ball), which is excused.
    let lat = super::ridge_scan::Lattice { part: &part, x0, y0, cell, nx, ny };
    let ridge = super::ridge_scan::ridge_rest(facets, &segs, &arcs, &rings, &lat, (r, tol, floor));
    rep.rest_mm = ridge.rest_mm;
    if let Some(at) = ridge.rest_at
        && ridge.rest_mm > worst
    {
        rep.at = at;
    }
    // Its own reading, never `rest_mm` (M13.9): scallop over
    // tolerance is a quality defect, reported beside the envelope
    // verdict, not folded into what blocks the post.
    rep.scallop_mm = ridge.scallop_mm;
    rep.scallop_at = ridge.scallop_at;
    // Solid truth behind the height field (M2.2d): material tops the
    // visible surface OCCLUDES - a ceiling's shadow over a floor -
    // judged by this same distance law. The union lives in the FIELD:
    // the ridge numbers above stand as computed, predicate untouched.
    // #1408: a turning path's hidden tops read as the turning work
    // sees them (`scan_turned`); a mill's band is `None`.
    let band = axis.and_then(|a| TurnedBand::of(a, &segs, facets, (r, leave, tol)));
    match hidden_rest(facets, &segs, &arcs, &part, (x0, y0, cell), nx, ny, r, floor, budget, band.as_ref()) {
        Ok((hidden, hidden_at)) => {
            if hidden > rep.rest_mm {
                rep.rest_mm = hidden;
                rep.at = hidden_at;
            }
        }
        Err(unjudged) => rep.hidden = unjudged, // said by name, never a zero
    }
    Ok(rep)
}

/// The ball-center polyline the path sweeps: rapids excluded, arcs
/// reduced to their sagitta chords with Z interpolated - an XY arc's
/// chords each carrying the true arc piece every distance is read
/// off (#413, `scan_arc`) - everything
/// lifted by `center_lift` - `r` for a tip-stored ball-mill path (the
/// exact discretization `subtract_ball` cuts with), 0.0 for a turning
/// path whose positions are already nose centers (M5.2 E).
///
/// A POSED move lifts along its pose, not along Z (M13 rotary): the
/// stream locates the tool tip, and a leaning ball's center stands
/// `r` up the tool axis from it - the probe frame's own reading. The
/// move's start rides the pose the previous move held (the post keeps
/// the tilt modal), its end the pose it states. A prismatic move - no
/// axis, or the exact Z column - lifts by `+center_lift` in Z, bit for
/// bit the law every 3-axis verdict and golden was measured with.
/// Each segment comes with the closed iso-Z ring its move rides
/// (`ridge::closed_ring_ids`; 0 for none) - the flank law's "two
/// distinct passes" (M13.9).
pub(super) fn center_segments_tagged(
    moves: &[Move],
    center_lift: f64,
    chord_tol: f64,
) -> CenterPath {
    let ring_of = super::ridge::closed_ring_ids(moves);
    let mut rings: Vec<u32> = Vec::new();
    let lift = |p: Point3, axis: Option<[f64; 3]>| -> Point3 {
        match axis {
            Some(a) if !crate::cl::is_vertical(&a) && center_lift != 0.0 => Point3::new(
                p.x + a[0] * center_lift,
                p.y + a[1] * center_lift,
                p.z + a[2] * center_lift,
            ),
            _ => Point3::new(p.x, p.y, p.z + center_lift),
        }
    };
    let mut segs = Vec::new();
    let mut arcs: Vec<Option<ScanArc>> = Vec::new();
    let mut held: Option<[f64; 3]> = None;
    let mut last_ring = 0u32;
    for (i, m) in moves.iter().enumerate() {
        // The segments the previous move added carry ITS ring; this
        // move's are tagged by the next iteration (or the tail below).
        rings.resize(segs.len(), last_ring);
        last_ring = ring_of.get(i).copied().unwrap_or(0);
        let was = held;
        if m.axis.is_some() {
            held = m.axis;
        }
        if m.kind == MoveKind::Rapid || !m.is_finite() {
            continue;
        }
        let tilted = |a: Option<[f64; 3]>| a.is_some_and(|a| !crate::cl::is_vertical(&a));
        if tilted(held) || tilted(was) {
            // Posed: its own 3-D pieces, each end lifted along the pose
            // it stands in - the move's start on the one held coming
            // in, everything after on the one it holds (a move stating
            // nothing keeps the modal tilt, as the post does).
            for (j, (a, b)) in m.segments3(chord_tol).into_iter().enumerate() {
                let pa = if j == 0 { was } else { held };
                segs.push((lift(a, pa), lift(b, held)));
            }
            continue;
        }
        match m.arc {
            None => segs.push((
                Point3::new(m.from.x, m.from.y, m.from.z + center_lift),
                Point3::new(m.to.x, m.to.y, m.to.z + center_lift),
            )),
            // XZ / YZ arcs (M11.2 A): their own 3-D chords, lifted.
            Some(arc) if !arc.plane.is_xy() => {
                for (a, b) in m.segments3(chord_tol) {
                    segs.push((
                        Point3::new(a.x, a.y, a.z + center_lift),
                        Point3::new(b.x, b.y, b.z + center_lift),
                    ));
                }
            }
            // XY arcs measure on the true circle (#413, `scan_arc`);
            // each piece keeps `arc_chords`' chord for indexing.
            Some(arc) => {
                for (seg, piece) in super::scan_arc::xy_arc_pieces(m, arc, center_lift, chord_tol) {
                    arcs.resize(segs.len(), None);
                    segs.push(seg);
                    arcs.push(piece);
                }
            }
        }
    }
    rings.resize(segs.len(), last_ring);
    arcs.resize(segs.len(), None);
    (segs, arcs, rings)
}

/// 3D point-to-segment distance - the honest "did any ball of the
/// sweep reach this surface point" test a per-column floor cannot
/// answer on tilted walls.
pub(super) fn dist_pt_seg3(px: f64, py: f64, pz: f64, a: Point3, b: Point3) -> f64 {
    let (ex, ey, ez) = (b.x - a.x, b.y - a.y, b.z - a.z);
    let len2 = ex * ex + ey * ey + ez * ez;
    let t = if len2 > 0.0 {
        ((px - a.x) * ex + (py - a.y) * ey + (pz - a.z) * ez) / len2
    } else {
        0.0
    }
    .clamp(0.0, 1.0);
    let (dx, dy, dz) = (px - a.x - ex * t, py - a.y - ey * t, pz - a.z - ez * t);
    dx.hypot(dy).hypot(dz)
}

// ---- M2.3 stage A: the program-level sim ------------------------------
