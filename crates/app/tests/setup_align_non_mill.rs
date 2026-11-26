//! #2287: part alignment on Plasma, Wire EDM, Sinker EDM and Waterjet.
//!
//! Provenance: our own synthetic fixtures turned `[90, 0, 90]` and
//! compared against the exact mesh with the turn baked in (#2276).
//! Gate map (#2287): covers app::setup_align, app::operation (job,
//! wire_ruled, sinker_faces, plasma_op, wire_op, sinker_op,
//! sinker_side_op), app::commands::electrode, app::posting,
//! app::posting_plasma, app::posting_wire, app::stock_sim.

mod pocket_support;

use flecs_ecs::prelude::World;

use camcraft_app::cl::Toolpath;
use camcraft_app::commands::{
    ActivateSetup, AddTool, AlignPartToSetup, CreateOperation, CreateSetup, ElectrodeFromFaces,
    GenerateOperation, History, ImportModel, LoadMachine, SelectPost, SetStock,
};
use camcraft_app::geometry::{Mesh, Point3, Triangle, transform_point};
use camcraft_app::machine::parse_mch;
use camcraft_app::model;
use camcraft_app::operation::{
    AfterOperation, AreaControls, CoolantMode, FeatureSel, GeoSel, Height, HeightReference,
    OpParams, OpStatus, PlasmaComp, PlasmaOp, SinkerBurnOp, SinkerDatum, SinkerPass,
    SinkerSideBurnOp, SinkerSideDirection, SrcRef, WireContourOp, WireFourAxis,
    WirePairingRule, WireSide,
};
use camcraft_app::posting;
use camcraft_app::schema::{Location, Operation, PostRef};
use camcraft_app::setup_align;
use camcraft_app::sinker_electrode::ElectrodeSpec;
use camcraft_app::stock::StockShape;
use camcraft_app::stock_sim::simulate_document;
use camcraft_app::tools::torch_tool;
use camcraft_app::wire_tools::wire_tool;
use camcraft_app::world::new_world;

const TURN: [f64; 3] = [90.0, 0.0, 90.0];
const WIRE_MCH: &str = include_str!("edm/wire-iso-fixture.mch");
const SINKER_MCH: &str = include_str!("edm/sinker-iso-dialect.mch");

/// Inverse of `TURN` ([90, 0, 90] maps `(y, z, x)` to `(x, y, z)`).
fn unturn(mesh: &Mesh) -> Mesh {
    let mv = |p: Point3| Point3::new(p.y, p.z, p.x);
    Mesh::from_facets(mesh.facets.iter().map(|t| Triangle::new(mv(t.a), mv(t.b), mv(t.c))).collect())
}

fn bake(mesh: &Mesh, loc: Location) -> Mesh {
    let mv = |p: Point3| transform_point(p, loc.pos, loc.rot);
    Mesh::from_facets(mesh.facets.iter().map(|t| Triangle::new(mv(t.a), mv(t.b), mv(t.c))).collect())
}

fn quad(out: &mut Vec<Triangle>, a: Point3, b: Point3, c: Point3, d: Point3) {
    out.push(Triangle::new(a, b, c));
    out.push(Triangle::new(a, c, d));
}

fn assert_moves_eq(a: &Toolpath, b: &Toolpath) {
    assert_eq!(a.moves.len(), b.moves.len(), "move count mismatch");
    for (i, (ma, mb)) in a.moves.iter().zip(&b.moves).enumerate() {
        assert_eq!(ma.kind, mb.kind, "move {i} kind");
        let close = |u: Point3, v: Point3| (u.x - v.x).abs() < 1e-5 && (u.y - v.y).abs() < 1e-5 && (u.z - v.z).abs() < 1e-5;
        assert!(close(ma.from, mb.from) && close(ma.to, mb.to), "move {i}: {:?}->{:?} vs {:?}->{:?}", ma.from, ma.to, mb.from, mb.to);
        match (ma.axis, mb.axis) {
            (Some(u), Some(v)) => assert!((u[0] - v[0]).abs() < 1e-5 && (u[1] - v[1]).abs() < 1e-5 && (u[2] - v[2]).abs() < 1e-5),
            (None, None) => {}
            other => panic!("move {i} axis presence: {other:?}"),
        }
    }
}

