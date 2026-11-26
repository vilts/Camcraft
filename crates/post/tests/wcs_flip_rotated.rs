//! #2476: a flipped and Z-rotated WCS follows the CAM path in either output mode.
//! Provenance: planar frame composition and RS-274 G68 / Siemens ROT semantics.
//! Gate map: covers camcraft_post::post::{engine, wcs_flip, wcs_rotate}.

mod inverted_support;
mod mirror_trace;

use camcraft_post::cl::{ArcPlane, CompSide, CycleKind, MoveKind};
use camcraft_post::post::cnc::{self, CycleRec, Record};
use camcraft_post::post::engine;
use inverted_support::{arc, cps, machine, mv};
use mirror_trace::{Dialect, assert_same_trace, cam_trace, controller_trace};
use serde_json::{Value, json};

const DIALECT: Dialect = Dialect { heidenhain: false };
const CASES: &[(&str, [bool; 3])] = &[
    ("X", [true, false, false]),
    ("Y", [false, true, false]),
    ("XY", [true, true, false]),
];

fn param(name: &str, value: Value) -> Record {
    Record::Param {
        name: name.into(),
        value,
    }
}

fn section(letters: Option<&str>, yaw: f64, control: bool) -> Vec<Record> {
    use MoveKind::{Feed, Plunge, Rapid};
    let mut records = vec![
        param("OperationName", json!("FlipYaw")),
        param("SpindleSpeed", json!(5000)),
        param("WorkOffset", json!(1)),
        param("OriginX", json!(10.0)),
        param("OriginY", json!(20.0)),
        param("OriginZ", json!(0.0)),
        param("ClearancePlane", json!(5.0)),
        param("WcsYaw", json!(yaw)),
        param(
            "WcsRotationMode",
            json!(if control { "control" } else { "bake" }),
        ),
    ];
    if let Some(letters) = letters {
        records.push(param("WcsFlip", json!(letters)));
    }
    records.extend([
        Record::Tool {
            number: 1,
            name: "END MILL".into(),
        },
        Record::Section { begin: true },
        mv(Rapid, [30.0, 30.0, 5.0], None),
        mv(Plunge, [30.0, 30.0, -1.0], Some(200.0)),
        mv(Feed, [50.0, 30.0, -1.0], Some(400.0)),
        arc([60.0, 40.0, -1.0], (50.0, 40.0), true, ArcPlane::Xy),
        arc([50.0, 50.0, -2.0], (50.0, 40.0), false, ArcPlane::Xy),
        Record::Comp {
            side: Some(CompSide::Left),
            radius_mm: 3.0,
            wear: false,
        },
        mv(Feed, [40.0, 50.0, -2.0], Some(400.0)),
        arc([30.0, 40.0, -2.0], (40.0, 40.0), true, ArcPlane::Xy),
        Record::Comp {
            side: None,
            radius_mm: 3.0,
            wear: false,
        },
        mv(Feed, [20.0, 40.0, -2.0], Some(400.0)),
        mv(Rapid, [20.0, 40.0, 5.0], None),
        mv(Rapid, [70.0, 20.0, 2.0], None),
    ]);
    let first_move = records
        .iter()
        .filter(|r| matches!(r, Record::Move(_)))
        .count();
    records.extend([
        Record::Cycle(CycleRec {
            kind: CycleKind::Drill,
            x: 70.0,
            y: 20.0,
            r_z: 2.0,
            bottom_z: -5.0,
            entry_z: 0.0,
            peck: 0.0,
            dwell: 0.0,
            first_move,
            feed: Some(150.0),
        }),
        mv(Plunge, [70.0, 20.0, -5.0], Some(150.0)),
        mv(Rapid, [70.0, 20.0, 2.0], None),
        Record::Section { begin: false },
    ]);
    records
}

fn post(stem: &str, records: &[Record]) -> String {
    let mut mch = machine(&[]);
    mch["coordinateRotation"] = json!(true);
    let out = engine::run(&cps(stem), &mch, &cnc::serialize(records))
        .unwrap_or_else(|e| panic!("{stem}: must post flipped yaw: {e}"));
    assert!(out.warnings.is_empty(), "{stem}: {:?}", out.warnings);
    out.program
}

fn rotate(p: [f64; 2], angle: f64) -> [f64; 2] {
    let (s, c) = angle.to_radians().sin_cos();
    [c * p[0] - s * p[1], s * p[0] + c * p[1]]
}

