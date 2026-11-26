//! Router materials (#3068): softwood, hardwood, MDF, plywood, acrylic
//! and HDPE give a stated suggestion for an end mill, a V-bit and a ball
//! nose; the chip load follows the router curve by cutter diameter; a
//! router spindle's range clamps the rpm, the feed is recomputed at the
//! clamped rpm so the chip load holds, and each clamp is named.
//!
//! Gate map (M4.4): covers app::cutting, tools::cutting, tools::cutting_router.

use camcraft_app::cutting;
use camcraft_app::machine::Machine;
use camcraft_app::tools::{Tool, ToolMaterial, WorkMaterial, catalog};

const ROUTER: [WorkMaterial; 6] = [
    WorkMaterial::Softwood,
    WorkMaterial::Hardwood,
    WorkMaterial::Mdf,
    WorkMaterial::Plywood,
    WorkMaterial::Acrylic,
    WorkMaterial::Hdpe,
];

fn catalog_tool(name: &str) -> Tool {
    catalog().into_iter().find(|(n, _)| *n == name).map(|(_, t)| t).expect("catalog entry")
}

/// A hobby router spindle: 10000..24000 rpm.
fn router() -> Machine {
    Machine { model: "Router".into(), spindle_min: Some(10_000.0), spindle_max: Some(24_000.0), ..Default::default() }
}

#[test]
fn router_materials_have_words_and_round_trip() {
    let words: Vec<&str> = ROUTER.iter().map(|m| m.word()).collect();
    assert_eq!(words, ["softwood", "hardwood", "mdf", "plywood", "acrylic", "hdpe"]);
    for m in ROUTER {
        assert_eq!(WorkMaterial::from_word(m.word()), Some(m));
        assert!(WorkMaterial::ALL.contains(&m));
    }
}

#[test]
fn each_router_material_matches_the_hand_calculation_on_a_6mm_end_mill() {
    let t = catalog_tool("EM-0600-4FL");
    // (material, carbide Vc mid m/min, carbide fz10 mid mm). The router
    // curve reads 0.65 of the 10 mm chip load at a 6 mm cutter.
    let want = [
        (WorkMaterial::Softwood, 500.0, 0.30),
        (WorkMaterial::Hardwood, 425.0, 0.24),
        (WorkMaterial::Mdf, 500.0, 0.30),
        (WorkMaterial::Plywood, 425.0, 0.265),
        (WorkMaterial::Acrylic, 275.0, 0.18),
        (WorkMaterial::Hdpe, 350.0, 0.215),
    ];
    for (m, vc, fz10) in want {
        let s = cutting::compute("EM", &t, 2, ToolMaterial::Carbide, m, None, None);
        let fz = fz10 * 0.65;
        assert!((s.vc - vc).abs() < 1e-9, "{m:?}: {s:?}");
        assert!((s.fz - fz).abs() < 1e-9, "{m:?}: {s:?}");
        let n = (vc * 1000.0 / (std::f64::consts::PI * 6.0)).round();
        assert_eq!(s.rpm, n, "{m:?}");
        assert_eq!(s.feed, (fz * 2.0 * n).round(), "{m:?}");
        assert!(s.notes.is_empty(), "no machine, no clamp: {:?}", s.notes);
    }
}

#[test]
fn end_mill_v_bit_and_ball_nose_stay_inside_a_router_spindle() {
    let m = router();
    for name in ["EM-0300-2FL", "EM-0600-4FL", "EM-1200-2FL", "CM-0600-90", "BN-0600"] {
        let t = catalog_tool(name);
        for mat in ROUTER {
            for tm in ToolMaterial::ALL {
                let s = cutting::compute(name, &t, 2, tm, mat, None, Some(&m));
                assert!((10_000.0..=24_000.0).contains(&s.rpm), "{name} {mat:?} {tm:?}: {s:?}");
                assert!(s.feed > 0.0 && s.feed.is_finite(), "{name} {mat:?}: {s:?}");
                // The feed is the chip load at the rpm that is posted.
                assert_eq!(s.feed, (s.fz * 2.0 * s.rpm).round(), "{name} {mat:?} {tm:?}");
            }
        }
    }
}

#[test]
fn a_v_bit_reads_its_chip_load_at_half_its_diameter() {
    // A V-bit cuts mostly near its tip: the chip load is the one of a
    // cutter half its diameter. CM-0600-90: 6 mm -> read at 3 mm,
    // router curve 0.30; softwood carbide fz10 mid 0.30 -> 0.09.
    let t = catalog_tool("CM-0600-90");
    let s = cutting::compute("CM", &t, 2, ToolMaterial::Carbide, WorkMaterial::Softwood, None, None);
    assert!((s.fz - 0.09).abs() < 1e-9, "{s:?}");
    // The metal rows keep their reading at the full diameter.
    let s = cutting::compute("CM", &t, 2, ToolMaterial::Carbide, WorkMaterial::Aluminium6061, None, None);
    assert!((s.fz - 0.0375).abs() < 1e-12, "{s:?}");
}

#[test]
fn a_router_spindle_clamps_both_ways_and_keeps_the_chip_load() {
    let m = router();
    // 3 mm carbide in softwood: n = 500000/(pi*3) = 53052 rpm -> held at
    // 24000. fz = 0.30 * 0.30 = 0.09; feed = 0.09*2*24000 = 4320.
    let t = catalog_tool("EM-0300-2FL");
    let s = cutting::compute("EM", &t, 2, ToolMaterial::Carbide, WorkMaterial::Softwood, None, Some(&m));
    assert!((s.fz - 0.09).abs() < 1e-9, "{s:?}");
    assert_eq!((s.rpm, s.feed), (24_000.0, 4_320.0));
    assert_eq!(s.notes, ["53052 rpm held to the machine's 24000"]);
    // 12 mm HSS in acrylic: n = 140000/(pi*12) = 3714 rpm -> raised to
    // 10000. fz = 0.125 * 1.15 = 0.14375; feed = 0.14375*2*10000 = 2875.
    let t = catalog_tool("EM-1200-2FL");
    let s = cutting::compute("EM", &t, 2, ToolMaterial::Hss, WorkMaterial::Acrylic, None, Some(&m));
    assert!((s.fz - 0.14375).abs() < 1e-9, "{s:?}");
    assert_eq!((s.rpm, s.feed), (10_000.0, 2_875.0));
    assert_eq!(s.notes, ["3714 rpm raised to the machine's minimum 10000"]);
}
