//! #2566: pure unit/integration tests for Tormach PathPilot (.csv) and
//! Hurco WinMax (.TCM) tool tables in `camcraft_tools`.
//!
//! Provenance: our own tests over the documented PathPilot CSV and
//! Hurco WinMax .TCM formats.

use camcraft_geometry::geometry::units::Unit;
use camcraft_tools::cutting::ToolMaterial;
use camcraft_tools::tool_table::{
    ToolTableExportRow, ToolTableFormat, detect_format, export_controller_table,
    parse_controller_table, parse_unit,
};
use camcraft_tools::tools::{Tool, ToolKind};
use std::path::Path;

const PATHPILOT_SAMPLE: &str = "\
Mill Tool Table,2026-11-18 19:09:05 - v1,This must be the first row - do not edit\r\n\
Tool number,Description,Z offset,Diameter,Z offset wear,Diameter wear,Max RPM\r\n\
1,endmill [1/8] 2FL R:0.000 loc:0.375 carbide dia:0.125 [37368],1.0000000,0.1250000,0.0000000,0.0000000,0.0\r\n\
2,,0.0000000,0.0000000,0.0000000,0.0000000,0.0\r\n\
10,drill dia:0.500,2.0000000,0.5000000,0.0000000,0.0000000,0.0\r\n\
99,,3.0000000,0.0000000,0.0000000,0.0000000,-2.0\r\n";

#[test]
fn detects_formats_by_extension_and_content() {
    assert_eq!(detect_format(Path::new("tools.csv"), None), Some(ToolTableFormat::PathPilot));
    assert_eq!(detect_format(Path::new("atc_mtc.TCM"), None), Some(ToolTableFormat::Hurco));
    assert_eq!(detect_format(Path::new("shop.camtools"), None), Some(ToolTableFormat::CamTools));
    assert_eq!(
        detect_format(Path::new("unknown.txt"), Some(PATHPILOT_SAMPLE)),
        Some(ToolTableFormat::PathPilot)
    );
    assert_eq!(
        detect_format(Path::new("unknown.xml"), Some("<Hurco_Document Version=\"1.1\"/>")),
        Some(ToolTableFormat::Hurco)
    );
    assert_eq!(parse_unit("inch"), Ok(Unit::Inches));
    assert_eq!(parse_unit("mm"), Ok(Unit::Millimeters));
    assert!(parse_unit("furlongs").is_err());
}

#[test]
fn pathpilot_parses_sample_and_ignores_placeholder_z_offset_and_negative_probe_marker() {
    let entries = parse_controller_table(ToolTableFormat::PathPilot, PATHPILOT_SAMPLE, Unit::Inches)
        .expect("sample parses");
    assert_eq!(entries.len(), 2);

    assert_eq!(entries[0].number, 1);
    assert_eq!(entries[0].name, "endmill [1/8] 2FL R:0.000 loc:0.375 carbide dia:0.125 [37368]");
    assert_eq!(entries[0].tool.kind, ToolKind::EndMill);
    assert!((entries[0].tool.diameter - 3.175).abs() < 1e-9);
    assert!((entries[0].tool.flute_len - 0.375 * 25.4).abs() < 1e-9);
    assert_eq!(entries[0].tool.flutes, Some(2));
    assert_eq!(entries[0].tool.tool_material, Some(ToolMaterial::Carbide));
    assert_eq!(entries[0].tool.catalog.part_number.as_deref(), Some("37368"));
    // Z offset 1.0 in must not become stick-out (flute_len stays 0.375 in).
    assert!((entries[0].tool.flute_len - 25.4).abs() > 1.0);

    assert_eq!(entries[1].number, 10);
    assert_eq!(entries[1].tool.kind, ToolKind::Drill);
    assert!((entries[1].tool.diameter - 12.7).abs() < 1e-9);

    // A configured probe with a non-empty description and -2.0 Max RPM parses cleanly.
    let probe_csv = "\
Mill Tool Table,2026-11-18 - v1,This must be the first row - do not edit\n\
Tool number,Description,Z offset,Diameter,Z offset wear,Diameter wear,Max RPM\n\
99,Passive Probe,3.0000000,0.1574803,0.0,0.0,-2.0\n";
    let probe = parse_controller_table(ToolTableFormat::PathPilot, probe_csv, Unit::Inches)
        .expect("probe parses");
    assert_eq!(probe[0].tool.kind, ToolKind::Probe);
    assert!(probe[0].tool.rpm > 0.0, "-2.0 marker must not become negative rpm");
}

