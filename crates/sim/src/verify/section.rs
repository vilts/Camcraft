//! Law (b), the section/tangency gouge law d(center, boundary) >= r - leave - tol, and the wiring of the solid law (b2) it shares a domain with - the shop practice of holding the cutter radius to the finish wall (machine-tool handbooks) on our own section model (split from `engine.rs` by M13.7; structure only).

use super::arc_wall::Trace;
use super::laws::{dist2_segs, law_points};
use super::rest::Rest;
use super::types::{Mode, VerifyContext, Warning};

use crate::cl::{Move, MoveKind, Toolpath};
use crate::geo2d::{Loop, Point2};

mod radius;
pub(super) mod wall;

use wall::edge_is_wall;

/// Sample spacing of the solid law (b2) along each move, mm - the
/// operation layer's containment step.
const SOLID_STEP: f64 = 0.25;

/// The share of the gouge tolerance an arc move is read to by this law
/// (#689). The law reads an arc by its inscribed chords: at the full
/// tolerance those stand up to `tol` off the arc the machine cuts -
/// toward the centre, so a nominal arc round a convex wall read as a
/// gouge, and away from it, so an arc bowing into a concave wall hid up
/// to `tol` of real gouge. A quarter keeps the reading's own error to a
/// quarter of the verdict both ways; the verdict itself is unchanged.
const ARC_READ_SHARE: f64 = 0.25;

/// Whether laws (b) and (b2) speak in `mode`.
///
/// Mode::Envelope stands this law down by design: a waterline ring
/// is tangent - its centers hold exactly r to the model at the
/// tangent line while sitting nearer than r to the level silhouette
/// elsewhere - and tangency is not gouging. Law (f) measures the
/// swept ball instead. Mode::Revolve stands down for the same
/// reason read the other way: a lathe's centers sit ON the
/// section's silhouette by design - the tool tip traces the
/// meridian - so "inside the section" is where a turned path
/// legitimately lives; the revolve verdict owns that geometry.
/// Mode::Radial stands down the same way read again (M5.2 F2): a
/// radial plunge's centers RUN THROUGH the silhouette wall - head
/// on, d = 0, every feed point of the bore inside the finish
/// circle it came to make. Tangency is the mill's law; the door
/// the kernel measured on the mesh is the radial one.
/// Mode::Cone stands down on the same reading from the vertical
/// mill's side (M2.5 E B): a chamfer ride's axis runs ON the wall
/// loop it rules (d = 0 by construction) and a deburr's tip
/// stands inside the corner material the ride exists to remove -
/// both facts are the generatrix law, not near misses. The reach
/// laws stay, unlike Engrave's: the cone's shank is wider than
/// the cone and the wall beside the edge is steel.
pub(super) fn judged(mode: Mode) -> bool {
    !matches!(
        mode,
        Mode::Engrave | Mode::Carve | Mode::Envelope | Mode::Revolve | Mode::Radial | Mode::Cone | Mode::Rotary | Mode::Undercut
    )
}

