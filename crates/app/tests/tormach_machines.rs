//! #170: the Tormach mill family's machine files - each states its own
//! operator's manual figures (travel, spindle, belt ranges, feeds, rigid
//! tapping, coolant) - and the PathPilot post speaking them: the belt note
//! from the machine's own ranges, a rigid tap refused by name on a machine
//! that cannot rigid-tap.
//! Provenance: our own tests over our own machine files and post; the
//! figures are each machine's Tormach operator's manual, § "Machine
//! Specifications" and § "Performance Expectations" (UM10587 770MX MF,
//! UM10864 770MX ML, UM10539 770M, UM10586 1100MX, UM10811 1500MX,
//! UM10372 PCNC 440, UM10540 0626A 1100M); the Mach3-era R8 mills' from § 9.4 / 9.1
//! "Specifications" (31725 Rev B1-4 PCNC 770, 32426 Rev C1-1 PCNC 770
//! Series 3, 30625 Rev C4-1
//! PCNC 1100 Series I, 31138 Rev B1-4 PCNC 1100 Series II, UM10087 1013A
//! PCNC 1100 Series 3).
//! Gate map (M4.4): covers post::machine (spindle ranges, rigid tapping),
//! post::post (tormach.cps), app::posting (MACHINES registry).

use camcraft_app::machine::{Machine, parse_mch};
use camcraft_app::post::{engine, machine};
use camcraft_app::posting::MACHINES;
use std::path::{Path, PathBuf};

const IN: f64 = 25.4;

fn read(rel: &str) -> String {
    let p: PathBuf = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../")
        .join(rel);
    std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("{rel}: {e}"))
}

fn shipped(name: &str) -> Machine {
    let text = MACHINES
        .iter()
        .find(|(n, _)| *n == name)
        .map(|(_, t)| *t)
        .unwrap_or_else(|| panic!("{name} is not a shipped machine"));
    parse_mch(text).unwrap_or_else(|e| panic!("{name}: {e}"))
}

/// The script's view of a shipped machine, as `posting::machine_value`
/// hands it over (parser, then mirror).
fn mirrored(name: &str) -> serde_json::Value {
    machine::mirror(&shipped(name))
}

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-6
}

/// One row of a manual's specification table.
struct Spec {
    name: &'static str,
    travel_in: [f64; 3],
    rpm: (f64, f64),
    kw: f64,
    ipm: (f64, f64),
    ranges: &'static [(f64, f64)],
    rigid: bool,
}

