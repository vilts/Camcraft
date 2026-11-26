//! M11.3 D owed: the entry engagement limit on the 3-D ladder slows only
//! each level's first tool diameter of plane cut out of its descent (the
//! dive link and the first pass), the rest of the level at the cutting
//! feed - the planar ladder's law (515525c). Off, nothing is slowed.
//! Provenance: our own design (the MRR reading of radial engagement in
//! `approach`), per the Clean-Room Protocol.
//!
//! Gate map (M11.3): covers camcraft_motion::machining3d (ladder,
//! level_ctx), camcraft_motion::approach, camcraft_motion::machining2d
//! (levels, unchanged).

use camcraft_motion::approach::Approach;
use camcraft_motion::cl::{Move, MoveKind, Toolpath};
use camcraft_motion::geo2d::{Loop, Point2, Region};
use camcraft_motion::geometry::{Point3, Triangle};
use camcraft_motion::links::Links;
use camcraft_motion::machining2d::{PocketParams, PocketStrategy, pocket};
use camcraft_motion::machining3d::{Entry3D, Pocket3DParams, pocket3d, pocket3d_offset};

const FEED: f64 = 500.0;
const SLOW: f64 = FEED * 0.5;
const D: f64 = 8.0;
const CLEAR: f64 = 5.0;

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

fn box_facets(min: [f64; 3], max: [f64; 3]) -> Vec<Triangle> {
    let v = |i: usize| {
        Point3::new([min[0], max[0]][i & 1], [min[1], max[1]][(i >> 1) & 1], [min[2], max[2]][(i >> 2) & 1])
    };
    let quad = |a: usize, b: usize, c: usize, d: usize| [Triangle::new(v(a), v(b), v(c)), Triangle::new(v(a), v(c), v(d))];
    let mut t = Vec::with_capacity(12);
    for q in [quad(4, 5, 7, 6), quad(0, 2, 3, 1), quad(0, 1, 5, 4), quad(2, 6, 7, 3), quad(1, 3, 7, 5), quad(2, 0, 4, 6)] {
        t.extend(q);
    }
    t
}

fn p3(entry: Entry3D, strategy: PocketStrategy, limit: f64) -> Pocket3DParams {
    Pocket3DParams {
        pass_start: Default::default(),
        links: Links { approach: Approach { max_engagement: limit, ..Approach::DEFAULT }, ..Links::ROUGH3D },
        tool_diameter: D,
        stepover: 3.0,
        stock_to_leave: 0.0,
        clearance_z: CLEAR,
        top_z: 0.0,
        bottom_z: -10.0,
        stepdown: 5.0,
        feed: FEED,
        plunge_feed: 100.0,
        climb: true,
        strategy,
        retract_z: Some(3.0),
        entry,
        helix_radius: 2.0,
        optimal_load: camcraft_motion::load::DEFAULT_OPTIMAL_LOAD,
    }
}

fn xy(m: &Move) -> f64 {
    (m.to.x - m.from.x).hypot(m.to.y - m.from.y)
}

fn flat(m: &Move) -> bool {
    m.kind == MoveKind::Feed && m.from.z == m.to.z
}

/// The leading run of plane feeds at the scaled feed among `moves`: its
/// XY length, and whether every later plane feed holds the cutting feed
/// (and at least one exists).
fn split_of<'a>(moves: impl Iterator<Item = &'a Move>) -> (f64, bool) {
    let plane: Vec<&Move> = moves.filter(|m| flat(m)).collect();
    let slowed: f64 = plane.iter().take_while(|m| m.feed == Some(SLOW)).map(|m| xy(m)).sum();
    let rest: Vec<_> = plane.iter().skip_while(|m| m.feed == Some(SLOW)).collect();
    (slowed, !rest.is_empty() && rest.iter().all(|m| m.feed == Some(FEED)))
}

/// The 3-D ladder's levels: each climbs to clearance (the default level
/// link) before its entry, so the path splits there - the shell can
/// floor two levels on one Z, which a per-Z grouping cannot tell apart.
fn levels(t: &Toolpath) -> Vec<&[Move]> {
    t.moves
        .split(|m| m.kind == MoveKind::Rapid && m.to.z == CLEAR && m.from.z < CLEAR)
        .filter(|c| c.iter().any(flat))
        .collect()
}

