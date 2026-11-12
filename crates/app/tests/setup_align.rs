//! #326: a setup holds the part aligned (flipped, turned or as modelled)
//! through an undoable command, saved with the project. Generation, the
//! stock and the post run in that setup's frame; an as-modelled setup is
//! byte-identical to before; the work offset is untouched (a plain
//! translation placed in the setup frame); every op kind not yet verified
//! in an aligned setup is refused by name.
//! Provenance: our own fixtures (a block with a blind square hole) over
//! plain re-clamp practice; no vendor material consulted.
//!
//! Gate map (#326): covers app::setup_align, app::commands
//! (AlignPartToSetup, MoveModel, CreateSetup), app::operation (job,
//! material, heights, stamp), app::stock_sim, sim::stock_sim (part_facets),
//! app::project (save, load, open), app::posting.
//! Lane shape (M4.4): all tests here run in seconds.

use flecs_ecs::prelude::World;

use camcraft_app::commands::{
    AddTool, AlignPartToSetup, CreateOperation, CreateSetup, GenerateOperation, History, ImportModel,
    SetStock,
};
use camcraft_app::geometry::{Mesh, Point3, Triangle, cube};
use camcraft_app::machining2d::{Lead, Side};
use camcraft_app::machining3d::Entry3D;
use camcraft_app::operation::{self, Carried, FeatureSel, GeoSel, Height, KindInfo, OpParams, OpStatus, OpType, SrcRef};
use camcraft_app::schema::Location;
use camcraft_app::setup_align::{self, AlignPreset};
use camcraft_app::stock::StockShape;
use camcraft_app::tools::catalog;
use camcraft_app::world::new_world;
use camcraft_app::{model, posting, project};

const TOOL: &str = "EM-0400-2FL";
/// Half the hole's side (the hole is 20 x 20, the block 40 x 40 x 20).
const H: f64 = 10.0;

fn flip(t: &Triangle) -> Triangle {
    Triangle::new(t.a, t.c, t.b)
}

/// A 40 x 40 x [0, 20] block with a 20 x 20 square hole from the top,
/// blind to Z 10. Outward-wound. Flipped over X, the hole opens down.
fn block_with_blind_hole() -> Mesh {
    let quad = |a: Point3, b: Point3, c: Point3, d: Point3, f: &mut Vec<Triangle>| {
        f.push(Triangle::new(a, b, c));
        f.push(Triangle::new(a, c, d));
    };
    let mut f = Vec::new();
    let outer = cube(Point3::new(-20.0, -20.0, 0.0), Point3::new(20.0, 20.0, 20.0));
    f.extend(outer.facets[4..12].iter().cloned());
    let o = [(-20.0, -20.0), (20.0, -20.0), (20.0, 20.0), (-20.0, 20.0)];
    let i = [(-H, -H), (H, -H), (H, H), (-H, H)];
    for k in 0..4 {
        let n = (k + 1) % 4;
        let p = |(x, y): (f64, f64)| Point3::new(x, y, 20.0);
        quad(p(o[k]), p(o[n]), p(i[n]), p(i[k]), &mut f);
    }
    let inner = cube(Point3::new(-H, -H, 10.0), Point3::new(H, H, 20.0));
    f.extend(inner.facets[4..12].iter().map(flip));
    f.extend(outer.facets[0..2].iter().cloned());
    f.extend(inner.facets[0..2].iter().map(flip));
    Mesh::from_facets(f)
}

fn contour_params() -> OpParams {
    OpParams::Contour {
        stock_to_leave: 0.0,
        clearance_z: Height::fixed(40.0),
        retract_z: Height::fixed(30.0),
        top_z: Height::fixed(20.0),
        bottom_z: Height::fixed(12.0),
        stepdown: 4.0,
        feed: 1000.0,
        plunge_feed: 300.0,
        climb: true,
        side: Side::Auto,
        lead: Lead::None,
        tolerance: 0.01,
        tab_count: 0,
        tab_width: 5.0,
        tab_height: 1.0,
        entry: Entry3D::Plunge,
        helix_radius: 1.0,
        blend_turns: false,
        tilt: None,
        links: camcraft_app::links::Links::PLANAR,
    }
}

