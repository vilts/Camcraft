//! M3.2 stage D acceptance: the setup sheet is documentation generated
//! from the CAM data itself - deterministic to the byte, program-ordered,
//! names the stock and offsets, resolves depths through the same
//! machinery the stamp uses, and degrades to honest "(none)" lines on a
//! half-built document instead of erroring or guessing.
//!
//! Helpers are duplicated from `tests/setups.rs` (cross-test-file imports
//! are not the idiom here).
//!
//! Gate map (M4.4): covers app::commands, app::geometry, app::operation, app::schema, app::setup_sheet, geometry::geometry::stock, tools::tools, app::world.

use flecs_ecs::prelude::World;

use camcraft_app::commands::{
    ActivateSetup, AddTool, CommandError, CreateOperation, CreateSetup, GenerateOperation, History,
    ReorderOperation, SetFixtureFixed, SetStock, SetWcsOrigin, TagFixture,
};
use camcraft_app::geometry::{IDENTITY_Q, Point3, Triangle};
use camcraft_app::machine::{self, AxisLimits, Machine};
use camcraft_app::model;
use camcraft_app::operation::{Height, HeightReference, OpParams, SrcRef};
use camcraft_app::schema::Location;
use camcraft_app::setup_sheet;
use camcraft_app::stock::StockShape;
use camcraft_app::tools::{catalog, Tool};
use camcraft_app::world::new_world;

fn catalog_tool(name: &str) -> Tool {
    catalog()
        .into_iter()
        .find(|(n, _)| *n == name)
        .map(|(_, t)| t)
        .expect("catalog entry")
}

fn at(x: f64, y: f64, z: f64) -> Location {
    Location {
        pos: Point3::new(x, y, z),
        rot: IDENTITY_Q,
    }
}

/// Face parameters for the 60x60x20 box stock with a 4 mm cutter - the
/// same fixture the operations tests face on.
fn face_params() -> OpParams {
    OpParams::Face { holes: Default::default(), tilt: None, blend_turns: false,
        stepover: 3.0,
        clearance_z: Height::fixed(25.0),
        cut_z: Height::fixed(19.5),
        top_z: None,
        stepdown: None,
        feed: 1200.0,
        plunge_feed: 300.0,
        climb: true,
    }
}

fn box_stock(world: &World, history: &mut History, dx: f64, dy: f64, dz: f64) {
    let cmd = SetStock::new(world, StockShape::Box { dx, dy, dz }, Location::default());
    history.execute(world, cmd.expect("box stock"));
}

fn add_tool(world: &World, history: &mut History, name: &str) {
    let cmd = AddTool::new(world, name, catalog_tool(name));
    history.execute(world, cmd.expect("catalog tool"));
}

fn create_face(world: &World, history: &mut History, name: &str, cut_z: f64) {
    let mut params = face_params();
    if let OpParams::Face { cut_z: c, .. } = &mut params {
        *c = Height::fixed(cut_z);
    }
    let cmd = CreateOperation::new(world, name, "EM-0400-2FL", SrcRef::Stock, params);
    history.execute(world, cmd.expect("create face op"));
    let cmd = GenerateOperation::new(world, name).expect("generate face op");
    history.execute(world, cmd);
}

fn run(
    world: &World,
    history: &mut History,
    cmd: Result<impl camcraft_app::commands::Command + 'static, CommandError>,
    what: &str,
) {
    history.execute(world, cmd.unwrap_or_else(|e| panic!("{what}: {e:?}")));
}

/// Stock, tool, one materialized setup and one private one, the private
/// stock set to the same 60x60x20 box at the same spot: the pair every
/// sheet law is judged against (from `tests/setups.rs`).
fn two_setup_world() -> (World, History) {
    let world = new_world();
    let mut history = History::default();
    box_stock(&world, &mut history, 60.0, 60.0, 20.0);
    add_tool(&world, &mut history, "EM-0400-2FL");
    run(
        &world,
        &mut history,
        CreateSetup::new(&world, None, None),
        "materialize",
    );
    run(
        &world,
        &mut history,
        CreateSetup::new(&world, None, None),
        "second setup",
    );
    box_stock(&world, &mut history, 60.0, 60.0, 20.0);
    (world, history)
}