#[test]
fn pathpilot_refuses_bad_headers_bad_rows_and_non_mill_export() {
    let missing_marker = "Tool number,Description,Z offset,Diameter\n1,EM,0,0.25\n";
    assert!(parse_controller_table(ToolTableFormat::PathPilot, missing_marker, Unit::Inches).is_err());

    let bad_dia = "\
Mill Tool Table,x - v1,This must be the first row - do not edit\n\
Tool number,Description,Z offset,Diameter,Z offset wear,Diameter wear,Max RPM\n\
4,Bad Cutter,0,zero,0,0,0\n";
    let err = parse_controller_table(ToolTableFormat::PathPilot, bad_dia, Unit::Inches).unwrap_err();
    assert!(err.contains("Bad Cutter") && err.contains("T4"), "{err}");

    let zero_dia_named = "\
Mill Tool Table,x - v1,This must be the first row - do not edit\n\
Tool number,Description,Z offset,Diameter,Z offset wear,Diameter wear,Max RPM\n\
4,Zero Cutter,0,0.0,0,0,0\n";
    let err = parse_controller_table(ToolTableFormat::PathPilot, zero_dia_named, Unit::Inches).unwrap_err();
    assert!(err.contains("Zero Cutter"), "{err}");

    let mut lathe = camcraft_tools::tools::catalog().into_iter().find(|(_, t)| t.kind == ToolKind::Turning).unwrap().1;
    lathe.diameter = 12.0;
    let err = export_controller_table(
        ToolTableFormat::PathPilot,
        &[ToolTableExportRow { number: 1, name: "Turn1", tool: &lathe, gauge_len: None }],
        Unit::Inches,
    )
    .unwrap_err();
    assert!(err.contains("Turn1"), "{err}");
}

#[test]
fn hurco_maps_supported_types_and_refuses_unsupported_by_name() {
    let xml = "\u{FEFF}<?xml version=\"1.0\" encoding=\"utf-8\"?>\r\n\
<Hurco_Document Version=\"1.1\">\r\n\
  <Tools>\r\n\
    <Tool Id=\"a\"><Name>Bull</Name><Number>1</Number><Diameter>12</Diameter><TipRadius>1.5</TipRadius><AbsoluteGaugeLength>90</AbsoluteGaugeLength><ToolType>0</ToolType></Tool>\r\n\
    <Tool Id=\"b\"><Name>Chamfer</Name><Number>2</Number><Diameter>10</Diameter><Angle>90</Angle><ToolType>2</ToolType></Tool>\r\n\
    <Tool Id=\"c\"><Name>Engrave</Name><Number>3</Number><Diameter>6</Diameter><Angle>60</Angle><ToolType>7</ToolType></Tool>\r\n\
    <Tool Id=\"d\"><Name>Drill</Name><Number>4</Number><Diameter>8.5</Diameter><ToolType>11</ToolType></Tool>\r\n\
    <Tool Id=\"e\"><Name>Tap</Name><Number>5</Number><Diameter>6</Diameter><ThreadPitch>1.0</ThreadPitch><ToolType>12</ToolType></Tool>\r\n\
    <Tool Id=\"f\"><Name>Bore</Name><Number>6</Number><Diameter>16</Diameter><ToolType>13</ToolType></Tool>\r\n\
    <Tool Id=\"g\"><Name>Face</Name><Number>7</Number><Diameter>50</Diameter><ToolType>15</ToolType></Tool>\r\n\
    <Tool Id=\"h\"><Name>TMMC</Name><Number>8</Number><Diameter>8</Diameter><FluteLength>16</FluteLength><ThreadPitch>1.25</ThreadPitch><ToolType>22</ToolType></Tool>\r\n\
    <Tool Id=\"i\"><Name>TMSC</Name><Number>9</Number><Diameter>8</Diameter><ThreadPitch>1.25</ThreadPitch><ToolType>26</ToolType></Tool>\r\n\
  </Tools>\r\n\
</Hurco_Document>\r\n";

    let entries = parse_controller_table(ToolTableFormat::Hurco, xml, Unit::Millimeters)
        .expect("supported Hurco types");
    assert_eq!(entries.len(), 9);
    let kinds: Vec<ToolKind> = entries.iter().map(|e| e.tool.kind).collect();
    assert_eq!(
        kinds,
        vec![
            ToolKind::BullNose,
            ToolKind::Taper,
            ToolKind::Taper,
            ToolKind::Drill,
            ToolKind::Tap,
            ToolKind::Bore,
            ToolKind::EndMill,
            ToolKind::ThreadMill,
            ToolKind::ThreadMillSingle,
        ]
    );
    assert!(!entries[6].tool.center_cutting, "face mill is not center cutting");
    assert_eq!(entries[0].tool.holder_len, 90.0);

    // Every Hurco type without a CamCraft kind must refuse by name, never guess.
    for (code, label) in [(1, "Center Drill"), (3, "Corner Round"), (6, "Dove Tail"), (8, "Keyseat"), (9, "Ream"), (17, "Back Spotface")] {
        let bad = format!(
            "<Hurco_Document Version=\"1.1\"><Tools><Tool Id=\"x\"><Name>Tool{code}</Name><Number>{code}</Number><Diameter>10</Diameter><ToolType>{code}</ToolType></Tool></Tools></Hurco_Document>"
        );
        let err = parse_controller_table(ToolTableFormat::Hurco, &bad, Unit::Millimeters).unwrap_err();
        assert!(err.contains(&format!("Tool{code}")) && err.contains(label), "{code}: {err}");
    }
}

