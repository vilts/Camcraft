//! The documents of the carried-stock suites, built once for the engine
//! agreement lane (#637): `setup_carry`'s faced block (flip, quarter
//! turn, "Same as", clip, new block, a flip off the grid pitch),
//! `rest_across_setups`'s plate with a pocket, `carried_verify`'s contour
//! beside a part. Mirrors of those suites' own fixtures - a change there
//! is mirrored here.
//! Provenance: our own fixtures; no vendor material consulted.

use flecs_ecs::prelude::World;

use camcraft_app::commands::{
    AddTool, AlignPartToSetup, CreateOperation, CreateSetup, GenerateOperation, History, ImportModel, SetSetupRelative,
    SetSetupStockSource, SetStock,
};
use camcraft_app::geometry::{cube, Mesh, Point3, Triangle};
use camcraft_app::machining2d::{Lead, PocketStrategy, Side};
use camcraft_app::machining3d::Entry3D;
use camcraft_app::operation::{Height, OpParams, SrcRef};
use camcraft_app::schema::Location;
use camcraft_app::setup_align::AlignPreset;
use camcraft_app::setup_stock::StockSource;
use camcraft_app::stock::StockShape;
use camcraft_app::tools::catalog;
use camcraft_app::world::new_world;

/// How Setup2 is entered from Setup1.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Second {
    /// Same hold, not linked: the M3.2 A clip.
    Clip,
    /// Placed "Same as" Setup1: the identity carry.
    SameAs,
    /// The part flipped over X: carried upside down.
    FlipX,
    /// The part turned 90 degrees about Z.
    TurnZ90,
    /// Flipped onto its own new block (#320).
    FlipOwn,
}

