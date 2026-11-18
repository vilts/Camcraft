//! 2D rest machining (#1446): the rest area is what a larger previous
//! tool could not reach, read large and never small; the rest passes stay
//! where that material is, never rapid below the top of the cut, pass the
//! verifier, and cover the material they were planned for. Refusals are
//! named, and degenerate input errors instead of crashing.
//!
//! Gate map (#1446): covers camcraft_motion::machining2d (rest2d, pocket,
//! pocket_strategy).

use camcraft_motion::cl::{Move, MoveKind, Toolpath};
use camcraft_motion::entry::Entry3D;
use camcraft_motion::geo2d::{Loop, Point2, Region};
use camcraft_motion::machining2d::{
    pocket, rest_plan, rest_pocket, GenError, PocketParams, PocketStrategy, RestParams, REST_FILM, REST_REACH,
};
use camcraft_motion::verify::{verify, Mode, ToolGeom, VerifyContext, Warning};

const W: f64 = 60.0;
const H: f64 = 40.0;
const CORNER: f64 = 3.0;
const PREV_D: f64 = 12.0;
const TOOL_D: f64 = 4.0;

fn poly(pts: Vec<Point2>) -> Region {
    Region::new(vec![Loop::new(pts).unwrap()]).unwrap()
}

/// A `w` x `h` pocket from the origin with corner radius `cr` (64 chords
/// per corner, vertices on the true arc).
fn rounded(w: f64, h: f64, cr: f64) -> Region {
    let centers = [(w - cr, cr, -90.0), (w - cr, h - cr, 0.0), (cr, h - cr, 90.0), (cr, cr, 180.0)];
    let mut pts = Vec::new();
    for (cx, cy, start) in centers {
        for k in 0..=64 {
            let a = (start + 90.0 * f64::from(k) / 64.0_f64).to_radians();
            pts.push(Point2::new(cx + cr * a.cos(), cy + cr * a.sin()));
        }
    }
    poly(pts)
}

/// Signed distance into the exact `W` x `H` rounded rectangle of corner
/// radius `cr` (positive inside).
fn depth_in(p: Point2, cr: f64) -> f64 {
    let qx = (p.x - W * 0.5).abs() - (W * 0.5 - cr);
    let qy = (p.y - H * 0.5).abs() - (H * 0.5 - cr);
    let outside = (qx.max(0.0).powi(2) + qy.max(0.0).powi(2)).sqrt();
    cr - outside - qx.max(qy).min(0.0)
}

fn prev() -> RestParams {
    RestParams {
        previous_tool_diameter: PREV_D,
        previous_corner_radius: 0.0,
        previous_stock_to_leave: 0.0,
        allowance: 0.0,
    }
}

fn pp(strategy: PocketStrategy) -> PocketParams {
    PocketParams {
        blend_radius: None,
        links: camcraft_motion::links::Links::PLANAR,
        tool_diameter: TOOL_D,
        stepover: 1.5,
        stock_to_leave: 0.0,
        clearance_z: 5.0,
        retract_z: None,
        top_z: 0.0,
        bottom_z: -4.0,
        stepdown: 2.0,
        feed: 400.0,
        plunge_feed: 80.0,
        climb: true,
        strategy,
        entry: Entry3D::Plunge,
        helix_radius: 1.0,
        optimal_load: camcraft_motion::load::DEFAULT_OPTIMAL_LOAD,
        blend_turns: false,
    }
}

fn check(path: &Toolpath, region: &Region, r: f64) -> Vec<Warning> {
    let t = ToolGeom {
        radius: r,
        nose_radius: 0.0,
        flute_len: 30.0,
        shank_radius: r,
        shank_len: 40.0,
        holder_radius: 20.0,
        holder_len: 60.0,
    };
    let cx = VerifyContext {
        region,
        stock: None,
        model: None,
        tool: &t,
        cutter: None,
        mode: Mode::Area,
        stock_top_z: 0.0,
        clearance_z: 5.0,
        stock_to_leave: 0.0,
        gouge_tol: 0.01,
        z_tol: 1e-6,
        finish_tol: None,
        fixtures: &[],
        spindle: None,
        ..Default::default()
    };
    verify(path, &cx)
}

