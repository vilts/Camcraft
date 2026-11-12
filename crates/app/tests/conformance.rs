//! The built-in post conformance law (M3.3 B): every shipped `.cps`
//! posts every committed `.cl` golden to byte-identical G-code. This
//! is OUR conformance - the goldens lock each dialect's decisions
//! (word style, framing, cycle spelling), not any vendor's output; a
//! script change that moves a byte shows up here as a diff, and the
//! diff is the review (UPDATE workflow per tests/golden/CLAUDE.md).
//!
//! Regenerate after an intentional change:
//! `WRITE_GOLDEN=1 cargo test -p camcraft-app --test conformance`
//! Fanuc's goldens live at the top of `golden/nc/` (M1.7b law, the
//! suite `fanuc_post.rs` already locks); the four stage-B dialects get
//! one subdirectory each.
//!
//! Gate map (M4.4): covers cl::cl, post::post, app::posting.

use camcraft_app::cl::{CycleKind, MoveKind};
use camcraft_app::post::{cnc, engine};
use serde_json::json;
use std::path::{Path, PathBuf};

fn repo(rel: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../")
        .join(rel)
}

fn read(rel: &str) -> String {
    std::fs::read_to_string(repo(rel)).unwrap_or_else(|e| panic!("{rel}: {e}"))
}

const GOLDENS: &[&str] = &[
    "contour_disc_hole",
    "contour_plate_with_hole",
    "contour_tangential_lead",
    "face_square_40x40",
    "face_square_40x40_stepped",
    "pocket_waterline_plate_with_island",
    "pocket_zigzag_plate_with_island",
    "slot_fullwidth_centerline",
    "slot_wide_channel_rows",
    "trace_plate_with_hole",
    "drill_plate",
    "drill_peck",
    "tap_plate",
];

/// (registry name, .cps file, golden subdir - "" means the shared top).
/// The MILLING dialects: the `.cl` goldens are mill programs (XY
/// contours, Z-down holes), so these are the posts they conform on.
const POSTS: &[(&str, &str, &str)] = &[
    ("Fanuc", "fanuc", ""),
    ("Haas", "haas", "haas"),
    ("Siemens 840D", "sinumerik", "sinumerik"),
    ("Heidenhain", "heidenhain", "heidenhain"),
    ("Mazak", "mazak", "mazak"),
    ("Tormach PathPilot", "tormach", "tormach"),
    ("Doosan DNM", "doosan-dnm", "doosan-dnm"),
    ("Makera Carvera", "carvera", "carvera"),
];

/// (post stem, golden, named reason): a golden a dialect must REFUSE
/// rather than post, so it has no `.nc` there. Heidenhain (#51) supports
/// tapping, but this legacy golden lacks spindle speed. Carvera (#707)
/// cannot tap (no canned cycles or spindle reverse).
const REFUSED: &[(&str, &str, &str)] = &[
    ("heidenhain", "tap_plate", "tapping needs a positive integral spindle speed"),
    ("carvera", "tap_plate", "tapping"),
];

/// The TURNING dialects (M13.5): their goldens are the lathe
/// documents' own streams (`lathe_post.rs`); on a mill golden they
/// must REFUSE by name - never project a Y move onto a diameter.
const LATHE_POSTS: &[(&str, &str)] = &[("Fanuc Lathe", "fanuc-lathe"), ("Tormach PathPilot Lathe", "tormach-lathe")];

/// The PLASMA dialects (#671): their goldens are the plasma streams'
/// own (`plasma_post.rs`, `nc/tormach-plasma/`); on a mill golden they
/// refuse by name (`plasma_post_refusals.rs`).
const PLASMA_POSTS: &[(&str, &str)] = &[("Tormach PathPilot Plasma", "tormach-plasma")];

#[test]
fn every_lathe_post_refuses_every_mill_golden_by_name() {
    let mch: serde_json::Value =
        serde_json::from_str(&read("assets/machines/lathe.mch")).expect("machine json");
    for (name, cps_stem) in LATHE_POSTS {
        let cps = read(&format!("assets/posts/{cps_stem}.cps"));
        for g in GOLDENS {
            let stream = read(&format!("crates/app/tests/golden/{g}.cl"));
            let out = engine::run(&cps, &mch, &stream)
                .unwrap_or_else(|e| panic!("{name} on {g}: the engine runs, the script refuses: {e}"));
            // A path that leaves every meridian is milling work; a
            // path that happens to ride one (the centerline slot) is
            // still a legacy stream with no spindle, which G99 feed
            // per revolution cannot pace - either way, named.
            assert!(
                out.warnings
                    .iter()
                    .any(|w| w.contains("milling work") || w.contains("needs a spindle speed")),
                "{name} on {g}: a mill program must be refused by name, got {:?}",
                out.warnings
            );
        }
    }
}

