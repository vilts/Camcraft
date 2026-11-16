//! #1205: the containment kinds' hole-wall excuse measures a facet about
//! its hole's own axis. A flat whose corners stand on the hole's circle
//! passes inside it along its middle - an octagon's flats on r 5 mm stand
//! at 4.62 - and a tool cutting there is a gouge, not tessellation. The
//! review of PR #1099 read 0 blocking warnings for a thread mill cutting
//! 0.32 mm into such flats; a finely faceted round hole still verifies
//! clean, up to the sagitta the excuse exists for (`WALL_SAG`).
//!
//! Gate map (#1205): covers sim::verify (hole_wall, solid_hits_walled),
//! app::operation::generate (contain_into), app::operation::cross_part.
//! Lane shape (M4.4): runs in seconds.
//!
//! Provenance: the reviewer's probe on PR #1099, rebuilt over our own
//! plate fixture; chord sagitta `r (1 - cos(pi / n))` from elementary
//! geometry. No vendor material consulted.

mod common;

use common::solid_part::{circle, plate, rect, P2};

use camcraft_app::commands::{AddTool, CreateOperation, GenerateOperation, History, ImportModel, SetStock};
use camcraft_app::drilling::{CycleChoice, Hole};
use camcraft_app::geo2d::Point2;
use camcraft_app::geometry::{Mesh, Point3};
use camcraft_app::model;
use camcraft_app::operation::{Height, OpParams, SrcRef};
use camcraft_app::schema::Location;
use camcraft_app::stock::StockShape;
use camcraft_app::tools::{catalog, Tool};
use camcraft_app::machining2d::{thread_mill, ThreadHole, ThreadParams};
use camcraft_app::verify::{Cutter, HoleAxis, SolidLaw, Warning, WALL_SAG, solid_hits_walled};
use camcraft_app::world::new_world;

/// A 30 x 30 x [0, 20] plate with the polygon hole `hole` from the top,
/// blind at Z 8.
fn part(hole: &[P2]) -> Mesh {
    part_to(hole, 8.0)
}

/// The plate with its hole blind at `floor`.
fn part_to(hole: &[P2], floor: f64) -> Mesh {
    let mut f = Vec::new();
    plate(&rect(-15.0, -15.0, 15.0, 15.0), Some((hole, floor)), 0.0, 20.0, &mut f);
    Mesh::from_facets(f)
}

/// #992: the thread's tap-drill hole is modelled deeper than the
/// thread (floor 10), so a drill's 118-degree point stays under the mill.
const TAP_FLOOR: f64 = 4.0;

/// #992: a thread mill is not centre-cutting - a Drill operation ahead
/// opens the declared hole of `radius` to `TAP_FLOOR`, at the minor
/// diameter rounded to the tenth below.
fn drill_ahead(world: &flecs_ecs::prelude::World, h: &mut History, radius: f64) {
    let d = ((radius * 2.0 - 0.02) * 10.0).floor() / 10.0;
    let tool = Tool { diameter: d, shank_d: d, flute_len: 30.0, ..catalog_tool("DR-0600") };
    h.execute(world, AddTool::new(world, "TAP-DRILL", tool).expect("tap drill"));
    let hole = Hole { xy: Point2::new(0.0, 0.0), radius: d * 0.5, entry_z: 20.0, floor_z: TAP_FLOOR, through: false, stepped: false };
    let params = OpParams::Drill {
        clearance_z: Height::fixed(25.0),
        retract_z: None,
        top_z: Height::fixed(20.0),
        bottom_z: None,
        cycle: CycleChoice::Drill,
        peck: 0.0,
        dwell: 0.0,
        feed: 120.0,
        plunge_feed: 80.0,
        holes: vec![hole],
        cross: Vec::new(),
    };
    h.execute(world, CreateOperation::new(world, "TAP", "TAP-DRILL", SrcRef::Entity("part".into()), params).expect("create drill"));
    h.execute(world, GenerateOperation::new(world, "TAP").expect("generate drill"));
}

/// A regular `n`-gon with its corners on r `r` about the origin.
fn ngon(r: f64, n: usize) -> Vec<P2> {
    circle((0.0, 0.0), r, n)
}

/// A D-bore: a 96-gon on r 5 with the flat chord at Y 4 between
/// (3, 4) and (-3, 4) - every vertex above it dropped.
fn d_bore() -> Vec<P2> {
    let mut v: Vec<P2> = ngon(5.0, 96).into_iter().filter(|p| p.1 < 4.0 - 1e-9).collect();
    let at = v.iter().position(|p| p.0 < 0.0 && p.1 > 0.0).unwrap_or(v.len());
    v.splice(at..at, [(3.0, 4.0), (-3.0, 4.0)]);
    v
}

fn catalog_tool(name: &str) -> Tool {
    catalog().into_iter().find(|(n, _)| *n == name).map(|(_, t)| t).expect("catalog entry")
}

/// A 6 mm multi-form thread mill of pitch 1 (#894: the pitch is the
/// tool's own).
fn thread_tool() -> Tool {
    Tool { material_group: "steel".into(), ..camcraft_app::thread_mill::thread_mill(6.0, 1.0, 0.0, 25.0) }
}

