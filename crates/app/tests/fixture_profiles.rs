//! #744 pins: fixtures as spun bodies or extruded profiles imported
//! from DXF, editable afterwards, saved/re-inserted in projects & fixture
//! library, optional profile derived from STEP keeping STEP as source,
//! and fixture collision/simulation using exact profile-built bodies.
//!
//! Gate map (M4.4): covers app::fixture_profile, app::commands::profile_fixture,
//! app::fixture, app::fixture_struck, app::posting, app::fixture_library,
//! app::project, app::mcp.
//! Provenance: our own domain fixtures and machining laws.

use std::path::Path;

use camcraft_app::cl::{Move, MoveKind};
use camcraft_app::commands::{
    AddTool, CreateOperation, DeriveFixtureProfile, GenerateOperation, History, ImportFixture,
    ImportModel, InsertProfileFixture, SetFixtureProfile, SetStock,
};
use camcraft_app::fixture;
use camcraft_app::fixture_library::{self, DatumChoice, entry_from_model};
use camcraft_app::fixture_profile::{ExtrudeAxis, ProfilePoint};
// Exercise the domain API directly while app commands accept the re-exported type.
use camcraft_sim::fixture_profile::FixtureProfile;
use camcraft_app::fixture_struck;
use camcraft_app::fixture_view::{self, BoxReason, MeshState};
use camcraft_app::geometry::{IDENTITY_Q, Point3};
use camcraft_app::model;
use camcraft_app::operation::{Height, OpParams, SrcRef};
use camcraft_app::posting;
use camcraft_app::project;
use camcraft_app::schema::Location;
use camcraft_app::stock::StockShape;
use camcraft_app::tools::catalog;
use camcraft_app::world::new_world;

fn dxf_wrap(entities: &str) -> String {
    format!(
        "0\nSECTION\n2\nHEADER\n9\n$INSUNITS\n70\n4\n0\nENDSEC\n\
         0\nSECTION\n2\nENTITIES\n{entities}0\nENDSEC\n0\nEOF\n"
    )
}

fn spun_chuck_dxf() -> String {
    dxf_wrap(
        "0\nLINE\n8\nCENTRELINE\n10\n-30\n20\n0\n11\n30\n21\n0\n\
         0\nLINE\n8\nPROFILE\n10\n-20\n20\n0\n11\n-20\n21\n45\n\
         0\nLINE\n8\nPROFILE\n10\n-20\n20\n45\n11\n5\n21\n45\n\
         0\nLINE\n8\nPROFILE\n10\n5\n20\n45\n11\n5\n21\n20\n\
         0\nLINE\n8\nPROFILE\n10\n5\n20\n20\n11\n15\n21\n20\n\
         0\nLINE\n8\nPROFILE\n10\n15\n20\n20\n11\n15\n21\n0\n",
    )
}

fn extruded_jaw_dxf(height: f64, width: f64) -> String {
    dxf_wrap(&format!(
        "0\nLWPOLYLINE\n8\nPROFILE\n90\n4\n70\n1\n\
         10\n-30\n20\n0\n\
         10\n30\n20\n0\n\
         10\n30\n20\n{height}\n\
         10\n-30\n20\n{height}\n\
         0\nTEXT\n8\nDEPTH\n10\n0\n20\n0\n1\nDEPTH={width}\n"
    ))
}

