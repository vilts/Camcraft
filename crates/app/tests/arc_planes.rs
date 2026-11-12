//! M11.2 A: the arc's plane end to end - CL dump and its reader, the
//! `.cnc` stream, the project file's arc key, and all five built-in
//! posts on one synthetic program carrying a G18 and a G19 arc between
//! XY ones. Conformance goldens: `golden/nc/arc_planes.nc` (Fanuc, the
//! top-level law) and `golden/nc/<dialect>/arc_planes.nc`; the CL
//! golden is `golden/arc_planes.cl`. The XY-only law - every program
//! whose arcs are all XY posts exactly what it posted before - is the
//! existing conformance matrix itself, plus the explicit no-plane-word
//! sweep below. Provenance: RS-274/NGC and ISO 6983 plane selection
//! (G17/G18/G19) and G2/G3 direction; the control families' public
//! programming conventions (behavior only).
//!
//! Regenerate after an intentional change:
//! `WRITE_GOLDEN=1 cargo test -p camcraft-app --test arc_planes`
//!
//! Gate map (M4.4): covers cl::cl, post::post, app::project.

mod common;

use camcraft_app::cl::{ArcPlane, CycleKind, MoveKind, PathBuilder, Toolpath};
use camcraft_app::geo2d::Point2;
use camcraft_app::geometry::Point3;
use camcraft_app::post::{cnc, dump, engine, Dump};
use camcraft_app::project::{ArcDto, ArcPlaneDto};
use std::path::{Path, PathBuf};

const POSTS: &[(&str, &str, &str)] = &[
    ("Fanuc", "fanuc", ""),
    ("Haas", "haas", "haas"),
    ("Siemens 840D", "sinumerik", "sinumerik"),
    ("Heidenhain", "heidenhain", "heidenhain"),
    ("Mazak", "mazak", "mazak"),
];

fn repo(rel: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../").join(rel)
}

fn read(rel: &str) -> String {
    std::fs::read_to_string(repo(rel))
        .unwrap_or_else(|e| panic!("{rel}: {e}"))
        .replace("\r\n", "\n")
}

fn p(x: f64, y: f64, z: f64) -> Point3 {
    Point3::new(x, y, z)
}

fn c(x: f64, y: f64) -> Point2 {
    Point2::new(x, y)
}

/// Down, roll onto the floor with a G18 G3 quarter (it LOOKS clockwise
/// drawn X-right/Z-up - the +Y view rule), cut, an XY G2, roll off
/// with a G19 G2 quarter rising in Z, a line (the plane stays modal
/// through it), an XY G3 (G17 comes back), up.
fn program() -> Toolpath {
    let mut b = PathBuilder::new(p(20.0, 0.0, 15.0));
    b.push(MoveKind::Rapid, p(20.0, 0.0, 5.0), None);
    b.push_arc_in(MoveKind::Feed, p(15.0, 0.0, 0.0), Some(600.0), c(15.0, 5.0), true, ArcPlane::Xz);
    b.push(MoveKind::Feed, p(0.0, 0.0, 0.0), Some(800.0));
    b.push_arc(MoveKind::Feed, p(-5.0, 5.0, 0.0), Some(800.0), c(0.0, 5.0), false);
    b.push_arc_in(MoveKind::Feed, p(-5.0, 10.0, 5.0), Some(600.0), c(10.0, 0.0), false, ArcPlane::Yz);
    b.push(MoveKind::Feed, p(0.0, 10.0, 5.0), Some(800.0));
    b.push_arc(MoveKind::Feed, p(5.0, 15.0, 5.0), Some(800.0), c(0.0, 15.0), true);
    b.push(MoveKind::Rapid, p(5.0, 15.0, 15.0), None);
    b.finish()
}

fn stream(t: &Toolpath) -> String {
    let d = Dump {
        op_name: "Plane arcs",
        tool_number: 1,
        tool_name: "6MM BALL",
        tool_diameter: 6.0,
        rpm: 10000.0,
        clearance_z: 15.0,
        retract_z: 10.0,
        top_z: 5.0,
        ..Dump::default()
    };
    cnc::serialize(&dump(t, &d))
}

fn mch() -> serde_json::Value {
    serde_json::from_str(&read("assets/machines/3axis-vmc.mch")).expect("machine json")
}

fn post(stem: &str, stream: &str) -> String {
    let cps = read(&format!("assets/posts/{stem}.cps"));
    let out = engine::run(&cps, &mch(), stream).unwrap_or_else(|e| panic!("{stem}: must post: {e}"));
    assert!(out.warnings.is_empty(), "{stem}: the gate counts warnings: {:?}", out.warnings);
    out.program
}