fn hole(radius: f64) -> Hole {
    Hole { xy: Point2::new(0.0, 0.0), radius, entry_z: 20.0, floor_z: 10.0, through: false, stepped: false }
}

#[derive(Clone, Copy)]
enum Kind {
    Thread,
    Bore,
    Drill,
}

/// The blocking warnings of one op of `kind` on the declared hole of
/// `radius`, cut in the plate whose hole is `model_hole`.
fn verdict(model_hole: &[P2], kind: Kind, radius: f64) -> Vec<Warning> {
    let world = new_world();
    let mut h = History::default();
    let mesh = match kind {
        Kind::Thread => part_to(model_hole, TAP_FLOOR),
        _ => part(model_hole),
    };
    h.execute(&world, ImportModel::from_mesh("part", mesh));
    let loc = Location { pos: Point3::new(0.0, 0.0, -2.0), ..Default::default() };
    h.execute(&world, SetStock::new(&world, StockShape::Box { dx: 40.0, dy: 40.0, dz: 22.0 }, loc).expect("stock"));
    let (tool_name, tool) = match kind {
        Kind::Thread => ("TM-06", thread_tool()),
        Kind::Bore => ("EM-0600-4FL", catalog_tool("EM-0600-4FL")),
        Kind::Drill => {
            let mut d = catalog_tool("DR-0600");
            d.flute_len = 30.0; // long series: the hole runs 10 mm from the top
            ("DR-0600", d)
        }
    };
    if matches!(kind, Kind::Thread) {
        drill_ahead(&world, &mut h, radius);
    }
    h.execute(&world, AddTool::new(&world, tool_name, tool).expect("tool"));
    let holes = vec![hole(radius)];
    let params = match kind {
        Kind::Thread => OpParams::Thread {
            clearance_z: Height::fixed(25.0),
            retract_z: None,
            top_z: Height::fixed(20.0),
            bottom_z: None,
            infeed: 0.1,
            feed: 400.0,
            plunge_feed: 150.0,
            climb: true,
            holes,
            pitch: None,
            hand: camcraft_app::cl::ThreadHand::Right,
            external: false,
            passes: camcraft_app::machining2d::ThreadPasses::default(),
        },
        Kind::Bore => OpParams::Bore {
            links: camcraft_app::links::Links::PLANAR,
            clearance_z: Height::fixed(25.0),
            retract_z: None,
            top_z: Height::fixed(20.0),
            bottom_z: None,
            stock_to_leave: 0.0,
            stepdown: 5.0,
            feed: 500.0,
            plunge_feed: 150.0,
            climb: true,
            tolerance: 0.01,
            holes,
        },
        Kind::Drill => OpParams::Drill {
            clearance_z: Height::fixed(30.0),
            retract_z: None,
            top_z: Height::fixed(22.0),
            bottom_z: None,
            cycle: CycleChoice::Auto,
            peck: 0.0,
            dwell: 0.0,
            feed: 130.0,
            plunge_feed: 100.0,
            holes,
            cross: Vec::new(),
        },
    };
    h.execute(&world, CreateOperation::new(&world, "OP", tool_name, SrcRef::Entity("part".into()), params).expect("create"));
    h.execute(&world, GenerateOperation::new(&world, "OP").expect("generate"));
    let e = model::find_by_name(&world, "OP").expect("op");
    model::toolpath_of(&world, &e).expect("cached").warnings.into_iter().filter(Warning::blocks_post).collect()
}

fn gouges(w: &[Warning]) -> usize {
    w.iter().filter(|w| matches!(w, Warning::Gouge { .. })).count()
}

/// The n-gon on r `r` whose flats' sagitta is the nearest above
/// (`over`) or below `WALL_SAG`.
fn ngon_at_cap(r: f64, over: bool) -> (Vec<P2>, f64) {
    let sag = |n: usize| r * (1.0 - (std::f64::consts::PI / n as f64).cos());
    let n = (3..4096).find(|&n| sag(n) <= WALL_SAG).expect("a fine enough n-gon");
    let n = if over { n - 1 } else { n };
    (ngon(r, n), sag(n))
}

/// The thread mill's solid law alone, as `contain_into` runs it: on
/// main every model-sourced Thread op is already refused by its region
/// law (the helix cuts past the declared minor circle by design, #1071,
/// which PR #1099 grants), so the op's verdict cannot show what the solid
/// law excuses. The program is the generator's own helix about the
/// declared hole of `minor` radius; the gouges are the solid law's.
fn thread_solid(model_hole: &[P2], minor: f64) -> usize {
    let path = thread_mill(&ThreadParams {
        holes: vec![ThreadHole { xy: Point2::new(0.0, 0.0), radius: minor, top_z: 20.0, floor_z: 10.0, stepped: false, through: false }],
        pitch: 1.0,
        tool_diameter: 6.0,
        // #894: a 25 mm multi-form mill of pitch 1 carries 25 rows.
        rows: 25,
        infeed: 0.1,
        external: false,
        passes: camcraft_app::machining2d::ThreadPasses::default(),
        hand: camcraft_app::cl::ThreadHand::Right,
        clearance_z: 25.0,
        r_z: None,
        retract_z: None,
        feed: 400.0,
        plunge_feed: 150.0,
        climb: true,
    })
    .expect("thread mill");
    let law = SolidLaw { step: 0.25, allow: 0.01, hole_walls: true };
    let mesh = part(model_hole);
    let w = solid_hits_walled(&path, &mesh.facets, &Cutter::of(&thread_tool()), &law, &[HoleAxis::vertical(0.0, 0.0)]);
    gouges(&w)
}

