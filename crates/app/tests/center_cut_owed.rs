//! M11.3 owed (RECOMMENDED "M11.3 owed: the centre-cutting law on every
//! plunge" (5)), through the application: the tool's own
//! `center_cutting` reaches Face and Drill. A stock-sourced Face enters
//! a radius off the stock - air - so a non-centre-cutting face mill
//! posts the very path a centre-cutting one does; a drill cycle with a
//! non-centre-cutting drill is refused by name (the M10.1 tool law is a
//! family match and lets it through, so the cycle's door says no). An
//! inner-patch Face runs its entry on along the first row out to air
//! (M11.3 owed (4)) instead of refusing.
//!
//! Gate map (M11.3): covers app::operation (generate, plunge_solid),
//! motion::drilling, motion::machining2d (face).

use flecs_ecs::prelude::World;

use camcraft_app::commands::{AddTool, History, SetStock};
use camcraft_app::drilling::{CycleChoice, Hole};
use camcraft_app::geo2d::Point2;
use camcraft_app::operation::{self, Height, OpParams, SrcRef};
use camcraft_app::schema::{Location, Operation};
use camcraft_app::stock::StockShape;
use camcraft_app::tools::{catalog, Tool};
use camcraft_app::world::new_world;
use camcraft_motion::approach::CENTER_CUT_REFUSAL;

fn catalog_tool(name: &str) -> Tool {
    catalog().into_iter().find(|(n, _)| *n == name).map(|(_, t)| t).expect("catalog entry")
}

/// A box stock and the catalog tool `base` twice: as itself, and as
/// "NC", the same body marked not centre-cutting.
fn world_with(base: &str) -> (World, History) {
    let world = new_world();
    let mut h = History::default();
    let stock = SetStock::new(&world, StockShape::Box { dx: 60.0, dy: 60.0, dz: 20.0 }, Location::default());
    h.execute(&world, stock.expect("box stock"));
    let t = catalog_tool(base);
    h.execute(&world, AddTool::new(&world, base, t.clone()).expect("tool"));
    h.execute(&world, AddTool::new(&world, "NC", Tool { center_cutting: false, ..t }).expect("tool"));
    (world, h)
}

fn op(tool: &str, params: OpParams) -> Operation {
    Operation { tool: tool.into(), src: SrcRef::Stock, params, stamp: None, setup: None, area: Default::default(), coolant: Default::default(), after: Default::default(), suppressed: false }
}

#[test]
fn a_stock_face_enters_from_air_for_a_tool_that_is_not_centre_cutting() {
    let (world, _h) = world_with("EM-0800-4FL");
    for (top_z, stepdown) in [(None, None), (Some(Height::fixed(20.0)), Some(1.0))] {
        let face = OpParams::Face { holes: Default::default(),
            blend_turns: false,
            tilt: None,
            stepover: 4.0,
            clearance_z: Height::fixed(30.0),
            cut_z: Height::fixed(18.0),
            top_z,
            stepdown,
            feed: 1200.0,
            plunge_feed: 300.0,
            climb: true,
        };
        let (cc, _) = operation::generate(&world, &op("EM-0800-4FL", face.clone()), "F").expect("cc face");
        let (nc, _) = operation::generate(&world, &op("NC", face), "F").expect("the facer enters from air");
        assert_eq!(cc.moves, nc.moves, "the same path for either tool");
    }
}

