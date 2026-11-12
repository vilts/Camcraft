//! M11.3 owed (RECOMMENDED "M11.3 B-D" (4)): the centre-cutting law holds
//! between passes, not just on a level's entry. A tool that is not
//! centre-cutting never feeds axially into solid anywhere in a path -
//! zigzag row restarts, waterline ring-to-ring dives, slot rows, bore and
//! circular re-entries, the 3-D ladder's pass hops - or the operation is
//! refused by name. The judge here is this file's own: every axial
//! descent (a cutting move with no XY run that goes down below the top of
//! the material) must land on a column an earlier cut at or below its
//! depth already passed within half a tool radius of; a centre-cutting
//! tool's paths show the judge is not vacuous.
//!
//! Gate map (M11.3): covers camcraft_motion::plunge_law,
//! camcraft_motion::machining2d (descent, descent_dive, ladder_links,
//! levels, contour_chain, contour_link), camcraft_motion::machining3d
//! (descent, level_ctx, ladder).

use camcraft_motion::approach::{Approach, CENTER_CUT_REFUSAL};
use camcraft_motion::cl::{Move, MoveKind, Toolpath};
use camcraft_motion::entry::Entry3D;
use camcraft_motion::geo2d::{Loop, Point2, Region};
use camcraft_motion::geometry::{Point3, Triangle};
use camcraft_motion::links::{Link, Links};
use camcraft_motion::machining2d::{
    BoreHole, BoreParams, CircularParams, GenError, PocketParams, PocketStrategy, SlotParams, bore, circular, pocket,
    slot,
};
use camcraft_motion::machining3d::{Pocket3DParams, pocket3d};
use camcraft_motion::verify::{Mode, ToolGeom, VerifyContext, verify};

const TOP: f64 = 0.0;

fn rect(x0: f64, y0: f64, x1: f64, y1: f64) -> Loop {
    Loop::new(vec![Point2::new(x0, y0), Point2::new(x1, y0), Point2::new(x1, y1), Point2::new(x0, y1)]).unwrap()
}

fn with_island() -> Region {
    let mut hole = rect(15.0, 12.0, 25.0, 18.0).points().to_vec();
    hole.reverse();
    Region::new(vec![rect(0.0, 0.0, 40.0, 30.0), Loop::new(hole).unwrap()]).unwrap()
}

fn round(r: f64) -> Region {
    let pts = (0..256)
        .map(|k| {
            let t = std::f64::consts::TAU * f64::from(k) / 256.0;
            Point2::new(r * t.cos(), r * t.sin())
        })
        .collect();
    Region::new(vec![Loop::new(pts).unwrap()]).unwrap()
}

fn not_cc(links: Links) -> Links {
    Links { approach: Approach { center_cutting: false, ..links.approach }, ..links }
}

fn seg_dist(p: Point2, a: Point3, b: Point3) -> f64 {
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    let dd = dx * dx + dy * dy;
    let t = if dd > 0.0 { (((p.x - a.x) * dx + (p.y - a.y) * dy) / dd).clamp(0.0, 1.0) } else { 0.0 };
    (p.x - a.x - t * dx).hypot(p.y - a.y - t * dy)
}

fn axial(m: &Move, top: f64) -> bool {
    m.kind != MoveKind::Rapid && m.from.x == m.to.x && m.from.y == m.to.y && m.to.z < m.from.z && m.to.z < top - 1e-6
}

/// Every axial descent into solid in `t`: the column under it was not
/// cut, at or below its depth, by an earlier move within `r / 2`.
fn plunges_into_solid(t: &Toolpath, r: f64, top: f64) -> Vec<Move> {
    let mut out = Vec::new();
    for (i, m) in t.moves.iter().enumerate() {
        if !axial(m, top) {
            continue;
        }
        let xy = Point2::new(m.to.x, m.to.y);
        let cleared = t.moves[..i].iter().filter(|q| q.kind != MoveKind::Rapid).any(|q| {
            q.segments3(0.005)
                .iter()
                .any(|(a, b)| a.z.max(b.z) <= m.to.z + 1e-6 && seg_dist(xy, *a, *b) <= r * 0.5)
        });
        if !cleared {
            out.push(*m);
        }
    }
    out
}

