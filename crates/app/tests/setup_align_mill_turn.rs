//! #2287: part alignment on a mill-turn machine - turning ops, live-tool
//! cross-drilling (`detect_holes` + `"Fanuc Lathe"` post) and milling ops
//! (`PostRef::Builtin`) cut like their baked mesh twins, while Probe,
//! 5-axis/rotary and 3+2 tilted ops keep their alignment refusals.
//!
//! Provenance: our own synthetic fixtures and `cross_support`'s drilled bar;
//! no vendor material consulted.
//! Gate map (#2287): covers app::setup_align, app::operation (job,
//! generate, material, spindle_law, cross_spindle, turning_*,
//! machining2d), app::posting.

mod cross_support;

use flecs_ecs::prelude::World;

use camcraft_app::cl::Toolpath;
use camcraft_app::commands::{
    AddTool, AlignPartToSetup, CreateOperation, CreateSetup, GenerateOperation, History,
    ImportModel, LoadMachine, SelectPost, SetStock,
};
use camcraft_app::geometry::{Mesh, Point3, Triangle, transform_point};
use camcraft_app::machine::{AxisLimits, Coolant, LiveToolCodes, Machine};
use camcraft_app::model;
use camcraft_app::operation::{
    self, AfterOperation, AreaControls, CoolantMode, Height, OpParams, OpStatus,
    OpType, SrcRef, Tilt, seed_params,
};
use camcraft_app::posting;
use camcraft_app::schema::{Location, Operation, PostRef};
use camcraft_app::setup_align;
use camcraft_app::stock::StockShape;
use camcraft_app::tools::{Tool, catalog};
use camcraft_app::world::new_world;

const TURN: [f64; 3] = [90.0, 0.0, 90.0];
const TAU: f64 = std::f64::consts::TAU;

fn catalog_tool(name: &str) -> Tool {
    catalog().into_iter().find(|(n, _)| *n == name).map(|(_, t)| t).expect("catalog tool")
}

fn mill_turn() -> Machine {
    Machine {
        model: "TestBench mill-turn".into(),
        vendor: "Camcraft Testbench".into(),
        kind: Some("mill-turn".into()),
        spindle_max: Some(12000.0),
        coolants: vec![Coolant { name: "Flood".into(), m_code: "M8".into() }],
        axes: vec![
            AxisLimits { name: "X".into(), feed: Some(4000.0), ..Default::default() },
            AxisLimits { name: "Y".into(), feed: Some(4000.0), ..Default::default() },
            AxisLimits { name: "Z".into(), feed: Some(4000.0), ..Default::default() },
            AxisLimits { name: "B".into(), rotary: true, axis: Some([0.0, 1.0, 0.0]), ..Default::default() },
            AxisLimits {
                name: "C".into(),
                rotary: true,
                axis: Some([0.0, 0.0, 1.0]),
                travel: Some((-360.0, 360.0)),
                feed: Some(6000.0),
                rapid: Some(12000.0),
                ..Default::default()
            },
        ],
        live_tool: Some(LiveToolCodes {
            c_axis_on: "M1001".into(),
            c_axis_off: "M1002".into(),
            spindle_on: "M1003".into(),
            spindle_off: "M1004".into(),
        }),
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
        assert!(
            (ma.from.x - mb.from.x).abs() < 1e-6
                && (ma.from.y - mb.from.y).abs() < 1e-6
                && (ma.from.z - mb.from.z).abs() < 1e-6
                && (ma.to.x - mb.to.x).abs() < 1e-6
                && (ma.to.y - mb.to.y).abs() < 1e-6
                && (ma.to.z - mb.to.z).abs() < 1e-6,
            "move {i}: {:?}->{:?} vs {:?}->{:?}",
            ma.from,
            ma.to,
            mb.from,
            mb.to
        );
    }
}