const SPECS: [Spec; 14] = [
    Spec {
        name: "Tormach 770MX",
        travel_in: [14.0, 7.5, 13.25],
        rpm: (175.0, 10000.0),
        kw: 1.1,
        ipm: (300.0, 250.0),
        ranges: &[(175.0, 3250.0), (525.0, 10000.0)],
        rigid: true,
    },
    Spec {
        name: "Tormach 770MX (ML)",
        travel_in: [14.0, 7.5, 13.25],
        rpm: (250.0, 10000.0),
        kw: 1.5,
        ipm: (450.0, 420.0),
        ranges: &[],
        rigid: true,
    },
    Spec {
        name: "Tormach 770M",
        travel_in: [14.0, 7.5, 13.25],
        rpm: (175.0, 10000.0),
        kw: 1.1,
        ipm: (135.0, 110.0),
        ranges: &[(175.0, 3250.0), (525.0, 10000.0)],
        rigid: false,
    },
    Spec {
        name: "Tormach 1100MX",
        travel_in: [18.0, 11.0, 16.25],
        rpm: (70.0, 10000.0),
        kw: 1.5,
        ipm: (300.0, 230.0),
        ranges: &[(70.0, 2000.0), (250.0, 10000.0)],
        rigid: true,
    },
    Spec {
        name: "Tormach 1500MX",
        travel_in: [19.7, 13.8, 14.0],
        rpm: (250.0, 10000.0),
        kw: 3.0,
        ipm: (1200.0, 600.0),
        ranges: &[],
        rigid: true,
    },
    Spec {
        name: "Tormach PCNC 440",
        travel_in: [10.0, 6.25, 10.0],
        rpm: (300.0, 10000.0),
        kw: 0.56,
        ipm: (135.0, 110.0),
        ranges: &[(300.0, 4500.0), (650.0, 10000.0)],
        rigid: false,
    },
    // The PathPilot-era R8 1100M: 70-7500 rpm on two belts, 2 hp (1.5 kW)
    // as the manual's own rating, tension/compression tapping only.
    Spec {
        name: "Tormach 1100M",
        travel_in: [18.0, 11.0, 16.25],
        rpm: (70.0, 7500.0),
        kw: 1.5,
        ipm: (110.0, 90.0),
        ranges: &[(70.0, 2000.0), (250.0, 7500.0)],
        rigid: false,
    },
    // The Mach3-era R8 mills: rated (not peak) motor power, the stated
    // rapid traverse as the feed ceiling, tapping heads only.
    // The PCNC 770 before Series 3 (31725 Rev B1-4): the Series 3's
    // figures except a 130 IPM X/Y rapid.
    Spec {
        name: "Tormach PCNC 770",
        travel_in: [14.0, 7.5, 13.25],
        rpm: (175.0, 10200.0),
        kw: 0.75,
        ipm: (130.0, 100.0),
        ranges: &[(175.0, 3250.0), (525.0, 10200.0)],
        rigid: false,
    },
    Spec {
        name: "Tormach PCNC 770 (Series 3)",
        travel_in: [14.0, 7.5, 13.25],
        rpm: (175.0, 10200.0),
        kw: 0.75,
        ipm: (135.0, 100.0),
        ranges: &[(175.0, 3250.0), (525.0, 10200.0)],
        rigid: false,
    },
    Spec {
        name: "Tormach PCNC 1100 (Series I)",
        travel_in: [18.0, 9.5, 16.5],
        rpm: (300.0, 4500.0),
        kw: 1.12,
        ipm: (65.0, 65.0),
        ranges: &[(300.0, 1750.0), (800.0, 4500.0)],
        rigid: false,
    },
    Spec {
        name: "Tormach PCNC 1100 (Series II)",
        travel_in: [18.0, 9.5, 16.5],
        rpm: (100.0, 5140.0),
        kw: 1.12,
        ipm: (90.0, 65.0),
        ranges: &[(100.0, 2000.0), (250.0, 5140.0)],
        rigid: false,
    },
    Spec {
        name: "Tormach PCNC 1100 (Series 3)",
        travel_in: [18.0, 9.5, 16.5],
        rpm: (100.0, 5140.0),
        kw: 1.12,
        ipm: (110.0, 90.0),
        ranges: &[(100.0, 2000.0), (250.0, 5140.0)],
        rigid: false,
    },    // #618: the M+ is an upgrade kit, not a model with its own manual
    // (Tormach, via the contributor, 2026-11-09): the M's travels,
    // spindle, belts, power and tapping, with the MX's axis speeds.
    Spec {
        name: "Tormach 770M+",
        travel_in: [14.0, 7.5, 13.25],
        rpm: (175.0, 10000.0),
        kw: 1.1,
        ipm: (300.0, 250.0),
        ranges: &[(175.0, 3250.0), (525.0, 10000.0)],
        rigid: false,
    },
    Spec {
        name: "Tormach 1100M+",
        travel_in: [18.0, 11.0, 16.25],
        rpm: (70.0, 7500.0),
        kw: 1.5,
        ipm: (300.0, 230.0),
        ranges: &[(70.0, 2000.0), (250.0, 7500.0)],
        rigid: false,
    },
];

/// #618: an M+ is its M in everything but the axis speeds, which are
/// the matching MX's - so a later fix to either file is caught here.
#[test]
fn an_m_plus_is_its_m_with_the_mx_axis_speeds() {
    for (plus, m, mx) in [
        ("Tormach 770M+", "Tormach 770M", "Tormach 770MX"),
        ("Tormach 1100M+", "Tormach 1100M", "Tormach 1100MX"),
    ] {
        let (p, m, mx) = (shipped(plus), shipped(m), shipped(mx));
        assert_eq!((p.spindle_min, p.spindle_max, p.spindle_power), (m.spindle_min, m.spindle_max, m.spindle_power), "{plus} spindle");
        assert_eq!(p.spindle_ranges, m.spindle_ranges, "{plus} belts");
        assert_eq!(p.rigid_tapping, m.rigid_tapping, "{plus} tapping");
        for ((a, b), c) in p.axes.iter().zip(&m.axes).zip(&mx.axes) {
            assert_eq!(a.travel, b.travel, "{plus} {} travel is the M's", a.name);
            assert_eq!(a.feed, c.feed, "{plus} {} speed is the MX's", a.name);
        }
    }
}

