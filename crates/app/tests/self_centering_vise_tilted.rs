//! Built-in self-centering vises in tilted setups (#787, #339 slice 3):
//! the fixture laws judge the placed vise's mesh against the posed stack.
//! A tool leaning 30 deg toward a jaw, its tip low beside the stock,
//! strikes the jaw - where the same tip upright clears it - and the same
//! lean from the vise's centre clears. End to end, a 3+2 face indexed
//! B 10 deg over stock clamped in the vise posts clean, and raising the
//! jaws into the pass blocks the post.
//!
//! Gate map (M4.4): covers app::vise::self_centering, app::fixture, sim::fixture,
//! app::fixture_struck, app::posting, sim::verify.

use camcraft_app::cl::{Move, MoveKind, Toolpath, is_vertical};
use camcraft_app::commands::{AddTool, CreateOperation, GenerateOperation, History, InsertVise, SetStock, SetVise, VisePlacement};
use camcraft_app::fixture;
use camcraft_app::fixture_struck;
use camcraft_app::fixture_view::{self, MeshState};
use camcraft_app::geometry::Point3;
use camcraft_app::model;
use camcraft_app::operation::{Height, OpParams, SrcRef, Tilt};
use camcraft_app::posting;
use camcraft_app::schema::Location;
use camcraft_app::stock::StockShape;
use camcraft_app::tools::catalog;
use camcraft_app::verify::Warning;
use camcraft_app::vise::{self, ViseParams};
use camcraft_app::world::new_world;
use flecs_ecs::prelude::World;

fn p(x: f64, y: f64, z: f64) -> Point3 {
    Point3::new(x, y, z)
}

/// 40 x 30 x 30 stock centred on the origin (x -20..20, y -15..15, z
/// 0..30), a 4 mm end mill (flute 12, shank r3, holder r22), a face on
/// the stock's top `tilt`ed or not, and the 80 mm self-centering vise
/// fitted to the stock: jaws closed to 30 mm (faces at y +-15, 25 mm deep,
/// 15 mm tall), body x -40..40 under the floor.
fn clamped(tilt: Option<Tilt>) -> (World, History, String) {
    let world = new_world();
    let mut history = History::default();
    let stock = SetStock::new(&world, StockShape::Box { dx: 40.0, dy: 30.0, dz: 30.0 }, Location::default());
    history.execute(&world, stock.expect("stock"));
    let tool = catalog().into_iter().find(|(n, _)| *n == "EM-0400-2FL").map(|(_, t)| t).expect("catalog");
    history.execute(&world, AddTool::new(&world, "EM-0400-2FL", tool).expect("tool"));
    let params = OpParams::Face {
        holes: Default::default(),
        tilt,
        blend_turns: false,
        stepover: 3.0,
        clearance_z: Height::fixed(40.0),
        cut_z: Height::fixed(29.5),
        top_z: None,
        stepdown: None,
        feed: 1200.0,
        plunge_feed: 300.0,
        climb: true,
    };
    history.execute(&world, CreateOperation::new(&world, "Face1", "EM-0400-2FL", SrcRef::Stock, params).expect("op"));
    history.execute(&world, GenerateOperation::new(&world, "Face1").expect("generate"));
    let sc = vise::size("80 mm self-centering").map(vise::ViseSize::half_open).expect("80 mm row");
    let cmd = InsertVise::new(&world, &sc, None, VisePlacement::FitStock).expect("fits");
    let name = cmd.name().to_owned();
    history.execute(&world, cmd);
    (world, history, name)
}

/// The post gate's own per-op law (`fixture_struck::op_hits`: box broad
/// phase, mesh-refined, posed stack) for `moves` cut by Face1's tool.
fn hits(world: &World, moves: Vec<Move>) -> Vec<Warning> {
    let e = model::find_by_name(world, "Face1").expect("op");
    let op = model::operation_of(world, &e).expect("operation");
    let (turning, fixed) = fixture::workholding_for(world, op.setup.as_deref());
    let meshes = fixture::meshes_for(world, op.setup.as_deref());
    fixture_struck::op_hits(world, &op, &Toolpath::new(moves), &turning, &fixed, &meshes).expect("judged")
}