#[test]
fn hurco_export_roundtrips_and_escapes_xml_without_control_state() {
    let mut tool = Tool {
        kind: ToolKind::EndMill,
        diameter: 12.0,
        corner_r: 0.0,
        tip_angle: 0.0,
        pitch: 0.0,
        pitch_min: 0.0,
        thread_hand: Default::default(),
        nose_radius: 0.0,
        width: 0.0,
        flute_len: 25.0,
        shank_d: 12.0,
        shank_len: 20.0,
        holder_d: 24.0,
        holder_len: 105.5,
        material_group: "aluminum".into(),
        rpm: 5000.0,
        feed: 1200.0,
        tool_material: None,
        flutes: Some(4),
        center_cutting: true,
        orientation: None,
        catalog: Default::default(),
        cutting: vec![],
        outline: vec![],
        turning: None,
        tip_form: Default::default(),
        tip_diameter: 0.0,
        form: None,
    };
    tool.catalog.vendor = Some("A & B <Carbide>".into());
    let text = export_controller_table(
        ToolTableFormat::Hurco,
        &[ToolTableExportRow { number: 12, name: "1/2\" EM & More", tool: &tool, gauge_len: Some(110.25) }],
        Unit::Millimeters,
    )
    .expect("export");
    assert!(text.contains("&amp;") && text.contains("&quot;"));
    assert!(text.contains("<AbsoluteGaugeLength>110.25</AbsoluteGaugeLength>"));
    assert!(!text.contains("UsedTime") && !text.contains("<Flags>"));

    let back = parse_controller_table(ToolTableFormat::Hurco, &text, Unit::Millimeters).expect("re-parse");
    assert_eq!(back[0].number, 12);
    assert_eq!(back[0].name, "1/2\" EM & More");
    assert_eq!(back[0].tool.holder_d, 24.0);
    assert_eq!(back[0].tool.holder_len, 110.25);
    assert_eq!(back[0].tool.catalog.vendor.as_deref(), Some("A & B <Carbide>"));
}

#[test]
fn pathpilot_refuses_recorded_slitting_saw_by_name() {
    let csv = "Mill Tool Table,x - v1,This must be the first row - do not edit\n\
Tool number,Description,Z offset,Diameter,Z offset wear,Diameter wear,Max RPM\n\
8,Slitting Saw [1/8] 30FL R:0.000 uncoated carbide dia:3.000 [34993],5.0000000,3.0000000,0.0000000,0.0000000,0.0\n";
    let err = parse_controller_table(ToolTableFormat::PathPilot, csv, Unit::Inches).unwrap_err();
    assert!(err.contains("Slitting Saw [1/8]") && err.contains("unsupported slitting saw"), "{err}");
}

