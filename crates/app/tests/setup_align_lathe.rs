//! #2287: part alignment on a lathe - all 12 turning kinds, axial drill,
//! cross drill (`detect_holes`), OP20 end-for-end flip, and spindle-axis
//! equivalence against the baked mesh twin.
//!
//! Provenance: our own synthetic revolved bars and `cross_support`'s
//! drilled bar; no vendor material consulted.
//! Gate map (#2287): covers app::setup_align, app::operation (job,
//! generate, material, spindle_law, cross_spindle, turning_*),
//! app::posting, app::stock_sim_lathe.

mod cross_support;
mod punched_bar;

use flecs_ecs::prelude::World;

use camcraft_app::cl::Toolpath;
use camcraft_app::commands::{
    AddTool, AlignPartToSetup, CreateOperation, CreateSetup, GenerateOperation, History,
    ImportModel, LoadMachine, SelectPost, SetStock,
};
use camcraft_app::drilling::{CrossHole, CycleChoice, Hole};
use camcraft_app::geo2d::Point2;
use camcraft_app::geometry::{Mesh, Point3, Triangle, transform_point};
use camcraft_app::machine::{Coolant, Machine};
use camcraft_app::model;
use camcraft_app::operation::{
    self, Height, OpParams, OpStatus, SrcRef,
};
use camcraft_app::posting;
use camcraft_app::schema::{Location, PostRef};
use camcraft_app::setup_align;
use camcraft_app::setup_turn::TurnPivot;
use camcraft_app::stock::StockShape;
use camcraft_app::tools::{Tool, catalog};
use camcraft_app::turning::{DepthSchedule, InfeedMethod, ThreadInfeed};
use camcraft_app::world::new_world;

const TURN: [f64; 3] = [90.0, 0.0, 90.0];
const FLIP: [f64; 3] = [180.0, 0.0, 90.0];
const TAU: f64 = std::f64::consts::TAU;

fn catalog_tool(name: &str) -> Tool {
    catalog().into_iter().find(|(n, _)| *n == name).map(|(_, t)| t).expect("catalog tool")
}

fn lathe() -> Machine {
    Machine {
        model: "TestBench lathe".into(),
        vendor: "Camcraft Testbench".into(),
        kind: Some("lathe".into()),
        spindle_min: Some(25.0),
        spindle_max: Some(12000.0),
        coolants: vec![Coolant { name: "Flood".into(), m_code: "M8".into() }],
        ..Default::default()
    }
}

fn revolve(meridian: &[(f64, f64)]) -> Mesh {
    let pt = |j: usize, r: f64, z: f64| {
        let th = TAU * j as f64 / 16.0;
        Point3::new(r * th.cos(), r * th.sin(), z)
    };
    let mut f = Vec::new();
    for seg in meridian.windows(2) {
        let ((ra, za), (rb, zb)) = (seg[0], seg[1]);
        for j in 0..16 {
            let (a1, a2, b1, b2) = (pt(j, ra, za), pt(j + 1, ra, za), pt(j, rb, zb), pt(j + 1, rb, zb));
            if ra <= 1e-12 {
                f.push(Triangle::new(a1, b2, b1));
            } else if rb <= 1e-12 {
                f.push(Triangle::new(a1, a2, b2));
            } else {
                f.push(Triangle::new(a1, a2, b2));
                f.push(Triangle::new(a1, b2, b1));
            }
        }
    }
    Mesh::from_facets(f)
}

fn unturn(mesh: &Mesh) -> Mesh {
    let mv = |p: Point3| Point3::new(p.y, p.z, p.x);
    Mesh::from_facets(mesh.facets.iter().map(|t| Triangle::new(mv(t.a), mv(t.b), mv(t.c))).collect())
}

fn bake(mesh: &Mesh, loc: Location) -> Mesh {
    let mv = |p: Point3| transform_point(p, loc.pos, loc.rot);
    Mesh::from_facets(mesh.facets.iter().map(|t| Triangle::new(mv(t.a), mv(t.b), mv(t.c))).collect())
}