/// The law's verdict on one generated path: clean, or refused by name.
fn judge(what: &str, res: Result<Toolpath, GenError>, r: f64, top: f64) -> Option<Toolpath> {
    match res {
        Ok(t) => {
            let bad = plunges_into_solid(&t, r, top);
            assert!(bad.is_empty(), "{what}: axial plunges into solid {bad:?}\nnotes {:?}", t.notes);
            Some(t)
        }
        Err(e) => {
            assert!(format!("{e:?}").contains(CENTER_CUT_REFUSAL), "{what}: refused, but not by the law: {e:?}");
            None
        }
    }
}

fn second_dive(t: &Toolpath) -> bool {
    t.notes.iter().any(|n| n.contains("second dive"))
}

fn pp(entry: Entry3D, strategy: PocketStrategy, links: Links) -> PocketParams {
    PocketParams {
        tool_diameter: 6.0,
        stepover: 2.5,
        stock_to_leave: 0.0,
        clearance_z: 25.0,
        top_z: TOP,
        bottom_z: -6.0,
        stepdown: 2.0,
        feed: 500.0,
        plunge_feed: 100.0,
        climb: true,
        strategy,
        retract_z: Some(15.0),
        entry,
        helix_radius: 1.5,
        optimal_load: camcraft_motion::load::DEFAULT_OPTIMAL_LOAD,
        blend_turns: false,
        links,
    }
}

fn check(t: &Toolpath, r: &Region) {
    let tool = ToolGeom {
        radius: 3.0,
        nose_radius: 0.0,
        flute_len: 30.0,
        shank_radius: 3.0,
        shank_len: 40.0,
        holder_radius: 20.0,
        holder_len: 60.0,
    };
    let cx = VerifyContext {
        region: r,
        stock: None,
        model: None,
        tool: &tool,
        cutter: None,
        mode: Mode::Area,
        stock_top_z: TOP,
        clearance_z: 25.0,
        stock_to_leave: 0.0,
        gouge_tol: 0.01,
        z_tol: 1e-6,
        finish_tol: None,
        fixtures: &[],
        spindle: None,
        ..Default::default()
    };
    assert_eq!(verify(t, &cx), Vec::new(), "{:?}", t.notes);
}

fn link_sets() -> Vec<(&'static str, Links)> {
    vec![
        ("planar", Links::PLANAR),
        ("clearance", Links { pass: Link::Clearance, ..Links::PLANAR }),
        ("ramp transition", Links { transition: Some(Entry3D::Ramp), ..Links::PLANAR }),
        ("keep down", Links { level: Link::KeepDown, pass: Link::KeepDown, ..Links::PLANAR }),
    ]
}

#[test]
fn the_judge_sees_the_plunges_a_centre_cutting_tool_takes_between_passes() {
    // Not vacuous: the island splits the zigzag into rows, and the
    // centre-cutting tool climbs and dives between them into solid.
    let r = with_island();
    let t = pocket(&r, &pp(Entry3D::Plunge, PocketStrategy::Zigzag, Links::PLANAR)).expect("pocket");
    let bad = plunges_into_solid(&t, 3.0, TOP);
    assert!(bad.len() > 3, "the level entries and some row restarts dive into solid: {bad:?}");
}

#[test]
fn a_tool_that_is_not_centre_cutting_never_plunges_into_a_pocket_between_passes() {
    let mut restarted = 0;
    let mut kept = 0;
    for (rn, r) in [("island", with_island()), ("round", round(15.0))] {
        for strategy in [PocketStrategy::Zigzag, PocketStrategy::Waterline] {
            for entry in [Entry3D::Plunge, Entry3D::Helix, Entry3D::Ramp, Entry3D::Arc] {
                for (ln, links) in link_sets() {
                    let what = format!("{rn} {strategy:?} {entry:?} {ln}");
                    // Wall-start rows: one entry per level, no second dive,
                    // for either tool.
                    let cc = pocket(&r, &pp(entry, strategy, links)).expect("a centre-cutting pocket");
                    assert!(!second_dive(&cc), "{what}: {:?}", cc.notes);
                    if let Some(t) = judge(&what, pocket(&r, &pp(entry, strategy, not_cc(links))), 3.0, TOP) {
                        assert!(!second_dive(&t), "{what}: {:?}", t.notes);
                        check(&t, &r);
                        kept += 1;
                        restarted += usize::from(t.notes.iter().any(|n| n.contains("pass start inside a level")));
                    }
                }
            }
        }
    }
    assert!(kept > 20, "most configurations find a legal entry: {kept}");
    assert!(restarted > 0, "some pass start inside a level took the helix or ramp");
}