/// (b) Gouge: tangency law d(center, boundary) >= r - leave - tol,
/// held at what [`segments`] reads of the move (in `Mode::Area` every
/// chord along its whole length, else the law points). For a
/// model-driven path the boundary is level-dependent: the region's
/// silhouette walls plus the model's cross-section strictly above
/// the move's deepest plane. That section is exact for a fixed-Z
/// level pass, and conservative for a ramp or helix, whose whole XY
/// the generator already proved inside the deepest level's center
/// area (the chord and swept-disc laws) - a column that is only an
/// island deeper is not a hole here, and one the ladder must never
/// enter is. A center inside the section is a full collision, not
/// a near miss, so containment joins the distance test.
///
/// The section is read where the cutter is a full-radius cylinder
/// (M13.7): `lift` above the tip, the height the real tip reaches
/// `r` (0 for a square end, so its verdict is unchanged). Material
/// in the tip band below it is the solid law's (b2, [`solid`]) -
/// judged exactly instead of as a square rim.
pub(super) fn tangency(
    path: &Toolpath,
    cx: &VerifyContext,
    rest: &Rest,
    lift: f64,
    chord_tol: f64,
    warns: &mut Vec<Warning>,
) {
    let r = cx.tool.radius;
    // The region's declared rims (#1057), found once: `loop_tol` reads
    // them at the plain tolerance, every other loop tightened.
    let rims: Vec<&Loop> = cx.region.loops().iter().filter(|l| cx.is_rim(l)).collect();
    let leave = (-cx.stock_to_leave).max(0.0);
    let radius = radius::Radius::new(path, cx);
    let zones = super::corner_relief::valid_zones(cx.corner_reliefs, r, cx.stock_to_leave, cx.gouge_tol, cx.stock);
    for (i, m) in path.moves.iter().enumerate() {
        if m.kind == MoveKind::Rapid {
            continue;
        }
        let r = radius.at(m);
        let (tight, plain) = ((r - leave - cx.tightened(cx.gouge_tol)).max(0.0), (r - leave - cx.gouge_tol).max(0.0));
        let floor_of = |l: &Loop| if rims.iter().any(|x| std::ptr::eq(*x, l)) { plain } else { tight };
        let section = cx.model.map(|f| rest.islands_at(f, m.from.z.min(m.to.z) + lift));
        if let Some(None) = section {
            warns.push(Warning::Gouge { at: i }); // cannot prove the slice
            continue;
        }
        let section = section.flatten();
        let on_relief = !zones.is_empty() && m.arc.is_none() && {
            let a = Point2::new(m.from.x, m.from.y);
            let b = Point2::new(m.to.x, m.to.y);
            let z_min = m.from.z.min(m.to.z);
            zones.iter().any(|rc| super::corner_relief::move_on_zone(a, b, z_min, m.from.z.max(m.to.z), r, cx.gouge_tol, rc))
        };
        let chords = segments(m, cx.mode, chord_tol, cx.stock_top_z + cx.z_tol);
        let mut ends = chords.first().map(|c| c.0).into_iter().chain(chords.iter().map(|c| c.1));
        if !on_relief && section.as_ref().is_some_and(|s| ends.any(|p| s.contains(p))) {
            warns.push(Warning::Gouge { at: i });
            continue;
        }
        // The part is gouged below `r`, less only a NEGATIVE leave (a
        // deliberate overcut) - b2's own allowance. A positive leave
        // never licenses cutting into the finished wall: the old
        // `r - leave` let a 0.5 mm leave sink 0.5 mm unconvicted.
        // #864: a model coarser than the importer promises gives its
        // excess chord error back out of the tolerance - the true wall
        // of a convex feature stands that far outside these loops -
        // except on a rim the operation declares (#1057, `loop_tol`).
        // #2657: in Boundary mode with a model, only region edges that
        // stand as walls in `section` are judged - open rims (such as a
        // Face's outer rim or through-holes at the cut plane) stay open.
        let boundary_with_model = cx.mode == Mode::Boundary && cx.model.is_some();
        let near_chords = |(p, q, l): (Point2, Point2, &Loop)| {
            let floor = floor_of(l);
            chords.iter().any(|&(a, b)| dist2_segs(a, b, p, q) < floor * floor)
        };
        // #3831: the cheap distance test first; both are pure, so no verdict moves.
        let region_gouged = edges(cx.region).any(|e| {
            near_chords(e)
                && (!boundary_with_model || section.as_ref().is_some_and(|s| edge_is_wall(e.0, e.1, s)))
        });
        let section_gouged = !on_relief && section.as_ref().is_some_and(|s| edges(s).any(near_chords));
        let gouged = region_gouged || section_gouged;
        // The whole-move wall reading (#43, XY arcs since #905, XZ / YZ
        // arcs since #946): every move along its whole XY trace, so a
        // wall corner between two of an arc's joints is measured, not
        // missed.
        let walled = !gouged
            && cx.mode == Mode::Boundary
            && wall_gouge(&Trace::of(m), cx.region, section.as_ref(), cx.model.is_some(), &floor_of);
        if gouged || walled {
            warns.push(Warning::Gouge { at: i });
        }
    }
}

