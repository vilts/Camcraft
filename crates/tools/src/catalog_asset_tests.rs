//! Pins for the shipped starter catalog asset (#741): it parses with every
//! tool valid, keeps the display order the Rust table had (checked tool for
//! tool against that table before it was deleted), and a bad entry is a
//! named error, never a panic.

use crate::catalog_asset::{CATALOG_JSON, parse_catalog};
use crate::tools::{Tool, ToolKind, ToolMaterial, catalog, probe_tool, torch_tool};

const ORDER: [&str; 63] = [
    "EM-0200-2FL", "EM-0300-2FL", "EM-0400-2FL", "EM-0600-4FL", "EM-0800-4FL", "EM-1000-4FL", "EM-1200-2FL",
    // #3036: BN-0300, BB-0600-R1 and the metric tap-drill and clearance drills sit in their families.
    "BN-0300", "BN-0600", "BN-1000", "BB-0600-R1", "BB-1000-R1",
    "DR-0200", "DR-0250", "DR-0300", "DR-0330", "DR-0340", "DR-0400", "DR-0420", "DR-0450", "DR-0500", "DR-0550",
    "DR-0600", "DR-0660", "DR-0680", "DR-0850", "DR-0900", "DR-1020", "DR-1100", "DR-1350", "CM-0400-90", "CM-0600-90",
    "CM-1000-90",
    // #3368: a flat-tip chamfer mill, a tapered flat end mill and a tapered ball nose.
    "CM-0600-90-F10", "TE-0250-03-F10", "TB-0160-01-R05",
    "TAP-M4", "TAP-M6", "BR-1000", "TN-1600-0R8", "GT-0200", "GT-0300", "TD-0866-60", "TD-1732-60",
    "FG-0600-04", "FG-1200-08", "PRB-0400", "PT-KERF-150", "EDM-E10X6", "TM-0600-P125", "TMS-0600-P050-150", "EDM-W025",
    // #1444: the undercut cutters.
    "TS-2000-0600", "KS-1600-0300", "DT-2000-60", "LP-1000",
    // #1883: the waterjet nozzle.
    "WJ-KERF-080",
    // #3369: roundover, cove and V-groove form mills, in mm and inch.
    "FM-RO-R6", "FM-RO-R0250IN", "FM-CV-R6", "FM-CV-R0250IN", "FM-VG-90-12", "FM-VG-90-0500IN",
];

fn tool(name: &str) -> Tool {
    catalog().into_iter().find(|(n, _)| *n == name).map(|(_, t)| t).expect("catalog tool")
}

#[test]
fn the_shipped_asset_parses_and_every_tool_is_valid() {
    let v = parse_catalog(CATALOG_JSON).expect("shipped asset parses");
    let legacy = v.iter().filter(|(n, _)| !n.starts_with("Router ")).map(|(n, _)| n.as_str()).collect::<Vec<_>>();
    assert_eq!(legacy, ORDER);
    for (n, t) in &v {
        assert!(t.is_sane(), "{n}");
    }
    assert_eq!(catalog().len(), ORDER.len() + 22);
}

