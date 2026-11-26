//! #1501: the shipped Sodick ALC400G wire EDM machine file. Pins the
//! published figures, its place in the library and the Sodick post's
//! offer for it, and a punch posted through that post on this machine.
//! Provenance: our own tests over our own machine file; the figures are
//! Sodick's published ALC series specifications (X/Y/Z 400 x 300 x 250 mm,
//! U/V 150 x 150 mm, taper +-25 deg at 130 mm: sodick.co.th/EN/EDW.php for
//! the taper; the sodick.eu page and the ALC series brochure for the travels);
//! see docs/decisions/1501-sodick-alc400g.md.
//! Gate map (#1501): covers app::posting (MACHINES, WIRE_POSTS,
//! post_script), app::machine (parse_mch, library), sodick-wire.cps.

use camcraft_app::commands::{CreateOperation, GenerateOperation, History, ImportModel, LoadMachine};
use camcraft_app::geometry::{Mesh, Point3, Triangle};
use camcraft_app::machine::{library, parse_mch, Kind};
use camcraft_app::operation::{self, Height, OpParams, SrcRef, WireContourOp, WireSide};
use camcraft_app::posting;
use camcraft_app::world::new_world;

const NAME: &str = "Sodick ALC400G";
const SODICK: &str = "Sodick Wire EDM (Mark 21)";

fn text() -> &'static str {
    camcraft_app::posting::builtin_machine_text(NAME).expect("the ALC400G is shipped")
}

#[test]
fn the_machine_file_states_only_the_published_figures() {
    parse_mch(text()).expect("the file parses");
    // X 400, Y 300, U and V 150 mm of travel (stated centred on zero).
    for (axis, span) in [("X", 400.0), ("Y", 300.0), ("U", 150.0), ("V", 150.0)] {
        let raw: serde_json::Value = serde_json::from_str(text()).expect("json");
        let hit = find_axis(&raw["kinematics"]["default"]["parts"], axis).unwrap_or_else(|| panic!("{axis}"));
        let (lo, hi) = (hit["min"].as_f64().expect("min"), hit["max"].as_f64().expect("max"));
        assert!((hi - lo - span).abs() < 1e-9, "{axis}: {lo}..{hi}");
    }
    let raw: serde_json::Value = serde_json::from_str(text()).expect("json");
    assert_eq!(raw["edm"]["max_taper_deg"].as_f64(), Some(25.0));
    // What no public source gives and no first cut needs is left out.
    for key in ["wire_diameters", "codes"] {
        assert!(raw["edm"].get(key).is_none(), "edm.{key} has no public source");
    }
}

/// #3214: the file ships a starter set so that a first Wire Contour can
/// generate and post. Each value is marked as an example to replace.
#[test]
fn the_shipped_starter_set_is_marked_as_examples_to_replace() {
    let raw: serde_json::Value = serde_json::from_str(text()).expect("json");
    let edm = &raw["edm"];
    let conds = edm["conditions"].as_array().expect("edm.conditions is stated");
    assert!(conds.len() >= 2, "{conds:?}");
    for c in conds {
        let id = c["id"].as_str().expect("id");
        // The Sodick post reads C and three digits only.
        assert!(id.len() == 4 && id.starts_with('C') && id[1..].chars().all(|d| d.is_ascii_digit()), "{id}");
        let notes = c["notes"].as_str().unwrap_or_default();
        assert!(notes.contains("EXAMPLE") && notes.contains("replace"), "{id}: {notes}");
        assert!(c["gap"].as_f64().is_some() && c["wire_diameter"].as_f64().is_some(), "{id}");
    }
    let roles: Vec<&str> = conds.iter().filter_map(|c| c["role"].as_str()).collect();
    assert!(roles.contains(&"rough") && roles.contains(&"skim"), "{roles:?}");
    assert!(edm["lower_guide_z"].as_f64().is_some() && edm["upper_guide_z"].as_f64().is_some());
    assert_eq!(edm["awt"].as_bool(), Some(true));
    let about = raw["general"]["description"].as_str().expect("description");
    assert!(about.contains("EXAMPLE") && about.contains("replace"), "{about}");
}