#[test]
fn every_tormach_machine_states_its_manuals_figures() {
    for s in &SPECS {
        let m = shipped(s.name);
        let n = s.name;
        assert_eq!(m.model, n);
        assert_eq!(
            m.units.as_deref(),
            Some("inch"),
            "{n} is programmed in inches"
        );
        assert_eq!(m.axes.len(), 3, "{n}: X, Y, Z");
        for (a, want) in m.axes.iter().zip(s.travel_in) {
            // Travel is program coordinates from the work zero: the full
            // stroke either way, so any zero on the table is reachable.
            let (lo, hi) = a.travel.unwrap_or_else(|| panic!("{n} {} travel", a.name));
            assert!(
                close(hi, want * IN) && close(lo, -want * IN),
                "{n} {}: {lo}..{hi}",
                a.name
            );
        }
        assert_eq!(
            (m.spindle_min, m.spindle_max),
            (Some(s.rpm.0), Some(s.rpm.1)),
            "{n} rpm"
        );
        assert_eq!(m.spindle_power, Some(s.kw), "{n} kW");
        for a in &m.axes {
            let want = if a.name == "Z" { s.ipm.1 } else { s.ipm.0 };
            assert!(
                a.feed.is_some_and(|f| close(f, want * IN)),
                "{n} {} feed {:?}",
                a.name,
                a.feed
            );
        }
        assert_eq!(m.spindle_ranges, s.ranges.to_vec(), "{n} belt ranges");
        assert_eq!(m.rigid_tapping, Some(s.rigid), "{n} rigid tapping");
        let codes: Vec<(&str, &str)> = m
            .coolants
            .iter()
            .map(|c| (c.name.as_str(), c.m_code.as_str()))
            .collect();
        assert_eq!(codes, [("Flood", "M8"), ("Mist", "M7")], "{n} coolant");
    }
}

#[test]
fn the_script_sees_the_ranges_and_the_tapping_word() {
    let p = machine::script_props(&mirrored("Tormach 1100MX"));
    assert_eq!(
        p["spindleRanges"],
        serde_json::json!([[70.0, 2000.0], [250.0, 10000.0]])
    );
    assert_eq!(p["rigidTapping"], true);
    let p = machine::script_props(&mirrored("Tormach PCNC 440"));
    assert_eq!(p["rigidTapping"], false);
    // Unstated: no ranges, and tapping is not limited.
    let p = machine::script_props(&mirrored("Tormach 1500MX"));
    assert_eq!(p["spindleRanges"], serde_json::json!([]));
    let p = machine::script_props(&serde_json::json!({}));
    assert_eq!(
        (&p["spindleRanges"], &p["rigidTapping"]),
        (&serde_json::json!([]), &serde_json::json!(true))
    );
}

fn post_at(name: &str, rpm: u32) -> engine::PostOutput {
    let cps = read("assets/posts/tormach.cps");
    let stream =
        format!("PARAM SpindleSpeed {rpm}\nTOOL 1 T1\nSECTION\nRapid X 0 Y 0 Z 5000\nENDSECTION\n");
    engine::run(&cps, &mirrored(name), &stream).unwrap_or_else(|e| panic!("{name}: {e}"))
}

