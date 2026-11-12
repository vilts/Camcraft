//! #835: a tool body is judged as ONE shape - the band stack the fixture
//! laws sweep and the simulation draws - by the reach laws too, not the
//! single cylinder enclosing it. Pinned here: the stack is never smaller
//! than the body (an imported mesh, a drawn profile with cones and arcs),
//! a stepped body whose nose clears a wall is no longer warned while a
//! band that reaches the wall still is, a tool with no body reads
//! unchanged, the drawn and judged stacks agree, and the laws that keep
//! the enclosing cylinder say so by name.
//!
//! Gate map (M4.4): covers sim::verify::reach, sim::verify::probe, sim::verify::body_bands, app::operation::body_judged, app::operation::generate, app::sim_tool_body, app::tool_body.

use camcraft_app::cl::{Move, MoveKind, Toolpath};
use camcraft_app::commands::{AddTool, CreateOperation, History, SetStock, SetToolBody};
use camcraft_app::geo2d::{Loop, Point2, Region};
use camcraft_app::geometry::Point3;
use camcraft_app::machining2d::GenError;
use camcraft_app::model;
use camcraft_app::operation::body_judged;
use camcraft_app::operation::{Height, OpParams, SrcRef};
use camcraft_app::schema::Location;
use camcraft_app::sim_tool_body;
use camcraft_app::spun::{SpunPoint, SpunProfile};
use camcraft_app::stock::StockShape;
use camcraft_app::tool_body;
use camcraft_app::tools::{catalog, Tool};
use camcraft_app::verify::{verify, BodyBands, Cutter, Mode, ToolGeom, VerifyContext, Warning};
use camcraft_app::world::new_world;
use camcraft_tools::holder_profile::{HolderBand, HolderProfile, HolderStack};

const TOOL: &str = "EM-0400-2FL";

fn catalog_tool() -> Tool {
    catalog().into_iter().find(|(n, _)| *n == TOOL).map(|(_, t)| t).expect("catalog entry")
}

// ---- the stack is never smaller than the body ----------------------

/// The farthest any band covering height `z` reaches (touching ends
/// count) - what the laws read at `z`.
fn reach_at(bands: &[HolderBand], z: f64) -> f64 {
    bands.iter().filter(|b| b.above - 1e-9 <= z && z <= b.above + b.len + 1e-9).fold(f64::NEG_INFINITY, |a, b| a.max(b.r))
}

/// A faceted solid of revolution of the meridian `m` ((z, r) points),
/// `n` sides, turned by `turn` radians and lifted `lift` - a stand-in
/// for an imported holder mesh whose vertices sit on no band edge.
fn revolve(m: &[(f64, f64)], n: usize, turn: f64, lift: f64) -> Vec<[[f64; 3]; 3]> {
    let at = |(z, r): (f64, f64), k: usize| {
        let a = turn + std::f64::consts::TAU * k as f64 / n as f64;
        [r * a.cos(), r * a.sin(), z + lift]
    };
    let mut out = Vec::new();
    for w in m.windows(2) {
        for k in 0..n {
            let (a, b, c, d) = (at(w[0], k), at(w[0], k + 1), at(w[1], k), at(w[1], k + 1));
            out.push([a, b, d]);
            out.push([a, d, c]);
        }
    }
    out
}

/// Points spread over the facet (barycentric grid, edges included).
fn samples(t: &[[f64; 3]; 3]) -> Vec<[f64; 3]> {
    let n = 24;
    let mut out = Vec::new();
    for i in 0..=n {
        for j in 0..=(n - i) {
            let (u, v) = (i as f64 / n as f64, j as f64 / n as f64);
            let w = 1.0 - u - v;
            out.push([0, 1, 2].map(|k| u * t[0][k] + v * t[1][k] + w * t[2][k]));
        }
    }
    out
}

