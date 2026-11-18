//! M11.1 owed item on a real contour: a D-shaped bore's line-to-arc
//! corners round with tangent fillets that never come closer to the wall
//! than the sharp path, and every corner left sharp is named.
//!
//! Gate map (M11.1): covers camcraft_motion::machining2d (fillet_arc, contour).

use camcraft_motion::cl::{Move, MoveKind, Toolpath};
use camcraft_motion::entry::Entry3D;
use camcraft_motion::geo2d::{Loop, Point2, Region};
use camcraft_motion::machining2d::{contour, ContourParams, Lead, Side};

fn p(x: f64, y: f64) -> Point2 {
    Point2::new(x, y)
}

fn cp(blend: bool) -> ContourParams {
    ContourParams {
        tool_diameter: 6.0,
        stock_to_leave: 0.0,
        clearance_z: 5.0,
        retract_z: 2.0,
        top_z: 0.0,
        bottom_z: -4.0,
        stepdown: 2.0,
        feed: 500.0,
        plunge_feed: 100.0,
        climb: true,
        side: Side::Inside,
        lead: Lead::None,
        tolerance: 0.01,
        tab_count: 0,
        tab_width: 0.0,
        tab_height: 0.0,
        entry: Entry3D::Plunge,
        helix_radius: 1.0,
        blend_turns: blend,
        links: camcraft_motion::links::Links::PLANAR,
    }
}

/// A D: a 30 mm flat closed by a semicircle of radius 15 (fine chords).
fn d_hole() -> Region {
    let mut pts = vec![p(0.0, 0.0), p(30.0, 0.0)];
    for k in 1..180 {
        let a = std::f64::consts::PI * k as f64 / 180.0;
        pts.push(p(15.0 + 15.0 * a.cos(), 15.0 * a.sin()));
    }
    Region::new(vec![Loop::new(pts).unwrap()]).unwrap()
}

fn min_to_wall(t: &Toolpath, region: &Region, keep: impl Fn(&Move) -> bool) -> f64 {
    let mut d = f64::INFINITY;
    for m in t.moves.iter().filter(|m| m.kind == MoveKind::Feed && m.from.z == m.to.z && keep(m)) {
        let (a, b) = (p(m.from.x, m.from.y), p(m.to.x, m.to.y));
        let pts: Vec<Point2> = match m.arc {
            None => (0..=200).map(|k| k as f64 / 200.0).map(|t| p(a.x + (b.x - a.x) * t, a.y + (b.y - a.y) * t)).collect(),
            Some(arc) => {
                let c = arc.center;
                let r = (a.x - c.x).hypot(a.y - c.y);
                let s = (a.y - c.y).atan2(a.x - c.x);
                let e = (b.y - c.y).atan2(b.x - c.x);
                let sg = if arc.ccw { 1.0 } else { -1.0 };
                let sw = (sg * (e - s)).rem_euclid(std::f64::consts::TAU);
                (0..=256).map(|k| s + sg * sw * k as f64 / 256.0).map(|t| p(c.x + r * t.cos(), c.y + r * t.sin())).collect()
            }
        };
        for q in pts {
            for l in region.loops() {
                let w = l.points();
                for i in 0..w.len() {
                    let (u, v) = (w[i], w[(i + 1) % w.len()]);
                    let (dx, dy) = (v.x - u.x, v.y - u.y);
                    let tt = (((q.x - u.x) * dx + (q.y - u.y) * dy) / (dx * dx + dy * dy)).clamp(0.0, 1.0);
                    d = d.min((q.x - u.x - tt * dx).hypot(q.y - u.y - tt * dy));
                }
            }
        }
    }
    d
}

#[test]
fn a_d_bore_holds_the_gouge_law_with_curved_corners_on() {
    let hole = d_hole();
    let sharp = contour(&hole, &cp(false)).unwrap();
    let blended = contour(&hole, &cp(true)).unwrap();
    assert!(blended.is_chained() && blended.all_finite());
    let ds = min_to_wall(&sharp, &hole, |_| true);
    // The fillets themselves: arcs about a centre the sharp walk never
    // used (the trimmed originals keep theirs and are subsets of the
    // sharp curves).
    let centres: Vec<_> = sharp.moves.iter().filter_map(|m| m.arc.map(|a| a.center)).collect();
    let fillet = |m: &Move| m.arc.is_some_and(|a| !centres.contains(&a.center));
    let n_fillets = blended.moves.iter().filter(|m| fillet(m)).count();
    let df = min_to_wall(&blended, &hole, fillet);
    assert!(n_fillets == 0 || df >= ds - 1e-6, "gouge: a fillet at {df} < {ds}");
    // Whole path, within the sampling's own resolution of the sharp one.
    let db = min_to_wall(&blended, &hole, |_| true);
    assert!(db >= ds - 1e-5, "gouge: {db} < {ds}");
    // Every corner is accounted for in the notes, none silently.
    for n in &blended.notes {
        assert!(n.starts_with("turn blends:") || n.starts_with("entry:") || n.starts_with("lead:"), "{n}");
    }
    // Each level's two line-arc corners: one is the seam (the entry
    // stands there, named), the other rounds - and rides the posted
    // moves tangent at both ends (to the 1 um lattice).
    // Two levels (-2, -4; #2004: no air level at Top Z 0).
    assert_eq!(n_fillets, 2, "{:?}", blended.notes);
    assert!(blended.notes.iter().any(|n| n.contains("2 of 4 walked corner(s) rounded")), "{:?}", blended.notes);
    let tan = |m: &Move, end: bool| {
        let (a, b) = ((m.from.x, m.from.y), (m.to.x, m.to.y));
        let v = match m.arc {
            None => (b.0 - a.0, b.1 - a.1),
            Some(arc) => {
                let q = if end { b } else { a };
                let (dx, dy) = (q.0 - arc.center.x, q.1 - arc.center.y);
                if arc.ccw { (-dy, dx) } else { (dy, -dx) }
            }
        };
        let l = v.0.hypot(v.1);
        (v.0 / l, v.1 / l)
    };
    for (i, m) in blended.moves.iter().enumerate().filter(|(_, m)| fillet(m)) {
        let (before, after) = (&blended.moves[i - 1], &blended.moves[i + 1]);
        for (u, w) in [(tan(before, true), tan(m, false)), (tan(m, true), tan(after, false))] {
            let cross = u.0 * w.1 - u.1 * w.0;
            assert!(u.0 * w.0 + u.1 * w.1 > 0.0 && cross.abs() < 1e-5, "kink {cross} at fillet move {i}");
        }
    }
}
