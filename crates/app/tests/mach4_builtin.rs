//! Integration tests for Mach4 built-in registry entry (#1968).
//!
//! Gate map (M4.4): covers app::posting (BUILTINS, post_serves, post_setup)
//! with assets/posts/mach4.cps.

use camcraft_app::commands::{
    AddTool, CreateOperation, CreateSetup, GenerateOperation, History, SetSetupMachine, SetSetupPost, SetStock,
};
use camcraft_app::machine::{parse_mch, Kind};
use camcraft_app::operation::{Height, OpParams, SrcRef};
use camcraft_app::posting;
use camcraft_app::schema::PostRef;
use camcraft_app::stock::StockShape;
use camcraft_app::tools::catalog;
use camcraft_app::world::new_world;

#[test]
fn mach4_is_registered_and_serves_mills_and_mill_turns_only() {
    assert!(
        posting::BUILTINS.iter().any(|(name, _)| *name == "Mach4"),
        "Mach4 must be registered in posting::BUILTINS"
    );
    assert!(posting::post_serves("Mach4", Kind::Mill));
    assert!(posting::post_serves("Mach4", Kind::MillTurn));
    for kind in [Kind::Lathe, Kind::Plasma, Kind::WireEdm, Kind::SinkerEdm, Kind::Waterjet] {
        assert!(!posting::post_serves("Mach4", kind), "Mach4 must not serve {kind:?}");
    }
}

#[test]
fn mach4_posts_full_app_setup_cleanly() {
    let world = new_world();
    let mut history = History::default();
    history.execute(&world, CreateSetup::new(&world, None, None).expect("Setup1"));

    let vmc_json = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/machines/3axis-vmc.mch"),
    )
    .expect("3axis-vmc.mch reads");
    let vmc = parse_mch(&vmc_json).expect("3axis-vmc parses");
    history.execute(
        &world,
        SetSetupMachine::new(&world, "Setup1", Some(("3axis-vmc.mch".into(), Some(vmc)))).expect("valid"),
    );

    let stock = SetStock::new(&world, StockShape::Box { dx: 60.0, dy: 60.0, dz: 20.0 }, Default::default())
        .expect("stock");
    history.execute(&world, stock);

    let tool_name = "EM-0600-4FL";
    let tool = catalog()
        .into_iter()
        .find(|(n, _)| *n == tool_name)
        .map(|(_, t)| t)
        .expect("catalog tool");
    history.execute(&world, AddTool::new(&world, tool_name, tool).expect("add tool"));

    let params = OpParams::Face {
        holes: Default::default(),
        blend_turns: false, blend_radius: None,
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
    history.execute(&world, CreateOperation::new(&world, "Face1", tool_name, SrcRef::Stock, params).expect("create"));
    history.execute(&world, GenerateOperation::new(&world, "Face1").expect("generate"));
    history.execute(
        &world,
        SetSetupPost::new(&world, "Setup1", Some(PostRef::Named("Mach4".into()))).expect("set post"),
    );

    let dir = std::env::temp_dir().join(format!("camcraft-1968-{}", std::process::id()));
    let _ = std::fs::create_dir_all(&dir);
    let out_path = dir.join("mach4_setup.nc");
    let out = posting::post_setup(&world, &out_path).expect("Mach4 setup posts");
    assert!(out.program.contains("G21"), "expected G21:\n{}", out.program);
    assert!(out.program.contains("G94"), "expected G94:\n{}", out.program);
    assert!(out.program.contains("G64"), "expected G64:\n{}", out.program);
}
