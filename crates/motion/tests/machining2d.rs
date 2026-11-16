//! Face, contour, pocket, slot and trace generator goldens: row counts,
//! offsets and spacings are exact hand-computable numbers, and rapids may
//! only appear where the strategy says (face: at the path ends; contour:
//! between loops and at the hop planes; pocket/slot/trace: at the
//! clearance plane for level changes, and at the retract hop plane for
//! lifts between passes of one level - None retract means the hop plane
//! is the clearance plane, and the rapids are where they always were).
//!
//! Gate map (M4.4): covers camcraft_motion::cl, camcraft_motion::geo2d, camcraft_motion::machining2d.

use camcraft_motion::cl::{MoveKind, Toolpath};
use camcraft_motion::entry::Entry3D;
use camcraft_motion::geo2d::{Loop, Point2, Region};
use camcraft_motion::machining2d::{
    ARC_FIT_TOL, chord_clears, contour, contour_along_chain, face, on_boundary, pocket,
    ring_is_circular, slot, trace, ContourParams, FaceHoles, FaceParams, GenError, Lead, PocketParams,
    PocketStrategy, Side, SlotParams, TraceParams,
};

fn rect(x0: f64, y0: f64, x1: f64, y1: f64) -> Region {
    Region::new(vec![Loop::new(vec![
        Point2::new(x0, y0),
        Point2::new(x1, y0),
        Point2::new(x1, y1),
        Point2::new(x0, y1),
    ])
    .unwrap()])
    .unwrap()
}

fn params() -> FaceParams {
    FaceParams { holes: Default::default(),
        tool_diameter: 10.0,
        stepover: 5.0,
        clearance_z: 5.0,
        cut_z: 2.0,
        top_z: None,
        stepdown: None,
        feed: 500.0,
        plunge_feed: 100.0,
        climb: true,
        blend_turns: false,
    }
}

/// Row moves: Feed segments at cut depth running mostly along X.
fn rows(t: &Toolpath) -> Vec<(f64, f64, f64)> {
    t.moves
        .iter()
        .filter(|m| m.kind == MoveKind::Feed && m.from.z == m.to.z)
        .filter(|m| (m.to.x - m.from.x).abs() > (m.to.y - m.from.y).abs())
        .map(|m| (m.from.y, m.from.x, m.to.x))
        .collect()
}

#[test]
fn face_of_a_square_is_three_exact_rows() {
    let r = rect(0.0, 0.0, 20.0, 20.0);
    let t = face(&r, &params()).expect("valid params");
    let rs = rows(&t);
    assert_eq!(rs.len(), 3, "rows at coverage centers 5/10/15: {rs:?}");
    // Serpentine order and alternation.
    assert_eq!(rs[0], (5.0, -5.0, 25.0));
    assert_eq!(rs[1], (10.0, 25.0, -5.0));
    assert_eq!(rs[2], (15.0, -5.0, 25.0));
    assert!(t.is_chained());
    assert!(t.all_finite());
}

#[test]
fn rapids_only_appear_at_the_ends() {
    let r = rect(0.0, 0.0, 20.0, 20.0);
    let t = face(&r, &params()).expect("valid params");
    // The approach (#690): a rapid on the clearance plane to the entry,
    // then the plunge - never a feed out of the tool-change position.
    assert_eq!(t.moves[0].kind, MoveKind::Rapid);
    assert_eq!((t.moves[0].from.z, t.moves[0].to.z), (5.0, 5.0), "the approach runs at clearance");
    assert_eq!(t.moves[1].kind, MoveKind::Plunge);
    assert_eq!(t.moves[1].from, t.moves[0].to, "the plunge drops from the approach's end");
    assert_eq!(t.moves.last().unwrap().kind, MoveKind::Rapid);
    for m in &t.moves[2..t.len() - 1] {
        assert_ne!(m.kind, MoveKind::Rapid, "mid-path rapid: {m:?}");
    }
    // The final rapid lifts straight up from the last row's end.
    let last = t.moves.last().unwrap();
    assert_eq!(last.from.x, last.to.x);
    assert_eq!(last.from.y, last.to.y);
    assert_eq!(last.to.z, 5.0);
}

#[test]
fn a_face_entering_at_the_origin_still_opens_with_its_approach() {
    // #690's edge: tool 10 (r 5), climb - the entry is the region's low
    // X minus r and the first row its low Y plus r, so this region puts
    // the entry exactly at X0 Y0, the cursor's own start. The approach
    // is still the first move, a rapid at clearance, before the plunge.
    let t = face(&rect(5.0, -5.0, 25.0, 15.0), &params()).expect("valid params");
    let (a, plunge) = (&t.moves[0], &t.moves[1]);
    assert_eq!(a.kind, MoveKind::Rapid, "{a:?}");
    assert_eq!((a.to.x, a.to.y, a.to.z), (0.0, 0.0, 5.0), "the approach commands the entry at clearance");
    assert_eq!(plunge.kind, MoveKind::Plunge);
    assert_eq!(plunge.from, a.to);
    assert!(t.is_chained());
}

#[test]
fn face_default_stays_single_level() {
    // The byte-shield of `face_square_40x40.cl`: with no staging the face
    // cuts exactly one level at `cut_z`. Naming the same height explicitly
    // (top == cut, one step) must produce the identical path - staging is
    // opt-in geometry, never a re-phrasing that moves the moves.
    let r = rect(0.0, 0.0, 20.0, 20.0);
    let legacy = face(&r, &params()).expect("valid params");
    let mut p = params();
    p.top_z = Some(p.cut_z);
    p.stepdown = Some(1.0);
    let explicit = face(&r, &p).expect("single-level staging is valid");
    assert_eq!(legacy, explicit, "top_z == cut_z must not change the path");
    assert_eq!(
        explicit.moves.iter().filter(|m| m.kind == MoveKind::Plunge).count(),
        1
    );
}

#[test]
fn face_steps_down_through_levels() {
    let r = rect(0.0, 0.0, 20.0, 20.0);
    let mut p = params(); // cut_z 2.0, clearance 5.0
    p.top_z = Some(4.0);
    p.stepdown = Some(1.0);
    let t = face(&r, &p).expect("valid params");
    assert!(t.is_chained());
    assert!(t.all_finite());
    let zs: Vec<f64> = t
        .moves
        .iter()
        .filter(|m| m.kind == MoveKind::Plunge)
        .map(|m| m.to.z)
        .collect();
    assert_eq!(zs, [4.0, 3.0, 2.0], "three staged levels");
    // Same rows at every level: the band law is computed once and the
    // single-level sweep count repeats per staged level (3 rows, 20x20).
    for &z in &zs {
        let sweeps = t
            .moves
            .iter()
            .filter(|m| {
                m.kind == MoveKind::Feed
                    && m.from.z == z
                    && m.to.z == z
                    && (m.to.x - m.from.x).abs() > (m.to.y - m.from.y).abs()
            })
            .count();
        assert_eq!(sweeps, 3, "level {z} must repeat every row");
    }
    // Between levels the tool lifts to clearance and re-enters from the
    // same corner it plunges at: exactly two rapids before each extra
    // level's plunge, and nothing but rapids leave the level plane.
    let rapids = t
        .moves
        .iter()
        .filter(|m| m.kind == MoveKind::Rapid)
        .count();
    assert_eq!(rapids, 1 + 2 * (zs.len() - 1) + 1, "the approach (#690), hop rapids, the final lift");
    for (i, m) in t.moves.iter().enumerate() {
        if m.kind == MoveKind::Plunge && i > 0 {
            assert_eq!(t.moves[i - 1].kind, MoveKind::Rapid);
            assert_eq!(m.from.z, 5.0, "plunge must start in air");
            assert_eq!(m.from.x, t.moves[i - 1].to.x);
            assert_eq!(m.from.y, t.moves[i - 1].to.y);
        }
    }
}

#[test]
fn face_levels_snap_to_bottom_exactly() {
    // A step that would cut below the finished height is replaced by the
    // finished height itself - never skipped, never overshot.
    let r = rect(0.0, 0.0, 20.0, 20.0);
    let mut p = params(); // cut_z 2.0, clearance 5.0
    p.top_z = Some(4.5);
    p.stepdown = Some(1.5); // would next cut at 1.5, below the finish
    let t = face(&r, &p).expect("valid params");
    let zs: Vec<f64> = t
        .moves
        .iter()
        .filter(|m| m.kind == MoveKind::Plunge)
        .map(|m| m.to.z)
        .collect();
    assert_eq!(zs, [4.5, 3.0, 2.0], "last level lands on cut_z, not below");

    let mut p = params();
    p.top_z = Some(4.5);
    p.stepdown = Some(2.0);
    let t = face(&r, &p).expect("valid params");
    let zs: Vec<f64> = t
        .moves
        .iter()
        .filter(|m| m.kind == MoveKind::Plunge)
        .map(|m| m.to.z)
        .collect();
    assert_eq!(zs, [4.5, 2.5, 2.0], "odd arithmetic still snaps exactly");
}

#[test]
fn conventional_starts_at_the_other_corner() {
    let mut p = params();
    p.climb = false;
    let t = face(&rect(0.0, 0.0, 20.0, 20.0), &p).expect("valid");
    let rs = rows(&t);
    assert_eq!(rs[0], (5.0, 25.0, -5.0));
    assert_eq!(rs[1], (10.0, -5.0, 25.0));
}

#[test]
fn thin_stock_gets_one_centered_row() {
    let r = rect(0.0, 0.0, 30.0, 6.0);
    let t = face(&r, &params()).expect("valid");
    let rs = rows(&t);
    assert_eq!(rs.len(), 1, "one 10-wide tool bands a 6-tall strip once");
    assert_eq!(rs[0].0, 3.0);
}

#[test]
fn holes_are_overflown_not_avoided() {
    // Facing clears a plane: a hole in the footprint does not reroute the
    // rows; the path simply runs across it (an air cut, not a gouge).
    let r = Region::new(vec![
        Loop::new(vec![
            Point2::new(0.0, 0.0),
            Point2::new(20.0, 0.0),
            Point2::new(20.0, 20.0),
            Point2::new(0.0, 20.0),
        ])
        .unwrap(),
        Loop::new(vec![
            Point2::new(8.0, 8.0),
            Point2::new(12.0, 8.0),
            Point2::new(12.0, 12.0),
            Point2::new(8.0, 12.0),
        ])
        .unwrap(),
    ])
    .unwrap();
    // M13.11: that is the `Through` law now - kept to the byte - while
    // the default lifts the row over the hole and re-enters past it.
    let mut through = params();
    through.holes = FaceHoles::Through;
    let t = face(&r, &through).expect("valid");
    assert_eq!(rows(&t).len(), 3, "same rows as the plain square");
    assert_eq!(t.moves, face(&rect(0.0, 0.0, 20.0, 20.0), &through).expect("plain").moves);
    let lifted = face(&r, &params()).expect("valid");
    assert_eq!(rows(&lifted).len(), 6, "the same three rows, each in two pieces around the hole");
    assert!(lifted.moves.iter().filter(|m| m.kind == MoveKind::Plunge).count() > 1, "re-entered past the hole");
}

#[test]
fn degenerate_parameters_are_refused_before_any_geometry() {
    let r = rect(0.0, 0.0, 20.0, 20.0);
    let cases = [
        {
            let mut p = params();
            p.stepover = 0.0;
            p
        },
        {
            let mut p = params();
            p.tool_diameter = f64::NAN;
            p
        },
        {
            let mut p = params();
            p.feed = -1.0;
            p
        },
        {
            let mut p = params();
            p.clearance_z = f64::INFINITY;
            p
        },
        {
            // Staging fields refuse the same way scalars do: a named
            // error before any geometry runs, never a silent one-level cut.
            let mut p = params();
            p.top_z = Some(1.0); // params cut_z is 2.0: top below the finish
            p.stepdown = Some(1.0);
            p
        },
        {
            let mut p = params();
            p.top_z = Some(4.0); // staged descent with no stepdown: the
            p.stepdown = None; //  while-loop of cut_levels must never spin
            p
        },
        {
            let mut p = params();
            p.top_z = Some(4.0);
            p.stepdown = Some(0.0);
            p
        },
        {
            let mut p = params();
            p.top_z = Some(6.0); // first level above the clearance plane
            p.stepdown = Some(1.0);
            p
        },
    ];
    for p in cases {
        assert!(matches!(face(&r, &p), Err(GenError::Invalid(_))), "{p:?}");
    }
    assert!(matches!(
        face(&Region::empty(), &params()),
        Err(GenError::Empty(_))
    ));
}