/// The belt note reads the machine's own ranges: the 1100MX's low range
/// ends at 2000 rpm (the 770's at 3250), and a one-range machine has no
/// belt to name.
#[test]
fn the_belt_note_follows_the_machines_own_ranges() {
    let note = |name: &str, rpm: u32| {
        post_at(name, rpm)
            .program
            .lines()
            .find(|l| l.contains("BELT RANGE"))
            .map(str::to_owned)
    };
    assert_eq!(
        note("Tormach 1100MX", 2500).as_deref(),
        Some("(SPINDLE 2500 RPM NEEDS THE HIGH BELT RANGE)")
    );
    assert_eq!(
        note("Tormach 770MX", 2500),
        None,
        "both 770 ranges serve 2500"
    );
    assert_eq!(
        note("Tormach 1100MX", 100).as_deref(),
        Some("(SPINDLE 100 RPM NEEDS THE LOW BELT RANGE)")
    );
    assert_eq!(note("Tormach 1100MX", 1500), None);
    assert_eq!(
        note("Tormach PCNC 440", 500).as_deref(),
        Some("(SPINDLE 500 RPM NEEDS THE LOW BELT RANGE)")
    );
    assert_eq!(
        note("Tormach PCNC 440", 6000).as_deref(),
        Some("(SPINDLE 6000 RPM NEEDS THE HIGH BELT RANGE)")
    );
    // The Series I 1100's ranges are its own (300-1750 / 800-4500), not
    // the later 1100s' (100-2000 / 250-5140).
    assert_eq!(
        note("Tormach PCNC 1100 (Series I)", 2000).as_deref(),
        Some("(SPINDLE 2000 RPM NEEDS THE HIGH BELT RANGE)")
    );
    assert_eq!(note("Tormach PCNC 1100 (Series I)", 1000), None);
    assert_eq!(
        note("Tormach PCNC 1100 (Series 3)", 2000),
        None,
        "both Series 3 ranges serve 2000"
    );
    assert_eq!(
        note("Tormach PCNC 1100 (Series II)", 150).as_deref(),
        Some("(SPINDLE 150 RPM NEEDS THE LOW BELT RANGE)")
    );
    // The PCNC 770's high belt reaches 10200, past the 770M's 10000.
    assert_eq!(
        note("Tormach PCNC 770 (Series 3)", 10100).as_deref(),
        Some("(SPINDLE 10100 RPM NEEDS THE HIGH BELT RANGE)")
    );
    // The 1100M's high belt stops at 7500 rpm and its low one at 2000,
    // the 1100MX's low range but not its 10000 rpm top.
    assert_eq!(
        note("Tormach 1100M", 2500).as_deref(),
        Some("(SPINDLE 2500 RPM NEEDS THE HIGH BELT RANGE)")
    );
    assert_eq!(
        note("Tormach 1100M", 100).as_deref(),
        Some("(SPINDLE 100 RPM NEEDS THE LOW BELT RANGE)")
    );
    assert_eq!(
        note("Tormach 1100M", 1500),
        None,
        "both 1100M ranges serve 1500"
    );
    // The earlier PCNC 770 has the Series 3's belts.
    assert_eq!(
        note("Tormach PCNC 770", 10100).as_deref(),
        Some("(SPINDLE 10100 RPM NEEDS THE HIGH BELT RANGE)")
    );
    assert_eq!(
        note("Tormach PCNC 770", 200).as_deref(),
        Some("(SPINDLE 200 RPM NEEDS THE LOW BELT RANGE)")
    );
    assert_eq!(note("Tormach 1500MX", 8000), None, "one range: no belt");
    assert_eq!(note("Tormach 770MX (ML)", 300), None, "one range: no belt");
}

fn tap_on(name: &str) -> engine::PostOutput {
    let cps = read("assets/posts/tormach.cps");
    let stream =
        read("crates/app/tests/golden/tap_plate.cl").replace("DWELL0 AT1", "DWELL500 F1270000 AT1");
    engine::run(&cps, &mirrored(name), &stream).unwrap_or_else(|e| panic!("{name}: {e}"))
}

/// The R8 mills (770M, 1100M, PCNC 440, the Mach3-era PCNC 770 and 1100s) cannot
/// rigid-tap: a tap is refused by name, once, and no G84 reaches the
/// program. The MX mills post it.
#[test]
fn a_rigid_tap_is_refused_on_a_machine_that_cannot_rigid_tap() {
    for name in [
        "Tormach 770M",
        "Tormach 770M+",
        "Tormach 1100M+",
        "Tormach PCNC 440",
        "Tormach 1100M",
        "Tormach PCNC 770",
        "Tormach PCNC 770 (Series 3)",
        "Tormach PCNC 1100 (Series I)",
        "Tormach PCNC 1100 (Series II)",
        "Tormach PCNC 1100 (Series 3)",
    ] {
        let out = tap_on(name);
        assert_eq!(
            out.warnings.len(),
            1,
            "{name}: one named refusal: {:?}",
            out.warnings
        );
        assert!(
            out.warnings[0].contains("rigid tapping") && out.warnings[0].contains(name),
            "{:?}",
            out.warnings
        );
        assert!(!out.program.contains("G84"), "{name}:\n{}", out.program);
    }
    for name in [
        "Tormach 770MX",
        "Tormach 770MX (ML)",
        "Tormach 1100MX",
        "Tormach 1500MX",
    ] {
        let out = tap_on(name);
        assert!(out.warnings.is_empty(), "{name}: {:?}", out.warnings);
        assert!(
            out.program.lines().any(|l| l.starts_with("G84")),
            "{name}:\n{}",
            out.program
        );
    }
}

