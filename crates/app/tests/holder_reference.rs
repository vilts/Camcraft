//! #2016 acceptance: a tool with a library holder stores a reference to
//! the entry (its name, the gauge length, the projection), not a copy
//! of its outline, and every reader gets the one shape the reference
//! resolves to. Pinned: the reference is what the document, the project
//! file and the tool library file hold; for every library entry the
//! judged tool, the band stack, the stamp term and the facets are those
//! of the drawn copy an older build stored; an older project and an
//! older `.camtools` file (a drawn copy, a plain holder cylinder) load
//! as the custom holder they were with the same collision result; a
//! name the library does not hold refuses the file by name; undo and
//! the unchanged outline keep the reference, an edited point drops it;
//! MCP's `set_tool_holder` writes it.
//! Provenance: our own tests over our own format and commands.
//!
//! Gate map (M4.4): covers app::tool_body, app::commands (tool_body),
//! app::project (tool_body_wire), app::tool_library, app::mcp (holders).

use std::hash::Hasher;

use camcraft_app::commands::{AddTool, CreateOperation, GenerateOperation, History, SetStock, SetToolBody, TagFixture};
use camcraft_app::geometry::{Point3, Triangle};
use camcraft_app::holder_library::{self, Clamp, HolderEntry, HolderRef};
use camcraft_app::mcp::Server;
use camcraft_app::model;
use camcraft_app::operation::{Height, OpParams, OpStatus, SrcRef, status_of};
use camcraft_app::project::{self, ProjectError, ToolBodyDto};
use camcraft_app::schema::{Location, ToolBody};
use camcraft_app::spun::{SpunPoint, SpunProfile};
use camcraft_app::stock::StockShape;
use camcraft_app::tool_body;
use camcraft_app::tool_library::{
    CUTTER_VERSION, CUTTING_STEPS_VERSION, HOLDER_REF_VERSION, TIP_FORM_VERSION, export_tool_library, parse_tool_library,
};
use camcraft_app::tools::{catalog, Tool};
use camcraft_app::verify::{FixturePart, Warning};
use camcraft_app::world::new_world;
use flecs_ecs::prelude::World;
use serde_json::{json, Value};

/// A 4 mm end mill on a 6 mm shank: flutes 12, shank 40 - the holder's
/// nose face stands 52 above the tip.
const TOOL: &str = "EM-0400-2FL";
const BARE: &str = "EM-0800-4FL";
const NOSE: f64 = 52.0;

fn catalog_tool(name: &str) -> Tool {
    catalog().into_iter().find(|(n, _)| *n == name).map(|(_, t)| t).expect("catalog entry")
}

fn body(world: &World, name: &str) -> Option<ToolBody> {
    model::find_by_name(world, name).and_then(|e| tool_body::body_of(world, &e))
}

/// The drawn copy an older build stored for a library pick: the
/// entry's outline as a spun profile, with no link to the entry.
fn old_copy(entry: &HolderEntry, gauge_len: f64, projection: f64) -> ToolBody {
    let points = entry.points(projection, gauge_len).expect("outline");
    tool_body::build_spun(SpunProfile::new(points).expect("profile"))
}

/// Everything a law reads off tool `name`: the judged tool and its
/// envelope, the band stack, the stamp term and the facets.
fn reads(world: &World, name: &str) -> String {
    let e = model::find_by_name(world, name).expect("tool");
    let tool = model::tool_of(world, &e).expect("a tool");
    let (judged, profile) = tool_body::judged(world, &e, tool).expect("judged");
    let stack = tool_body::stack(&judged.geom(), profile.as_ref());
    let mut h = std::collections::hash_map::DefaultHasher::new();
    tool_body::digest(world, &e, &mut h);
    let facets = tool_body::body_of(world, &e).map(|b| b.facets.to_vec());
    format!("{judged:?}|{profile:?}|{stack:?}|{:x}|{facets:?}", h.finish())
}

/// A shank the entry clamps.
fn shank_for(e: &HolderEntry) -> f64 {
    match e.clamp {
        Clamp::Er(er) => er.max_bore.min(er.min_bore.max(6.0)),
        Clamp::Weldon { bore, .. } => bore,
    }
}