#[test]
fn the_band_stack_is_never_smaller_than_an_imported_mesh_body() {
    // A nose, a cone up to a flange, the flange, a taper back in.
    let m = [(0.0, 0.0), (0.0, 7.5), (11.3, 7.5), (19.7, 16.2), (31.1, 16.2), (37.0, 9.0), (37.0, 0.0)];
    let tris = revolve(&m, 23, 0.37, 18.13);
    let profile = HolderProfile::envelope(&tris).expect("a sane body");
    let t = catalog_tool();
    let judged = profile.enclose(&t).expect("the body sits above the flutes");
    let g = judged.geom();
    let stack = tool_body::stack(&g, Some(&profile));
    let carried = BodyBands::of(&g, &stack).expect("the stack fits the tool it was read for");
    let shank_top = g.flute_len + g.shank_len;
    for tri in &tris {
        for p in samples(tri) {
            let (z, r) = (p[2], p[0].hypot(p[1]));
            assert!(reach_at(profile.bands(), z) >= r - 1e-9, "envelope at z {z}: {} < body r {r}", reach_at(profile.bands(), z));
            assert!(reach_at(&stack.bands, z) >= r - 1e-9, "swept stack at z {z} is smaller than the body r {r}");
            // What the reach laws read: the shank beside it, the carried bands above.
            let law = if z <= shank_top { g.shank_radius.max(reach_at(carried.bands(), z)) } else { reach_at(carried.bands(), z) };
            assert!(law >= r - 1e-9, "the reach laws' stack at z {z} reads {law} < body r {r}");
        }
    }
}

fn pt(z: f64, r: f64) -> SpunPoint {
    SpunPoint::line(z, r)
}

/// A drawn holder with a nose, a cone, a convex and a concave arc.
fn drawn(z0: f64) -> Vec<SpunPoint> {
    vec![
        pt(z0, 0.0),
        pt(z0, 6.0),
        pt(z0 + 10.0, 6.0),
        SpunPoint { z: z0 + 20.0, r: 14.0, arc: 8.0 },
        SpunPoint { z: z0 + 30.0, r: 14.0, arc: -6.0 },
        pt(z0 + 40.0, 14.0),
        pt(z0 + 40.0, 0.0),
    ]
}

/// Points on the drawn outline itself: lines exactly, arcs on their own
/// circle (the minor arc through both ends, centre on the side the
/// signed radius names).
fn on_outline(pts: &[SpunPoint]) -> Vec<(f64, f64)> {
    let mut out = Vec::new();
    for w in pts.windows(2) {
        let (a, b) = (w[0], w[1]);
        let (dz, dr) = (b.z - a.z, b.r - a.r);
        let len = dz.hypot(dr);
        for k in 0..=200 {
            let s = k as f64 / 200.0;
            if a.arc == 0.0 {
                out.push((a.z + dz * s, a.r + dr * s));
                continue;
            }
            let rad = a.arc.abs();
            let n = (-dr / len, dz / len);
            let back = (rad * rad - len * len * 0.25).max(0.0).sqrt();
            let mid = ((a.z + b.z) * 0.5, (a.r + b.r) * 0.5);
            let c = (mid.0 - a.arc.signum() * back * n.0, mid.1 - a.arc.signum() * back * n.1);
            let (u, v) = ((a.z - c.0, a.r - c.1), (b.z - c.0, b.r - c.1));
            let q = (u.0 + (v.0 - u.0) * s, u.1 + (v.1 - u.1) * s);
            let l = q.0.hypot(q.1);
            out.push((c.0 + rad * q.0 / l, c.1 + rad * q.1 / l));
        }
    }
    out
}

