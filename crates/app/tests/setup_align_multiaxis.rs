//! #2286: the 5-axis and rotary kinds read the part, its picked drive
//! chains, its fitted rotary axis, and its heights in the setup frame.
//! Each of the ten kinds must generate, verify, and post identically in
//! a turned setup and with that turn baked into the mesh, while the
//! unaligned as-modelled part cuts a different path or refuses.
//! Provenance: analytic test geometries (spherical cap, blade cylinder,
//! turned bar, box) and rigid-body transform algebra.
//! Gate map: covers app::setup_align, app::operation::selection,
//! app::operation::material_region, app::fixture, app::posting.

use camcraft_app::cl::Toolpath;
use camcraft_app::commands::{
    AddTool, AlignPartToSetup, CreateOperation, CreateSetup, GenerateOperation, History,
    ImportModel, LoadMachine, SetFixtureFixed, SetStock, SetWcsOrigin, TagFixture,
};
use camcraft_app::geometry::{
    Mesh, Point3, Triangle, cube, feature, quat_from_axis_angle, transform_point, IDENTITY_Q,
};
use camcraft_app::links::Links;
use camcraft_app::load::DEFAULT_OPTIMAL_LOAD;
use camcraft_app::machine::{AxisLimits, Machine};
use camcraft_app::machining2d::PocketStrategy;
use camcraft_app::machining3d::Entry3D;
use camcraft_app::operation::{
    self, AfterOperation, AreaControls, CoolantMode, FeatureSel, GeoSel, Height, HeightReference,
    OpParams, OpType, SrcRef, seed_params,
};
use camcraft_app::schema::{Location, Operation};
use camcraft_app::stock::StockShape;
use camcraft_app::tools::{Tool, catalog};
use camcraft_app::{fixture, model, posting, setup_align, world::new_world};
use flecs_ecs::prelude::{EntityViewGet, World};
use std::path::PathBuf;

const BALL: &str = "BN-0600";
const SWARF_BALL: &str = "SWARF-BN-0600";
const TURN: [f64; 3] = [90.0, 0.0, 90.0];
const OBLIQUE: [f64; 3] = [35.0, 25.0, 40.0];

#[derive(Clone, Copy, Debug, PartialEq)]
enum Case { Turned, Baked, Modelled }

fn ok<T, E: std::fmt::Debug>(r: Result<T, E>, what: &str) -> T {
    r.unwrap_or_else(|e| panic!("{what}: {e:?}"))
}

fn unturn_xz(facets: Vec<Triangle>) -> Vec<Triangle> {
    let p = |q: Point3| Point3::new(q.y, q.z, q.x);
    facets.into_iter().map(|t| Triangle::new(p(t.a), p(t.b), p(t.c))).collect()
}

fn tilt_by_inverse(facets: Vec<Triangle>, deg: [f64; 3]) -> Vec<Triangle> {
    let q = setup_align::rotation_from_angles(deg).expect("finite angles");
    let qi = [-q[0], -q[1], -q[2], q[3]];
    let p = |v: Point3| transform_point(v, Point3::ZERO, qi);
    facets.into_iter().map(|t| Triangle::new(p(t.a), p(t.b), p(t.c))).collect()
}

fn stored_turn(raw: &[Triangle], deg: [f64; 3]) -> Location {
    let world = new_world();
    let mut h = History::default();
    h.execute(&world, ImportModel::from_mesh("part", Mesh::from_facets(raw.to_vec())));
    h.execute(&world, ok(CreateSetup::new(&world, None, None), "Setup1"));
    h.execute(&world, ok(AlignPartToSetup::angles(&world, "Setup1", deg), "turn"));
    setup_align::stored(&world, "Setup1").expect("stored alignment")
}

