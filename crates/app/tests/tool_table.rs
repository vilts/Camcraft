//! #2566: PathPilot (.csv) and Hurco WinMax (.TCM) machine tool tables:
//! shared import and export over MCP (`import_tool_library`,
//! `export_tool_library`), one undo step, controller tool numbers,
//! Hurco gauge lengths, and named refusals.
//!
//! Provenance: our own integration tests over the recorded sample
//! shapes described in issue #2566; no vendor library copied.
//!
//! Gate map (M4.4): covers app::tool_library, app::tool_table,
//! app::commands (tool_import), app::mcp (tools), app::script (api).

use camcraft_app::mcp::Server;
use serde_json::{Value, json};
use std::path::PathBuf;

fn tmp_dir(tag: &str) -> PathBuf {
    let mut p = std::env::temp_dir();
    p.push(format!("camcraft-tooltable-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&p);
    std::fs::create_dir_all(&p).expect("temp dir");
    p
}

struct Client {
    server: Server,
    next: i64,
}

impl Client {
    fn new() -> Self {
        Self { server: Server::new(), next: 1 }
    }

    fn call(&mut self, name: &str, args: Value) -> (bool, Value) {
        let id = self.next;
        self.next += 1;
        let msg = json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": "tools/call",
            "params": { "name": name, "arguments": args }
        });
        let reply = self.server.handle(&msg).expect("reply");
        let result = &reply["result"];
        let is_err = result["isError"] == json!(true);
        let body = if is_err {
            result["content"][0]["text"].clone()
        } else {
            result["structuredContent"].clone()
        };
        (is_err, body)
    }

    fn ok(&mut self, name: &str, args: Value) -> Value {
        let (err, body) = self.call(name, args);
        assert!(!err, "{name} refused unexpectedly: {body}");
        body
    }

    fn refused(&mut self, name: &str, args: Value) -> String {
        let (err, body) = self.call(name, args);
        assert!(err, "{name} succeeded unexpectedly: {body}");
        body.as_str().expect("error string").to_owned()
    }
}

const PATHPILOT_SAMPLE: &str = "\
Mill Tool Table,2026-11-18 10:00:00 - v1,This must be the first row - do not edit\r\n\
Tool number,Description,Z offset,Diameter,Z offset wear,Diameter wear,Max RPM\r\n\
1,endmill [1/8] 2FL R:0.000 loc:0.375 carbide dia:0.125 [37368],1.0000000,0.1250000,0.0000000,0.0000000,0.0\r\n\
2,,0.0000000,0.0000000,0.0000000,0.0000000,0.0\r\n\
10,drill dia:0.500,2.0000000,0.5000000,0.0000000,0.0000000,0.0\r\n\
99,,3.0000000,0.0000000,0.0000000,0.0000000,-2.0\r\n";

const HURCO_SAMPLE: &str = "\u{FEFF}<?xml version=\"1.0\" encoding=\"utf-8\"?>\r\n\
<Hurco_Document Version=\"1.1\">\r\n\
  <FileInfo>\r\n\
    <Language>English</Language>\r\n\
    <Machine>VMX30U</Machine>\r\n\
    <SoftwareVersion>09.01.393.30</SoftwareVersion>\r\n\
  </FileInfo>\r\n\
  <Tools>\r\n\
    <Tool Id=\"11111111-1111-4111-8111-111111111111\">\r\n\
      <Name>10MM FLAT END MILL</Name>\r\n\
      <Number>4</Number>\r\n\
      <Diameter>10.0</Diameter>\r\n\
      <ToolLength>45.0</ToolLength>\r\n\
      <ShankDiameter>10.0</ShankDiameter>\r\n\
      <FluteLength>22.0</FluteLength>\r\n\
      <AbsoluteGaugeLength>115.5</AbsoluteGaugeLength>\r\n\
      <ImportZeroCal>0.0</ImportZeroCal>\r\n\
      <ToolType>14</ToolType>\r\n\
      <NumberOfFlutes>4</NumberOfFlutes>\r\n\
      <Manufacturer>OSG</Manufacturer>\r\n\
      <UsedTime>1234</UsedTime>\r\n\
      <Flags>7</Flags>\r\n\
      <GeometryFlags>3</GeometryFlags>\r\n\
    </Tool>\r\n\
    <Tool Id=\"22222222-2222-4222-8222-222222222222\">\r\n\
      <Name>6MM BALL END MILL</Name>\r\n\
      <Number>7</Number>\r\n\
      <Diameter>6.0</Diameter>\r\n\
      <ToolLength>38.0</ToolLength>\r\n\
      <ShankDiameter>6.0</ShankDiameter>\r\n\
      <FluteLength>14.0</FluteLength>\r\n\
      <AbsoluteGaugeLength>98.25</AbsoluteGaugeLength>\r\n\
      <ToolType>16</ToolType>\r\n\
      <NumberOfFlutes>2</NumberOfFlutes>\r\n\
    </Tool>\r\n\
    <Tool Id=\"33333333-3333-4333-8333-333333333333\">\r\n\
      <Name>RENISHAW PROBE</Name>\r\n\
      <Number>99</Number>\r\n\
      <Diameter>4.0</Diameter>\r\n\
      <ToolLength>40.0</ToolLength>\r\n\
      <ShankDiameter>6.0</ShankDiameter>\r\n\
      <ToolType>18</ToolType>\r\n\
    </Tool>\r\n\
  </Tools>\r\n\
</Hurco_Document>\r\n";