#[test]
fn the_band_stack_is_never_smaller_than_a_drawn_profile_with_cones_and_arcs() {
    let t = catalog_tool();
    let z0 = t.flute_len + t.shank_len;
    let pts = drawn(z0);
    let spun = SpunProfile::new(pts.clone()).expect("a sane drawn profile");
    let profile = spun.holder_profile();
    let judged = profile.enclose(&t).expect("on the shank's top");
    let g = judged.geom();
    let stack = tool_body::stack(&g, Some(&profile));
    let carried = BodyBands::of(&g, &stack).expect("the stack fits");
    let outline = on_outline(&pts);
    // The convex arc bulges past r 14: the test must see it.
    assert!(outline.iter().any(|p| p.1 > 15.0), "the convex arc's bulge is sampled");
    for (z, r) in outline {
        assert!(reach_at(&stack.bands, z) >= r - 1e-9, "swept stack at z {z} < drawn r {r}");
        assert!(reach_at(carried.bands(), z) >= r - 1e-9, "the reach laws' stack at z {z} < drawn r {r}");
    }
    // ...and it is tighter than the enclosing cylinder: the nose band is the nose.
    assert!(reach_at(carried.bands(), z0 + 5.0) < g.holder_radius - 5.0, "{:?}", carried.bands());
}

// ---- law (c): the stepped body ------------------------------------

/// Stepped body on a Ø8 end mill: an r 6 nose 14..34 above the tip, an
/// r 20 flange 34..49. Its enclosing cylinder is r 20 from 14 to 49.
fn stepped() -> (ToolGeom, HolderStack) {
    let g = ToolGeom { radius: 4.0, nose_radius: 0.0, flute_len: 8.0, shank_radius: 4.0, shank_len: 6.0, holder_radius: 20.0, holder_len: 35.0 };
    let stack = HolderStack {
        bands: vec![HolderBand { above: 14.0, len: 20.0, r: 6.0 }, HolderBand { above: 34.0, len: 15.0, r: 20.0 }],
        top: 49.0,
        neck: Vec::new(),
    };
    (g, stack)
}

fn rect(x0: f64, y0: f64, x1: f64, y1: f64) -> Loop {
    Loop::new(vec![Point2::new(x0, y0), Point2::new(x1, y0), Point2::new(x1, y1), Point2::new(x0, y1)]).expect("loop")
}

/// A 100 mm plate with a 20 mm part island, its wall `gap` from the
/// tool's axis; the tool plunges to `tip_z` beside it.
fn plunge_beside_island(gap: f64, tip_z: f64, g: &ToolGeom, cutter: Option<Cutter>) -> Vec<Warning> {
    let region = Region::new(vec![rect(0.0, 0.0, 100.0, 100.0), rect(40.0, 40.0, 60.0, 60.0)]).expect("region");
    let stock = Region::new(vec![rect(0.0, 0.0, 100.0, 100.0)]).expect("stock");
    let x = 40.0 - gap;
    let t = Toolpath::new(vec![
        Move::new(MoveKind::Rapid, Point3::new(-20.0, -20.0, 5.0), Point3::new(x, 50.0, 5.0), None),
        Move::new(MoveKind::Plunge, Point3::new(x, 50.0, 5.0), Point3::new(x, 50.0, tip_z), Some(80.0)),
    ]);
    let cx = VerifyContext {
        region: &region,
        stock: Some(&stock),
        model: None,
        tool: g,
        cutter,
        mode: Mode::Boundary,
        stock_top_z: 0.0,
        clearance_z: 5.0,
        stock_to_leave: 0.0,
        gouge_tol: 0.01,
        z_tol: 1e-6,
        finish_tol: None,
        fixtures: &[],
        spindle: None,
        ..Default::default()
    };
    verify(&t, &cx).into_iter().filter(|w| matches!(w, Warning::ShankHit { .. } | Warning::SweepHit { .. })).collect()
}