fn bench_machine() -> Machine {
    Machine {
        model: "TestBench 5-Axis".into(),
        kind: Some("mill".into()),
        axes: vec![
            AxisLimits { name: "X".into(), ..Default::default() },
            AxisLimits { name: "Y".into(), ..Default::default() },
            AxisLimits { name: "Z".into(), ..Default::default() },
            AxisLimits { name: "A".into(), rotary: true, axis: Some([1.0, 0.0, 0.0]), ..Default::default() },
            AxisLimits { name: "B".into(), rotary: true, axis: Some([0.0, 1.0, 0.0]), ..Default::default() },
        ],
        ..Default::default()
    }
}

fn box_stock(dx: f64, dy: f64, dz: f64, z: f64) -> (StockShape, Location) {
    (StockShape::Box { dx, dy, dz }, Location { pos: Point3::new(0.0, 0.0, z), rot: IDENTITY_Q })
}

fn cyl_stock(diameter: f64, height: f64, pos: Point3, rot: [f64; 4]) -> (StockShape, Location) {
    (StockShape::Cylinder { diameter, height }, Location { pos, rot })
}

fn document(raw: Vec<Triangle>, deg: [f64; 3], case: Case, stock: &(StockShape, Location), mount: bool) -> (World, History) {
    let world = new_world();
    let mut h = History::default();
    let facets = match case {
        Case::Baked => {
            let loc = stored_turn(&raw, deg);
            let p = |q: Point3| transform_point(q, loc.pos, loc.rot);
            raw.into_iter().map(|t| Triangle::new(p(t.a), p(t.b), p(t.c))).collect()
        }
        Case::Turned | Case::Modelled => raw,
    };
    h.execute(&world, ImportModel::from_mesh("part", Mesh::from_facets(facets)));
    let tool = catalog().into_iter().find(|(n, _)| *n == BALL).map(|(_, t)| t).expect("ball");
    h.execute(&world, ok(AddTool::new(&world, BALL, tool.clone()), "add tool"));
    h.execute(&world, ok(AddTool::new(&world, SWARF_BALL, Tool { flute_len: 60.0, shank_d: tool.diameter, ..tool }), "add swarf tool"));
    h.execute(&world, ok(CreateSetup::new(&world, None, None), "Setup1"));
    if mount {
        h.execute(&world, ok(LoadMachine::new(&world, "bench-5ax.mch".into(), Some(bench_machine())), "mount 5ax"));
    }
    h.execute(&world, ok(SetWcsOrigin::new(&world, Location { pos: Point3::new(0.0, 0.0, 50.0), rot: IDENTITY_Q }), "G54"));
    if case == Case::Turned {
        h.execute(&world, ok(AlignPartToSetup::angles(&world, "Setup1", deg), "align"));
    }
    h.execute(&world, ok(SetStock::new(&world, stock.0.clone(), stock.1), "stock"));
    (world, h)
}

fn generate_and_post(world: &World, h: &mut History, case: Case, tool: &str, src: SrcRef, params: OpParams, mount_tilthead: bool) -> Result<(Toolpath, String, Operation), String> {
    h.execute(world, ok(CreateOperation::new(world, "OP", tool, src, params), "create"));
    let cmd = GenerateOperation::new(world, "OP").map_err(|e| e.to_string())?;
    h.execute(world, cmd);
    let e = model::find_by_name(world, "OP").expect("op");
    let op = e.entity_view(world).try_get::<&Operation>(Clone::clone).expect("Operation");
    let cached = model::toolpath_of(world, &e).ok_or_else(|| format!("no path: {:?}", operation::status_of(world, &e)))?;
    assert!(!cached.path.moves.is_empty(), "{case:?}: empty toolpath");
    if case == Case::Modelled {
        return Ok((cached.path, String::new(), op));
    }
    if mount_tilthead {
        let mch = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets/machines/tilthead.mch");
        h.execute(world, ok(LoadMachine::from_file(world, &mch), "mount tilthead"));
    }
    assert!(!cached.warnings.iter().any(|w| w.blocks_post()), "{case:?}: blocking warnings {:?}", cached.warnings);
    Ok((cached.path, ok(posting::post_setup_unwritten(world), &format!("{case:?}: post")).program, op))
}