#[test]
fn every_builtin_posts_every_golden_byte_exact() {
    let mch: serde_json::Value =
        serde_json::from_str(&read("assets/machines/3axis-vmc.mch")).expect("machine json");
    let write = std::env::var_os("WRITE_GOLDEN").is_some();
    let mut written = 0usize;
    for (name, cps_stem, sub) in POSTS {
        let cps = read(&format!("assets/posts/{cps_stem}.cps"));
        for g in GOLDENS {
            let stream = read(&format!("crates/app/tests/golden/{g}.cl"));
            let out = engine::run(&cps, &mch, &stream)
                .unwrap_or_else(|e| panic!("{name} on {g}: must post: {e}"));
            if let Some((_, _, why)) = REFUSED.iter().find(|(s, gg, _)| s == cps_stem && gg == g) {
                assert!(
                    out.warnings.iter().any(|w| w.contains(why)),
                    "{name} on {g}: must refuse by name ({why}), got {:?}",
                    out.warnings
                );
                continue;
            }
            assert!(
                out.warnings.is_empty(),
                "{name} on {g}: the gate counts warnings: {:?}",
                out.warnings
            );
            let rel = if sub.is_empty() {
                format!("crates/app/tests/golden/nc/{g}.nc")
            } else {
                format!("crates/app/tests/golden/nc/{sub}/{g}.nc")
            };
            let path = repo(&rel);
            if write {
                // The golden_cl workflow: WRITE_GOLDEN rewrites and the
                // test passes quietly - the review is the git diff, not
                // a red run.
                std::fs::create_dir_all(path.parent().unwrap()).expect("golden dir");
                std::fs::write(&path, &out.program).unwrap_or_else(|e| panic!("{rel}: {e}"));
                written += 1;
                continue;
            }
            let want = read(&rel);
            assert_eq!(
                out.program, want,
                "{name}: {rel} is stale; regenerate with WRITE_GOLDEN=1"
            );
        }
    }
    let _ = written;
}