#[test]
fn face_is_deterministic() {
    // Undo/redo exactness rests on generators being pure.
    let r = rect(-7.5, 2.0, 41.25, 33.0);
    let a = face(&r, &params()).unwrap();
    let b = face(&r, &params()).unwrap();
    assert_eq!(a, b);
    assert!(a.is_chained());
    // Odd extents still band inside themselves: first row at ymin+r, no
    // row beyond the last coverage center, x overrun exactly one radius.
    let rs = rows(&a);
    assert_eq!(rs.first().unwrap().0, 7.0);
    assert_eq!(rs.last().unwrap().0, 28.0);
    for (_, x0, x1) in &rs {
        assert!([(-7.5 - 5.0, 41.25 + 5.0), (41.25 + 5.0, -7.5 - 5.0)].contains(&(*x0, *x1)));
    }
}

// ---- contour ------------------------------------------------------------

fn cp() -> ContourParams {
    ContourParams {
        tool_diameter: 10.0,
        stock_to_leave: 0.0,
        clearance_z: 5.0,
        retract_z: 3.0,
        top_z: 0.0,
        bottom_z: 0.0,
        stepdown: 1.0,
        feed: 500.0,
        plunge_feed: 100.0,
        climb: true,
        side: Side::Auto,
        lead: Lead::None,
        tolerance: 0.01,
        tab_count: 0,
        tab_width: 0.0,
        tab_height: 0.0,
        entry: Entry3D::Plunge,
        helix_radius: 1.0,
        blend_turns: false,
        links: camcraft_motion::links::Links::PLANAR,
    }
}

/// Chord budget for flattening arc moves into polylines: the same sagitta
/// allowance the fitter promised, so the sampled polyline never lies more
/// than tol off the path that will actually be cut.
const FLATTEN_TOL: f64 = 0.01;

/// XY polyline of the level's in-plane Feed moves (start = first move's
/// from, then every chord end in order; arcs flattened at FLATTEN_TOL).
fn level_xy(t: &Toolpath, z: f64) -> Vec<(f64, f64)> {
    let mut out = Vec::new();
    for m in &t.moves {
        if m.kind == MoveKind::Feed && m.from.z == z && m.to.z == z {
            if out.is_empty() {
                out.push((m.from.x, m.from.y));
            }
            for (_, b) in m.chords(FLATTEN_TOL) {
                out.push((b.x, b.y));
            }
        }
    }
    out
}

/// Total in-plane feed length at `z`, summed per move so tab-hop
/// transitions (which leave the plane) can't count as cut distance.
/// Arc moves carry their true circle length - the metal the rim actually
/// walks - not the chord across them.
fn level_len(t: &Toolpath, z: f64) -> f64 {
    t.moves
        .iter()
        .filter(|m| m.kind == MoveKind::Feed && m.from.z == z && m.to.z == z)
        .map(|m| match m.arc {
            None => ((m.to.x - m.from.x).powi(2) + (m.to.y - m.from.y).powi(2)).sqrt(),
            Some(a) => {
                let (rx, ry) = (m.from.x - a.center.x, m.from.y - a.center.y);
                let (ux, uy) = (m.to.x - a.center.x, m.to.y - a.center.y);
                let r = (rx * rx + ry * ry).sqrt();
                let sweep = if a.ccw {
                    (uy.atan2(ux) - ry.atan2(rx)).rem_euclid(std::f64::consts::TAU)
                } else {
                    (ry.atan2(rx) - uy.atan2(ux)).rem_euclid(std::f64::consts::TAU)
                };
                r * sweep
            }
        })
        .sum()
}

fn plunge_xy(t: &Toolpath) -> Vec<(f64, f64)> {
    t.moves
        .iter()
        .filter(|m| m.kind == MoveKind::Plunge)
        .map(|m| (m.to.x, m.to.y))
        .collect()
}

fn shoelace(p: &[(f64, f64)]) -> f64 {
    let mut s = 0.0;
    for i in 0..p.len() {
        let (a, b) = (p[i], p[(i + 1) % p.len()]);
        s += a.0 * b.1 - b.0 * a.1;
    }
    s * 0.5
}

#[test]
fn contour_of_a_square_grazes_the_boundary() {
    let t = contour(&rect(0.0, 0.0, 20.0, 20.0), &cp()).expect("valid");
    assert!(t.is_chained());
    assert!(t.all_finite());
    assert_eq!(plunge_xy(&t).len(), 1, "one loop, one level");
    let p = level_xy(&t, 0.0);
    // Cutter center sits one radius out; straight runs are exact, and the
    // rounded corner passes through radius-5 territory around (20, 20).
    let xs: Vec<f64> = p.iter().map(|q| q.0).collect();
    let ys: Vec<f64> = p.iter().map(|q| q.1).collect();
    assert_eq!(xs.iter().cloned().fold(f64::INFINITY, f64::min), -5.0);
    assert_eq!(xs.iter().cloned().fold(f64::NEG_INFINITY, f64::max), 25.0);
    assert_eq!(ys.iter().cloned().fold(f64::INFINITY, f64::min), -5.0);
    assert_eq!(ys.iter().cloned().fold(f64::NEG_INFINITY, f64::max), 25.0);
    assert!(
        p.iter()
            .any(|q| ((q.0 - 20.0).powi(2) + (q.1 - 20.0).powi(2)).sqrt() - 5.0 < 1e-3),
        "corner arc points expected around the original corner"
    );
    // Closed ring: the last in-plane feed returns to the plunge point.
    assert_eq!(p[0], *plunge_xy(&t).first().unwrap());
    assert_eq!(p[p.len() - 1], p[0]);
}

#[test]
fn leave_pushes_the_path_off_the_wall() {
    let mut p = cp();
    p.stock_to_leave = 2.0;
    let t = contour(&rect(0.0, 0.0, 20.0, 20.0), &p).expect("valid");
    let pts = level_xy(&t, 0.0);
    let xs: Vec<f64> = pts.iter().map(|q| q.0).collect();
    assert_eq!(xs.iter().cloned().fold(f64::INFINITY, f64::min), -7.0);
    assert_eq!(xs.iter().cloned().fold(f64::NEG_INFINITY, f64::max), 27.0);
}

#[test]
fn contour_walks_hole_walls_too() {
    // A 10-wide tool in a 12-wide hole: cutter-center loop is the hole
    // shrunk by r, so the edge grazes both walls exactly.
    let r = Region::new(vec![
        Loop::new(vec![
            Point2::new(0.0, 0.0),
            Point2::new(40.0, 0.0),
            Point2::new(40.0, 40.0),
            Point2::new(0.0, 40.0),
        ])
        .unwrap(),
        Loop::new(vec![
            Point2::new(14.0, 14.0),
            Point2::new(26.0, 14.0),
            Point2::new(26.0, 26.0),
            Point2::new(14.0, 26.0),
        ])
        .unwrap(),
    ])
    .unwrap();
    let t = contour(&r, &cp()).expect("valid");
    assert_eq!(plunge_xy(&t).len(), 2, "outer loop then island wall");
    let pts = level_xy(&t, 0.0);
    assert!(pts.iter().all(|q| q.0 >= -5.0 && q.0 <= 45.0 && q.1 >= -5.0 && q.1 <= 45.0));
    let hole: Vec<(f64, f64)> = pts
        .iter()
        .copied()
        .filter(|q| q.0 > 18.0 && q.0 < 22.0 && q.1 > 18.0 && q.1 < 22.0)
        .collect();
    assert!(!hole.is_empty());
    let hx: Vec<f64> = hole.iter().map(|q| q.0).collect();
    assert_eq!(hx.iter().cloned().fold(f64::INFINITY, f64::min), 19.0);
    assert_eq!(hx.iter().cloned().fold(f64::NEG_INFINITY, f64::max), 21.0);
    assert!(hole.iter().any(|q| q.0 == 19.0 && q.1 == 21.0));
}

#[test]
fn a_hole_the_tool_cannot_enter_vanishes() {
    // 10-wide hole, 10-wide tool: the center loop collapses, so only the
    // outer wall is contoured - nothing is silently cut off the part.
    let r = Region::new(vec![
        Loop::new(vec![
            Point2::new(0.0, 0.0),
            Point2::new(40.0, 0.0),
            Point2::new(40.0, 40.0),
            Point2::new(0.0, 40.0),
        ])
        .unwrap(),
        Loop::new(vec![
            Point2::new(15.0, 15.0),
            Point2::new(25.0, 15.0),
            Point2::new(25.0, 25.0),
            Point2::new(15.0, 25.0),
        ])
        .unwrap(),
    ])
    .unwrap();
    let t = contour(&r, &cp()).expect("valid");
    assert_eq!(plunge_xy(&t).len(), 1);
}

#[test]
fn contour_levels_snap_to_the_bottom() {
    let mut p = cp();
    p.top_z = 0.0;
    p.bottom_z = -6.0;
    p.stepdown = 2.5;
    let t = contour(&rect(0.0, 0.0, 20.0, 20.0), &p).expect("valid");
    assert_eq!(plunge_xy(&t).len(), 4, "levels 0, -2.5, -5, -6");
    let mut zs: Vec<f64> = t
        .moves
        .iter()
        .filter(|m| m.kind == MoveKind::Feed && m.from.z == m.to.z)
        .map(|m| m.from.z)
        .collect();
    zs.sort_by(f64::total_cmp);
    zs.dedup();
    assert_eq!(zs, vec![-6.0, -5.0, -2.5, 0.0], "steps snap onto the bottom");
}

#[test]
fn climb_and_conventional_reverse_the_travel() {
    let up = contour(&rect(0.0, 0.0, 20.0, 20.0), &cp()).unwrap();
    let mut p = cp();
    p.climb = false;
    let down = contour(&rect(0.0, 0.0, 20.0, 20.0), &p).unwrap();
    let (a, b) = (level_xy(&up, 0.0), level_xy(&down, 0.0));
    // #1555: climb (M3) keeps the material right of travel - clockwise
    // round an outside wall; conventional is the reverse.
    assert!(shoelace(&a) < 0.0, "climb runs clockwise round the part");
    assert!(shoelace(&b) > 0.0, "conventional runs counter-clockwise");
    // The chorded polygons agree to the flattening's own word: chord
    // endpoints ride the fitted circle measured from each run's start,
    // so the mirror images differ by sub-step chords, not by geometry.
    assert!((shoelace(&a) + shoelace(&b)).abs() < 1e-3);
    // Same wall, opposite direction: the same perimeter. The fitter
    // walks each ring from its own end, so the seam corner splits
    // differently in the two runs - one direction fits it to arcs, the
    // other leaves a chorded remainder, which measures short by the
    // sagittas it actually travels. Both are honest descriptions of
    // the identical rounded square, so the meaningful law is each
    // path against the wall's true perimeter: straights plus full
    // quarter circles, r = 5 at each corner.
    let true_perimeter = 80.0 + std::f64::consts::TAU * 5.0;
    for l in [level_len(&up, 0.0), level_len(&down, 0.0)] {
        assert!((l - true_perimeter).abs() < 2e-3, "{l} vs wall {true_perimeter}");
    }
}

#[test]
fn linear_lead_overshoots_the_tangent() {
    let base = contour(&rect(0.0, 0.0, 20.0, 20.0), &cp()).unwrap();
    let mut p = cp();
    p.lead = Lead::Linear { length: 4.0 };
    let led = contour(&rect(0.0, 0.0, 20.0, 20.0), &p).unwrap();
    let s = plunge_xy(&base)[0];
    let pts = level_xy(&base, 0.0);
    let q = pts[1];
    let tl = ((q.0 - s.0) / ((q.0 - s.0).powi(2) + (q.1 - s.1).powi(2)).sqrt(),
              (q.1 - s.1) / ((q.0 - s.0).powi(2) + (q.1 - s.1).powi(2)).sqrt());
    let e = plunge_xy(&led)[0];
    let (ex, ey) = (e.0 - s.0, e.1 - s.1);
    let d = (ex * ex + ey * ey).sqrt();
    // Overshoot is exactly the lead's length behind the rim start, and
    // it lies along the wall's own tangent there - measured to the next
    // sampled wall vertex, so a chord-vs-tangent half-step, not a
    // right-angle slam into the wall, is all that may show.
    assert!((d - 4.0).abs() < 1e-9, "overshoot distance {d}");
    let cos = -(ex * tl.0 + ey * tl.1) / d;
    assert!(cos > 0.998, "lead leaves the wall at {cos} rad-equivalent");
    // The lead run lands exactly on the base path's entry vertex.
    assert!(level_xy(&led, 0.0).windows(2).any(|w| w[1] == s && w[0] == e));
    assert!(led.is_chained() && led.all_finite());
}