fn assert_same_path_and_post(kind: &str, a: &Toolpath, b: &Toolpath, nc_a: &str, nc_b: &str) {
    assert_eq!(a.moves.len(), b.moves.len(), "{kind}: move count");
    for (i, (ma, mb)) in a.moves.iter().zip(&b.moves).enumerate() {
        assert_eq!(ma.kind, mb.kind, "{kind}: move {i} kind");
        let df = (ma.from.x - mb.from.x).abs().max((ma.from.y - mb.from.y).abs()).max((ma.from.z - mb.from.z).abs());
        let dt = (ma.to.x - mb.to.x).abs().max((ma.to.y - mb.to.y).abs()).max((ma.to.z - mb.to.z).abs());
        assert!(df < 1e-5, "{kind}: move {i} from {:?} vs {:?}", ma.from, mb.from);
        assert!(dt < 1e-5, "{kind}: move {i} to {:?} vs {:?}", ma.to, mb.to);
        match (ma.axis, mb.axis) {
            (Some(aa), Some(ab)) => {
                let da = (aa[0] - ab[0]).abs().max((aa[1] - ab[1]).abs()).max((aa[2] - ab[2]).abs());
                assert!(da < 1e-5, "{kind}: move {i} axis {aa:?} vs {ab:?}");
            }
            (None, None) => {}
            _ => panic!("{kind}: move {i} axis presence {:?} vs {:?}", ma.axis, mb.axis),
        }
    }
    assert_eq!(nc_a, nc_b, "{kind}: posted NC differs between turned and baked");
}

fn assert_modelled_differs_or_refuses(kind: &str, turned: &Toolpath, modelled: Result<(Toolpath, String, Operation), String>) {
    if let Ok((m, _, _)) = modelled {
        let differs = m.moves.len() != turned.moves.len()
            || m.moves.iter().zip(&turned.moves).any(|(a, b)| {
                (a.to.x - b.to.x).abs() + (a.to.y - b.to.y).abs() + (a.to.z - b.to.z).abs() > 1e-2
                    || a.axis.zip(b.axis).is_some_and(|(x, y)| (x[0] - y[0]).abs() + (x[1] - y[1]).abs() + (x[2] - y[2]).abs() > 1e-2)
            });
        assert!(differs, "{kind}: as-modelled part cut identically to turned part");
    }
}

fn prove_kind(kind: &str, raw: Vec<Triangle>, deg: [f64; 3], stock: &(StockShape, Location), tool: &str, params: OpParams) {
    let part = SrcRef::Entity("part".into());
    let (wt, mut ht) = document(raw.clone(), deg, Case::Turned, stock, true);
    let (turned, nc_t, _) = generate_and_post(&wt, &mut ht, Case::Turned, tool, part.clone(), params.clone(), false)
        .unwrap_or_else(|e| panic!("{kind}: turned failed: {e}"));
    let (wb, mut hb) = document(raw.clone(), deg, Case::Baked, stock, true);
    let (baked, nc_b, _) = generate_and_post(&wb, &mut hb, Case::Baked, tool, part.clone(), params.clone(), false)
        .unwrap_or_else(|e| panic!("{kind}: baked failed: {e}"));
    assert_same_path_and_post(kind, &turned, &baked, &nc_t, &nc_b);
    let (wm, mut hm) = document(raw, deg, Case::Modelled, stock, true);
    assert_modelled_differs_or_refuses(kind, &turned, generate_and_post(&wm, &mut hm, Case::Modelled, tool, part, params, false));
}

