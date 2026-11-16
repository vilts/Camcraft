//! Pins for the shipped starter catalog asset (#741): it parses with every
//! tool valid, keeps the display order the Rust table had (checked tool for
//! tool against that table before it was deleted), and a bad entry is a
//! named error, never a panic.

use crate::catalog_asset::{CATALOG_JSON, parse_catalog};
use crate::tools::{Tool, ToolKind, ToolMaterial, catalog, probe_tool, torch_tool};

const ORDER: [&str; 32] = [
    "EM-0200-2FL", "EM-0300-2FL", "EM-0400-2FL", "EM-0600-4FL", "EM-0800-4FL", "EM-1000-4FL", "EM-1200-2FL",
    "BN-0600", "BN-1000", "BB-1000-R1", "DR-0200", "DR-0300", "DR-0400", "DR-0600", "CM-0400-90", "CM-0600-90",
    "CM-1000-90", "TAP-M4", "TAP-M6", "BR-1000", "TN-1600-0R8", "GT-0200", "GT-0300", "TD-0866-60", "TD-1732-60",
    "FG-0600-04", "FG-1200-08", "PRB-0400", "PT-KERF-150", "EDM-E10X6", "TM-0600-P125", "TMS-0600-P050-150",
];

fn tool(name: &str) -> Tool {
    catalog().into_iter().find(|(n, _)| *n == name).map(|(_, t)| t).expect("catalog tool")
}

#[test]
fn the_shipped_asset_parses_and_every_tool_is_valid() {
    let v = parse_catalog(CATALOG_JSON).expect("shipped asset parses");
    assert_eq!(v.iter().map(|(n, _)| n.as_str()).collect::<Vec<_>>(), ORDER);
    for (n, t) in &v {
        assert!(t.is_sane(), "{n}");
    }
    assert_eq!(catalog().len(), ORDER.len());
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
