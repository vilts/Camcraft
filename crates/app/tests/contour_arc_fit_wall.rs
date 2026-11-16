//! #1579: a 2D Contour arc-fitted at a coarse tolerance around a
//! tessellated round wall stays on the air side of the wall in both
//! walk directions - the fit never hands the verifier a gouge.
//!
//! Gate map (M4.4): covers motion::machining2d (arcs::fit_arcs), app::operation (generate + verify of a 2D contour).

use camcraft_app::commands::{AddTool, CreateOperation, GenerateOperation, History, SetStock};
use camcraft_app::geometry::{IDENTITY_Q, Point3};
use camcraft_app::machining2d::{Lead, Side};
use camcraft_app::model;
use camcraft_app::operation::{Height, OpParams, SrcRef};
use camcraft_app::schema::Location;
use camcraft_app::stock::StockShape;
use camcraft_app::tools::catalog;
use camcraft_app::world::new_world;

/// The UI draw law's fixture (`arc_moves_draw_curved`): a Ø30 cylinder
/// stock skinned by a 4 mm end mill, Side Auto, tolerance 0.1.
fn skin(climb: bool) -> camcraft_app::schema::Toolpath {
    let world = new_world();
    let mut history = History::default();
    let stock = SetStock::new(
        &world,
        StockShape::Cylinder { diameter: 30.0, height: 10.0 },
        Location { pos: Point3::new(0.0, 0.0, 0.0), rot: IDENTITY_Q },
    );
    history.execute(&world, stock.expect("cylinder stock"));
    let em = catalog()
        .into_iter()
        .find(|(n, _)| *n == "EM-0400-2FL")
        .map(|(_, t)| t)
        .expect("catalog entry");
    history.execute(&world, AddTool::new(&world, "EM-0400-2FL", em).expect("tool"));
    let params = OpParams::Contour {
        tilt: None,
        stock_to_leave: 0.0,
        clearance_z: Height::fixed(20.0),
        retract_z: Height::fixed(15.0),
        top_z: Height::fixed(10.0),
        bottom_z: Height::fixed(8.0),
        stepdown: 2.0,
        feed: 800.0,
        plunge_feed: 200.0,
        climb,
        side: Side::Auto,
        lead: Lead::None,
        tolerance: 0.1,
        tab_count: 0,
        tab_width: 0.0,
        tab_height: 0.0,
        entry: camcraft_app::machining3d::Entry3D::Plunge,
        helix_radius: 1.0,
        blend_turns: false,
        links: camcraft_app::links::Links::PLANAR,
    };
    let op = CreateOperation::new(&world, "Skin1", "EM-0400-2FL", SrcRef::Stock, params);
    history.execute(&world, op.expect("operation"));
    history.execute(&world, GenerateOperation::new(&world, "Skin1").expect("generate"));
    model::find_by_name(&world, "Skin1")
        .and_then(|e| model::toolpath_of(&world, &e))
        .expect("cache")
}

#[test]
fn coarse_arc_fit_on_a_round_wall_verifies_clean_both_ways() {
    for climb in [true, false] {
        let cache = skin(climb);
        assert!(
            cache.path.has_arcs(),
            "climb={climb}: the tessellated wall still fits to arcs at tol 0.1"
        );
        assert!(
            cache.warnings.is_empty(),
            "climb={climb}: the fitted contour must not cross the wall: {:?}",
            cache.warnings
        );
        // Each arc move ends on its own circle (within the fit's 1e-3 mm
        // part-side slack): a split piece that ended on a vertex off the
        // circle bent between the checked vertices and gouged.
        for m in cache.path.moves.iter() {
            if let Some(a) = &m.arc {
                let rf = (m.from.x - a.center.x).hypot(m.from.y - a.center.y);
                let rt = (m.to.x - a.center.x).hypot(m.to.y - a.center.y);
                assert!((rf - rt).abs() <= 1e-3 + 1e-6, "climb={climb}: an arc ends {} mm off its circle", (rf - rt).abs());
            }
        }
    }
}