#[test]
fn pathpilot_csv_imports_with_numbers_diameters_undo_and_roundtrips() {
    let dir = tmp_dir("pathpilot");
    let csv = dir.join("770mx.csv");
    std::fs::write(&csv, PATHPILOT_SAMPLE).expect("write csv");

    let mut c = Client::new();
    let imp = c.ok(
        "import_tool_library",
        json!({ "path": csv.display().to_string(), "units": "inch" }),
    );
    assert_eq!(imp["added"].as_array().map(Vec::len), Some(2), "{imp}");

    let job = c.ok("get_job", json!({}));
    let tools = job["tools"].as_array().expect("tools");
    assert_eq!(tools.len(), 2, "empty slots and unconfigured probe marker skipped: {job}");

    let t1 = tools.iter().find(|t| t["number"] == json!(1)).expect("T1");
    assert_eq!(t1["name"], json!("endmill [1/8] 2FL R:0.000 loc:0.375 carbide dia:0.125 [37368]"));
    let d1 = t1["definition"]["diameter"].as_f64().expect("d1");
    assert!((d1 - 0.125 * 25.4).abs() < 1e-6, "T1 diameter {d1}");
    let fl1 = t1["definition"]["flute_len"].as_f64().expect("fl1");
    assert!((fl1 - 0.375 * 25.4).abs() < 1e-6, "T1 flute_len {fl1}");
    assert_eq!(t1["definition"]["flutes"], json!(2));
    assert_eq!(t1["definition"]["tool_material"], json!("carbide"));
    assert_eq!(t1["definition"]["part_number"], json!("37368"));

    let t10 = tools.iter().find(|t| t["number"] == json!(10)).expect("T10");
    assert_eq!(t10["definition"]["kind"], json!("drill"));
    assert!((t10["definition"]["diameter"].as_f64().unwrap() - 0.5 * 25.4).abs() < 1e-6);

    let out_csv = dir.join("hub_export.csv");
    let exp = c.ok(
        "export_tool_library",
        json!({ "path": out_csv.display().to_string(), "units": "inch" }),
    );
    assert_eq!(exp["tools"], json!(2));
    let written = std::fs::read_to_string(&out_csv).expect("read exported csv");
    let lines: Vec<&str> = written.lines().collect();
    assert!(lines[0].starts_with("Mill Tool Table,") && lines[0].ends_with(",This must be the first row - do not edit"), "{}", lines[0]);
    assert_eq!(lines[1], "Tool number,Description,Z offset,Diameter,Z offset wear,Diameter wear,Max RPM");

    c.ok("undo", json!({}));
    assert!(c.ok("get_job", json!({}))["tools"].as_array().unwrap().is_empty(), "one undo clears import");
    c.ok("redo", json!({}));
    assert_eq!(c.ok("get_job", json!({}))["tools"].as_array().unwrap().len(), 2);

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn hurco_tcm_imports_with_numbers_kinds_diameters_gauge_lengths_and_exports_without_control_state() {
    let dir = tmp_dir("hurco");
    let tcm = dir.join("atc_mtc.TCM");
    std::fs::write(&tcm, HURCO_SAMPLE).expect("write tcm");

    let mut c = Client::new();
    let imp = c.ok("import_tool_library", json!({ "path": tcm.display().to_string() }));
    assert_eq!(imp["added"].as_array().map(Vec::len), Some(3), "{imp}");

    let job = c.ok("get_job", json!({}));
    let tools = job["tools"].as_array().expect("tools");
    let t4 = tools.iter().find(|t| t["number"] == json!(4)).expect("T4");
    assert_eq!(t4["definition"]["kind"], json!("end_mill"));
    assert_eq!(t4["definition"]["diameter"], json!(10.0));
    assert_eq!(t4["definition"]["flute_len"], json!(22.0));
    assert_eq!(t4["definition"]["holder_len"], json!(115.5));
    assert_eq!(t4["definition"]["flutes"], json!(4));
    assert_eq!(t4["definition"]["vendor"], json!("OSG"));

    let t7 = tools.iter().find(|t| t["number"] == json!(7)).expect("T7");
    assert_eq!(t7["definition"]["kind"], json!("ball_nose"));
    assert_eq!(t7["definition"]["diameter"], json!(6.0));
    assert_eq!(t7["definition"]["holder_len"], json!(98.25));

    let t99 = tools.iter().find(|t| t["number"] == json!(99)).expect("T99");
    assert_eq!(t99["definition"]["kind"], json!("probe"));
    assert_eq!(t99["definition"]["diameter"], json!(4.0));

    let out_tcm = dir.join("winmax_export.TCM");
    c.ok("export_tool_library", json!({ "path": out_tcm.display().to_string() }));
    let bytes = std::fs::read(&out_tcm).expect("read tcm");
    assert!(bytes.starts_with(b"\xEF\xBB\xBF"), "UTF-8 BOM required");
    let text = std::str::from_utf8(&bytes[3..]).expect("utf8");
    assert!(text.contains("\r\n"), "CRLF required");
    assert!(text.contains("<Hurco_Document Version=\"1.1\">"));
    assert!(text.contains("<AbsoluteGaugeLength>115.5</AbsoluteGaugeLength>") || text.contains("<AbsoluteGaugeLength>115.5000</AbsoluteGaugeLength>"));
    for control_field in ["<UsedTime>", "<Flags>", "<GeometryFlags>", "<ImportZeroCal>"] {
        assert!(!text.contains(control_field), "must not overwrite control state {control_field}");
    }

    let mut dst = Client::new();
    dst.ok("import_tool_library", json!({ "path": out_tcm.display().to_string() }));
    assert_eq!(dst.ok("get_job", json!({}))["tools"], job["tools"], "TCM round-trips cleanly");

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn unsupported_hurco_tool_type_and_bad_pathpilot_rows_are_refused_by_name() {
    let dir = tmp_dir("refusals");
    let keyseat = dir.join("bad.TCM");
    std::fs::write(
        &keyseat,
        "<Hurco_Document Version=\"1.1\"><Tools><Tool Id=\"1\">\
         <Name>T8 KEYSEAT</Name><Number>8</Number><Diameter>12.0</Diameter>\
         <ToolLength>40.0</ToolLength><ToolType>8</ToolType></Tool></Tools></Hurco_Document>",
    )
    .expect("write");

    let mut c = Client::new();
    let msg = c.refused("import_tool_library", json!({ "path": keyseat.display().to_string() }));
    assert!(msg.contains("T8 KEYSEAT"), "must name tool: {msg}");
    assert!(msg.to_lowercase().contains("keyseat"), "must name Hurco tool type: {msg}");
    assert!(c.ok("get_job", json!({}))["tools"].as_array().unwrap().is_empty());

    let bad_csv = dir.join("bad.csv");
    std::fs::write(
        &bad_csv,
        "Mill Tool Table,2026-11-18 - v1,This must be the first row - do not edit\n\
         Tool number,Description,Z offset,Diameter,Z offset wear,Diameter wear,Max RPM\n\
         5,Broken Cutter,0.0,not_a_number,0.0,0.0,0.0\n",
    )
    .expect("write");
    let msg = c.refused("import_tool_library", json!({ "path": bad_csv.display().to_string() }));
    assert!(msg.contains("Broken Cutter") || msg.contains("T5"), "must name bad row: {msg}");

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn controller_number_conflicts_refuse_in_add_mode_and_swap_in_replace_or_update() {
    let dir = tmp_dir("clash");
    let csv = dir.join("tools.csv");
    std::fs::write(
        &csv,
        "Mill Tool Table,2026-11-18 - v1,This must be the first row - do not edit\r\n\
         Tool number,Description,Z offset,Diameter,Z offset wear,Diameter wear,Max RPM\r\n\
         1,1/4 Flat Endmill,0.0,0.2500000,0.0,0.0,0.0\r\n",
    )
    .expect("write");

    let mut c = Client::new();
    // Existing tool claims T1.
    c.ok("add_tool", json!({ "catalog": "EM-0600-4FL" }));

    // Add mode refuses T1 number clash by name, leaving document intact.
    let err = c.refused(
        "import_tool_library",
        json!({ "path": csv.display().to_string(), "mode": "add", "units": "inch" }),
    );
    assert!(err.contains("T1") && err.contains("EM-0600-4FL"), "{err}");

    // Replace mode drops EM-0600-4FL, lands 1/4 Flat Endmill at T1, and undo restores EM-0600-4FL at T1.
    let rep = c.ok(
        "import_tool_library",
        json!({ "path": csv.display().to_string(), "mode": "replace", "units": "inch" }),
    );
    assert_eq!(rep["removed"], json!(["EM-0600-4FL"]));
    assert_eq!(rep["added"], json!(["1/4 Flat Endmill"]));
    let job = c.ok("get_job", json!({}));
    assert_eq!(job["tools"][0]["number"], json!(1));
    assert_eq!(job["tools"][0]["name"], json!("1/4 Flat Endmill"));

    c.ok("undo", json!({}));
    let undone = c.ok("get_job", json!({}));
    assert_eq!(undone["tools"][0]["name"], json!("EM-0600-4FL"));
    assert_eq!(undone["tools"][0]["number"], json!(1));

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn script_verbs_roundtrip_pathpilot_csv_and_hurco_tcm() {
    let dir = tmp_dir("script");
    let csv = dir.join("pp.csv");
    let tcm = dir.join("hurco.TCM");
    std::fs::write(&csv, PATHPILOT_SAMPLE).expect("write csv");
    let out = camcraft_app::script::run(&format!(
        "var r = cam.importTools({0:?}, \"add\", true, \"pathpilot\", \"inch\"); \
         cam.exportTools({1:?}, \"hurco\", \"mm\"); r",
        csv.display().to_string(),
        tcm.display().to_string()
    ));
    assert!(out.ok, "{:?}", out.errors);
    assert_eq!(out.result["format"], json!("pathpilot"));
    assert_eq!(out.result["added"].as_array().map(Vec::len), Some(2));
    let written = std::fs::read_to_string(&tcm).expect("tcm written");
    assert!(written.contains("<Hurco_Document Version=\"1.1\">"));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn hurco_export_refuses_catalog_wider_holder_and_positive_nose_radius_bore() {
    let dir = tmp_dir("hurco-refusals");
    let out_tcm = dir.join("refused.TCM");

    let mut c1 = Client::new();
    c1.ok("add_tool", json!({ "catalog": "EM-0200-2FL" }));
    let err = c1.refused("export_tool_library", json!({ "path": out_tcm.display().to_string() }));
    assert!(err.contains("EM-0200-2FL") && err.contains("holder diameter"), "{err}");
    assert!(!out_tcm.exists(), "refused export must not write file");

    let mut c2 = Client::new();
    c2.ok("add_tool", json!({ "catalog": "BR-1000" }));
    let err = c2.refused("export_tool_library", json!({ "path": out_tcm.display().to_string() }));
    assert!(err.contains("BR-1000") && err.contains("nose radius"), "{err}");
    assert!(!out_tcm.exists(), "refused export must not write file");

    let in_tcm = dir.join("in.TCM");
    std::fs::write(
        &in_tcm,
        "<Hurco_Document Version=\"1.1\"><Tools><Tool Id=\"1\">\
         <Name>6MM BALL</Name><Number>7</Number><Diameter>6.0</Diameter>\
         <ShankDiameter>6.0</ShankDiameter><AbsoluteGaugeLength>98.25</AbsoluteGaugeLength>\
         <ToolType>16</ToolType></Tool></Tools></Hurco_Document>",
    )
    .expect("write");
    let mut c3 = Client::new();
    c3.ok("import_tool_library", json!({ "path": in_tcm.display().to_string() }));
    c3.ok("set_tool_holder", json!({ "tool": "6MM BALL", "library": "BT30 ER20" }));
    let err = c3.refused("export_tool_library", json!({ "path": out_tcm.display().to_string() }));
    assert!(err.contains("6MM BALL") && err.contains("holder diameter"), "{err}");
    assert!(!out_tcm.exists(), "refused export must not write file");

    let _ = std::fs::remove_dir_all(&dir);
}