fn path_of(w: &World, name: &str) -> Toolpath {
    let e = model::find_by_name(w, name).expect("op");
    assert_eq!(camcraft_app::operation::status_of(w, &e), OpStatus::Ok, "{name}");
    model::toolpath_of(w, &e).expect("cached").path
}

fn stepped_pad() -> Mesh {
    let p = Point3::new;
    let quad = |out: &mut Vec<Triangle>, a: Point3, b: Point3, c: Point3, d: Point3| {
        out.push(Triangle::new(a, b, c));
        out.push(Triangle::new(a, c, d));
    };
    let mut f = Vec::new();
    let (x0, x1, y0, y1, z0, z1) = (-20.0, 20.0, -12.0, 12.0, -4.0, 4.0);
    quad(&mut f, p(x0, y0, z1), p(x1, y0, z1), p(x1, y1, z1), p(x0, y1, z1));
    quad(&mut f, p(x0, y0, z0), p(x0, y1, z0), p(x1, y1, z0), p(x1, y0, z0));
    quad(&mut f, p(x0, y0, z0), p(x1, y0, z0), p(x1, y0, z1), p(x0, y0, z1));
    quad(&mut f, p(x1, y1, z0), p(x0, y1, z0), p(x0, y1, z1), p(x1, y1, z1));
    quad(&mut f, p(x0, y1, z0), p(x0, y0, z0), p(x0, y0, z1), p(x0, y1, z1));
    quad(&mut f, p(x1, y0, z0), p(x1, y1, z0), p(x1, y1, z1), p(x1, y0, z1));
    Mesh::from_facets(f)
}