fn assert_moves_eq(a: &Toolpath, b: &Toolpath) {
    assert_eq!(a.moves.len(), b.moves.len(), "move count");
    for (i, (ma, mb)) in a.moves.iter().zip(&b.moves).enumerate() {
        assert_eq!(ma.kind, mb.kind, "move {i} kind");
        let eq = |u: Point3, v: Point3| (u.x - v.x).abs() < 1e-6 && (u.y - v.y).abs() < 1e-6 && (u.z - v.z).abs() < 1e-6;
        assert!(eq(ma.from, mb.from) && eq(ma.to, mb.to), "move {i}: {:?}->{:?} vs {:?}->{:?}", ma.from, ma.to, mb.from, mb.to);
    }
}

fn path_of(w: &World, name: &str) -> Toolpath {
    let e = model::find_by_name(w, name).expect("op");
    assert_eq!(camcraft_app::operation::status_of(w, &e), OpStatus::Ok, "{name}");
    model::toolpath_of(w, &e).expect("cached").path
}

fn run_case(
    raw: Mesh,
    pivot: TurnPivot,
    stock: StockShape,
    stock_at: Location,
    tools: &[(&str, Option<Tool>)],
    ops: Vec<(&str, &str, OpParams)>,
    post_nc: bool,
) {
    let build = |mesh: Mesh, align: Option<[f64; 3]>| -> (World, Result<Vec<String>, String>, Option<Mesh>) {
        let w = new_world();
        let mut h = History::default();
        h.execute(&w, LoadMachine::new(&w, "lathe.mch".into(), Some(lathe())).expect("lathe"));
        h.execute(&w, SelectPost::new(&w, PostRef::Named("Fanuc Lathe".into())).expect("post"));
        for (name, custom) in tools {
            let tool = custom.clone().unwrap_or_else(|| catalog_tool(name));
            h.execute(&w, AddTool::new(&w, name, tool).expect("tool"));
        }
        h.execute(&w, ImportModel::from_mesh("bar", mesh.clone()));
        h.execute(&w, CreateSetup::new(&w, None, None).expect("Setup1"));
        let baked = align.map(|deg| {
            h.execute(&w, AlignPartToSetup::turn(&w, "Setup1", deg, pivot).expect("align"));
            let loc = setup_align::stored(&w, "Setup1").expect("stored");
            bake(&mesh, loc)
        });
        h.execute(&w, SetStock::new(&w, stock.clone(), stock_at).expect("stock"));
        let mut names = Vec::new();
        for (oname, tool, params) in &ops {
            h.execute(&w, CreateOperation::new(&w, oname, tool, SrcRef::Entity("bar".into()), params.clone()).expect("op"));
            if let Err(err) = GenerateOperation::new(&w, oname).map(|g| h.execute(&w, g)) {
                return (w, Err(err.to_string()), baked);
            }
            names.push((*oname).to_string());
        }
        (w, Ok(names), baked)
    };
    let (tw, tres, baked) = build(raw.clone(), Some(TURN));
    let names = tres.expect("turned ops generate");
    let (bw, bres, _) = build(baked.expect("baked"), None);
    bres.expect("baked ops generate");
    for name in &names {
        assert_moves_eq(&path_of(&tw, name), &path_of(&bw, name));
    }
    if post_nc {
        assert_eq!(
            posting::post_setup_unwritten(&tw).expect("post turned").program,
            posting::post_setup_unwritten(&bw).expect("post baked").program
        );
    }
    let (_, ures, _) = build(raw, None);
    assert!(ures.is_err(), "unaligned turned-aside bar should be refused on spindle_law");
}