#[test]
fn a_stepped_nose_clear_of_the_wall_is_no_longer_warned() {
    let (g, stack) = stepped();
    let body = Cutter::flat(&g).with_body(&g, &stack);
    assert!(body.body.is_some(), "the stepped stack fits its enclosing cylinder");
    // Wall 10 mm off the axis, tip 30 deep: the nose (r 6) stands clear,
    // the flange (r 20) stands 4 mm ABOVE the stock top. The enclosing
    // cylinder (r 20 from 16 below the top) read a false hit.
    let before = plunge_beside_island(10.0, -30.0, &g, None);
    assert_eq!(before, vec![Warning::ShankHit { at: 1, part: true }], "the enclosing cylinder's reading");
    let after = plunge_beside_island(10.0, -30.0, &g, Some(body));
    assert_eq!(after, Vec::new(), "the drawn nose is clear and the flange is above the wall");
}

#[test]
fn a_band_that_reaches_the_wall_is_still_warned() {
    let (g, stack) = stepped();
    let body = Cutter::flat(&g).with_body(&g, &stack);
    // The flange sinks 6 mm below the top beside a wall 10 mm off.
    assert_eq!(plunge_beside_island(10.0, -40.0, &g, Some(body)), vec![Warning::ShankHit { at: 1, part: true }]);
    // The nose itself grazes a wall 5 mm off.
    assert_eq!(plunge_beside_island(5.0, -30.0, &g, Some(body)), vec![Warning::ShankHit { at: 1, part: true }]);
    // Both bands in the wall still report the move once.
    assert_eq!(plunge_beside_island(5.0, -40.0, &g, Some(body)), vec![Warning::ShankHit { at: 1, part: true }]);
}

#[test]
fn a_tool_with_no_body_reads_unchanged() {
    let (g, _) = stepped();
    let plain = Cutter::flat(&g);
    // The plain holder cylinder is not carried: the cutter is the same value.
    assert_eq!(plain.with_body(&g, &HolderStack::cylinder(&g)), plain);
    assert_eq!(plain.holder_bands(), vec![(14.0, 20.0, 35.0)]);
    for (gap, z) in [(10.0, -30.0), (10.0, -40.0), (5.0, -30.0), (30.0, -10.0)] {
        assert_eq!(plunge_beside_island(gap, z, &g, None), plunge_beside_island(gap, z, &g, Some(plain)), "gap {gap}, tip {z}");
    }
}

#[test]
fn a_stack_that_is_not_this_tools_is_refused_and_the_cylinder_stands() {
    let (g, stack) = stepped();
    let wider = HolderStack { bands: vec![HolderBand { above: 14.0, len: 35.0, r: 25.0 }], top: 49.0, neck: Vec::new() };
    let shorter = HolderStack { bands: vec![stack.bands[0]], top: 34.0, neck: Vec::new() };
    let in_flutes = HolderStack { bands: vec![HolderBand { above: 2.0, len: 47.0, r: 3.0 }], top: 49.0, neck: Vec::new() };
    let nan = HolderStack { bands: vec![HolderBand { above: 14.0, len: f64::NAN, r: 6.0 }], top: 49.0, neck: Vec::new() };
    for s in [wider, shorter, in_flutes, nan] {
        assert_eq!(BodyBands::of(&g, &s), None, "{s:?}");
        assert_eq!(Cutter::flat(&g).with_body(&g, &s).holder_bands(), vec![(14.0, 20.0, 35.0)]);
    }
    // A cutter whose bands disagree with the geometry the laws read
    // (the record's holder changed under it) judges by the cylinder.
    let body = Cutter::flat(&g).with_body(&g, &stack);
    let shrunk = ToolGeom { holder_radius: 10.0, ..g };
    let shrunk_body = Cutter { holder_radius: 10.0, ..body };
    assert_eq!(Cutter::judged(&shrunk, Some(shrunk_body)).body, None);
    assert_eq!(Cutter::judged(&g, Some(body)).body, body.body);
}

// ---- drawn shape == judged shape ----------------------------------