/// #3069: generic router sizes, cutting shapes and stated reach.
#[test]
fn router_starter_tools_state_their_shapes_and_reach() {
    let cat = catalog();
    let router: Vec<_> = cat.iter().filter(|(n, _)| n.starts_with("Router ")).collect();
    assert_eq!(router.len(), 22, "the complete router starter set ships");
    for (size, diameter, flute, reach) in [
        ("3 mm", 3.0, 12.0, 25.0), ("6 mm", 6.0, 22.0, 38.0),
        ("1/8 in", 3.175, 12.0, 25.0), ("1/4 in", 6.35, 22.0, 38.0),
    ] {
        for cut in ["up-cut", "down-cut", "compression", "ball nose"] {
            let name = format!("Router {cut} {size}");
            let t = &cat.iter().find(|(n, _)| *n == name).expect("router tool").1;
            let ball = cut == "ball nose";
            assert_eq!(t.kind, if ball { ToolKind::BallNose } else { ToolKind::EndMill }, "{name}");
            assert_eq!((t.diameter, t.corner_r), (diameter, if ball { diameter / 2.0 } else { 0.0 }), "{name}");
            assert_eq!((t.flute_len, t.flute_len + t.shank_len, t.shank_d), (flute, reach, diameter), "{name}");
            assert!(t.center_cutting, "{name}");
        }
    }
    for (size, diameter) in [("6 mm", 6.0), ("1/4 in", 6.35)] {
        for angle in [60, 90] {
            let name = format!("Router V-bit {angle} deg {size}");
            let t = &cat.iter().find(|(n, _)| *n == name).expect("V-bit").1;
            assert_eq!(t.kind, ToolKind::Taper, "{name}");
            assert_eq!((t.diameter, t.tip_angle, t.corner_r), (diameter, f64::from(angle), 0.0), "{name}");
            assert_eq!((t.flute_len, t.flute_len + t.shank_len, t.shank_d), (10.0, 25.0, diameter), "{name}");
            assert!(t.center_cutting, "{name}");
        }
    }
    for (size, diameter, shank) in [("25 mm", 25.0, 6.0), ("1 in", 25.4, 6.35)] {
        let name = format!("Router surfacing {size}");
        let t = &cat.iter().find(|(n, _)| *n == name).expect("surfacing bit").1;
        assert_eq!(t.kind, ToolKind::EndMill, "{name}");
        assert_eq!((t.diameter, t.corner_r, t.shank_d), (diameter, 0.0, shank), "{name}");
        assert_eq!((t.flute_len, t.flute_len + t.shank_len), (8.0, 25.0), "{name}");
        assert!(!t.center_cutting, "{name} cannot plunge");
    }
    for (name, t) in router {
        assert_eq!((t.tool_material, t.flutes), (Some(ToolMaterial::Carbide), Some(2)), "{name}");
        assert_eq!((t.holder_d, t.holder_len), (32.0, 25.0), "{name}");
        assert_eq!(t.material_group, "wood", "{name}");
        assert_eq!((t.rpm, t.feed), (if name.contains("surfacing") { 12000.0 } else { 18000.0 }, 600.0), "{name}");
        assert!(t.cutting.is_empty(), "material-specific data waits for #3068");
        assert!(t.is_sane(), "{name}");
    }
}

#[test]
fn the_json_catalog_keeps_the_old_tables_values() {
    // Cutting data: mills state carbide and their flute count, the rest nothing.
    let em = tool("EM-0600-4FL");
    assert_eq!((em.kind, em.diameter, em.flute_len, em.shank_d, em.rpm, em.feed), (ToolKind::EndMill, 6.0, 15.0, 8.0, 9500.0, 1800.0));
    assert_eq!((em.flutes, em.tool_material), (Some(4), Some(ToolMaterial::Carbide)));
    assert_eq!((em.shank_len, em.holder_d, em.holder_len, em.material_group.as_str()), (40.0, 44.0, 35.0, "aluminum"));
    assert_eq!(tool("BN-1000").corner_r, 5.0);
    assert_eq!(tool("BB-1000-R1").flutes, Some(2));
    assert_eq!(tool("CM-0400-90").flutes, Some(1));
    let dr = tool("DR-0400");
    assert_eq!((dr.tip_angle, dr.flutes, dr.tool_material), (118.0, None, None));
    assert_eq!(tool("TAP-M6").pitch, 1.0);
    let br = tool("BR-1000");
    assert_eq!((br.nose_radius, br.holder_d, br.holder_len), (0.4, 0.0, 0.0));
    assert_eq!(tool("TN-1600-0R8").nose_radius, 0.8);
    assert_eq!(tool("GT-0300").width, 3.0);
    let td = tool("TD-1732-60");
    assert_eq!((td.width, td.tip_angle, td.shank_len, td.holder_len), (1.732, 60.0, 60.0, 0.0));
    let fg = tool("FG-1200-08");
    assert_eq!((fg.nose_radius, fg.width, fg.feed), (12.0, 8.0, 50.0));
    // The probe row is exactly the constructor's tool.
    assert_eq!(format!("{:?}", tool("PRB-0400")), format!("{:?}", probe_tool(4.0, 30.0)));
    // #670 (merged upstream while this PR was open): the torch row is torch_tool(1.5).
    assert_eq!(format!("{:?}", tool("PT-KERF-150")), format!("{:?}", torch_tool(1.5)));
    // #541: the electrode row is electrode_tool(10, 6, 30, 15).
    assert_eq!(format!("{:?}", tool("EDM-E10X6")), format!("{:?}", crate::electrode::electrode_tool(10.0, 6.0, 30.0, 15.0)));
    // #2011: the wire row is wire_tool(0.25).
    assert_eq!(format!("{:?}", tool("EDM-W025")), format!("{:?}", crate::wire::wire_tool(0.25)));
    // #1883: the nozzle row is waterjet_tool(0.8), the kerf the golden plate cuts with.
    assert_eq!(format!("{:?}", tool("WJ-KERF-080")), format!("{:?}", crate::waterjet::waterjet_tool(0.8)));
}

