//! Level transitions and links (M11.2 C/D): every descent between levels
//! (plunge, angle-bounded ramp, pitched helix, vertical arc) and every
//! link mode (clearance, retract plane, keep down) on a representative
//! fixture per ladder family - the move sequences the user asked for,
//! the verifier clean, every fallback named, and the defaults posting the
//! pre-M11.2 ladder unchanged.
//!
//! Gate map (M11.2): covers camcraft_motion::links,
//! camcraft_motion::machining2d (levels, descent, contour_link,
//! contour_step, contour_chain), camcraft_motion::machining3d (ladder,
//! descent, level_ctx), camcraft_motion::entry (helix_turns_down).

use camcraft_motion::cl::{Move, MoveKind, Toolpath};
use camcraft_motion::entry::Entry3D;
use camcraft_motion::geo2d::{Loop, Point2, Region};
use camcraft_motion::geometry::{Point3, Triangle};
use camcraft_motion::links::{Link, Links};
use camcraft_motion::machining2d::{
    bore, contour, pocket, BoreHole, BoreParams, ContourParams, Lead, PocketParams, PocketStrategy,
    Side,
};
use camcraft_motion::machining3d::{pocket3d, Pocket3DParams};
use camcraft_motion::verify::{verify, Mode, ToolGeom, VerifyContext, Warning};

const CLEAR: f64 = 5.0;
const HOP: f64 = 2.0;

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

/// A 60 x 40 pocket around a 20 x 10 island: the zigzag rows split
/// around it, so a level has several passes to link.
fn holed() -> Region {
    let outer = Loop::new(vec![
        Point2::new(0.0, 0.0),
        Point2::new(60.0, 0.0),
        Point2::new(60.0, 40.0),
        Point2::new(0.0, 40.0),
    ])
    .unwrap();
    let island = Loop::new(vec![
        Point2::new(20.0, 15.0),
        Point2::new(20.0, 25.0),
        Point2::new(40.0, 25.0),
        Point2::new(40.0, 15.0),
    ])
    .unwrap();
    Region::new(vec![outer, island]).unwrap()
}

fn disk(cx: f64, r: f64) -> Region {
    let pts = (0..256)
        .map(|k| {
            let t = std::f64::consts::TAU * f64::from(k) / 256.0;
            Point2::new(cx + r * t.cos(), r * t.sin())
        })
        .collect();
    Region::new(vec![Loop::new(pts).unwrap()]).unwrap()
}

fn tool(r: f64) -> ToolGeom {
    ToolGeom {
        radius: r,
        nose_radius: 0.0,
        flute_len: 30.0,
        shank_radius: r,
        shank_len: 40.0,
        holder_radius: 20.0,
        holder_len: 60.0,
    }
}

