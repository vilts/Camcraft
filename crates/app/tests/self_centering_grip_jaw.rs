//! #1274: a self-centering vise's grip jaw - a step cut into each jaw's
//! inside top edge that the stock sits on. The jaw bodies carry the
//! step (closed shell, steel and air where they belong); a fitted vise
//! seats the stock on the ledge, the vise floor `jaw_height -
//! grip_depth` below it; the fixture law judges the stepped jaw (a face
//! pass clear of the lowered jaws posts, one cut into the step is
//! refused); bad figures are refused by name; an older file (no grip
//! keys) loads and saves unchanged, a grip vise round-trips, and the
//! edit is one undo; parallels under a grip vise are refused by name.
//!
//! Gate map (M4.4): covers app::vise, app::vise::self_centering,
//! app::commands::builtin_vise, app::project (fixture vise key),
//! app::fixture, app::posting, app::commands::accessory.

use camcraft_app::accessory::Accessory;
use camcraft_app::commands::{
    AccessoryPlacement, AddTool, CreateOperation, GenerateOperation, History, InsertAccessory, InsertVise, SetStock, SetVise,
    VisePlacement,
};
use camcraft_app::fixture;
use camcraft_app::geometry::{IDENTITY_Q, Point3, Triangle};
use camcraft_app::model;
use camcraft_app::operation::{Height, OpParams, SrcRef};
use camcraft_app::posting;
use camcraft_app::project::{self, ViseDto};
use camcraft_app::schema::Location;
use camcraft_app::stock::StockShape;
use camcraft_app::tools::catalog;
use camcraft_app::vise::{self, ViseParams, ViseStyle};
use camcraft_app::world::new_world;
use camcraft_sim::fixture_mesh::FixtureMesh;
use flecs_ecs::prelude::World;

const SC80: &str = "80 mm self-centering";

fn row(label: &str) -> ViseParams {
    vise::size(label).map(vise::ViseSize::half_open).expect(label)
}

/// `p` with a grip step `depth` down and `width` in.
fn grip(p: &ViseParams, depth: f64, width: f64) -> ViseParams {
    let mut q = p.clone();
    if let ViseStyle::SelfCentering(sc) = &mut q.style {
        sc.grip_depth = depth;
        sc.grip_width = width;
    }
    q
}

fn volume(tris: &[Triangle]) -> f64 {
    tris.iter()
        .map(|t| {
            let (a, b, c) = (t.a, t.b, t.c);
            a.x * (b.y * c.z - b.z * c.y) - a.y * (b.x * c.z - b.z * c.x) + a.z * (b.x * c.y - b.y * c.x)
        })
        .sum::<f64>()
        / 6.0
}

#[test]
fn the_grip_jaw_carries_the_step_and_its_seat_is_the_frame_origin() {
    let p = row(SC80).with_opening(40.0);
    let q = grip(&p, 3.0, 1.5);
    assert_eq!(q.seat_height(), 12.0, "15 mm jaw, 3 mm step: the ledge 12 mm over the floor");
    assert_eq!(p.seat_height(), 0.0, "no step: the floor");
    let tris = q.mesh().expect("meshes");
    // Plain jaw bodies plus the two ledges (1.5 wide, 12 high).
    let want = (p.length * p.base_height + 2.0 * p.jaw_body_depth * p.jaw_height + 2.0 * 1.5 * 12.0) * p.width;
    let v = volume(&tris);
    assert!((v - want).abs() < 1e-6 * want, "outward volume {v} vs {want}");
    let lo = tris.iter().flat_map(|t| [t.a.z, t.b.z, t.c.z]).fold(f64::MAX, f64::min);
    let hi = tris.iter().flat_map(|t| [t.a.z, t.b.z, t.c.z]).fold(f64::MIN, f64::max);
    assert!((lo + 12.0 + p.base_height).abs() < 1e-9, "the body drops by the seat: {lo}");
    assert!((hi - 3.0).abs() < 1e-9, "the jaw top stands the step's depth over the seat: {hi}");
    let m = FixtureMesh::build(&tris).expect("the mesh law takes it");
    for s in [1.0, -1.0] {
        assert!(m.contains(Point3::new(0.0, s * 19.25, -1.0)), "the ledge under the stock is steel");
        assert!(!m.contains(Point3::new(0.0, s * 19.25, 1.0)), "over the ledge, the stock's room");
        assert!(m.contains(Point3::new(0.0, s * 20.5, 1.0)), "the jaw grips above the ledge");
        assert!(!m.contains(Point3::new(0.0, s * 18.0, -1.0)), "between the ledges is air");
    }
}