/// The inner-patch Face (M11.3 owed (4)): a 20 x 20 part standing in
/// the middle of the 60 x 60 stock, faced at its top. The plain entry
/// lands on the stock around the part; for the non-centre-cutting tool
/// it runs on along the first row out past the stock's edge, so the
/// plunge is air, and the path verifies as clean as the centre-cutting
/// one - which is the facer's own, unchanged.
#[test]
fn an_inner_patch_face_runs_its_entry_out_to_air_for_a_tool_that_is_not_centre_cutting() {
    use camcraft_app::commands::ImportModel;
    use camcraft_app::geo2d::{Loop, Region};
    use camcraft_app::geometry::{cube, Point3};
    use camcraft_app::machining2d::{face, FaceParams};
    use camcraft_motion::plunge_law::{audit, Solid};

    let (world, mut h) = world_with("EM-0800-4FL");
    let part = cube(Point3::new(-10.0, -10.0, 0.0), Point3::new(10.0, 10.0, 18.0));
    h.execute(&world, ImportModel::from_mesh("part", part));
    let stock = Region::new(vec![Loop::new(vec![
        Point2::new(-30.0, -30.0),
        Point2::new(30.0, -30.0),
        Point2::new(30.0, 30.0),
        Point2::new(-30.0, 30.0),
    ])
    .expect("loop")])
    .expect("region");
    let solid = Solid { top: 20.0, footprint: Some(&stock) };
    for (top_z, stepdown, climb) in
        [(None, None, true), (None, None, false), (Some(Height::fixed(20.0)), Some(1.0), true)]
    {
        let params = OpParams::Face { holes: Default::default(),
            blend_turns: false,
            tilt: None,
            stepover: 4.0,
            clearance_z: Height::fixed(30.0),
            cut_z: Height::fixed(18.0),
            top_z,
            stepdown,
            feed: 1200.0,
            plunge_feed: 300.0,
            climb,
        };
        let on_part = |tool: &str| Operation { src: SrcRef::Entity("part".into()), ..op(tool, params.clone()) };
        let (cc, cc_w) = operation::generate(&world, &on_part("EM-0800-4FL"), "F").expect("cc face");
        // The centre-cutting path is the facer's own, byte for byte, and
        // its plain entry is the one that lands on the stock.
        let own = face(
            &Region::new(vec![Loop::new(vec![
                Point2::new(-10.0, -10.0),
                Point2::new(10.0, -10.0),
                Point2::new(10.0, 10.0),
                Point2::new(-10.0, 10.0),
            ])
            .expect("loop")])
            .expect("region"),
            &FaceParams { holes: Default::default(),
                tool_diameter: 8.0,
                stepover: 4.0,
                clearance_z: 30.0,
                cut_z: 18.0,
                top_z: top_z.map(|_| 20.0),
                stepdown,
                feed: 1200.0,
                plunge_feed: 300.0,
                climb,
                blend_turns: false,
            },
        )
        .expect("face");
        assert_eq!(cc.moves, own.moves, "centre-cutting output unchanged");
        assert!(audit(&cc, false, 4.0, &solid, "the facing entry").is_err(), "the plain entry is over stock");

        let (nc, nc_w) = operation::generate(&world, &on_part("NC"), "F").expect("the entry runs out to air");
        assert_eq!(audit(&nc, false, 4.0, &solid, "the facing entry"), Ok(()), "{climb} {top_z:?}");
        assert_eq!(nc.moves.len(), cc.moves.len(), "only the entry point moved");
        assert_ne!(nc.moves, cc.moves, "the entry moved");
        assert_eq!(nc_w, cc_w, "verifies as clean as the centre-cutting path");
        assert!(
            !nc_w.iter().any(|w| format!("{w:?}").to_lowercase().contains("gouge")),
            "no gouge: {nc_w:?}"
        );
    }
}

#[test]
fn a_drill_cycle_with_a_tool_that_is_not_centre_cutting_is_refused_by_name() {
    let (world, _h) = world_with("DR-0400");
    let drill = OpParams::Drill {
        clearance_z: Height::fixed(30.0),
        retract_z: None,
        top_z: Height::fixed(22.0),
        bottom_z: None,
        cycle: CycleChoice::Auto,
        peck: 0.0,
        dwell: 0.0,
        feed: 130.0,
        plunge_feed: 100.0,
        holes: vec![Hole { xy: Point2::new(10.0, 10.0), radius: 2.0, entry_z: 20.0, floor_z: 10.0, through: false, stepped: false }],
        cross: Vec::new(),
    };
    operation::generate(&world, &op("DR-0400", drill.clone()), "D").expect("a centre-cutting drill");
    let e = operation::generate(&world, &op("NC", drill), "D").expect_err("a plunge into solid");
    assert!(format!("{e:?}").contains(CENTER_CUT_REFUSAL), "refused, but not by the law: {e:?}");
}