#[test]
fn spun_dxf_fixture_imports_meshes_edits_and_roundtrips() {
    let world = new_world();
    let mut history = History::default();
    let loc = Location { pos: Point3::new(0.0, 0.0, -5.0), rot: IDENTITY_Q };
    let cmd = InsertProfileFixture::from_dxf_spun(&world, "Chuck", spun_chuck_dxf().as_bytes(), None, loc)
        .expect("import spun DXF fixture");
    let name = cmd.name().to_owned();
    history.execute(&world, cmd);

    assert_eq!(fixture_view::mesh_states(&world, None), [(name.clone(), MeshState::BoxOnly(BoxReason::ProfileChord))]);
    let e = model::find_by_name(&world, &name).expect("chuck entity");
    let rev1 = model::rev_of(&world, &e);
    let bb1 = model::entity_world_aabb(&world, &e).expect("aabb");
    assert!((bb1.min.x - (-45.0)).abs() < 0.2 && (bb1.max.x - 45.0).abs() < 0.2);

    // Editable afterwards: shrink outer radius from 45 mm to 30 mm.
    let edited = match fixture::def_of(&world, &e).and_then(|d| d.profile).expect("profile") {
        FixtureProfile::Spun { mut points } => {
            for p in &mut points {
                if (p.r - 45.0).abs() < 1e-6 {
                    p.r = 30.0;
                }
            }
            FixtureProfile::Spun { points }
        }
        other => panic!("expected Spun profile, got {other:?}"),
    };
    history.execute(&world, SetFixtureProfile::new(&world, &name, edited.clone()).expect("edit profile"));
    let rev2 = model::rev_of(&world, &e);
    assert!(rev2 > rev1, "profile edit must bump revision");
    let bb2 = model::entity_world_aabb(&world, &e).expect("aabb 2");
    assert!((bb2.max.x - 30.0).abs() < 0.2, "edited radius reflected in mesh: {bb2:?}");

    // Undo / redo restores exact mesh & profile.
    assert!(history.undo(&world));
    let bb_undo = model::entity_world_aabb(&world, &e).expect("undo aabb");
    assert!((bb_undo.max.x - 45.0).abs() < 0.2);
    assert!(history.redo(&world));

    // Survives project save + load.
    let saved = serde_json::to_string(&project::collect(&world)).expect("save");
    let back = new_world();
    project::open_noted(&back, &saved).expect("load");
    let back_e = model::find_by_name(&back, &name).expect("reloaded");
    assert_eq!(fixture::def_of(&back, &back_e).and_then(|d| d.profile), Some(edited));
}

