//! M7.2 B3 (app half): `stock_view::field_view` on a live world. The
//! program walks the FIELD arm from the same plan the column verdict
//! walks (`plan_program`), the stock is the entity's own mesh at its
//! placement, and the eye that looks at it names the same verdicts the
//! column census names on the M2.3 fixtures - faced to 18 over a model
//! top of 18 (clean), 20 (gouge), 16 (rest). Refusals name their law.
//! Provenance: our own fixtures over the M7.1 field engine; nothing
//! from vendor binaries.
//!
//! Gate map (M4.4): covers app::stock_view, app::stock_sim, sim::stock_sim (plan_program, part_facets), app::commands, app::model.

use flecs_ecs::prelude::World;

use camcraft_app::commands::{AddTool, CreateOperation, GenerateOperation, History, ImportModel, SetStock};
use camcraft_app::geometry::{cube, quat_from_yaw, Point3, IDENTITY_Q};
use camcraft_app::model;
use camcraft_app::operation::{Height, OpParams, SrcRef};
use camcraft_app::schema::{Location, STOCK_NAME};
use camcraft_app::stock::StockShape;
use camcraft_app::stock_sim::{PreviewCamera, DEV_TOL};
use camcraft_app::stock_view::{field_view, field_view_with, subdivide};

/// Half the fixture's footprint. The M2.3 box is 60x60; the display
/// cut's cost is the Face program's swept area (~16 s a document in
/// debug at 60), so the eye suite faces a 20x20 box - same heights,
/// same cell (COARSE scales with the volume), a ninth of the area.
const HALF: f64 = 10.0;

/// A deliberately coarse display budget (~1.5 mm cells: the fixture's
/// volume over 3.6 mm^3 a texel): the wiring and the classes are the
/// law under test, not resolution - and a debug build cuts it in
/// seconds, not minutes.
const COARSE: f64 = (2.0 * HALF) * (2.0 * HALF) * 20.0 / 3.6;
use camcraft_app::tools::catalog;
use camcraft_app::world::new_world;

fn catalog_tool(name: &str) -> camcraft_app::tools::Tool {
    catalog().into_iter().find(|(n, _)| *n == name).map(|(_, t)| t).expect("catalog entry")
}

/// The M2.3 fixture shrunk to 20x20 (see `HALF`): a box stock at `at`, a model box of
/// `model_top` on the same footprint, one Face op to z 18 - generated.
fn faced_doc(at: Point3, model_top: f64) -> (World, History) {
    let world = new_world();
    let mut history = History::default();
    let loc = Location { pos: at, rot: IDENTITY_Q };
    let cmd = SetStock::new(&world, StockShape::Box { dx: 2.0 * HALF, dy: 2.0 * HALF, dz: 20.0 }, loc);
    history.execute(&world, cmd.expect("box stock"));
    let cmd = AddTool::new(&world, "EM-0400-2FL", catalog_tool("EM-0400-2FL"));
    history.execute(&world, cmd.expect("catalog tool"));
    let lo = Point3::new(at.x - HALF, at.y - HALF, 0.0);
    let hi = Point3::new(at.x + HALF, at.y + HALF, model_top);
    history.execute(&world, ImportModel::from_mesh("part", cube(lo, hi)));
    let params = OpParams::Face { holes: Default::default(),
        tilt: None,
        stepover: 3.0,
        clearance_z: Height::fixed(25.0),
        cut_z: Height::fixed(18.0),
        top_z: None,
        stepdown: None,
        feed: 1200.0,
        plunge_feed: 300.0,
        climb: true,
        blend_turns: false,
    };
    let cmd = CreateOperation::new(&world, "Face1", "EM-0400-2FL", SrcRef::Stock, params);
    history.execute(&world, cmd.expect("create face"));
    history.execute(&world, GenerateOperation::new(&world, "Face1").expect("generate face"));
    (world, history)
}