fn seg_dist(q: Point2, a: Point2, b: Point2) -> f64 {
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    let len2 = dx * dx + dy * dy;
    let t = if len2 > 0.0 { (((q.x - a.x) * dx + (q.y - a.y) * dy) / len2).clamp(0.0, 1.0) } else { 0.0 };
    ((q.x - a.x - t * dx).powi(2) + (q.y - a.y - t * dy).powi(2)).sqrt()
}

/// XY chords of every feed that runs flat at `z`.
fn cuts_at(path: &Toolpath, z: f64) -> Vec<(Point2, Point2)> {
    path.moves
        .iter()
        .filter(|m| m.kind == MoveKind::Feed && (m.from.z - z).abs() < 1e-9 && (m.to.z - z).abs() < 1e-9)
        .flat_map(|m| m.chords(0.005))
        .collect()
}

/// The only rapid allowed below the top of the cut: straight up, out of
/// where the tool has just cut.
fn lifts_out(m: &Move) -> bool {
    m.from.x == m.to.x && m.from.y == m.to.y && m.to.z > m.from.z
}

/// Grid points of the fixture pocket (0.1 mm pitch).
fn grid() -> impl Iterator<Item = Point2> {
    (0..=600).flat_map(|i| (0..=400).map(move |j| Point2::new(f64::from(i) * 0.1, f64::from(j) * 0.1)))
}

/// Material the Ø12 tool certainly left: inside the pocket and outside
/// the radius-6 opening, each by more than the film tolerance.
fn certain_residue(p: Point2) -> bool {
    depth_in(p, CORNER) > REST_FILM + 0.01 && depth_in(p, PREV_D * 0.5) < -(REST_FILM + 0.01)
}

fn invalid(r: Result<Toolpath, GenError>) -> String {
    match r {
        Err(GenError::Invalid(s)) => s,
        other => panic!("expected an Invalid refusal, got {other:?}"),
    }
}

#[test]
fn rest_area_is_the_four_corners_and_never_reads_small() {
    let region = rounded(W, H, CORNER);
    let plan = rest_plan(&region, TOOL_D, 0.0, &prev()).unwrap();
    assert!(plan.previous_fits);
    assert_eq!(plan.rest.loops().len(), 4, "one rest area per corner");
    // Exact residue: 4 x ((36 - 9 pi) - (9 - 2.25 pi)) = 23.18 mm2; the
    // margin may add to it, nothing may take from it.
    let exact = 4.0 * ((36.0 - 9.0 * std::f64::consts::PI) - (9.0 - 2.25 * std::f64::consts::PI));
    let area = plan.rest.area().abs();
    assert!(area > exact - 0.05 && area < exact + 1.5, "rest area {area}, exact {exact}");
    let (mut certain, mut missed, mut far_inside) = (0usize, 0usize, 0usize);
    for p in grid() {
        if certain_residue(p) {
            certain += 1;
            if !plan.rest.contains(p) {
                missed += 1;
            }
        }
        // Well inside what the previous tool cleared: not rest.
        if depth_in(p, PREV_D * 0.5) > 0.2 && plan.rest.contains(p) {
            far_inside += 1;
        }
    }
    assert!(certain > 1500, "the fixture has residue to find ({certain} grid points)");
    assert_eq!(missed, 0, "residue the previous tool left is missing from the rest area");
    assert_eq!(far_inside, 0, "the rest area reaches into cleared floor");
    // Every rest loop sits in a 6 mm corner box.
    for l in plan.rest.loops() {
        for q in l.points() {
            let (dx, dy) = (q.x.min(W - q.x), q.y.min(H - q.y));
            assert!(dx < 6.2 && dy < 6.2, "rest point ({}, {}) is outside the corners", q.x, q.y);
        }
    }
}