/// Execute the controller rotation before the reversed slides are read by
/// the existing independent trace reader. Rotation blocks are coordinate
/// declarations, never tool motion. IJ are relative vectors, not positions.
fn execute_rotation(program: &str, wrong_angle: bool) -> String {
    let mut angle = 0.0;
    let mut pos = [0.0; 2];
    let mut lines = Vec::new();
    for raw in program.lines() {
        let line = raw.split(';').next().unwrap_or("").trim();
        if line.starts_with('(') || line.is_empty() {
            continue;
        }
        let tokens: Vec<&str> = line.split_whitespace().collect();
        let word = |letter: char| {
            tokens
                .iter()
                .find_map(|t| t.strip_prefix(letter)?.parse::<f64>().ok())
        };
        let g = |n: f64| {
            tokens
                .iter()
                .any(|t| t.strip_prefix('G').and_then(|v| v.parse::<f64>().ok()) == Some(n))
        };
        if g(68.0) || tokens.contains(&"ROT") {
            angle = if g(68.0) {
                assert_eq!(word('X'), Some(0.0));
                assert_eq!(word('Y'), Some(0.0));
                word('R').expect("G68 angle")
            } else {
                tokens
                    .iter()
                    .find_map(|t| t.strip_prefix("RPL=")?.parse::<f64>().ok())
                    .unwrap_or(0.0)
            };
            if wrong_angle {
                angle = -angle;
            }
            continue;
        }
        if g(69.0) {
            angle = 0.0;
            continue;
        }
        if g(28.0) || g(30.0) || g(53.0) {
            lines.push(line.to_owned());
            continue;
        }
        let xy = word('X').is_some() || word('Y').is_some();
        let ij = word('I').is_some() || word('J').is_some();
        let mut out: Vec<String> = tokens
            .iter()
            .filter(|t| {
                !(xy && (t.starts_with('X') || t.starts_with('Y'))
                    || ij && (t.starts_with('I') || t.starts_with('J')))
            })
            .map(|t| (*t).to_owned())
            .collect();
        if xy {
            pos = [word('X').unwrap_or(pos[0]), word('Y').unwrap_or(pos[1])];
            let [x, y] = rotate(pos, angle);
            out.extend([format!("X{x:.8}"), format!("Y{y:.8}")]);
        }
        if ij {
            let [i, j] = rotate([word('I').unwrap_or(0.0), word('J').unwrap_or(0.0)], angle);
            out.extend([format!("I{i:.8}"), format!("J{j:.8}")]);
        }
        lines.push(out.join(" "));
    }
    lines.join("\n")
}

#[test]
fn rotation_reader_calibrates_against_unflipped_cam_paths() {
    let want = cam_trace(&section(None, 30.0, false));
    assert!(
        !want.engages.is_empty(),
        "the fixture exercises compensation"
    );
    for stem in ["fanuc", "haas", "mazak", "doosan-dnm", "sinumerik"] {
        for control in [false, true] {
            let program = post(stem, &section(None, 30.0, control));
            let got = controller_trace(&execute_rotation(&program, false), [false; 3], DIALECT);
            assert_same_trace(
                &want,
                &got,
                0.006,
                &format!("reader calibration {stem}, Control={control}"),
            );
        }
    }
}

#[test]
fn flipped_yaw_bake_traces_the_cam_path() {
    for yaw in [30.0, -37.0, 90.0] {
        let want = cam_trace(&section(None, yaw, false));
        for stem in ["fanuc", "haas", "mazak", "doosan-dnm", "sinumerik"] {
            for (letters, flip) in CASES {
                let program = post(stem, &section(Some(letters), yaw, false));
                assert!(
                    !program.contains("G68") && !program.contains("ROT RPL="),
                    "{program}"
                );
                assert_same_trace(
                    &want,
                    &controller_trace(&program, *flip, DIALECT),
                    0.004,
                    &format!("{stem} Bake {letters} yaw {yaw}"),
                );
            }
        }
    }
}

#[test]
fn flipped_yaw_control_traces_the_cam_path_in_each_rotation_dialect() {
    for yaw in [30.0, -37.0, 90.0] {
        let want = cam_trace(&section(None, yaw, false));
        for stem in ["fanuc", "haas", "mazak", "doosan-dnm", "sinumerik"] {
            for (letters, flip) in CASES {
                let program = post(stem, &section(Some(letters), yaw, true));
                let angle = if flip[0] != flip[1] { -yaw } else { yaw };
                let block = if stem == "sinumerik" {
                    format!("ROT RPL={angle}")
                } else {
                    format!("G68 X0. Y0. R{angle}.")
                };
                assert!(
                    program.contains(&block),
                    "{stem}: mirrored frame angle: {block}\n{program}"
                );
                let got = controller_trace(&execute_rotation(&program, false), *flip, DIALECT);
                assert_same_trace(
                    &want,
                    &got,
                    0.006,
                    &format!("{stem} Control {letters} yaw {yaw}"),
                );
                let wrong = controller_trace(&execute_rotation(&program, true), *flip, DIALECT);
                assert_ne!(
                    got.cuts, wrong.cuts,
                    "the reader must detect the opposite rotation angle"
                );
            }
        }
    }
}
