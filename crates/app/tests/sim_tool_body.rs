//! #743 part 1: the simulated tool is drawn as a revolved body from the
//! very outline the collision laws read - the judged tool record's
//! cutter and shank, and the band stack the fixture laws sweep above
//! them. The mesh's meridian is exactly that outline, it follows a
//! tilted tool axis, a degenerate outline draws nothing (never a panic),
//! and the cached mesh is dropped with its tool, its operation or a
//! change of outline. Our own tests.
//!
//! Gate map (M4.4): covers app::sim_tool_body, app::tool_body, app::tool_body_view.

use std::sync::Arc;

use camcraft_app::commands::{AddTool, CreateOperation, DeleteOperation, DeleteTool, History, SetStock, SetToolBody, ToolBodySpec};
use camcraft_app::geometry::{Point3, Triangle};
use camcraft_app::model;
use camcraft_app::operation::{Height, OpParams, SrcRef};
use camcraft_app::schema::Location;
use camcraft_app::sim_tool_body::{self as sim_body, JudgedOutline, OutlineError, SimToolCache};
use camcraft_app::spun::SpunPoint;
use camcraft_app::stock::StockShape;
use camcraft_app::tool_body;
use camcraft_app::tools::{catalog, Tool};
use camcraft_app::world::new_world;
use camcraft_tools::holder_profile::{HolderBand, HolderStack};
use flecs_ecs::prelude::World;

const TOOL: &str = "EM-0400-2FL";
const OP: &str = "Face1";

fn catalog_tool(name: &str) -> Tool {
    catalog().into_iter().find(|(n, _)| *n == name).map(|(_, t)| t).expect("catalog entry")
}

fn shank_top() -> f64 {
    let t = catalog_tool(TOOL);
    t.flute_len + t.shank_len
}

/// A closed `n`-gon prism of circumradius `r` about model Z over `[z0, z1]`.
fn prism(r: f64, z0: f64, z1: f64, n: usize) -> Vec<Triangle> {
    let q = |k: usize, z: f64| {
        let a = std::f64::consts::TAU * k as f64 / n as f64;
        Point3::new(r * a.cos(), r * a.sin(), z)
    };
    let mut v = Vec::new();
    for k in 0..n {
        let (a0, a1, b0, b1) = (q(k, z0), q(k + 1, z0), q(k, z1), q(k + 1, z1));
        v.push(Triangle::new(a0, a1, b1));
        v.push(Triangle::new(a0, b1, b0));
        v.push(Triangle::new(Point3::new(0.0, 0.0, z0), a1, a0));
        v.push(Triangle::new(Point3::new(0.0, 0.0, z1), b0, b1));
    }
    v
}

fn face_world() -> (World, History) {
    let world = new_world();
    let mut history = History::default();
    history.execute(
        &world,
        SetStock::new(&world, StockShape::Box { dx: 60.0, dy: 60.0, dz: 20.0 }, Location::default()).expect("stock"),
    );
    history.execute(&world, AddTool::new(&world, TOOL, catalog_tool(TOOL)).expect("tool"));
    let face = OpParams::Face {
        holes: Default::default(),
        tilt: None,
        blend_turns: false,
        stepover: 3.0,
        clearance_z: Height::fixed(25.0),
        cut_z: Height::fixed(19.5),
        top_z: None,
        stepdown: None,
        feed: 1200.0,
        plunge_feed: 300.0,
        climb: true,
    };
    history.execute(&world, CreateOperation::new(&world, OP, TOOL, SrcRef::Stock, face).expect("op"));
    (world, history)
}

fn tool_entity(world: &World) -> flecs_ecs::prelude::Entity {
    model::find_by_name(world, TOOL).expect("tool")
}

/// The judged record and stack, read the way `fixture_struck` and
/// `operation::generate` read them.
fn judged_stack(world: &World) -> (Tool, HolderStack) {
    let (judged, body) = tool_body::judged(world, &tool_entity(world), catalog_tool(TOOL)).expect("fits");
    let stack = tool_body::stack(&judged.geom(), body.as_ref());
    (judged, stack)
}

/// Every vertex of a tool-frame mesh as `(z, r)`.
fn meridian_of(tris: &[Triangle]) -> Vec<(f64, f64)> {
    tris.iter().flat_map(|t| [t.a, t.b, t.c]).map(|p| (p.z, p.x.hypot(p.y))).collect()
}

fn has(points: &[(f64, f64)], z: f64, r: f64) -> bool {
    points.iter().any(|q| (q.0 - z).abs() < 1e-9 && (q.1 - r).abs() < 1e-9)
}