fn cap_facets(capped: bool) -> Vec<Triangle> {
    const TAU: f64 = std::f64::consts::TAU;
    let (rad, max, nl, nc) = (12.0, std::f64::consts::FRAC_PI_3, 4usize, 12usize);
    let s = |x: f64, y: f64, z: f64| Point3::new(x, y, z - 9.0);
    let ring = |k: usize| -> Vec<Point3> {
        let ph = max * k as f64 / nl as f64;
        let (z, rr) = (rad * ph.cos(), rad * ph.sin());
        (0..nc).map(|j| s(rr * (TAU * j as f64 / nc as f64).cos(), rr * (TAU * j as f64 / nc as f64).sin(), z)).collect()
    };
    let (apex, first, mut f) = (s(0.0, 0.0, rad), ring(1), Vec::new());
    for j in 0..nc { f.push(Triangle::new(apex, first[j], first[(j + 1) % nc])); }
    for k in 1..nl {
        let (a, b) = (ring(k), ring(k + 1));
        for j in 0..nc {
            let n = (j + 1) % nc;
            f.push(Triangle::new(a[j], b[j], b[n]));
            f.push(Triangle::new(a[j], b[n], a[n]));
        }
    }
    if capped {
        let (rim, c) = (ring(nl), s(0.0, 0.0, rad * max.cos()));
        for j in 0..nc { f.push(Triangle::new(c, rim[j], rim[(j + 1) % nc])); }
    }
    f
}

fn blade_facets() -> Vec<Triangle> {
    const TAU: f64 = std::f64::consts::TAU;
    let (rad, x_len, nc, nx) = (12.0, 24.0, 16usize, 6usize);
    let v = |j: usize, i: usize| Point3::new(-x_len + 2.0 * x_len * i as f64 / nx as f64, rad * (TAU * j as f64 / nc as f64).cos(), rad * (TAU * j as f64 / nc as f64).sin());
    let mut f = Vec::new();
    for j in 0..nc {
        for i in 0..nx {
            let (a, b, c, d) = (v(j, i), v((j + 1) % nc, i), v((j + 1) % nc, i + 1), v(j, i + 1));
            f.push(Triangle::new(a, b, c));
            f.push(Triangle::new(a, c, d));
        }
        f.push(Triangle::new(Point3::new(x_len, 0.0, 0.0), v(j, nx), v((j + 1) % nc, nx)));
        f.push(Triangle::new(Point3::new(-x_len, 0.0, 0.0), v((j + 1) % nc, 0), v(j, 0)));
    }
    f
}

fn bar_facets() -> Vec<Triangle> {
    const MERIDIAN: [(f64, f64); 6] = [(0.0, -16.0), (6.0, -16.0), (6.0, -2.0), (5.0, 2.0), (5.0, 16.0), (0.0, 16.0)];
    let pt = |rho: f64, z: f64, j: usize| {
        let th = std::f64::consts::TAU * j as f64 / 16.0;
        Point3::new(rho * th.cos(), rho * th.sin(), z)
    };
    let mut f = Vec::new();
    for w in MERIDIAN.windows(2) {
        for j in 0..16 {
            let (a1, a2, b1, b2) = (pt(w[0].0, w[0].1, j), pt(w[0].0, w[0].1, (j + 1) % 16), pt(w[1].0, w[1].1, j), pt(w[1].0, w[1].1, (j + 1) % 16));
            if w[0].0 <= 1e-12 { f.push(Triangle::new(a1, b2, b1)); }
            else if w[1].0 <= 1e-12 { f.push(Triangle::new(a1, a2, b2)); }
            else { f.push(Triangle::new(a1, a2, b2)); f.push(Triangle::new(a1, b2, b1)); }
        }
    }
    f
}

fn finish5d_heights() -> (Height, Height) {
    (Height::of(HeightReference::TopOfModel, -0.5), Height::of(HeightReference::TopOfModel, -2.5))
}

#[test]
fn turned_contour5d_cuts_and_posts_like_the_baked_part() {
    let (top_z, bottom_z) = finish5d_heights();
    let params = OpParams::Contour5D {
        tolerance: 0.1, clearance_z: Height::fixed(20.0), retract_z: None, top_z, bottom_z,
        feed: 500.0, plunge_feed: 100.0, climb: true, lead_deg: 15.0, lag_deg: 0.0, max_tilt_deg: 60.0, shank_avoid: false,
    };
    let stock = box_stock(30.0, 30.0, 10.0, -5.0);
    prove_kind("Contour5D-XZ", unturn_xz(cap_facets(false)), TURN, &stock, BALL, params.clone());
    prove_kind("Contour5D-Oblique", tilt_by_inverse(cap_facets(false), OBLIQUE), OBLIQUE, &stock, BALL, params);
}

