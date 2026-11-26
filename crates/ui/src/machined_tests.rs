//! M13.1 owed: the machined stock's mesh reaches the viewer. Nothing
//! before a field view; the whole program's view extracts once and is
//! read back every frame as the scene's stock part; the transport up
//! reads nothing; a stock hover is re-asked of the mesh - machined-away
//! air reports nothing, remaining material reports the stock.
//! Provenance: our own fixtures over the M7.1 field engine; nothing
//! from vendor binaries.
//!
//! #25: the mesh paints its deviation classes (judged once, with the
//! extraction) and the hovered mesh facet projects for the highlight.
//!
//! Gate map (M4.4): covers ui::machined, ui::egui_app::world (collect_parts), app::stock_export::MeshCache, app::pick::soup_hit, app::machined_classes.

use std::sync::Arc;

use camcraft_app::commands::{AddTool, CreateOperation, GenerateOperation, ImportModel, SetStock};
use camcraft_app::geometry::{cube, Point3, IDENTITY_Q};
use camcraft_app::operation::{Height, OpParams, SrcRef};
use camcraft_app::schema::Location;
use camcraft_app::stock::StockShape;
use camcraft_app::stock_view::field_view_with;
use camcraft_app::tools::catalog;

use crate::egui_app::world::collect;
use crate::machined::{filter_hit, filter_hit_facet, hover_facet_uv, mesh, prime};
use crate::state::{Sim, State};

const HALF: f64 = 6.0;
/// The coarse display budget the app's stock_export suite walks at.
const COARSE: f64 = (2.0 * HALF) * (2.0 * HALF) * 20.0 / 3.6;

/// A 12x12x20 box faced to z 18 over a 12x12x18 model, in a `State`.
fn faced_state() -> State {
    let mut state = State::new();
    let loc = Location { pos: Point3::ZERO, rot: IDENTITY_Q };
    let stock = SetStock::new(&state.world, StockShape::Box { dx: 2.0 * HALF, dy: 2.0 * HALF, dz: 20.0 }, loc)
        .expect("box stock");
    state.history.execute(&state.world, stock);
    let tool = catalog().into_iter().find(|(n, _)| *n == "EM-0400-2FL").map(|(_, t)| t).expect("catalog entry");
    let add = AddTool::new(&state.world, "EM-0400-2FL", tool).expect("catalog tool");
    state.history.execute(&state.world, add);
    let part = cube(Point3::new(-HALF, -HALF, 0.0), Point3::new(HALF, HALF, 18.0));
    state.history.execute(&state.world, ImportModel::from_mesh("part", part));
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
        blend_turns: false, blend_radius: None,
        picks: Default::default(),
    };
    let create = CreateOperation::new(&state.world, "Face1", "EM-0400-2FL", SrcRef::Stock, params).expect("create face");
    state.history.execute(&state.world, create);
    let generate = GenerateOperation::new(&state.world, "Face1").expect("generate face");
    state.history.execute(&state.world, generate);
    state
}

/// The whole program's view, as `actions::walk_whole_program` would
/// hold it - at the coarse budget, so a debug run takes seconds.
fn view_the_program(state: &mut State) {
    let fv = field_view_with(&state.world, COARSE).expect("field view");
    state.field_view = Some(Ok(Arc::new(fv)));
    state.field_mode = Some((false, true, 0));
}

#[test]
fn nothing_before_a_view_and_one_extraction_after() {
    let mut state = faced_state();
    assert!(mesh(&state).is_none(), "no field view: no mesh");
    prime(&mut state);
    assert!(mesh(&state).is_none(), "priming without a view extracts nothing");

    view_the_program(&mut state);
    assert!(mesh(&state).is_none(), "a view alone is not read until primed - never an extraction per frame");
    prime(&mut state);
    let a = mesh(&state).expect("the whole program's mesh");
    assert!(!a.is_empty(), "a non-empty shell");
    prime(&mut state);
    let b = mesh(&state).expect("still there");
    assert!(Arc::ptr_eq(&a, &b), "the same view is never re-extracted");

    // The scene's stock slot carries exactly that mesh, nothing else of stock.
    let scene = collect(&state);
    let stock: Vec<_> = scene.parts.iter().filter(|p| p.stock).collect();
    assert_eq!(stock.len(), 1, "one stock part");
    assert_eq!(stock[0].verts.len(), a.len() * 3, "the mesh's own facets, three vertices each");
    assert_eq!(stock[0].classes.len(), stock[0].verts.len(), "judged: one class per vertex (#25)");
    let classes = state.stock_mesh.viewed_classes(&match &state.field_view {
        Some(Ok(fv)) => Arc::clone(fv),
        _ => panic!("the view stands"),
    });
    let classes = classes.expect("judged with the extraction");
    assert!(Arc::ptr_eq(&classes, &{
        prime(&mut state);
        match &state.field_view {
            Some(Ok(fv)) => state.stock_mesh.viewed_classes(fv).expect("still judged"),
            _ => panic!("the view stands"),
        }
    }), "a second prime never re-judges");
    assert!(classes.iter().all(|&c| [0.0, 1.0, 2.0, 3.0].contains(&c)), "the four-class vocabulary only");
    // The faced top sits on the model's top: the up-facing facets at z 18
    // read clean by the eye's gate, never rest or unjudged.
    let top: Vec<f32> = a
        .iter()
        .zip(classes.iter())
        .filter(|(t, _)| t.normal().z > 0.9 && t.a.x.abs() < HALF - 2.0 && t.a.y.abs() < HALF - 2.0)
        .map(|(_, &c)| c)
        .collect();
    assert!(!top.is_empty(), "the faced top has facets");
    assert!(top.iter().all(|&c| c == 0.0), "the faced top is clean: {top:?}");

    // The transport up reads nothing here (a playback view changes every frame).
    state.sim = Some(Sim::new());
    assert!(mesh(&state).is_none(), "playback: the sheet stands");
    let scene = collect(&state);
    let box_part = scene.parts.iter().find(|p| p.stock).expect("the box stands in");
    assert_eq!(box_part.verts.len(), 36, "the uncut box's twelve facets");
}