fn path_of(world: &World, name: &str) -> Toolpath {
    let e = model::find_by_name(world, name).expect("op");
    assert_eq!(camcraft_app::operation::status_of(world, &e), OpStatus::Ok, "{name}");
    model::toolpath_of(world, &e).expect("cached").path
}

fn align_and_bake(world: &World, h: &mut History, raw: &Mesh) -> Mesh {
    h.execute(world, AlignPartToSetup::angles(world, "Setup1", TURN).expect("align"));
    bake(raw, setup_align::stored(world, "Setup1").expect("stored alignment"))
}

fn plasma_plate() -> Mesh {
    let p = Point3::new;
    let ring = |x0: f64, x1: f64, y0: f64, y1: f64, z: f64| [p(x0, y0, z), p(x1, y0, z), p(x1, y1, z), p(x0, y1, z)];
    let mut f = Vec::new();
    for (up, z) in [(true, 2.5), (false, -2.5)] {
        let (o, h) = (ring(-20.0, 20.0, -15.0, 15.0, z), ring(4.0, 14.0, 1.0, 9.0, z));
        for i in 0..4 {
            let j = (i + 1) % 4;
            if up { quad(&mut f, o[i], o[j], h[j], h[i]); } else { quad(&mut f, o[i], h[i], h[j], o[j]); }
        }
    }
    let (ol, oh, hl, hh) = (ring(-20.0, 20.0, -15.0, 15.0, -2.5), ring(-20.0, 20.0, -15.0, 15.0, 2.5), ring(4.0, 14.0, 1.0, 9.0, -2.5), ring(4.0, 14.0, 1.0, 9.0, 2.5));
    for i in 0..4 {
        let j = (i + 1) % 4;
        quad(&mut f, ol[i], ol[j], oh[j], oh[i]);
        quad(&mut f, hl[i], hh[i], hh[j], hl[j]);
    }
    Mesh::from_facets(f)
}

fn plasma_params() -> OpParams {
    OpParams::PlasmaProfile(PlasmaOp {
        clearance_z: Height::of(HeightReference::TopOfModel, 15.0),
        top_z: Height::of(HeightReference::TopOfModel, 0.0),
        bottom_z: Height::of(HeightReference::BottomOfModel, 0.0),
        kerf: None, comp: PlasmaComp::Computer, reversed: false, lead_in: 4.0, lead_out: 3.0,
        small_hole_dia: 32.0, small_hole_feed: 0.65, overburn: 1.0, feed: Some(2500.0),
        pierce_height: None, cut_height: None, pierce_delay: None,
        picks: Default::default(),
    })
}

fn face_along(world: &World, entity: &str, dir: Point3) -> GeoSel {
    let e = model::find_by_name(world, entity).expect("entity");
    let feat = model::features_of(world, &e).expect("features");
    let dot = |v: [f64; 3]| v[0] * dir.x + v[1] * dir.y + v[2] * dir.z;
    let id = feat.faces.iter()
        .filter(|fa| dot(fa.normal).abs() > 0.99)
        .filter(|fa| fa.loops.iter().flat_map(|l| &l.verts).any(|&vi| dot(feat.verts[vi as usize]) > 1.0))
        .max_by(|a, b| a.area.total_cmp(&b.area))
        .expect("matching face")
        .id;
    GeoSel { entity: entity.into(), feature: FeatureSel::Face(id) }
}