fn find_axis<'a>(parts: &'a serde_json::Value, id: &str) -> Option<&'a serde_json::Value> {
    for p in parts.as_array()? {
        if p["name"] == id {
            return Some(&p["travel"]);
        }
        if let Some(t) = find_axis(&p["parts"], id) {
            return Some(t);
        }
    }
    None
}

#[test]
fn the_library_lists_it_and_offers_the_sodick_post() {
    let e = library::builtins().into_iter().find(|e| e.name == NAME).expect("listed");
    assert!(e.is_valid(), "{:?}", e.refusals);
    assert_eq!(e.kind, Some(Kind::WireEdm));
    assert!(e.posts.contains(&SODICK), "{:?}", e.posts);
}

fn slug() -> Mesh {
    let pts = [(20.0, 10.0), (-20.0, 10.0), (-20.0, -10.0), (20.0, -10.0)];
    let p = |(x, y): (f64, f64), z: f64| Point3::new(x, y, z);
    let mut f = Vec::new();
    for i in 0..pts.len() {
        let (a, b) = (pts[i], pts[(i + 1) % pts.len()]);
        f.push(Triangle::new(Point3::new(0.0, 0.0, 12.0), p(a, 12.0), p(b, 12.0)));
        f.push(Triangle::new(Point3::new(0.0, 0.0, 0.0), p(b, 0.0), p(a, 0.0)));
        f.push(Triangle::new(p(a, 0.0), p(b, 0.0), p(b, 12.0)));
        f.push(Triangle::new(p(a, 0.0), p(b, 12.0), p(a, 12.0)));
    }
    Mesh::from_facets(f)
}

/// The world with `machine_text` mounted and a punch contour under
/// `condition`; the op's creation error as text when the machine refuses it.
fn punch_world(machine_text: &str, condition: &str, taper_deg: f64) -> Result<flecs_ecs::prelude::World, String> {
    let world = new_world();
    let mut h = History::default();
    let m = parse_mch(machine_text).expect("parses");
    h.execute(&world, LoadMachine::new(&world, "alc400g.mch".into(), Some(m)).expect("mount"));
    h.execute(
        &world,
        camcraft_app::commands::AddTool::new(&world, "EDM-W025", camcraft_app::wire_tools::wire_tool(0.25))
            .expect("wire"),
    );
    h.execute(&world, ImportModel::from_mesh("slug", slug()));
    let op = WireContourOp {
        lead_arc_radius: None,
        top_z: Height::fixed(12.0),
        bottom_z: Height::fixed(0.0),
        condition: condition.into(),
        side: WireSide::Punch,
        picks: camcraft_app::path_picks::PathPicks { start: vec![camcraft_app::path_picks::PathPick::xy(camcraft_app::geo2d::Point2::new(-30.0, 0.0))], exit: Vec::new() },
        tabs: None,
        capture: true,
        both_pieces_restrained: false,
        stock_centered_in_xy_travel: false,
        taper_deg,
        four_axis: None,
        burnout: None,
        clearance: operation::DEFAULT_CLEARANCE,
        skims: Vec::new(),
        sharp_corners: false,
    };
    let c = CreateOperation::new(&world, "Punch1", "EDM-W025", SrcRef::Entity("slug".into()), OpParams::WireContour(op)).map_err(|e| format!("{e:?}"))?;
    h.execute(&world, c);
    h.execute(&world, GenerateOperation::new(&world, "Punch1").expect("generates"));
    Ok(world)
}

fn post_sodick(world: &flecs_ecs::prelude::World, tag: &str) -> Result<String, String> {
    let cps = camcraft_app::posting::builtin_post_text(SODICK).expect("registered");
    let out = std::env::temp_dir().join(format!("camcraft-alc400g-{tag}-{}.nc", std::process::id()));
    let posted = posting::post_script(world, &out, cps);
    let _ = std::fs::remove_file(&out);
    posted.map(|p| p.program).map_err(|e| format!("{e:?}"))
}