#[test]
fn arc_lead_enters_as_a_quarter_arc() {
    let base = contour(&rect(0.0, 0.0, 20.0, 20.0), &cp()).unwrap();
    let mut p = cp();
    p.lead = Lead::Arc { radius: 5.0 };
    let led = contour(&rect(0.0, 0.0, 20.0, 20.0), &p).unwrap();
    let (s, e) = (plunge_xy(&base)[0], plunge_xy(&led)[0]);
    // The quarter-arc diagonal: |entry - start| = radius * sqrt(2).
    let d = ((e.0 - s.0).powi(2) + (e.1 - s.1).powi(2)).sqrt();
    assert!((d - 5.0 * 2.0f64.sqrt()).abs() < 1e-9, "entry-to-start {d}");
    // The lead is one arc move landing exactly at the rim's start - not
    // thirteen chords. Two passes per loop (in and out), and nothing
    // else about the rim changed: its arcs are the same fitted circles
    // the Lead::None path carries.
    let arcs = |t: &Toolpath| t.moves.iter().filter(|m| m.arc.is_some()).count();
    assert_eq!(arcs(&led), arcs(&base) + 2, "one arc lands, one arc leaves");
    let land = led
        .moves
        .iter()
        .find(|m| m.arc.is_some() && m.to.x == s.0 && m.to.y == s.1)
        .expect("the landing arc hands off at the rim start, vertex-exact");
    // Tangent continuity: at the join the lead circle and the wall's
    // first arc share their tangent line - centers and contact point
    // collinear, which (radii 5 + 5) means |o - c| = 10. To the
    // fitter's own word, though: the wall's circle is fitted to the
    // tessellated rim, so the contact point sits on the rim's vertex,
    // a residual off the fitted circle - tangency inherits the fit.
    // When the wall starts on a straight, the join is tangent by
    // construction and there is no rim center to test.
    if let Some(wall) = base.moves.iter().find(|m| m.arc.is_some() && m.from.x == s.0 && m.from.y == s.1) {
        let (oc, wc) = (land.arc.unwrap().center, wall.arc.unwrap().center);
        let cd = ((oc.x - wc.x).powi(2) + (oc.y - wc.y).powi(2)).sqrt();
        assert!((cd - 10.0).abs() < 1e-4, "lead and wall circles tangent at s: {cd}");
    }
    // Every arc move is a planar feed; chords chain and stay finite.
    assert!(led.is_chained() && led.all_finite());
}

#[test]
fn tangential_lead_enters_parallel_and_lands_g1() {
    let base = contour(&rect(0.0, 0.0, 20.0, 20.0), &cp()).unwrap();
    let mut p = cp();
    p.lead = Lead::Tangential { radius: 5.0 };
    let led = contour(&rect(0.0, 0.0, 20.0, 20.0), &p).unwrap();
    let (s, e) = (plunge_xy(&base)[0], plunge_xy(&led)[0]);
    // The half-arc law, from one measurement chain: the landing arc
    // (a) has radius 5 at the join, (b) sits with its center on the
    // wall's air-side normal (center - s perpendicular to the wall's
    // own direction there), and (c) ends where it began: e and s are
    // antipodal about the center, so |e - s| = 2r and the motion at the
    // plunge point is PARALLEL to the wall - the difference from the
    // quarter arc, which starts normal to it. (c) also forces the
    // landing direction to be tangent to the path: at s the arc's
    // tangent is perpendicular to (s - o) which is the normal.
    let arcs = |t: &Toolpath| t.moves.iter().filter(|m| m.arc.is_some()).count();
    assert_eq!(arcs(&led), arcs(&base) + 2, "one arc lands, one arc leaves");
    let land = led
        .moves
        .iter()
        .find(|m| m.arc.is_some() && m.to.x == s.0 && m.to.y == s.1)
        .expect("the landing arc hands off at the rim start, vertex-exact");
    let o = land.arc.unwrap().center;
    // The arc center carries a lattice snap (half a micron at most);
    // every bound below allows it.
    let d_os = ((o.x - s.0).powi(2) + (o.y - s.1).powi(2)).sqrt();
    assert!((d_os - 5.0).abs() < 1e-6, "center one radius off the wall: {d_os}");
    assert!(
        ((e.0 - s.0).powi(2) + (e.1 - s.1).powi(2)).sqrt() - 10.0 < 1e-5,
        "plunge stands 2*radius off the join"
    );
    assert!(
        (e.0 - (2.0 * o.x - s.0)).abs() < 1e-6 && (e.1 - (2.0 * o.y - s.1)).abs() < 1e-6,
        "e and s antipodal about the center"
    );
    // Center on the air normal: perpendicular to the wall's TRUE
    // tangent at the seam (the fitted walk's, which is what lead_in was
    // handed), not a chord across a corner. Whichever the first rim
    // move is - line or arc - its tangent at `from` is exact geometry.
    let li = led.moves.iter().position(|m| m.arc.is_some() && m.to.x == s.0 && m.to.y == s.1).unwrap();
    let next = &led.moves[li + 1];
    let tan = match next.arc {
        Some(a) => {
            let (rx, ry) = (next.from.x - a.center.x, next.from.y - a.center.y);
            if a.ccw { (-ry, rx) } else { (ry, -rx) }
        }
        None => (next.to.x - next.from.x, next.to.y - next.from.y),
    };
    let tn = tan.0.hypot(tan.1).max(1e-12);
    // (o - s) lies along the normal means it is PERPENDICULAR to the
    // tangent: the tangent-normalized dot vanishes (a cross here would
    // read R for a perfect join - the sign trap this line exists to
    // show).
    let along = ((o.x - s.0) * tan.0 + (o.y - s.1) * tan.1) / tn;
    assert!(along.abs() < 5e-6, "center lies on the normal: {along}");
    // And on the AIR side: farther from the region's centroid than the
    // rim is (the rect is centered at 10,10; the Outside law puts air
    // away from it).
    let d_c = |x: f64, y: f64| ((x - 10.0).powi(2) + (y - 10.0).powi(2)).sqrt();
    assert!(d_c(o.x, o.y) > d_c(s.0, s.1), "lead arcs bulge into air");
    // The sign law, stated machine-independently: arriving at s moving
    // along +t means the traversal turns from the plunge side - ccw iff
    // the air normal bears RIGHT of t (cross(air, t) < 0 with
    // air = o - s). air_side's own rotation convention is then verified
    // through the air test above, not assumed here.
    let u = (o.x - s.0, o.y - s.1); // = radius * air, unit it
    let un = (u.0 / 5.0, u.1 / 5.0);
    let want_ccw = (un.0 * tan.1 - un.1 * tan.0) / tn < 0.0;
    assert_eq!(land.arc.unwrap().ccw, want_ccw, "ccw law = cross(air,t)<0");
    let mut c = cp();
    c.lead = Lead::Tangential { radius: 5.0 };
    c.climb = false;
    let down = contour(&rect(0.0, 0.0, 20.0, 20.0), &c).unwrap();
    let land_d = down
        .moves
        .iter()
        .find(|m| m.arc.is_some() && m.to.x == plunge_xy(&down)[0].0)
        .expect("conventional lands too");
    assert_ne!(
        land_d.arc.unwrap().ccw,
        land.arc.unwrap().ccw,
        "conventional reverses the traversal, so the lead mirrors too"
    );
    // The exit arc: tangent to the walk's end at the seam, and - this
    // rim's seam sits on a corner's round join, where the walk's end
    // and start tangents differ - closing on the entry's plunge point,
    // so the two half arcs never cross out in the air (the seam law of
    // `contour_entry::seam_leads`). Where the tangents agree it is the
    // entry's exact mirror; here its radius stays within a hair of 5.
    let last = led
        .moves
        .iter()
        .rev()
        .find(|m| m.arc.is_some())
        .expect("a leave arc exists");
    assert!((last.to.x - e.0).abs() < 1e-6 && (last.to.y - e.1).abs() < 1e-6, "exit closes on the plunge point");
    let c = last.arc.unwrap().center;
    let rho = (last.from.x - c.x).hypot(last.from.y - c.y);
    assert!((rho - 5.0).abs() < 0.05, "exit radius {rho}");
    assert!(led.notes.iter().any(|n| n.contains("closes on the lead-in's start")), "{:?}", led.notes);
    assert!(led.is_chained() && led.all_finite() && down.is_chained());
}

#[test]
fn tabs_hold_the_part_until_the_floor() {
    let mut p = cp();
    p.top_z = 0.0;
    p.bottom_z = -6.0;
    p.stepdown = 3.0;
    p.tab_count = 4;
    p.tab_width = 6.0;
    p.tab_height = 2.0;
    let t = contour(&rect(0.0, 0.0, 20.0, 20.0), &p).expect("valid");
    // Levels 0 and -3 run full rings, -6 runs four subpaths between tabs.
    assert_eq!(plunge_xy(&t).len(), 6);
    // Hop motion only at the tabbed level: three retract feeds + rapids.
    let up = t
        .moves
        .iter()
        .filter(|m| m.kind == MoveKind::Feed && m.from.z != m.to.z)
        .count();
    assert_eq!(up, 3, "retracts are the only z-varying feeds");
    let low_rapids = t
        .moves
        .iter()
        .filter(|m| m.kind == MoveKind::Rapid && m.from.z == 3.0 && m.to.z == 3.0)
        .count();
    assert_eq!(low_rapids, 3);
    // The tabbed level cuts four tab widths less than a full ring.
    // Tab windows are placed along the ring's polyline parameter, so a
    // window boundary that falls inside a fitted corner arc removes an
    // arc length off 6.0 by the chord-to-arc drift of one tessellation
    // step - sub-micron, and the four boundaries share the bias.
    let full = level_len(&t, -3.0);
    let tabbed = level_len(&t, -6.0);
    assert!((full - tabbed - 24.0).abs() < 1e-3, "{full} - {tabbed}");
    assert!(t.is_chained() && t.all_finite());
}

#[test]
fn contour_refuses_bad_input() {
    let sq = rect(0.0, 0.0, 20.0, 20.0);
    let cases = [
        {
            let mut p = cp();
            p.stock_to_leave = -0.1;
            p
        },
        {
            let mut p = cp();
            p.stepdown = 0.0;
            p
        },
        {
            let mut p = cp();
            p.bottom_z = 1.0; // above top_z = 0.0
            p
        },
        {
            let mut p = cp();
            p.clearance_z = -1.0; // below the first cut level
            p
        },
        {
            let mut p = cp();
            p.lead = Lead::Arc { radius: 0.0 };
            p
        },
        {
            let mut p = cp();
            p.lead = Lead::Tangential { radius: 0.0 };
            p
        },
        {
            let mut p = cp();
            p.tab_count = 2;
            p.tab_width = 0.0;
            p
        },
        {
            let mut p = cp();
            p.feed = f64::NAN;
            p
        },
        {
            // The contour helix door: positivity only. machining3d's
            // <= -tool-radius law is the roughing overlap law; a
            // contour orbit descends an already-open bore and its fit
            // is certified per level by dive_room (M8.2 A).
            let mut p = cp();
            p.entry = Entry3D::Helix;
            p.helix_radius = 0.0;
            p
        },
    ];
    for p in cases {
        assert!(matches!(contour(&sq, &p), Err(GenError::Invalid(_))), "{p:?}");
    }
    assert!(matches!(
        contour(&Region::empty(), &cp()),
        Err(GenError::Empty(_))
    ));
}

#[test]
fn an_open_chain_run_refuses_the_entry_family_by_name() {
    // No area, no air to certify against: the chain keeps its dive at
    // the run start and says the refusal once - the moves do not change.
    let pts = [
        Point2::new(0.0, 0.0),
        Point2::new(20.0, 0.0),
        Point2::new(30.0, 8.0),
    ];
    let base = contour_along_chain(&pts, &cp()).expect("chain valid");
    let mut p = cp();
    p.entry = Entry3D::Helix;
    p.helix_radius = 3.0;
    let t = contour_along_chain(&pts, &p).expect("chain valid");
    assert_eq!(t.moves, base.moves, "the refusal must not move the path");
    assert_eq!(
        t.notes.iter().filter(|n| n.contains("open centerline run")).count(),
        1,
        "said once, named: {:?}",
        t.notes
    );
    assert!(base.notes.is_empty(), "Plunge default says nothing: {:?}", base.notes);
    assert!(t.is_chained() && t.all_finite());
}

#[test]
fn the_inside_contour_dives_in_the_bore_floor() {
    // M8.2 A, level-0 voice: the eroded region states the bore's bounded
    // air, so the family is obeyed on the FIRST entry too - the Plunge
    // stands in the floor's room, never dwells on a finished-wall vertex.
    let mut p = cp();
    p.side = Side::Inside;
    let t = contour(&rect(0.0, 0.0, 30.0, 30.0), &p).expect("inside contour");
    let dives: Vec<_> = t
        .moves
        .iter()
        .filter(|m| m.kind == MoveKind::Plunge)
        .collect();
    assert_eq!(dives.len(), 1, "one certified dive for one level: {:?}", t.moves);
    let d = &dives[0];
    assert!(
        d.to.x > 5.0 && d.to.x < 25.0 && d.to.y > 5.0 && d.to.y < 25.0,
        "the dive must stand inside the r = 5 inset, not on its wall: {d:?}"
    );
    assert!(t.notes.is_empty(), "room was found; nothing to say: {:?}", t.notes);
    assert!(t.is_chained() && t.all_finite());
}