fn face_with_nz(world: &World, entity: &str, setup: Option<&str>, sign: f64) -> GeoSel {
    let e = model::find_by_name(world, entity).expect("entity");
    let feat = model::features_of(world, &e).expect("features");
    let loc = setup_align::pose_in(world, &e, setup).expect("pose");
    let id = feat.faces.iter()
        .filter(|fa| transform_point(Point3::new(fa.normal[0], fa.normal[1], fa.normal[2]), Point3::ZERO, loc.rot).z * sign > 0.99)
        .max_by(|a, b| a.area.total_cmp(&b.area))
        .expect("horizontal face")
        .id;
    GeoSel { entity: entity.into(), feature: FeatureSel::Face(id) }
}

#[test]
fn plasma_cuts_the_turned_plate_like_its_baked_twin() {
    let raw = unturn(&plasma_plate());
    let setup_doc = |mesh: Mesh, do_align: bool, use_sel: bool| -> (World, Option<Mesh>) {
        let w = new_world();
        let mut h = History::default();
        h.execute(&w, LoadMachine::builtin(&w, "Tormach 1300PL").expect("1300PL"));
        h.execute(&w, SelectPost::new(&w, PostRef::Named("Tormach PathPilot Plasma".into())).expect("post"));
        h.execute(&w, AddTool::new(&w, "PT", torch_tool(1.5)).expect("torch"));
        h.execute(&w, ImportModel::from_mesh("plate", mesh.clone()));
        h.execute(&w, CreateSetup::new(&w, None, None).expect("Setup1"));
        let baked = do_align.then(|| align_and_bake(&w, &mut h, &mesh));
        let src = if use_sel { SrcRef::Selection(vec![face_with_nz(&w, "plate", do_align.then_some("Setup1"), 1.0)]) } else { SrcRef::Entity("plate".into()) };
        h.execute(&w, CreateOperation::new(&w, "P1", "PT", src, plasma_params()).expect("op"));
        h.execute(&w, GenerateOperation::new(&w, "P1").expect("generate"));
        (w, baked)
    };
    for use_sel in [false, true] {
        let (tw, baked) = setup_doc(raw.clone(), true, use_sel);
        let (bw, _) = setup_doc(baked.expect("baked"), false, use_sel);
        assert_moves_eq(&path_of(&tw, "P1"), &path_of(&bw, "P1"));
        assert_eq!(posting::post_setup_unwritten(&tw).expect("t").program, posting::post_setup_unwritten(&bw).expect("b").program);
    }
    let (uw, _) = setup_doc(raw, false, false);
    assert!(path_of(&uw, "P1").moves.iter().map(|m| m.to.z).fold(0.0, f64::max) > 30.0);
}

fn pentagon_punch() -> Mesh {
    let pts = [(-12.0, -8.0), (14.0, -8.0), (14.0, 4.0), (4.0, 12.0), (-12.0, 12.0)];
    let (z0, z1) = (-4.5, 4.5);
    let mut f = Vec::new();
    let p = |q: (f64, f64), z: f64| Point3::new(q.0, q.1, z);
    for i in 1..(pts.len() - 1) {
        f.push(Triangle::new(p(pts[0], z1), p(pts[i], z1), p(pts[i + 1], z1)));
        f.push(Triangle::new(p(pts[0], z0), p(pts[i + 1], z0), p(pts[i], z0)));
    }
    for i in 0..pts.len() {
        let (a, b) = (pts[i], pts[(i + 1) % pts.len()]);
        quad(&mut f, p(a, z0), p(b, z0), p(b, z1), p(a, z1));
    }
    Mesh::from_facets(f)
}