#[test]
fn the_cl_dump_states_the_plane_and_reads_back() {
    let t = program();
    assert_eq!(t.arc_count(), 4, "two vertical, two XY - none split");
    let text = common::gen_to_cltext(&t);
    assert!(text.contains(" I15000 K5000 S1"), "the G18 arc dumps I/K:\n{text}");
    assert!(text.contains(" J10000 K0 S-1"), "the G19 arc dumps J/K:\n{text}");
    assert!(text.contains(" I0 J5000 S-1"), "an XY arc dumps exactly as ever:\n{text}");
    common::write_golden("arc_planes.cl", &text);
    let back = common::cl_read::parse_cl(&text).unwrap_or_else(|e| panic!("{e}"));
    // The dump speaks microns; compare on the lattice it speaks.
    let l = camcraft_app::cl::lattice;
    let lp = |q: Point3| (l(q.x), l(q.y), l(q.z));
    assert_eq!(back.len(), t.moves.len());
    for (a, b) in back.iter().zip(&t.moves) {
        assert_eq!((a.kind, lp(a.from), lp(a.to)), (b.kind, lp(b.from), lp(b.to)));
        assert_eq!(a.feed.map(l), b.feed.map(l));
        let arc = |m: &camcraft_app::cl::Move| {
            m.arc.map(|r| (l(r.center.x), l(r.center.y), r.ccw, r.plane))
        };
        assert_eq!(arc(a), arc(b), "the dump is the path, plane and all");
    }
}

#[test]
fn the_stream_round_trips_the_plane() {
    let t = program();
    let s = stream(&t);
    assert!(s.contains("Arc CCW X 15000 Y 0 Z 0 F 600000 I 15000 K 5000\n"), "{s}");
    assert!(s.contains("Arc CW X -5000 Y 10000 Z 5000 F 600000 J 10000 K 0\n"), "{s}");
    assert!(s.contains("Arc CW X -5000 Y 5000 Z 0 F 800000 I 0 J 5000\n"), "{s}");
    let recs = cnc::parse(&s).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(cnc::serialize(&recs), s, "parse . serialize is the identity");
    let planes: Vec<ArcPlane> = recs
        .iter()
        .filter_map(|r| match r {
            cnc::Record::Move(m) if m.center.is_some() => Some(m.plane),
            _ => None,
        })
        .collect();
    assert_eq!(planes, [ArcPlane::Xz, ArcPlane::Xy, ArcPlane::Yz, ArcPlane::Xy]);
    // One center word, or all three, names no plane: refused by name.
    for bad in ["Arc CW X 1000 Y 0 Z 0 I 0\n", "Arc CW X 1000 Y 0 Z 0 I 0 J 0 K 0\n"] {
        let e = cnc::parse(&format!("; HSMW-CNC v1\nSECTION\n{bad}"));
        assert!(e.is_err(), "{bad:?} must refuse");
    }
}

#[test]
fn every_builtin_posts_the_plane_program_byte_exact() {
    let s = stream(&program());
    let write = std::env::var_os("WRITE_GOLDEN").is_some();
    for (name, stem, sub) in POSTS {
        let out = post(stem, &s);
        let rel = if sub.is_empty() {
            "crates/app/tests/golden/nc/arc_planes.nc".to_string()
        } else {
            format!("crates/app/tests/golden/nc/{sub}/arc_planes.nc")
        };
        if write {
            std::fs::write(repo(&rel), &out).unwrap_or_else(|e| panic!("{rel}: {e}"));
            continue;
        }
        assert_eq!(out, read(&rel), "{name}: {rel} is stale; regenerate with WRITE_GOLDEN=1");
    }
}