#[test]
fn a_tool_that_is_not_centre_cutting_never_plunges_into_a_slot() {
    for (w, d, so) in [(20.0, 6.0, 4.0), (12.0, 6.0, 3.0), (30.0, 10.0, 6.0), (10.0, 10.0, 5.0)] {
        for entry in [Entry3D::Plunge, Entry3D::Helix, Entry3D::Ramp, Entry3D::Arc] {
            for (ln, links) in link_sets() {
                let p = SlotParams {
                    links: not_cc(links),
                    tool_diameter: d,
                    width: w,
                    stock_to_leave: 0.0,
                    clearance_z: 5.0,
                    retract_z: None,
                    top_z: TOP,
                    bottom_z: -6.0,
                    stepdown: 3.0,
                    stepover: so,
                    feed: 500.0,
                    plunge_feed: 100.0,
                    climb: true,
                    entry,
                    helix_radius: 1.5,
                };
                let what = format!("slot w{w} d{d} {entry:?} {ln}");
                let (a, b) = (Point2::new(0.0, 0.0), Point2::new(40.0, 10.0));
                // The slot's rows start on the channel wall: no second dive.
                let cc = slot(a, b, &SlotParams { links, ..p }).expect("a centre-cutting slot");
                assert!(!second_dive(&cc), "{what}: {:?}", cc.notes);
                if let Some(t) = judge(&what, slot(a, b, &p), d * 0.5, TOP) {
                    assert!(!second_dive(&t), "{what}: {:?}", t.notes);
                }
            }
        }
    }
}

#[test]
fn a_tool_that_is_not_centre_cutting_never_plunges_into_a_bore_or_circular() {
    for entry in [Entry3D::Plunge, Entry3D::Helix, Entry3D::Ramp, Entry3D::Arc] {
        for (ln, links) in link_sets() {
            let bp = BoreParams {
                holes: vec![
                    BoreHole { xy: Point2::new(0.0, 0.0), radius: 10.0, top_z: TOP, floor_z: -6.0, through: false, stepped: false },
                    BoreHole { xy: Point2::new(40.0, 0.0), radius: 6.0, top_z: TOP, floor_z: -4.0, through: false, stepped: false },
                ],
                tool_diameter: 6.0,
                stock_to_leave: 0.0,
                stepdown: 2.0,
                through_allowance: 0.5,
                clearance_z: 25.0,
                retract_z: Some(15.0),
                feed: 500.0,
                plunge_feed: 100.0,
                climb: true,
                tolerance: 0.01,
                entry,
                helix_radius: 1.5,
                links: not_cc(links),
            };
            judge(&format!("bore {entry:?} {ln}"), bore(&bp), 3.0, TOP);
            for stepover in [2.0, 3.5] {
                let cp = CircularParams {
                    links: not_cc(links),
                    center: Point2::new(0.0, 0.0),
                    radius: 15.0,
                    vault: false,
                    bottom_z: 10.0,
                    top_z: 20.0,
                    tool_diameter: 8.0,
                    stock_to_leave: 0.0,
                    stepover,
                    stepdown: 5.0,
                    clearance_z: 30.0,
                    retract_z: None,
                    feed: 800.0,
                    plunge_feed: 200.0,
                    climb: true,
                    entry,
                    helix_radius: 1.0,
                };
                judge(&format!("circular so{stepover} {entry:?} {ln}"), circular(&round(15.0), &cp), 4.0, 20.0);
            }
        }
    }
}

fn box_facets(min: [f64; 3], max: [f64; 3]) -> Vec<Triangle> {
    let v = |i: usize| {
        Point3::new([min[0], max[0]][i & 1], [min[1], max[1]][(i >> 1) & 1], [min[2], max[2]][(i >> 2) & 1])
    };
    let quad = |a: usize, b: usize, c: usize, d: usize| {
        [Triangle::new(v(a), v(b), v(c)), Triangle::new(v(a), v(c), v(d))]
    };
    let mut t = Vec::with_capacity(12);
    for q in [quad(4, 5, 7, 6), quad(0, 2, 3, 1), quad(0, 1, 5, 4), quad(2, 6, 7, 3), quad(1, 3, 7, 5), quad(2, 0, 4, 6)] {
        t.extend(q);
    }
    t
}