fn tool_for(e: &HolderEntry) -> Tool {
    let mut t = catalog_tool(TOOL);
    t.shank_d = shank_for(e);
    t.holder_d = t.holder_d.max(t.shank_d + 4.0);
    t
}

#[test]
fn a_library_holder_is_the_reference_in_the_document_and_the_file() {
    let (world, mut history) = (new_world(), History::default());
    history.execute(&world, AddTool::new(&world, TOOL, catalog_tool(TOOL)).expect("tool"));
    history.execute(&world, SetToolBody::library(&world, TOOL, "bt30  er20", None, None).expect("pick"));
    let want = HolderRef { entry: "BT30 ER20".into(), gauge_len: 60.0, projection: NOSE };
    assert_eq!(body(&world, TOOL).and_then(|b| b.library), Some(want.clone()), "the entry's own name, both lengths stated");

    let text = project::save_text(&world).expect("save");
    let file: Value = serde_json::from_str(&text).expect("json");
    const { assert!(project::PROJECT_VERSION >= 70, "v69 and earlier readers must refuse the holder reference"); }
    assert_eq!(file["version"], json!(project::PROJECT_VERSION));
    let mut future = file.clone();
    future["version"] = json!(project::NEWEST_PROJECT_VERSION + 1);
    assert!(matches!(project::open(&new_world(), &future.to_string()), Err(ProjectError::UnsupportedVersion(_))));
    let tool = file["entities"].as_array().expect("entities").iter().find(|e| e["name"] == TOOL).expect("the tool")["tool"].clone();
    assert_eq!(tool["holder"], json!({ "entry": "BT30 ER20", "gauge_len": 60.0, "projection": 52.0 }));
    assert!(tool.get("body").is_none(), "no copy of the outline is written: {tool}");
    assert!(!tool.to_string().contains("facets") && !tool.to_string().contains("spun"), "{tool}");

    let back = new_world();
    project::open(&back, &text).expect("load");
    assert_eq!(body(&back, TOOL), body(&world, TOOL), "the reference draws the same body on load");
    assert_eq!(reads(&back, TOOL), reads(&world, TOOL));
    assert_eq!(project::save_text(&back).expect("save again"), text, "save -> load -> save is byte-stable");

    // Undo and redo carry the reference with the body.
    assert!(history.undo(&world));
    assert_eq!(body(&world, TOOL), None);
    assert!(history.redo(&world));
    assert_eq!(body(&world, TOOL).and_then(|b| b.library), Some(want));
}

/// The safety floor: for every entry of the library, the referenced
/// holder is read exactly as the drawn copy an older build stored -
/// picked, and assumed (a machine default, floored by #828).
#[test]
fn every_entry_reads_the_same_bands_by_reference_as_its_old_copy() {
    let mut seen = 0;
    for e in holder_library::library() {
        let tool = tool_for(&e);
        for (gauge, projection) in [(None, None), (Some(e.min_gauge() + 25.0), Some(NOSE + 7.5))] {
            for assumed in [false, true] {
                let (by_ref, copied) = (new_world(), new_world());
                let mut h = History::default();
                h.execute(&by_ref, AddTool::new(&by_ref, TOOL, tool.clone()).expect("tool"));
                h.execute(&copied, AddTool::new(&copied, TOOL, tool.clone()).expect("tool"));
                h.execute(&by_ref, SetToolBody::library(&by_ref, TOOL, &e.name(), gauge, projection).expect("pick"));
                let mut copy = old_copy(&e, gauge.unwrap_or(e.gauge_len), projection.unwrap_or(NOSE));
                let mut landed = body(&by_ref, TOOL).expect("landed");
                assert!(landed.library.is_some() && copy.library.is_none());
                copy.machine_default = assumed;
                landed.machine_default = assumed;
                let (er, ec) = (model::find_by_name(&by_ref, TOOL).expect("tool"), model::find_by_name(&copied, TOOL).expect("tool"));
                tool_body::set_body(&by_ref, &er, Some(&landed));
                tool_body::set_body(&copied, &ec, Some(&copy));
                assert_eq!(reads(&by_ref, TOOL), reads(&copied, TOOL), "{} gauge {gauge:?} assumed {assumed}", e.name());
                // And through the file: the reference re-drawn on load.
                let back = new_world();
                project::open(&back, &project::save_text(&by_ref).expect("save")).expect("load");
                assert_eq!(reads(&back, TOOL), reads(&copied, TOOL), "{} reloaded, assumed {assumed}", e.name());
                assert_eq!(body(&back, TOOL).map(|b| b.machine_default), Some(assumed));
                seen += 1;
            }
        }
    }
    assert!(seen >= 4 * 40, "the whole library was compared: {seen}");
}