/// A stream the legacy `.cl` goldens cannot be: params (spindle, work
/// offset, slot origin), a tool block, a contour (rapid, feed, arc) and
/// a five-hole drilling group - dwelled drill, a two-hole peck run
/// (the modal-continuation branch), a deep hole, a tap. Every
/// dialect-specific word - `S.. M3`, `MOTON`, `T2 M6`, `G43 H2`, `D2`,
/// `G98`/its absence, `P..` - appears here, so the goldens that lock
/// them exist for all five scripts.
///
/// Each cycle anchors (`AT`) on the opening rapid of its own hole
/// chain, exactly as a v1 dump from the generator lays one out: that
/// is what puts the contour BEFORE the cycle group, where the
/// chain-suppression law (`if (inCycle) return`) leaves it alone.
/// Anchor every cycle at move 0 instead and normalization re-seats
/// the whole group ahead of the contour, whose moves then die in
/// that same guard - the stream must be shaped like a real one.
///
/// #51: each hole carries its real expanded chain (approach, plunges,
/// peck retracts, exit), matching the generator's `drilling::cycle` shape.
/// Cycle-capable posts suppress the expanded cuts; Heidenhain keeps safe
/// links and replaces its native cycle's cuts. The Heidenhain variant now
/// uses this same G55 + rigid-tap stream as the others; its one remaining
/// golden refusal is `tap_plate`, whose legacy stream does not carry spindle speed.
fn synth_stream_with(wcs: &str, tap: bool) -> String {
    use cnc::Record as R;
    let rec = |kind: MoveKind, x: f64, y: f64, z: f64, feed: Option<f64>| {
        R::Move(cnc::MoveRec {
            kind,
            x,
            y,
            z,
            feed,
            center: None,
            ccw: false,
            plane: Default::default(),
            abc: None,
        })
    };
    let mut recs = vec![
        R::Param { name: "SpindleSpeed".into(), value: json!(12000) },
        R::Param { name: "WorkOffset".into(), value: json!(wcs) },
        R::Param { name: "OriginX".into(), value: json!(10.0) },
        R::Param { name: "OriginY".into(), value: json!(20.0) },
        R::Param { name: "OriginZ".into(), value: json!(5.0) },
        // #1028: the planes a generator states for every section (as in
        // the recorded drill_retract stream): the clearance, and the hop
        // the holes traverse at - above their R plane 2. The holes below
        // carry no leaving rapids, so each cycle is seated at this hop.
        R::Param { name: "ClearancePlane".into(), value: json!(10.0) },
        R::Param { name: "RetractPlane".into(), value: json!(5.0) },
        R::Tool { number: 2, name: "10MM END MILL".into() },
        R::Section { begin: true },
        // The contour: moves 0..2, no cycle anchored at or below 2
        // until the hole group opens, so all three post as motion.
        rec(MoveKind::Rapid, 30.0, 30.0, 2.0, None),
        rec(MoveKind::Feed, 40.0, 30.0, -1.0, Some(800.0)),
        R::Move(cnc::MoveRec {
            kind: MoveKind::Feed,
            x: 40.0,
            y: 40.0,
            z: -1.0,
            feed: Some(500.0),
            center: Some((40.0, 35.0)),
            ccw: true,
            plane: Default::default(),
            abc: None,
        }),
    ];
    // (kind, x, y, bottom, peck, dwell, feed): R plane 2 for all.
    let mut holes = vec![
        (CycleKind::Drill, 50.0, 50.0, -8.0, 0.0, 0.5, 120.0),
        (CycleKind::Peck, 60.0, 50.0, -20.0, 5.0, 0.0, 150.0),
        (CycleKind::Peck, 60.0, 60.0, -20.0, 5.0, 0.0, 150.0),
        (CycleKind::Deep, 70.0, 60.0, -30.0, 8.0, 0.0, 90.0),
    ];
    if tap {
        holes.push((CycleKind::Tap, 70.0, 70.0, -12.0, 0.0, 0.0, 240.0));
    }
    let r_z = 2.0;
    let mut n = 3usize; // the contour took moves 0..2
    for (kind, x, y, bottom, peck, dwell, feed) in holes {
        recs.push(R::Cycle(cnc::CycleRec {
            kind,
            x,
            y,
            r_z,
            bottom_z: bottom,
            entry_z: 0.0,
            peck,
            dwell,
            first_move: n,
            feed: Some(feed),
        }));
        let mut chain = vec![rec(MoveKind::Rapid, x, y, r_z, None)];
        match kind {
            CycleKind::Peck | CycleKind::Deep => {
                let mut t = (r_z - peck).max(bottom);
                loop {
                    chain.push(rec(MoveKind::Plunge, x, y, t, Some(feed)));
                    if t <= bottom {
                        break;
                    }
                    let up = if kind == CycleKind::Peck { (t + 1.0).min(r_z) } else { r_z };
                    chain.push(rec(MoveKind::Rapid, x, y, up, None));
                    t = (t - peck).max(bottom);
                }
                chain.push(rec(MoveKind::Rapid, x, y, r_z, None));
            }
            CycleKind::Tap | CycleKind::FloatingTap => {
                chain.push(rec(MoveKind::Feed, x, y, bottom, Some(feed)));
                chain.push(rec(MoveKind::Feed, x, y, r_z, Some(feed)));
            }
            _ => {
                chain.push(rec(MoveKind::Plunge, x, y, bottom, Some(feed)));
                chain.push(rec(MoveKind::Rapid, x, y, r_z, None));
            }
        }
        n += chain.len();
        recs.extend(chain);
    }
    recs.push(R::Section { begin: false });
    cnc::serialize(&recs)
}

fn synth_stream() -> String {
    synth_stream_with("G55", true)
}

#[test]
fn the_synthesized_stream_locks_the_tool_and_spindle_words() {
    let mch: serde_json::Value =
        serde_json::from_str(&read("assets/machines/3axis-vmc.mch")).expect("machine json");
    let write = std::env::var_os("WRITE_GOLDEN").is_some();
    for (name, cps_stem, _) in POSTS {
        // #51: Heidenhain supports the full G55 + tap stream.
        // #707: the Carvera refuses the tap the same way (no canned
        // cycles, no spindle reverse); its G55 stands.
        let stream = match *cps_stem {
            "carvera" => synth_stream_with("G55", false),
            _ => synth_stream(),
        };
        let cps = read(&format!("assets/posts/{cps_stem}.cps"));
        let out = engine::run(&cps, &mch, &stream)
            .unwrap_or_else(|e| panic!("{name} on the synth stream: must post: {e}"));
        assert!(
            out.warnings.is_empty(),
            "{name} synth: the gate counts warnings: {:?}",
            out.warnings
        );
        let rel = format!("crates/app/tests/golden/nc/synth_{cps_stem}.nc");
        let path = repo(&rel);
        if write {
            std::fs::write(&path, &out.program).unwrap_or_else(|e| panic!("{rel}: {e}"));
            continue;
        }
        let want = read(&rel);
        assert_eq!(
            out.program, want,
            "{name}: {rel} is stale; regenerate with WRITE_GOLDEN=1"
        );
    }
}