#[test]
fn hurco_malformed_multibyte_entity_returns_error() {
    let entity = format!("&{}", "é".repeat(20));
    for xml in [
        format!("<Hurco_Document><Tools>{entity}</Tools></Hurco_Document>"),
        format!("<Hurco_Document name=\"{entity}\"><Tools/></Hurco_Document>"),
    ] {
        let err = parse_controller_table(ToolTableFormat::Hurco, &xml, Unit::Millimeters).unwrap_err();
        assert!(err.contains("unterminated XML entity"), "{err}");
        assert!(err.contains('é'), "{err}");
    }
}

#[test]
fn hurco_xml_nesting_limit_returns_error_before_recursion_exhaustion() {
    // The boundary is exercised without a process-aborting stack-overflow mutation.
    for depth in [32, 20_000] {
        let xml = format!("<Hurco_Document>{}{}</Hurco_Document>", "<x>".repeat(depth), "</x>".repeat(depth));
        let err = parse_controller_table(ToolTableFormat::Hurco, &xml, Unit::Millimeters).unwrap_err();
        assert!(err.contains("XML nesting exceeds the limit of 32"), "{depth}: {err}");
    }
    let at_limit = format!("<Hurco_Document>{}{}</Hurco_Document>", "<x>".repeat(31), "</x>".repeat(31));
    let err = parse_controller_table(ToolTableFormat::Hurco, &at_limit, Unit::Millimeters).unwrap_err();
    assert!(err.contains("missing <Tools>"), "allowed depth must reach the table schema: {err}");
}

#[test]
fn pathpilot_neutral_name_roundtrips_or_refuses_changed_cutter_geometry() {
    let source = "Mill Tool Table,x - v1,This must be the first row - do not edit\n\
Tool number,Description,Z offset,Diameter,Z offset wear,Diameter wear,Max RPM\n\
1,Finisher,0,6,0,0,0\n";
    let mut tool = parse_controller_table(ToolTableFormat::PathPilot, source, Unit::Millimeters)
        .expect("neutral-name end mill").remove(0).tool;
    let export = |tool: &Tool| export_controller_table(
        ToolTableFormat::PathPilot,
        &[ToolTableExportRow { number: 1, name: "Finisher", tool, gauge_len: None }],
        Unit::Millimeters,
    );
    let text = export(&tool).expect("matching end mill geometry");
    let back = parse_controller_table(ToolTableFormat::PathPilot, &text, Unit::Millimeters).unwrap();
    assert_eq!(back[0].name, "Finisher");
    assert_eq!(back[0].tool.kind, tool.kind);
    assert_eq!(back[0].tool.diameter, tool.diameter);
    assert_eq!(back[0].tool.flute_len, tool.flute_len);
    for (kind, corner_r, tip_angle) in [
        (ToolKind::BallNose, 3.0, 0.0),
        (ToolKind::BullNose, 1.0, 0.0),
        (ToolKind::Taper, 0.0, 60.0),
    ] {
        tool.kind = kind;
        tool.corner_r = corner_r;
        tool.tip_angle = tip_angle;
        let err = export(&tool).expect_err("export must preserve the cutter geometry");
        assert!(err.contains("Finisher") && err.contains("cannot preserve the cutter geometry"), "{err}");
    }
    tool = back[0].tool.clone();
    tool.flute_len += 1.0;
    assert!(export(&tool).unwrap_err().contains("cannot preserve the cutter geometry"));
}

#[test]
fn pathpilot_refuses_threading_tools_without_known_pitch() {
    for name in ["tap M6x0.5", "tap M6", "thread mill M6x0.5", "single thread mill M6"] {
        let csv = format!(
            "Mill Tool Table,x - v1,This must be the first row - do not edit\n\
Tool number,Description,Z offset,Diameter,Z offset wear,Diameter wear,Max RPM\n\
17,{name},0,6,0,0,0\n"
        );
        let err = parse_controller_table(ToolTableFormat::PathPilot, &csv, Unit::Millimeters)
            .expect_err("a description must not supply an invented thread pitch");
        assert!(err.contains(name) && err.contains("pitch"), "{err}");
    }
}