const CUT_Z: f64 = 19.5;
const CLEAR_Z: f64 = 25.0;

/// A faced block with a fixture wall where the BT30 flange passes
/// (22.5 mm off the 4 mm tool's outermost pass, 90-110 above the tip):
/// the BT30 ER20 flange strikes it. A second tool faces the block with
/// its plain holder cylinder.
fn walled_world() -> (World, History) {
    let world = new_world();
    let mut history = History::default();
    history.execute(
        &world,
        SetStock::new(&world, StockShape::Box { dx: 60.0, dy: 60.0, dz: 20.0 }, Location::default()).expect("stock"),
    );
    for name in [TOOL, BARE] {
        history.execute(&world, AddTool::new(&world, name, catalog_tool(name)).expect("tool"));
    }
    for (op, tool) in [("Face1", TOOL), ("Face2", BARE)] {
        let params = OpParams::Face {
            holes: Default::default(),
            tilt: None,
            blend_turns: false, blend_radius: None,
            stepover: 3.0,
            clearance_z: Height::fixed(CLEAR_Z),
            cut_z: Height::fixed(CUT_Z),
            top_z: None,
            stepdown: None,
            feed: 1200.0,
            plunge_feed: 300.0,
            climb: true,
            picks: Default::default(),
        };
        history.execute(&world, CreateOperation::new(&world, op, tool, SrcRef::Stock, params).expect("face op"));
        history.execute(&world, GenerateOperation::new(&world, op).expect("generate"));
    }
    let cache = model::toolpath_of(&world, &model::find_by_name(&world, "Face1").expect("Face1")).expect("generated");
    let xs = cache.path.moves.iter().flat_map(|m| [m.from.x, m.to.x]);
    let ys = || cache.path.moves.iter().flat_map(|m| [m.from.y, m.to.y]);
    let x = xs.fold(f64::NEG_INFINITY, f64::max) + 22.5;
    let (y0, y1) = (ys().fold(f64::INFINITY, f64::min), ys().fold(f64::NEG_INFINITY, f64::max));
    let (zb, zt) = (CUT_Z + 95.0, CUT_Z + 105.0);
    let (a, b, c, d) = (Point3::new(x, y0, zb), Point3::new(x, y1, zb), Point3::new(x, y1, zt), Point3::new(x, y0, zt));
    let e = model::spawn(&world, "Wall1");
    model::set_facets(&world, &e, vec![Triangle::new(a, b, c), Triangle::new(a, c, d)].into());
    model::set_rev(&world, &e, 1);
    history.execute(&world, TagFixture::new(&world, "Wall1", None).expect("tagged"));
    history.execute(&world, SetToolBody::library(&world, TOOL, "BT30 ER20", None, None).expect("pick"));
    regenerate(&world, &mut history);
    (world, history)
}

fn regenerate(world: &World, history: &mut History) {
    for op in ["Face1", "Face2"] {
        history.execute(world, GenerateOperation::new(world, op).expect("regenerate"));
    }
}

fn warnings(world: &World, op: &str) -> Vec<Warning> {
    let e = model::find_by_name(world, op).expect("op");
    model::toolpath_of(world, &e).expect("generated").warnings.clone()
}

fn holder_hits(world: &World, op: &str) -> usize {
    warnings(world, op).iter().filter(|w| matches!(w, Warning::FixtureHit { fixture_part: FixturePart::Holder, .. })).count()
}