fn ruled_loft() -> Mesh {
    const N: usize = 64;
    let ring = |rx: f64, ry: f64, ox: f64, oy: f64| -> Vec<(f64, f64)> {
        (0..N).map(|j| {
            let t = std::f64::consts::PI + std::f64::consts::TAU * j as f64 / N as f64;
            (ox + rx * t.cos(), oy + ry * t.sin())
        }).collect()
    };
    let (lo, hi) = (ring(8.8, 6.8, 0.3, -0.2), ring(8.0, 8.0, 0.0, 0.0));
    let (z0, z1) = (-4.5, 4.5);
    let mut f = Vec::new();
    for (pts, ctr_xy, z, up) in [(&lo, (0.3, -0.2), z0, false), (&hi, (0.0, 0.0), z1, true)] {
        let ctr = Point3::new(ctr_xy.0, ctr_xy.1, z);
        for j in 0..N {
            let (a, b) = (Point3::new(pts[j].0, pts[j].1, z), Point3::new(pts[(j + 1) % N].0, pts[(j + 1) % N].1, z));
            f.push(if up { Triangle::new(ctr, a, b) } else { Triangle::new(ctr, b, a) });
        }
    }
    for j in 0..N {
        let k = (j + 1) % N;
        let (a, b, c, d) = (Point3::new(lo[j].0, lo[j].1, z0), Point3::new(lo[k].0, lo[k].1, z0), Point3::new(hi[k].0, hi[k].1, z1), Point3::new(hi[j].0, hi[j].1, z1));
        let mid = |p: Point3, q: Point3| ((p.x + q.x) * 0.5).hypot((p.y + q.y) * 0.5);
        if mid(a, c) <= mid(b, d) {
            f.push(Triangle::new(a, b, c));
            f.push(Triangle::new(a, c, d));
        } else {
            f.push(Triangle::new(a, b, d));
            f.push(Triangle::new(b, c, d));
        }
    }
    Mesh::from_facets(f)
}

#[test]
fn wire_edm_2axis_and_4axis_ruled_cut_like_baked() {
    let wire_doc = |mesh: Mesh, do_align: bool, baked_mode: bool, four_axis: bool| -> (World, Result<(), String>, Option<Mesh>) {
        let w = new_world();
        let mut h = History::default();
        let m = parse_mch(WIRE_MCH).expect("wire mch");
        h.execute(&w, LoadMachine::new(&w, "wire-iso.mch".into(), Some(m)).expect("mount"));
        h.execute(&w, SelectPost::new(&w, PostRef::Named("Generic ISO Wire EDM".into())).expect("post"));
        h.execute(&w, AddTool::new(&w, "W", wire_tool(0.25)).expect("wire tool"));
        h.execute(&w, ImportModel::from_mesh("slug", mesh.clone()));
        h.execute(&w, CreateSetup::new(&w, None, None).expect("Setup1"));
        let baked = do_align.then(|| align_and_bake(&w, &mut h, &mesh));
        let src = if four_axis {
            let (hi, lo) = if baked_mode { (Point3::new(0.0, 0.0, 1.0), Point3::new(0.0, 0.0, -1.0)) } else { (Point3::new(0.0, 1.0, 0.0), Point3::new(0.0, -1.0, 0.0)) };
            SrcRef::Selection(vec![face_along(&w, "slug", hi), face_along(&w, "slug", lo)])
        } else {
            SrcRef::Entity("slug".into())
        };
        let op = WireContourOp {
            lead_arc_radius: None,
            top_z: Height::of(HeightReference::TopOfModel, 0.0),
            bottom_z: Height::of(HeightReference::BottomOfModel, 0.0),
            condition: "C110".into(), side: WireSide::Punch, picks: camcraft_app::path_picks::PathPicks { start: vec![camcraft_app::path_picks::PathPick::xy(camcraft_app::geo2d::Point2::new(-25.0, 0.0))], exit: Vec::new() }, tabs: None, capture: true, both_pieces_restrained: false, stock_centered_in_xy_travel: false,
            taper_deg: 0.0, burnout: None, clearance: camcraft_app::operation::DEFAULT_CLEARANCE, skims: Vec::new(), sharp_corners: false,
            four_axis: four_axis.then(|| WireFourAxis { pairing: WirePairingRule::ArcLength, verify_tolerance: 0.12, ..WireFourAxis::default() }),
        };
        h.execute(&w, CreateOperation::new(&w, "W1", "W", src, OpParams::WireContour(op)).expect("op"));
        let res = GenerateOperation::new(&w, "W1").map(|c| h.execute(&w, c)).map_err(|e| e.to_string());
        (w, res, baked)
    };
    for (mesh, four) in [(unturn(&pentagon_punch()), false), (unturn(&ruled_loft()), true)] {
        let (tw, tres, baked) = wire_doc(mesh.clone(), true, false, four);
        tres.expect("aligned wire generates");
        let (bw, bres, _) = wire_doc(baked.expect("baked"), false, true, four);
        bres.expect("baked wire generates");
        assert_moves_eq(&path_of(&tw, "W1"), &path_of(&bw, "W1"));
        assert_eq!(posting::post_setup_unwritten(&tw).expect("t").program, posting::post_setup_unwritten(&bw).expect("b").program);
        let (uw, ures, _) = wire_doc(mesh, false, false, four);
        if four {
            assert!(ures.is_err(), "unaligned 4-axis faces are vertical, not horizontal");
        } else {
            ures.expect("unaligned 2-axis cuts different silhouette");
            assert!(path_of(&uw, "W1").moves.len() != path_of(&bw, "W1").moves.len()
                || (path_of(&uw, "W1").moves[0].to.z - path_of(&bw, "W1").moves[0].to.z).abs() > 1.0);
        }
    }
}

