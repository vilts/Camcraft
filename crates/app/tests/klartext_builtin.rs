//! The qualified Picomax and the selected Klartext post reach the normal posting gate.
//! Provenance: a Camcraft-generated synthetic facing job; the posted bounds are its stock.
//! Gate map: covers app::posting, app::project, post::machine, assets/posts/heidenhain-klartext.cps.

use camcraft_app::commands::{
    AddTool, CreateOperation, CreateSetup, GenerateOperation, History, ImportModel,
    SetSetupMachine, SetSetupPost, SetStock,
};
use camcraft_app::machine::Kind;
use camcraft_app::operation::{Height, OpParams, SrcRef};
use camcraft_app::schema::PostRef;
use camcraft_app::stock::StockShape;
use camcraft_app::{posting, project, tools, world::new_world};

const POST: &str = "Heidenhain Klartext";
const MILL: &str = "Fehlmann Picomax 55 (iTNC 530)";

fn generated_contour_and_face() -> flecs_ecs::prelude::World {
    let world = new_world();
    let mut history = History::default();
    history.execute(&world, CreateSetup::new(&world, None, None).unwrap());
    history.execute(
        &world,
        SetSetupMachine::builtin(&world, "Setup1", MILL).unwrap(),
    );
    history.execute(
        &world,
        SetSetupPost::new(&world, "Setup1", Some(PostRef::Named(POST.into()))).unwrap(),
    );
    let stock = StockShape::Box {
        dx: 60.0,
        dy: 60.0,
        dz: 20.0,
    };
    history.execute(
        &world,
        SetStock::new(&world, stock, Default::default()).unwrap(),
    );

    let name = "EM-0600-4FL";
    let (_, mut tool) = tools::catalog()
        .into_iter()
        .find(|(n, _)| *n == name)
        .unwrap();
    tool.rpm = 3500.0;
    history.execute(&world, AddTool::new(&world, name, tool).unwrap());
    let face = OpParams::Face {
        holes: Default::default(),
        blend_turns: false,
        blend_radius: None,
        tilt: None,
        stepover: 3.0,
        clearance_z: Height::fixed(25.0),
        cut_z: Height::fixed(19.5),
        top_z: None,
        stepdown: None,
        feed: 1200.0,
        plunge_feed: 300.0,
        climb: true,
        picks: Default::default(),
    };
    history.execute(
        &world,
        CreateOperation::new(&world, "Face1", name, SrcRef::Stock, face).unwrap(),
    );
    history.execute(&world, GenerateOperation::new(&world, "Face1").unwrap());

    let part = camcraft_app::geometry::cube(
        camcraft_app::geometry::Point3::ZERO,
        camcraft_app::geometry::Point3::new(10.0, 10.0, 19.5),
    );
    history.execute(&world, ImportModel::from_mesh("Block", part));
    let contour = OpParams::Contour {
        chain_overrides: Vec::new(),
        tilt: None,
        stock_to_leave: 0.0,
        clearance_z: Height::fixed(25.0),
        retract_z: Height::fixed(24.0),
        top_z: Height::fixed(19.5),
        bottom_z: Height::fixed(15.0),
        stepdown: 2.5,
        feed: 800.0,
        plunge_feed: 200.0,
        climb: true,
        side: camcraft_app::machining2d::Side::Auto,
        lead: camcraft_app::machining2d::Lead::None,
        tolerance: 0.01,
        tab_count: 0,
        tab_width: 0.0,
        tab_height: 0.0,
        entry: camcraft_app::machining3d::Entry3D::Plunge,
        helix_radius: 1.0,
        blend_turns: false,
        blend_radius: None,
        links: camcraft_app::links::Links::PLANAR,
        picks: Default::default(),
    };
    history.execute(
        &world,
        CreateOperation::new(
            &world,
            "Contour1",
            name,
            SrcRef::Entity("Block".into()),
            contour,
        )
        .unwrap(),
    );
    history.execute(&world, GenerateOperation::new(&world, "Contour1").unwrap());
    world
}

#[test]
fn facing_and_contouring_post_on_the_picomax_after_reopening() {
    let world = generated_contour_and_face();
    let nc = posting::post_setup_unwritten(&world)
        .expect("qualified machine, generated milling toolpath")
        .program;
    assert!(nc.contains("Face1") && nc.contains("Contour1"), "{nc}");
    assert!(nc.contains("BEGIN PGM") && nc.contains("END PGM"), "{nc}");
    for block in ["BLK FORM 0.1", "BLK FORM 0.2"] {
        assert!(nc.contains(block), "{nc}");
    }

    let saved = serde_json::to_string(&project::collect(&world)).unwrap();
    let reopened = new_world();
    project::open_noted(&reopened, &saved).expect("restore machine and post selection");
    assert_eq!(
        posting::post_setup_unwritten(&reopened).unwrap().program,
        nc
    );
}

#[test]
fn changing_controller_or_tool_procedure_keeps_the_previous_nc_file() {
    let world = generated_contour_and_face();
    let mut history = History::default();
    let destination =
        std::env::temp_dir().join(format!("camcraft-klartext-app-{}.H", std::process::id()));
    let accepted = posting::post_setup(&world, &destination).unwrap().program;

    let source = include_str!("../../../assets/machines/fehlmann-picomax55.mch");
    for (from, to, diagnostic) in [
        ("340490-03", "340490-04", "controller profile"),
        ("manual-retract-top", "manual", "manual-retract-top"),
        ("manual-retract-top", "changer", "manual-retract-top"),
    ] {
        let changed = camcraft_app::machine::parse_mch(&source.replace(from, to)).unwrap();
        let mount = Some(("alternate machine.mch".into(), Some(changed)));
        history.execute(
            &world,
            SetSetupMachine::new(&world, "Setup1", mount).unwrap(),
        );
        let why = posting::post_setup(&world, &destination)
            .expect_err("unsupported installation");
        assert!(why.contains(diagnostic), "{why}");
        assert_eq!(std::fs::read_to_string(&destination).unwrap(), accepted);
    }
    std::fs::remove_file(destination).unwrap();
}

#[test]
fn the_klartext_post_is_only_offered_for_mills() {
    assert!(posting::post_serves(POST, Kind::Mill));
    for kind in [
        Kind::Lathe,
        Kind::MillTurn,
        Kind::Plasma,
        Kind::WireEdm,
        Kind::SinkerEdm,
        Kind::Waterjet,
    ] {
        assert!(!posting::post_serves(POST, kind), "{kind:?}");
    }
}
