//! #3360: Bore reads the shared "Pass starts at" order. Auto keeps the
//! centre-out ring ladder byte for byte; Outside cuts the wall ring
//! first and walks in to the centre; Inside is the centre-out order (a
//! bore's area has no inner wall).
//!
//! Provenance: our own tests over the kernel's ring law (circular
//! interpolation boring, machine-tool handbooks); no vendor material
//! consulted.
//!
//! Gate map: covers camcraft_motion::machining2d (bore, pass_start).

use camcraft_motion::cl::{MoveKind, Toolpath};
use camcraft_motion::entry::Entry3D;
use camcraft_motion::geo2d::Point2;
use camcraft_motion::links::Links;
use camcraft_motion::machining2d::{BoreHole, BoreParams, PASS_START_NOTE, PassStart, bore, bore_started};

/// One blind Ø20 hole at (5, 5), top 0, floor -4, a Ø6 square end mill,
/// 2 mm stepdown, plunge entry: the wall ring rides at 10 - 3 = 7 mm.
fn params() -> BoreParams {
    BoreParams {
        holes: vec![BoreHole {
            xy: Point2::new(5.0, 5.0),
            radius: 10.0,
            top_z: 0.0,
            floor_z: -4.0,
            through: false,
            stepped: false,
        }],
        tool_diameter: 6.0,
        corner_radius: 0.0,
        stock_to_leave: 0.0,
        stepdown: 2.0,
        through_allowance: 0.0,
        clearance_z: 5.0,
        retract_z: None,
        feed: 500.0,
        plunge_feed: 250.0,
        climb: true,
        tolerance: 0.01,
        entry: Entry3D::Plunge,
        helix_radius: 1.0,
        links: Links::PLANAR,
    }
}

const WALL: f64 = 7.0;

/// The ring radii of the first level, in cut order: each feed move at
/// the level's Z, its end's distance from the hole centre, rounded to
/// 0.01 mm, kept once in order of first appearance.
fn first_level_radii(t: &Toolpath) -> Vec<f64> {
    let z = -2.0;
    let mut out: Vec<f64> = Vec::new();
    for m in &t.moves {
        if m.kind == MoveKind::Rapid || (m.to.z - z).abs() > 1e-6 || (m.from.z - z).abs() > 1e-6 {
            continue;
        }
        let r = ((m.to.x - 5.0).hypot(m.to.y - 5.0) * 100.0).round() / 100.0;
        if r > 1e-3 && !out.iter().any(|&q| (q - r).abs() < 0.02) {
            out.push(r);
        }
    }
    out
}

#[test]
fn bore_auto_keeps_the_centre_out_ladder_byte_for_byte() {
    let p = params();
    let old = bore(&p).expect("a legal bore");
    let auto = bore_started(&p, PassStart::Auto).expect("a legal bore");
    assert_eq!(old.moves, auto.moves);
    let radii = first_level_radii(&auto);
    assert!(radii.len() >= 2, "a Ø20 bore with a Ø6 cutter has several rings: {radii:?}");
    assert!(radii.windows(2).all(|w| w[0] < w[1]), "Auto walks centre-out: {radii:?}");
    assert!((radii[radii.len() - 1] - WALL).abs() < 0.02, "Auto ends on the wall: {radii:?}");
}

#[test]
fn bore_outside_starts_each_level_on_the_wall_and_walks_in() {
    let p = params();
    let auto = first_level_radii(&bore_started(&p, PassStart::Auto).expect("a legal bore"));
    let t = bore_started(&p, PassStart::Outside).expect("a legal bore");
    let radii = first_level_radii(&t);
    assert!((radii[0] - WALL).abs() < 0.02, "Outside cuts the wall ring first: {radii:?}");
    assert!(radii.windows(2).all(|w| w[0] > w[1]), "Outside walks in to the centre: {radii:?}");
    // The same rings, only in the other order: the hole is still cleared.
    let mut back = radii.clone();
    back.reverse();
    assert_eq!(back, auto);
    // Each level ends on the innermost ring, not on the wall.
    let last_feed = t.moves.iter().rev().find(|m| m.kind != MoveKind::Rapid && (m.to.z + 4.0).abs() < 1e-6);
    let r_last = last_feed.map(|m| (m.to.x - 5.0).hypot(m.to.y - 5.0));
    assert!(r_last.is_some_and(|r| r < WALL - 0.5), "the last level ends inside: {r_last:?}");
}

#[test]
fn bore_inside_is_the_centre_out_order() {
    let p = params();
    let auto = bore_started(&p, PassStart::Auto).expect("a legal bore");
    let inside = bore_started(&p, PassStart::Inside).expect("a legal bore");
    assert_eq!(auto.moves, inside.moves);
}

/// #3360/#3402: Outside cuts the wall ring first, a full tool width along
/// the hole wall in uncut stock. The family warning law names that cut on
/// the path notes; Auto and Inside start at the centre in open air, so
/// they post no such note.
#[test]
fn bore_outside_names_the_full_width_wall_cut() {
    let p = params();
    let outside = bore_started(&p, PassStart::Outside).expect("a legal bore");
    let named: Vec<&String> = outside.notes.iter().filter(|n| n.starts_with(PASS_START_NOTE)).collect();
    assert_eq!(named.len(), 1, "Outside names the full-width wall cut: {:?}", outside.notes);
    assert!(
        named[0].contains("full tool width") && named[0].contains("wall"),
        "the note names the wall cut: {:?}",
        outside.notes
    );
    for start in [PassStart::Auto, PassStart::Inside] {
        let t = bore_started(&p, start).expect("a legal bore");
        assert!(
            t.notes.iter().all(|n| !n.starts_with(PASS_START_NOTE)),
            "{start:?} starts at the centre in open air, so it posts no wall-first note: {:?}",
            t.notes
        );
    }
}