#[test]
fn the_inside_contour_bore_opens_with_a_helix() {
    // The helix's own arcs reach the surface plane - no wall dive
    // anywhere - and the orbit's fit is the widened-disc test, not the
    // roughing overlap law (a contour orbit cuts nothing on the way in).
    let mut p = cp();
    p.side = Side::Inside;
    p.entry = Entry3D::Helix;
    p.helix_radius = 3.0;
    let t = contour(&rect(0.0, 0.0, 30.0, 30.0), &p).expect("helix bore valid");
    assert!(
        !t.moves.iter().any(|m| m.kind == MoveKind::Plunge),
        "a helix enters with no axial dive: {:?}",
        t.moves
    );
    let reach = t
        .moves
        .iter()
        .find(|m| m.to.z.abs() < 1e-9 && m.kind != MoveKind::Rapid)
        .expect("something reaches the cut plane");
    assert_eq!(reach.kind, MoveKind::Feed, "the plane is entered in arcs: {reach:?}");
    assert!(reach.arc.is_some(), "and the arc is the helix's own");
    assert!(t.notes.is_empty(), "the bore holds the orbit: {:?}", t.notes);
    assert!(t.is_chained() && t.all_finite());
}

#[test]
fn a_wall_side_entry_is_refused_by_name_grazing_unchanged() {
    // The grown region contains the part's own interior, so a chord or a
    // dive certified on it would lie - on the wall side the arms are
    // refused by name, once, and the graze this generator has always
    // posted stands. Byte-for-byte: the refusal costs no move.
    let base = contour(&rect(0.0, 0.0, 30.0, 30.0), &cp()).expect("plunge graze");
    for (arm, e) in [("ramp", Entry3D::Ramp), ("helix", Entry3D::Helix)] {
        let mut p = cp();
        p.side = Side::Outside;
        p.entry = e;
        p.helix_radius = 3.0;
        let t = contour(&rect(0.0, 0.0, 30.0, 30.0), &p).expect("wall contour valid");
        assert_eq!(t.moves, base.moves, "the {arm} must not move the wall-side path");
        assert_eq!(
            t.notes.iter().filter(|n| n.contains("needs bounded air")).count(),
            1,
            "the {arm} refusal is said exactly once: {:?}",
            t.notes
        );
    }
}

#[test]
fn a_blocked_bore_entry_substitutes_and_says_it() {
    // Prime Directive voice: an arm whose certification fails falls back
    // to the axial dive AT the lead-in and the note names the reason.
    // Ramp: home XY sits outside the bore, so its chord is unproven.
    let mut p = cp();
    p.side = Side::Inside;
    p.entry = Entry3D::Ramp;
    let t = contour(&rect(0.0, 0.0, 30.0, 30.0), &p).expect("ramp valid");
    assert!(
        t.notes.iter().any(|n| n.contains("ramp's chord is blocked")),
        "the level-0 substitution must be said, got {:?}",
        t.notes
    );
    assert!(t.is_chained() && t.all_finite());
    // Helix: an orbit wider than the room left inside the inset cannot
    // fit - the widened disc says so and the dive stands in.
    let mut p = cp();
    p.side = Side::Inside;
    p.entry = Entry3D::Helix;
    p.helix_radius = 10.0; // inset is 20 wide: no centre clears r + 1e-3
    let t = contour(&rect(0.0, 0.0, 30.0, 30.0), &p).expect("fat helix valid");
    assert!(
        t.notes.iter().any(|n| n.contains("widened disc fits nowhere")),
        "the no-fit helix must be said, got {:?}",
        t.notes
    );
    assert!(t.is_chained() && t.all_finite());
}

#[test]
fn contour_is_deterministic() {
    let r = rect(-3.5, 1.25, 27.0, 19.5);
    let a = contour(&r, &cp()).unwrap();
    let b = contour(&r, &cp()).unwrap();
    assert_eq!(a, b);
    assert!(a.is_chained() && a.all_finite());
}

// ---- pocket -------------------------------------------------------------

fn pp() -> PocketParams {
    PocketParams {
        blend_radius: None,
        links: camcraft_motion::links::Links::PLANAR,
        tool_diameter: 10.0,
        stepover: 5.0,
        stock_to_leave: 0.0,
        clearance_z: 5.0,
        retract_z: None,
        top_z: 0.0,
        bottom_z: -6.0,
        stepdown: 3.0,
        feed: 500.0,
        plunge_feed: 100.0,
        climb: true,
        strategy: PocketStrategy::Zigzag,
        // The M5.1 H defaults: the dive every pocket posted before the
        // entry family existed, a helix radius the door would accept,
        // and the law's 45° reading only under Adaptive.
        entry: Entry3D::Plunge,
        helix_radius: 1.0,
        optimal_load: camcraft_motion::load::DEFAULT_OPTIMAL_LOAD,
        blend_turns: false,
    }
}

fn in_closed(r: &Region, q: (f64, f64)) -> bool {
    let p = Point2::new(q.0, q.1);
    r.contains(p) || on_boundary(r, p)
}

/// The level's cutter-center polyline in path order, with the flag saying
/// whether each point was reached by a plunge (a subpath start after a
/// hop) rather than by feeding from the previous point. Area and engrave
/// paths only (pocket, slot, trace): the one non-feed move at cut depth
/// they make is an entry plunge.
fn polyline_at(t: &Toolpath, z: f64) -> (Vec<(f64, f64)>, Vec<bool>) {
    let mut pts = Vec::new();
    let mut lifted = Vec::new();
    for m in &t.moves {
        match m.kind {
            MoveKind::Feed if m.from.z == z && m.to.z == z => {
                if pts.is_empty() {
                    pts.push((m.from.x, m.from.y));
                    lifted.push(true);
                }
                // An arc is walked as the chords the machine can read,
                // never as the long chord across it.
                for (_, b) in m.chords(FLATTEN_TOL) {
                    pts.push((b.x, b.y));
                    lifted.push(false);
                }
            }
            MoveKind::Plunge if m.to.z == z => {
                pts.push((m.to.x, m.to.y));
                lifted.push(true);
            }
            _ => {}
        }
    }
    (pts, lifted)
}

fn levels_3_zs() -> [f64; 3] {
    [0.0, -3.0, -6.0]
}

#[test]
fn zigzag_fills_a_square_with_one_plunge_per_level() {
    let region = rect(0.0, 0.0, 30.0, 30.0);
    let center = region.offset(-5.0).expect("offset");
    let t = pocket(&region, &pp()).expect("valid");
    assert!(t.is_chained() && t.all_finite());
    // M8.1's entry law on zigzag, from level 0: the serpentine keeps
    // its rows, but every level - the first included (Plunge never
    // lands its dive on a wall corner, not even the stock's own
    // surface corner) - first dives at the floor's room, then links
    // to the row start - a chord across the rows' lines, so the
    // hop-and-dive link. 3 levels: 2 * 3. M11.3's one-entry law: the
    // dive lands on the first row a little past its wall-corner start
    // and links back along the row at depth - one dive per level.
    assert_eq!(plunge_xy(&t).len(), 3, "one entry dive at every level");
    let floor = t
        .moves
        .iter()
        .filter(|m| m.kind == MoveKind::Plunge && (m.to.z - 0.0).abs() > 1e-9)
        .nth(1)
        .expect("a level-change dive");
    assert!(
        center.contains(Point2::new(floor.to.x, floor.to.y)),
        "the level-change dive at ({}, {}) is not inside the center area",
        floor.to.x,
        floor.to.y
    );

    let pts = level_xy(&t, -6.0);
    let ys: Vec<f64> = {
        let mut v: Vec<f64> = pts.iter().map(|q| q.1).collect();
        v.sort_by(f64::total_cmp);
        v.dedup();
        // M11.3: the entry dive lands just inside the first row (a wall
        // margin off it) and links back to its start - not a row.
        v.retain(|y| (y - y.round()).abs() < 1e-9);
        v
    };
    assert_eq!(ys, vec![5.0, 10.0, 15.0, 20.0, 25.0]);
    for q in &pts {
        assert!(in_closed(&center, *q), "feed point {q:?} outside the center area");
    }
    // Five 20 mm rows plus four 5 mm connectors, all at cut depth - and
    // (M11.3) the short at-depth link from the entry onto the first row.
    let len = level_len(&t, -6.0);
    assert!((120.0 - 1e-9..120.0 + 2.0).contains(&len), "{len}");
}

#[test]
fn zigzag_hops_over_islands_and_leaves_them_standing() {
    let region = Region::new(vec![
        Loop::new(vec![
            Point2::new(0.0, 0.0),
            Point2::new(40.0, 0.0),
            Point2::new(40.0, 40.0),
            Point2::new(0.0, 40.0),
        ])
        .unwrap(),
        Loop::new(vec![
            Point2::new(15.0, 15.0),
            Point2::new(25.0, 15.0),
            Point2::new(25.0, 25.0),
            Point2::new(15.0, 25.0),
        ])
        .unwrap(),
    ])
    .unwrap();
    // Where the tool center may not go because of the island (rounded
    // corners): `center` is the exact grown area, so `in_closed` below is
    // the gouge check; the distance test is a loose physical cross-check.
    let center = region.offset(-5.0).expect("offset");
    let t = pocket(&region, &pp()).expect("valid");
    // M8.1 read from level 0: every level posts five dives - the
    // interior entry dive whose link to the row start the island
    // forces to a hop, plus three island-crossing hops. 5 + 5 + 5.
    // M11.3: the entry lands on the first row and links along it, so the
    // entry's second dive is gone - 4 per level.
    assert_eq!(plunge_xy(&t).len(), 12);

    for z in levels_3_zs() {
        let (pts, lifted) = polyline_at(&t, z);
        for q in &pts {
            assert!(in_closed(&center, *q), "point {q:?} outside the center area");
            // Distance from the point to the island rect [15,25]^2.
            let dx = (15.0 - q.0).max(q.0 - 25.0).max(0.0);
            let dy = (15.0 - q.1).max(q.1 - 25.0).max(0.0);
            assert!((dx * dx + dy * dy).sqrt() > 5.0 - 1e-3, "point {q:?} in the island");
        }
        // Consecutive centers never cross the island while feeding: every
        // chord either provably clears the center area or is a subpath
        // start the tool plunged into.
        for i in 1..pts.len() {
            let a = Point2::new(pts[i - 1].0, pts[i - 1].1);
            let b = Point2::new(pts[i].0, pts[i].1);
            if !chord_clears(&center, a, b) {
                assert!(lifted[i], "gouging feed chord {:?} -> {:?}", pts[i - 1], pts[i]);
            }
        }
    }
}

#[test]
fn pocket_stock_to_leave_insets_the_whole_path() {
    let mut p = pp();
    p.stock_to_leave = 2.0;
    let region = rect(0.0, 0.0, 30.0, 30.0);
    let t = pocket(&region, &p).expect("valid");
    let pts = level_xy(&t, -6.0);
    let min = |v: &[f64]| v.iter().cloned().fold(f64::INFINITY, f64::min);
    let max = |v: &[f64]| v.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
    let xs: Vec<f64> = pts.iter().map(|q| q.0).collect();
    let ys: Vec<f64> = pts.iter().map(|q| q.1).collect();
    assert_eq!((min(&xs), max(&xs), min(&ys), max(&ys)), (7.0, 23.0, 7.0, 23.0));
}

#[test]
fn waterline_cuts_concentric_rings_until_collapse() {
    let mut p = pp();
    p.strategy = PocketStrategy::Waterline;
    p.tool_diameter = 8.0;
    p.stepover = 2.0;
    p.top_z = 0.0;
    p.bottom_z = 0.0;
    let region = rect(0.0, 0.0, 30.0, 30.0);
    let center = region.offset(-4.0).expect("offset");
    let t = pocket(&region, &p).expect("valid");
    assert!(t.is_chained() && t.all_finite());
    let (pts, lifted) = polyline_at(&t, 0.0);
    for q in &pts {
        assert!(in_closed(&center, *q), "ring point {q:?} outside the center area");
    }
    for i in 1..pts.len() {
        let a = Point2::new(pts[i - 1].0, pts[i - 1].1);
        let b = Point2::new(pts[i].0, pts[i].1);
        if !chord_clears(&center, a, b) {
            assert!(lifted[i], "gouging ring chord {:?} -> {:?}", pts[i - 1], pts[i]);
        }
    }
    // Ring k is [4+2k, 26-2k]^2 for k = 0..5; k = 6 is empty.
    let min = |v: &[f64]| v.iter().cloned().fold(f64::INFINITY, f64::min);
    let max = |v: &[f64]| v.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
    let xs: Vec<f64> = pts.iter().map(|q| q.0).collect();
    let ys: Vec<f64> = pts.iter().map(|q| q.1).collect();
    assert_eq!((min(&xs), max(&xs), min(&ys), max(&ys)), (4.0, 26.0, 4.0, 26.0));
    // The innermost ring: the 2 mm box [14,16]^2 was reached and closed.
    assert!(pts.iter().any(|q| (q.0 - 14.0).abs() < 1e-9 && (q.1 - 16.0).abs() < 1e-9));
    let n_rings = plunge_xy(&t).len();
    assert!((1..=6).contains(&n_rings), "rings: {n_rings}");
}

