//! #2609: G73/G85 dwell refusals from the public LinuxCNC cycle formats (no P word).
//! Mach3 (#2482) has the same formats ("Using Mach3Mill" 1.84, 10.7.22 and
//! 10.7.24.6: no P word on G73 or G85).
//! Gate map: covers assets/posts/tormach.cps, assets/posts/grblhal.cps,
//! assets/posts/linuxcnc.cps, assets/posts/mach3.cps, camcraft_post::post::cnc, camcraft_post::post::engine.

use camcraft_post::cl::{CycleKind, MoveKind};
use camcraft_post::post::{
    cnc::{self, CycleRec, MoveRec, Record},
    engine,
};
use serde_json::json;
use std::path::Path;

const POSTS: [&str; 4] = ["tormach", "grblhal", "linuxcnc", "mach3"];

fn read(rel: &str) -> String {
    std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../")
            .join(rel),
    )
    .expect("fixture")
}

fn stream(kind: CycleKind, dwells: &[f64]) -> String {
    let mut records = vec![
        Record::Param {
            name: "SpindleSpeed".into(),
            value: json!(900.0),
        },
        Record::Param {
            name: "ClearancePlane".into(),
            value: json!(10.0),
        },
        Record::Param {
            name: "RetractPlane".into(),
            value: json!(10.0),
        },
        Record::Tool {
            number: 4,
            name: "DRILL".into(),
        },
        Record::Section { begin: true },
        Record::Move(MoveRec {
            kind: MoveKind::Rapid,
            x: 10.0,
            y: 10.0,
            z: 2.0,
            feed: None,
            center: None,
            ccw: false,
            plane: Default::default(),
            abc: None,
        }),
    ];
    // Consecutive records also exercise the open cycle group's shortcut.
    for &dwell in dwells {
        records.push(Record::Cycle(CycleRec {
            kind,
            x: 10.0,
            y: 10.0,
            r_z: 2.0,
            bottom_z: -12.0,
            entry_z: 0.0,
            peck: 3.0,
            dwell,
            first_move: 1,
            feed: Some(120.0),
        }));
    }
    records.push(Record::Section { begin: false });
    cnc::serialize(&records)
}

fn run(stem: &str, stream: &str) -> engine::PostOutput {
    let machine = serde_json::from_str(&read("assets/machines/3axis-vmc.mch")).expect("machine");
    engine::run(&read(&format!("assets/posts/{stem}.cps")), &machine, stream).expect("post engine")
}

fn cycle_lines<'a>(program: &'a str, g: &str) -> Vec<&'a str> {
    program
        .lines()
        .filter(|line| line.split_whitespace().any(|word| word == g))
        .collect()
}

fn assert_refusal(out: &engine::PostOutput, stem: &str, g: &str, name: &str, dwell: f64) {
    let expected = format!(
        "canned cycle {g} ({name}) dwell is not supported by this post - a {dwell} s dwell is refused, not dropped; set the dwell to 0"
    );
    assert!(
        out.warnings.iter().any(|w| w.contains(&expected)),
        "{stem}: {g} dwell {dwell} needs a named refusal: {:?}",
        out.warnings
    );
}

fn refuses(stem: &str, kind: CycleKind, g: &str, name: &str) {
    for dwell in [0.001, 0.5] {
        let out = run(stem, &stream(kind, &[dwell]));
        assert_refusal(&out, stem, g, name, dwell);
        assert!(
            cycle_lines(&out.program, g).is_empty(),
            "{stem}: a refused cycle must not be written:\n{}",
            out.program
        );
    }
    let out = run(stem, &stream(kind, &[0.0, 0.5]));
    assert_refusal(&out, stem, g, name, 0.5);
    assert_eq!(
        cycle_lines(&out.program, g).len(),
        1,
        "{stem}: keep only the first, zero-dwell cycle:\n{}",
        out.program
    );
    assert!(
        !out.program.lines().any(|line| {
            let words: Vec<_> = line.split_whitespace().collect();
            words.iter().any(|w| w.starts_with('X'))
                && words.iter().all(|w| w.starts_with(['X', 'Y', 'N']))
        }),
        "{stem}: a refused second hole must not write a modal XY cycle point:\n{}",
        out.program
    );
    // Serialization rounds to milliseconds. A raw stream can retain a smaller P.
    let raw = stream(kind, &[0.001]).replace("P 1 ", "P 0.1 ");
    let parsed = cnc::parse(&raw).expect("fractional millisecond stream");
    let dwell = parsed
        .iter()
        .find_map(|record| match record {
            Record::Cycle(c) => Some(c.dwell),
            _ => None,
        })
        .expect("cycle");
    assert!(
        (dwell - 0.0001).abs() < 1e-10,
        "retain the raw sub-ms dwell"
    );
    let out = run(stem, &raw);
    assert_refusal(&out, stem, g, name, 0.0001);
    assert!(
        cycle_lines(&out.program, g).is_empty(),
        "{stem}: a sub-ms refusal must not write {g}"
    );
}

#[test]
fn tormach_refuses_g73_dwell() {
    refuses("tormach", CycleKind::Peck, "G73", "peck");
}

#[test]
fn tormach_refuses_g85_dwell() {
    refuses("tormach", CycleKind::Ream, "G85", "ream");
}

#[test]
fn grblhal_refuses_g73_dwell() {
    refuses("grblhal", CycleKind::Peck, "G73", "peck");
}

#[test]
fn grblhal_refuses_g85_dwell() {
    refuses("grblhal", CycleKind::Ream, "G85", "ream");
}

#[test]
fn linuxcnc_refuses_g73_dwell() {
    refuses("linuxcnc", CycleKind::Peck, "G73", "peck");
}

#[test]
fn linuxcnc_refuses_g85_dwell() {
    refuses("linuxcnc", CycleKind::Ream, "G85", "ream");
}

#[test]
fn mach3_refuses_g73_dwell() {
    refuses("mach3", CycleKind::Peck, "G73", "peck");
}

#[test]
fn mach3_refuses_g85_dwell() {
    refuses("mach3", CycleKind::Ream, "G85", "ream");
}

#[test]
fn linuxcnc_family_posts_g73_g85_without_dwell_unchanged() {
    for stem in POSTS {
        for (kind, g) in [(CycleKind::Peck, "G73"), (CycleKind::Ream, "G85")] {
            let out = run(stem, &stream(kind, &[0.0]));
            assert!(out.warnings.is_empty(), "{stem} {g}: {:?}", out.warnings);
            let lines = cycle_lines(&out.program, g);
            assert_eq!(lines.len(), 1, "{stem}: one {g} block:\n{}", out.program);
            let words: Vec<_> = lines[0].split_whitespace().collect();
            for word in ["X10", "Y10", "Z-12", "R2", "F120"] {
                assert!(
                    words.contains(&word),
                    "{stem}: {g} keeps {word}: {}",
                    lines[0]
                );
            }
            assert_eq!(
                words.contains(&"Q3"),
                g == "G73",
                "{stem}: peck Q only on G73"
            );
            assert!(
                !words.iter().any(|w| w.starts_with('P')),
                "{stem}: no dwell, no P: {}",
                lines[0]
            );
        }
    }
}