#[test]
fn mill_turn_turning_and_cross_drill_match_baked() {
    // 1. Turning ops on mill-turn posted through Fanuc Lathe.
    let bar = revolve(&[(0.0, -20.0), (10.0, -20.0), (10.0, -2.0), (7.0, -2.0), (7.0, 20.0), (0.0, 20.0)]);
    let raw = unturn(&bar);
    let turn_doc = |mesh: Mesh, do_align: bool| -> (World, Result<(), String>, Option<Mesh>) {
        let w = new_world();
        let mut h = History::default();
        h.execute(&w, LoadMachine::new(&w, "mt.mch".into(), Some(mill_turn())).expect("mt"));
        h.execute(&w, SelectPost::new(&w, PostRef::Named("Fanuc Lathe".into())).expect("post"));
        h.execute(&w, AddTool::new(&w, "TN-1600-0R8", catalog_tool("TN-1600-0R8")).expect("insert"));
        h.execute(&w, ImportModel::from_mesh("bar", mesh.clone()));
        h.execute(&w, CreateSetup::new(&w, None, None).expect("Setup1"));
        let baked = do_align.then(|| {
            h.execute(&w, AlignPartToSetup::angles(&w, "Setup1", TURN).expect("align"));
            bake(&mesh, setup_align::stored(&w, "Setup1").expect("stored"))
        });
        h.execute(&w, SetStock::new(&w, StockShape::Cylinder { diameter: 20.0, height: 42.0 }, Location { pos: Point3::new(0.0, 0.0, -20.0), ..Default::default() }).expect("stock"));
        let ops = [
            ("FC1", OpParams::Facing {
                tolerance: 0.05, clearance_z: Height::fixed(26.0),
                top_z: Height::fixed(22.0), bottom_z: Height::fixed(20.0), stepdown: 1.0, feed: 150.0, plunge_feed: 80.0,
            }),
            ("PR1", OpParams::ProfileRough {
                tolerance: 0.05, clearance_z: Height::fixed(25.0),
                top_z: Height::fixed(20.0), bottom_z: Height::fixed(-2.0), stepdown: 1.0, stock_to_leave: 0.25, feed: 150.0, plunge_feed: 80.0,
            }),
            ("PF1", OpParams::ProfileFinish {
                tolerance: 0.05, clearance_z: Height::fixed(25.0),
                top_z: Height::fixed(20.0), bottom_z: Height::fixed(-2.0), feed: 120.0, plunge_feed: 60.0,
            }),
        ];
        for (name, params) in ops {
            h.execute(&w, CreateOperation::new(&w, name, "TN-1600-0R8", SrcRef::Entity("bar".into()), params).expect("op"));
            if let Err(err) = GenerateOperation::new(&w, name).map(|g| h.execute(&w, g)) {
                return (w, Err(err.to_string()), baked);
            }
        }
        (w, Ok(()), baked)
    };
    let (tw, tres, baked) = turn_doc(raw.clone(), true);
    tres.expect("aligned mill-turn turning generates");
    let (bw, bres, _) = turn_doc(baked.expect("baked"), false);
    bres.expect("baked mill-turn turning generates");
    for name in ["FC1", "PR1", "PF1"] {
        assert_moves_eq(&path_of(&tw, name), &path_of(&bw, name));
    }
    assert_eq!(posting::post_setup_unwritten(&tw).expect("t").program, posting::post_setup_unwritten(&bw).expect("b").program);
    let (_, ures, _) = turn_doc(raw, false);
    assert!(ures.is_err(), "unaligned bar sits off spindle axis");

    // 2. Live-tool cross-drilling on mill-turn posted through mill post.
    let raw_cross = unturn(&Mesh::from_facets(cross_support::radial_bar()));
    let cross_doc = |mesh: Mesh, do_align: bool| -> (World, Result<(), String>, Option<Mesh>) {
        let w = new_world();
        let mut h = History::default();
        h.execute(&w, LoadMachine::new(&w, "mt.mch".into(), Some(mill_turn())).expect("mt"));
        let mut drill = catalog_tool("DR-0400");
        drill.flute_len = 50.0;
        h.execute(&w, AddTool::new(&w, "DR-0400", drill).expect("drill"));
        h.execute(&w, ImportModel::from_mesh("bar", mesh.clone()));
        h.execute(&w, CreateSetup::new(&w, None, None).expect("Setup1"));
        let baked = do_align.then(|| {
            h.execute(&w, AlignPartToSetup::angles(&w, "Setup1", TURN).expect("align"));
            bake(&mesh, setup_align::stored(&w, "Setup1").expect("stored"))
        });
        h.execute(&w, SetStock::new(&w, StockShape::Box { dx: 30.0, dy: 10.0, dz: 30.0 }, Location { pos: Point3::new(0.0, 0.0, -15.0), ..Default::default() }).expect("stock"));
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
    tres.expect("aligned cross drill generates");
    let (bw, bres, _) = cross_doc(baked.expect("baked"), false);
    bres.expect("baked cross drill generates");
    assert_moves_eq(&path_of(&tw, "CD1"), &path_of(&bw, "CD1"));
    assert_eq!(posting::post_setup_unwritten(&tw).expect("t").program, posting::post_setup_unwritten(&bw).expect("b").program);
    let (uw, ures, _) = cross_doc(raw_cross, false);
    if ures.is_ok() {
        assert_ne!(path_of(&uw, "CD1").moves.len(), path_of(&bw, "CD1").moves.len());
    }
}

#[test]
fn mill_turn_milling_ops_match_baked_while_probe_5axis_and_tilt_stay_refused() {
    let raw = unturn(&stepped_pad());
    let mill_doc = |mesh: Mesh, do_align: bool| -> (World, Result<(), String>, Option<Mesh>) {
        let w = new_world();
        let mut h = History::default();
        h.execute(&w, LoadMachine::new(&w, "mt.mch".into(), Some(mill_turn())).expect("mt"));
        h.execute(&w, AddTool::new(&w, "EM-0600-4FL", catalog_tool("EM-0600-4FL")).expect("em"));
        h.execute(&w, ImportModel::from_mesh("pad", mesh.clone()));
        h.execute(&w, CreateSetup::new(&w, None, None).expect("Setup1"));
        let baked = do_align.then(|| {
            h.execute(&w, AlignPartToSetup::angles(&w, "Setup1", TURN).expect("align"));
            bake(&mesh, setup_align::stored(&w, "Setup1").expect("stored"))
        });
        h.execute(&w, SetStock::new(&w, StockShape::Box { dx: 44.0, dy: 28.0, dz: 9.0 }, Location { pos: Point3::new(0.0, 0.0, -4.0), ..Default::default() }).expect("stock"));
        let mut face = seed_params(OpType::Face.ordinal());
        if let OpParams::Face { cut_z, .. } = &mut face {
            *cut_z = Height::fixed(4.0);
        }
        let mut contour = seed_params(OpType::Contour.ordinal());
        if let OpParams::Contour { top_z, bottom_z, .. } = &mut contour {
            *top_z = Height::fixed(4.0);
            *bottom_z = Height::fixed(-4.0);
        }
        for (name, params) in [("F1", face), ("C1", contour)] {
            h.execute(&w, CreateOperation::new(&w, name, "EM-0600-4FL", SrcRef::Entity("pad".into()), params).expect("op"));
            if let Err(err) = GenerateOperation::new(&w, name).map(|g| h.execute(&w, g)) {
                return (w, Err(err.to_string()), baked);
            }
        }
        (w, Ok(()), baked)
    };
    let (tw, tres, baked) = mill_doc(raw.clone(), true);
    tres.expect("aligned mill-turn milling generates");
    let (bw, bres, _) = mill_doc(baked.expect("baked"), false);
    bres.expect("baked mill-turn milling generates");
    for name in ["F1", "C1"] {
        assert_moves_eq(&path_of(&tw, name), &path_of(&bw, name));
    }
    assert_eq!(posting::post_setup_unwritten(&tw).expect("t").program, posting::post_setup_unwritten(&bw).expect("b").program);
    let (uw, ures, _) = mill_doc(raw, false);
    ures.expect("unaligned cuts raw pad differently");
    assert!(
        path_of(&uw, "C1")
            .moves
            .iter()
            .zip(&path_of(&bw, "C1").moves)
            .any(|(a, b)| (a.to.x - b.to.x).abs() > 1.0 || (a.to.y - b.to.y).abs() > 1.0),
        "unaligned contour must cut a different XY footprint"
    );

    // Probe, 5-axis, rotary and 3+2 tilted ops stay refused on aligned mill-turn.
    let mut tilted = seed_params(OpType::Face.ordinal());
    if let OpParams::Face { tilt, .. } = &mut tilted {
        *tilt = Some(Tilt { basis_version: 1, a_deg: 15.0, b_deg: 0.0, origin: [0.0; 3] });
    }
    let tilted_op = Operation {
        tool: "EM-0600-4FL".into(),
        src: SrcRef::Entity("pad".into()),
        params: tilted,
        stamp: None,
        setup: Some("Setup1".into()),
        area: AreaControls::default(),
        coolant: CoolantMode::default(),
        after: AfterOperation::default(),
        suppressed: false,
    };
    let err = setup_align::generation_problem(&tw, &tilted_op).expect("tilted stays refused");
    assert!(err.contains("tilted plane is not supported yet"), "{err}");

    for ty in [OpType::Probe, OpType::Contour5D, OpType::RotaryContour] {
        let params = seed_params(ty.ordinal());
        let op = Operation {
            tool: "EM-0600-4FL".into(),
            src: SrcRef::Entity("pad".into()),
            params,
            stamp: None,
            setup: Some("Setup1".into()),
            area: AreaControls::default(),
            coolant: CoolantMode::default(),
            after: AfterOperation::default(),
            suppressed: false,
        };
        let err = setup_align::generation_problem(&tw, &op).expect("unverified kind stays refused on mill-turn");
        assert!(err.contains("not supported yet in an aligned setup"), "{ty:?}: {err}");
    }
}