#[test]
fn the_3d_ladder_slows_exactly_the_first_diameter_of_each_level() {
    let facets = box_facets([-20.0, -20.0, -20.0], [20.0, 20.0, -10.0]);
    let footprint = rect(-20.0, -20.0, 20.0, 20.0);
    for strategy in [PocketStrategy::Zigzag, PocketStrategy::Waterline] {
        for entry in [Entry3D::Helix, Entry3D::Plunge, Entry3D::Ramp] {
            let tag = format!("{strategy:?} {entry:?}");
            let on = pocket3d(&facets, &footprint, &p3(entry, strategy, 0.5)).expect("pocket3d");
            let lv = levels(&on);
            assert!(lv.len() >= 2, "{tag}: {} levels", lv.len());
            for (i, l) in lv.iter().enumerate() {
                let (slowed, rest_at_feed) = split_of(l.iter());
                assert!((slowed - D).abs() < 1e-5, "{tag} level {i}: {slowed} mm slowed, want exactly {D}");
                assert!(rest_at_feed, "{tag} level {i}: the rest of the level is not at the cutting feed");
            }
            // Nothing else carries the scaled feed but the descents.
            let slow_flat: f64 = on.moves.iter().filter(|m| flat(m) && m.feed == Some(SLOW)).map(xy).sum();
            let want = D * lv.len() as f64;
            assert!((slow_flat - want).abs() < 1e-5, "{tag}: {slow_flat} mm of plane cut slowed, want {want}");
            assert!(on.notes.iter().any(|n| n.contains("over the 50% limit")), "{tag}: {:?}", on.notes);

            // Off: every plane feed at the cutting feed, nothing scaled.
            let off = pocket3d(&facets, &footprint, &p3(entry, strategy, 0.0)).expect("pocket3d");
            assert!(off.moves.iter().all(|m| m.feed != Some(SLOW)), "{tag}: limit off slowed a move");
            assert!(!off.notes.iter().any(|n| n.contains("limit")), "{tag}: {:?}", off.notes);
            assert!(off.moves.iter().filter(|m| flat(m)).all(|m| m.feed == Some(FEED)), "{tag}: limit off");
            // The same path otherwise: only plane feeds split at the
            // stretch ends (one extra vertex per level at most).
            let other = |t: &Toolpath| t.moves.iter().filter(|m| !flat(m)).count();
            assert_eq!(other(&on), other(&off), "{tag}: the limit changed more than plane feeds");
            let (n_on, n_off) = (on.moves.len(), off.moves.len());
            assert!(n_on >= n_off && n_on <= n_off + lv.len(), "{tag}: {n_on} vs {n_off}");
        }
    }
}

#[test]
fn the_offset_shell_slows_whole_chords_never_less_than_a_diameter() {
    // Prismatic: the shell collapses onto the planes, but walks chords
    // of at most the tool radius - each slowed whole (conservative).
    let facets = box_facets([-20.0, -20.0, -20.0], [20.0, 20.0, -10.0]);
    let footprint = rect(-20.0, -20.0, 20.0, 20.0);
    for entry in [Entry3D::Helix, Entry3D::Plunge, Entry3D::Ramp] {
        let on = pocket3d_offset(&facets, &footprint, &p3(entry, PocketStrategy::Zigzag, 0.5)).expect("shell");
        let lv = levels(&on);
        assert!(lv.len() >= 2, "{entry:?}: {} levels", lv.len());
        for (i, l) in lv.iter().enumerate() {
            let (slowed, rest_at_feed) = split_of(l.iter());
            assert!((D - 1e-5..D * 1.5).contains(&slowed), "{entry:?} level {i}: {slowed} mm slowed");
            assert!(rest_at_feed, "{entry:?} level {i}");
        }
        let off = pocket3d_offset(&facets, &footprint, &p3(entry, PocketStrategy::Zigzag, 0.0)).expect("shell");
        assert!(off.moves.iter().all(|m| m.feed != Some(SLOW)), "{entry:?}");
        assert_eq!(on.moves.len(), off.moves.len(), "{entry:?}: the shell never splits a chord");
    }
}

#[test]
fn the_planar_ladder_still_slows_its_link_and_first_diameter() {
    // 515525c's planar law, unchanged: the descent's link plus the
    // first tool diameter of the first pass, then the cutting feed.
    let r = rect(0.0, 0.0, 40.0, 30.0);
    let pp = |links: Links| PocketParams {
        blend_radius: None,
        tool_diameter: 6.0,
        stepover: 2.5,
        stock_to_leave: 0.0,
        clearance_z: 25.0,
        top_z: 0.0,
        bottom_z: -6.0,
        stepdown: 2.0,
        feed: FEED,
        plunge_feed: 100.0,
        climb: true,
        strategy: PocketStrategy::Zigzag,
        retract_z: Some(15.0),
        entry: Entry3D::Helix,
        helix_radius: 1.5,
        optimal_load: camcraft_motion::load::DEFAULT_OPTIMAL_LOAD,
        blend_turns: false,
        links,
    };
    let on = pocket(&r, &pp(Links { approach: Approach { max_engagement: 0.5, ..Approach::DEFAULT }, ..Links::PLANAR }))
        .expect("pocket");
    let off = pocket(&r, &pp(Links::PLANAR)).expect("pocket");
    assert!(off.moves.iter().all(|m| m.feed != Some(SLOW)));
    for z in [-2.0, -4.0, -6.0] {
        let (slowed, rest_at_feed) = split_of(on.moves.iter().filter(|m| m.from.z == z && m.to.z == z));
        assert!(slowed >= 6.0 - 1e-6, "level {z}: only {slowed} mm slowed");
        assert!(rest_at_feed, "level {z}");
    }
}