#[test]
fn the_g_word_posts_state_the_plane_modally() {
    let s = stream(&program());
    for (stem, g2, g3) in [
        ("fanuc", "G02", "G03"),
        ("haas", "G2", "G3"),
        ("sinumerik", "G2", "G3"),
        ("mazak", "G02", "G03"),
    ] {
        let out = post(stem, &s);
        // Sinumerik numbers its blocks; read past the N word.
        let lines: Vec<&str> = out
            .lines()
            .map(|l| match l.split_once(' ') {
                Some((n, rest)) if n.starts_with('N') && n[1..].bytes().all(|b| b.is_ascii_digit()) => rest,
                _ => l,
            })
            .collect();
        let has = |want: &str| lines.contains(&want);
        assert!(has(&format!("G18 {g3} X15 Y0 Z0 I-5 K0 F600")), "{stem}:\n{out}");
        assert!(has(&format!("G17 {g2} X-5 Y5 Z0 I0 J5 F800")), "{stem}: G17 returns:\n{out}");
        assert!(has(&format!("G19 {g2} X-5 Y10 Z5 J5 K0 F600")), "{stem}:\n{out}");
        assert!(has(&format!("G17 {g3} X5 Y15 Z5 I0 J5 F800")), "{stem}:\n{out}");
        let words = |w: &str| lines.iter().filter(|l| l.split(' ').any(|x| x == w)).count();
        assert_eq!(words("G18"), 1, "{stem}: G18 once - modal");
        assert_eq!(words("G19"), 1, "{stem}: G19 once - modal");
    }
}

/// The Heidenhain blocks' words (#50: `N<n> <words>*`, TNC 640 ISO
/// manual 892909-25 p. 130), the number and the terminator stripped.
fn hh_blocks(out: &str) -> Vec<&str> {
    out.lines()
        .filter_map(|l| l.strip_prefix('N')?.split_once(' ')?.1.strip_suffix('*'))
        .collect()
}

/// The Heidenhain G01 fan of one vertical arc: the F600 blocks whose
/// normal coordinate (`fixed`, at `at`) holds, read as in-plane pairs.
fn hh_fan(out: &str, fixed: char, at: f64, a: char, b: char) -> Vec<(f64, f64)> {
    hh_blocks(out)
        .into_iter()
        .filter(|l| l.starts_with("G01 ") && l.ends_with("F600"))
        .filter_map(|l| {
            let g = |k: char| {
                l.split(' ')
                    .find(|w| w.starts_with(k))
                    .and_then(|w| w[1..].parse::<f64>().ok())
            };
            (g(fixed) == Some(at)).then_some((g(a)?, g(b)?))
        })
        .collect()
}