#[test]
fn lathe_facing_rough_finish_chamfer_and_axial_drill_match_baked() {
    let bar = revolve(&[(0.0, -20.0), (10.0, -20.0), (10.0, -2.0), (8.0, -2.0), (8.0, 18.0), (6.0, 20.0), (2.05, 20.0), (2.05, 10.0), (0.0, 10.0)]);
    let raw = unturn(&bar);
    let stock = StockShape::Cylinder { diameter: 20.0, height: 42.0 };
    let at = Location { pos: Point3::new(0.0, 0.0, -20.0), ..Default::default() };
    let hole = Hole { xy: Point2::new(0.0, 0.0), radius: 2.05, entry_z: 20.0, floor_z: 10.0, through: false, stepped: false };
    let ops = vec![
        ("FC1", "TN-1600-0R8", OpParams::Facing {
            tolerance: 0.05, clearance_z: Height::fixed(26.0),
            top_z: Height::fixed(22.0), bottom_z: Height::fixed(20.0), stepdown: 1.0, feed: 150.0, plunge_feed: 80.0,
        }),
        ("AX1", "DR-0400", OpParams::Drill {
            break_through: None, break_through_from: Default::default(), clearance_z: Height::fixed(30.0), retract_z: None,
            top_z: Height::fixed(20.05), bottom_z: None, cycle: CycleChoice::Auto,
            peck: 0.0, dwell: 0.0, feed: 120.0, plunge_feed: 80.0, holes: vec![hole], cross: vec![],
        }),
        ("PR1", "TN-1600-0R8", OpParams::ProfileRough {
            tolerance: 0.05, clearance_z: Height::fixed(25.0),
            top_z: Height::fixed(20.0), bottom_z: Height::fixed(-2.0), stepdown: 1.0, stock_to_leave: 0.25, feed: 150.0, plunge_feed: 80.0,
        }),
        ("PF1", "TN-1600-0R8", OpParams::ProfileFinish {
            tolerance: 0.05, clearance_z: Height::fixed(25.0),
            top_z: Height::fixed(18.0), bottom_z: Height::fixed(-2.0), feed: 120.0, plunge_feed: 60.0,
        }),
        ("CH1", "TN-1600-0R8", OpParams::Chamfering {
            tolerance: 0.05, clearance_z: Height::fixed(25.0),
            top_z: Height::fixed(20.0), bottom_z: Height::fixed(18.0), angle_deg: 45.0, feed: 150.0, plunge_feed: 50.0,
        }),
    ];
    run_case(raw, TurnPivot::PartCentre, stock, at, &[("TN-1600-0R8", None), ("DR-0400", None)], ops, true);
}