#[test]
fn turned_parallel5d_cuts_and_posts_like_the_baked_part() {
    let (top_z, bottom_z) = finish5d_heights();
    let params = OpParams::Parallel5D {
        tolerance: 0.1, stepover: Some(3.0), clearance_z: Height::fixed(20.0), retract_z: None, top_z, bottom_z,
        feed: 500.0, plunge_feed: 100.0, climb: true, lead_deg: 15.0, lag_deg: 0.0, max_tilt_deg: 60.0, shank_avoid: false,
    };
    prove_kind("Parallel5D", unturn_xz(cap_facets(true)), TURN, &box_stock(30.0, 30.0, 10.0, -5.0), BALL, params);
}

#[test]
fn turned_flow5d_cuts_and_posts_like_the_baked_part() {
    let (top_z, bottom_z) = finish5d_heights();
    let params = OpParams::Flow5D {
        tolerance: 0.1, stepover: Some(3.0), clearance_z: Height::fixed(20.0), retract_z: None, top_z, bottom_z,
        feed: 500.0, plunge_feed: 100.0, climb: true, lead_deg: 15.0, lag_deg: 0.0, max_tilt_deg: 60.0, shank_avoid: false,
    };
    prove_kind("Flow5D", unturn_xz(cap_facets(true)), TURN, &box_stock(30.0, 30.0, 10.0, -5.0), BALL, params);
}

#[test]
fn turned_swarf_and_advanced_swarf_cut_and_post_like_the_baked_part() {
    let stock = cyl_stock(24.4, 48.0, Point3::new(-24.0, 0.0, 0.0), quat_from_axis_angle(1, std::f64::consts::FRAC_PI_2));
    let swarf = OpParams::Swarf {
        tolerance: 0.1, stepover: Some(2.0), clearance_z: Height::fixed(25.0), retract_z: None,
        top_z: Height::fixed(11.0), bottom_z: Height::fixed(5.0), feed: 500.0, plunge_feed: 100.0,
        climb: true, lead_deg: 15.0, lag_deg: 0.0, max_tilt_deg: 60.0,
    };
    let adv = OpParams::AdvancedSwarf {
        tolerance: 0.1, stepover: Some(2.0), clearance_z: Height::fixed(25.0), retract_z: None,
        top_z: Height::fixed(11.0), bottom_z: Height::fixed(5.0), feed: 500.0, plunge_feed: 100.0,
        climb: true, lead_deg: 15.0, lag_deg: 0.0, max_tilt_deg: 60.0, passes: 2,
    };
    prove_kind("Swarf", unturn_xz(blade_facets()), TURN, &stock, SWARF_BALL, swarf);
    prove_kind("AdvancedSwarf", unturn_xz(blade_facets()), TURN, &stock, SWARF_BALL, adv);
}

#[test]
fn turned_multiaxis_rough_cuts_and_posts_like_the_baked_part() {
    let params = OpParams::MultiAxisRough {
        links: Links::ROUGH3D, tolerance: 0.3, stepover: 1.5, stock_to_leave: 0.0,
        clearance_z: Height::fixed(20.0), retract_z: None,
        top_z: Height::of(HeightReference::TopOfModel, 0.0),
        bottom_z: Height::of(HeightReference::TopOfModel, -6.0),
        stepdown: 2.0, feed: 800.0, plunge_feed: 200.0, climb: true,
        strategy: PocketStrategy::Zigzag, entry: Entry3D::Plunge, helix_radius: 1.0,
        optimal_load: DEFAULT_OPTIMAL_LOAD, lead_deg: 0.0, lag_deg: 0.0, max_tilt_deg: 90.0, shank_avoid: false,
    };
    prove_kind("MultiAxisRough", unturn_xz(cap_facets(true)), TURN, &box_stock(30.0, 30.0, 6.0, -3.0), SWARF_BALL, params);
}