/// The part, a 60 x `dy` x 22 stock from Z -2, the tool, Setup1, and a
/// whole-model Contour from Z 20 down to Z 12, generated.
fn document(dy: f64) -> (World, History) {
    let world = new_world();
    let mut h = History::default();
    h.execute(&world, ImportModel::from_mesh("part", block_with_blind_hole()));
    let at = Location { pos: Point3::new(0.0, 0.0, -2.0), ..Default::default() };
    let stock = SetStock::new(&world, StockShape::Box { dx: 60.0, dy, dz: 22.0 }, at);
    h.execute(&world, stock.expect("stock"));
    let tool = catalog().into_iter().find(|(n, _)| *n == TOOL).map(|(_, t)| t).expect("tool");
    h.execute(&world, AddTool::new(&world, TOOL, tool).expect("add tool"));
    h.execute(&world, CreateSetup::new(&world, None, None).expect("materialize Setup1"));
    let op = CreateOperation::new(&world, "Contour1", TOOL, SrcRef::Entity("part".into()), contour_params());
    h.execute(&world, op.expect("create"));
    generate(&world, &mut h, "Contour1").expect("generates as modelled");
    (world, h)
}

fn generate(world: &World, h: &mut History, name: &str) -> Result<(), String> {
    let cmd = GenerateOperation::new(world, name).map_err(|e| e.to_string())?;
    h.execute(world, cmd);
    Ok(())
}

fn align(world: &World, h: &mut History, preset: AlignPreset) {
    h.execute(world, AlignPartToSetup::preset(world, "Setup1", preset).expect("align"));
}

fn status(world: &World, name: &str) -> OpStatus {
    let e = model::find_by_name(world, name).expect("op");
    operation::status_of(world, &e)
}

fn post(world: &World, tag: &str) -> Result<String, String> {
    let mut path = std::env::temp_dir();
    path.push(format!("camcraft-align-{tag}-{}.nc", std::process::id()));
    let out = posting::post_setup(world, &path).map(|p| p.program);
    let _ = std::fs::remove_file(&path);
    out
}

/// Every (X, Y) the program cuts at below Z 19.9, with modal axes.
fn cut_points(nc: &str) -> Vec<(f64, f64)> {
    let (mut x, mut y, mut z) = (f64::NAN, f64::NAN, f64::NAN);
    let mut out = Vec::new();
    for line in nc.lines().filter(|l| !l.contains("G28")) {
        for t in line.split_whitespace() {
            let mut c = t.chars();
            let (Some(a), Ok(v)) = (c.next(), c.as_str().parse::<f64>()) else { continue };
            match a {
                'X' => x = v,
                'Y' => y = v,
                'Z' => z = v,
                _ => {}
            }
        }
        if z < 19.9 && x.is_finite() && y.is_finite() {
            out.push((x, y));
        }
    }
    out
}

fn stock_now(world: &World) -> (StockShape, Location) {
    let e = model::find_by_name(world, "Stock").expect("stock");
    (model::stock_def_of(world, &e).expect("shape"), model::transform_of(world, &e).expect("loc"))
}

#[test]
fn as_modelled_is_byte_identical_and_the_stamp_returns_fresh() {
    let (world, mut h) = document(60.0);
    let before = post(&world, "id0").expect("posts as modelled");
    align(&world, &mut h, AlignPreset::FlipX);
    assert_eq!(status(&world, "Contour1"), OpStatus::Stale, "turning the part is a moved input");
    assert!(post(&world, "stale").is_err(), "a stale op never posts");
    align(&world, &mut h, AlignPreset::AsModelled);
    assert_eq!(setup_align::stored(&world, "Setup1"), None, "as modelled is stored as nothing");
    assert_eq!(status(&world, "Contour1"), OpStatus::Ok, "the stamp is the pre-#326 stamp again");
    generate(&world, &mut h, "Contour1").expect("regenerates");
    assert_eq!(post(&world, "id1").expect("posts"), before, "byte-identical program");
}

