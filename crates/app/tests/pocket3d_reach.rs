//! #1844: a closed cavity must refuse a tool whose shank or holder cannot reach.
//! Gate map: covers app::operation::generate.
//! Provenance: hand-built 40 mm block with a 20 mm square cavity, 30 mm deep.

mod area_support;

use area_support::*;
use camcraft_app::geometry::{Mesh, Triangle};
use camcraft_app::tools::Tool;

const NAME: &str = "reach-tool";

fn cavity() -> Mesh {
    let p = Point3::new;
    let outer = cube(p(-20.0, -20.0, 0.0), p(20.0, 20.0, 40.0));
    let inner = cube(p(-10.0, -10.0, 10.0), p(10.0, 10.0, 40.0));
    let floor = cube(p(-10.0, -10.0, 0.0), p(10.0, 10.0, 10.0));
    let mut facets = outer.facets[0..2].to_vec();
    facets.extend_from_slice(&outer.facets[4..12]);
    facets.extend_from_slice(&floor.facets[2..4]);
    facets.extend(inner.facets[4..12].iter().map(|t| Triangle::new(t.a, t.c, t.b)));
    let outer = [(-20.0, -20.0), (20.0, -20.0), (20.0, 20.0), (-20.0, 20.0)];
    let inner = [(-10.0, -10.0), (10.0, -10.0), (10.0, 10.0), (-10.0, 10.0)];
    for k in 0..4 {
        let n = (k + 1) % 4;
        let at = |(x, y)| p(x, y, 40.0);
        facets.push(Triangle::new(at(outer[k]), at(outer[n]), at(inner[n])));
        facets.push(Triangle::new(at(outer[k]), at(inner[n]), at(inner[k])));
    }
    Mesh::from_facets(facets)
}

fn tool() -> Tool {
    catalog().into_iter().find(|(n, _)| *n == "EM-0600-4FL").map(|(_, t)| t).expect("catalog tool")
}

fn generate(tool: Tool, bottom: f64) -> Result<(Toolpath, Vec<camcraft_app::verify::Warning>), String> {
    let world = new_world();
    let mut history = History::default();
    history.execute(&world, ImportModel::from_mesh("cavity", cavity()));
    history.execute(&world, SetStock::new(&world, StockShape::Box { dx: 40.0, dy: 40.0, dz: 40.0 }, Location::default()).expect("stock"));
    history.execute(&world, AddTool::new(&world, NAME, tool).expect("tool"));
    let params = OpParams::Pocket3D {
        pass_start: Default::default(),
        links: camcraft_app::links::Links::ROUGH3D,
        stepover: 2.0,
        stock_to_leave: 0.0,
        clearance_z: Height::fixed(45.0),
        retract_z: None,
        top_z: Height::fixed(40.0),
        bottom_z: Height::fixed(bottom),
        stepdown: 5.0,
        feed: 1200.0,
        plunge_feed: 300.0,
        climb: true,
        strategy: PocketStrategy::OffsetRings,
        entry: Entry3D::Plunge,
        helix_radius: 1.0,
        rest_rough: false,
        optimal_load: camcraft_app::load::DEFAULT_OPTIMAL_LOAD,
        feed_opt: false,
        shell: false,
        method: camcraft_app::machining2d::AdaptiveMethod::Spiral,
        picks: Default::default(),
    };
    let mut operation = op(SrcRef::Entity("cavity".into()), params, AreaControls::default());
    operation.tool = NAME.into();
    operation::generate(&world, &operation, "R1").map_err(|e| e.to_string())
}

fn refusal(tool: Tool, band: &str) {
    let error = generate(tool, 10.0)
        .map(|(path, warnings)| format!("{} moves and {} warnings", path.moves.len(), warnings.len()))
        .expect_err("the unreachable cavity must refuse before returning a path");
    assert!(error.contains("3D Pocket") && error.contains(band), "name the operation and obstruction: {error}");
    assert!(error.contains("Bottom") && error.contains(NAME), "name the tool and remedy: {error}");
}

#[test]
fn a_closed_cavity_refuses_the_wider_shank_by_name() {
    refusal(tool(), "shank");
}

#[test]
fn a_closed_cavity_refuses_the_holder_with_a_flute_width_shank() {
    refusal(Tool { shank_d: 6.0, shank_len: 10.0, ..tool() }, "holder");
}

#[test]
fn a_shallower_bottom_keeps_the_wider_shank_above_the_wall() {
    let (path, warnings) = generate(tool(), 30.0).expect("10 mm depth is within the 15 mm flutes");
    assert!(path.moves.iter().any(|m| m.kind == MoveKind::Feed));
    assert!(!warnings.iter().any(|w| w.blocks_post()), "the shallow cut verifies clean: {warnings:?}");
}

#[test]
fn longer_flutes_reach_the_closed_cavity_floor() {
    let (path, warnings) = generate(Tool { flute_len: 35.0, ..tool() }, 10.0).expect("flutes reach the 30 mm cavity");
    assert!(path.moves.iter().any(|m| m.kind == MoveKind::Feed && (m.to.z - 10.0).abs() < 1e-6));
    assert!(!warnings.iter().any(|w| w.blocks_post()), "the reaching tool verifies clean: {warnings:?}");
}

#[test]
fn a_wider_shank_touching_the_wall_top_is_refused() {
    let error = generate(tool(), 25.0)
        .map(|(path, warnings)| format!("{} moves and {} warnings", path.moves.len(), warnings.len()))
        .expect_err("the wider shank touches the top at exactly the flute length");
    assert!(error.contains("shank") && error.contains("Bottom"), "{error}");
    let (_, warnings) = generate(tool(), 25.001).expect("a positive gap clears the wall top");
    assert!(!warnings.iter().any(|w| w.blocks_post()), "the gap verifies clean: {warnings:?}");
}

#[test]
fn a_flute_width_shank_can_extend_below_the_wall_top() {
    let (_, warnings) = generate(Tool { shank_d: 6.0, ..tool() }, 10.0).expect("the equal-width shank fits and the holder stays above the wall");
    assert!(!warnings.iter().any(|w| w.blocks_post()), "the flute-width shank verifies clean: {warnings:?}");
}