#[test]
fn waterline_stays_clear_of_an_island() {
    let region = Region::new(vec![
        Loop::new(vec![
            Point2::new(0.0, 0.0),
            Point2::new(40.0, 0.0),
            Point2::new(40.0, 40.0),
            Point2::new(0.0, 40.0),
        ])
        .unwrap(),
        Loop::new(vec![
            Point2::new(15.0, 15.0),
            Point2::new(25.0, 15.0),
            Point2::new(25.0, 25.0),
            Point2::new(15.0, 25.0),
        ])
        .unwrap(),
    ])
    .unwrap();
    let mut p = pp();
    p.strategy = PocketStrategy::Waterline;
    p.stepover = 2.5;
    // The test's own chord flattening of a fitted arc cuts a sagitta
    // INSIDE the arc - the ring's true geometry stays on the exact
    // center boundary, but its flattened image dips FLATTEN_TOL into
    // the hole, where the exact polygon would report a gouge. So the
    // clearance question is measured against the center grown by twice
    // that sagitta; a real gouge (a ring reaching INTO the island) is
    // millimeters, and still fails this by a mile.
    let center = region.offset(-5.0 + 2.0 * FLATTEN_TOL).expect("offset");
    let t = pocket(&region, &p).expect("valid");
    for z in levels_3_zs() {
        let (pts, lifted) = polyline_at(&t, z);
        for q in &pts {
            assert!(in_closed(&center, *q), "ring point {q:?} outside the center area");
            let dx = (15.0 - q.0).max(q.0 - 25.0).max(0.0);
            let dy = (15.0 - q.1).max(q.1 - 25.0).max(0.0);
            assert!((dx * dx + dy * dy).sqrt() > 5.0 - 2.0 * FLATTEN_TOL, "point {q:?} in the island");
        }
        for i in 1..pts.len() {
            let a = Point2::new(pts[i - 1].0, pts[i - 1].1);
            let b = Point2::new(pts[i].0, pts[i].1);
            if !chord_clears(&center, a, b) {
                assert!(lifted[i], "gouging ring chord {:?} -> {:?}", pts[i - 1], pts[i]);
            }
        }
    }
}

#[test]
fn pocket_refuses_bad_input() {
    let sq = rect(0.0, 0.0, 20.0, 20.0);
    let cases = [
        {
            let mut p = pp();
            p.stepover = 0.0;
            p
        },
        {
            let mut p = pp();
            p.tool_diameter = f64::NAN;
            p
        },
        {
            let mut p = pp();
            p.stepover = 11.0; // beyond tool reach for zigzag
            p
        },
        {
            let mut p = pp();
            p.feed = 0.0;
            p
        },
        {
            let mut p = pp();
            p.stock_to_leave = -1.0;
            p
        },
        {
            let mut p = pp();
            p.bottom_z = 1.0; // bottom above top
            p
        },
        {
            let mut p = pp();
            p.clearance_z = -1.0; // under the first cut level
            p
        },
        {
            let mut p = pp();
            p.stepdown = 0.0;
            p
        },
        {
            // Waterline's tighter coverage bound: inside zigzag's limit.
            let mut p = pp();
            p.strategy = PocketStrategy::Waterline;
            p.tool_diameter = 8.0;
            p.stepover = 5.0; // > r = 4
            p
        },
        {
            // The entry family's doors, same words as the 3-D ladder.
            let mut p = pp();
            p.entry = Entry3D::Helix;
            p.helix_radius = 0.0;
            p
        },
        {
            let mut p = pp();
            p.entry = Entry3D::Helix;
            p.helix_radius = 6.0; // over r = 5: turns would not overlap
            p
        },
        {
            let mut p = pp();
            p.helix_radius = f64::NAN; // unread by Plunge, still must be finite
            p
        },
        {
            let mut p = pp();
            p.strategy = PocketStrategy::Adaptive;
            p.optimal_load = 0.0; // a zero step is an infinite ladder
            p
        },
        {
            let mut p = pp();
            p.strategy = PocketStrategy::Adaptive;
            p.optimal_load = 0.51; // past where consecutive rings overlap
            p
        },
    ];
    for p in cases {
        assert!(matches!(pocket(&sq, &p), Err(GenError::Invalid(_))), "{p:?}");
    }
    // The same numbers zigzag accepts.
    let mut ok = pp();
    ok.tool_diameter = 8.0;
    ok.stepover = 5.0;
    assert!(pocket(&sq, &ok).is_ok());
    // Adaptive spaces its rings at the law's step, so a stepover past
    // Waterline's radius bound is ignored, not refused (M5.1 H): the
    // reach door asks `tiling_step`, and r(1 - cos 45) is well inside.
    let mut law = pp();
    law.strategy = PocketStrategy::Adaptive;
    law.stepover = 9.0; // > r = 5, read only by the other strategies
    assert!(pocket(&sq, &law).is_ok());
    // Narrower than the tool: material no legal position can reach.
    assert!(matches!(
        pocket(&rect(0.0, 0.0, 8.0, 8.0), &pp()),
        Err(GenError::Invalid(_))
    ));
    assert!(matches!(
        pocket(&Region::empty(), &pp()),
        Err(GenError::Empty(_))
    ));
}

#[test]
fn pocket_is_deterministic() {
    let r = Region::new(vec![
        Loop::new(vec![
            Point2::new(0.0, 0.0),
            Point2::new(40.0, 0.0),
            Point2::new(40.0, 40.0),
            Point2::new(0.0, 40.0),
        ])
        .unwrap(),
        Loop::new(vec![
            Point2::new(15.0, 15.0),
            Point2::new(25.0, 15.0),
            Point2::new(25.0, 25.0),
            Point2::new(15.0, 25.0),
        ])
        .unwrap(),
    ])
    .unwrap();
    for s in [PocketStrategy::Zigzag, PocketStrategy::Waterline] {
        let mut p = pp();
        p.strategy = s;
        if s == PocketStrategy::Waterline {
            p.stepover = 2.0;
        }
        let a = pocket(&r, &p).unwrap();
        let b = pocket(&r, &p).unwrap();
        assert_eq!(a, b, "{s:?}");
    }
}

/// A regular n-gon approximating a circle of radius `rad`, CCW, centered
/// on the origin - the ring Auto must recognize as round.
fn ring_circle(rad: f64, n: usize) -> Vec<Point2> {
    (0..n)
        .map(|i| {
            let t = 2.0 * std::f64::consts::PI * (i as f64) / (n as f64);
            Point2::new(rad * t.cos(), rad * t.sin())
        })
        .collect()
}

/// Waterline rings earn a hop only where the rings cannot join: an
/// island splits each offset level into loops whose connecting chords
/// cross the island void, so `chord_clears` refuses them and each loop
/// starts its own pass. Passes of ONE level then lift only to the
/// retract plane; the first pass of a level still enters from clearance
/// (it crosses in over the top of the material), and a level change
/// always clears the whole stack. Island-free waterline chains its rings
/// at depth with diagonal stepover feeds - there is nothing to hop, and
/// `zigzag_and_levels_still_lift_to_clearance` covers the transitions.
#[test]
fn waterline_rings_split_by_an_island_hop_below_clearance() {
    let mut p = pp();
    p.strategy = PocketStrategy::Waterline;
    p.tool_diameter = 8.0;
    p.stepover = 2.0;
    p.top_z = 0.0;
    p.bottom_z = -3.0;
    p.stepdown = 3.0; // two levels: 0 and -3
    p.clearance_z = 5.0;
    p.retract_z = Some(2.0); // hop plane, below clearance, above the cut
    // The traverse-plane law itself (M11.3's plunge height 0 keeps it).
    p.links.approach.plunge_height = 0.0;
    let region = Region::new(vec![
        Loop::new(vec![
            Point2::new(0.0, 0.0),
            Point2::new(40.0, 0.0),
            Point2::new(40.0, 40.0),
            Point2::new(0.0, 40.0),
        ])
        .unwrap(),
        Loop::new(vec![
            Point2::new(15.0, 15.0),
            Point2::new(25.0, 15.0),
            Point2::new(25.0, 25.0),
            Point2::new(15.0, 25.0),
        ])
        .unwrap(),
    ])
    .unwrap();
    let t = pocket(&region, &p).expect("valid");
    assert!(t.is_chained() && t.all_finite());
    let plunges: Vec<(f64, f64)> = t
        .moves
        .iter()
        .filter(|m| m.kind == MoveKind::Plunge)
        .map(|m| (m.from.z, m.to.z))
        .collect();
    assert!(plunges.len() >= 4, "the island must split several passes");
    assert!(
        (plunges[0].0 - p.clearance_z).abs() < 1e-9,
        "the first entry crosses in at clearance"
    );
    // The whole law, per consecutive pair: same level -> the hop plane;
    // new level -> the full clearance climb. Nothing else ever appears.
    let hop = p.retract_z.unwrap_or(p.clearance_z).min(p.clearance_z);
    for pair in plunges.windows(2) {
        let same_level = (pair[0].1 - pair[1].1).abs() < 1e-9;
        let want = if same_level { hop } else { p.clearance_z };
        assert!(
            (pair[1].0 - want).abs() < 1e-9,
            "plunge at z {} lifted from {}, not {}",
            pair[1].1,
            pair[1].0,
            want
        );
    }
    assert!(
        plunges.iter().any(|p2| (p2.0 - 2.0).abs() < 1e-9),
        "the hop must actually engage on this shape"
    );
}

/// A level change engages material the previous level never touched, so
/// its FIRST bite must clear the whole stack no matter what retract
/// plane is set. M8.1 splits that law honestly: the level-change entry
/// dive (at the floor's room) comes down from clearance; the dive that
/// link then makes at the row start is a between-pass move - it, and
/// every in-level hop, use the retract plane, which validation keeps at
/// or above the stock top, so no hop ever slides through solid.
#[test]
fn zigzag_and_levels_still_lift_to_clearance() {
    let mut p = pp(); // zigzag, top 0, bottom -6, stepdown 3 -> 3 levels
    p.clearance_z = 5.0;
    p.retract_z = Some(2.0);
    // The traverse-plane law itself (M11.3's plunge height 0 keeps it).
    p.links.approach.plunge_height = 0.0;
    let region = rect(0.0, 0.0, 30.0, 30.0);
    let t = pocket(&region, &p).expect("valid");
    let hop = p.retract_z.unwrap_or(p.clearance_z).min(p.clearance_z);
    let mut first_zs: Vec<f64> = Vec::new();
    for m in &t.moves {
        if m.kind == MoveKind::Plunge {
            if !first_zs.contains(&m.to.z) {
                assert!(
                    (m.from.z - p.clearance_z).abs() < 1e-9,
                    "the first dive of level {} came from {}, not clearance",
                    m.to.z,
                    m.from.z
                );
                first_zs.push(m.to.z);
            } else {
                assert!(
                    (m.from.z - hop).abs() < 1e-9 || (m.from.z - p.clearance_z).abs() < 1e-9,
                    "a zigzag level plunged from {} - between-pass dives ride the hop",
                    m.from.z
                );
            }
        }
    }
    assert_eq!(first_zs.len(), 3, "each of the three levels engages once");
}

/// A retract plane inside the material would make a between-pass hop
/// slide through solid - a gouge. Refused by name, for every kind that
/// owns the field.
#[test]
fn retract_inside_material_refuses() {
    let region = rect(0.0, 0.0, 30.0, 30.0);
    {
        let mut p = pp();
        p.retract_z = Some(-1.0); // below top_z 0.0
        assert!(matches!(pocket(&region, &p), Err(GenError::Invalid(r)) if r.contains("retract")));
    }
    {
        let mut c = cp();
        c.retract_z = -1.0;
        assert!(matches!(contour(&region, &c), Err(GenError::Invalid(r)) if r.contains("retract")));
    }
    {
        let mut s = sp();
        s.retract_z = Some(-1.0);
        assert!(
            matches!(slot(Point2::new(-10.0, 0.0), Point2::new(10.0, 0.0), &s), Err(GenError::Invalid(r)) if r.contains("retract"))
        );
    }
    {
        let mut tr = tp();
        tr.retract_z = Some(-1.0);
        assert!(matches!(trace(&region, &tr), Err(GenError::Invalid(r)) if r.contains("retract")));
    }
}