/// #18 (final): this DIN/ISO dialect posts no XZ/YZ arc word - both
/// vertical planes ride 1 um chord fans that sit on the arc's circle
/// and sweep the arc's own sense (a mirrored fan would gouge).
#[test]
fn heidenhain_rides_its_vertical_arcs_as_chords_and_never_switches_plane() {
    let out = post("heidenhain", &stream(&program()));
    assert!(!out.contains("G18") && !out.contains("G19"), "no plane switch:
{out}");
    // The XY arcs stay arcs; the G18 G3 quarter (r 5) is a fine G1 fan
    // whose every point sits on its circle in XZ about (X15, Z5), on
    // the short way round seen from +Y: X15..20, Z0..5.
    // #49: the centre is its own block, absolute (TNC 640 ISO manual
    // 892909-25 p. 260); here centre (0, 5) happens to equal the
    // increment from the start (0, 0).
    let lines = hh_blocks(&out);
    assert!(lines.windows(2).any(|w| w == ["I+0 J+5", "G02 X-5 Y+5 Z+0 F800"]), "{out}");
    let fan = hh_fan(&out, 'Y', 0.0, 'X', 'Z');
    assert!(fan.len() > 20, "a fine fan, {} chords", fan.len());
    for (x, z) in &fan {
        let r = ((x - 15.0).powi(2) + (z - 5.0).powi(2)).sqrt();
        assert!((r - 5.0).abs() < 1e-3, "({x}, {z}) is on the circle");
        assert!(*x >= 15.0 - 1e-9 && *z <= 5.0 + 1e-9, "({x}, {z}) sweeps the G3 sense");
    }
    assert_eq!(fan.last(), Some(&(15.0, 0.0)), "the fan ends on the arc's end");
    // The G19 G2 quarter: about (Y10, Z0) at X-5, from (Y5, Z0) up to
    // (Y10, Z5) - clockwise seen from +X, so Y5..10, Z0..5.
    let fan = hh_fan(&out, 'X', -5.0, 'Y', 'Z');
    assert!(fan.len() > 20, "a fine YZ fan, {} chords", fan.len());
    for (y, z) in &fan {
        let r = ((y - 10.0).powi(2) + z.powi(2)).sqrt();
        assert!((r - 5.0).abs() < 1e-3, "({y}, {z}) is on the circle");
        assert!(*y <= 10.0 + 1e-9 && *z >= -1e-9, "({y}, {z}) sweeps the G2 sense");
    }
    assert_eq!(fan.last(), Some(&(10.0, 5.0)), "the YZ fan ends on the arc's end");
}

#[test]
fn the_plane_goes_back_to_g17_before_a_cycle_and_at_section_end() {
    use cnc::Record as R;
    let mv = |x: f64, y: f64, z: f64, center: Option<(f64, f64)>, plane: ArcPlane| {
        R::Move(cnc::MoveRec {
            kind: if center.is_some() { MoveKind::Feed } else { MoveKind::Rapid },
            x,
            y,
            z,
            feed: center.map(|_| 500.0),
            center,
            ccw: true,
            plane,
            abc: None,
        })
    };
    let s = cnc::serialize(&[
        R::Tool { number: 1, name: "DRILL".into() },
        R::Section { begin: true },
        mv(10.0, 0.0, 5.0, None, ArcPlane::Xy),
        mv(5.0, 0.0, 0.0, Some((5.0, 5.0)), ArcPlane::Xz),
        mv(20.0, 0.0, 5.0, None, ArcPlane::Xy),
        R::Cycle(cnc::CycleRec {
            kind: CycleKind::Drill,
            x: 20.0,
            y: 0.0,
            r_z: 2.0,
            bottom_z: -5.0,
            entry_z: 0.0,
            peck: 0.0,
            dwell: 0.0,
            first_move: 3,
            feed: Some(100.0),
        }),
        mv(20.0, 0.0, 5.0, None, ArcPlane::Xy),
        R::Section { begin: false },
        R::Section { begin: true },
        mv(30.0, 0.0, 5.0, None, ArcPlane::Xy),
        mv(30.0, 5.0, 0.0, Some((5.0, 5.0)), ArcPlane::Yz),
        R::Section { begin: false },
    ]);
    for stem in ["fanuc", "haas", "sinumerik", "mazak"] {
        let out = post(stem, &s);
        let at = |pat: &str| out.lines().position(|l| l.contains(pat));
        let (g18, cyc) = (at("G18"), at("G81"));
        let back = out.lines().enumerate().filter(|(_, l)| l.ends_with("G17")).map(|(i, _)| i);
        let backs: Vec<usize> = back.collect();
        assert!(g18.is_some() && cyc.is_some(), "{stem}:\n{out}");
        assert!(
            backs.iter().any(|&i| Some(i) > g18 && Some(i) < cyc),
            "{stem}: G17 between the G18 arc and the cycle:\n{out}"
        );
        let g19 = at("G19");
        let m5 = out.lines().enumerate().filter(|(_, l)| l.ends_with("M5") || l.ends_with("MOTOFF")).map(|(i, _)| i).last();
        assert!(
            backs.iter().any(|&i| Some(i) > g19 && Some(i) < m5),
            "{stem}: G17 before the section's spindle stop:\n{out}"
        );
    }
}

#[test]
fn no_xy_only_golden_posts_a_plane_word() {
    // The byte law is the conformance matrix (every golden, every post);
    // this is its plain-language face: an XY-only program states no
    // G18/G19, and every committed golden is XY-only.
    for g in ["contour_disc_hole", "contour_tangential_lead", "face_square_40x40", "drill_peck"] {
        let s = read(&format!("crates/app/tests/golden/{g}.cl"));
        for (_, stem, _) in POSTS {
            let out = post(stem, &s);
            assert!(!out.contains("G18") && !out.contains("G19"), "{stem} on {g}");
        }
    }
}

#[test]
fn the_project_file_states_only_a_vertical_plane() {
    let xy = ArcDto { center: [1.0, 2.0], ccw: true, plane: ArcPlaneDto::Xy };
    let xz = ArcDto { center: [1.0, 2.0], ccw: false, plane: ArcPlaneDto::Xz };
    let sxy = serde_json::to_string(&xy).unwrap_or_default();
    let sxz = serde_json::to_string(&xz).unwrap_or_default();
    assert_eq!(sxy, r#"{"center":[1.0,2.0],"ccw":true}"#, "XY writes what it always wrote");
    assert_eq!(sxz, r#"{"center":[1.0,2.0],"ccw":false,"plane":"xz"}"#);
    let old: ArcDto = serde_json::from_str(r#"{"center":[1.0,2.0],"ccw":true}"#).expect("pre-M11.2 arc");
    assert_eq!(old, xy, "absent is XY");
    let back: ArcDto = serde_json::from_str(&sxz).expect("xz");
    assert_eq!(back, xz);
    let yz: ArcDto = serde_json::from_str(r#"{"center":[0.0,0.0],"ccw":true,"plane":"yz"}"#).expect("yz");
    assert_eq!(yz.plane, ArcPlaneDto::Yz);
}