#[test]
fn a_flipped_setup_contours_the_former_bottom_and_posts_in_its_frame() {
    let (world, mut h) = document(60.0);
    let modelled = cut_points(&post(&world, "m").expect("posts"));
    let near = |pts: &[(f64, f64)], r: f64| pts.iter().any(|(x, y)| (x.abs().max(y.abs()) - r).abs() < 0.01);
    // As modelled the cut levels (Z 16, 12) run the hole's ring, 2 mm in
    // from its 10 mm wall, and the outer ring 2 mm out from 20.
    assert!(near(&modelled, 8.0) && near(&modelled, 22.0), "{modelled:?}");

    align(&world, &mut h, AlignPreset::FlipX);
    let a = setup_align::stored(&world, "Setup1").expect("stored");
    assert_eq!(a.rot, [1.0, 0.0, 0.0, 0.0], "flip over X is the exact half turn");
    assert_eq!([a.pos.x, a.pos.y, a.pos.z], [0.0, 0.0, 20.0], "about the part's centre, Z 10");
    // The stock follows, square to the setup frame: what stood 2 mm under
    // the part stands 2 mm over its new top.
    let (shape, loc) = stock_now(&world);
    assert_eq!(shape, StockShape::Box { dx: 60.0, dy: 60.0, dz: 22.0 });
    assert_eq!((loc.pos, loc.rot), (Point3::new(0.0, 0.0, 0.0), [0.0, 0.0, 0.0, 1.0]));

    generate(&world, &mut h, "Contour1").expect("generates in the flipped frame");
    let nc = post(&world, "f").expect("posts");
    assert!(nc.contains("G54"), "the work offset is untouched: {nc}");
    let flipped = cut_points(&nc);
    // Flipped, Z 12..20 is the former bottom: solid, no hole - only the
    // outer ring, at the same 22 mm.
    assert!(near(&flipped, 22.0), "{flipped:?}");
    assert!(flipped.iter().all(|(x, y)| x.abs().max(y.abs()) > 21.9), "no hole ring: {flipped:?}");
}

#[test]
fn a_quarter_turn_rebuilds_the_stock_square_and_undo_redo_walk_it() {
    let (world, mut h) = document(40.0);
    let before = stock_now(&world);
    align(&world, &mut h, AlignPreset::TurnZ90);
    let (shape, loc) = stock_now(&world);
    assert_eq!(shape, StockShape::Box { dx: 40.0, dy: 60.0, dz: 22.0 }, "the 60 x 40 block lies 40 x 60");
    assert_eq!(loc.rot, [0.0, 0.0, 0.0, 1.0], "square to the setup frame");
    assert!((loc.pos.z + 2.0).abs() < 1e-9 && loc.pos.x.abs() < 1e-9 && loc.pos.y.abs() < 1e-9, "{loc:?}");
    assert!(h.undo(&world));
    assert_eq!(setup_align::stored(&world, "Setup1"), None);
    assert_eq!(stock_now(&world), before, "undo restores the stock exactly");
    assert!(h.redo(&world));
    assert_eq!(stock_now(&world).0, shape);
    assert!(setup_align::stored(&world, "Setup1").is_some(), "redo restores the turn");
    // Validated at construction: no repeat, no non-setup, finite angles.
    assert!(AlignPartToSetup::preset(&world, "Setup1", AlignPreset::TurnZ90).is_err());
    assert!(AlignPartToSetup::preset(&world, "Contour1", AlignPreset::FlipX).is_err());
    assert!(AlignPartToSetup::angles(&world, "Setup1", [f64::NAN, 0.0, 0.0]).is_err());
    // Numeric angles and the preset are one rotation.
    let typed = AlignPartToSetup::angles(&world, "Setup1", [0.0, 0.0, 90.0]);
    assert!(typed.is_err(), "typing 90 about Z is the turn already held");
}