/// The mesh's meridian is exactly the outline: every mesh vertex sits on
/// an outline vertex, and every outline vertex is in the mesh.
fn assert_mesh_is_outline(outline: &JudgedOutline, tris: &[Triangle]) {
    let mut want = outline.cutter.clone();
    want.extend(outline.holder_meridian());
    let got = meridian_of(tris);
    assert!(!got.is_empty(), "a body is drawn");
    for &(z, r) in &got {
        assert!(has(&want, z, r), "mesh vertex (z {z}, r {r}) is not on the judged outline {want:?}");
    }
    for &(z, r) in &want {
        assert!(has(&got, z, r), "outline vertex (z {z}, r {r}) is missing from the mesh");
    }
}

#[test]
fn the_bare_tool_is_spun_from_the_cylinder_the_laws_sweep() {
    let (world, _history) = face_world();
    let t = catalog_tool(TOOL);
    let outline = sim_body::for_operation(&world, OP).expect("no refusal").expect("a mill draws");
    // The holder is the one cylinder `HolderStack::cylinder` hands the laws.
    assert_eq!(outline.holder, HolderStack::cylinder(&t.geom()).bands);
    assert_eq!(outline.holder, vec![HolderBand { above: shank_top(), len: t.holder_len, r: t.holder_d * 0.5 }]);
    // The cutter and shank are the record's own profile up to the shank top.
    assert!(has(&outline.cutter, 0.0, t.diameter * 0.5), "the cutter's rim at the tip");
    assert!(has(&outline.cutter, t.flute_len, t.shank_d * 0.5), "the shank band starts at the flute top");
    assert!(has(&outline.cutter, shank_top(), 0.0), "closed to the axis at the shank top");
    let tris = outline.mesh();
    assert_mesh_is_outline(&outline, &tris);
    let m = meridian_of(&tris);
    let top = m.iter().fold(0.0f64, |a, q| a.max(q.0));
    let wide = m.iter().fold(0.0f64, |a, q| a.max(q.1));
    assert_eq!(top, shank_top() + t.holder_len, "tip to the holder's top");
    assert!((wide - t.holder_d * 0.5).abs() < 1e-9, "the holder's radius is the widest");
    assert_eq!(outline.top(), top);
    assert!((outline.max_radius() - wide).abs() < 1e-9);
}

#[test]
fn a_modelled_body_is_drawn_as_its_judged_envelope_not_its_facets() {
    let (world, mut history) = face_world();
    let z0 = shank_top();
    // A hexagonal collar: its facets reach r = 20 only at six corners.
    let spec = ToolBodySpec { source: "hex.stl".into(), facets: prism(20.0, z0, z0 + 30.0, 6), axis: [0.0, 0.0, 1.0], offset: 0.0 };
    history.execute(&world, SetToolBody::new(&world, TOOL, Some(spec)).expect("body"));
    let (judged, stack) = judged_stack(&world);
    let outline = sim_body::for_operation(&world, OP).expect("no refusal").expect("draws");
    assert_eq!(outline.holder, stack.bands, "the drawn holder is the stack the fixture laws sweep");
    assert_eq!(outline, JudgedOutline::new(&judged, &stack).expect("sane"));
    let tris = outline.mesh();
    assert_mesh_is_outline(&outline, &tris);
    // Revolved: the body is at its envelope radius all the way round,
    // not only at the hexagon's corners (its flats sit at r = 17.3).
    let rim: Vec<f64> = meridian_of(&tris).into_iter().filter(|q| (q.0 - (z0 + 30.0)).abs() < 1e-9 && q.1 > 1.0).map(|q| q.1).collect();
    assert!(rim.len() >= 32, "a full ring at the body's top: {}", rim.len());
    let band_r = stack.bands.last().expect("a band").r;
    assert!(rim.iter().all(|r| (r - band_r).abs() < 1e-9), "every rim vertex at the judged radius {band_r}");
    assert!(band_r >= 20.0, "never smaller than the solid");
}

#[test]
fn a_drawn_cone_is_drawn_as_the_stepped_bands_the_laws_judge() {
    let (world, mut history) = face_world();
    let z0 = shank_top();
    // A cone from r = 10 to r = 30 over 20 mm: judged as one band at 30.
    let cone = vec![SpunPoint::line(z0, 0.0), SpunPoint::line(z0, 10.0), SpunPoint::line(z0 + 20.0, 30.0), SpunPoint::line(z0 + 20.0, 0.0)];
    history.execute(&world, SetToolBody::spun(&world, TOOL, Some(cone)).expect("a sane profile"));
    let (_, stack) = judged_stack(&world);
    assert_eq!(stack.bands, vec![HolderBand { above: z0, len: 20.0, r: 30.0 }], "the law's reading of the cone");
    let outline = sim_body::for_operation(&world, OP).expect("no refusal").expect("draws");
    assert_eq!(outline.holder, stack.bands);
    let tris = outline.mesh();
    assert_mesh_is_outline(&outline, &tris);
    assert!(has(&meridian_of(&tris), z0, 30.0), "the foot is drawn at the judged 30, not the drawn 10");
}