/// #3214: the shipped file as it is - no edit by the user - lets a first
/// Wire Contour generate and post, on an example condition.
#[test]
fn a_first_wire_contour_generates_and_posts_on_the_shipped_file() {
    let world = punch_world(text(), "C110", 0.0).expect("the shipped example condition admits the punch");
    let text = post_sodick(&world, "shipped").expect("the punch posts");
    let has = |w: &str| text.lines().any(|l| l == w);
    assert!(has("G21") && has("C110") && has("M02"), "{text}");
    // The file states AWT, so the post threads and cuts the wire.
    assert!(has("T91") && has("T90"), "{text}");
    // The kerf offset used the example row's gap: the program says so,
    // so a user who did not edit the file cannot cut it unwarned.
    assert!(
        has("(WARNING CONDITION C110 IS AN EXAMPLE ROW - THE KERF USES AN EXAMPLE GAP OF 0.040 MM - REPLACE IT BEFORE CUTTING)"),
        "{text}"
    );
}

/// The guide's procedure: the user replaces the example table with the
/// rows of their own control. Their condition posts; an example id that
/// is gone from the table is refused by name.
#[test]
fn a_punch_posts_through_the_sodick_post_on_it() {
    let mut raw: serde_json::Value = serde_json::from_str(text()).expect("json");
    raw["edm"]["conditions"] = serde_json::json!([
        { "id": "C210", "name": "test row", "gap": 0.04, "wire_diameter": 0.25, "role": "rough", "notes": "TEST ONLY - not a Sodick figure" }
    ]);
    let world = punch_world(&raw.to_string(), "C210", 0.0).expect("the user's condition lets the punch generate");
    let text = post_sodick(&world, "own").expect("the punch posts");
    let has = |w: &str| text.lines().any(|l| l == w);
    assert!(has("G21") && has("C210") && has("M02"), "{text}");
    // The user's own row: its gap is stated to compare with the control,
    // and no example warning is written.
    assert!(has("(CONDITION C210 KERF GAP 0.040 MM - COMPARE WITH THE CONTROL ROW)"), "{text}");
    assert!(!text.contains("EXAMPLE"), "{text}");
    let e = punch_world(&raw.to_string(), "C110", 0.0).expect_err("the example row was replaced");
    assert!(e.contains("C110"), "{e}");
}

/// Refs #1800: the file states a taper limit (`edm.max_taper_deg`) but no
/// guide heights. Since #1818 the command door (`wire_edm::taper_door`)
/// refuses a tapered cut by name; the post keeps its own refusal as
/// defence in depth (`wire_edm.rs`: no guide heights, no taper limit, or a
/// lean over the limit). The Sodick post also writes a vertical 2-axis cut
/// only (#1800). This pins the two guide-height refusals (#1856).
/// #3214: the shipped file now states example guide heights, so the door
/// admits a taper inside the limit and the Sodick post's own 2-axis
/// refusal still blocks it; a file with the guide heights taken out keeps
/// the two guide-height refusals.
#[test]
fn a_tapered_punch_is_refused_at_post_while_the_file_states_no_guide_heights() {
    const NO_GUIDES: &str = "states no edm.lower_guide_z and edm.upper_guide_z";
    // The shipped file: the door admits 5 deg, the Sodick post refuses it.
    let world = punch_world(text(), "C110", 5.0).expect("the example guide heights admit the taper");
    let err = post_sodick(&world, "taper-shipped").expect_err("the Sodick post writes a vertical cut only");
    assert!(err.contains("is tapered") && err.contains("vertical 2-axis cut only"), "{err}");
    let mut raw: serde_json::Value = serde_json::from_str(text()).expect("json");
    let edm = raw["edm"].as_object_mut().expect("edm");
    edm.remove("lower_guide_z");
    edm.remove("upper_guide_z");
    let unguided = raw.to_string();
    // The door: 5 deg is inside the 25 deg limit, but no guide heights are stated.
    let e = punch_world(&unguided, "C110", 5.0).expect_err("the door refuses a taper on this file");
    assert!(e.contains(NO_GUIDES) && e.contains("set the taper to 0"), "{e}");
    // The post: the op is made on the shipped file (guide heights stated),
    // then the file without them is mounted. The post must refuse.
    let mut h = History::default();
    let m = parse_mch(&unguided).expect("parses");
    h.execute(&world, LoadMachine::new(&world, "alc400g.mch".into(), Some(m)).expect("remount"));
    let err = post_sodick(&world, "taper-unguided").expect_err("a taper cannot post on this file");
    assert!(err.contains(NO_GUIDES), "{err}");
}