fn prove_rotary(kind: &str, raw: Vec<Triangle>, stock: &(StockShape, Location), params: OpParams, want_dir: [f64; 3], opts: (bool, bool, bool)) {
    let (mount_before, with_steady, expect_refusal) = opts;
    let plant = |w: &World, h: &mut History| if with_steady {
        let block = cube(Point3::new(-1.0, -1.0, -60.0), Point3::new(1.0, 1.0, -58.0)).facets.to_vec();
        h.execute(w, ImportModel::from_mesh("steady", Mesh::from_facets(block)));
        h.execute(w, ok(TagFixture::new(w, "steady", None), "tag"));
        h.execute(w, ok(SetFixtureFixed::new(w, "steady", true), "fixed"));
    };
    let part = SrcRef::Entity("part".into());
    let (wt, mut ht) = document(raw.clone(), TURN, Case::Turned, stock, mount_before);
    plant(&wt, &mut ht);
    let (p_t, nc_t, op_t) = generate_and_post(&wt, &mut ht, Case::Turned, BALL, part.clone(), params.clone(), !mount_before).unwrap_or_else(|e| panic!("{kind} turned: {e}"));
    let (wb, mut hb) = document(raw.clone(), TURN, Case::Baked, stock, mount_before);
    plant(&wb, &mut hb);
    let (p_b, nc_b, op_b) = generate_and_post(&wb, &mut hb, Case::Baked, BALL, part.clone(), params.clone(), !mount_before).unwrap_or_else(|e| panic!("{kind} baked: {e}"));
    assert_same_path_and_post(kind, &p_t, &p_b, &nc_t, &nc_b);
    let (at, ab) = (fixture::rotary_axis_of(&wt, &op_t).expect("turned axis"), fixture::rotary_axis_of(&wb, &op_b).expect("baked axis"));
    assert!((at.dir[0] * want_dir[0] + at.dir[1] * want_dir[1] + at.dir[2] * want_dir[2]).abs() > 0.999, "{kind}: axis {:?}", at.dir);
    assert!((at.dir[0] - ab.dir[0]).abs() + (at.dir[1] - ab.dir[1]).abs() + (at.dir[2] - ab.dir[2]).abs() < 1e-6);
    let (wm, mut hm) = document(raw, TURN, Case::Modelled, stock, mount_before);
    let modelled = generate_and_post(&wm, &mut hm, Case::Modelled, BALL, part, params, false);
    if expect_refusal { assert!(modelled.is_err(), "{kind}: expected refusal on modelled axis"); } else { assert_modelled_differs_or_refuses(kind, &p_t, modelled); }
}

#[test]
fn turned_rotary_contour_cuts_posts_and_fits_axis_like_the_baked_part() {
    let rc = OpParams::RotaryContour { tolerance: 0.1, clearance_z: Height::fixed(25.0), retract_z: None, top_z: Height::fixed(6.0), bottom_z: Height::fixed(-6.0), feed: 500.0, plunge_feed: 100.0, climb: true, lead_deg: 0.0, lag_deg: 0.0, max_tilt_deg: 60.0 };
    prove_rotary("RotaryContour", unturn_xz(bar_facets()), &cyl_stock(12.4, 32.0, Point3::new(0.0, 0.0, -16.0), IDENTITY_Q), rc, [0.0, 0.0, 1.0], (false, true, true));
}

#[test]
fn turned_rotary_parallel_cuts_posts_and_fits_axis_like_the_baked_part() {
    let rp = OpParams::RotaryParallel { tolerance: 0.1, stepover: Some(4.0), clearance_z: Height::fixed(20.0), retract_z: None, top_z: Height::fixed(6.0), bottom_z: Height::fixed(1.0), feed: 500.0, plunge_feed: 100.0, climb: true, lead_deg: 0.0, lag_deg: 0.0, max_tilt_deg: 60.0 };
    prove_rotary("RotaryParallel", bar_facets(), &cyl_stock(12.4, 32.0, Point3::new(-16.0, 0.0, 0.0), quat_from_axis_angle(1, std::f64::consts::FRAC_PI_2)), rp, [1.0, 0.0, 0.0], (true, false, true));
}