#[test]
fn the_body_follows_a_tilted_tool_axis() {
    let (world, _history) = face_world();
    let mut cache = SimToolCache::default();
    let frame = cache.tool_frame(&world, OP).expect("a mesh");
    let want = meridian_of(&frame);
    let tip = [10.0, -20.0, 30.0];
    let n = (0.3f64 * 0.3 + 0.2 * 0.2 + 0.9 * 0.9).sqrt();
    for axis in [[0.3 / n, -0.2 / n, 0.9 / n], [1.0, 0.0, 0.0], [0.0, 0.0, -1.0], [0.0, 0.0, 1.0]] {
        let posed = cache.posed(&world, OP, tip, axis).expect("posed");
        assert_eq!(posed.len(), frame.len());
        // Each vertex keeps its height along the axis and its distance from it.
        let got: Vec<(f64, f64)> = posed
            .iter()
            .flat_map(|t| [t.a, t.b, t.c])
            .map(|p| {
                let d = [p.x - tip[0], p.y - tip[1], p.z - tip[2]];
                let h = d[0] * axis[0] + d[1] * axis[1] + d[2] * axis[2];
                let off = [d[0] - h * axis[0], d[1] - h * axis[1], d[2] - h * axis[2]];
                (h, (off[0] * off[0] + off[1] * off[1] + off[2] * off[2]).sqrt())
            })
            .collect();
        for (g, w) in got.iter().zip(&want) {
            assert!((g.0 - w.0).abs() < 1e-9 && (g.1 - w.1).abs() < 1e-9, "axis {axis:?}: {g:?} vs {w:?}");
        }
    }
    // A prismatic frame is the tool frame moved to the tip.
    let up = cache.posed(&world, OP, tip, [0.0, 0.0, 1.0]).expect("posed");
    assert!((up[0].a.x - (frame[0].a.x + tip[0])).abs() < 1e-12 && (up[0].a.z - (frame[0].a.z + tip[2])).abs() < 1e-12);
}

#[test]
fn a_degenerate_outline_draws_nothing_and_names_itself() {
    let t = catalog_tool(TOOL);
    let stack = HolderStack::cylinder(&t.geom());
    assert!(JudgedOutline::new(&t, &stack).is_ok(), "the sane tool draws");
    let band = |above: f64, len: f64, r: f64| HolderStack { bands: vec![HolderBand { above, len, r }], top: above + len, neck: Vec::new() };
    assert_eq!(JudgedOutline::new(&Tool { diameter: f64::INFINITY, ..t.clone() }, &stack), Err(OutlineError::NonFinite));
    assert_eq!(JudgedOutline::new(&t, &band(50.0, f64::NAN, 5.0)), Err(OutlineError::NonFinite));
    assert_eq!(JudgedOutline::new(&t, &band(50.0, 10.0, -5.0)), Err(OutlineError::Negative));
    assert_eq!(JudgedOutline::new(&t, &band(50.0, -10.0, 5.0)), Err(OutlineError::Negative));
    // Bands that overlap along the axis: the outline would cross itself.
    let crossing = HolderStack {
        bands: vec![HolderBand { above: 50.0, len: 20.0, r: 10.0 }, HolderBand { above: 60.0, len: 20.0, r: 15.0 }],
        top: 80.0,
        neck: Vec::new(),
    };
    assert_eq!(JudgedOutline::new(&t, &crossing), Err(OutlineError::RunsBack));
    // No radius anywhere, and no length anywhere.
    let none = HolderStack { bands: Vec::new(), top: 0.0, neck: Vec::new() };
    let thin = Tool { diameter: 0.0, shank_d: 0.0, ..t.clone() };
    assert_eq!(JudgedOutline::new(&thin, &none), Err(OutlineError::NoRadius));
    let flat = Tool { flute_len: 0.0, shank_len: 0.0, ..t.clone() };
    assert_eq!(JudgedOutline::new(&flat, &none), Err(OutlineError::NoRadius));
    assert_eq!(JudgedOutline::new(&t, &band(50.0, 10.0, 501.0)), Err(OutlineError::TooLarge));
    assert_eq!(JudgedOutline::new(&t, &band(1995.0, 10.0, 5.0)), Err(OutlineError::TooLarge));
    // A half-typed draft (NaN numbers) never panics: it draws short or not at all.
    let draft = Tool { diameter: f64::NAN, flute_len: f64::NAN, ..t.clone() };
    let _ = JudgedOutline::new(&draft, &stack).map(|o| o.mesh());
    // An empty holder band is no holder, not an error.
    let bare = JudgedOutline::new(&t, &band(50.0, 0.0, 22.0)).expect("the cutter still draws");
    assert!(bare.holder.is_empty() && bare.holder_meridian().is_empty());
    // Every catalog tool either draws a sane mesh or is a lathe kind.
    for (name, tool) in catalog() {
        let o = JudgedOutline::new(&tool, &HolderStack::cylinder(&tool.geom())).unwrap_or_else(|e| panic!("{name}: {e}"));
        assert!(o.mesh().iter().all(|q| [q.a, q.b, q.c].iter().all(|p| p.x.is_finite() && p.y.is_finite() && p.z.is_finite())), "{name}");
    }
}