/// #3036: the small ball, the small bull nose, and a tap drill and a
/// clearance drill (ISO 273 medium) for each coarse thread M3-M12.
#[test]
fn the_catalog_carries_the_metric_hole_drills_and_small_ball_tools() {
    let bn = tool("BN-0300");
    assert_eq!((bn.kind, bn.diameter, bn.corner_r, bn.flute_len, bn.shank_d), (ToolKind::BallNose, 3.0, 1.5, 9.0, 4.0));
    assert_eq!((bn.flutes, bn.tool_material, bn.rpm, bn.feed), (Some(2), Some(ToolMaterial::Carbide), 15000.0, 750.0));
    let bb = tool("BB-0600-R1");
    assert_eq!((bb.kind, bb.diameter, bb.corner_r, bb.flute_len, bb.shank_d), (ToolKind::BullNose, 6.0, 1.0, 15.0, 8.0));
    assert_eq!((bb.flutes, bb.tool_material, bb.rpm, bb.feed), (Some(2), Some(ToolMaterial::Carbide), 9500.0, 1500.0));
    // (thread, tap drill, clearance drill), in mm.
    for (thread, tap, clear) in
        [("M3", 2.5, 3.4), ("M4", 3.3, 4.5), ("M5", 4.2, 5.5), ("M6", 5.0, 6.6), ("M8", 6.8, 9.0), ("M10", 8.5, 11.0), ("M12", 10.2, 13.5)]
    {
        for d in [tap, clear] {
            let name = format!("DR-{:04.0}", d * 100.0);
            let t = catalog().into_iter().find(|(n, _)| *n == name).map(|(_, t)| t);
            let Some(t) = t else { panic!("{thread}: no catalog drill {name}") };
            assert_eq!((t.kind, t.diameter, t.tip_angle, t.shank_d), (ToolKind::Drill, d, 118.0, d), "{name}");
            assert_eq!((t.flutes, t.tool_material, t.material_group.as_str()), (None, None, "aluminum"), "{name}");
            // 3 x D on a 0.5 mm step: an exact binary length, so the spun holder
            // stacks bit-identically (spun_tests).
            assert_eq!(t.flute_len, (6.0 * d).round() / 2.0, "{name}: flute length is 3 x D on a 0.5 mm step");
            assert!(t.rpm > 0.0 && t.feed > 0.0 && t.is_sane(), "{name}");
        }
    }
    // A bigger drill never turns faster than a smaller one.
    let drills: Vec<_> = catalog().into_iter().filter(|(n, _)| n.starts_with("DR-")).map(|(_, t)| (t.diameter, t.rpm)).collect();
    assert!(drills.windows(2).all(|w| w[0].0 < w[1].0 && w[0].1 >= w[1].1), "{drills:?}");
}

