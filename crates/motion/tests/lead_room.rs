//! Lead room law: every lead a contour posts keeps its cutter center in
//! the legal center region - inside the tool-radius inset for an Inside
//! contour (off islands), outside the grown region for an Outside one.
//! The oracle here is independent of the generator's own offset: the
//! verifier's tangency law (distance to the boundary >= r + leave, on
//! the requested side), sampled densely along every arc.
//!
//! Gate map (M4.4): covers camcraft_motion::machining2d (contour, lead
//! placement), camcraft_motion::geo2d.

use camcraft_motion::cl::{MoveKind, Toolpath};
use camcraft_motion::entry::Entry3D;
use camcraft_motion::geo2d::{Loop, Point2, Region};
use camcraft_motion::machining2d::{contour, ContourParams, Lead, Side};

fn lp(pts: &[(f64, f64)]) -> Loop {
    Loop::new(pts.iter().map(|&(x, y)| Point2::new(x, y)).collect()).unwrap()
}

/// `pts` with its vertex order rotated by `k` - the same loop, a
/// different first vertex for the offsetter to start from.
fn rotated(pts: &[(f64, f64)], k: usize) -> Vec<(f64, f64)> {
    let n = pts.len();
    (0..n).map(|i| pts[(i + k) % n]).collect()
}

/// One closed loop's vertices.
type Shape = Vec<(f64, f64)>;

fn rect_pts(x0: f64, y0: f64, x1: f64, y1: f64) -> Vec<(f64, f64)> {
    vec![(x0, y0), (x1, y0), (x1, y1), (x0, y1)]
}

fn l_pts() -> Vec<(f64, f64)> {
    vec![(0.0, 0.0), (40.0, 0.0), (40.0, 15.0), (15.0, 15.0), (15.0, 40.0), (0.0, 40.0)]
}

fn circle_pts(cx: f64, cy: f64, r: f64, n: usize) -> Vec<(f64, f64)> {
    (0..n)
        .map(|k| {
            let t = std::f64::consts::TAU * k as f64 / n as f64;
            (cx + r * t.cos(), cy + r * t.sin())
        })
        .collect()
}

fn params(tool: f64, side: Side, lead: Lead, climb: bool) -> ContourParams {
    ContourParams {
        tool_diameter: tool,
        stock_to_leave: 0.0,
        clearance_z: 5.0,
        retract_z: 2.0,
        top_z: 0.0,
        bottom_z: -4.0,
        stepdown: 2.0,
        feed: 500.0,
        plunge_feed: 100.0,
        climb,
        side,
        lead,
        tolerance: 0.01,
        tab_count: 0,
        tab_width: 0.0,
        tab_height: 0.0,
        entry: Entry3D::Plunge,
        helix_radius: 1.0,
        blend_turns: false, blend_radius: None,
        links: camcraft_motion::links::Links::PLANAR,
        picks: Default::default(),
    }
}

fn seg_dist(p: Point2, a: Point2, b: Point2) -> f64 {
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    let dd = dx * dx + dy * dy;
    let t = if dd <= 0.0 {
        0.0
    } else {
        (((p.x - a.x) * dx + (p.y - a.y) * dy) / dd).clamp(0.0, 1.0)
    };
    ((p.x - a.x - t * dx).powi(2) + (p.y - a.y - t * dy).powi(2)).sqrt()
}

fn wall_dist(region: &Region, p: Point2) -> f64 {
    let mut d = f64::INFINITY;
    for l in region.loops() {
        let pts = l.points();
        for k in 0..pts.len() {
            d = d.min(seg_dist(p, pts[k], pts[(k + 1) % pts.len()]));
        }
    }
    d
}

