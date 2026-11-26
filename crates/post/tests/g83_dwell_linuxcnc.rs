//! #2348: the G83 dwell on the LinuxCNC-family posts, from the public
//! LinuxCNC G-code reference (G83 format R L Q, no P), Tormach's PathPilot
//! G83 page (X Y Z A R L Q, no P) and grblHAL's core code list (G83 is
//! "drilling cycle with peck", implemented to the LinuxCNC specification).
//! A positive G83 dwell is refused by name before any G83 block is written;
//! a zero dwell posts G83 with no P, as before.
//!
//! Gate map: covers assets/posts/tormach.cps, assets/posts/grblhal.cps,
//! assets/posts/linuxcnc.cps (the
//! G83 dwell door), camcraft_post::post::cnc, camcraft_post::post::engine.

use camcraft_post::cl::{Cycle, CycleKind, MoveKind, PathBuilder, Toolpath};
use camcraft_post::geo2d::Point2;
use camcraft_post::geometry::Point3;
use camcraft_post::post::{cnc, dump, engine, Dump};
use std::path::Path;

/// The posts whose G83 has no dwell word.
const LINUXCNC_FAMILY: [&str; 3] = ["tormach", "grblhal", "linuxcnc"];

const R: f64 = 2.0;
const HOP: f64 = 10.0;
const BOTTOM: f64 = -12.0;
const FEED: f64 = 120.0;

fn read(rel: &str) -> String {
    std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../")
            .join(rel),
    )
    .unwrap_or_else(|e| panic!("{rel}: {e}"))
    .replace("\r\n", "\n")
}

/// One deep-peck hole: rapid to R, feed to the bottom, rapid back to R, hop.
fn program(dwell: f64) -> Toolpath {
    let mut b = PathBuilder::new(Point3::new(10.0, 10.0, HOP));
    b.push(MoveKind::Rapid, Point3::new(10.0, 10.0, R), None);
    let fm = b.moves_len();
    b.push(
        MoveKind::Plunge,
        Point3::new(10.0, 10.0, BOTTOM),
        Some(FEED),
    );
    b.push(MoveKind::Rapid, Point3::new(10.0, 10.0, R), None);
    b.push_cycle(Cycle {
        kind: CycleKind::Deep,
        xy: Point2::new(10.0, 10.0),
        r_z: R,
        bottom_z: BOTTOM,
        entry_z: 0.0,
        peck: 3.0,
        dwell,
        first_move: fm,
    });
    b.push(MoveKind::Rapid, Point3::new(10.0, 10.0, HOP), None);
    b.finish()
}

fn stream(dwell: f64) -> String {
    let d = Dump {
        op_name: "Deep drill",
        tool_number: 4,
        tool_name: "DRILL 6",
        tool_diameter: 6.0,
        rpm: 900.0,
        clearance_z: HOP,
        retract_z: HOP,
        top_z: 0.0,
        ..Dump::default()
    };
    cnc::serialize(&dump(&program(dwell), &d))
}

fn run(stem: &str, dwell: f64) -> engine::PostOutput {
    let cps = read(&format!("assets/posts/{stem}.cps"));
    let mch = serde_json::from_str(&read("assets/machines/3axis-vmc.mch")).expect("machine json");
    engine::run(&cps, &mch, &stream(dwell)).unwrap_or_else(|e| panic!("{stem}: runs: {e}"))
}

fn g83_lines(program: &str) -> Vec<&str> {
    program
        .lines()
        .filter(|l| l.split_whitespace().any(|w| w == "G83"))
        .collect()
}

#[test]
fn linuxcnc_family_refuses_a_g83_dwell_by_name() {
    for stem in LINUXCNC_FAMILY {
        for dwell in [0.001, 0.5] {
            let out = run(stem, dwell);
            let named = format!("a {dwell} s dwell is refused, not dropped");
            assert!(
                out.warnings
                    .iter()
                    .any(|w| w.contains("canned cycle G83 (deep peck) dwell") && w.contains(&named)),
                "{stem}: the G83 dwell {dwell} must be refused by name: {:?}",
                out.warnings
            );
            assert!(
                g83_lines(&out.program).is_empty(),
                "{stem}: a refused G83 must not be written:\n{}",
                out.program
            );
        }
    }
}

#[test]
fn linuxcnc_family_posts_g83_without_dwell_unchanged() {
    for stem in LINUXCNC_FAMILY {
        let out = run(stem, 0.0);
        assert!(out.warnings.is_empty(), "{stem}: {:?}", out.warnings);
        let lines = g83_lines(&out.program);
        assert_eq!(lines.len(), 1, "{stem}: one G83 block:\n{}", out.program);
        let words: Vec<&str> = lines[0].split_whitespace().collect();
        assert!(
            words.contains(&"Q3"),
            "{stem}: the peck rides Q: {}",
            lines[0]
        );
        assert!(
            !words.iter().any(|w| w.starts_with('P')),
            "{stem}: no dwell, no P: {}",
            lines[0]
        );
    }
}