#[test]
fn extruded_vise_jaw_from_dxf_is_hit_by_rapid_until_edited_below_it() {
    let world = new_world();
    let mut history = History::default();
    // 60 x 40 x 20 stock centred on XY (z 0..20).
    history.execute(
        &world,
        SetStock::new(&world, StockShape::Box { dx: 60.0, dy: 40.0, dz: 20.0 }, Location::default()).expect("stock"),
    );
    let tool = catalog().into_iter().find(|(n, _)| *n == "EM-0400-2FL").map(|(_, t)| t).expect("tool");
    history.execute(&world, AddTool::new(&world, "EM-0400-2FL", tool).expect("tool"));
    let op = OpParams::Face {
        holes: Default::default(),
        tilt: None,
        blend_turns: false,
        stepover: 4.0,
        clearance_z: Height::fixed(30.0),
        cut_z: Height::fixed(19.5),
        top_z: None,
        stepdown: None,
        feed: 1200.0,
        plunge_feed: 300.0,
        climb: true,
    };
    history.execute(&world, CreateOperation::new(&world, "Face1", "EM-0400-2FL", SrcRef::Stock, op).expect("op"));
    history.execute(&world, GenerateOperation::new(&world, "Face1").expect("gen"));

    // Tall vise jaw built from DXF: X -30..30, Y 20..40 (depth 20 along Y), Z 0..38.
    let jaw_pose = Location { pos: Point3::new(0.0, 20.0, 0.0), rot: IDENTITY_Q };
    let cmd = InsertProfileFixture::from_dxf_extruded(
        &world,
        "ViseJaw",
        extruded_jaw_dxf(38.0, 20.0).as_bytes(),
        None,
        ExtrudeAxis::Y,
        None,
        jaw_pose,
    )
    .expect("import extruded DXF jaw");
    let jaw_name = cmd.name().to_owned();
    history.execute(&world, cmd);
    assert_eq!(fixture_view::mesh_states(&world, None), [(jaw_name.clone(), MeshState::Mesh)]);

    // A rapid move at Z 32 right through the profile-built jaw (Y 0..50 at X 0).
    let op_val = model::find_by_name(&world, "Face1").and_then(|e| model::operation_of(&world, &e)).expect("op");
    let rapid = vec![Move::new(
        MoveKind::Rapid,
        Point3::new(0.0, 0.0, 32.0),
        Point3::new(0.0, 50.0, 32.0),
        None,
    )];
    let path = camcraft_app::cl::Toolpath::new(rapid);
    let (turning, fixed) = fixture::workholding_for(&world, None);
    let meshes = fixture::meshes_for(&world, None);
    let warnings = fixture_struck::op_hits(&world, &op_val, &path, &turning, &fixed, &meshes).expect("hits");
    assert_eq!(fixture_struck::warned_moves(&warnings), [0], "rapid through vise jaw must hit it");

    // Also pin posting verification_report rejects a colliding rapid when clearance/link is through the jaw,
    // AND editing the jaw profile down from 38 mm to 15 mm makes the exact same rapid clear!
    let lower_jaw = FixtureProfile::Extruded {
        points: vec![
            ProfilePoint { u: -30.0, v: 0.0, bulge: 0.0 },
            ProfilePoint { u: 30.0, v: 0.0, bulge: 0.0 },
            ProfilePoint { u: 30.0, v: 15.0, bulge: 0.0 },
            ProfilePoint { u: -30.0, v: 15.0, bulge: 0.0 },
        ],
        axis: ExtrudeAxis::Y,
        depth: 20.0,
    };
    history.execute(&world, SetFixtureProfile::new(&world, &jaw_name, lower_jaw).expect("lower jaw"));
    let (turning2, fixed2) = fixture::workholding_for(&world, None);
    let meshes2 = fixture::meshes_for(&world, None);
    let warnings2 = fixture_struck::op_hits(&world, &op_val, &path, &turning2, &fixed2, &meshes2).expect("hits 2");
    assert!(fixture_struck::warned_moves(&warnings2).is_empty(), "edited lower jaw must clear z=32 rapid");
    let report = posting::verification_report(&world);
    assert!(report.problems.is_empty(), "post verification stays healthy: {report:?}");
}

#[test]
fn step_fixture_can_optionally_derive_profile_while_keeping_step_as_source() {
    let step_path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testdata/models/box.step");
    let step_bytes = std::fs::read(&step_path).expect("box.step");
    let world = new_world();
    let mut history = History::default();
    let import = ImportModel::from_bytes("vendor_parallel.step", &step_bytes, None).expect("STEP");
    let fix = ImportFixture::new(&world, import, None).expect("fixture");
    let name = fix.name().to_owned();
    history.execute(&world, fix);

    let e = model::find_by_name(&world, &name).expect("entity");
    let original_facets = model::facets_of(&world, &e).expect("facets");

    history.execute(&world, DeriveFixtureProfile::extruded(&world, &name, ExtrudeAxis::Z).expect("derive"));
    let e_after = model::find_by_name(&world, &name).expect("entity after");
    assert_eq!(
        model::facets_of(&world, &e_after).expect("facets after").as_ref(),
        original_facets.as_ref(),
        "deriving a profile must keep the vendor STEP solid mesh intact"
    );
    assert!(fixture::def_of(&world, &e_after).and_then(|d| d.profile).is_some());
    let entry = entry_from_model(&world, &name, DatumChoice::ModelOrigin, None).expect("library entry");
    let text = fixture_library::to_text(&[entry]).expect("save");
    assert!(text.contains("ISO-10303-21"), "STEP stays the source in library save");

    // Conversion compatibility with #743 turning/tool SpunProfile (arc radii <-> DXF bulge).
    let tool_spun = camcraft_tools::spun::SpunProfile::new(vec![
        camcraft_tools::spun::SpunPoint::line(0.0, 0.0),
        camcraft_tools::spun::SpunPoint::line(0.0, 20.0),
        camcraft_tools::spun::SpunPoint { z: 10.0, r: 20.0, arc: 15.0 },
        camcraft_tools::spun::SpunPoint::line(20.0, 12.0),
        camcraft_tools::spun::SpunPoint::line(20.0, 0.0),
    ])
    .expect("tool spun");
    let conv = FixtureProfile::from_tool_spun(&tool_spun).expect("convert from #743");
    let back_spun = conv.to_tool_spun().expect("convert back to #743");
    assert_eq!(back_spun.points().len(), tool_spun.points().len());
    assert!((back_spun.points()[2].arc - 15.0).abs() < 1e-3);
}