/// The clamp: a legacy file may park retract above clearance. Refusing
/// would brick it, so the effective hop is `min(retract, clearance)` -
/// which makes the path byte-identical to the no-retract (hop-to-
/// clearance) shape. The panel echo that names the clamp is a UI test.
#[test]
fn legacy_retract_above_clearance_clamps_and_says_so() {
    let region = rect(0.0, 0.0, 30.0, 30.0);
    let mut a = pp();
    a.strategy = PocketStrategy::Waterline;
    a.tool_diameter = 8.0;
    a.stepover = 2.0;
    a.top_z = 0.0;
    a.bottom_z = 0.0;
    a.clearance_z = 5.0;
    a.retract_z = Some(50.0); // absurd: retract far above clearance
    let mut b = a;
    b.retract_z = None; // the hop-to-clearance every pre-step-5 file did
    assert_eq!(
        pocket(&region, &a).expect("a"),
        pocket(&region, &b).expect("b"),
        "retract above clearance clamps to clearance, path unchanged"
    );
}

/// Auto reads the pocket's own outline: a round boundary whose fitted
/// arcs agree on one center and whose stepover stays inside a tool
/// radius becomes Waterline; a square, whose fitted runs are straight,
/// stays Zigzag.
#[test]
fn auto_picks_waterline_for_a_circular_region() {
    // 192 segments on a Ø40 rim keep every chord's sagitta inside
    // ARC_FIT_TOL, so the fitter can honour the circle it is shown. A
    // coarser tessellation (last assert) is NOT round at that tolerance
    // - Auto falls back to Zigzag rather than claim rings the chords
    // would cut outside of.
    let circle = ring_circle(20.0, 192);
    assert!(ring_is_circular(&circle, ARC_FIT_TOL, 5.0), "the circle must read as round");
    let chosen = PocketStrategy::Auto.resolve(&circle, 5.0, 10.0, ARC_FIT_TOL);
    assert_eq!(chosen, PocketStrategy::Waterline, "round + reachable -> waterline");
    // An unreachable stepover demotes to zigzag even on a round ring:
    // rings a whole stepover apart would leave a band no ring edge cuts.
    let demoted = PocketStrategy::Auto.resolve(&circle, 8.0, 10.0, ARC_FIT_TOL);
    assert_eq!(demoted, PocketStrategy::Zigzag, "stepover past reach -> zigzag");
    assert!(
        !ring_is_circular(&ring_circle(20.0, 48), ARC_FIT_TOL, 5.0),
        "coarse facets are not a round rim at this tolerance"
    );
}

#[test]
fn rectangular_pocket_stays_zigzag_under_auto() {
    let square = vec![
        Point2::new(0.0, 0.0),
        Point2::new(40.0, 0.0),
        Point2::new(40.0, 40.0),
        Point2::new(0.0, 40.0),
    ];
    assert!(!ring_is_circular(&square, ARC_FIT_TOL, 5.0), "a square is not round");
    let chosen = PocketStrategy::Auto.resolve(&square, 5.0, 10.0, ARC_FIT_TOL);
    assert_eq!(chosen, PocketStrategy::Zigzag);
}

// ---- slot and trace -----------------------------------------------------

fn sp() -> SlotParams {
    SlotParams {
        links: camcraft_motion::links::Links::PLANAR,
        tool_diameter: 10.0,
        width: 10.0,
        stock_to_leave: 0.0,
        clearance_z: 5.0,
        retract_z: None,
        top_z: 0.0,
        bottom_z: -6.0,
        stepdown: 3.0,
        stepover: 5.0,
        feed: 500.0,
        plunge_feed: 100.0,
        climb: true,
        entry: Entry3D::Plunge,
        helix_radius: 1.0,
    }
}

fn tp() -> TraceParams {
    TraceParams {
        clearance_z: 5.0,
        retract_z: None,
        top_z: 0.0,
        bottom_z: -2.0,
        stepdown: 1.0,
        feed: 500.0,
        plunge_feed: 100.0,
        climb: true,
    }
}

/// Distance from p to the segment a -> b: the stadium membership test (a
/// point is in the stadium of half-width h iff it is within h of the axis
/// segment).
fn seg_dist(p: (f64, f64), a: (f64, f64), b: (f64, f64)) -> f64 {
    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
    let dd = dx * dx + dy * dy;
    let t = if dd == 0.0 {
        0.0
    } else {
        (((p.0 - a.0) * dx + (p.1 - a.1) * dy) / dd).clamp(0.0, 1.0)
    };
    (p.0 - a.0 - t * dx).hypot(p.1 - a.1 - t * dy)
}

#[test]
fn slot_the_tool_fills_the_channel_and_runs_the_centerline() {
    let (a, b) = (Point2::new(0.0, 0.0), Point2::new(40.0, 0.0));
    let t = slot(a, b, &sp()).expect("valid"); // h = 0: one row
    assert!(t.is_chained() && t.all_finite());
    for &z in &levels_3_zs() {
        let (pts, lifted) = polyline_at(&t, z);
        assert_eq!(pts, vec![(0.0, 0.0), (40.0, 0.0)]);
        assert!(lifted[0] && lifted[1..].iter().all(|&l| !l));
        assert!((level_len(&t, z) - 40.0).abs() < 1e-9);
    }
    assert_eq!(plunge_xy(&t).len(), 3, "one plunge per level, no in-cut lift");
    for m in t.moves.iter().filter(|m| m.kind == MoveKind::Rapid) {
        assert!(m.from.z == 5.0 || m.to.z == 5.0, "rapid inside the cut: {m:?}");
    }
}

#[test]
fn slot_offsets_rows_across_a_wide_channel() {
    let (a, b) = (Point2::new(0.0, 0.0), Point2::new(40.0, 0.0));
    let p = SlotParams {
        tool_diameter: 6.0,
        width: 20.0,
        stepover: 4.0,
        bottom_z: 0.0,
        ..sp()
    };
    let t = slot(a, b, &p).expect("valid"); // h = 7: rows -7, -3, 1, 5, 7
    let (s40, s48, s24) = (40f64.sqrt(), 48f64.sqrt(), 24f64.sqrt());
    let want = vec![
        (0.0, -7.0),
        (40.0, -7.0),
        (40.0 + s40, -3.0),
        (-s40, -3.0),
        (-s48, 1.0),
        (40.0 + s48, 1.0),
        (40.0 + s24, 5.0),
        (-s24, 5.0),
        (0.0, 7.0),
        (40.0, 7.0),
    ];
    let (pts, _) = polyline_at(&t, 0.0);
    assert_eq!(pts, want, "serpentine rows, each reaching its round end by sqrt(h^2 - d^2)");
    assert_eq!(
        t.moves.iter().filter(|m| m.kind == MoveKind::Feed).count(),
        want.len() - 1,
        "joins are feeds: the center area is convex"
    );
    assert_eq!(plunge_xy(&t).len(), 1);
    for q in &pts {
        assert!(
            seg_dist(*q, (0.0, 0.0), (40.0, 0.0)) <= 7.0 + 1e-9,
            "center left the area: {q:?}"
        );
    }
    let max_x = pts.iter().map(|q| q.0).fold(f64::NEG_INFINITY, f64::max);
    let min_x = pts.iter().map(|q| q.0).fold(f64::INFINITY, f64::min);
    assert_eq!(max_x, 40.0 + s48);
    assert_eq!(min_x, -s48);
}

#[test]
fn conventional_slot_starts_at_the_far_end() {
    let p = SlotParams {
        tool_diameter: 6.0,
        width: 20.0,
        stepover: 4.0,
        bottom_z: 0.0,
        climb: false,
        ..sp()
    };
    let t = slot(Point2::new(0.0, 0.0), Point2::new(40.0, 0.0), &p).unwrap();
    let (pts, _) = polyline_at(&t, 0.0);
    assert_eq!(pts[0], (40.0, -7.0), "conventional enters at +x");
    assert_eq!(pts[1], (0.0, -7.0));
    let mut ys: Vec<f64> = pts.iter().map(|q| q.1).collect();
    ys.sort_by(f64::total_cmp);
    ys.dedup();
    assert_eq!(ys, vec![-7.0, -3.0, 1.0, 5.0, 7.0], "same rows, reversed travel");
}

#[test]
fn slot_stock_to_leave_narrows_the_center_area() {
    let p = SlotParams {
        tool_diameter: 6.0,
        width: 20.0,
        stock_to_leave: 2.0,
        stepover: 2.0,
        bottom_z: 0.0,
        ..sp()
    };
    let t = slot(Point2::new(0.0, 0.0), Point2::new(40.0, 0.0), &p).unwrap();
    let (pts, _) = polyline_at(&t, 0.0);
    // h = 10 - 3 - 2: rows reach +-5 lateral, sqrt(24) past the ends.
    for q in &pts {
        assert!(
            seg_dist(*q, (0.0, 0.0), (40.0, 0.0)) <= 5.0 + 1e-9,
            "cut into the allowance: {q:?}"
        );
    }
    let mut ys: Vec<f64> = pts.iter().map(|q| q.1).collect();
    ys.sort_by(f64::total_cmp);
    ys.dedup();
    assert_eq!(ys, vec![-5.0, -3.0, -1.0, 1.0, 3.0, 5.0]);
    let max_x = pts.iter().map(|q| q.0).fold(f64::NEG_INFINITY, f64::max);
    assert_eq!(max_x, 40.0 + 24f64.sqrt());
}

#[test]
fn slot_refuses_bad_input() {
    let (a, b) = (Point2::new(0.0, 0.0), Point2::new(40.0, 0.0));
    let cases: Vec<(&str, SlotParams)> = vec![
        ("tool wider than the slot", SlotParams { tool_diameter: 12.0, ..sp() }),
        ("stepover beyond the tool", SlotParams { stepover: 11.0, ..sp() }),
        ("zero width", SlotParams { width: 0.0, ..sp() }),
        ("non-finite width", SlotParams { width: f64::NAN, ..sp() }),
        ("leave eats the slot", SlotParams { stock_to_leave: 6.0, ..sp() }),
        ("negative leave", SlotParams { stock_to_leave: -1.0, ..sp() }),
        ("top below bottom", SlotParams { bottom_z: 1.0, ..sp() }),
        ("clearance below the cut", SlotParams { clearance_z: -2.0, ..sp() }),
        ("retract inside the material", SlotParams { retract_z: Some(-1.0), ..sp() }),
        ("zero feed", SlotParams { feed: 0.0, ..sp() }),
        ("zero stepdown", SlotParams { stepdown: 0.0, ..sp() }),
    ];
    for (name, p) in cases {
        assert!(
            matches!(slot(a, b, &p), Err(GenError::Invalid(_))),
            "{name}: expected a clean refusal"
        );
    }
    assert!(
        matches!(slot(a, a, &sp()), Err(GenError::Invalid(_))),
        "coincident endpoints"
    );
    assert!(
        matches!(slot(a, Point2::new(f64::INFINITY, 0.0), &sp()), Err(GenError::Invalid(_))),
        "non-finite endpoints"
    );
}

#[test]
fn slot_is_deterministic() {
    let p = SlotParams {
        tool_diameter: 6.0,
        width: 20.0,
        stepover: 4.0,
        ..sp()
    };
    let (a, b) = (Point2::new(-3.0, 2.0), Point2::new(21.0, 8.0)); // any old angle
    let x = slot(a, b, &p).unwrap();
    let y = slot(a, b, &p).unwrap();
    assert_eq!(x, y);
    assert!(x.is_chained() && x.all_finite());
}

// --- M8.2 C: the slot's entry arms, certified against the stadium ---

#[test]
fn slot_helix_orbits_in_the_certified_channel() {
    // Tool 6 in a channel 20 wide: h = 10 - 3 = 7 of cutter-center
    // room, and a 1.5 orbit stands in it easy.
    let p = SlotParams {
        tool_diameter: 6.0,
        width: 20.0,
        stepover: 4.0,
        entry: Entry3D::Helix,
        helix_radius: 1.5,
        ..sp()
    };
    let (a, b) = (Point2::new(0.0, 0.0), Point2::new(40.0, 0.0));
    let t = slot(a, b, &p).expect("valid");
    // M11.3: the rows start on the channel's wall, where no orbit can
    // close - the second dive there is named, nothing else is.
    assert!(t.notes.iter().all(|n| n.contains("second dive")), "the channel has room; nothing else to say: {:?}", t.notes);
    assert!(t.is_chained() && t.all_finite());
    let arc_moves: Vec<_> = t.moves.iter().filter(|m| m.arc.is_some()).collect();
    assert!(!arc_moves.is_empty(), "a chosen helix must actually descend");
    for m in &arc_moves {
        // The orbit's standing center keeps a full orbit radius clear of
        // the stadium wall - dive_room eroded by radius + 1e-3.
        assert!(
            seg_dist((m.arc.unwrap().center.x, m.arc.unwrap().center.y), (0.0, 0.0), (40.0, 0.0))
                <= 7.0 - 1.5 + 1e-3,
            "helix center stands in the wall: {:?}",
            m.arc.unwrap().center
        );
        for (_, q) in m.chords(FLATTEN_TOL) {
            assert!(
                seg_dist((q.x, q.y), (0.0, 0.0), (40.0, 0.0)) <= 7.0 + 2.0 * FLATTEN_TOL,
                "the orbit crossed the channel it certified: {q:?}"
            );
        }
    }
    // The wall dive this law replaces had no arcs at all.
    let base = slot(a, b, &SlotParams { entry: Entry3D::Plunge, ..p }).expect("valid");
    assert_eq!(base.moves.iter().filter(|m| m.arc.is_some()).count(), 0);
    assert_ne!(t.moves, base.moves, "the helix changed the entry");
}