/// The Boundary-mode whole-move reading (issue #43): a straight move, an
/// XY arc on its true circle (#905, [`super::arc_wall`]), or an XZ / YZ
/// arc on its true XY footprint (#946, [`super::arc_wall_plane`]), measured
/// along its whole length against the region's WALLS only.
/// The law points cannot see a G1 chorded across a convex corner of the
/// wall with both ends `r` off it; a whole-chord reading of EVERY edge
/// convicts a Face's rows, which cross the face's OPEN rim by design.
/// So each edge is classified first:
/// - every edge of the model section is a wall (part material on its
///   one side, by construction);
/// - with a model, a region edge is a wall when the section holds a
///   probe `WALL_PROBE` off its midpoint on either side - a Face's
///   rim over air (or stock) is open, a Contour's outline at a level
///   the part still stands is a wall;
/// - without a model there is no material to probe: a move that
///   properly crosses a region edge is overrunning an open rim (the
///   Face row; a wall is never crossed by a legitimate move), and only
///   a move that crosses none is read against the region's edges.
fn wall_gouge(
    trace: &Trace,
    region: &crate::geo2d::Region,
    section: Option<&crate::geo2d::Region>,
    has_model: bool,
    floor_of: &dyn Fn(&Loop) -> f64,
) -> bool {
    let near = |(p, q, l): (Point2, Point2, &Loop)| trace.near(p, q, floor_of(l));
    if section.is_some_and(|s| edges(s).any(near)) {
        return true;
    }
    if has_model {
        // #3831: the cheap distance test first; both are pure.
        edges(region).any(|e| near(e) && section.is_some_and(|s| edge_is_wall(e.0, e.1, s)))
    } else {
        !crosses_rim(trace, region) && edges(region).any(near)
    }
}

/// Whether `trace` crosses the rim of `region`: properly through an
/// edge's interior, or through a vertex from one side to the other
/// (#1659). A loop may carry vertices on a straight side (the stock's
/// outline did, on the rows' own lines), and a Face row that runs
/// exactly through such a vertex crosses no edge's interior - it was
/// read as a move inside the region, and its tangent pass to the far
/// side convicted, with no part in the document.
fn crosses_rim(trace: &Trace, region: &crate::geo2d::Region) -> bool {
    edges(region).any(|(p, q, _)| trace.crosses(p, q))
        || region.loops().iter().any(|l| {
            let pts = l.points();
            let n = pts.len();
            (0..n).any(|k| trace.through(pts[(k + n - 1) % n], pts[k], pts[(k + 1) % n]))
        })
}

/// Every closed edge of every loop of `r`, with the loop it closes.
fn edges(r: &crate::geo2d::Region) -> impl Iterator<Item = (Point2, Point2, &Loop)> + '_ {
    r.loops().iter().flat_map(|l| {
        let pts = l.points();
        (0..pts.len()).map(move |k| (pts[k], pts[(k + 1) % pts.len()], l))
    })
}