#[test]
fn lathe_groove_part_thread_taper_multipass_and_form_match_baked() {
    // 1. GrooveRough + GrooveFinish + Part
    let grooved = revolve(&[(0.0, -16.0), (6.0, -16.0), (6.0, -6.8), (4.0, -6.8), (4.0, -3.6), (6.0, -3.6), (6.0, 28.0), (0.0, 28.0)]);
    run_case(
        unturn(&grooved),
        TurnPivot::WcsOrigin,
        StockShape::Cylinder { diameter: 12.0, height: 44.0 },
        Location { pos: Point3::new(0.0, 0.0, -16.0), ..Default::default() },
        &[("GT-0200", None), ("GT-0300", None)],
        vec![
            ("GR1", "GT-0200", OpParams::GrooveRough {
                tolerance: 0.05, clearance_z: Height::fixed(32.0),
                top_z: Height::fixed(-3.6), bottom_z: Height::fixed(-6.8), stepdown: 0.5, stock_to_leave: 0.2, feed: 150.0, plunge_feed: 60.0,
            }),
            ("GF1", "GT-0200", OpParams::GrooveFinish {
                tolerance: 0.05, clearance_z: Height::fixed(32.0),
                top_z: Height::fixed(-3.6), bottom_z: Height::fixed(-6.8), feed: 150.0, plunge_feed: 60.0,
            }),
            ("PT1", "GT-0300", OpParams::Part {
                tolerance: 0.05, clearance_z: Height::fixed(32.0),
                top_z: Height::fixed(28.0), bottom_z: Height::fixed(-10.0), stepdown: 0.5, feed: 150.0, plunge_feed: 60.0,
                bar_load: true, bar_advance: 30.0, bar_unload: true, bar_transfer: false, inspect_tol: 0.0,
            }),
        ],
        true,
    );
    // 2. Threading
    let half = 1.2 * (30.0_f64).to_radians().tan();
    let mut tm = vec![(0.0, -16.0), (6.0, -16.0), (6.0, 8.0 - half)];
    for k in 0..=3 {
        let c = 8.0 + 2.0 * f64::from(k);
        tm.push((4.8, c));
        tm.push((6.0, c + half));
        if k < 3 { tm.push((6.0, c + 2.0 - half)); }
    }
    tm.extend([(6.0, 40.0), (0.0, 40.0)]);
    run_case(
        unturn(&revolve(&tm)),
        TurnPivot::WcsOrigin,
        StockShape::Cylinder { diameter: 12.0, height: 56.0 },
        Location { pos: Point3::new(0.0, 0.0, -16.0), ..Default::default() },
        &[("TD-1732-60", None)],
        vec![("TH1", "TD-1732-60", OpParams::Threading {
            tolerance: 0.05, clearance_z: Height::fixed(44.0), top_z: Height::fixed(14.3), bottom_z: Height::fixed(7.7),
            pitch: 2.0, depth: 1.2, infeed: 0.4, feed: 120.0, plunge_feed: 40.0,
            infeed_plan: ThreadInfeed { method: InfeedMethod::Flank, schedule: DepthSchedule::ConstantArea, min_depth: 0.1, finish_allowance: 0.05, spring_passes: 1, flank_angle: 27.5, ..ThreadInfeed::default() },
            hand: camcraft_app::cl::ThreadHand::Right,
        })],
        true,
    );
    // 3. TaperTurning & MultiPassTurning
    let taper = revolve(&[(0.0, -16.0), (10.0, -16.0), (8.0, 40.0), (0.0, 40.0)]);
    run_case(
        unturn(&taper),
        TurnPivot::WcsOrigin,
        StockShape::FromModel { source: "bar".into() },
        Location::default(),
        &[("TN-1600-0R8", None)],
        vec![("TP1", "TN-1600-0R8", OpParams::TaperTurning {
            tolerance: 0.05, clearance_z: Height::fixed(48.0), top_z: Height::fixed(40.0), bottom_z: Height::fixed(-16.0),
            bottom_dia: 20.0, top_dia: 16.0, feed: 200.0, plunge_feed: 50.0,
        })],
        true,
    );
    let stepped = revolve(&[(0.0, -16.0), (6.0, -16.0), (6.0, 2.0), (5.0, 2.0), (5.0, 16.0), (0.0, 16.0)]);
    run_case(
        unturn(&stepped),
        TurnPivot::WcsOrigin,
        StockShape::Cylinder { diameter: 12.0, height: 32.0 },
        Location { pos: Point3::new(0.0, 0.0, -16.0), ..Default::default() },
        &[("TN-1600-0R8", None)],
        vec![("MP1", "TN-1600-0R8", OpParams::MultiPassTurning {
            tolerance: 0.05, clearance_z: Height::fixed(24.0), top_z: Height::fixed(16.0), bottom_z: Height::fixed(-16.0),
            stepdown: 0.5, stock_to_leave: 0.25, passes: 3, feed: 200.0, plunge_feed: 50.0,
        })],
        true,
    );
    // 4. FormTurning
    let sag = |d: f64| 6.0 - (36.0_f64 - d * d).sqrt();
    let formed = revolve(&[
        (0.0, -50.0), (10.0, -50.0), (10.0, 8.0), (8.0 + sag(2.0), 8.0), (8.0 + sag(1.0), 9.0),
        (8.0, 10.0), (8.0 + sag(1.0), 11.0), (8.0 + sag(2.0), 12.0), (10.0, 12.0), (10.0, 50.0), (0.0, 50.0),
    ]);
    run_case(
        unturn(&formed),
        TurnPivot::PartCentre,
        StockShape::Cylinder { diameter: 20.0, height: 100.0 },
        Location { pos: Point3::new(0.0, 0.0, -50.0), ..Default::default() },
        &[("FG-0600-04", None)],
        vec![("FM1", "FG-0600-04", OpParams::FormTurning {
            tolerance: 0.05, clearance_z: Height::fixed(54.0), top_z: Height::fixed(13.0), bottom_z: Height::fixed(7.0), feed: 40.0, plunge_feed: 20.0,
        })],
        true,
    );
}