#[test]
fn slot_ramp_walks_down_the_certified_channel() {
    let p = SlotParams {
        tool_diameter: 6.0,
        width: 20.0,
        stepover: 4.0,
        entry: Entry3D::Ramp,
        ..sp()
    };
    let (a, b) = (Point2::new(0.0, 0.0), Point2::new(40.0, 0.0));
    let t = slot(a, b, &p).expect("valid");
    assert!(t.notes.is_empty(), "the chord cleared; nothing to say: {:?}", t.notes);
    assert!(t.is_chained() && t.all_finite());
    // Every level opens on a diagonal - a Feed that spends XY and Z at
    // once - never a vertical dive: the wall the pre-C law drilled.
    for z in levels_3_zs() {
        let diag = t.moves.iter().any(|m| {
            m.kind == MoveKind::Feed
                && m.to.z == z
                && (m.from.z - m.to.z).abs() > 1e-9
                && (m.from.x - m.to.x).hypot(m.from.y - m.to.y) > 1e-9
        });
        assert!(diag, "level {z} opened with a dive, not a ramp");
        let vdives = t
            .moves
            .iter()
            .filter(|m| m.kind == MoveKind::Plunge && m.to.z == z)
            .count();
        assert_eq!(vdives, 0, "level {z} still drilled the wall");
    }
}

#[test]
fn slot_full_width_admits_no_arm_and_says_so() {
    // sp() is the full-width channel: h = 0, a line, no area a descent
    // could stand in. The law substitutes the axial dive it always
    // posted - byte for byte - and refuses by name.
    let (a, b) = (Point2::new(0.0, 0.0), Point2::new(40.0, 0.0));
    let base = slot(a, b, &sp()).expect("valid");
    assert!(base.notes.is_empty(), "Plunge default says nothing: {:?}", base.notes);
    for entry in [Entry3D::Ramp, Entry3D::Helix] {
        let t = slot(a, b, &SlotParams { entry, ..sp() }).expect("valid");
        assert_eq!(t.notes.len(), 1, "{entry:?} substituted once: {:?}", t.notes);
        assert!(
            t.notes[0].contains("no room the descent could stand in"),
            "refused without naming it: {:?}",
            t.notes
        );
        assert_eq!(t.moves, base.moves, "{entry:?} in a channelless slot must post the old dive");
    }
}

#[test]
fn contour_tabs_stand_down_the_entry_arms() {
    // A bore held by four 2-high tabs: below -4 the tab is material
    // INSIDE the certified region - no arm may descend through it.
    let mk = |entry: Entry3D| {
        let mut p = cp();
        p.side = Side::Inside;
        p.top_z = 0.0;
        p.bottom_z = -6.0;
        p.stepdown = 3.0;
        p.tab_count = 4;
        p.tab_width = 6.0;
        p.tab_height = 2.0;
        p.entry = entry;
        p.helix_radius = 1.5;
        contour(&rect(0.0, 0.0, 20.0, 20.0), &p).expect("valid")
    };
    let t = mk(Entry3D::Helix);
    assert!(
        t.notes.iter().any(|n| n.contains("no arm stands where a tab is material")),
        "the arms stood anyway: {:?}",
        t.notes
    );
    // No orbit reaches a tabbed plane; the levels above -4 keep their
    // certified helix entries.
    assert!(t.moves.iter().any(|m| m.arc.is_some()), "the clean levels lost their arm");
    assert!(
        t.moves.iter().all(|m| m.arc.is_none() || m.to.z > -4.0),
        "a helix descended into a standing tab"
    );
    // At the tabbed plane the dives are the pre-certification lead-in
    // axial ones - four windows, and no floor dive through the web.
    let floor_dive = t
        .moves
        .iter()
        .any(|m| m.kind == MoveKind::Plunge && m.to.z == -6.0 && (m.to.x - 10.0).abs() < 0.5 && (m.to.y - 10.0).abs() < 0.5);
    assert!(!floor_dive, "a certified floor dive stands where the tab does");
    let p = mk(Entry3D::Plunge);
    assert!(p.notes.is_empty(), "the deferral shape is plunge's own - say nothing: {:?}", p.notes);
    assert!(t.is_chained() && t.all_finite() && p.is_chained());
}

#[test]
fn contour_hop_rides_the_tall_tab_top() {
    // A tab taller than the hop plane: between windows the tool must
    // cross ABOVE the tab, or it drags its shank through the tab it was
    // left to hold the part. (tab_height past top_z is a pathological
    // but legal file; the law is the max, not a clamp.)
    let mk = |tab_height: f64| {
        let mut p = cp();
        p.side = Side::Inside;
        p.top_z = 0.0;
        p.bottom_z = -6.0;
        p.stepdown = 3.0;
        p.tab_count = 4;
        p.tab_width = 6.0;
        p.tab_height = tab_height;
        contour(&rect(0.0, 0.0, 20.0, 20.0), &p).expect("valid")
    };
    let tall = mk(12.0); // tab top at +6, above the 3.0 hop plane
    assert!(tall.notes.is_empty(), "plunge-shaped deferral: {:?}", tall.notes);
    assert!(
        tall.moves
            .iter()
            .any(|m| m.kind == MoveKind::Rapid && m.from.z == 6.0 && m.to.z == 6.0),
        "no window-hop ever rode the tab top"
    );
    let sane = mk(2.0); // tab top at -4, below the hop plane: the raise is a no-op
    assert!(
        sane.moves
            .iter()
            .all(|m| m.kind != MoveKind::Rapid || m.from.z != 6.0 || m.to.z != 6.0),
        "the hop plane moved on a sane tab file"
    );
    assert!(sane.is_chained() && tall.is_chained());
}

#[test]
fn trace_runs_exactly_on_the_boundary() {
    let r = rect(0.0, 0.0, 20.0, 20.0);
    let p = TraceParams { bottom_z: 0.0, ..tp() };
    let t = trace(&r, &p).expect("valid");
    let (pts, lifted) = polyline_at(&t, 0.0);
    assert_eq!(pts.len(), 5, "closed walk of a 4-vertex loop");
    assert_eq!(pts[0], pts[4], "the traversal returns to its plunge point");
    assert!(lifted[0] && lifted[1..].iter().all(|&l| !l));
    let corners = [(0.0, 0.0), (20.0, 0.0), (20.0, 20.0), (0.0, 20.0)];
    for q in &pts[..4] {
        assert!(corners.iter().any(|c| c == q), "left the boundary: {q:?}");
    }
    assert!((level_len(&t, 0.0) - 80.0).abs() < 1e-9);
    assert!(shoelace(&pts[..4]) > 0.0, "climb keeps the interior on the left");
    assert_eq!(plunge_xy(&t).len(), 1);
}

#[test]
fn trace_conventional_reverses_the_walk() {
    let r = rect(0.0, 0.0, 20.0, 20.0);
    let p = TraceParams { bottom_z: 0.0, climb: false, ..tp() };
    let t = trace(&r, &p).unwrap();
    let (pts, _) = polyline_at(&t, 0.0);
    assert!(shoelace(&pts[..4]) < 0.0, "conventional runs the loop backwards");
    assert_eq!(pts[0], pts[4]);
}

#[test]
fn trace_cuts_every_loop_at_every_level() {
    let r = Region::new(vec![
        Loop::new(vec![
            Point2::new(0.0, 0.0),
            Point2::new(20.0, 0.0),
            Point2::new(20.0, 20.0),
            Point2::new(0.0, 20.0),
        ])
        .unwrap(),
        Loop::new(vec![
            Point2::new(8.0, 8.0),
            Point2::new(12.0, 8.0),
            Point2::new(12.0, 12.0),
            Point2::new(8.0, 12.0),
        ])
        .unwrap(),
    ])
    .unwrap();
    let t = trace(&r, &tp()).expect("valid"); // zs 0, -1, -2
    assert_eq!(plunge_xy(&t).len(), 6, "two loops at three levels");
    for m in t.moves.iter().filter(|m| m.kind == MoveKind::Rapid) {
        assert!(m.from.z == 5.0 || m.to.z == 5.0, "rapid inside the cut: {m:?}");
    }
    let verts: Vec<(f64, f64)> = r
        .loops()
        .iter()
        .flat_map(|l| l.points().iter().map(|q| (q.x, q.y)))
        .collect();
    for &z in &[0.0, -1.0, -2.0] {
        let (pts, _) = polyline_at(&t, z);
        assert_eq!(pts.len(), 10, "both rings closed at {z}: {pts:?}");
        for q in &pts {
            assert!(verts.iter().any(|v| v == q), "point off the sketch: {q:?}");
        }
        assert!((level_len(&t, z) - 96.0).abs() < 1e-9, "80 outer + 16 hole");
    }
}

#[test]
fn trace_refuses_bad_input() {
    assert!(
        matches!(trace(&Region::empty(), &tp()), Err(GenError::Empty(_))),
        "empty region"
    );
    let r = rect(0.0, 0.0, 10.0, 10.0);
    let cases: Vec<(&str, TraceParams)> = vec![
        ("non-finite clearance", TraceParams { clearance_z: f64::NAN, ..tp() }),
        ("zero stepdown", TraceParams { stepdown: 0.0, ..tp() }),
        ("top below bottom", TraceParams { bottom_z: 1.0, ..tp() }),
        ("clearance below the cut", TraceParams { clearance_z: -3.0, ..tp() }),
        ("retract inside the material", TraceParams { retract_z: Some(-1.0), ..tp() }),
        ("zero feed", TraceParams { feed: 0.0, ..tp() }),
        ("zero plunge feed", TraceParams { plunge_feed: 0.0, ..tp() }),
    ];
    for (name, p) in cases {
        assert!(matches!(trace(&r, &p), Err(GenError::Invalid(_))), "{name}");
    }
}

#[test]
fn trace_is_deterministic() {
    let r = rect(0.0, 0.0, 10.0, 10.0);
    let a = trace(&r, &tp()).unwrap();
    let b = trace(&r, &tp()).unwrap();
    assert_eq!(a, b);
    assert!(a.is_chained() && a.all_finite());
}



#[test]
fn offset_rings_are_the_engagement_law_on_the_plane() {
    // Since M5.1 H the planar pocket's engagement rings (Adaptive until
    // M11.4 C, Offset rings since - Adaptive is the HSM plan) march at
    // the engagement step r (1 - cos theta), not at a free stepover -
    // the alias is gone. The law's closed end is the equality check:
    // 90 degrees gives step = r exactly, the tiling a Waterline run at
    // stepover r produces, move for move.
    let region = rect(0.0, 0.0, 30.0, 30.0);
    let mut w = pp();
    w.strategy = PocketStrategy::Waterline;
    let mut a = pp();
    a.strategy = PocketStrategy::OffsetRings;
    a.optimal_load = 0.5;
    a.stepover = 9.0; // the ignored field, deliberately out of reach
    let tw = pocket(&region, &w).expect("waterline valid");
    let ta = pocket(&region, &a).expect("90 degrees is the one-radius step");
    assert_eq!(
        ta.moves, tw.moves,
        "theta 90 must be exactly the one-radius waterline"
    );
    // Below the closed end the angle, not the field, spaces the rings:
    // a 45 budget bites at r(1 - cos 45) ~ 1.46 and rings far denser.
    let mut s = pp();
    s.strategy = PocketStrategy::OffsetRings;
    s.optimal_load = camcraft_motion::load::DEFAULT_OPTIMAL_LOAD;
    let ts = pocket(&region, &s).expect("adaptive at 45 valid");
    assert!(
        ts.moves.len() > tw.moves.len(),
        "a 45 deg budget must ring tighter than the 5 mm stepover does"
    );
    assert!(ts.is_chained() && ts.all_finite());
}