fn check(path: &Toolpath, region: &Region, model: Option<&[Triangle]>, mode: Mode, r: f64) -> Vec<Warning> {
    let t = tool(r);
    let cx = VerifyContext {
        region,
        stock: None,
        model,
        tool: &t,
        cutter: None,
        mode,
        stock_top_z: 0.0,
        clearance_z: CLEAR,
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

/// A planar cutting move: a feed (line or arc) at constant Z.
fn planar(m: &Move) -> bool {
    m.kind == MoveKind::Feed && m.from.z == m.to.z
}

/// The moves between the cutting at one level and the cutting at the
/// next (deeper) one: `(above, below, moves)`.
fn transitions(t: &Toolpath) -> Vec<(f64, f64, Vec<Move>)> {
    let mut out = Vec::new();
    let mut level: Option<f64> = None;
    let mut between: Vec<Move> = Vec::new();
    for m in &t.moves {
        if planar(m) && m.to.z < CLEAR - 1e-9 {
            match level {
                Some(z) if m.to.z < z - 1e-9 => {
                    out.push((z, m.to.z, std::mem::take(&mut between)));
                    level = Some(m.to.z);
                }
                Some(z) if (m.to.z - z).abs() <= 1e-9 => between.clear(),
                Some(_) => between.clear(),
                None => {
                    level = Some(m.to.z);
                    between.clear();
                }
            }
        } else {
            between.push(*m);
        }
    }
    out
}

/// The moves between two cuts of one level: `(z, moves)` for every gap
/// that holds something other than planar feeds.
fn pass_gaps(t: &Toolpath) -> Vec<(f64, Vec<Move>)> {
    let mut out = Vec::new();
    let mut level: Option<f64> = None;
    let mut between: Vec<Move> = Vec::new();
    for m in &t.moves {
        if planar(m) && m.to.z < CLEAR - 1e-9 {
            if level.is_some_and(|z| (m.to.z - z).abs() <= 1e-9) && !between.is_empty() {
                out.push((m.to.z, std::mem::take(&mut between)));
            }
            between.clear();
            level = Some(m.to.z);
        } else {
            between.push(*m);
        }
    }
    out
}

fn top(ms: &[Move]) -> f64 {
    ms.iter().map(|m| m.to.z.max(m.from.z)).fold(f64::NEG_INFINITY, f64::max)
}

fn pp(entry: Entry3D, links: Links) -> PocketParams {
    PocketParams {
        tool_diameter: 6.0,
        stepover: 3.0,
        stock_to_leave: 0.0,
        clearance_z: CLEAR,
        top_z: 0.0,
        bottom_z: -4.0,
        stepdown: 2.0,
        feed: 500.0,
        plunge_feed: 100.0,
        climb: true,
        strategy: PocketStrategy::Waterline,
        retract_z: Some(HOP),
        entry,
        helix_radius: 2.0,
        optimal_load: camcraft_motion::load::DEFAULT_OPTIMAL_LOAD,
        blend_turns: false,
        links,
    }
}

#[test]
fn same_as_entry_is_the_entry_byte_for_byte() {
    let r = rect(0.0, 0.0, 60.0, 40.0);
    for entry in [Entry3D::Plunge, Entry3D::Ramp, Entry3D::Helix, Entry3D::Arc] {
        let base = pocket(&r, &pp(entry, Links::PLANAR)).expect("pocket");
        // Naming the entry's own arm with the entry's own law (pitch 0)
        // is the same descent; only the zig-zag ramp reads its angle.
        if entry != Entry3D::Ramp {
            let same = pocket(&r, &pp(entry, Links { transition: Some(entry), ..Links::PLANAR })).expect("pocket");
            assert_eq!(base.moves, same.moves, "{entry:?}");
        }
    }
}

#[test]
fn a_ramp_transition_descends_at_no_more_than_the_angle() {
    let r = rect(0.0, 0.0, 60.0, 40.0);
    for angle in [3.0_f64, 10.0] {
        let links = Links { transition: Some(Entry3D::Ramp), ramp_angle: angle, ..Links::PLANAR };
        let t = pocket(&r, &pp(Entry3D::Plunge, links)).expect("pocket");
        assert!(t.is_chained() && t.all_finite());
        assert_eq!(check(&t, &r, None, Mode::Area, 3.0), Vec::new(), "{:?}", t.notes);
        let tan = angle.to_radians().tan();
        let tr = transitions(&t);
        assert_eq!(tr.len(), 2, "two level changes");
        for (above, below, ms) in &tr {
            let ramp: Vec<&Move> = ms.iter().filter(|m| m.kind == MoveKind::Feed && m.to.z < m.from.z).collect();
            assert!(!ramp.is_empty(), "{above} -> {below}: a ramp descends");
            for m in &ramp {
                let run = (m.to.x - m.from.x).hypot(m.to.y - m.from.y);
                assert!(m.from.z - m.to.z <= run * tan + 1e-9, "steeper than {angle} deg: {m:?}");
            }
            assert!(!ms.iter().any(|m| m.kind == MoveKind::Plunge), "no plunge between levels: {ms:?}");
            assert_eq!(ramp.last().map(|m| m.to.z), Some(*below), "the ramp lands on the level");
        }
    }
}

#[test]
fn a_helix_transition_honours_its_radius_and_pitch() {
    let r = rect(0.0, 0.0, 60.0, 40.0);
    let links = Links { transition: Some(Entry3D::Helix), helix_pitch: 0.5, ..Links::PLANAR };
    let t = pocket(&r, &pp(Entry3D::Plunge, links)).expect("pocket");
    assert_eq!(check(&t, &r, None, Mode::Area, 3.0), Vec::new(), "{:?}", t.notes);
    for (above, below, ms) in transitions(&t) {
        let arcs: Vec<&Move> = ms.iter().filter(|m| m.arc.is_some()).collect();
        // 2 mm at 0.5 mm per turn: four turns, two half-turn arcs each.
        assert_eq!(arcs.len(), 8, "{above} -> {below}: {ms:?}");
        for w in arcs.windows(2) {
            assert_eq!(w[0].to, w[1].from, "the helix is continuous");
        }
        for (k, m) in arcs.iter().enumerate() {
            let c = m.arc.map(|a| a.center).expect("arc");
            let rad = (m.from.x - c.x).hypot(m.from.y - c.y);
            assert!((rad - 2.0).abs() < 1e-3, "the helix radius is the op's: {rad}");
            assert!(m.to.z < m.from.z, "every half turn descends");
            let want = above - 0.25 * (k as f64 + 1.0);
            assert!((m.to.z - want).abs() < 1e-6, "0.5 mm per full turn: {} vs {want}", m.to.z);
        }
        assert_eq!(arcs.last().map(|m| m.to.z), Some(below));
    }
    // A pitch that asks for more turns than the law allows substitutes,
    // named - never a hang.
    let silly = Links { transition: Some(Entry3D::Helix), helix_pitch: 1e-6, ..Links::PLANAR };
    let t = pocket(&r, &pp(Entry3D::Plunge, silly)).expect("pocket");
    assert!(t.notes.iter().any(|n| n.contains("pitch asks too many turns")), "{:?}", t.notes);
}

#[test]
fn level_and_pass_links_reach_their_planes() {
    let r = holed();
    let mut zig = pp(Entry3D::Plunge, Links::PLANAR);
    zig.strategy = PocketStrategy::Zigzag;
    for (level, want) in [(Link::Clearance, CLEAR), (Link::Retract, HOP)] {
        let t = pocket(&r, &PocketParams { links: Links { level, ..Links::PLANAR }, ..zig }).expect("pocket");
        for (above, below, ms) in transitions(&t) {
            assert_eq!(top(&ms), want, "{level:?}: {above} -> {below} lifts to {want}");
        }
    }
    for (pass, want) in [(Link::Clearance, CLEAR), (Link::Retract, HOP)] {
        let t = pocket(&r, &PocketParams { links: Links { pass, ..Links::PLANAR }, ..zig }).expect("pocket");
        let gaps = pass_gaps(&t);
        assert!(!gaps.is_empty());
        for (z, ms) in gaps {
            assert_eq!(top(&ms), want, "{pass:?}: a pass link at {z} lifts to {want}");
        }
    }
}

#[test]
fn keep_down_pass_links_stay_at_depth_where_proved_and_say_where_not() {
    let r = holed();
    let mut p = pp(Entry3D::Plunge, Links { pass: Link::KeepDown, ..Links::PLANAR });
    p.strategy = PocketStrategy::Zigzag;
    let t = pocket(&r, &p).expect("pocket");
    assert_eq!(check(&t, &r, None, Mode::Area, 3.0), Vec::new(), "{:?}", t.notes);
    let gaps = pass_gaps(&t);
    let lifted = gaps.iter().filter(|(z, ms)| top(ms) > z + 1e-9).count();
    let stayed = gaps.len() - lifted;
    let said = t.notes.iter().any(|n| n.starts_with("link:") && n.contains("keep-down"));
    assert!(lifted == 0 || said, "every lift is named: {:?}", t.notes);
    let base = pocket(&r, &PocketParams { links: Links::PLANAR, ..p }).expect("pocket");
    let rapids = |t: &Toolpath| t.moves.iter().filter(|m| m.kind == MoveKind::Rapid).count();
    assert!(stayed > 0 || rapids(&t) == rapids(&base), "keep-down found nothing to keep: {gaps:?}");
    assert!(rapids(&t) <= rapids(&base));
    // A stay-down limit shorter than every link lifts them all, named.
    let short = PocketParams { links: Links { pass: Link::KeepDown, max_stay_down: 1e-3, ..Links::PLANAR }, ..p };
    let t = pocket(&r, &short).expect("pocket");
    assert!(pass_gaps(&t).iter().all(|(z, ms)| top(ms) > z + 1e-9));
    assert!(t.notes.iter().any(|n| n.contains("maximum stay-down distance")), "{:?}", t.notes);
}

fn bore_params(links: Links) -> BoreParams {
    BoreParams {
        holes: vec![BoreHole {
            xy: Point2::new(0.0, 0.0),
            radius: 10.0,
            top_z: 0.0,
            floor_z: -6.0,
            through: false,
            stepped: false,
        }],
        tool_diameter: 6.0,
        stock_to_leave: 0.0,
        stepdown: 2.0,
        through_allowance: 0.5,
        clearance_z: CLEAR,
        retract_z: Some(HOP),
        feed: 500.0,
        plunge_feed: 100.0,
        climb: true,
        tolerance: 0.01,
        entry: Entry3D::Plunge,
        helix_radius: 1.5,
        links,
    }
}

#[test]
fn keep_down_between_levels_never_rises_above_the_level_just_cut() {
    let region = disk(0.0, 10.0);
    for arm in [Entry3D::Plunge, Entry3D::Ramp, Entry3D::Helix, Entry3D::Arc] {
        let links = Links { transition: Some(arm), level: Link::KeepDown, ..Links::PLANAR };
        let t = bore(&bore_params(links)).expect("bore");
        assert!(t.is_chained() && t.all_finite());
        assert_eq!(check(&t, &region, None, Mode::Area, 3.0), Vec::new(), "{arm:?}: {:?}", t.notes);
        let tr = transitions(&t);
        assert_eq!(tr.len(), 3, "{arm:?}");
        let said = t.notes.iter().any(|n| n.starts_with("link:"));
        for (above, below, ms) in &tr {
            assert!(
                top(ms) <= above + 1e-9 || said,
                "{arm:?}: {above} -> {below} rose to {} unnamed: {:?}",
                top(ms),
                t.notes
            );
            assert!(!ms.iter().any(|m| m.kind == MoveKind::Rapid && m.to.z < above - 1e-9 && m.from.z <= *above + 1e-9 && (m.to.x != m.from.x || m.to.y != m.from.y)),
                "{arm:?}: no rapid across at depth: {ms:?}");
        }
        if !said {
            // Stayed down at every level: no rapid at all between levels.
            assert!(tr.iter().all(|(_, _, ms)| ms.iter().all(|m| m.kind != MoveKind::Rapid)), "{arm:?}");
        }
    }
}

fn wall(entry: Entry3D, links: Links) -> ContourParams {
    ContourParams {
        tool_diameter: 6.0,
        stock_to_leave: 0.0,
        clearance_z: CLEAR,
        retract_z: HOP,
        top_z: 0.0,
        bottom_z: -6.0,
        stepdown: 2.0,
        feed: 500.0,
        plunge_feed: 100.0,
        climb: true,
        side: Side::Outside,
        lead: Lead::Arc { radius: 2.0 },
        tolerance: 0.01,
        tab_count: 0,
        tab_width: 0.0,
        tab_height: 0.0,
        entry,
        helix_radius: 1.0,
        blend_turns: false,
        links,
    }
}

#[test]
fn contour_keep_down_descends_on_the_ring_by_every_arm() {
    let r = disk(0.0, 20.0);
    for arm in [Entry3D::Plunge, Entry3D::Ramp, Entry3D::Helix, Entry3D::Arc] {
        let links = Links { transition: Some(arm), level: Link::KeepDown, helix_pitch: 1.0, ..Links::PLANAR };
        let t = contour(&r, &wall(Entry3D::Plunge, links)).expect("contour");
        assert_eq!(check(&t, &r, None, Mode::Boundary, 3.0), Vec::new(), "{arm:?}: {:?}", t.notes);
        let tr = transitions(&t);
        assert_eq!(tr.len(), 3, "{arm:?}");
        for (above, below, ms) in &tr {
            assert!(top(ms) <= above + 1e-9, "{arm:?}: {above} -> {below} stays down: {ms:?}");
            assert!(!ms.iter().any(|m| m.kind == MoveKind::Rapid), "{arm:?}: no rapid: {ms:?}");
            match arm {
                Entry3D::Plunge => assert!(ms.iter().any(|m| m.kind == MoveKind::Plunge)),
                Entry3D::Helix => {
                    // 2 mm at 1 mm per lap: two laps of the ring.
                    let len: f64 = ms.iter().map(|m| (m.to.x - m.from.x).hypot(m.to.y - m.from.y)).sum();
                    let lap = std::f64::consts::TAU * 23.0;
                    assert!((len / lap - 2.0).abs() < 0.05, "{arm:?}: {} laps", len / lap);
                }
                Entry3D::Ramp => {
                    let tan = 3.0_f64.to_radians().tan();
                    for m in ms {
                        let run = (m.to.x - m.from.x).hypot(m.to.y - m.from.y);
                        assert!(m.from.z - m.to.z <= run * tan + 1e-9, "{m:?}");
                    }
                }
                Entry3D::Arc => {
                    let named = t.notes.iter().any(|n| n.contains("no stay-down arc"));
                    let fan = ms.iter().filter(|m| m.kind == MoveKind::Feed && m.to.z < m.from.z).count();
                    assert!(fan >= 3 || named, "{arm:?}: an arc fan or a named plunge: {ms:?}");
                }
            }
            assert_eq!(ms.last().map(|m| m.to.z), Some(*below));
        }
    }
}

#[test]
fn contour_level_retract_lifts_to_the_retract_plane_only() {
    let r = disk(0.0, 20.0);
    let t = contour(&r, &wall(Entry3D::Plunge, Links { level: Link::Retract, ..Links::PLANAR })).expect("contour");
    assert_eq!(check(&t, &r, None, Mode::Boundary, 3.0), Vec::new(), "{:?}", t.notes);
    for (above, below, ms) in transitions(&t) {
        assert_eq!(top(&ms), HOP, "{above} -> {below}");
    }
    let base = contour(&r, &wall(Entry3D::Plunge, Links::PLANAR)).expect("contour");
    for (above, below, ms) in transitions(&base) {
        assert_eq!(top(&ms), CLEAR, "the default still lifts to clearance: {above} -> {below}");
    }
}

#[test]
fn contour_ring_to_ring_keep_down_is_proved_or_named() {
    // Two bosses, one level ladder: the ring-to-ring link at each level.
    let r = Region::new(vec![
        Loop::new(disk(0.0, 8.0).loops()[0].points().to_vec()).unwrap(),
        Loop::new(disk(40.0, 8.0).loops()[0].points().to_vec()).unwrap(),
    ])
    .unwrap();
    for (pass, max) in [(Link::KeepDown, 0.0), (Link::KeepDown, 1.0), (Link::Clearance, 0.0)] {
        let links = Links { pass, max_stay_down: max, ..Links::PLANAR };
        let t = contour(&r, &wall(Entry3D::Plunge, links)).expect("contour");
        assert_eq!(check(&t, &r, None, Mode::Boundary, 3.0), Vec::new(), "{pass:?}: {:?}", t.notes);
        let gaps = pass_gaps(&t);
        // A long planar feed at depth is the kept-down link itself.
        let long_link = t.moves.iter().any(|m| {
            planar(m) && m.arc.is_none() && (m.to.x - m.from.x).hypot(m.to.y - m.from.y) > 15.0
        });
        match (pass, max > 0.0) {
            (Link::KeepDown, false) => {
                assert!(gaps.is_empty(), "stays down between the bosses at every level: {gaps:?}");
                assert!(long_link, "the ring-to-ring link feeds across at depth");
            }
            (Link::KeepDown, true) => {
                assert_eq!(gaps.len(), 4, "every level's link lifts");
                assert!(gaps.iter().all(|(_, ms)| top(ms) >= HOP), "too long for the limit: lifts");
                assert!(t.notes.iter().any(|n| n.contains("maximum stay-down")), "{:?}", t.notes);
            }
            _ => {
                assert_eq!(gaps.len(), 4);
                assert!(gaps.iter().all(|(_, ms)| top(ms) == CLEAR));
                assert!(!long_link);
            }
        }
    }
}

fn box_facets(min: [f64; 3], max: [f64; 3]) -> Vec<Triangle> {
    let p = |x, y, z| Point3::new(x, y, z);
    let v = |i: usize| p([min[0], max[0]][i & 1], [min[1], max[1]][(i >> 1) & 1], [min[2], max[2]][(i >> 2) & 1]);
    let quad = |a: usize, b: usize, c: usize, d: usize| [Triangle::new(v(a), v(b), v(c)), Triangle::new(v(a), v(c), v(d))];
    let mut t = Vec::new();
    for q in [quad(4, 5, 7, 6), quad(0, 2, 3, 1), quad(0, 1, 5, 4), quad(2, 6, 7, 3), quad(1, 3, 7, 5), quad(2, 0, 4, 6)] {
        t.extend(q);
    }
    t
}

fn p3(links: Links) -> Pocket3DParams {
    Pocket3DParams {
        tool_diameter: 8.0,
        stepover: 3.0,
        stock_to_leave: 0.0,
        clearance_z: CLEAR,
        top_z: 0.0,
        bottom_z: -10.0,
        stepdown: 2.5,
        feed: 500.0,
        plunge_feed: 100.0,
        climb: true,
        strategy: PocketStrategy::Waterline,
        retract_z: Some(HOP),
        entry: Entry3D::Plunge,
        helix_radius: 1.0,
        optimal_load: camcraft_motion::load::DEFAULT_OPTIMAL_LOAD,
        links,
    }
}

#[test]
fn the_3d_ladder_takes_the_same_choices() {
    let f = box_facets([-20.0, -20.0, -20.0], [20.0, 20.0, -10.0]);
    let region = camcraft_motion::geometry::slice::project(&f, 64).unwrap();
    let base = pocket3d(&f, &region, &p3(Links::ROUGH3D)).expect("3d");
    // The default is the pre-M11.2 ladder: same as naming the entry.
    let same = pocket3d(&f, &region, &p3(Links { transition: Some(Entry3D::Plunge), ..Links::ROUGH3D })).expect("3d");
    assert_eq!(base.moves, same.moves);
    // Retract between levels.
    let t = pocket3d(&f, &region, &p3(Links { level: Link::Retract, ..Links::ROUGH3D })).expect("3d");
    assert_eq!(check(&t, &region, Some(&f), Mode::Area, 4.0), Vec::new(), "{:?}", t.notes);
    for (above, below, ms) in transitions(&t) {
        assert_eq!(top(&ms), HOP, "{above} -> {below}");
    }
    // A zig-zag ramp at 5 degrees between levels.
    let t = pocket3d(&f, &region, &p3(Links { transition: Some(Entry3D::Ramp), ramp_angle: 5.0, ..Links::ROUGH3D })).expect("3d");
    assert_eq!(check(&t, &region, Some(&f), Mode::Area, 4.0), Vec::new(), "{:?}", t.notes);
    let tan = 5.0_f64.to_radians().tan();
    for (_, _, ms) in transitions(&t) {
        for m in ms.iter().filter(|m| m.kind == MoveKind::Feed && m.to.z < m.from.z) {
            let run = (m.to.x - m.from.x).hypot(m.to.y - m.from.y);
            assert!(m.from.z - m.to.z <= run * tan + 1e-9, "{m:?}");
        }
    }
    // Keep down between levels: never above the level just cut unless named.
    let t = pocket3d(&f, &region, &p3(Links { level: Link::KeepDown, transition: Some(Entry3D::Helix), ..Links::ROUGH3D })).expect("3d");
    assert_eq!(check(&t, &region, Some(&f), Mode::Area, 4.0), Vec::new(), "{:?}", t.notes);
    let said = t.notes.iter().any(|n| n.starts_with("link:"));
    for (above, below, ms) in transitions(&t) {
        assert!(top(&ms) <= above + 1e-9 || said, "{above} -> {below}: {:?}", t.notes);
    }
    // Pass links: always lift to clearance when asked.
    let t = pocket3d(&f, &region, &p3(Links { pass: Link::Clearance, ..Links::ROUGH3D })).expect("3d");
    for (z, ms) in pass_gaps(&t) {
        assert_eq!(top(&ms), CLEAR, "{z}");
    }
}

#[test]
fn nonsense_link_numbers_refuse_by_name() {
    let r = rect(0.0, 0.0, 60.0, 40.0);
    for bad in [
        Links { transition: Some(Entry3D::Ramp), ramp_angle: 0.0, ..Links::PLANAR },
        Links { transition: Some(Entry3D::Ramp), ramp_angle: 90.0, ..Links::PLANAR },
        Links { level: Link::KeepDown, ramp_angle: f64::NAN, ..Links::PLANAR },
        Links { helix_pitch: -1.0, ..Links::PLANAR },
        Links { max_stay_down: f64::INFINITY, ..Links::PLANAR },
    ] {
        assert!(pocket(&r, &pp(Entry3D::Plunge, bad)).is_err(), "{bad:?}");
        assert!(contour(&r, &wall(Entry3D::Plunge, bad)).is_err(), "{bad:?}");
    }
}


/// The user's hand-test (2026-11-05): a 2D Adaptive pocket, Helix entry
/// at a 1.0 mm lead radius, a 10 mm tool - "just arcs on different
/// planes, not a continuous helix". The CL is a continuous helix: every
/// orbit move is an arc carrying its own Z drop, each starting where the
/// last ended, Z monotone, the drop per turn constant; it lands on the
/// level and links onto the first cut. (The flat look was the viewport
/// drawing every arc chord at its start Z - fixed in the UI overlay.)
///
/// The law is the helix's, not the pocket's size: since the HSM plan
/// (M11.4 C) measures every cut, the debug lane holds it on a 30 x 20
/// pocket; the user's 60 x 40 rides the heavy lane in `adaptive_hsm`
/// (`a_small_helix_rectangle_never_crosses_itself`).
#[test]
fn the_helix_entry_is_one_continuous_descent() {
    let r = rect(0.0, 0.0, 30.0, 20.0);
    let mut p = pp(Entry3D::Helix, Links::PLANAR);
    p.tool_diameter = 10.0;
    p.stepover = 4.0;
    p.helix_radius = 1.0;
    p.strategy = PocketStrategy::Adaptive;
    let t = pocket(&r, &p).expect("pocket");
    assert_eq!(check(&t, &r, None, Mode::Area, 5.0), Vec::new(), "{:?}", t.notes);
    let mut runs: Vec<Vec<Move>> = Vec::new();
    let mut cur: Vec<Move> = Vec::new();
    for m in &t.moves {
        if m.arc.is_some() && m.to.z < m.from.z {
            cur.push(*m);
        } else if !cur.is_empty() {
            runs.push(std::mem::take(&mut cur));
        }
    }
    assert_eq!(runs.len(), 3, "one helix per level (0, -2, -4): {:?} {:?}", runs.iter().map(|r| r.len()).collect::<Vec<_>>(), t.notes);
    for run in &runs {
        let drop = run[0].from.z - run[0].to.z;
        for w in run.windows(2) {
            assert_eq!(w[0].to, w[1].from, "each half turn starts where the last ended");
            assert!(w[1].to.z < w[1].from.z, "Z falls on every move");
            assert!(((w[1].from.z - w[1].to.z) - drop).abs() < 2.5e-6, "constant pitch (to the 1 um lattice)");
        }
        for m in run {
            let c = m.arc.map(|a| a.center).expect("arc");
            let rad = (m.from.x - c.x).hypot(m.from.y - c.y);
            assert!((rad - 1.0).abs() < 1e-3, "the asked radius: {rad}");
        }
    }
}