#[test]
fn bad_grip_figures_are_refused_by_name() {
    let p = row(SC80).with_opening(40.0);
    let refused = |q: ViseParams, word: &str| {
        let e = q.mesh().expect_err(word);
        assert!(e.contains(word), "'{e}' names {word}");
    };
    refused(grip(&p, 3.0, 0.0), "grip step needs both a depth and a width");
    refused(grip(&p, -3.0, 1.5), "grip step depth must not be negative");
    refused(grip(&p, 3.0, f64::NAN), "grip step width must be a finite length");
    refused(grip(&p, 15.0, 1.5), "grip step depth 15.000 mm must be less than its jaw height");
    refused(grip(&p, 3.0, 12.0), "grip step width 12.000 mm must be less than its jaw thickness");
    refused(grip(&p.with_opening(3.0), 3.0, 1.5), "the ledges would meet under the stock");
    let mut both = grip(&p, 3.0, 1.5);
    if let ViseStyle::SelfCentering(sc) = &mut both.style {
        sc.lip_height = 2.0;
        sc.lip_depth = 4.0;
    }
    refused(both, "a dovetail or a grip jaw, not both");
}

/// Stock 60 x 30 x 10, centred on the origin, bottom at z 0.
fn stock(world: &World, history: &mut History) {
    let loc = Location { pos: Point3::new(0.0, 0.0, 0.0), rot: IDENTITY_Q };
    history.execute(world, SetStock::new(world, StockShape::Box { dx: 60.0, dy: 30.0, dz: 10.0 }, loc).expect("stock"));
}

fn fitted(world: &World, history: &mut History, p: &ViseParams) -> String {
    let cmd = InsertVise::new(world, p, None, VisePlacement::FitStock).expect("fits");
    let name = cmd.name().to_owned();
    history.execute(world, cmd);
    name
}

fn params_of(world: &World, name: &str) -> Option<ViseParams> {
    model::find_by_name(world, name).and_then(|e| fixture::def_of(world, &e)).and_then(|d| d.vise)
}

/// The lowest world z of fixture `name` (its floor's underside).
fn bottom(world: &World, name: &str) -> f64 {
    let e = model::find_by_name(world, name).expect("placed");
    let tris = model::world_facets(world, &e).expect("facets");
    tris.iter().flat_map(|t| [t.a.z, t.b.z, t.c.z]).fold(f64::MAX, f64::min)
}

#[test]
fn a_grip_vise_seats_the_stock_on_the_ledge() {
    let (plain, gripped) = (new_world(), new_world());
    let (mut hp, mut hg) = (History::default(), History::default());
    stock(&plain, &mut hp);
    stock(&gripped, &mut hg);
    let np = fitted(&plain, &mut hp, &row(SC80));
    let ng = fitted(&gripped, &mut hg, &grip(&row(SC80), 3.0, 1.5));
    let base = row(SC80).base_height;
    assert!((bottom(&plain, &np) + base).abs() < 1e-9, "bed-seated: the floor under the stock");
    let drop = bottom(&plain, &np) - bottom(&gripped, &ng);
    assert!((drop - 12.0).abs() < 1e-9, "on a 3 mm step of a 15 mm jaw the vise sits 12 mm lower: {drop}");
    // The stock rests on the ledges and the jaws grip its lowest 3 mm.
    let e = model::find_by_name(&gripped, &ng).expect("placed");
    let m = FixtureMesh::build(&model::world_facets(&gripped, &e).expect("facets")).expect("mesh");
    assert!(m.contains(Point3::new(0.0, 14.25, -0.5)) && !m.contains(Point3::new(0.0, 14.25, 0.5)), "a ledge under the stock's edge");
    assert!(m.contains(Point3::new(0.0, 15.5, 2.5)) && !m.contains(Point3::new(0.0, 15.5, 3.5)), "the jaw grips to z 3");
}

/// A face pass over the stock at world z `cut`, generated.
fn face(world: &World, history: &mut History, cut: f64) {
    let tool = catalog().into_iter().find(|(n, _)| *n == "EM-0400-2FL").map(|(_, t)| t).expect("catalog");
    history.execute(world, AddTool::new(world, "EM-0400-2FL", tool).expect("tool"));
    let op = OpParams::Face {
        holes: Default::default(),
        tilt: None,
        blend_turns: false,
        stepover: 3.0,
        clearance_z: Height::fixed(25.0),
        cut_z: Height::fixed(cut),
        top_z: None,
        stepdown: None,
        feed: 1200.0,
        plunge_feed: 300.0,
        climb: true,
    };
    history.execute(world, CreateOperation::new(world, "Face1", "EM-0400-2FL", SrcRef::Stock, op).expect("face op"));
    history.execute(world, GenerateOperation::new(world, "Face1").expect("generate"));
}

fn struck(world: &World) -> bool {
    posting::verification_gate(world).iter().any(|pr| pr.starts_with("Face1:") && pr.contains("strikes a fixture"))
}