#[test]
fn a_body_that_no_longer_fits_draws_nothing_and_says_why() {
    let (world, _history) = face_world();
    let mut cache = SimToolCache::default();
    assert!(cache.tool_frame(&world, OP).is_some());
    // Past the commands' own guard: a body standing over the flutes.
    let low = tool_body::build("low.stl", [0.0, 0.0, 1.0], 0.0, prism(20.0, 1.0, 30.0, 12)).expect("an envelope");
    tool_body::set_body(&world, &tool_entity(&world), Some(&low));
    assert!(sim_body::for_operation(&world, OP).is_err());
    assert!(cache.tool_frame(&world, OP).is_none(), "nothing is drawn");
    assert!(cache.is_empty(), "and the stale mesh is dropped");
    assert!(cache.refusal().is_some_and(|w| w.contains(TOOL) && w.contains("flutes")), "{:?}", cache.refusal());
    assert!(cache.posed(&world, OP, [0.0; 3], [0.0, 0.0, 1.0]).is_none());
    tool_body::set_body(&world, &tool_entity(&world), None);
    assert!(cache.tool_frame(&world, OP).is_some(), "drawn again once it fits");
    assert!(cache.refusal().is_none());
}

#[test]
fn the_cached_mesh_is_reused_rebuilt_on_a_new_outline_and_freed_with_its_tool() {
    let (world, mut history) = face_world();
    let mut cache = SimToolCache::default();
    assert!(cache.is_empty());
    let first = cache.tool_frame(&world, OP).expect("a mesh");
    let again = cache.tool_frame(&world, OP).expect("a mesh");
    assert!(Arc::ptr_eq(&first, &again), "an unchanged outline reuses the mesh");
    // A new body changes the judged outline: the old mesh is let go.
    let z0 = shank_top();
    let spec = ToolBodySpec { source: "c.stl".into(), facets: prism(20.0, z0, z0 + 30.0, 16), axis: [0.0, 0.0, 1.0], offset: 0.0 };
    history.execute(&world, SetToolBody::new(&world, TOOL, Some(spec)).expect("body"));
    drop(again);
    let weak = Arc::downgrade(&first);
    drop(first);
    let bodied = cache.tool_frame(&world, OP).expect("a mesh");
    assert!(weak.upgrade().is_none(), "the replaced mesh is freed");
    // Undo restores the old outline; the cache follows.
    history.undo(&world);
    let back = cache.tool_frame(&world, OP).expect("a mesh");
    assert!(!Arc::ptr_eq(&bodied, &back));
    let weak = Arc::downgrade(&back);
    drop((bodied, back));
    // Deleting the operation leaves nothing to draw and nothing held.
    history.execute(&world, DeleteOperation::new(&world, OP).expect("delete op"));
    assert!(cache.tool_frame(&world, OP).is_none());
    assert!(cache.is_empty() && weak.upgrade().is_none(), "the op's mesh is freed");
    history.undo(&world);
    let held = Arc::downgrade(&cache.tool_frame(&world, OP).expect("drawn again"));
    // Deleting the tool does the same.
    history.execute(&world, DeleteTool::new(&world, TOOL).expect("delete tool"));
    assert!(cache.tool_frame(&world, OP).is_none());
    assert!(cache.is_empty() && held.upgrade().is_none(), "the tool's mesh is freed");
    assert!(cache.refusal().is_none(), "a missing tool is not a refusal");
    cache.clear();
    assert!(cache.is_empty());
}