#[test]
fn the_engagement_limit_slows_the_first_diameter_of_each_level_then_lets_go() {
    // M11.3 D owed: over the limit, the descent, its link AND the first
    // tool diameter of the first cut run at the scaled feed; the rest of
    // the level at the cutting feed. Off, nothing planar is slowed.
    let r = Region::new(vec![rect(0.0, 0.0, 40.0, 30.0)]).unwrap();
    for entry in [Entry3D::Helix, Entry3D::Plunge, Entry3D::Ramp] {
        let links = Links { approach: Approach { max_engagement: 0.5, ..Approach::DEFAULT }, ..Links::PLANAR };
        let t = pocket(&r, &pp(entry, PocketStrategy::Zigzag, links)).expect("pocket");
        let off = pocket(&r, &pp(entry, PocketStrategy::Zigzag, Links::PLANAR)).expect("pocket");
        let flat = |m: &&Move| m.kind == MoveKind::Feed && m.from.z == m.to.z;
        assert!(off.moves.iter().filter(flat).all(|m| m.feed == Some(500.0)), "{entry:?}");
        for z in [0.0, -2.0, -4.0, -6.0] {
            let planar: Vec<&Move> = t.moves.iter().filter(flat).filter(|m| m.from.z == z).collect();
            let slowed: f64 = planar
                .iter()
                .take_while(|m| m.feed == Some(250.0))
                .map(|m| (m.to.x - m.from.x).hypot(m.to.y - m.from.y))
                .sum();
            assert!(slowed >= 6.0 - 1e-6, "{entry:?} level {z}: only {slowed} mm slowed");
            let rest: Vec<_> = planar.iter().skip_while(|m| m.feed == Some(250.0)).collect();
            assert!(!rest.is_empty() && rest.iter().all(|m| m.feed == Some(500.0)), "{entry:?} level {z}: {rest:?}");
        }
    }
}

#[test]
fn a_tool_that_is_not_centre_cutting_never_plunges_on_the_3d_ladder() {
    // The slab with a boss: every level's rings and rows split around it,
    // so the pass hops dive at pass starts the level has not cleared.
    let mut facets = box_facets([-20.0, -20.0, -20.0], [20.0, 20.0, -10.0]);
    facets.extend(box_facets([-5.0, -5.0, -10.0], [5.0, 5.0, 0.0]));
    let footprint = Region::new(vec![rect(-20.0, -20.0, 20.0, 20.0)]).unwrap();
    let mut cc_bad = 0;
    for strategy in [PocketStrategy::Zigzag, PocketStrategy::Waterline] {
        for entry in [Entry3D::Plunge, Entry3D::Helix, Entry3D::Ramp, Entry3D::Arc] {
            for (ln, links) in [
                ("rough3d", Links::ROUGH3D),
                ("retract", Links { pass: Link::Retract, ..Links::ROUGH3D }),
                ("clearance", Links { pass: Link::Clearance, level: Link::Clearance, ..Links::ROUGH3D }),
            ] {
                let p = Pocket3DParams {
                    links,
                    tool_diameter: 8.0,
                    stepover: 3.0,
                    stock_to_leave: 0.0,
                    clearance_z: 5.0,
                    top_z: TOP,
                    bottom_z: -10.0,
                    stepdown: 5.0,
                    feed: 500.0,
                    plunge_feed: 100.0,
                    climb: true,
                    strategy,
                    retract_z: Some(3.0),
                    entry,
                    helix_radius: 2.0,
                    optimal_load: camcraft_motion::load::DEFAULT_OPTIMAL_LOAD,
                };
                if let Ok(t) = pocket3d(&facets, &footprint, &p) {
                    cc_bad += plunges_into_solid(&t, 4.0, TOP).len();
                }
                let q = Pocket3DParams { links: not_cc(links), ..p };
                judge(&format!("3d {strategy:?} {entry:?} {ln}"), pocket3d(&facets, &footprint, &q), 4.0, TOP);
            }
        }
    }
    assert!(cc_bad > 0, "the judge sees the centre-cutting tool's pass-hop dives");
}