#[test]
fn a_stock_hover_is_asked_of_the_machined_mesh() {
    let mut state = faced_state();
    let stock = ("Stock".to_string(), 0usize, [0.0, 0.0, 20.0]);
    // Before a mesh: the entity pick's answer passes through untouched.
    let down = ([0.0, 0.0, 40.0], [0.0, 0.0, -1.0]);
    let got = filter_hit(&state, down.0, down.1, Some(stock.clone()));
    assert_eq!(got.as_ref().map(|h| h.1), Some(0), "no mesh: the box facet stands");

    view_the_program(&mut state);
    prime(&mut state);
    assert!(mesh(&state).is_some(), "{}", state.status);
    // Straight down onto the faced top: the stock, at the machined height.
    let (name, facet, p) = filter_hit(&state, down.0, down.1, Some(stock.clone())).expect("material under the ray");
    assert_eq!(name, "Stock");
    assert_eq!(facet, usize::MAX, "the mesh is no entity: no box facet to light");
    assert!((p[2] - 18.0).abs() < 2.0, "the hit sits on the faced top, not the box top: z {}", p[2]);
    // Sideways through the skimmed-away slab (z 19.6, a cell above the
    // faced top): the box is crossed, the material is not.
    let side = ([-40.0, 0.0, 19.6], [1.0, 0.0, 0.0]);
    let boxed = Some(("Stock".to_string(), 0usize, [-HALF, 0.0, 19.6]));
    assert!(filter_hit(&state, side.0, side.1, boxed).is_none(), "machined-away air reports nothing");
    // A model hit is kept: models win at any depth.
    let model = Some(("part".to_string(), 3usize, [0.0, 0.0, 18.0]));
    assert_eq!(filter_hit(&state, down.0, down.1, model.clone()), model);
}

#[test]
fn the_hovered_mesh_facet_is_the_one_the_ray_hit_and_projects() {
    let mut state = faced_state();
    let down = ([0.5, 0.5, 40.0], [0.0, 0.0, -1.0]);
    let stock = Some(("Stock".to_string(), 0usize, [0.5, 0.5, 20.0]));
    let (_, none) = filter_hit_facet(&state, down.0, down.1, stock.clone());
    assert!(none.is_none(), "no mesh: no mesh facet");
    state.hover_mesh = Some(camcraft_app::geometry::Triangle::new(Point3::ZERO, Point3::ZERO, Point3::ZERO));
    assert!(hover_facet_uv(&state, 1.5).is_none(), "no mesh standing: no highlight");

    view_the_program(&mut state);
    prime(&mut state);
    let (hit, facet) = filter_hit_facet(&state, down.0, down.1, stock);
    let (_, _, p) = hit.expect("material under the ray");
    let t = facet.expect("the mesh facet the ray landed on");
    let (lo_x, hi_x) = (t.a.x.min(t.b.x).min(t.c.x), t.a.x.max(t.b.x).max(t.c.x));
    let (lo_y, hi_y) = (t.a.y.min(t.b.y).min(t.c.y), t.a.y.max(t.b.y).max(t.c.y));
    assert!(lo_x <= p[0] && p[0] <= hi_x && lo_y <= p[1] && p[1] <= hi_y, "the facet holds the hit {p:?}: {t:?}");
    // A model hit carries no mesh facet: models win, nothing of the mesh lights.
    let model = Some(("part".to_string(), 3usize, [0.0, 0.0, 18.0]));
    assert!(filter_hit_facet(&state, down.0, down.1, model).1.is_none());

    // The highlight projects through the camera, inside the image when framed.
    state.hover_mesh = Some(t);
    state.cam.target = [0.0, 0.0, 18.0];
    let uv = hover_facet_uv(&state, 1.5).expect("a framed facet projects");
    for c in uv {
        assert!(c.iter().all(|v| v.is_finite() && (-0.5..=1.5).contains(v)), "uv {uv:?}");
    }
}