fn statuses(world: &World) -> Vec<OpStatus> {
    ["Face1", "Face2"].iter().map(|op| status_of(world, &model::find_by_name(world, op).expect("op"))).collect()
}

/// Rewrite every tool of a saved project (or library) the way a build
/// before #2016 wrote it: the library holder as a drawn copy under
/// `body`, no `holder` key.
fn as_older_build(v: &mut Value) {
    match v {
        Value::Object(m) => {
            if let Some(h) = m.remove("holder") {
                let e = holder_library::find(h["entry"].as_str().expect("entry")).expect("known");
                let copy = old_copy(&e, h["gauge_len"].as_f64().expect("gauge"), h["projection"].as_f64().expect("projection"));
                m.insert("body".into(), serde_json::to_value(ToolBodyDto::from_body(&copy)).expect("dto"));
            }
            m.values_mut().for_each(as_older_build);
        }
        Value::Array(a) => a.iter_mut().for_each(as_older_build),
        _ => {}
    }
}

#[test]
fn an_older_project_loads_as_custom_holders_with_the_same_collision_result() {
    let (world, _history) = walled_world();
    assert!(holder_hits(&world, "Face1") > 0, "the referenced BT30 flange strikes the wall");
    let text = project::save_text(&world).expect("save");
    let reloaded = new_world();
    project::open(&reloaded, &text).expect("load");

    // The same document as v0.21.0 (project v61) and pre-#2016 main (v69) wrote it.
    for old_version in [61_u32, 69] {
        let mut file: Value = serde_json::from_str(&text).expect("json");
        as_older_build(&mut file);
        file["version"] = json!(old_version);
        let old_text = serde_json::to_string_pretty(&file).expect("json");
        assert!(!old_text.contains("\"holder\":") && old_text.contains("\"spun\":"), "a drawn copy, no reference");
        let older = new_world();
        project::open(&older, &old_text).expect("an older file loads");

        let copy = body(&older, TOOL).expect("the drawn copy");
        assert_eq!(copy.library, None, "v{old_version}: an older file's holder is a custom (drawn) holder");
        assert_eq!(copy.spun, body(&world, TOOL).and_then(|b| b.spun), "v{old_version}: with the same outline");
        assert_eq!(body(&older, BARE), None, "v{old_version}: a holder cylinder stays the tool's own numbers");
        for tool in [TOOL, BARE] {
            assert_eq!(reads(&older, tool), reads(&world, tool), "v{old_version} {tool}: judged, stack, stamp and facets unchanged");
        }
        assert_eq!(statuses(&older), statuses(&reloaded), "v{old_version}: no operation goes stale over the file's form");
        let mut h = History::default();
        regenerate(&older, &mut h);
        for op in ["Face1", "Face2"] {
            assert_eq!(warnings(&older, op), warnings(&world, op), "v{old_version} {op}: every warning, hit for hit");
        }
        // It stays a copy: saving never turns it into a reference, and migrates forward to current version.
        let resaved = project::save_text(&older).expect("save");
        assert!(!resaved.contains("\"holder\":") && resaved.contains("\"spun\":"));
        let resaved_json: Value = serde_json::from_str(&resaved).expect("json");
        assert_eq!(resaved_json["version"], json!(project::PROJECT_VERSION));
    }
}