/// True when no numeric token anywhere in the text is scientific
/// notation: a token that starts with a digit (after sign/bang trim)
/// may only continue with digits and dots. The `2D`/`3D` operation-type
/// prefixes are exempt - they are kind names, not floats.
fn plain_floats_only(text: &str) -> bool {
    text.split(|c: char| c.is_whitespace() || c == '(' || c == ')')
        .map(|tok| tok.trim_start_matches(['-', '+', '!']))
        .all(|t| {
            t.is_empty()
                || !t.starts_with(|c: char| c.is_ascii_digit())
                || (t.len() == 2 && matches!(t.as_bytes()[0], b'2' | b'3') && t.as_bytes()[1] == b'D')
                || t.chars().all(|c| c.is_ascii_digit() || c == '.')
        })
}

#[test]
fn sheet_is_deterministic_byte_identical_twice() {
    let (world, mut history) = two_setup_world();
    run(
        &world,
        &mut history,
        SetWcsOrigin::new(&world, at(10.0, 0.0, 5.0)),
        "place offset",
    ); // lands in Setup2 (active)
    run(
        &world,
        &mut history,
        ActivateSetup::new(&world, Some("Setup1")),
        "Setup1 active",
    );
    create_face(&world, &mut history, "F1", 19.5);
    run(
        &world,
        &mut history,
        ActivateSetup::new(&world, Some("Setup2")),
        "Setup2 active",
    );
    create_face(&world, &mut history, "F2", 19.5);

    let once = setup_sheet::render(&world, Some("Setup2")).expect("Setup2 sheet");
    let twice = setup_sheet::render(&world, Some("Setup2")).expect("Setup2 sheet again");
    assert_eq!(once, twice, "byte-identical renders are the law");
    let none_once = setup_sheet::render(&world, None).expect("first-setup sheet");
    let none_twice = setup_sheet::render(&world, None).expect("first-setup sheet again");
    assert_eq!(none_once, none_twice);
    assert!(once.contains("F2") && none_once.contains("F1"));

    let all_once = setup_sheet::render_all(&world);
    let all_twice = setup_sheet::render_all(&world);
    assert_eq!(all_once, all_twice, "the flip chart renders twice the same");
    assert_eq!(
        all_once.iter().map(|(n, _)| n.as_str()).collect::<Vec<_>>(),
        vec!["Setup1", "Setup2"]
    );
    assert_eq!(
        setup_sheet::simulate_verdict(&world),
        setup_sheet::simulate_verdict(&world),
        "the sim verdict is deterministic too"
    );
}

#[test]
fn sheet_names_stock_wcs_and_op_depths() {
    let (world, mut history) = two_setup_world();
    run(
        &world,
        &mut history,
        SetWcsOrigin::new(&world, at(12.5, -3.0, 4.0)),
        "place G54 in Setup2",
    );
    create_face(&world, &mut history, "Face2", 19.5);

    let sheet = setup_sheet::render(&world, Some("Setup2")).expect("Setup2 sheet");
    assert!(sheet.contains("SETUP SHEET - Setup2"), "{sheet}");
    assert!(sheet.contains("G54"), "the slot prints its G-word:\n{sheet}");
    assert!(
        sheet.contains("X 12.500") && sheet.contains("Y -3.000") && sheet.contains("Z 4.000"),
        "the origin follows the entity transform:\n{sheet}"
    );
    assert!(
        sheet.contains("X 60.000") && sheet.contains("Y 60.000") && sheet.contains("Z 20.000"),
        "the stock box dims are on the sheet:\n{sheet}"
    );
    assert!(sheet.contains("19.500"), "the resolved cut Z:\n{sheet}");
    assert!(sheet.contains("Face2") && sheet.contains("EM-0400-2FL"));
}

#[test]
fn implicit_document_sheet_labels_it_so() {
    let world = new_world();
    let mut history = History::default();
    box_stock(&world, &mut history, 60.0, 60.0, 20.0);
    add_tool(&world, &mut history, "EM-0400-2FL");
    create_face(&world, &mut history, "Face1", 19.5);

    let sheet = setup_sheet::render(&world, None).expect("implicit sheet");
    assert!(
        sheet.contains("SETUP SHEET - Implicit"),
        "no setups means the sheet says so:\n{sheet}"
    );
    assert!(sheet.contains("Face1"), "the implicit op rides in:\n{sheet}");
}

#[test]
fn sheet_orders_ops_by_the_program_law() {
    // Created B then A, then A moved ahead of B: the program law (setup
    // rank, then position - #167) puts A first - the sheet follows the
    // post's order, not history.
    let world = new_world();
    let mut history = History::default();
    box_stock(&world, &mut history, 60.0, 60.0, 20.0);
    add_tool(&world, &mut history, "EM-0400-2FL");
    create_face(&world, &mut history, "B_Face", 19.5);
    create_face(&world, &mut history, "A_Face", 19.5);
    let before = setup_sheet::render(&world, None).expect("sheet");
    assert!(
        before.find("B_Face") < before.find("A_Face"),
        "created B then A: B runs first (#167):\n{before}"
    );
    history.execute(&world, ReorderOperation::new(&world, "A_Face", "B_Face", false).expect("reorder"));

    let sheet = setup_sheet::render(&world, None).expect("sheet");
    let a = sheet.find("A_Face").expect("A on the sheet");
    let b = sheet.find("B_Face").expect("B on the sheet");
    assert!(a < b, "program order, not creation order:\n{sheet}");
}