#[test]
fn hurco_requires_explicit_thread_pitch_and_preserves_fine_pitch() {
    for (code, kind) in [(12, ToolKind::Tap), (22, ToolKind::ThreadMill), (26, ToolKind::ThreadMillSingle)] {
        let xml = |pitch: &str| format!(
            "<Hurco_Document><Tools><Tool><Name>Fine Thread {code}</Name><Number>17</Number>\
<Diameter>6</Diameter><ToolType>{code}</ToolType>{pitch}</Tool></Tools></Hurco_Document>"
        );
        for pitch in ["", "<ThreadPitch>0</ThreadPitch>", "<ThreadPitch>-0.5</ThreadPitch>"] {
            let err = parse_controller_table(ToolTableFormat::Hurco, &xml(pitch), Unit::Millimeters)
                .expect_err("threading tools need a recorded positive pitch");
            assert!(err.contains(&format!("Fine Thread {code}")) && err.contains("T17") && err.contains("pitch"), "{err}");
        }
        for (unit, value) in [(Unit::Millimeters, "0.5"), (Unit::Inches, "0.01968503937007874")] {
            let entries = parse_controller_table(
                ToolTableFormat::Hurco, &xml(&format!("<ThreadPitch>{value}</ThreadPitch>")), unit,
            ).expect("explicit fine pitch");
            assert_eq!(entries[0].tool.kind, kind);
            assert!((entries[0].tool.pitch - 0.5).abs() < 1e-12);
            if kind == ToolKind::ThreadMillSingle {
                assert!((entries[0].tool.pitch_min - 0.5).abs() < 1e-12);
            }
        }
    }
}

#[test]
fn hurco_refuses_left_hand_tap_export_and_roundtrips_right_hand() {
    let xml = "<Hurco_Document><Tools><Tool><Name>Fine Tap</Name><Number>17</Number>\
<Diameter>6</Diameter><ThreadPitch>0.5</ThreadPitch><ToolType>12</ToolType></Tool></Tools></Hurco_Document>";
    let mut tap = parse_controller_table(ToolTableFormat::Hurco, xml, Unit::Millimeters)
        .expect("fine-pitch tap").remove(0).tool;
    let export = |tool: &Tool| export_controller_table(
        ToolTableFormat::Hurco,
        &[ToolTableExportRow { number: 17, name: "Fine Tap", tool, gauge_len: None }],
        Unit::Millimeters,
    );
    let exported = export(&tap).expect("right-hand tap export");
    let entries = parse_controller_table(ToolTableFormat::Hurco, &exported, Unit::Millimeters)
        .expect("right-hand tap round trip");
    assert_eq!(entries[0].tool.kind, ToolKind::Tap);
    assert_eq!(entries[0].tool.pitch, 0.5);
    assert_eq!(entries[0].tool.thread_hand, camcraft_tools::tools::ThreadHand::Right);
    tap.thread_hand = camcraft_tools::tools::ThreadHand::Left;
    assert!(tap.why_not_sane().is_none(), "fixture must be a valid left-hand tap");
    let err = export(&tap).expect_err("Hurco export must not silently change tap hand");
    assert!(err.contains("Fine Tap") && err.contains("left-hand"), "{err}");
}

#[test]
fn hurco_refuses_non_center_cutting_exports_and_preserves_supported_roundtrips() {
    for (code, kind, geometry) in [
        (0, ToolKind::BullNose, "<TipRadius>1</TipRadius>"),
        (2, ToolKind::Taper, "<Angle>90</Angle>"),
        (16, ToolKind::BallNose, ""),
        (14, ToolKind::EndMill, ""),
    ] {
        let name = format!("Six mm {kind}");
        let xml = format!(
            "<Hurco_Document><Tools><Tool><Name>{name}</Name><Number>17</Number>\
<Diameter>6</Diameter><ToolType>{code}</ToolType>{geometry}</Tool></Tools></Hurco_Document>"
        );
        let mut tool = parse_controller_table(ToolTableFormat::Hurco, &xml, Unit::Millimeters)
            .expect("sane cutter fixture").remove(0).tool;
        for center_cutting in [true, false] {
            tool.center_cutting = center_cutting;
            assert!(tool.why_not_sane().is_none(), "fixture must be sane: {name}");
            let result = export_controller_table(
                ToolTableFormat::Hurco,
                &[ToolTableExportRow { number: 17, name: &name, tool: &tool, gauge_len: None }],
                Unit::Millimeters,
            );
            if !center_cutting && kind != ToolKind::EndMill {
                let err = result.expect_err("export must preserve the plunge restriction");
                assert!(err.contains(&name) && err.contains("non-center-cutting"), "{err}");
            } else {
                let exported = result.expect("supported cutter export");
                let back = parse_controller_table(ToolTableFormat::Hurco, &exported, Unit::Millimeters)
                    .expect("supported cutter round trip");
                assert_eq!(back[0].tool.kind, kind);
                assert_eq!(back[0].tool.center_cutting, center_cutting, "{name}");
            }
        }
    }
}