#[test]
fn a_ramp_down_the_plane_feeds_in_one_diagonal() {
    // The 3-D ladder's ramp on a planar pocket: every level shares one
    // center area, so a chord that clears it is legal at every plane
    // the diagonal crosses. Level 0 takes the arm too (M8.1): home
    // stands outside the center, so its chord is unproven - the arm
    // substitutes the axial dive at the floor's room and SAYS so,
    // while the -3.0 level, entering from cut floor, rides the slope.
    let region = rect(0.0, 0.0, 30.0, 30.0);
    let mut p = pp();
    p.entry = Entry3D::Ramp;
    let t = pocket(&region, &p).expect("ramp valid");
    let first_plunge = t.moves.iter().find(|m| m.to.z == 0.0).expect("level 0 cut");
    assert_eq!(first_plunge.kind, MoveKind::Plunge, "the substituted dive");
    assert!(
        t.notes.iter().any(|n| n.contains("ramp's chord is blocked")),
        "the level 0 substitution must be said, got {:?}",
        t.notes
    );
    assert!(
        t.moves.iter().any(|m| {
            m.kind == MoveKind::Feed
                && (m.from.x != m.to.x || m.from.y != m.to.y)
                && m.from.z != m.to.z
                && m.from.z > -3.0
                && m.to.z == -3.0
        }),
        "the -3.0 level is entered on a ramp"
    );
    assert!(t.is_chained() && t.all_finite());
}

#[test]
fn the_entry_family_is_obeyed_at_the_first_level() {
    // The probe that reopened M8.1: an entry stated as Helix opened a
    // pocket with a straight Plunge at level 0 all the same, because
    // both ladders gated the arms on crossing a level (`!first &&` /
    // `li > 0`). The family is a promise about every move onto fresh
    // material, and the first one is the most public of all. Helix
    // first: the motion that reaches the surface plane is the orbit's
    // own arc, not a wall dive.
    let region = rect(0.0, 0.0, 30.0, 30.0);
    let mut p = pp();
    p.entry = Entry3D::Helix;
    p.helix_radius = 2.0;
    let t = pocket(&region, &p).expect("helix valid");
    let reach = t
        .moves
        .iter()
        .find(|m| (m.to.z - 0.0).abs() < 1e-9 && m.kind != MoveKind::Rapid)
        .expect("something reaches the surface plane");
    assert_eq!(reach.kind, MoveKind::Feed, "the first level is entered in arcs: {reach:?}");
    assert!(reach.arc.is_some(), "and the arc is the helix's own");
    // M11.3: the helix lands on the first row; only a named second dive
    // (the row start on the wall) may be said - never a substitution.
    assert!(t.notes.iter().all(|n| n.contains("second dive")), "room exists; nothing to substitute: {:?}", t.notes);
    // Plunge next - the DEFAULT, so this is the shape every pocket
    // posts: the first dive stands in the floor's room, never on the
    // corner of `passes[0]`.
    let t2 = pocket(&region, &pp()).expect("plunge valid");
    let center = region.offset(-5.0).expect("offset");
    let dive = t2
        .moves
        .iter()
        .find(|m| m.kind == MoveKind::Plunge)
        .expect("a dive at level 0");
    assert!(
        center.contains(Point2::new(dive.to.x, dive.to.y)),
        "the level 0 dive is interior, got {:?}",
        dive.to
    );
    assert!(t2.notes.is_empty(), "the axial arm found its room on the first row: {:?}", t2.notes);
}

#[test]
fn a_helix_that_fits_on_the_plane_descends_in_arcs() {
    // Same law as the 3-D twin, planar: the dive sits mid-row, the
    // widened disc (tool radius + helix radius) eroded from the one
    // center every level shares, and Z sinks with the orbit.
    let region = rect(0.0, 0.0, 30.0, 30.0);
    let mut p = pp();
    p.entry = Entry3D::Helix;
    p.helix_radius = 2.0;
    let t = pocket(&region, &p).expect("helix valid");
    assert!(
        t.moves.iter().any(|m| m.arc.is_some() && m.to.z < 0.0),
        "the helix posts arcs that sink"
    );
    // The orbit's XY net is zero: a full-turn dive ends where it began
    // in plan, so the level's first feed starts from the dive point.
    assert!(t.is_chained() && t.all_finite());
}

#[test]
fn a_helix_with_no_room_on_the_plane_plunges() {
    // Fail-safe the ladder proves (M8.1's wording): where the widened
    // disc fits nowhere (a pocket barely wider than the tool) the
    // crossing gives up the orbit but not the entry law - it dives on
    // the FLOOR's own room, the one place the disc's axial column is
    // proven open, and says so. Before Phase 8 this was the silent
    // wall climb-and-dive; a substitution nobody reads is a lie.
    let region = rect(0.0, 0.0, 12.0, 12.0); // center is a 2 mm square
    let mut p = pp();
    p.entry = Entry3D::Helix;
    p.helix_radius = 4.9; // disc r + rh ~ 9.9: the band has no room
    let t = pocket(&region, &p).expect("still a valid pocket");
    let level2 = t
        .moves
        .iter()
        .find(|m| m.to.z == -3.0)
        .expect("the -3.0 level was cut by something");
    // M11.3's fallback order: the helix stands down to the ramp along
    // the first cut (a plunge only where no ramp fits either).
    assert_ne!(level2.kind, MoveKind::Rapid, "the level is entered by a cut");
    assert!(
        level2.arc.is_none()
            && !t.moves.iter().any(|m| m.arc.is_some() && m.from.z != m.to.z),
        "the substituted descent never orbits"
    );
    assert!(t.notes.iter().any(|n| n.contains("stands down to a ramp")), "{:?}", t.notes);
    assert!(
        t.notes.iter().any(|n| n.contains("widened disc")),
        "the substitution must be said, got {:?}",
        t.notes
    );
    assert!(t.is_chained() && t.all_finite());
    // The channel runs end to end: the note reaches the status line.
    let line = t.status_summary(
        "pocket",
        0,
        camcraft_motion::geometry::units::Unit::Millimeters,
    );
    assert!(line.contains("widened disc"), "the line says it: {line}");
}

#[test]
fn the_helix_on_a_disc_spirals_from_the_core_not_the_wall() {
    // M8.1's killer check: the tessellated disc is the geometry the old
    // dive scan died on. Its waterline plan is all arcs, so every
    // consecutive-pair midpoint sat ON a ring line - never inside the
    // leg eroded again for the widened disc - the scan resolved None,
    // and the helix entry silently became a wall climb-and-dive on
    // every round pocket this app ever posted. Two laws meet here:
    // the floor's own triangulation picks the landing (descending arcs
    // post, with nothing to report), and the rings run inside-out, so
    // the first contact is the core and the wall pass comes last.
    let disc = Region::new(vec![Loop::new(ring_circle(20.0, 192)).unwrap()]).unwrap();
    let mut p = pp();
    p.strategy = PocketStrategy::Waterline;
    p.entry = Entry3D::Helix;
    p.helix_radius = 2.0;
    let t = pocket(&disc, &p).expect("round helix valid");
    assert!(
        t.moves.iter().any(|m| {
            m.arc.is_some() && m.kind == MoveKind::Feed && m.from.z != m.to.z
        }),
        "the level change descends in arcs, not a silent wall plunge"
    );
    assert!(t.notes.is_empty(), "room was found; nothing to say: {:?}", t.notes);
    let dist = |m: &camcraft_motion::cl::Move| (m.to.x * m.to.x + m.to.y * m.to.y).sqrt();
    // The path's very first contact: the core, not the Ø30 wall ring.
    // M11.3: the helix closes on the first ring's start - its first arc
    // is the first contact.
    let first = t
        .moves
        .iter()
        .find(|m| m.arc.is_some() && m.from.z != m.to.z)
        .expect("some descent into the plane");
    assert!(
        dist(first) < 8.0,
        "the first descent stands in the core, got {:?}",
        first.to
    );
    // And the crossing that opens -3.0 lands where it lands: the
    // helix's own arcs reach the plane inside the core (an earned
    // orbit, not a dive - a substitution would have put a Plunge
    // here, and the empty-notes assert above already refused one).
    let open = t
        .moves
        .iter()
        .find(|m| m.to.z == -3.0)
        .expect("the -3.0 level was reached");
    assert!(dist(open) < 12.0, "the level opens in the floor's room: {:?}", open.to);
    assert!(t.is_chained() && t.all_finite());
}

#[test]
fn roughing_rings_run_inside_out_and_the_wall_walks_last() {
    // The order law by itself, no entry family: roughing reaches the
    // finished wall only over floor its neighbours already cleared.
    // Wall-first made `passes[0]` the outermost ring - every level's
    // first contact a dive onto the wall the part must keep - and
    // pinned a hop behind every chain for no reason at all.
    let region = rect(0.0, 0.0, 40.0, 40.0); // center: [5,35]^2
    let mut p = pp();
    p.strategy = PocketStrategy::Waterline;
    let t = pocket(&region, &p).expect("waterline valid");
    let d = |q: &camcraft_motion::cl::Move| {
        ((q.to.x - 20.0).powi(2) + (q.to.y - 20.0).powi(2)).sqrt()
    };
    let first = t
        .moves
        .iter()
        .find(|m| m.kind == MoveKind::Plunge)
        .expect("the pocket dives somewhere");
    assert!(d(first) < 8.0, "the pocket opens in the core, got {:?}", first.to);
    let last_top = t
        .moves
        .iter()
        .rfind(|m| m.kind == MoveKind::Feed && m.to.z == 0.0)
        .expect("the top level cuts");
    assert!(d(last_top) >= 14.0, "the wall ring walks last, got {:?}", last_top.to);
    assert!(t.is_chained() && t.all_finite());
}

#[test]
fn dive_room_never_lands_outside_the_region() {
    // The M8.1 guard in `dive_room`: eroding an annulus by more than
    // its own width leaves an inverted ring - the hole has swallowed
    // the outer loop - and the ear-clip tessellates that to phantom
    // triangles whose union is not the region. A dive that trusts a
    // phantom orbits the tool into the void. Centroids are accepted
    // only where `contains` agrees, so a phantom floor answers None
    // and the caller substitutes loudly.
    let outer = Loop::new(vec![
        Point2::new(-14.0, -14.0),
        Point2::new(14.0, -14.0),
        Point2::new(14.0, 14.0),
        Point2::new(-14.0, 14.0),
    ])
    .unwrap();
    // A hole loop with opposite orientation is the region's void.
    let hole = Loop::new(vec![
        Point2::new(-12.0, -12.0),
        Point2::new(-12.0, 12.0),
        Point2::new(12.0, 12.0),
        Point2::new(12.0, -12.0),
    ])
    .unwrap();
    let ring = Region::new(vec![outer, hole]).unwrap();
    assert_eq!(ring.area(), 784.0 - 576.0, "the 2 mm band by both measures");
    // Ask for room a 1.5 mm disc fits: no point of a 2 mm band is
    // 1.5 clear of both its edges, so the honest answer is None -
    // while the same question put to `Region::offset` hands back a
    // crossed-loops ring of phantom material.
    if let Some(p) = camcraft_motion::entry::dive_room(&ring, 1.5) {
        panic!("a 2 mm band has no 1.5 clearance anywhere, got {p:?}");
    }
    // The band itself has honest room for the axial arm: the margin
    // test must not have become a blanket refusal.
    let d = camcraft_motion::entry::dive_room(&ring, 0.0).expect("the band's own floor has room");
    assert!(
        ring.contains(d),
        "the dive point must stand inside the center, got {d:?}"
    );
    // And a disc the band comfortably fits finds centroids in it:
    // every ear of this ring has its centre within 2/3 mm of one
    // edge, so the 0.5 question answers somewhere honest. (Centroid
    // sampling can miss room on a hair-thin band - a real disc of
    // half the width fits nowhere a triangle centre sits; the law
    // accepts that as None, which is a said substitution, never a
    // silent wrong dive.)
    let tight = camcraft_motion::entry::dive_room(&ring, 0.5)
        .expect("a 2 mm band offers centroids a 0.5 disc clears");
    assert!(ring.contains(tight), "{tight:?} must stand in the band");
}

#[test]
fn the_plunge_entry_posts_the_plain_ladder() {
    // Byte-identity, structural form: with the family's default arm
    // the level crossings are exactly the climb-and-dive the pocket
    // posted before M5.1 H - no arcs, no diagonals, no XY-changing
    // feed with two different z ends.
    let region = rect(0.0, 0.0, 30.0, 30.0);
    let t = pocket(&region, &pp()).expect("plain pocket");
    assert!(
        !t.moves.iter().any(|m| m.arc.is_some()),
        "zigzag never arcs, entry or not"
    );
    assert!(
        !t.moves.iter().any(|m| {
            m.kind == MoveKind::Feed && m.from.z != m.to.z && (m.from.x != m.to.x || m.from.y != m.to.y)
        }),
        "a Plunge pocket has no diagonal feeds"
    );
}