#[test]
fn the_fixture_law_judges_the_stepped_jaw() {
    // A face pass at z 9.5 overhangs the 30 mm stock's sides: bed-seated,
    // the 15 mm jaws stand in its way; on a 3 mm grip step they reach z 3.
    let world = new_world();
    let mut history = History::default();
    stock(&world, &mut history);
    face(&world, &mut history, 9.5);
    let name = fitted(&world, &mut history, &row(SC80));
    assert!(struck(&world), "bed-seated jaws stand in the pass");
    let edit = SetVise::new(&world, &name, grip(&params_of(&world, &name).expect("vise"), 3.0, 1.5)).expect("grip");
    history.execute(&world, edit);
    let problems = posting::verification_gate(&world);
    assert!(problems.is_empty(), "the stock seated on the step clears the pass: {problems:?}");

    // A pass at z 1 runs into the step's 3 mm of jaw: refused.
    let low = new_world();
    let mut h = History::default();
    stock(&low, &mut h);
    face(&low, &mut h, 1.0);
    fitted(&low, &mut h, &grip(&row(SC80), 3.0, 1.5));
    assert!(struck(&low), "a cut into the jaw step blocks the post");
}

#[test]
fn an_older_vise_key_reads_and_writes_unchanged_and_a_grip_round_trips() {
    // A key written before #1274: no grip fields.
    let old = r#"{"size":"80 mm self-centering","jaw_width":80.0,"jaw_height":15.0,"jaw_thickness":12.0,"jaw_body_depth":25.0,"max_opening":127.0,"length":182.0,"width":80.0,"base_height":50.0,"housing_length":0.0,"housing_height":0.0,"opening":40.0,"self_centering":{"min_opening":0.0,"lip_height":0.0,"lip_depth":0.0}}"#;
    let dto: ViseDto = serde_json::from_str(old).expect("parses");
    let p = dto.to_params();
    assert!(matches!(p.style, ViseStyle::SelfCentering(sc) if !sc.has_grip()), "no step");
    assert_eq!(serde_json::to_string(&ViseDto::from_params(&p)).expect("writes"), old, "and writes back byte for byte");

    let world = new_world();
    let mut history = History::default();
    stock(&world, &mut history);
    let name = fitted(&world, &mut history, &row(SC80));
    let e = model::find_by_name(&world, &name).expect("placed");
    let before = model::facets_of(&world, &e).expect("facets").to_vec();
    let gripped = grip(&params_of(&world, &name).expect("vise"), 3.0, 1.5);
    history.execute(&world, SetVise::new(&world, &name, gripped.clone()).expect("edit"));
    let path = std::env::temp_dir().join(format!("camcraft-grip-vise-{}.camproj", std::process::id()));
    project::save(&world, &path).expect("save");
    let text = std::fs::read_to_string(&path).expect("read");
    assert!(text.contains("\"grip_depth\"") && text.contains("\"grip_width\""), "the step is saved");
    let back = new_world();
    project::load(&back, &path).expect("load");
    let _ = std::fs::remove_file(&path);
    assert_eq!(params_of(&back, &name), Some(gripped), "the step reloads");
    assert!(history.undo(&world));
    assert_eq!(model::facets_of(&world, &e).expect("facets").to_vec(), before, "one undo takes the step");
    assert!(history.redo(&world));
    assert_eq!(params_of(&world, &name).map(|p| p.seat_height()), Some(12.0), "redo puts it back");
}

/// Parallels fit from the vise floor; a grip vise seats the stock on its
/// ledges, so "parallels under the stock" there would stand the stock
/// clear of the jaws. It is refused by name; a stepless vise still fits.
#[test]
fn parallels_are_refused_under_a_grip_jaw_vise() {
    let par = Accessory::Parallels { height: 25.4, thickness: 3.175, length: 152.4, spacing: 101.6 };
    let (plain, gripped) = (new_world(), new_world());
    let (mut hp, mut hg) = (History::default(), History::default());
    stock(&plain, &mut hp);
    stock(&gripped, &mut hg);
    let np = fitted(&plain, &mut hp, &row(SC80));
    let ng = fitted(&gripped, &mut hg, &grip(&row(SC80), 3.0, 1.5));
    let under = |vise: String| AccessoryPlacement::UnderStockInVise { vise, above: 2.0 };
    assert!(InsertAccessory::new(&plain, &par, None, under(np)).is_ok(), "a stepless vise takes parallels");
    let err = InsertAccessory::new(&gripped, &par, None, under(ng)).err().map(|e| e.to_string()).unwrap_or_default();
    assert!(err.contains("grip-jaw vise") && err.contains("parallels do not go under it"), "refused by name: {err}");
}