#[test]
fn lathe_boring_posts_like_baked() {
    let bored = revolve(&[(0.0, -50.0), (10.0, -50.0), (10.0, 50.0), (6.0, 50.0), (6.0, -20.0), (0.0, -20.0)]);
    let mut pilot = catalog_tool("DR-0400");
    pilot.flute_len = 70.0;
    // The larger nose leaves the head inside the pilot during the upward cut.
    // The window reaches the stock top, so no uncut band remains above it.
    // This is the clean roughing fixture from turning_bore_feeds.
    pilot.diameter = 10.9;
    pilot.shank_d = 10.9;
    let mut bar = catalog_tool("BR-1000");
    bar.nose_radius = 0.8;
    run_case(
        unturn(&bored),
        TurnPivot::PartCentre,
        StockShape::Cylinder { diameter: 20.0, height: 100.0 },
        Location { pos: Point3::new(0.0, 0.0, -50.0), ..Default::default() },
        &[("DR-1090", Some(pilot)), ("BR-1008", Some(bar))],
        vec![
            ("AX0", "DR-1090", OpParams::Drill {
                break_through: None, break_through_from: Default::default(), clearance_z: Height::fixed(60.0), retract_z: None,
                top_z: Height::fixed(50.0), bottom_z: None, cycle: CycleChoice::Auto,
                peck: 0.0, dwell: 0.0, feed: 120.0, plunge_feed: 80.0,
                holes: vec![Hole { xy: Point2::new(0.0, 0.0), radius: 5.45, entry_z: 50.0, floor_z: -20.0, through: false, stepped: false }],
                cross: vec![],
            }),
            ("BR1", "BR-1008", OpParams::BoringTurning {
                tolerance: 0.05, clearance_z: Height::fixed(60.0), top_z: Height::fixed(50.0), bottom_z: Height::fixed(-10.0), feed: 120.0, plunge_feed: 40.0,
            }),
        ],
        true,
    );
}