#[test]
fn a_thread_mill_cutting_into_an_octagons_flats_is_a_gouge() {
    // The reviewer's probe: corners on r 5, flats at 4.62; the thread
    // declared at r 4.4 is cut to its major circle 4.94, 0.32 mm past
    // the flats. Before #1205 the solid law excused all of it.
    let n = thread_solid(&ngon(5.0, 8), 4.4);
    eprintln!("thread solid law on octagon r5 at 4.4: {n} gouges");
    assert!(n > 0, "the flats are cut");
    // The D-bore's flat stands 0.94 mm inside the major circle.
    let n = thread_solid(&d_bore(), 4.4);
    eprintln!("thread solid law on D-bore at 4.4: {n} gouges");
    assert!(n > 0, "the D-flat is cut");
    // A 32-gon at the major circle (4.4 + 0.5413 x 1) is the thread's
    // own round wall: nothing to excuse but its 0.024 mm sagitta.
    assert_eq!(thread_solid(&ngon(4.4 + 0.5413, 32), 4.4), 0, "thread at its major circle");
    // The op itself is refused on main either way - by its region law.
    let w = verdict(&ngon(5.0, 8), Kind::Thread, 4.4);
    assert!(gouges(&w) > 0, "{w:?}");
}

#[test]
fn a_bore_or_drill_into_an_octagons_flats_is_a_gouge() {
    let w = verdict(&ngon(5.0, 8), Kind::Bore, 5.0);
    eprintln!("bore on octagon r5 at 5: {} gouges of {w:?}", gouges(&w));
    assert!(gouges(&w) > 0, "0.38 mm into each flat: {w:?}");
    let w = verdict(&d_bore(), Kind::Bore, 5.0);
    eprintln!("bore on D-bore at 5: {} gouges", gouges(&w));
    assert!(gouges(&w) > 0, "1 mm into the D-flat: {w:?}");
    // A 6 mm drill in an octagon on r 3: flats at 2.77.
    let w = verdict(&ngon(3.0, 8), Kind::Drill, 3.0);
    eprintln!("drill on octagon r3: {} gouges of {w:?}", gouges(&w));
    assert!(gouges(&w) > 0, "0.23 mm into each flat: {w:?}");
}

#[test]
fn a_round_tool_in_a_finely_faceted_round_hole_still_verifies_clean() {
    // A 32-gon on r 5 stands 0.024 mm inside its circle - past the
    // 0.01 gouge tolerance, inside the wall excuse.
    assert_eq!(verdict(&ngon(5.0, 32), Kind::Bore, 5.0), Vec::new(), "bore at the hole's size");
    assert_eq!(verdict(&ngon(3.0, 32), Kind::Drill, 3.0), Vec::new(), "drill at the hole's size");
    // An octagon bored to its flats (r 4.6 < 4.62) is no gouge either.
    assert_eq!(verdict(&ngon(5.0, 8), Kind::Bore, 4.6), Vec::new(), "bored inside the flats");
}

#[test]
fn the_wall_excuse_ends_at_its_stated_sagitta() {
    // The finest n-gon on r 5 whose flats stand deeper than `WALL_SAG`
    // convicts a bore at its corners' circle; the next one is round.
    let (round, s_in) = ngon_at_cap(5.0, false);
    let (flat, s_out) = ngon_at_cap(5.0, true);
    eprintln!("boundary: {} sides sag {s_in:.4} / {} sides sag {s_out:.4}", round.len(), flat.len());
    assert!(s_in <= WALL_SAG && s_out > WALL_SAG);
    assert_eq!(verdict(&round, Kind::Bore, 5.0), Vec::new(), "{} sides, sag {s_in}", round.len());
    let w = verdict(&flat, Kind::Bore, 5.0);
    assert!(gouges(&w) > 0, "{} sides, sag {s_out}: {w:?}", flat.len());
}


#[test]
fn a_thread_op_in_a_finely_faceted_round_hole_stays_clean() {
    // #894 x #1205: the crest allowance past the model wall holds where
    // the hole is a tessellated circle - a 96-gon at the minor r 4.4
    // (sagitta 0.002 mm) - so the thread's own cut is no gouge there,
    // while the octagon above convicts.
    let w = verdict(&ngon(4.4, 96), Kind::Thread, 4.4);
    assert_eq!(gouges(&w), 0, "{w:?}");
}