#[test]
fn profile_fixture_roundtrips_library_and_malformed_project_key_loads_safely() {
    let world = new_world();
    let mut history = History::default();
    let cmd = InsertProfileFixture::from_dxf_spun(&world, "Collet", spun_chuck_dxf().as_bytes(), None, Location::default())
        .expect("insert collet");
    let name = cmd.name().to_owned();
    history.execute(&world, cmd);

    // Save profile fixture to library and re-insert into fresh world.
    let lib_entry = entry_from_model(&world, &name, DatumChoice::ModelOrigin, Some("collet")).expect("entry");
    let lib_txt = fixture_library::to_text(&[lib_entry]).expect("serialize lib");
    let parsed = fixture_library::parse(&lib_txt).expect("parse lib");
    assert!(parsed[0].profile.is_some() && parsed[0].step_source.is_none());

    let fresh = new_world();
    let mut h2 = History::default();
    let ins = camcraft_app::commands::InsertFixture::new(&fresh, &parsed[0], None, Location::default()).expect("insert");
    let inserted_name = ins.name().to_owned();
    h2.execute(&fresh, ins);
    let e2 = model::find_by_name(&fresh, &inserted_name).expect("e2");
    assert!(fixture::def_of(&fresh, &e2).and_then(|d| d.profile).is_some());

    // Malformed `profile` key in .camproj drops leniently with a load note, keeping saved steel.
    let mut j: serde_json::Value = serde_json::from_str(&serde_json::to_string(&project::collect(&fresh)).expect("save")).expect("json");
    let entities = j.get_mut("entities").and_then(serde_json::Value::as_array_mut).expect("entities");
    let row = entities.iter_mut().find(|r| r.get("name").and_then(serde_json::Value::as_str) == Some(inserted_name.as_str())).expect("row");
    row["fixture"]["profile"]["depth"] = serde_json::json!(-5.0);
    row["fixture"]["profile"]["kind"] = serde_json::json!("extruded");
    let loaded = new_world();
    let notes = project::open_noted(&loaded, &j.to_string()).expect("lenient load");
    assert!(!notes.is_empty(), "dropped invalid profile emits load note");
    let e3 = model::find_by_name(&loaded, &inserted_name).expect("kept entity");
    assert!(model::facets_of(&loaded, &e3).is_some(), "saved steel preserved");
}