#[test]
fn two_setups_hold_the_part_differently_without_touching_each_other() {
    let (world, mut h) = document(60.0);
    h.execute(&world, CreateSetup::new(&world, None, Some("Setup1")).expect("Setup2"));
    h.execute(&world, AlignPartToSetup::preset(&world, "Setup2", AlignPreset::FlipX).expect("align 2"));
    assert_eq!(setup_align::stored(&world, "Setup1"), None);
    assert_eq!(status(&world, "Contour1"), OpStatus::Ok, "Setup1's op is untouched");
    let e = model::find_by_name(&world, "Setup2/Stock").expect("own stock");
    assert_eq!(model::transform_of(&world, &e).map(|l| l.pos.z), Some(0.0), "Setup2's stock followed");
    assert_eq!(stock_now(&world).1.pos.z, -2.0, "Setup1's stock did not");
    // A setup holding the part turned shows it turned: the bounds read
    // in Setup2's frame keep the box, the part's top face is solid.
    let b = setup_align::solid_aabb_in(&world, Some("Setup2")).expect("bounds");
    assert!((b.min.z - 0.0).abs() < 1e-9 && (b.max.z - 20.0).abs() < 1e-9);
}

#[test]
fn unverified_kinds_and_picks_are_refused_by_name_in_an_aligned_setup() {
    let (world, mut h) = document(60.0);
    let seed = (KindInfo::ALL[OpType::Pocket3D.ordinal()].seed)(&Carried::of(&contour_params()));
    let op = CreateOperation::new(&world, "Rough3D", TOOL, SrcRef::Entity("part".into()), seed);
    h.execute(&world, op.expect("create 3D pocket"));
    let face = model::find_by_name(&world, "part")
        .and_then(|e| model::features_of(&world, &e))
        .and_then(|f| f.faces.first().map(|fa| fa.id))
        .expect("a face");
    let pick = SrcRef::Selection(vec![GeoSel { entity: "part".into(), feature: FeatureSel::Face(face) }]);
    let picked = CreateOperation::new(&world, "Picked", TOOL, pick, contour_params());
    h.execute(&world, picked.expect("create a face-picked contour"));
    align(&world, &mut h, AlignPreset::FlipY);
    let err = generate(&world, &mut h, "Rough3D").expect_err("3D pocket is not verified aligned");
    assert!(err.contains("part alignment") && err.contains("not supported yet"), "{err}");
    let err = generate(&world, &mut h, "Picked").expect_err("picks are not verified aligned");
    assert!(err.contains("picked faces"), "{err}");
    generate(&world, &mut h, "Contour1").expect("the verified kind generates");
}

#[test]
fn the_alignment_round_trips_and_older_files_load_as_modelled() {
    let (world, mut h) = document(60.0);
    align(&world, &mut h, AlignPreset::FlipX);
    let mut path = std::env::temp_dir();
    path.push(format!("camcraft-align-proj-{}.ccam", std::process::id()));
    project::save(&world, &path).expect("save");
    let first = std::fs::read_to_string(&path).expect("read");
    assert!(first.contains("\"part_alignment\""), "{first}");
    let world2 = new_world();
    project::load(&world2, &path).expect("load");
    assert_eq!(setup_align::stored(&world2, "Setup1"), setup_align::stored(&world, "Setup1"));
    project::save(&world2, &path).expect("resave");
    let second = std::fs::read_to_string(&path).expect("reread");
    assert_eq!(first, second, "byte-identical round trip");
    assert!(h.undo(&world));
    project::save(&world, &path).expect("save as modelled");
    let plain = std::fs::read_to_string(&path).expect("read");
    let _ = std::fs::remove_file(&path);
    assert!(!plain.contains("part_alignment"), "as modelled writes no key");

    // A v52 file (no key) is as modelled; a non-rigid one is corrupt.
    let old = r#"{"version": 52, "entities": [
        {"name": "Setup1", "translation": [0.0, 0.0, 0.0], "kind": "setup",
         "setup": {"stock": "Stock", "wcs": ["G54"]}}]}"#;
    let w = new_world();
    project::open(&w, old).expect("a v52 setup reads");
    assert_eq!(setup_align::stored(&w, "Setup1"), None);
    let bad = old.replace("\"version\": 52", &format!("\"version\": {}", project::PROJECT_VERSION)).replace(
        "\"wcs\": [\"G54\"]",
        "\"wcs\": \"G54\", \"part_alignment\": {\"translation\": [0.0, 0.0, 0.0], \"rotation\": [2.0, 0.0, 0.0, 0.0]}",
    );
    let err = project::open(&new_world(), &bad).expect_err("a stretched rotation is no rigid transform");
    assert!(err.to_string().contains("rigid"), "{err}");
}