#[test]
fn turned_rotary_offset_cuts_posts_and_fits_axis_like_the_baked_part() {
    let ro = OpParams::RotaryOffset {
        links: Links::ROUGH3D, tolerance: 0.1, stepover: 4.0, stock_to_leave: 0.5,
        clearance_z: Height::fixed(20.0), top_z: Height::fixed(14.0), bottom_z: Height::fixed(4.0),
        stepdown: 2.0, feed: 500.0, plunge_feed: 100.0, climb: true,
        strategy: PocketStrategy::Auto, entry: Entry3D::Plunge, helix_radius: 1.0,
        optimal_load: DEFAULT_OPTIMAL_LOAD, lead_deg: 0.0, lag_deg: 0.0, max_tilt_deg: 60.0,
    };
    prove_rotary("RotaryOffset", unturn_xz(bar_facets()), &cyl_stock(20.0, 32.0, Point3::new(0.0, 0.0, -16.0), IDENTITY_Q), ro, [0.0, 0.0, 1.0], (false, false, false));
}

#[test]
fn turned_multiaxis_contour_routes_drive_chains_through_setup_frame() {
    let run = |deg: [f64; 3], raw: Vec<Triangle>| {
        let loc = stored_turn(&raw, deg);
        let stock = box_stock(20.0, 20.0, 10.0, -5.0);
        let pick_edges = |w: &World, case: Case| -> SrcRef {
            let e = model::find_by_name(w, "part").expect("part");
            let f = model::features_of(w, &e).expect("features");
            let mut items: Vec<([f64; 3], GeoSel)> = feature::chains(&f)
                .into_iter()
                .filter_map(|c| {
                    let pts = feature::chain_points(&f, &c);
                    let sp: Vec<Point3> = pts.iter().map(|p| {
                        let q = Point3::new(p[0], p[1], p[2]);
                        if case == Case::Baked { q } else { transform_point(q, loc.pos, loc.rot) }
                    }).collect();
                    let is_edge = sp.len() == 2 && ((sp[0] - sp[1]).dot(sp[0] - sp[1]).sqrt() - 10.0).abs() < 0.1;
                    (!c.closed && is_edge && sp.iter().all(|s| (s.z - 2.5).abs() < 1e-4)).then(|| {
                        let key = [(sp[0].x + sp[1].x) * 0.5, (sp[0].y + sp[1].y) * 0.5, (sp[0].z + sp[1].z) * 0.5];
                        (key, GeoSel { entity: "part".into(), feature: FeatureSel::Chain(c.id) })
                    })
                })
                .collect();
            assert_eq!(items.len(), 4, "expected the four edges of the turned top face");
            items.sort_by(|a, b| a.0[0].total_cmp(&b.0[0]).then(a.0[1].total_cmp(&b.0[1])).then(a.0[2].total_cmp(&b.0[2])));
            SrcRef::Selection(items.into_iter().map(|(_, g)| g).collect())
        };
        let params = OpParams::MultiAxisContour {
            tolerance: 0.05, clearance_z: Height::fixed(20.0), retract_z: None,
            top_z: Height::of(HeightReference::TopOfModel, 0.5),
            bottom_z: Height::of(HeightReference::TopOfModel, -1.0),
            feed: 500.0, plunge_feed: 120.0, climb: true,
            lead_deg: 0.0, lag_deg: 0.0, max_tilt_deg: 60.0, shank_avoid: false,
        };
        let (wt, mut ht) = document(raw.clone(), deg, Case::Turned, &stock, true);
        let (p_t, nc_t, _) = generate_and_post(&wt, &mut ht, Case::Turned, BALL, pick_edges(&wt, Case::Turned), params.clone(), false).expect("mac turned");
        let (wb, mut hb) = document(raw.clone(), deg, Case::Baked, &stock, true);
        let (p_b, nc_b, _) = generate_and_post(&wb, &mut hb, Case::Baked, BALL, pick_edges(&wb, Case::Baked), params.clone(), false).expect("mac baked");
        assert_same_path_and_post("MultiAxisContour", &p_t, &p_b, &nc_t, &nc_b);
        let (wm, mut hm) = document(raw, deg, Case::Modelled, &stock, true);
        assert_modelled_differs_or_refuses("MultiAxisContour", &p_t, generate_and_post(&wm, &mut hm, Case::Modelled, BALL, pick_edges(&wm, Case::Modelled), params, false));
    };
    let centered_box = cube(Point3::new(-5.0, -5.0, -2.5), Point3::new(5.0, 5.0, 2.5)).facets.to_vec();
    run(TURN, unturn_xz(centered_box.clone()));
    run(OBLIQUE, tilt_by_inverse(centered_box, OBLIQUE));
}