#[test]
fn rest_passes_cut_only_the_corners_and_cover_them() {
    let region = rounded(W, H, CORNER);
    for strategy in [PocketStrategy::Waterline, PocketStrategy::OffsetRings, PocketStrategy::Auto] {
        let p = pp(strategy);
        let path = rest_pocket(&region, &p, &prev()).unwrap();
        assert_eq!(check(&path, &region, TOOL_D * 0.5), Vec::new(), "{strategy:?}: verifier");
        assert!(path.notes.iter().any(|n| n.contains("rest machining trusts the entered previous tool")));
        let mut cutting = 0usize;
        for m in &path.moves {
            let below = m.from.z < p.top_z - 1e-9 || m.to.z < p.top_z - 1e-9;
            if m.kind == MoveKind::Rapid {
                assert!(!below || lifts_out(m), "{strategy:?}: a rapid moves below the top of the cut: {m:?}");
                continue;
            }
            if !below {
                continue;
            }
            cutting += 1;
            // The tool center stays within tool radius of a 6 mm corner
            // box: no pass over the floor the previous tool cleared.
            for q in [m.from, m.to] {
                let (dx, dy) = (q.x.min(W - q.x), q.y.min(H - q.y));
                assert!(dx < 8.3 && dy < 8.3, "{strategy:?}: cut at ({}, {}) is outside the corners", q.x, q.y);
            }
        }
        assert!(cutting > 8, "{strategy:?}: the corners are cut");
        // Coverage at the floor: every certain residue point is under
        // the tool on some pass.
        let cuts = cuts_at(&path, p.bottom_z);
        assert!(!cuts.is_empty());
        let reach = TOOL_D * 0.5 + 0.02;
        let uncut = grid()
            .filter(|&q| certain_residue(q))
            .filter(|&q| !cuts.iter().any(|&(a, b)| seg_dist(q, a, b) <= reach))
            .count();
        assert_eq!(uncut, 0, "{strategy:?}: residue left uncut at the floor");
    }
}

#[test]
fn rest_pass_is_far_shorter_than_the_full_pocket() {
    let region = rounded(W, H, CORNER);
    let p = pp(PocketStrategy::Waterline);
    let len = |t: &Toolpath| -> f64 {
        cuts_at(t, p.bottom_z).iter().map(|&(a, b)| ((b.x - a.x).powi(2) + (b.y - a.y).powi(2)).sqrt()).sum()
    };
    let full = len(&pocket(&region, &p).unwrap());
    let rest = len(&rest_pocket(&region, &p, &prev()).unwrap());
    assert!(rest > 0.0 && rest < full * 0.25, "rest {rest} mm against full {full} mm");
}

#[test]
fn previous_tool_not_larger_is_refused_by_name() {
    let region = rounded(W, H, CORNER);
    let p = pp(PocketStrategy::Waterline);
    for d in [TOOL_D, 3.0] {
        let msg = invalid(rest_pocket(&region, &p, &RestParams { previous_tool_diameter: d, ..prev() }));
        assert!(msg.contains("needs a previous tool larger than this one"), "{msg}");
        assert!(msg.contains(&format!("{d} mm")), "{msg}");
    }
}

#[test]
fn bad_rest_parameters_are_refused_by_name() {
    let region = rounded(W, H, CORNER);
    let p = pp(PocketStrategy::Waterline);
    let cases = [
        (RestParams { previous_corner_radius: 6.5, ..prev() }, "corner radius"),
        (RestParams { previous_corner_radius: -0.1, ..prev() }, "corner radius"),
        (RestParams { previous_stock_to_leave: -0.1, ..prev() }, "previous stock to leave"),
        (RestParams { allowance: -0.1, ..prev() }, "rest allowance"),
        (RestParams { allowance: f64::NAN, ..prev() }, "must be finite"),
        (RestParams { previous_tool_diameter: f64::INFINITY, ..prev() }, "must be finite"),
        (RestParams { previous_tool_diameter: 0.0, ..prev() }, "previous tool diameter must be positive"),
    ];
    for (rest, word) in cases {
        let msg = invalid(rest_pocket(&region, &p, &rest));
        assert!(msg.contains(word), "{rest:?}: {msg}");
    }
    let adaptive = invalid(rest_pocket(&region, &pp(PocketStrategy::Adaptive), &prev()));
    assert!(adaptive.contains("not available with the Adaptive strategy yet"), "{adaptive}");
    let zigzag = rest_pocket(&region, &pp(PocketStrategy::Zigzag), &prev());
    assert!(zigzag.is_ok(), "Zigzag rest machining is supported: {zigzag:?}");
    assert!(rest_plan(&region, 0.0, 0.0, &prev()).is_err());
    assert!(rest_plan(&region, f64::NAN, 0.0, &prev()).is_err());
    assert!(rest_plan(&region, TOOL_D, -1.0, &prev()).is_err());
    assert!(rest_plan(&Region::empty(), TOOL_D, 0.0, &prev()).is_err());
}