#[test]
fn empty_world_renders_header_only_not_error() {
    let world = new_world();
    let sheet = setup_sheet::render(&world, None).expect("empty world must render, not error");
    assert!(sheet.contains("SETUP SHEET - Implicit"));
    assert!(sheet.contains("(none)"), "sections degrade visibly:\n{sheet}");
    assert!(sheet.contains("no operations in this setup"));
    assert!(sheet.contains("END OF SHEET"));
    // The one thing that IS an error: a name that wears no setup.
    assert!(
        setup_sheet::render(&world, Some("Nope")).is_err(),
        "a declared setup that does not exist is refused"
    );
}

/// A plain model with one facet spanning `min..max`, the way an import
/// leaves it - the solid a fixture tag rides on (from `tests/fixtures.rs`).
fn add_block(world: &World, name: &str, min: Point3, max: Point3) {
    let entity = model::spawn(world, name);
    let tri = Triangle::new(min, Point3::new(max.x, max.y, max.z), Point3::new(min.x, max.y, max.z));
    model::set_facets(world, &entity, vec![tri].into());
    model::set_rev(world, &entity, 1);
}

#[test]
fn sheet_prints_each_fixtures_workholding_frame() {
    // M13.8's workholding law, printed: the sheet must say which frame
    // the verifier judged every piece of steel in - a fixed fixture
    // stays in the machine frame while the table turns, the default
    // turns with the part, and the machine table never turns.
    let world = new_world();
    let mut history = History::default();
    box_stock(&world, &mut history, 60.0, 60.0, 20.0);
    add_block(&world, "Chuck1", Point3::new(-10.0, -10.0, -30.0), Point3::new(10.0, 10.0, 0.0));
    add_block(&world, "Tailstock1", Point3::new(80.0, -5.0, -5.0), Point3::new(100.0, 5.0, 5.0));
    run(&world, &mut history, TagFixture::new(&world, "Chuck1", None), "tag the chuck");
    run(&world, &mut history, TagFixture::new(&world, "Tailstock1", None), "tag the tailstock");
    run(
        &world,
        &mut history,
        SetFixtureFixed::new(&world, "Tailstock1", true),
        "the tailstock stands still",
    );
    let axis = |n: &str| AxisLimits {
        name: n.to_string(),
        travel: Some((-100.0, 100.0)),
        ..Default::default()
    };
    machine::set(
        &world,
        "vmc.mch",
        Some(&Machine {
            model: "TestBench VMC".to_string(),
            axes: vec![axis("X"), axis("Y"), axis("Z")],
            ..Default::default()
        }),
    );

    let sheet = setup_sheet::render(&world, None).expect("sheet");
    let line_of = |name: &str| {
        sheet
            .lines()
            .find(|l| l.trim_start().starts_with(name))
            .unwrap_or_else(|| panic!("{name} has a fixture line:\n{sheet}"))
            .to_string()
    };
    let chuck = line_of("Chuck1");
    let tail = line_of("Tailstock1");
    let table = line_of(camcraft_app::fixture::MACHINE_TABLE);
    assert!(
        chuck.ends_with(setup_sheet::TURNING_WORDING) && chuck.contains("footprint 20.000 x 20.000"),
        "the default fixture turns with the part:\n{chuck}"
    );
    assert!(
        tail.ends_with(setup_sheet::FIXED_WORDING) && tail.contains("footprint 20.000 x 10.000"),
        "a fixed fixture says so, after its footprint:\n{tail}"
    );
    assert!(
        table.ends_with(setup_sheet::FIXED_WORDING),
        "the machine table never turns:\n{table}"
    );
    assert!(
        !chuck.contains(setup_sheet::FIXED_WORDING) && !tail.contains(setup_sheet::TURNING_WORDING),
        "each line carries exactly its own frame:\n{sheet}"
    );

    // Flipping the flag flips the words - and undo restores them.
    run(&world, &mut history, SetFixtureFixed::new(&world, "Chuck1", true), "fix the chuck");
    let flipped = setup_sheet::render(&world, None).expect("sheet");
    assert!(
        flipped.lines().any(|l| l.trim_start().starts_with("Chuck1") && l.ends_with(setup_sheet::FIXED_WORDING)),
        "the flag is read live:\n{flipped}"
    );
    assert!(history.undo(&world), "the flag write is one undo unit");
    assert_eq!(setup_sheet::render(&world, None).expect("sheet"), sheet, "undo restores the wording");
}