#[test]
fn an_older_tool_library_loads_as_custom_holders_and_a_new_one_round_trips_the_reference() {
    let (world, _history) = walled_world();
    let text = export_tool_library(&world).expect("export");
    let file: Value = serde_json::from_str(&text).expect("json");
    const { assert!(HOLDER_REF_VERSION > CUTTING_STEPS_VERSION, "v0.22.0 (v2) and v3 readers must refuse holder references"); }
    assert_eq!(file["version"], json!(HOLDER_REF_VERSION), "a reader that would drop the reference must refuse the file");
    let mut future = file.clone();
    // #3904, #3368: the newest version this build reads is TIP_FORM_VERSION.
    const { assert!(TIP_FORM_VERSION > CUTTER_VERSION) };
    future["version"] = json!(TIP_FORM_VERSION + 1);
    let err = parse_tool_library(&future.to_string()).expect_err("future library version refused");
    assert!(err.contains("version"), "{err}");
    let def = file["tools"].as_array().expect("tools").iter().find(|t| t["name"] == TOOL).expect("tool")["definition"].clone();
    assert_eq!(def["holder"], json!({ "entry": "BT30 ER20", "gauge_len": 60.0, "projection": 52.0 }));
    assert!(def.get("body").is_none(), "{def}");
    let lib = parse_tool_library(&text).expect("parse");
    let held = lib.iter().find(|t| t.name == TOOL).and_then(|t| t.body.clone()).expect("held");
    assert_eq!(Some(held), body(&world, TOOL), "the reference arrives as the same body");

    // The same library as an older build wrote it (version 1, a copy).
    let mut old = file.clone();
    as_older_build(&mut old);
    old["version"] = json!(1);
    let lib = parse_tool_library(&serde_json::to_string(&old).expect("json")).expect("an older library loads");
    let copy = lib.iter().find(|t| t.name == TOOL).and_then(|t| t.body.clone()).expect("copy");
    assert_eq!(copy.library, None);
    assert_eq!(copy.profile(), body(&world, TOOL).as_ref().and_then(ToolBody::profile), "the same bands");
    assert_eq!(copy.facets, body(&world, TOOL).expect("body").facets);
    assert!(lib.iter().find(|t| t.name == BARE).is_some_and(|t| t.body.is_none()));

    // A library with no reference keeps its old version.
    let (bare, mut h) = (new_world(), History::default());
    h.execute(&bare, AddTool::new(&bare, BARE, catalog_tool(BARE)).expect("tool"));
    assert!(export_tool_library(&bare).expect("export").contains("\"version\": 1"));
}

/// One corruption of the tool's record.
type Edit<'a> = &'a dyn Fn(&mut Value);

fn with_tool(text: &str, f: impl Fn(&mut Value)) -> String {
    let mut v: Value = serde_json::from_str(text).expect("json");
    let list = if v.get("entities").is_some() { "entities" } else { "tools" };
    let key = if list == "entities" { "tool" } else { "definition" };
    let t = v[list].as_array_mut().expect("list").iter_mut().find(|t| t["name"] == TOOL).expect("tool");
    f(&mut t[key]);
    serde_json::to_string(&v).expect("json")
}

#[test]
fn an_unknown_entry_refuses_the_file_by_name_and_never_lands_another_holder() {
    let (world, _history) = walled_world();
    let project_text = project::save_text(&world).expect("save");
    let library_text = export_tool_library(&world).expect("export");
    let cases: [(Edit, &str); 6] = [
        (&|t| t["holder"]["entry"] = json!("BT35 ER20"), "there is no holder 'BT35 ER20' in the holder library"),
        // A grouped pick is no entry: the file names one bore.
        (&|t| t["holder"]["entry"] = json!("BT30 Weldon"), "there is no holder 'BT30 Weldon' in the holder library"),
        (&|t| t["holder"]["gauge_len"] = json!(5.0), "gauge length is shorter"),
        (&|t| t["holder"]["projection"] = json!(5.0), "reaches into the flutes"),
        (&|t| t["body"] = json!({ "source": "x", "facets": [] }), "both a library holder"),
        (&|t| t["holder"]["nut_d"] = json!(1.0), "nut_d"),
    ];
    for (edit, says) in cases {
        let fresh = new_world();
        let err = project::open(&fresh, &with_tool(&project_text, edit)).expect_err("refused").to_string();
        assert!(err.contains(says), "project: {err:?} should say {says:?}");
        assert!(model::find_by_name(&fresh, TOOL).is_none(), "nothing of a refused file lands");
        let err = parse_tool_library(&with_tool(&library_text, edit)).expect_err("refused");
        assert!(err.contains(says), "library: {err:?} should say {says:?}");
        if !says.contains("nut_d") {
            assert!(err.contains(&format!("tool '{TOOL}'")), "by tool name: {err:?}");
        }
    }
}