/// Straight down the stock's axis at `c`: the centre pixel of a 33x33
/// frame is the ray through `c`.
fn look_down(c: Point3) -> PreviewCamera {
    PreviewCamera::look_at(
        Point3::new(c.x, c.y, 60.0),
        Point3::new(c.x, c.y, 0.0),
        Point3::new(0.0, 1.0, 0.0),
        0.5,
    )
    .expect("a sane camera")
}

const W: u32 = 33;
const CENTRE: usize = 16 * 33 + 16;

#[test]
fn the_eye_lands_on_the_faced_floor_and_names_its_class() {
    for (model_top, class, word) in [(18.0, 0.0, "clean"), (20.0, 2.0, "gouge"), (16.0, 1.0, "rest")] {
        let (world, _h) = faced_doc(Point3::ZERO, model_top);
        let v = field_view_with(&world, COARSE).expect("a faced box draws");
        assert!(v.target.is_some(), "a closed model box signs: {:?}", v.target_note);
        let pv = v.stock.preview(v.target.as_deref(), &look_down(Point3::ZERO), W, W, DEV_TOL).expect("frame");
        let z = 60.0 - pv.depth[CENTRE];
        assert!((z - 18.0).abs() <= v.cell.max(0.25), "the faced floor stands at 18, the eye saw {z} (cell {})", v.cell);
        assert_eq!(pv.class[CENTRE], class, "model top {model_top} reads {word}");
    }
}

#[test]
fn the_stock_stands_where_the_entity_stands() {
    let at = Point3::new(100.0, 50.0, 0.0);
    let (world, _h) = faced_doc(at, 18.0);
    let v = field_view_with(&world, COARSE).expect("a placed box draws");
    let pv = v.stock.preview(v.target.as_deref(), &look_down(at), W, W, DEV_TOL).expect("frame");
    assert!(pv.depth[CENTRE].is_finite(), "the eye finds the stock at its placement");
    let away = v.stock.preview(None, &look_down(Point3::ZERO), W, W, DEV_TOL).expect("frame");
    assert!(away.depth[CENTRE].is_nan(), "and nothing where it used to be");
}

#[test]
fn the_field_view_refuses_what_it_cannot_draw_by_name() {
    let err = field_view(&new_world()).expect_err("an empty document has no program");
    assert!(!err.is_empty());
    let (world, mut history) = faced_doc(Point3::ZERO, 18.0);
    let stock = model::find_by_name(&world, STOCK_NAME).expect("stock");
    model::set_transform(&world, &stock, Location { pos: Point3::ZERO, rot: quat_from_yaw(0.7) });
    history.execute(&world, GenerateOperation::new(&world, "Face1").expect("regenerate rotated"));
    let err = field_view(&world).expect_err("a rotated stock cannot be drawn on the lattice");
    assert!(err.contains("axis-aligned"), "{err}");
}

#[test]
fn subdivision_keeps_the_shell_and_bounds_the_edges() {
    let c = cube(Point3::ZERO, Point3::new(60.0, 60.0, 20.0));
    let s = subdivide(&c.facets, 0.25);
    assert_eq!(s.len() % c.facets.len(), 0, "every facet splits into the same power of four");
    assert!(s.len() > c.facets.len(), "a 60 mm face at 0.25 mm cells must split");
    let longest = s
        .iter()
        .flat_map(|t| [(t.a, t.b), (t.b, t.c), (t.c, t.a)])
        .map(|(p, q)| ((p.x - q.x).powi(2) + (p.y - q.y).powi(2) + (p.z - q.z).powi(2)).sqrt())
        .fold(0.0f64, f64::max);
    assert!(longest <= 32.0 * 0.25 + 1e-9, "edges bounded to 32 cells: {longest}");
    let area = |f: &[camcraft_app::geometry::Triangle]| {
        f.iter()
            .map(|t| {
                let (u, w) = (t.b - t.a, t.c - t.a);
                u.cross(w).dot(u.cross(w)).sqrt() * 0.5
            })
            .sum::<f64>()
    };
    assert!((area(&s) - area(&c.facets)).abs() < 1e-6, "subdivision changes no area");
}