fn sinker_block() -> Mesh {
    let raw = pocket_support::height_field(&[-30.0, -1.1, 9.1, 30.0], &[-30.0, -6.1, 0.1, 30.0], |i, j| if (i, j) == (1, 1) { 14.0 } else { 20.0 });
    let shift = |p: Point3| Point3::new(p.x, p.y, p.z - 10.0);
    Mesh::from_facets(raw.facets.iter().map(|t| Triangle::new(shift(t.a), shift(t.b), shift(t.c))).collect())
}

#[test]
fn sinker_electrode_burn_matches_baked_and_side_burn_stays_refused() {
    let raw = unturn(&sinker_block());
    let sinker_doc = |mesh: Mesh, do_align: bool, add_side: bool| -> (World, Result<Option<Mesh>, String>) {
        let w = new_world();
        let mut h = History::default();
        let m = parse_mch(SINKER_MCH).expect("sinker mch");
        h.execute(&w, LoadMachine::new(&w, "sinker.mch".into(), Some(m)).expect("mount"));
        h.execute(&w, SelectPost::new(&w, PostRef::Named("Sinker EDM (ISO)".into())).expect("post"));
        h.execute(&w, ImportModel::from_mesh("die", mesh.clone()));
        h.execute(&w, CreateSetup::new(&w, None, None).expect("Setup1"));
        let baked = do_align.then(|| align_and_bake(&w, &mut h, &mesh));
        h.execute(&w, SetStock::new(&w, StockShape::Box { dx: 60.0, dy: 60.0, dz: 20.0 }, Location { pos: Point3::new(0.0, 0.0, -10.0), ..Default::default() }).expect("stock"));
        let e = model::find_by_name(&w, "die").expect("die");
        let feat = model::features_of(&w, &e).expect("features");
        let loc = setup_align::pose_in(&w, &e, do_align.then_some("Setup1")).expect("pose");
        let picks: Vec<GeoSel> = feat.faces.iter().filter(|fa| {
            let vs: Vec<Point3> = fa.loops.iter().flat_map(|l| &l.verts).filter_map(|&vi| feat.verts.get(vi as usize)).map(|v| transform_point(Point3::new(v[0], v[1], v[2]), loc.pos, loc.rot)).collect();
            !vs.is_empty() && vs.iter().all(|p| (-1.2..=9.2).contains(&p.x) && (-6.2..=0.2).contains(&p.y) && (3.9..=10.1).contains(&p.z))
        }).map(|fa| GeoSel { entity: "die".into(), feature: FeatureSel::Face(fa.id) }).collect();
        let spec = ElectrodeSpec { undersize: 0.1, orbit: None, stub: 10.0, pickup: 15.0 };
        let cmd = match ElectrodeFromFaces::new(&w, "E-CAV", &picks, &spec) {
            Ok(c) => c,
            Err(err) => return (w, Err(err.to_string())),
        };
        h.execute(&w, cmd);
        let burn = SinkerBurnOp {
            clearance_z: Height::of(HeightReference::TopOfModel, 5.0),
            top_z: Height::of(HeightReference::TopOfModel, 0.0),
            floor_z: Height::fixed(4.0), datum: SinkerDatum::WorkTop, depth: 6.0, x: 4.0, y: -3.0,
            feed: 40.0, condition: "S1".into(), pass: SinkerPass::Rough, cavity_x: 10.2, cavity_y: 6.2,
            cavity_outline: Vec::new(),
            orbit: None, layer: None, wear_lost: None, wear_over: None,
            picks: Default::default(),
        };
        h.execute(&w, CreateOperation::new(&w, "SB1", "E-CAV", SrcRef::Entity("die".into()), OpParams::SinkerBurn(burn.clone())).expect("burn"));
        if add_side {
            let side = SinkerSideBurnOp { burn: SinkerBurnOp { pass: SinkerPass::Single, cavity_x: 0.0, cavity_y: 0.0, ..burn }, direction: SinkerSideDirection::PlusX, amount: 0.04, undersize: 0.1 };
            h.execute(&w, CreateOperation::new(&w, "SS1", "E-CAV", SrcRef::Entity("die".into()), OpParams::SinkerSideBurn(side)).expect("side"));
        }
        for op_name in if add_side { &["SB1", "SS1"][..] } else { &["SB1"][..] } {
            if let Err(err) = GenerateOperation::new(&w, op_name).map(|g| h.execute(&w, g)) {
                return (w, Err(err.to_string()));
            }
        }
        (w, Ok(baked))
    };
    let (tw, tres) = sinker_doc(raw.clone(), true, false);
    let baked = tres.expect("aligned sinker burn generates").expect("baked");
    let (bw, bres) = sinker_doc(baked.clone(), false, false);
    bres.expect("baked sinker burn generates");
    assert_moves_eq(&path_of(&tw, "SB1"), &path_of(&bw, "SB1"));
    assert_eq!(simulate_document(&tw).expect("sim").dev.gouge_cells, 0);
    assert_eq!(posting::post_setup_unwritten(&tw).expect("t").program, posting::post_setup_unwritten(&bw).expect("b").program);

    let (_, ts_res) = sinker_doc(raw.clone(), true, true);
    let err = ts_res.expect_err("aligned side burn needs posted NC proof");
    assert!(err.contains("Sinker Side Burn is not supported yet in an aligned setup"), "{err}");
    let (bs, bs_res) = sinker_doc(baked, false, true);
    bs_res.expect("baked side burn generates");
    assert_eq!(simulate_document(&bs).expect("sim side").dev.gouge_cells, 0);
    // #3001: the ISO post writes a side burn only through the machine's
    // explicit `edm.codes.side`. This synthetic dialect states none, so the
    // post refuses the side burn by name and writes no file.
    let err = posting::post_setup_unwritten(&bs).expect_err("a dialect without edm.codes.side refuses the side burn");
    let err = err.to_string();
    assert!(err.contains("SINKER EDM SYNTHETIC DIALECT (FIXTURE) states no edm.codes.side"), "{err}");
    assert!(!err.contains("edm.codes.depth"), "the straight burn SB1 posts; only the side burn is refused: {err}");

    for add_side in [false, true] {
        let (_, ures) = sinker_doc(raw.clone(), false, add_side);
        assert!(ures.is_err(), "unaligned cavity faces point sideways, not +Z");
    }

    // Explicit `setup` forwarding (`ElectrodeFromFaces::new_in`) derives in the named setup even when Setup1 is unaligned.
    let w = new_world();
    let mut h = History::default();
    let m = parse_mch(SINKER_MCH).expect("sinker mch");
    h.execute(&w, LoadMachine::new(&w, "sinker.mch".into(), Some(m)).expect("mount"));
    h.execute(&w, ImportModel::from_mesh("die", raw));
    h.execute(&w, CreateSetup::new(&w, None, None).expect("Setup1"));
    h.execute(&w, CreateSetup::new(&w, Some("Setup2"), None).expect("Setup2"));
    h.execute(&w, AlignPartToSetup::angles(&w, "Setup2", TURN).expect("align Setup2"));
    h.execute(&w, ActivateSetup::new(&w, Some("Setup1")).expect("activate Setup1"));
    let e = model::find_by_name(&w, "die").expect("die");
    let feat = model::features_of(&w, &e).expect("features");
    let loc = setup_align::pose_in(&w, &e, Some("Setup2")).expect("pose");
    let picks: Vec<GeoSel> = feat.faces.iter().filter(|fa| {
        let vs: Vec<Point3> = fa.loops.iter().flat_map(|l| &l.verts).filter_map(|&vi| feat.verts.get(vi as usize)).map(|v| transform_point(Point3::new(v[0], v[1], v[2]), loc.pos, loc.rot)).collect();
        !vs.is_empty() && vs.iter().all(|p| (-1.2..=9.2).contains(&p.x) && (-6.2..=0.2).contains(&p.y) && (3.9..=10.1).contains(&p.z))
    }).map(|fa| GeoSel { entity: "die".into(), feature: FeatureSel::Face(fa.id) }).collect();
    let spec = ElectrodeSpec { undersize: 0.1, orbit: None, stub: 10.0, pickup: 15.0 };
    assert!(ElectrodeFromFaces::new(&w, "E-ACT", &picks, &spec).is_err(), "active Setup1 is unaligned");
    let cmd = ElectrodeFromFaces::new_in(&w, Some("Setup2"), "E-S2", &picks, &spec).expect("explicit Setup2 derives aligned electrode");
    assert!((cmd.electrode().center.0 - 4.0).abs() < 1e-6 && (cmd.electrode().center.1 - -3.0).abs() < 1e-6);
}