/// What law (b) measures of one move. `Mode::Area` (the pocket family:
/// every region edge is a wall or an island) reads every chord along
/// its whole length (issue #42): a straight feed whose two ends both
/// stand r off the part still cuts across a convex corner between them,
/// and the reading at the move's `to` alone let a G1 chorded over a
/// block corner sink 1.46 mm into it unconvicted. `Mode::Boundary` keeps the
/// law points as zero-length chords: a Face's rows run across the
/// face's open edge by design, which a whole-chord reading of every
/// edge would convict; its straight moves and arcs answer to the WALL
/// edges along their whole length instead ([`wall_gouge`], issues #43,
/// #905 and #946).
///
/// The whole-chord reading covers only the stretch of the move that is
/// below `wall_top` (the stock top): above it the region's walls are
/// not standing, so a ramp that crosses the wall line in the air on its
/// way down into the pocket is not a gouge. A move entirely above
/// `wall_top` keeps the law points.
///
/// Arcs are read at [`ARC_READ_SHARE`] of `chord_tol`. The law points
/// are the UNION of the joints at both tolerances (#689 review B1): the
/// two joint sets are not nested (3 chords at `tol` become 5 at
/// `tol / 4`), so the fine set alone can step over a sharp wall vertex
/// an old joint stood on. Points on the arc never read closer than the
/// truth, so the union adds no false gouge and keeps every old one. A
/// joint reading is still a sampling: in Boundary mode an arc also
/// answers on its true XY trace to the walls ([`wall_gouge`], #905 for
/// XY, #946 for XZ / YZ).
fn segments(m: &Move, mode: Mode, chord_tol: f64, wall_top: f64) -> Vec<(Point2, Point2)> {
    let fine = chord_tol * ARC_READ_SHARE;
    let points = || {
        let mut v = law_points(m, fine);
        if m.arc.is_some() {
            v.extend(law_points(m, chord_tol));
        }
        v.into_iter().map(|p| (p, p)).collect()
    };
    let chord_tol = fine;
    if mode != Mode::Area {
        return points();
    }
    let (z0, z1) = (m.from.z, m.to.z);
    if !(z0.is_finite() && z1.is_finite()) {
        return m.chords(chord_tol); // fail closed: the whole move answers
    }
    if z0 > wall_top && z1 > wall_top {
        return points();
    }
    let chords = m.chords(chord_tol);
    if z0 <= wall_top && z1 <= wall_top {
        return chords;
    }
    // One end above the wall top: keep the chords past the crossing.
    // z is linear in the move's parameter, chords run from -> to in
    // order, so the crossing is a fraction of the summed chord length.
    let t = ((wall_top - z0) / (z1 - z0)).clamp(0.0, 1.0);
    let lens: Vec<f64> = chords.iter().map(|&(a, b)| ((b.x - a.x).powi(2) + (b.y - a.y).powi(2)).sqrt()).collect();
    let total: f64 = lens.iter().sum();
    if total.is_nan() || total <= 0.0 {
        return chords;
    }
    let cut = t * total;
    let descending = z0 > z1; // the end below the top is `to`
    let mut out = Vec::new();
    let mut acc = 0.0;
    for (&(a, b), &len) in chords.iter().zip(&lens) {
        let (s0, s1) = (acc, acc + len);
        acc = s1;
        let lerp = |s: f64| {
            let u = if len > 0.0 { ((s - s0) / len).clamp(0.0, 1.0) } else { 0.0 };
            Point2::new(a.x + (b.x - a.x) * u, a.y + (b.y - a.y) * u)
        };
        if descending {
            if s1 >= cut {
                out.push((lerp(cut.max(s0)), b));
            }
        } else if s0 <= cut {
            out.push((a, lerp(cut.min(s1))));
        }
    }
    out
}