/// Every XY the cutter center visits on a non-rapid move: lines and arc
/// chords (1 um sagitta) subdivided at 50 um. A move whose XY repeats an
/// earlier one (the next level's copy) is sampled once.
fn samples(t: &Toolpath) -> Vec<(usize, Point2)> {
    let mut out = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for (i, m) in t.moves.iter().enumerate() {
        if m.kind == MoveKind::Rapid {
            continue;
        }
        let key = [m.from.x, m.from.y, m.to.x, m.to.y].map(f64::to_bits);
        if !seen.insert((key, m.arc.map(|a| (a.center.x.to_bits(), a.center.y.to_bits(), a.ccw)))) {
            continue;
        }
        for (a, b) in m.chords(1e-3) {
            let len = ((b.x - a.x).powi(2) + (b.y - a.y).powi(2)).sqrt();
            let n = ((len / 0.05).ceil() as usize).max(1);
            for k in 0..=n {
                let u = k as f64 / n as f64;
                out.push((i, Point2::new(a.x + (b.x - a.x) * u, a.y + (b.y - a.y) * u)));
            }
        }
    }
    out
}

/// The first cutter center that leaves the legal room, if any.
fn first_violation(region: &Region, t: &Toolpath, p: &ContourParams) -> Option<(usize, Point2, f64)> {
    let d = p.tool_diameter * 0.5 + p.stock_to_leave;
    let inside = p.side == Side::Inside;
    // 15 um: the verifier's 10 um gouge budget, plus half the 10 um fit
    // tolerance - a fitted WALK arc on a tessellated bore may stand that
    // far past the inset polyline it replaced (fit_arcs' two-sided law,
    // not a lead). A lead that leaves the room at a corner does so by
    // millimetres.
    let tol = 0.015;
    samples(t).into_iter().find_map(|(i, q)| {
        let w = wall_dist(region, q);
        let side_ok = region.contains(q) == inside;
        if w < d - tol || (!side_ok && w > tol) {
            Some((i, q, w))
        } else {
            None
        }
    })
}

#[test]
fn inside_arc_lead_at_a_corner_start_stays_off_the_wall() {
    // The reported repro: 40x40 pocket, Inside, Ø10 tool, Arc{3}. The
    // legal center room is [5,35]^2; the loop starts on a corner of it,
    // and the quarter arc used to begin at (8, 2) - 3 mm into the wall
    // at y = 0 - plunge there and arc to (5, 5).
    let region = Region::new(vec![lp(&rect_pts(0.0, 0.0, 40.0, 40.0))]).unwrap();
    let p = params(10.0, Side::Inside, Lead::Arc { radius: 3.0 }, true);
    let t = contour(&region, &p).unwrap();
    let bad = first_violation(&region, &t, &p);
    assert!(bad.is_none(), "lead leaves the legal room: {bad:?}\n{:#?}", &t.moves[..8.min(t.moves.len())]);
    for m in &t.moves {
        if m.kind == MoveKind::Plunge {
            assert!(
                (5.0 - 1e-9..=35.0 + 1e-9).contains(&m.to.x) && (5.0 - 1e-9..=35.0 + 1e-9).contains(&m.to.y),
                "plunge outside the inset: {m:?}"
            );
        }
    }
    assert!(t.has_arcs(), "the arc lead still posts where it fits");
}

#[test]
fn plate_hole_too_small_for_the_lead_refuses_it_by_name() {
    // The plate-with-hole golden's job: Ø6 tool in an 8x8 hole leaves a
    // 2x2 center room; a 3 mm arc lead fits nowhere in it. The lead must
    // shrink or be refused - by name - never post through the plate.
    let region = Region::new(vec![
        lp(&rect_pts(0.0, 0.0, 40.0, 20.0)),
        lp(&rect_pts(16.0, 6.0, 24.0, 14.0)),
    ])
    .unwrap();
    let p = params(6.0, Side::Auto, Lead::Arc { radius: 3.0 }, true);
    let t = contour(&region, &p).unwrap();
    let p_out = ContourParams { side: Side::Outside, ..p };
    let bad = first_violation(&region, &t, &p_out);
    assert!(bad.is_none(), "lead leaves the legal room: {bad:?}");
    assert!(
        t.notes.iter().any(|n| n.contains("lead")),
        "the shrunk/refused lead is named: {:?}",
        t.notes
    );
}