#[test]
fn hurco_refuses_lossy_holder_diameter_and_preserves_inferred_roundtrips() {
    let xml = "<Hurco_Document><Tools><Tool><Name>4mm EM</Name><Number>3</Number>\
<Diameter>4</Diameter><ShankDiameter>4</ShankDiameter><AbsoluteGaugeLength>35</AbsoluteGaugeLength>\
<ToolType>14</ToolType></Tool></Tools></Hurco_Document>";
    let base = parse_controller_table(ToolTableFormat::Hurco, xml, Unit::Millimeters)
        .expect("sane mill tool").remove(0).tool;
    assert_eq!(base.holder_d, 20.0);

    let export = |name: &str, tool: &Tool, unit: Unit| export_controller_table(
        ToolTableFormat::Hurco,
        &[ToolTableExportRow { number: 3, name, tool, gauge_len: None }],
        unit,
    );

    // Matching inferred holder diameter round-trips in mm and inches.
    for unit in [Unit::Millimeters, Unit::Inches] {
        let text = export("4mm EM", &base, unit).expect("matching inferred holder");
        let back = parse_controller_table(ToolTableFormat::Hurco, &text, unit).expect("re-parse");
        assert!((back[0].tool.holder_d - 20.0).abs() < 1e-4, "{unit:?}: {}", back[0].tool.holder_d);
        assert!((back[0].tool.holder_len - 35.0).abs() < 1e-4, "{unit:?}: {}", back[0].tool.holder_len);
    }

    // Wider holders (including a 60 mm holder on a 4 mm shank) and mismatched holders must refuse by name.
    for (holder_d, holder_len) in [(60.0, 35.0), (60.0, 0.0), (10.0, 35.0), (0.0, 35.0)] {
        let wider = Tool { holder_d, holder_len, ..base.clone() };
        assert!(wider.why_not_sane().is_none(), "fixture must be sane: ({holder_d}, {holder_len})");
        let err = export("4mm Wide Holder", &wider, Unit::Millimeters)
            .expect_err("Hurco export must refuse unpreserved holder diameter");
        assert!(err.contains("4mm Wide Holder") && err.contains("holder diameter"), "{err}");
    }
}

#[test]
fn hurco_refuses_boring_bar_with_positive_nose_radius_and_roundtrips_zero() {
    let xml = "<Hurco_Document><Tools><Tool><Name>Boring Bar</Name><Number>6</Number>\
<Diameter>16</Diameter><ToolType>13</ToolType></Tool></Tools></Hurco_Document>";
    let mut bore = parse_controller_table(ToolTableFormat::Hurco, xml, Unit::Millimeters)
        .expect("sane boring bar").remove(0).tool;
    assert_eq!(bore.kind, ToolKind::Bore);
    assert_eq!(bore.nose_radius, 0.0);

    let export = |name: &str, tool: &Tool| export_controller_table(
        ToolTableFormat::Hurco,
        &[ToolTableExportRow { number: 6, name, tool, gauge_len: None }],
        Unit::Millimeters,
    );

    let zero_text = export("Boring Bar", &bore).expect("zero nose radius round-trips");
    let back = parse_controller_table(ToolTableFormat::Hurco, &zero_text, Unit::Millimeters).expect("re-parse");
    assert_eq!(back[0].tool.kind, ToolKind::Bore);
    assert_eq!(back[0].tool.nose_radius, 0.0);

    bore.nose_radius = 0.8;
    assert!(bore.why_not_sane().is_none(), "fixture must be a sane boring bar");
    let err = export("Ground Bore", &bore).expect_err("Hurco export must refuse positive boring nose radius");
    assert!(err.contains("Ground Bore") && err.contains("nose radius"), "{err}");
}