#[test]
fn every_float_uses_the_format_helper() {
    // Chosen path, documented: a NaN height is REFUSED at command
    // creation (`is_sane` runs on the expression, so a NaN never reaches
    // a params set the sheet could render) - so instead of feeding the
    // sheet a NaN, this feeds it a height that cannot resolve (a work
    // offset with no origin placed) and checks the visible refusal, and
    // separately checks the creation-time refusal of the NaN itself.
    let world = new_world();
    let mut history = History::default();
    box_stock(&world, &mut history, 60.0, 60.0, 20.0);
    add_tool(&world, &mut history, "EM-0400-2FL");

    assert!(
        CreateOperation::new(
            &world,
            "BadOp",
            "EM-0400-2FL",
            SrcRef::Stock,
            {
                let mut p = face_params();
                if let OpParams::Face { cut_z: c, .. } = &mut p {
                    *c = Height::fixed(f64::NAN);
                }
                p
            },
        )
        .is_err(),
        "a NaN height must be refused at command creation"
    );

    let mut params = face_params();
    if let OpParams::Face { cut_z: c, .. } = &mut params {
        *c = Height::of(HeightReference::WorkOffset, 0.0);
    }
    // Unresolvable at render time, sane at creation time - the command
    // gate checks structure, not resolution (depth_sane's rule).
    let cmd = CreateOperation::new(&world, "OddOp", "EM-0400-2FL", SrcRef::Stock, params);
    history.execute(&world, cmd.expect("unresolved height is constructible"));

    let sheet = setup_sheet::render(&world, None).expect("sheet");
    assert!(
        sheet.contains("! BOT"),
        "the unresolved depth shows as a dash with an annotation:\n{sheet}"
    );
    assert!(
        sheet.contains("origin"),
        "and says why in plain words:\n{sheet}"
    );
    assert!(plain_floats_only(&sheet), "no scientific notation anywhere:\n{sheet}");
    assert!(
        !sheet.contains("NaN") && !sheet.contains("nan") && !sheet.contains("inf"),
        "no Debug floats ever:\n{sheet}"
    );
}

/// #1063: the tool list prints a tool's stated part number, vendor,
/// coating and helix on a line of its own, and prints nothing for a tool
/// that states none of them (no "None", no empty labels).
#[test]
fn tool_list_prints_stated_catalog_facts_and_omits_unstated() {
    use camcraft_app::catalog_fields::ToolCatalog;
    use camcraft_app::commands::ChangeTool;
    let (world, mut history) = two_setup_world();
    let other = catalog()
        .into_iter()
        .map(|(n, _)| n)
        .find(|n| *n != "EM-0400-2FL")
        .expect("a second catalog tool");
    add_tool(&world, &mut history, other);
    let mut t = catalog_tool("EM-0400-2FL");
    t.catalog = ToolCatalog {
        part_number: Some("XY-1234".into()),
        vendor: Some("Acme Tooling".into()),
        coating: None,
        helix_deg: Some(35.0),
        notes: Some("never printed".into()),
    };
    run(&world, &mut history, ChangeTool::new(&world, "EM-0400-2FL", t), "state facts");
    create_face(&world, &mut history, "F1", 19.5);
    let cmd = CreateOperation::new(&world, "F2", other, SrcRef::Stock, face_params());
    history.execute(&world, cmd.expect("second face op"));
    let cmd = GenerateOperation::new(&world, "F2").expect("generate F2");
    history.execute(&world, cmd);
    let sheet = setup_sheet::render(&world, Some("Setup2")).expect("sheet");
    let lines: Vec<&str> = sheet.lines().collect();
    let catalog_lines: Vec<&&str> = lines.iter().filter(|l| l.contains("PART ")).collect();
    assert_eq!(catalog_lines.len(), 1, "only the stating tool gets a line:\n{sheet}");
    let l = catalog_lines[0];
    assert!(l.contains("PART XY-1234") && l.contains("VENDOR Acme Tooling"), "{l}");
    assert!(l.contains("HELIX 35"), "{l}");
    assert!(!l.contains("COATING"), "unstated coating is omitted: {l}");
    assert!(!sheet.contains("None") && !sheet.contains("never printed"), "{sheet}");
    assert!(plain_floats_only(&sheet));
}