#[test]
fn every_lead_stays_in_the_legal_room() {
    // Property sweep over the polygonal shapes (the corner cases):
    // shapes x sides x lead kinds x radii x climb x start vertex x tool.
    sweep(
        vec![
            ("square40", vec![rect_pts(0.0, 0.0, 40.0, 40.0)]),
            ("rect40x14", vec![rect_pts(0.0, 0.0, 40.0, 14.0)]),
            ("ell", vec![l_pts()]),
            ("coarse_disc", vec![circle_pts(0.0, 0.0, 9.0, 12)]),
            (
                "plate_hole",
                vec![rect_pts(0.0, 0.0, 40.0, 20.0), rect_pts(16.0, 6.0, 24.0, 14.0)],
            ),
        ],
        300,
    );
}

#[test]
#[ignore = "heavy lane: dense lead-room scan over fine tessellations (cargo test --release -- --ignored)"]
fn every_lead_stays_in_the_legal_room_on_fine_rings() {
    // The same law over a fine 96-gon disc and a framed island: the
    // arc-fitted walks and the many-edge reseat candidates.
    sweep(
        vec![
            ("disc", vec![circle_pts(20.0, 20.0, 15.0, 96)]),
            (
                "frame",
                vec![rect_pts(0.0, 0.0, 50.0, 50.0), rect_pts(15.0, 15.0, 35.0, 35.0)],
            ),
        ],
        100,
    );
}

/// Each case either generates a path whose every cutter center sits in
/// the legal room, or refuses cleanly; at least `min_cases` generate.
fn sweep(shapes: Vec<(&str, Vec<Shape>)>, min_cases: usize) {
    let leads = [
        Lead::None,
        Lead::Linear { length: 2.0 },
        Lead::Linear { length: 6.0 },
        Lead::Arc { radius: 1.0 },
        Lead::Arc { radius: 3.0 },
        Lead::Arc { radius: 8.0 },
        Lead::Tangential { radius: 1.0 },
        Lead::Tangential { radius: 4.0 },
    ];
    let mut checked = 0usize;
    for (name, loops) in &shapes {
        let n0 = loops[0].len();
        let starts: Vec<usize> = if n0 > 6 { vec![0, n0 / 3] } else { (0..n0).collect() };
        for k in starts {
            let mut ls: Vec<Loop> = vec![lp(&rotated(&loops[0], k))];
            for extra in &loops[1..] {
                ls.push(lp(&rotated(extra, k % extra.len())));
            }
            let Ok(region) = Region::new(ls) else { continue };
            for side in [Side::Inside, Side::Outside] {
                for &lead in &leads {
                    for climb in [true, false] {
                        for tool in [6.0, 10.0] {
                            let p = params(tool, side, lead, climb);
                            let Ok(t) = contour(&region, &p) else { continue };
                            assert!(t.all_finite() && t.is_chained(), "{name} {side:?} {lead:?}");
                            let bad = first_violation(&region, &t, &p);
                            assert!(
                                bad.is_none(),
                                "{name} k={k} {side:?} {lead:?} climb={climb} tool={tool}: \
                                 center leaves the legal room at {bad:?}: {:?}",
                                bad.map(|b| &t.moves[b.0])
                            );
                            checked += 1;
                        }
                    }
                }
            }
        }
    }
    assert!(checked >= min_cases, "the sweep must actually generate: {checked}");
}

#[test]
fn tabbed_windows_keep_their_leads_in_the_room_too() {
    // Tab windows start mid-ring wherever the spacing puts them - often
    // near a corner. Same law, per window.
    let region = Region::new(vec![lp(&rect_pts(0.0, 0.0, 40.0, 40.0))]).unwrap();
    for side in [Side::Inside, Side::Outside] {
        for lead in [Lead::Arc { radius: 3.0 }, Lead::Linear { length: 4.0 }, Lead::Tangential { radius: 2.0 }] {
            for count in [1u32, 3, 4, 7] {
                let p = ContourParams {
                    tab_count: count,
                    tab_width: 4.0,
                    tab_height: 2.0,
                    ..params(10.0, side, lead, true)
                };
                let t = contour(&region, &p).unwrap();
                let bad = first_violation(&region, &t, &p);
                assert!(bad.is_none(), "{side:?} {lead:?} tabs={count}: {bad:?}");
            }
        }
    }
}
