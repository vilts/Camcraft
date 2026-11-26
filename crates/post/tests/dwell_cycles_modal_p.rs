//! G82/G89 modal dwell P on the Fanuc, Mazak, Doosan, grblHAL and Tormach
//! posts (#2360, the Haas fix of #2350 carried over), from the public Fanuc
//! canned-cycle formats (G82/G89 X Y Z R P F, G85 X Y Z R F) and the
//! LinuxCNC G-code reference (G82/G86/G89 take P in seconds, G85 has no P;
//! numbers stay sticky until programmed again).
//! Gate map: covers assets/posts/fanuc.cps, assets/posts/mazak.cps,
//! assets/posts/doosan-dnm.cps, assets/posts/grblhal.cps,
//! assets/posts/tormach.cps, assets/posts/linuxcnc.cps, assets/posts/mach3.cps,
//! camcraft_post::post::engine and
//! camcraft_post::post::cnc.

use camcraft_post::cl::{CycleKind, MoveKind};
use camcraft_post::post::{
    cnc::{self, CycleRec, MoveRec, Record},
    engine,
};
use serde_json::json;
use std::path::Path;

/// The posts, with the P word each writes for 0.5 s and 0.75 s.
const POSTS: [(&str, &str, &str); 7] = [
    ("fanuc", "P500", "P750"),
    ("mazak", "P500", "P750"),
    ("doosan-dnm", "P500", "P750"),
    ("grblhal", "P0.5", "P0.75"),
    ("tormach", "P0.5", "P0.75"),
    ("linuxcnc", "P0.5", "P0.75"),
    ("mach3", "P0.5", "P0.75"),
];

fn read(rel: &str) -> String {
    std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../")
            .join(rel),
    )
    .unwrap_or_else(|e| panic!("{rel}: {e}"))
    .replace("\r\n", "\n")
}

/// Back-to-back holes `(kind, dwell s)` at one XY, so an unchanged
/// signature keeps the group open and posts the hole as a bare X Y block.
fn stream(holes: &[(CycleKind, f64)]) -> String {
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
    for &(kind, dwell) in holes {
        records.push(Record::Cycle(CycleRec {
            kind,
            x: 10.0,
            y: 10.0,
            r_z: 2.0,
            bottom_z: -12.0,
            entry_z: 0.0,
            peck: 0.0,
            dwell,
            first_move: 1,
            feed: Some(120.0),
        }));
    }
    records.push(Record::Section { begin: false });
    cnc::serialize(&records)
}

fn run(stem: &str, holes: &[(CycleKind, f64)]) -> engine::PostOutput {
    let machine = serde_json::from_str(&read("assets/machines/3axis-vmc.mch")).expect("machine");
    engine::run(
        &read(&format!("assets/posts/{stem}.cps")),
        &machine,
        &stream(holes),
    )
    .unwrap_or_else(|e| panic!("{stem}: post engine: {e}"))
}

const CYCLES: [&str; 6] = ["G81", "G82", "G85", "G86", "G89", "G83"];

/// Each hole as (cycle word, P word). A bare X Y block inside an open
/// group repeats the open cycle and runs its modal P: `(g, None)`.
fn holes(program: &str) -> Vec<(String, Option<String>)> {
    let mut out = Vec::new();
    let mut open: Option<String> = None;
    for line in program.lines() {
        let words: Vec<&str> = line.split_whitespace().collect();
        if let Some(g) = words.iter().find(|w| CYCLES.contains(w)) {
            open = Some(g.to_string());
            let p = words
                .iter()
                .find(|w| w.starts_with('P'))
                .map(|w| w.to_string());
            out.push((g.to_string(), p));
        } else if words.contains(&"G80") {
            open = None;
        } else if let Some(g) = &open
            && !words.is_empty()
            && words
                .iter()
                .all(|w| w.starts_with('X') || w.starts_with('Y'))
        {
            out.push((g.clone(), None));
        }
    }
    out
}

fn pins(stem: &str, input: &[(CycleKind, f64)], want: &[(&str, Option<&str>)]) {
    let out = run(stem, input);
    assert!(out.warnings.is_empty(), "{stem}: {:?}", out.warnings);
    let want: Vec<(String, Option<String>)> = want
        .iter()
        .map(|(g, p)| (g.to_string(), p.map(str::to_string)))
        .collect();
    assert_eq!(
        holes(&out.program),
        want,
        "{stem}: every hole must post its own dwell P, never inherit the modal one:\n{}",
        out.program
    );
}

#[test]
fn g82_rewrites_p_when_the_dwell_changes() {
    for (stem, p5, p75) in POSTS {
        pins(
            stem,
            &[(CycleKind::Drill, 0.5), (CycleKind::Drill, 0.75)],
            &[("G82", Some(p5)), ("G82", Some(p75))],
        );
    }
}

#[test]
fn g82_then_g89_each_hole_posts_its_own_p() {
    for (stem, p5, p75) in POSTS {
        pins(
            stem,
            &[
                (CycleKind::Drill, 0.5),
                (CycleKind::BackBore, 0.5),
                (CycleKind::BackBore, 0.75),
            ],
            &[("G82", Some(p5)), ("G89", Some(p5)), ("G89", Some(p75))],
        );
    }
}

#[test]
fn dwell_to_zero_leaves_no_p_active() {
    for (stem, p5, _) in POSTS {
        // A drill goes to G81 and a back-bore to G85: neither reads P.
        pins(
            stem,
            &[(CycleKind::Drill, 0.5), (CycleKind::Drill, 0.0)],
            &[("G82", Some(p5)), ("G81", None)],
        );
        pins(
            stem,
            &[(CycleKind::BackBore, 0.5), (CycleKind::BackBore, 0.0)],
            &[("G89", Some(p5)), ("G85", None)],
        );
        pins(stem, &[(CycleKind::BackBore, 0.0)], &[("G85", None)]);
    }
}

#[test]
fn same_dwell_keeps_the_open_group() {
    for (stem, p5, _) in POSTS {
        for (kind, g) in [(CycleKind::Drill, "G82"), (CycleKind::BackBore, "G89")] {
            // The unchanged dwell rides the open group as a bare X Y hole.
            pins(
                stem,
                &[(kind, 0.5), (kind, 0.5)],
                &[(g, Some(p5)), (g, None)],
            );
        }
    }
}

#[test]
fn linuxcnc_family_g86_rewrites_p_when_the_dwell_changes() {
    for (stem, p5, p75) in &POSTS[3..] {
        pins(
            stem,
            &[(CycleKind::Bore, 0.5), (CycleKind::Bore, 0.75)],
            &[("G86", Some(p5)), ("G86", Some(p75))],
        );
    }
}