#[test]
fn nothing_left_is_refused_not_cut() {
    // The previous tool's own radius in the corners: it reached all of it.
    let region = rounded(W, H, 6.0);
    let plan = rest_plan(&region, TOOL_D, 0.0, &prev()).unwrap();
    assert!(plan.rest.is_empty() && plan.center.is_empty());
    match rest_pocket(&region, &pp(PocketStrategy::Waterline), &prev()) {
        Err(GenError::Empty(s)) => assert!(s.contains("no rest material"), "{s}"),
        other => panic!("expected Empty, got {other:?}"),
    }
}

#[test]
fn previous_tool_that_fits_nowhere_leaves_the_whole_pocket() {
    // A 10 mm wide pocket: the Ø12 tool never entered it.
    let region = rounded(40.0, 10.0, 2.5);
    let p = pp(PocketStrategy::Waterline);
    let plan = rest_plan(&region, TOOL_D, 0.0, &prev()).unwrap();
    assert!(!plan.previous_fits);
    assert!((plan.rest.area() - region.area()).abs() < 1e-9);
    let rest = rest_pocket(&region, &p, &prev()).unwrap();
    let full = pocket(&region, &p).unwrap();
    assert_eq!(rest.moves, full.moves, "the whole pocket is cut, as the plain pocket cuts it");
    assert!(rest.notes.iter().any(|n| n.contains("fits nowhere in this pocket")));
}

#[test]
fn a_neck_the_previous_tool_could_not_pass_is_rest_material() {
    // Two 30 x 30 bays joined by a 6 mm wide, 10 mm long neck.
    let region = poly(vec![
        Point2::new(0.0, 0.0),
        Point2::new(30.0, 0.0),
        Point2::new(30.0, 12.0),
        Point2::new(40.0, 12.0),
        Point2::new(40.0, 0.0),
        Point2::new(70.0, 0.0),
        Point2::new(70.0, 30.0),
        Point2::new(40.0, 30.0),
        Point2::new(40.0, 18.0),
        Point2::new(30.0, 18.0),
        Point2::new(30.0, 30.0),
        Point2::new(0.0, 30.0),
    ]);
    let plan = rest_plan(&region, TOOL_D, 0.0, &prev()).unwrap();
    // (A 12 mm tool noses 0.8 mm into each mouth, between the corners.)
    for x in [31.5, 35.0, 38.5] {
        for y in [12.1, 15.0, 17.9] {
            assert!(plan.rest.contains(Point2::new(x, y)), "neck point ({x}, {y}) is rest material");
        }
    }
    assert!(!plan.rest.contains(Point2::new(15.0, 15.0)), "the bay floor was cleared");
    // No slivers in the center area: one body for the neck, one per
    // bay corner.
    assert_eq!(plan.center.loops().len(), 9, "center areas");
    assert!(plan.center.loops().iter().all(|l| l.area().abs() > 1.0), "a sliver in the center area");
    let p = pp(PocketStrategy::Waterline);
    let path = rest_pocket(&region, &p, &prev()).unwrap();
    assert_eq!(check(&path, &region, TOOL_D * 0.5), Vec::new());
    let cuts = cuts_at(&path, p.bottom_z);
    let mid = Point2::new(35.0, 15.0);
    assert!(cuts.iter().any(|&(a, b)| seg_dist(mid, a, b) <= TOOL_D * 0.5), "the neck is cut through");
    for m in &path.moves {
        if m.kind == MoveKind::Rapid && (m.from.z < p.top_z - 1e-9 || m.to.z < p.top_z - 1e-9) {
            assert!(lifts_out(m), "a rapid moves below the top of the cut: {m:?}");
        }
    }
}