/// One document of the lane.
pub struct Doc {
    pub name: String,
    pub world: World,
    /// Kept alive with the world (the commands own undo state).
    pub _history: History,
    /// The consumer in Setup2 whose rest and verify reads are compared:
    /// its name and its tool radius.
    pub consumer: Option<(&'static str, f64)>,
}

fn tool(world: &World, h: &mut History, name: &str) {
    let t = catalog().into_iter().find(|(n, _)| *n == name).map(|(_, t)| t).expect("catalog tool");
    h.execute(world, AddTool::new(world, name, t).expect("add tool"));
}

fn add(world: &World, h: &mut History, name: &str, tool: &str, src: SrcRef, params: OpParams) {
    h.execute(world, CreateOperation::new(world, name, tool, src, params).expect("create"));
    h.execute(world, GenerateOperation::new(world, name).expect("generate"));
}

fn second_setup(world: &World, h: &mut History, second: Second) {
    h.execute(world, CreateSetup::new(world, None, Some("Setup1")).expect("Setup2"));
    match second {
        Second::Clip => {}
        Second::SameAs => h.execute(world, SetSetupRelative::link(world, "Setup2", "Setup1", [0.0; 3]).expect("same as")),
        Second::FlipX | Second::FlipOwn => {
            h.execute(world, AlignPartToSetup::preset(world, "Setup2", AlignPreset::FlipX).expect("flip"));
        }
        Second::TurnZ90 => h.execute(world, AlignPartToSetup::preset(world, "Setup2", AlignPreset::TurnZ90).expect("turn")),
    }
}

fn face(cut_z: f64, stepover: f64) -> OpParams {
    OpParams::Face {
        holes: Default::default(),
        tilt: None,
        blend_turns: false,
        stepover,
        clearance_z: Height::fixed(30.0),
        cut_z: Height::fixed(cut_z),
        top_z: None,
        stepdown: None,
        feed: 1200.0,
        plunge_feed: 300.0,
        climb: true,
    }
}

const FACE_TOOL: &str = "EM-0400-2FL";

/// `setup_carry`: a part centred at `c`, `2*hx` x `2*hy` x [0, 16], in a
/// `dx` x `dy` block Z -2..18; Face1 to `cut1` in Setup1, Setup2 entered
/// as `second`, Face2 to 16.
pub fn faced(name: &str, c: (f64, f64), (hx, hy): (f64, f64), (dx, dy): (f64, f64), cut1: f64, second: Second) -> Doc {
    let world = new_world();
    let mut h = History::default();
    let part = cube(Point3::new(c.0 - hx, c.1 - hy, 0.0), Point3::new(c.0 + hx, c.1 + hy, 16.0));
    h.execute(&world, ImportModel::from_mesh("part", part));
    let at = Location { pos: Point3::new(0.0, 0.0, -2.0), ..Default::default() };
    h.execute(&world, SetStock::new(&world, StockShape::Box { dx, dy, dz: 20.0 }, at).expect("stock"));
    tool(&world, &mut h, FACE_TOOL);
    h.execute(&world, CreateSetup::new(&world, None, None).expect("materialize Setup1"));
    add(&world, &mut h, "Face1", FACE_TOOL, SrcRef::Stock, face(cut1, 3.0));
    second_setup(&world, &mut h, second);
    add(&world, &mut h, "Face2", FACE_TOOL, SrcRef::Stock, face(16.0, 3.0));
    if second == Second::FlipOwn {
        h.execute(&world, SetSetupStockSource::new(&world, "Setup2", StockSource::Own).expect("own block"));
        h.execute(&world, GenerateOperation::new(&world, "Face2").expect("Face2 on its own block"));
    }
    Doc { name: name.to_owned(), world, _history: h, consumer: Some(("Face2", 2.0)) }
}

/// A 40x40x20 plate with a 20x20 pocket sunk to z=10.
fn plate_with_pocket() -> Mesh {
    let mut facets = Vec::new();
    let outer = cube(Point3::new(-20.0, -20.0, 0.0), Point3::new(20.0, 20.0, 20.0));
    facets.extend(outer.facets[0..2].iter().cloned());
    facets.extend(outer.facets[4..12].iter().cloned());
    let inner = cube(Point3::new(-10.0, -10.0, 10.0), Point3::new(10.0, 10.0, 20.0));
    facets.extend(inner.facets[0..2].iter().cloned());
    facets.extend(inner.facets[4..12].iter().cloned());
    let o = [(-20.0, -20.0), (20.0, -20.0), (20.0, 20.0), (-20.0, 20.0)].map(|(x, y)| Point3::new(x, y, 20.0));
    let i = [(-10.0, -10.0), (10.0, -10.0), (10.0, 10.0), (-10.0, 10.0)].map(|(x, y)| Point3::new(x, y, 20.0));
    for k in 0..4 {
        let (a, b, c, d) = (o[k], o[(k + 1) % 4], i[(k + 1) % 4], i[k]);
        facets.push(Triangle::new(a, b, c));
        facets.push(Triangle::new(a, c, d));
    }
    Mesh::from_facets(facets)
}

fn pocket(bottom_z: f64, stepover: f64, rest_rough: bool) -> OpParams {
    OpParams::Pocket3D {
        links: camcraft_app::links::Links::ROUGH3D,
        stepover,
        stock_to_leave: 0.0,
        clearance_z: Height::fixed(30.0),
        retract_z: None,
        top_z: Height::fixed(20.0),
        bottom_z: Height::fixed(bottom_z),
        stepdown: 5.0,
        feed: 1500.0,
        plunge_feed: 400.0,
        climb: true,
        strategy: PocketStrategy::Zigzag,
        entry: Entry3D::Plunge,
        helix_radius: 1.0,
        rest_rough,
        optimal_load: camcraft_app::load::load_of_degrees(60.0),
        feed_opt: false,
        shell: false,
    }
}

/// `rest_across_setups`: the plate roughed with the 8 mm tool in Setup1,
/// the 4 mm rest pass in Setup2 entered as `second`.
pub fn rest_plate(name: &str, second: Second) -> Doc {
    let world = new_world();
    let mut h = History::default();
    for t in ["EM-0800-4FL", "EM-0400-2FL"] {
        tool(&world, &mut h, t);
    }
    let stock = SetStock::new(&world, StockShape::Box { dx: 40.0, dy: 40.0, dz: 20.0 }, Location::default());
    h.execute(&world, stock.expect("box stock"));
    h.execute(&world, ImportModel::from_mesh("plate", plate_with_pocket()));
    h.execute(&world, CreateSetup::new(&world, None, None).expect("materialize Setup1"));
    let src = || SrcRef::Entity("plate".into());
    add(&world, &mut h, "Rough1", "EM-0800-4FL", src(), pocket(12.0, 4.0, false));
    second_setup(&world, &mut h, second);
    add(&world, &mut h, "Rest2", "EM-0400-2FL", src(), pocket(10.0, 2.0, true));
    Doc { name: name.to_owned(), world, _history: h, consumer: Some(("Rest2", 2.0)) }
}

fn contour() -> OpParams {
    OpParams::Contour {
        tilt: None,
        stock_to_leave: 0.0,
        clearance_z: Height::fixed(12.0),
        retract_z: Height::fixed(8.0),
        top_z: Height::fixed(5.0),
        bottom_z: Height::fixed(0.0),
        stepdown: 2.5,
        feed: 800.0,
        plunge_feed: 200.0,
        climb: true,
        side: Side::Auto,
        lead: Lead::None,
        tolerance: 0.01,
        tab_count: 0,
        tab_width: 0.0,
        tab_height: 0.0,
        entry: Entry3D::Plunge,
        helix_radius: 1.0,
        blend_turns: false,
        links: camcraft_app::links::Links::PLANAR,
    }
}

const CONTOUR_TOOL: &str = "EM-0600-4FL";

/// `carried_verify`: the part (0..10)^2 x [0, 5] in Setup1's `w1`-wide
/// block Z -5..10 faced to `cut1`; Setup2 entered as `second`, declaring
/// a `w2`-wide block Z -5..5, and Contour2 round the part.
pub fn verify_contour(name: &str, w1: f64, cut1: f64, second: Second, w2: f64) -> Doc {
    let world = new_world();
    let mut h = History::default();
    let block = |world: &World, w: f64, dz: f64| {
        let at = Location { pos: Point3::new(5.0, 5.0, -5.0), ..Default::default() };
        SetStock::new(world, StockShape::Box { dx: w, dy: w, dz }, at).expect("stock")
    };
    h.execute(&world, ImportModel::from_mesh("part", cube(Point3::ZERO, Point3::new(10.0, 10.0, 5.0))));
    h.execute(&world, block(&world, w1, 15.0));
    tool(&world, &mut h, CONTOUR_TOOL);
    h.execute(&world, CreateSetup::new(&world, None, None).expect("materialize Setup1"));
    add(&world, &mut h, "Face1", CONTOUR_TOOL, SrcRef::Stock, face(cut1, 4.0));
    second_setup(&world, &mut h, second);
    h.execute(&world, block(&world, w2, 10.0));
    add(&world, &mut h, "Contour2", CONTOUR_TOOL, SrcRef::Entity("part".into()), contour());
    Doc { name: name.to_owned(), world, _history: h, consumer: Some(("Contour2", 3.0)) }
}

/// Every document of the lane.
pub fn documents() -> Vec<Doc> {
    let sq = (30.0, 30.0);
    vec![
        // setup_carry
        faced("carry/flip, faced clean", (0.0, 0.0), sq, (60.0, 60.0), 16.0, Second::FlipX),
        faced("carry/flip after a 1 mm gouge", (0.0, 0.0), sq, (60.0, 60.0), 15.0, Second::FlipX),
        faced("carry/quarter turn", (0.0, 0.0), (30.0, 20.0), (60.0, 40.0), 16.0, Second::TurnZ90),
        faced("carry/same as", (0.0, 0.0), sq, (60.0, 60.0), 17.0, Second::SameAs),
        faced("carry/clip in the same hold", (0.0, 0.0), sq, (60.0, 60.0), 17.0, Second::Clip),
        faced("carry/flip onto a new block", (0.0, 0.0), sq, (60.0, 60.0), 16.0, Second::FlipOwn),
        faced("carry/flip about a centre off the pitch", (0.37, -0.21), (20.0, 20.0), (61.0, 47.0), 16.0, Second::FlipX),
        // rest_across_setups
        rest_plate("rest/same as", Second::SameAs),
        rest_plate("rest/clip", Second::Clip),
        // carried_verify
        verify_contour("verify/same as, material left standing", 100.0, 9.0, Second::SameAs, 100.0),
        verify_contour("verify/same as, faced away", 100.0, 5.0, Second::SameAs, 100.0),
        verify_contour("verify/flip", 100.0, 9.0, Second::FlipX, 100.0),
        verify_contour("verify/declared stock beyond the carried block", 20.0, 9.0, Second::SameAs, 100.0),
        verify_contour("verify/clip", 100.0, 9.0, Second::Clip, 100.0),
    ]
}