#[test]
fn lathe_op20_flip_and_cross_drill_detect_holes_in_setup_frame() {
    let raw_op10 = revolve(&[(0.0, -20.0), (6.0, -20.0), (6.0, 0.0), (10.0, 0.0), (10.0, 20.0), (0.0, 20.0)]);
    let rough = OpParams::ProfileRough {
        tolerance: 0.05, clearance_z: Height::fixed(24.0), top_z: Height::fixed(20.0), bottom_z: Height::fixed(0.0),
        stepdown: 1.0, stock_to_leave: 0.25, feed: 150.0, plunge_feed: 80.0,
    };
    let finish = OpParams::ProfileFinish {
        tolerance: 0.05, clearance_z: Height::fixed(24.0), top_z: Height::fixed(20.0), bottom_z: Height::fixed(0.0), feed: 120.0, plunge_feed: 60.0,
    };
    let op20_doc = |mesh: Mesh, do_flip: bool| -> (World, Option<Mesh>) {
        let w = new_world();
        let mut h = History::default();
        h.execute(&w, LoadMachine::new(&w, "lathe.mch".into(), Some(lathe())).expect("lathe"));
        h.execute(&w, SelectPost::new(&w, PostRef::Named("Fanuc Lathe".into())).expect("post"));
        h.execute(&w, AddTool::new(&w, "TN-1600-0R8", catalog_tool("TN-1600-0R8")).expect("insert"));
        h.execute(&w, ImportModel::from_mesh("bar", mesh.clone()));
        h.execute(&w, CreateSetup::new(&w, None, None).expect("Setup1"));
        let baked = do_flip.then(|| {
            h.execute(&w, AlignPartToSetup::angles(&w, "Setup1", FLIP).expect("flip"));
            bake(&mesh, setup_align::stored(&w, "Setup1").expect("stored"))
        });
        h.execute(&w, SetStock::new(&w, StockShape::Cylinder { diameter: 20.0, height: 40.0 }, Location { pos: Point3::new(0.0, 0.0, -20.0), ..Default::default() }).expect("stock"));
        for (n, p) in [("PR", rough.clone()), ("PF", finish.clone())] {
            h.execute(&w, CreateOperation::new(&w, n, "TN-1600-0R8", SrcRef::Entity("bar".into()), p).expect("op"));
            h.execute(&w, GenerateOperation::new(&w, n).expect("generate"));
        }
        (w, baked)
    };
    let (tw, baked) = op20_doc(raw_op10.clone(), true);
    let (bw, _) = op20_doc(baked.expect("baked"), false);
    assert_moves_eq(&path_of(&tw, "PF"), &path_of(&bw, "PF"));
    assert_eq!(posting::post_setup_unwritten(&tw).expect("t").program, posting::post_setup_unwritten(&bw).expect("b").program);
    let (uw, _) = op20_doc(raw_op10, false);
    let max_r = |p: &Toolpath| p.moves.iter().filter(|m| m.to.z >= 1.0 && m.to.z <= 19.0).map(|m| m.to.x.hypot(m.to.y)).fold(0.0, f64::max);
    assert!((max_r(&path_of(&tw, "PF")) - max_r(&path_of(&uw, "PF"))).abs() > 3.0);

    // Cross drill: detect_holes on a turned cylindrical bar with a punched hole matches baked.
    let hole = CrossHole {
        origin: Point3::new(10.0, 0.0, 0.0), dir: Point3::new(-1.0, 0.0, 0.0),
        radius: 2.0, entry_d: 0.0, floor_d: -3.0, through: false,
    };
    let raw_cross = unturn(&punched_bar::punched_bar(10.0, punched_bar::FOOT, punched_bar::TOP, &[hole]));
    let cross_doc = |mesh: Mesh, do_align: bool| -> (World, Result<(), String>, Option<Mesh>) {
        let w = new_world();
        let mut h = History::default();
        h.execute(&w, LoadMachine::new(&w, "lathe.mch".into(), Some(lathe())).expect("lathe"));
        let mut drill = catalog_tool("DR-0400");
        drill.flute_len = 50.0;
        h.execute(&w, AddTool::new(&w, "DR-0400", drill).expect("drill"));
        h.execute(&w, ImportModel::from_mesh("bar", mesh.clone()));
        h.execute(&w, CreateSetup::new(&w, None, None).expect("Setup1"));
        let baked = do_align.then(|| {
            h.execute(&w, AlignPartToSetup::turn(&w, "Setup1", TURN, TurnPivot::WcsOrigin).expect("align"));
            bake(&mesh, setup_align::stored(&w, "Setup1").expect("stored"))
        });
        h.execute(&w, SetStock::new(&w, StockShape::Cylinder { diameter: 20.0, height: 100.0 }, Location { pos: Point3::new(0.0, 0.0, -80.0), ..Default::default() }).expect("stock"));
        let seed = cross_support::cross_params(Vec::new(), None);
        let res = match operation::detect_holes(&w, "CD1", &seed, Some("Setup1"), &SrcRef::Entity("bar".into())) {
            Ok(d) => {
                h.execute(&w, CreateOperation::new(&w, "CD1", "DR-0400", SrcRef::Entity("bar".into()), d.params).expect("op"));
                GenerateOperation::new(&w, "CD1").map(|g| h.execute(&w, g)).map_err(|e| e.to_string())
            }
            Err(e) => Err(e.to_string()),
        };
        (w, res, baked)
    };
    let (tw, tres, baked) = cross_doc(raw_cross.clone(), true);
    tres.expect("aligned lathe cross drill generates");
    let (bw, bres, _) = cross_doc(baked.expect("baked"), false);
    bres.expect("baked lathe cross drill generates");
    assert_moves_eq(&path_of(&tw, "CD1"), &path_of(&bw, "CD1"));
    let (_, ures, _) = cross_doc(raw_cross, false);
    assert!(ures.is_err(), "unaligned bar has its axis along Y, not spindle Z");
}