#[test]
fn allowance_stock_and_corner_radius_only_grow_the_rest_area() {
    let region = rounded(W, H, CORNER);
    let base = rest_plan(&region, TOOL_D, 0.0, &prev()).unwrap().rest;
    let variants = [
        RestParams { allowance: 0.5, ..prev() },
        RestParams { previous_stock_to_leave: 0.3, ..prev() },
        RestParams { previous_corner_radius: 1.0, ..prev() },
    ];
    for v in variants {
        let grown = rest_plan(&region, TOOL_D, 0.0, &v).unwrap().rest;
        assert!(grown.area().abs() > base.area().abs() + 0.5, "{v:?}: {} mm2", grown.area());
        // Nothing of the base area is lost (20 um for the offset chords).
        let lost = base.difference(&grown).unwrap().offset(-0.02).unwrap();
        assert!(lost.is_empty(), "{v:?}: the grown rest area dropped part of the base one");
        let within = grown.difference(&region).unwrap().offset(-0.02).unwrap();
        assert!(within.is_empty(), "{v:?}: the rest area leaves the pocket");
    }
    // Stock left on the walls is rest material all the way round.
    let walls = rest_plan(&region, TOOL_D, 0.0, &variants[1]).unwrap().rest;
    assert!(walls.contains(Point2::new(30.0, 0.15)) && walls.contains(Point2::new(0.15, 20.0)));
}

#[test]
fn degenerate_pockets_error_and_never_panic() {
    let p = pp(PocketStrategy::Waterline);
    // Narrower than this tool.
    let sliver = rounded(20.0, 3.0, 0.5);
    assert!(invalid(rest_pocket(&sliver, &p, &prev())).contains("narrower than the tool"));
    // Sub-micron and huge shapes: any answer but a panic or a hang.
    for (w, h) in [(1e-4, 1e-4), (1e5, 1e5), (60.0, 4.0 + 1e-7)] {
        let _ = rest_pocket(&rounded(w, h, w.min(h) * 0.1), &p, &prev());
    }
    let bad = PocketParams { stepover: 0.0, ..p };
    assert!(rest_pocket(&rounded(W, H, CORNER), &bad, &prev()).is_err());
}

/// Measure total XY length of flat feed moves below `top_z` whose
/// sub-segments (sampled every 0.05 mm) lie outside `zone`.
fn cleared_floor_feed_len(path: &Toolpath, top_z: f64, zone: &Region) -> f64 {
    let mut air = 0.0;
    for m in &path.moves {
        if m.kind != MoveKind::Feed || (m.from.z - m.to.z).abs() >= 1e-9 || m.from.z >= top_z - 1e-9 {
            continue;
        }
        for (a, b) in m.chords(0.005) {
            let len = (b.x - a.x).hypot(b.y - a.y);
            if len <= 1e-9 {
                continue;
            }
            let steps = (len / 0.05).ceil().max(1.0) as usize;
            let step = len / steps as f64;
            for k in 0..steps {
                let t = (k as f64 + 0.5) / steps as f64;
                let mid = Point2::new(a.x + (b.x - a.x) * t, a.y + (b.y - a.y) * t);
                if !zone.contains(mid) {
                    air += step;
                }
            }
        }
    }
    air
}

#[test]
fn rest_rings_do_not_feed_over_cleared_floor() {
    let region = rounded(W, H, CORNER);
    let plan = rest_plan(&region, TOOL_D, 0.0, &prev()).unwrap();
    let allowed = plan.rest.offset(TOOL_D * 0.5 + REST_REACH * 0.5 + 0.005).unwrap();
    for strategy in [PocketStrategy::Waterline, PocketStrategy::OffsetRings, PocketStrategy::Auto] {
        for climb in [true, false] {
            for entry in [Entry3D::Plunge, Entry3D::Helix, Entry3D::Ramp] {
                let p = PocketParams { strategy, climb, entry, ..pp(strategy) };
                let path = rest_pocket(&region, &p, &prev()).unwrap();
                assert_eq!(check(&path, &region, TOOL_D * 0.5), Vec::new(), "{strategy:?} climb={climb} entry={entry:?}");
                let air = cleared_floor_feed_len(&path, p.top_z, &allowed);
                assert!(air < 0.05, "{strategy:?} climb={climb} entry={entry:?}: {air:.2} mm cleared-floor feed");
                let cuts = cuts_at(&path, p.bottom_z);
                let reach = TOOL_D * 0.5 + 0.02;
                let uncut = grid()
                    .filter(|&q| certain_residue(q))
                    .filter(|&q| !cuts.iter().any(|&(a, b)| seg_dist(q, a, b) <= reach))
                    .count();
                assert_eq!(uncut, 0, "{strategy:?} climb={climb} entry={entry:?}: residue left uncut");
            }
        }
    }
}