#[test]
fn the_unchanged_outline_keeps_the_reference_and_an_edited_point_makes_it_custom() {
    let (world, mut history) = (new_world(), History::default());
    history.execute(&world, AddTool::new(&world, TOOL, catalog_tool(TOOL)).expect("tool"));
    history.execute(&world, SetToolBody::library(&world, TOOL, "BT30 ER20", None, None).expect("pick"));
    let landed = body(&world, TOOL).expect("landed");
    let points: Vec<SpunPoint> = landed.spun.as_ref().expect("drawn").points().to_vec();
    history.execute(&world, SetToolBody::spun(&world, TOOL, Some(points.clone())).expect("apply unchanged"));
    assert_eq!(body(&world, TOOL), Some(landed.clone()), "the same holder, still by reference");
    let mut edited = points;
    edited[1].r += 1.0;
    edited[2].r += 1.0;
    history.execute(&world, SetToolBody::spun(&world, TOOL, Some(edited)).expect("apply edited"));
    let custom = body(&world, TOOL).expect("custom");
    assert_eq!(custom.library, None, "an edited outline is a custom holder");
    assert!(project::save_text(&world).expect("save").contains("\"spun\":"), "written as its own outline");
    assert!(history.undo(&world));
    assert_eq!(body(&world, TOOL), Some(landed), "undo: the library holder again");
}

fn call(s: &mut Server, id: i64, name: &str, args: Value) -> (bool, Value) {
    let req = json!({ "jsonrpc": "2.0", "id": id, "method": "tools/call", "params": { "name": name, "arguments": args } });
    let resp = s.handle(&req).expect("a response");
    let r = &resp["result"];
    let text = r["content"][0]["text"].as_str().unwrap_or_default();
    (r["isError"].as_bool().unwrap_or(false), serde_json::from_str(text).unwrap_or_else(|_| Value::String(text.to_owned())))
}

#[test]
fn mcp_set_tool_holder_writes_the_reference() {
    let mut s = Server::new();
    let (err, r) = call(&mut s, 1, "add_tool", json!({ "catalog": TOOL, "holder": "none" }));
    assert!(!err, "{r}");
    let (err, r) = call(&mut s, 2, "set_tool_holder", json!({ "tool": TOOL, "library": "BT30 ER20", "gauge_len": 70 }));
    assert!(!err, "{r}");
    assert_eq!(r["holder"]["library"], json!({ "entry": "BT30 ER20", "gauge_len": 70.0, "projection": 52.0 }), "{r}");
    let (_, job) = call(&mut s, 3, "get_job", json!({}));
    let row = job["tools"].as_array().expect("tools").iter().find(|t| t["name"] == TOOL).expect("row").clone();
    assert_eq!(row["holder"]["library"]["entry"], json!("BT30 ER20"), "{row}");
    // A drawn profile is a custom holder: no reference.
    let pts = json!([{ "z": 52, "r": 0 }, { "z": 52, "r": 20 }, { "z": 90, "r": 20 }, { "z": 90, "r": 0 }]);
    let (err, r) = call(&mut s, 4, "set_tool_holder", json!({ "tool": TOOL, "points": pts }));
    assert!(!err, "{r}");
    assert_eq!(r["holder"]["library"], Value::Null, "{r}");
    // A definition's own reference is refused, never dropped.
    let mut def = project::tool_to_json(&catalog_tool(BARE));
    def["holder"] = json!({ "entry": "BT30 ER20", "gauge_len": 60.0, "projection": 60.0 });
    let (err, r) = call(&mut s, 5, "add_tool", json!({ "name": "T9", "definition": def }));
    assert!(err && r.to_string().contains("\\\"holder\\\" reference is not read by add_tool"), "{r}");
    let (err, r) = call(&mut s, 6, "edit_tool", json!({ "name": TOOL, "patch": { "holder": { "entry": "BT40 ER20" } } }));
    assert!(err && r.to_string().contains("not edited by edit_tool"), "{r}");
}