/// The tool axis leaning `deg` from vertical toward +y (an A-type lean).
fn lean_y(deg: f64) -> [f64; 3] {
    let (s, c) = deg.to_radians().sin_cos();
    [0.0, s, c]
}

/// A feed past the stock's +x end, tip 2 mm over the floor (13 mm under
/// the jaw tops), `y` off the vise's centre, with `axis` (None: upright).
fn pass(y: f64, axis: Option<[f64; 3]>) -> Vec<Move> {
    let m = Move::new(MoveKind::Feed, p(22.0, y, 2.0), p(30.0, y, 2.0), Some(800.0));
    vec![match axis {
        Some(a) => m.with_axis(a),
        None => m,
    }]
}

#[test]
fn a_tool_leaning_into_a_jaw_strikes_it_where_upright_it_clears() {
    let (world, _history, name) = clamped(None);
    assert_eq!(fixture_view::mesh_states(&world, None), [(name, MeshState::Mesh)], "judged by its mesh, not a box");
    // Tip 10 mm off centre: the 4 mm cutter stands 3 mm clear of the
    // jaw face at y 15. Upright, nothing above the tip reaches the jaw.
    let upright = hits(&world, pass(10.0, None));
    assert!(upright.is_empty(), "upright, the cutter clears the jaw by 3 mm: {upright:?}");
    // Leaning 30 deg toward that jaw, the flute crosses y 15 about 6.5 mm
    // up the axis, at z ~7 - inside the 15 mm tall jaw.
    let tilted = hits(&world, pass(10.0, Some(lean_y(30.0))));
    assert!(
        tilted.iter().any(|w| matches!(w, Warning::FixtureHit { at: 0, .. })),
        "the leaning tool strikes the jaw: {tilted:?}"
    );
    // The same lean the other way runs away from it, toward the far jaw
    // 25 mm off: clear.
    let away = hits(&world, pass(10.0, Some(lean_y(-30.0))));
    assert!(away.is_empty(), "leaning away clears: {away:?}");
}

#[test]
fn the_same_lean_from_the_vise_centre_clears_the_jaws() {
    let (world, _history, _name) = clamped(None);
    // From y 0 the flute and shank reach y 15 only above z ~22, and the
    // holder's low rim stays above z 36: over the 15 mm jaws.
    let tilted = hits(&world, pass(0.0, Some(lean_y(30.0))));
    assert!(tilted.is_empty(), "the leaning stack passes over the jaws: {tilted:?}");
    // The box law alone convicts it (the vise's one box covers the move);
    // the mesh is what clears it.
    let (turning, _) = fixture::workholding_for(&world, None);
    assert!(turning.iter().any(|b| b.min.z < 2.0 && b.max.z > 2.0 && b.min.x < 22.0 && b.max.x > 30.0), "inside the vise's box");
}

fn struck(world: &World) -> bool {
    posting::verification_gate(world).iter().any(|p| p.starts_with("Face1:") && p.contains("strikes a fixture"))
}

#[test]
fn a_tilted_face_over_the_clamped_stock_posts_and_raised_jaws_block_it() {
    let tilt = Tilt { a_deg: 0.0, b_deg: 10.0, origin: [0.0, 0.0, 30.0] };
    let (world, mut history, name) = clamped(Some(tilt));
    let e = model::find_by_name(&world, "Face1").expect("op");
    let path = model::toolpath_of(&world, &e).expect("generated").path;
    assert!(
        !path.moves.is_empty() && path.moves.iter().all(|m| m.axis.is_some_and(|a| !is_vertical(&a))),
        "the 3+2 index poses every move"
    );
    assert!(!struck(&world), "the tilted face runs ~10 mm over the jaws: {:?}", posting::verification_gate(&world));
    // Jaws raised to 28 mm (a taller jaw set): the pass, overhanging the
    // stock's sides by the cutter, now runs into them.
    let tall = model::find_by_name(&world, &name)
        .and_then(|v| fixture::def_of(&world, &v))
        .and_then(|d| d.vise)
        .map(|p| ViseParams { jaw_height: 28.0, ..p })
        .expect("the vise");
    history.execute(&world, SetVise::new(&world, &name, tall).expect("taller jaws"));
    assert!(struck(&world), "the gate names the strike: {:?}", posting::verification_gate(&world));
    assert!(history.undo(&world));
    assert!(!struck(&world), "undo puts the low jaws back");
}