#[test]
fn profile_arithmetic_refusal_preserves_fixture_history_and_project() {
    use camcraft_app::fixture_profile::SpunPoint;

    let valid = FixtureProfile::Spun { points: vec![
        SpunPoint::line(0.0, 0.0), SpunPoint::line(0.0, 10.0),
        SpunPoint::line(10.0, 10.0), SpunPoint::line(10.0, 0.0),
    ] };
    let world = new_world();
    let mut history = History::default();
    history.execute(&world, InsertProfileFixture::new(&world, "Steel", valid.clone(), None, Location::default()).unwrap());
    let edited = FixtureProfile::Spun { points: vec![
        SpunPoint::line(0.0, 0.0), SpunPoint::line(0.0, 20.0),
        SpunPoint::line(10.0, 20.0), SpunPoint::line(10.0, 0.0),
    ] };
    history.execute(&world, SetFixtureProfile::new(&world, "Steel", edited.clone()).unwrap());
    assert!(history.undo(&world));
    history.mark_clean();
    let before = project::save_text(&world).unwrap();
    let entries = history.done_entry_ids();
    let boxes = fixture::boxes_for(&world, None);
    let invalid = [
        FixtureProfile::Spun { points: vec![
            SpunPoint::line(0.0, 0.0), SpunPoint { z: 0.0, r: 10.0, bulge: 1e200 },
            SpunPoint::line(10.0, 10.0), SpunPoint::line(10.0, 0.0),
        ] },
        FixtureProfile::Extruded { points: vec![
            ProfilePoint { u: 0.0, v: 0.0, bulge: 1e200 }, ProfilePoint::line(10.0, 0.0),
            ProfilePoint::line(10.0, 10.0), ProfilePoint::line(0.0, 10.0),
        ], axis: ExtrudeAxis::Z, depth: 10.0 },
        FixtureProfile::Spun { points: vec![
            SpunPoint::line(0.0, 0.0), SpunPoint::line(0.0, 1e12),
            SpunPoint::line(10.0, 1e12), SpunPoint::line(10.0, 0.0),
        ] },
        FixtureProfile::Extruded { points: vec![
            ProfilePoint::line(0.0, 0.0), ProfilePoint::line(10.0, 0.0),
            ProfilePoint::line(10.0, 10.0), ProfilePoint::line(0.0, 10.0),
        ], axis: ExtrudeAxis::X, depth: 1e12 },
        // Finite facets can still have an out-of-range conservative collision envelope.
        FixtureProfile::Spun { points: vec![
            SpunPoint::line(0.0, 0.0), SpunPoint::line(0.0, 9.99e11),
            SpunPoint::line(10.0, 9.99e11), SpunPoint::line(10.0, 0.0),
        ] },
        // Finite endpoints and bulge still compute an out-of-range arc.
        FixtureProfile::Extruded { points: vec![
            ProfilePoint { u: 0.0, v: 0.0, bulge: 1e12 }, ProfilePoint::line(10.0, 0.0),
            ProfilePoint::line(10.0, 10.0), ProfilePoint::line(0.0, 10.0),
        ], axis: ExtrudeAxis::Y, depth: 10.0 },
    ];
    for (index, profile) in invalid.into_iter().enumerate() {
        let error = SetFixtureProfile::new(&world, "Steel", profile.clone()).err().expect("unsafe arithmetic refused before entering history").to_string();
        assert!(error.contains("profile") || error.contains("extrusion"), "named refusal: {error}");
        if index < 2 { assert!(error.contains("arc arithmetic overflow"), "{error}"); }
        if index == 4 { assert!(error.contains("collision bound"), "{error}"); }
        assert!(InsertProfileFixture::new(&world, "Unsafe", profile, None, Location::default()).is_err());
        assert_eq!(history.done_entry_ids(), entries);
        assert!(history.can_undo() && history.can_redo() && !history.is_dirty());
        assert_eq!(fixture::boxes_for(&world, None), boxes);
        let saved = project::save_text(&world).unwrap();
        assert_eq!(saved, before, "refusal must preserve all fixture data and facets");
        let reopened = new_world();
        project::open(&reopened, &saved).expect("refused edit cannot corrupt the saved project");
        let entity = model::find_by_name(&reopened, "Steel").unwrap();
        assert_eq!(fixture::def_of(&reopened, &entity).unwrap().profile, Some(valid.clone()));
    }
    assert!(history.redo(&world), "refusal preserves the pending redo");
    let entity = model::find_by_name(&world, "Steel").unwrap();
    assert_eq!(fixture::def_of(&world, &entity).unwrap().profile, Some(edited));
    assert!(history.undo(&world));
    assert_eq!(project::save_text(&world).unwrap(), before);
}
