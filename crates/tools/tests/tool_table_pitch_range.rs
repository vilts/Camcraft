//! Provenance: original round-trip tests for the documented Hurco .TCM format.
//! Gate map (M4.4): covers tools::tool_table_hurco_export, tools::tool_table_hurco.

use camcraft_geometry::geometry::units::Unit;
use camcraft_tools::thread_mill::resolve_pitch;
use camcraft_tools::tool_table::{
    ToolTableExportRow, ToolTableFormat, export_controller_table, parse_controller_table,
};
use camcraft_tools::tools::{Tool, ToolKind};

#[test]
fn hurco_refuses_single_form_pitch_range_loss_and_roundtrips_equal_pitches() {
    let xml = "<Hurco_Document><Tools><Tool><Name>Single Form</Name><Number>26</Number>\
<Diameter>8</Diameter><ThreadPitch>1.5</ThreadPitch><ToolType>26</ToolType>\
</Tool></Tools></Hurco_Document>";
    let mut tool = parse_controller_table(ToolTableFormat::Hurco, xml, Unit::Millimeters)
        .expect("single-form thread mill")
        .remove(0)
        .tool;
    assert_eq!(tool.kind, ToolKind::ThreadMillSingle);
    assert_eq!(tool.pitch, 1.5);
    assert_eq!(tool.pitch_min, 1.5);

    let export = |tool: &Tool, unit| {
        export_controller_table(
            ToolTableFormat::Hurco,
            &[ToolTableExportRow {
                number: 26,
                name: "Single Form Range",
                tool,
                gauge_len: None,
            }],
            unit,
        )
    };

    for unit in [Unit::Millimeters, Unit::Inches] {
        let text = export(&tool, unit).expect("equal pitches survive conversion");
        let back = parse_controller_table(ToolTableFormat::Hurco, &text, unit)
            .expect("single-form thread mill round trip");
        assert_eq!(back[0].tool.kind, ToolKind::ThreadMillSingle);
        assert!((back[0].tool.pitch - tool.pitch).abs() < 1e-4);
        assert!((back[0].tool.pitch_min - tool.pitch_min).abs() < 1e-4);
        assert_eq!(
            resolve_pitch(&back[0].tool, Some(back[0].tool.pitch)),
            Ok(back[0].tool.pitch),
        );
        assert!(resolve_pitch(&back[0].tool, Some(1.0)).is_err());
    }

    tool.pitch_min = 0.5;
    assert!(
        tool.why_not_sane().is_none(),
        "the pitch-range fixture must be sane"
    );
    assert_eq!(resolve_pitch(&tool, Some(1.0)), Ok(1.0));
    for unit in [Unit::Millimeters, Unit::Inches] {
        let err = export(&tool, unit).expect_err("Hurco must not narrow the supported pitch range");
        assert!(
            err.contains("Single Form Range") && err.contains("cannot preserve"),
            "{err}"
        );
    }
}