#[test]
fn the_program_sim_carries_the_stock_across_differently_held_setups() {
    let (world, mut h) = document(60.0);
    h.execute(&world, CreateSetup::new(&world, None, Some("Setup1")).expect("Setup2"));
    h.execute(&world, AlignPartToSetup::preset(&world, "Setup2", AlignPreset::FlipX).expect("align 2"));
    let op = CreateOperation::new(&world, "Contour2", TOOL, SrcRef::Entity("part".into()), contour_params())
        .expect("create in Setup2 (the active one)");
    h.execute(&world, op);
    generate(&world, &mut h, "Contour2").expect("generates in Setup2's frame");
    // #317 lifted #326's refusal: the flip is carried (setup_carry.rs
    // pins the body), and the verdict is read in Setup2's frame.
    camcraft_app::stock_sim::simulate_document(&world).expect("the flip is carried");
    let body = camcraft_app::stock_sim::incoming_body(&world, "Setup2").expect("Setup2 starts from Setup1's cut");
    assert!(body.floor_judged(), "the contoured side lies underneath");
}

#[test]
fn the_pocket_and_face_kinds_cut_the_flipped_part_and_post() {
    let (world, mut h) = document(60.0);
    let carried = Carried::of(&contour_params());
    let pocket = (KindInfo::ALL[OpType::Pocket.ordinal()].seed)(&carried);
    let op = CreateOperation::new(&world, "Pocket1", TOOL, SrcRef::Entity("part".into()), pocket);
    h.execute(&world, op.expect("create pocket"));
    let face = (KindInfo::ALL[OpType::Face.ordinal()].seed)(&carried);
    let op = CreateOperation::new(&world, "Face1", TOOL, SrcRef::Stock, face);
    h.execute(&world, op.expect("create face"));
    align(&world, &mut h, AlignPreset::FlipX);
    for name in ["Contour1", "Pocket1", "Face1"] {
        generate(&world, &mut h, name).unwrap_or_else(|e| panic!("{name} generates flipped: {e}"));
    }
    let e = model::find_by_name(&world, "Pocket1").expect("pocket");
    let path = model::toolpath_of(&world, &e).expect("pocket path");
    // Z 12..20 of the flipped part is solid: the pocket clears the stock
    // around it and never enters where the hole stood as modelled.
    let inside = path.path.moves.iter().filter(|m| m.to.z < 19.9).find(|m| m.to.x.abs().max(m.to.y.abs()) < 21.9);
    assert!(inside.is_none(), "{inside:?}");
    assert!(path.path.moves.iter().any(|m| m.to.z < 19.9), "the pocket cuts");
    post(&world, "kinds").expect("the flipped program posts");
}

/// Review of PR #341: the setup sheet tells the operator how the part is
/// clamped in an aligned setup, and says nothing as modelled.
#[test]
fn the_setup_sheet_names_the_part_alignment() {
    let (world, mut h) = document(60.0);
    let plain = camcraft_app::setup_sheet::render(&world, Some("Setup1")).expect("sheet");
    assert!(!plain.contains("PART ALIGNMENT"), "{plain}");
    align(&world, &mut h, AlignPreset::FlipX);
    let sheet = camcraft_app::setup_sheet::render(&world, Some("Setup1")).expect("sheet");
    assert!(sheet.contains("PART ALIGNMENT\n  Flip over X - clamp the part this way up"), "{sheet}");
    align(&world, &mut h, AlignPreset::TurnZ90);
    let sheet = camcraft_app::setup_sheet::render(&world, Some("Setup1")).expect("sheet");
    assert!(sheet.contains("  Turn 90° about Z - clamp"), "{sheet}");
}