/// A Ø40 x 10 mm puck with a blind Ø6.2 hole 8 mm deep on its axis (a
/// 16-gon revolve, closed and outward-facing): room for a Ø6 tap without
/// the material sim calling the sweep a gouge.
fn tapped_puck() -> camcraft_app::geometry::Mesh {
    use camcraft_app::geometry::{Mesh, Point3, Triangle};
    let meridian = [(0.0, 0.0), (20.0, 0.0), (20.0, 10.0), (3.1, 10.0), (3.1, 2.0), (0.0, 2.0)];
    let v = |j: usize, r: f64, z: f64| {
        let th = std::f64::consts::TAU * j as f64 / 16.0;
        Point3::new(r * th.cos(), r * th.sin(), z)
    };
    let mut facets = Vec::new();
    for seg in meridian.windows(2) {
        let ((ra, za), (rb, zb)) = (seg[0], seg[1]);
        for j in 0..16 {
            let (a1, a2, b1, b2) = (v(j, ra, za), v(j + 1, ra, za), v(j, rb, zb), v(j + 1, rb, zb));
            if ra <= 1e-12 {
                facets.push(Triangle::new(a1, b2, b1));
            } else if rb <= 1e-12 {
                facets.push(Triangle::new(a1, a2, b2));
            } else {
                facets.push(Triangle::new(a1, a2, b2));
                facets.push(Triangle::new(a1, b2, b1));
            }
        }
    }
    Mesh::from_facets(facets)
}

/// The GUI's own path (`posting::post_setup`, what the Post button runs):
/// a tap job on the built-in 770M is refused with the rigid-tapping
/// reason and writes no file; the same job on the 1100MX posts its G84.
#[test]
fn a_tap_job_posts_on_the_1100mx_and_is_refused_on_the_770m() {
    use camcraft_app::commands::{
        AddTool, CreateOperation, GenerateOperation, History, ImportModel, LoadMachine, SelectPost,
        SetStock,
    };
    use camcraft_app::drilling::{CycleChoice, Hole};
    use camcraft_app::geo2d::Point2;
    use camcraft_app::operation::{Height, OpParams, SrcRef};
    use camcraft_app::schema::{Location, PostRef};
    use camcraft_app::stock::StockShape;
    use camcraft_app::tools::catalog;

    let job = |machine: &str| {
        let world = camcraft_app::world::new_world();
        let mut h = History::default();
        h.execute(&world, LoadMachine::builtin(&world, machine).expect("built-in machine"));
        h.execute(
            &world,
            SelectPost::new(&world, PostRef::Named("Tormach PathPilot".into())).expect("post"),
        );
        let stock = StockShape::Cylinder { diameter: 40.0, height: 10.0 };
        h.execute(&world, SetStock::new(&world, stock, Location::default()).expect("stock"));
        h.execute(&world, ImportModel::from_mesh("puck", tapped_puck()));
        let tap = catalog().into_iter().find(|(n, _)| *n == "TAP-M6").map(|(_, t)| t).expect("tap");
        h.execute(&world, AddTool::new(&world, "TAP-M6", tap).expect("add tap"));
        let drill = OpParams::Drill {
            clearance_z: Height::fixed(25.0),
            retract_z: None,
            top_z: Height::fixed(12.0),
            bottom_z: None,
            cycle: CycleChoice::Auto,
            peck: 0.0,
            dwell: 0.0,
            feed: 100.0,
            plunge_feed: 100.0,
            holes: vec![Hole {
                xy: Point2::new(0.0, 0.0),
                radius: 3.05,
                entry_z: 10.0,
                floor_z: 2.0,
                through: false,
                stepped: false,
            }],
            cross: Vec::new(),
        };
        let src = SrcRef::Entity("puck".into());
        h.execute(&world, CreateOperation::new(&world, "Tap1", "TAP-M6", src, drill).expect("tap op"));
        h.execute(&world, GenerateOperation::new(&world, "Tap1").expect("generate"));
        let path = std::env::temp_dir().join(format!(
            "camcraft_tormach_tap_{}_{}.nc",
            machine.replace(' ', "_"),
            std::process::id()
        ));
        let _ = std::fs::remove_file(&path);
        let posted = camcraft_app::posting::post_setup(&world, &path);
        let written = path.exists();
        let _ = std::fs::remove_file(&path);
        (posted, written)
    };

    let (posted, written) = job("Tormach 1100MX");
    let posted = posted.unwrap_or_else(|e| panic!("the 1100MX rigid-taps: {e}"));
    assert!(written, "the program is on disk");
    assert!(posted.program.lines().any(|l| l.starts_with("G84 ")), "{}", posted.program);

    let (refused, written) = job("Tormach 770M");
    let err = match refused {
        Ok(p) => panic!("the 770M must refuse a rigid tap:\n{}", p.program),
        Err(e) => e,
    };
    assert!(err.contains("rigid tapping") && err.contains("Tormach 770M"), "{err}");
    assert!(!written, "a refused post writes nothing");
}
