//! #697: a path seeded on the clearance plane always opens with a rapid,
//! even when its start is exactly the seed (X0 Y0), where the generator's
//! own rapid to the start is a zero-length move the builder drops.
//!
//! Gate map: covers cl::cl (PathBuilder::approached) and every motion
//! generator seeded at (0, 0, clearance).
//!
//! Provenance: #690's posted-program law (the first motion after the tool
//! call is a rapid on the clearance plane); our own builder design.

use camcraft_cl::cl::{MoveKind, PathBuilder};
use camcraft_cl::geometry::Point3;

const CLEAR: f64 = 5.0;

#[test]
fn a_start_at_the_seed_still_opens_with_a_rapid() {
    let seed = Point3::new(0.0, 0.0, CLEAR);
    let mut b = PathBuilder::approached(seed);
    b.push(MoveKind::Rapid, seed, None); // the generator's approach: zero length, dropped
    b.push(MoveKind::Plunge, Point3::new(0.0, 0.0, -1.0), Some(100.0));
    let t = b.finish();
    assert_eq!(t.moves[0].kind, MoveKind::Rapid);
    assert_eq!((t.moves[0].from, t.moves[0].to), (seed, seed), "the approach states the start at clearance");
    assert_eq!(t.moves[1].kind, MoveKind::Plunge);
    assert!(t.is_chained());
}

#[test]
fn an_arc_first_move_is_approached_too() {
    let seed = Point3::new(0.0, 0.0, CLEAR);
    let mut b = PathBuilder::approached(seed);
    b.push_arc(MoveKind::Feed, Point3::new(2.0, 2.0, CLEAR), Some(100.0), camcraft_cl::geo2d::Point2::new(2.0, 0.0), true);
    let t = b.finish();
    assert_eq!(t.moves[0].kind, MoveKind::Rapid);
    assert_eq!(t.moves[1].kind, MoveKind::Feed);
    assert!(t.moves[1].arc.is_some());
}

#[test]
fn a_path_that_opens_with_its_own_rapid_is_unchanged() {
    let seed = Point3::new(0.0, 0.0, CLEAR);
    let mut b = PathBuilder::approached(seed);
    b.push(MoveKind::Rapid, Point3::new(3.0, 4.0, CLEAR), None);
    b.push(MoveKind::Plunge, Point3::new(3.0, 4.0, 0.0), Some(100.0));
    let t = b.finish();
    assert_eq!(t.len(), 2, "no extra move when the generator's approach stands");
    assert_eq!(t.moves[0].kind, MoveKind::Rapid);
    assert_eq!(t.moves[0].to, Point3::new(3.0, 4.0, CLEAR));
}

#[test]
fn a_plain_builder_keeps_its_old_behaviour() {
    let seed = Point3::new(0.0, 0.0, CLEAR);
    let mut b = PathBuilder::new(seed);
    b.push(MoveKind::Plunge, Point3::new(0.0, 0.0, -1.0), Some(100.0));
    let t = b.finish();
    assert_eq!(t.len(), 1);
    assert_eq!(t.moves[0].kind, MoveKind::Plunge);
}

#[test]
fn no_generator_seeds_the_clearance_plane_without_the_approach_law() {
    // Every mill generator that seeds its cursor at X0 Y0 on the clearance plane takes
    // `approached`; a plain `new` there is #697's bug waiting for a start
    // at X0 Y0.
    fn walk(dir: &std::path::Path, out: &mut Vec<String>) {
        let Ok(rd) = std::fs::read_dir(dir) else { return };
        for e in rd.flatten() {
            let p = e.path();
            if p.is_dir() {
                walk(&p, out);
            } else if p.extension().is_some_and(|x| x == "rs") {
                let text = std::fs::read_to_string(&p).unwrap_or_default();
                for (i, line) in text.lines().enumerate() {
                    // The mill seed: X0 Y0 on the clearance plane. Lathe
                    // generators seed at a clearance radius and station,
                    // with their own approach rules - not this law.
                    if line.contains("PathBuilder::new(Point3::new(0.0, 0.0,") && line.contains("clearance_z") {
                        out.push(format!("{}:{}: {}", p.display(), i + 1, line.trim()));
                    }
                }
            }
        }
    }
    let mut found = Vec::new();
    walk(&std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../motion/src"), &mut found);
    assert!(found.is_empty(), "seed on clearance with PathBuilder::approached:\n{}", found.join("\n"));
}