#[test]
fn waterjet_still_refuses_an_aligned_setup_by_name() {
    let w = new_world();
    let mut h = History::default();
    let wj = r#"{"general":{"model":"Waterjet test","kind":"waterjet"},"kinematics":{"default":{"parts":[{"name":"X","type":"linear","travel":{"min":0,"max":460}},{"name":"Y","type":"linear","travel":{"min":-305,"max":0}},{"name":"nozzle","type":"head"}]}},"controller":{"synced_configuration":{"parts":{"X":{"coordinate":0,"max_normal_speed":1000},"Y":{"coordinate":1,"max_normal_speed":700}}}}}"#;
    let m = parse_mch(wj).expect("valid waterjet");
    h.execute(&w, LoadMachine::new(&w, "wj.mch".into(), Some(m)).expect("mount"));
    h.execute(&w, ImportModel::from_mesh("plate", plasma_plate()));
    h.execute(&w, CreateSetup::new(&w, None, None).expect("Setup1"));
    h.execute(&w, AlignPartToSetup::angles(&w, "Setup1", TURN).expect("align"));
    let op = Operation {
        tool: "Nozzle".into(), src: SrcRef::Entity("plate".into()), params: plasma_params(),
        stamp: None, setup: Some("Setup1".into()), area: AreaControls::default(),
        coolant: CoolantMode::default(), after: AfterOperation::default(), suppressed: false,
    };
    let err = setup_align::generation_problem(&w, &op).expect("waterjet stays refused");
    assert!(err.contains("an aligned part is not supported yet on a waterjet machine"), "{err}");
}