#[test]
fn the_dialects_actually_differ() {
    // A registry of five byte-identical scripts would pass the matrix
    // while shipping one post - the diversity check is part of the
    // conformance, so compare every pair on one stream (drill_peck:
    // cycles, tool change, spindle, the lot).
    let mch: serde_json::Value =
        serde_json::from_str(&read("assets/machines/3axis-vmc.mch")).expect("machine json");
    let stream = read("crates/app/tests/golden/drill_peck.cl");
    let texts: Vec<(&str, String)> = POSTS
        .iter()
        .map(|(name, stem, _)| {
            let cps = read(&format!("assets/posts/{stem}.cps"));
            (
                *name,
                engine::run(&cps, &mch, &stream)
                    .unwrap_or_else(|e| panic!("{name} on drill_peck: {e}"))
                    .program,
            )
        })
        .collect();
    for (i, (a, x)) in texts.iter().enumerate() {
        for (b, y) in &texts[i + 1..] {
            assert_ne!(x, y, "posts {a} and {b} emit identical programs");
        }
    }
}

#[test]
fn every_builtin_registry_entry_compiles() {
    // The registry text the app embeds must match the shipped files -
    // a stale include or a renamed file is a compile-time... no:
    // include_str! is compile-time, but the NAMES are the persistence
    // contract, so spell the registry out here against posting.rs.
    let names: Vec<&str> = camcraft_app::posting::BUILTINS.iter().map(|(n, _)| *n).collect();
    let mut want: Vec<&str> = POSTS.iter().map(|(n, _, _)| *n).collect();
    want.extend(LATHE_POSTS.iter().map(|(n, _)| *n));
    want.extend(PLASMA_POSTS.iter().map(|(n, _)| *n));
    assert_eq!(names, want, "the picker lists what the registry ships");
    for (n, _) in PLASMA_POSTS {
        assert!(camcraft_app::posting::PLASMA_POSTS.contains(n), "{n} is a plasma dialect in the registry's own table");
    }
    for (n, _) in LATHE_POSTS {
        assert!(
            camcraft_app::posting::LATHE_POSTS.contains(n),
            "{n} is a turning dialect in the registry's own table"
        );
    }
}

/// (.cps file, the file extension its manifest declares). The engine
/// never reads `extension`, so nothing else would catch a wrong one:
/// #52 - a TNC ISO program is a `.I` file (HEIDENHAIN "TNC 640 User's
/// Manual, ISO programming", ID 892909-25, p. 143); the rest write `.nc`.
const EXTENSIONS: &[(&str, &str)] = &[
    ("fanuc", "nc"),
    ("haas", "nc"),
    ("sinumerik", "nc"),
    ("heidenhain", "I"),
    ("mazak", "nc"),
    ("tormach", "nc"),
    ("doosan-dnm", "nc"),
    ("carvera", "nc"),
    ("fanuc-lathe", "nc"),
    ("tormach-lathe", "nc"),
    ("tormach-plasma", "nc"),
];

/// The top-level `extension = "..."` manifest line's value.
fn declared_extension(script: &str) -> Option<&str> {
    script
        .lines()
        .find_map(|l| l.strip_prefix("extension = \"")?.split_once('"').map(|(e, _)| e))
}

#[test]
fn every_builtin_declares_its_controls_file_extension() {
    let mut stems: Vec<&str> = POSTS.iter().map(|(_, s, _)| *s).collect();
    stems.extend(LATHE_POSTS.iter().map(|(_, s)| *s));
    stems.extend(PLASMA_POSTS.iter().map(|(_, s)| *s));
    let pinned: Vec<&str> = EXTENSIONS.iter().map(|(s, _)| *s).collect();
    assert_eq!(pinned, stems, "every shipped post has its extension pinned");
    for (stem, want) in EXTENSIONS {
        let cps = read(&format!("assets/posts/{stem}.cps"));
        assert_eq!(
            declared_extension(&cps),
            Some(*want),
            "{stem}.cps declares the .{want} extension"
        );
    }
}
