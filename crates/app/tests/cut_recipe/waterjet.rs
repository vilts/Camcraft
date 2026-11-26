//! Recipe waterjet tab-position regressions for PR #3399.
//! Gate map: covers app::commands::operation::recipe.
//! Provenance: original fixtures and command contracts.

use super::*;
use camcraft_app::commands::{ImportModel, LoadMachine};
use camcraft_app::geometry::{Mesh, Point3, Triangle};
use camcraft_app::machine;
use camcraft_app::operation::{JetLead, JetSide, JetTabsOp, WaterjetOp};
use camcraft_app::waterjet_tools::waterjet_tool;

const NOZZLE: &str = "Nozzle";
const PLATE: &str = "plate";

const WATERJET: &str = r#"{"general":{"model":"Waterjet test","kind":"waterjet"},"kinematics":{"default":{"parts":[{"name":"X","type":"linear","travel":{"min":-100,"max":360}},{"name":"Y","type":"linear","travel":{"min":-150,"max":150}},{"name":"Z","type":"linear","travel":{"min":-20,"max":60}},{"name":"nozzle","type":"head"}]}},"controller":{"synced_configuration":{"parts":{"X":{"coordinate":0,"max_normal_speed":1000},"Y":{"coordinate":1,"max_normal_speed":700},"Z":{"coordinate":2,"max_normal_speed":500}}}}}"#;

/// A 40 x 40 x 5 plate, centred on the origin.
fn plate() -> Mesh {
    let p = |x: f64, y: f64, z: f64| Point3::new(x, y, z);
    let (lo, hi) = (-20.0, 20.0);
    let b = [p(lo, lo, 0.0), p(hi, lo, 0.0), p(hi, hi, 0.0), p(lo, hi, 0.0)];
    let t = [p(lo, lo, 5.0), p(hi, lo, 5.0), p(hi, hi, 5.0), p(lo, hi, 5.0)];
    let mut f = vec![
        Triangle::new(t[0], t[1], t[2]),
        Triangle::new(t[0], t[2], t[3]),
        Triangle::new(b[0], b[2], b[1]),
        Triangle::new(b[0], b[3], b[2]),
    ];
    for i in 0..4 {
        let j = (i + 1) % 4;
        f.push(Triangle::new(b[i], b[j], t[j]));
        f.push(Triangle::new(b[i], t[j], t[i]));
    }
    Mesh::from_facets(f)
}

/// A waterjet document: the waterjet mounted, the nozzle, the plate.
fn waterjet_doc() -> (World, History) {
    let world = new_world();
    let mut history = History::default();
    let m = machine::parse_mch(WATERJET).expect("valid waterjet");
    history.execute(&world, LoadMachine::new(&world, "wj.mch".into(), Some(m)).expect("mounts"));
    history.execute(&world, AddTool::new(&world, NOZZLE, waterjet_tool(0.8)).expect("nozzle"));
    history.execute(&world, ImportModel::from_mesh(PLATE, plate()));
    (world, history)
}

fn waterjet_recipe(tabs: Option<JetTabsOp>) -> CutRecipe {
    CutRecipe {
        name: "Jet recipe".into(),
        tool: NOZZLE.into(),
        material: None,
        params: OpParams::WaterjetProfile(WaterjetOp {
            clearance_z: Height::fixed(25.0),
            top_z: Height::fixed(5.0),
            bottom_z: Height::fixed(0.0),
            kerf: None,
            control_comp: false,
            reversed: false,
            lead: JetLead::Arc,
            lead_in: 3.0,
            lead_out: 2.0,
            open_side: JetSide::Center,
            tabs,
            feed: 250.0,
            pierce_time: 1.5,
            abrasive_delay: 0.5,
            linearize: None,
            picks: Default::default(),
        }),
    }
}

#[test]
fn cut_recipe_refuses_saved_waterjet_tab_positions_by_name() {
    let (world, _) = waterjet_doc();
    let tabs = JetTabsOp {
        count: 2,
        width: 2.0,
        at: vec![[0.0, -20.0], [0.0, 20.0]],
    };
    let recipe = waterjet_recipe(Some(tabs));
    // The same parameters are legal in a plain operation: only the recipe
    // door refuses them.
    CreateOperation::new(&world, "Direct", NOZZLE, SrcRef::Entity(PLATE.into()), recipe.params.clone())
        .expect("the tabbed parameters are legal outside a recipe");
    let err = ApplyCutRecipe::new(&world, &recipe, "New", SrcRef::Entity(PLATE.into()))
        .err()
        .expect("tab positions refused")
        .to_string();
    assert!(err.contains("Jet recipe") && err.contains("saved geometry"), "{err}");
    assert!(model::find_by_name(&world, "New").is_none());
}

#[test]
fn cut_recipe_applies_a_waterjet_recipe_without_tab_positions() {
    let (world, mut history) = waterjet_doc();
    let recipe = waterjet_recipe(None);
    let cmd = ApplyCutRecipe::new(&world, &recipe, "New", SrcRef::Entity(PLATE.into())).expect("a recipe without tabs applies");
    history.execute(&world, cmd);
    let e = model::find_by_name(&world, "New").expect("op made");
    let op = model::operation_of(&world, &e).expect("operation");
    assert_eq!(op.params, recipe.params);
    history.execute(&world, GenerateOperation::new(&world, "New").expect("generate"));
    assert!(model::toolpath_of(&world, &e).is_some_and(|t| !t.path.moves.is_empty()), "a generated path");
}