#[test]
fn all_ten_multiaxis_and_rotary_kinds_stay_refused_on_aligned_mill_turn() {
    let stock = box_stock(30.0, 30.0, 10.0, -5.0);
    let (w, mut h) = document(unturn_xz(cap_facets(true)), TURN, Case::Turned, &stock, false);
    let mill_turn = Machine {
        model: "TestBench MillTurn".into(),
        kind: Some("mill-turn".into()),
        ..bench_machine()
    };
    h.execute(&w, ok(LoadMachine::new(&w, "bench-mt.mch".into(), Some(mill_turn)), "mount mill-turn"));

    let multiaxis_kinds = [
        OpType::Contour5D,
        OpType::Parallel5D,
        OpType::Swarf,
        OpType::AdvancedSwarf,
        OpType::RotaryContour,
        OpType::RotaryParallel,
        OpType::RotaryOffset,
        OpType::MultiAxisRough,
        OpType::Flow5D,
        OpType::MultiAxisContour,
    ];
    for (idx, ty) in multiaxis_kinds.into_iter().enumerate() {
        let params = seed_params(ty.ordinal());
        let op = Operation {
            tool: BALL.into(),
            src: SrcRef::Entity("part".into()),
            params: params.clone(),
            stamp: None,
            setup: Some("Setup1".into()),
            area: AreaControls::default(),
            coolant: CoolantMode::default(),
            after: AfterOperation::default(),
            suppressed: false,
        };
        let err = setup_align::generation_problem(&w, &op)
            .unwrap_or_else(|| panic!("{ty:?} must stay refused on aligned mill-turn"));
        assert!(err.contains("not supported yet in an aligned setup on a mill-turn machine"), "{ty:?}: {err}");
        assert!(!err.contains("5-Axis Contour,"), "{ty:?}: mill-turn refusal should use machine branch: {err}");

        let name = format!("MT{idx}");
        h.execute(&w, ok(CreateOperation::new(&w, &name, BALL, SrcRef::Entity("part".into()), params), "create"));
        let gen_err = GenerateOperation::new(&w, &name)
            .err()
            .unwrap_or_else(|| panic!("{ty:?} GenerateOperation must stay refused on aligned mill-turn"))
            .to_string();
        assert!(gen_err.contains("not supported yet in an aligned setup on a mill-turn machine"), "{ty:?}: {gen_err}");
    }

    // Mill still names all 27 verified kinds in its Mill-only refusal branch.
    let (wm, _) = document(unturn_xz(cap_facets(true)), TURN, Case::Turned, &stock, true);
    let probe = Operation {
        tool: "PRB-0400".into(),
        src: SrcRef::Entity("part".into()),
        params: seed_params(OpType::Probe.ordinal()),
        stamp: None,
        setup: Some("Setup1".into()),
        area: AreaControls::default(),
        coolant: CoolantMode::default(),
        after: AfterOperation::default(),
        suppressed: false,
    };
    let mill_err = setup_align::generation_problem(&wm, &probe).expect("Probe refused on aligned mill");
    assert!(mill_err.contains("5-Axis Contour") && mill_err.contains("Multi-Axis Contour"), "{mill_err}");
}