#[test]
fn the_simulation_draws_the_stack_the_reach_laws_judge() {
    let world = new_world();
    let mut history = History::default();
    history.execute(&world, SetStock::new(&world, StockShape::Box { dx: 60.0, dy: 60.0, dz: 20.0 }, Location::default()).expect("stock"));
    history.execute(&world, AddTool::new(&world, TOOL, catalog_tool()).expect("tool"));
    history.execute(
        &world,
        CreateOperation::new(
            &world,
            "Face1",
            TOOL,
            SrcRef::Stock,
            OpParams::Face {
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
            },
        )
        .expect("face op"),
    );
    let t = catalog_tool();
    history.execute(&world, SetToolBody::spun(&world, TOOL, Some(drawn(t.flute_len + t.shank_len))).expect("a sane profile"));
    let drawn_outline = sim_tool_body::for_operation(&world, "Face1").expect("drawable").expect("a milling tool");
    // The judged cutter, read the way `operation::generate` builds it.
    let te = model::find_by_name(&world, TOOL).expect("tool");
    let (judged, body) = tool_body::judged(&world, &te, t).expect("fits");
    let g = judged.geom();
    let stack = tool_body::stack(&g, body.as_ref());
    let cutter = Cutter::of(&judged).with_body(&g, &stack);
    assert!(cutter.body.is_some(), "a drawn body is judged by its bands");
    let judged_bands: Vec<(f64, f64, f64)> = cutter.holder_bands();
    let drawn_bands: Vec<(f64, f64, f64)> = drawn_outline.holder.iter().map(|b| (b.above, b.r, b.len)).collect();
    assert_eq!(judged_bands, drawn_bands, "the reach, solid and rotary laws judge what the simulation draws");
    // The fixture laws sweep the same bands.
    let fixture: Vec<(f64, f64, f64)> = stack.bands.iter().map(|b| (b.above, b.r, b.len)).collect();
    assert_eq!(judged_bands, fixture);
}

// ---- kept cylinder, said by name ----------------------------------

#[test]
fn a_law_that_keeps_the_cylinder_names_it() {
    let t = catalog_tool();
    let z0 = t.flute_len + t.shank_len;
    let profile = SpunProfile::new(drawn(z0)).expect("sane").holder_profile();
    let judged = profile.enclose(&t).expect("fits");
    let msg = format!("tool {TOOL}'s holder (dia. {:.3}) sinks into the stock", judged.holder_d);
    let refusal = || GenError::Invalid(msg.clone());
    let named = body_judged::name_enclosure(refusal(), &judged, Some(&profile)).to_string();
    let words = format!("judged here by the cylinder enclosing it, Ø{:.3} mm", judged.holder_d);
    assert!(named.contains(&words), "{named}");
    // #871: a ghost-carrying refusal is named alike, its ghost kept.
    let built = camcraft_app::cl::Toolpath::new(vec![camcraft_app::cl::Move::new(
        camcraft_app::cl::MoveKind::Feed,
        camcraft_app::geometry::Point3::new(0.0, 0.0, 0.0),
        camcraft_app::geometry::Point3::new(5.0, 0.0, 0.0),
        Some(500.0),
    )]);
    let ghosted = body_judged::name_enclosure(refusal().with_ghost(&built), &judged, Some(&profile));
    assert!(ghosted.ghost().is_some(), "the ghost rides the rewrite");
    assert_eq!(ghosted.to_string(), named, "the same words as the plain refusal");
    // No body, or a refusal about something else: unchanged.
    assert_eq!(body_judged::name_enclosure(refusal(), &t, None).to_string(), refusal().to_string());
    let other = || GenError::Invalid("the hole is deeper than the flute".into());
    assert_eq!(body_judged::name_enclosure(other(), &judged, Some(&profile)).to_string(), other().to_string());
    // The touch probe's stylus reach folds stem and holder: its note says so.
    let note = body_judged::probe_note(&judged, Some(&profile)).expect("a body is noted");
    assert!(note.contains(&words), "{note}");
    assert_eq!(body_judged::probe_note(&t, None), None);
}