#[test]
fn a_bad_catalog_is_a_named_error_not_a_panic() {
    let row = |name: &str, def: &str| format!(r#"{{"name":"{name}","definition":{def}}}"#);
    let good = r#"{"kind":"end_mill","diameter":6,"corner_r":0,"flute_len":15,"shank_d":8,"shank_len":40,"holder_d":44,"holder_len":35,"material_group":"aluminum","rpm":9000,"feed":1500}"#;
    let file = |hdr: &str, rows: &[String]| format!(r#"{{{hdr},"tools":[{}]}}"#, rows.join(","));
    let ok = r#""format":"camcraft-tools","version":1"#;
    assert!(parse_catalog(&file(ok, &[row("A", good)])).is_ok());
    let err = |t: String| parse_catalog(&t).expect_err("must refuse");
    assert!(err("not json".into()).contains("bad JSON"));
    assert!(err(file(r#""format":"other","version":1"#, &[row("A", good)])).contains("format"));
    assert!(err(file(r#""format":"camcraft-tools","version":2"#, &[row("A", good)])).contains("version 2"));
    assert!(err(file(r#""format":"camcraft-tools","version":1,"units":"in""#, &[row("A", good)])).contains("units"));
    assert!(err(file(ok, &[])).contains("no tools"));
    assert!(err(file(ok, &[row(" ", good)])).contains("no name"));
    assert!(err(file(ok, &[row("A", good), row("A", good)])).contains("'A' appears twice"));
    assert!(err(file(ok, &[row("A", r#"{"kind":"end_mill"}"#)])).contains("'A'"));
    assert!(err(file(ok, &[row("A", &good.replace("end_mill", "laser"))])).contains("'A'"));
    assert!(err(file(ok, &[row("A", &good.replace("\"rpm\":9000", "\"rpm\":0"))])).contains("'A' is refused"));
    assert!(err(file(ok, &[row("A", &good.replace("\"feed\"", "\"typo\":1,\"feed\""))])).contains("'A'"));
}

/// #3662: a single-form thread mill row with no `pitch_min` reads as a
/// range of its one pitch, the same as the library and project readers.
#[test]
fn a_catalog_single_form_thread_mill_without_a_range_cuts_its_one_pitch() {
    let def = r#"{"kind":"thread_mill_single","diameter":6,"corner_r":0,"tip_angle":60,"pitch":1.5,"flute_len":2,"shank_d":6,"shank_len":40,"holder_d":44,"holder_len":35,"material_group":"aluminum","rpm":6000,"feed":300}"#;
    let text = format!(r#"{{"format":"camcraft-tools","version":1,"tools":[{{"name":"TMS","definition":{def}}}]}}"#);
    let v = parse_catalog(&text).expect("a single-form row without a range parses");
    assert_eq!(v[0].1.kind, ToolKind::ThreadMillSingle);
    assert_eq!((v[0].1.pitch_min, v[0].1.pitch), (1.5, 1.5));
}

/// #1152: a library row states a body electrode's outline; the reader
/// keeps it, and refuses by name a row whose sizes leave its outline.
#[test]
fn a_catalog_electrode_keeps_its_outline() {
    let def = |x: f64| format!(
        r#"{{"kind":"electrode","diameter":{x},"corner_r":0,"width":8,"flute_len":16,"shank_d":10,"shank_len":15,"holder_d":0,"holder_len":0,"material_group":"graphite","rpm":0,"feed":50,"outline":[[-5,-4],[5,-4],[5,0],[-1,0],[-1,4],[-5,4]]}}"#
    );
    let file = |d: String| format!(r#"{{"format":"camcraft-tools","version":1,"tools":[{{"name":"E-L","definition":{d}}}]}}"#);
    let v = parse_catalog(&file(def(10.0))).expect("an outline electrode parses");
    let ell = vec![[-5.0, -4.0], [5.0, -4.0], [5.0, 0.0], [-1.0, 0.0], [-1.0, 4.0], [-5.0, 4.0]];
    assert_eq!(v[0].1, crate::electrode::electrode_outline_tool(&ell, 16.0, 15.0));
    assert!(parse_catalog(&file(def(12.0))).expect_err("sizes off the outline").contains("'E-L' is refused"));
}

/// #3369: the catalogue form mills write their outline from the preset,
/// cut at the catalogue's diameter, and keep the pilot out of the cut.
/// The inch bits keep their true inch numbers: a 3/16 in pilot (4.7625
/// mm) under a 1/2 in cutting height, so a 11/16 in flute length (17.4625
/// mm). A sharp V-groove whose height is its cone has two points: the tip
/// on the axis and the top of the cone.
#[test]
fn the_catalog_form_mills_cut_at_their_catalogue_diameter() {
    const IN: f64 = 25.4;
    // (name, diameter, pilot length, flute length, outline points)
    for (name, d, pilot, flute, n) in [
        ("FM-RO-R6", 25.0, 5.0, 18.0, 5),
        ("FM-RO-R0250IN", 1.0 * IN, 0.1875 * IN, 0.6875 * IN, 5),
        ("FM-CV-R6", 25.0, 5.0, 18.0, 5),
        ("FM-CV-R0250IN", 1.0 * IN, 0.1875 * IN, 0.6875 * IN, 5),
        ("FM-VG-90-12", 12.0, 0.0, 6.0, 2),
        ("FM-VG-90-0500IN", 0.5 * IN, 0.0, 0.25 * IN, 2),
    ] {
        let t = tool(name);
        assert_eq!(t.kind, ToolKind::FormMill, "{name}");
        assert_eq!(t.why_not_sane(), None, "{name}");
        let form = t.form.clone().unwrap_or_default();
        assert!(form.preset.is_some(), "{name}: the catalogue states a preset");
        assert_eq!(form.points.len(), n, "{name}: the preset wrote the points");
        // The catalogue states the exact inch value: 1e-9 mm is float dust.
        assert!((form.pilot_len - pilot).abs() < 1e-9, "{name}: pilot {} mm", form.pilot_len);
        assert!((t.flute_len - flute).abs() < 1e-9, "{name}: flute length {} mm", t.flute_len);
        let m = t.meridian().unwrap_or_else(|e| panic!("{name}: {e}"));
        let widest = m.points().iter().fold(0.0_f64, |r, p| r.max(p.r));
        assert!((2.0 * widest - d).abs() < 1e-9, "{name}: cuts at {} mm", 2.0 * widest);
        assert_eq!(m.cutting().contains(&false), pilot > 0.0, "{name}: only a pilot does not cut");
    }
}