#[test]
fn deep_corner_and_filleted_turns_clear_residue_without_cleared_floor_laps() {
    for (cr, prev_d) in [(2.2, 20.0), (0.25, 16.0)] {
        let region = rounded(40.0, 30.0, cr);
        let rest = RestParams { previous_tool_diameter: prev_d, ..prev() };
        let plan = rest_plan(&region, TOOL_D, 0.0, &rest).unwrap();
        let allowed = plan.rest.offset(TOOL_D * 0.5 + REST_REACH * 0.5 + 0.005).unwrap();
        let own_center = region.offset(-(TOOL_D * 0.5)).unwrap();
        let reachable = own_center.offset(TOOL_D * 0.5).unwrap();
        let check_area = plan.rest.intersect(&reachable).unwrap().offset(-0.02).unwrap();
        for blend_turns in [false, true] {
            for climb in [true, false] {
                for links in [camcraft_motion::links::Links::PLANAR, camcraft_motion::links::Links::ROUGH3D] {
                    let p = PocketParams { blend_turns, climb, links, ..pp(PocketStrategy::Waterline) };
                    let path = rest_pocket(&region, &p, &rest).unwrap();
                    assert_eq!(check(&path, &region, TOOL_D * 0.5), Vec::new(), "cr={cr} blend={blend_turns} climb={climb} keep_down={}", links.pass == camcraft_motion::links::Link::KeepDown);
                    let air = cleared_floor_feed_len(&path, p.top_z, &allowed);
                    assert!(air < 0.05, "cr={cr} blend={blend_turns} climb={climb}: {air:.2} mm cleared-floor feed");
                    if !blend_turns || cr > TOOL_D * 0.5 {
                        let cuts = cuts_at(&path, p.bottom_z);
                        let reach = TOOL_D * 0.5 + 0.02;
                        for i in 0..=400 {
                            for j in 0..=300 {
                                let q = Point2::new(f64::from(i) * 0.1, f64::from(j) * 0.1);
                                if check_area.contains(q) {
                                    assert!(
                                        cuts.iter().any(|&(a, b)| seg_dist(q, a, b) <= reach),
                                        "cr={cr} blend={blend_turns} climb={climb}: uncut deep residue at ({:.1}, {:.1})",
                                        q.x,
                                        q.y
                                    );
                                }
                            }
                        }
                    } else {
                        assert!(
                            path.notes.iter().any(|n| n.contains("blend")),
                            "cr={cr} blend=true climb={climb}: expected corner blend note: {:?}",
                            path.notes
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn previous_stock_to_leave_drops_cleared_floor_hole_lap() {
    let region = rounded(W, H, CORNER);
    for leave in [0.3, 5.8] {
        let rest = RestParams { previous_stock_to_leave: leave, ..prev() };
        let plan = rest_plan(&region, TOOL_D, 0.0, &rest).unwrap();
        let allowed = plan.rest.offset(TOOL_D * 0.5 + REST_REACH * 0.5 + 0.005).unwrap();
        let p = pp(PocketStrategy::Waterline);
        let path = rest_pocket(&region, &p, &rest).unwrap();
        assert_eq!(check(&path, &region, TOOL_D * 0.5), Vec::new(), "leave={leave}");
        let air = cleared_floor_feed_len(&path, p.top_z, &allowed);
        assert!(air < 0.05, "leave={leave}: {air:.2} mm cleared-floor feed");
        let cuts = cuts_at(&path, p.bottom_z);
        let reach = TOOL_D * 0.5 + 0.02;
        let core = plan.rest.offset(-0.02).unwrap();
        for i in 0..=300 {
            for j in 0..=200 {
                let q = Point2::new(f64::from(i) * 0.2, f64::from(j) * 0.2);
                if core.contains(q) {
                    assert!(
                        cuts.iter().any(|&(a, b)| seg_dist(q, a, b) <= reach),
                        "leave={leave}: uncut wall stock at ({:.1}, {:.1})",
                        q.x,
                        q.y
                    );
                }
            }
        }
    }
}