/// (b2) The solid law: the plane sections of (b) are blind to what
/// no level slices - a blind floor between two planes, an
/// overhang above the ring, an undercut lip - so a model-driven
/// path also answers to the mesh's solid at every sampled tip,
/// with the op's real tip (M13.7) when the context carries one
/// that agrees with `cx.tool`, else the square stack (never
/// smaller than the real tip, so it can only over-report).
pub(super) fn solid(
    path: &Toolpath,
    cx: &VerifyContext,
    cutter: &super::Cutter,
    warns: &mut Vec<Warning>,
) {
    if let Some(f) = cx.model {
        let law = super::SolidLaw {
            step: SOLID_STEP,
            // #864: tightened by a coarse model's excess, as (b) is.
            allow: cx.tightened(cx.gouge_tol) + (-cx.stock_to_leave).max(0.0),
            hole_walls: false,
        };
        let zones: Vec<_> = super::corner_relief::valid_zones(
            cx.corner_reliefs,
            cx.tool.radius,
            cx.stock_to_leave,
            cx.gouge_tol,
            cx.stock,
        )
        .into_iter()
        .copied()
        .collect();
        for w in super::solid_hits_relieved(path, f, cutter, &law, &zones) {
            if !warns.contains(&w) {
                warns.push(w);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    // Provenance: analytic lattice wall and trace fixtures for issue #1669.
    use super::*;
    use crate::geo2d::Region;

    #[test]
    fn rim_vertex_wall_wobble_still_answers_to_walls() {
        // The lattice keeps these three vertices distinct, while each
        // neighbour is less than CROSSING_TOUCH off the trace line.
        let wall = Loop::new(vec![
            Point2::new(-1e-6, -10000.0),
            Point2::new(0.0, 0.0),
            Point2::new(1e-6, 9999.0),
            Point2::new(10.0, 9999.0),
            Point2::new(10.0, -10000.0),
        ])
        .unwrap();
        let region = Region::new(vec![wall]).unwrap();
        assert!(
            region
                .loops()
                .iter()
                .any(|l| l.points().contains(&Point2::new(0.0, 0.0)))
        );
        let trace = Trace::Seg(Point2::new(-0.5e-6, -5000.5), Point2::new(0.5e-6, 5000.5));
        assert!(
            !crosses_rim(&trace, &region),
            "sub-band wobble is no rim crossing"
        );
        assert!(wall_gouge(&trace, &region, None, false, &|_| 0.1));
    }

    #[test]
    fn tangency_excuses_open_rim_with_model_and_convicts_wall_below_top() {
        use crate::geometry::{Point3, cube};
        // Plate z 0..10 (outer wall below model top z 15) + boss z 10..15.
        let mut model = cube(Point3::new(-20.0, -30.0, 0.0), Point3::new(20.0, 30.0, 10.0)).facets.to_vec();
        model.extend(cube(Point3::new(5.0, -10.0, 10.0), Point3::new(15.0, 10.0, 15.0)).facets.iter().cloned());
        let region = Region::new(vec![Loop::new(vec![
            Point2::new(-20.0, -30.0), Point2::new(20.0, -30.0),
            Point2::new(20.0, 30.0), Point2::new(-20.0, 30.0),
        ]).unwrap()]).unwrap();
        let tool = super::super::types::ToolGeom {
            radius: 5.0, nose_radius: 0.0, flute_len: 20.0,
            shank_radius: 5.0, shank_len: 30.0, holder_radius: 16.0, holder_len: 40.0,
        };
        let cx = VerifyContext {
            region: &region, model: Some(&model), tool: &tool, mode: Mode::Boundary,
            stock_top_z: 15.0, clearance_z: 30.0, gouge_tol: 0.01, z_tol: 1e-6, ..Default::default()
        };
        let run = |moves: Vec<Move>| {
            let path = Toolpath { moves, ..Toolpath::default() };
            let mut warns = Vec::new();
            tangency(&path, &cx, &Rest::new(&path, &cx, cx.gouge_tol, &[]), 0.0, cx.gouge_tol, &mut warns);
            warns
        };
        // At z 10, (-18.227, -5) is 1.773 mm inside the open x = -20 rim: clean.
        assert_eq!(run(vec![
            Move::new(MoveKind::Plunge, Point3::new(-18.227, -5.0, 30.0), Point3::new(-18.227, -5.0, 10.0), Some(100.0)),
            Move::new(MoveKind::Feed, Point3::new(-25.0, -5.0, 10.0), Point3::new(-18.227, -5.0, 10.0), Some(500.0)),
        ]), Vec::new());
        // At z 8 (below z 10 wall top, below z 15 model top), x = -24.95 is 4.95 mm from wall: gouge.
        assert_eq!(run(vec![
            Move::new(MoveKind::Plunge, Point3::new(-24.95, -5.0, 30.0), Point3::new(-24.95, -5.0, 8.0), Some(100.0)),
            Move::new(MoveKind::Feed, Point3::new(-24.95, -5.0, 8.0), Point3::new(-24.95, 5.0, 8.0), Some(500.0)),
        ]), vec![Warning::Gouge { at: 0 }, Warning::Gouge { at: 1 }]);
    }
}
