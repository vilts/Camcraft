//! One region whatever was picked (M11.4 A): a pocket's floor face, its
//! rim edge loop at the top or at the floor, its wall patch - alone or
//! together - resolve to the same enclosed area and cut byte-identical
//! CL, and the strategy the user chose runs on it: a round region under
//! Adaptive rings at the optimal-load step, under Zigzag rows - nothing
//! swaps the generator because of the pick or the shape. On a part the
//! islands come from the part itself, so a face's hole loop and a rim
//! chain that never saw the boss agree too; a Bottom below the picked
//! floor, or a pick that encloses only part material, is refused by
//! name - never cut into the part.
//!
//! Gate map (M11.4 A): covers app::operation (selection, area, stitch,
//! job, generate), motion::machining2d (pocket), sim::verify, motion::load.
//!
//! Lane shape (M13.8): the debug ladder runs every test here in seconds.
//! The island law runs under Zigzag in debug; its `_full` twin carries
//! the same assertions under Adaptive and Zigzag (the HSM plan around the
//! boss, ~12 min debug), and the bore's HSM pick law rides beside it in
//! the heavy lane (`#[ignore]`, `cargo test --release -p camcraft-app
//! --test pick_agnostic -- --ignored`).

mod common;

use camcraft_app::cl::{Move, MoveKind, Toolpath};
use camcraft_app::commands::{AddTool, History, ImportModel, SetStock};
use camcraft_app::geometry::{cube, feature, Mesh, Point3, Triangle};
use camcraft_app::machining2d::PocketStrategy;
use camcraft_app::machining3d::Entry3D;
use camcraft_app::model;
use camcraft_app::operation::{self, FeatureSel, GeoSel, Height, OpParams, SrcRef};
use camcraft_app::schema::{Location, Operation};
use camcraft_app::stock::StockShape;
use camcraft_app::tools::catalog;
use camcraft_app::world::new_world;
use flecs_ecs::prelude::World;

fn pocket(strategy: PocketStrategy, top: f64, bottom: f64) -> OpParams {
    OpParams::Pocket {
        links: camcraft_app::links::Links::PLANAR,
        stepover: 4.0,
        stock_to_leave: 0.0,
        clearance_z: Height::fixed(40.0),
        retract_z: None,
        top_z: Height::fixed(top),
        bottom_z: Height::fixed(bottom),
        stepdown: 5.0,
        feed: 1200.0,
        plunge_feed: 300.0,
        climb: true,
        strategy,
        entry: Entry3D::Plunge,
        helix_radius: 1.0,
        optimal_load: camcraft_app::load::DEFAULT_OPTIMAL_LOAD,
        blend_turns: false,
        tilt: None,
    }
}

fn adaptive2d(top: f64, bottom: f64) -> OpParams {
    OpParams::Adaptive2D {
        links: camcraft_app::links::Links::PLANAR,
        stepover: 4.0,
        stock_to_leave: 0.0,
        clearance_z: Height::fixed(40.0),
        retract_z: None,
        top_z: Height::fixed(top),
        bottom_z: Height::fixed(bottom),
        stepdown: 5.0,
        feed: 1200.0,
        plunge_feed: 300.0,
        climb: true,
        entry: Entry3D::Plunge,
        helix_radius: 1.0,
        optimal_load: camcraft_app::load::DEFAULT_OPTIMAL_LOAD,
        tilt: None,
    }
}

/// A world holding the imported model, a 70 x 70 block stock from `z0`
/// and the 10 mm end mill.
fn world_with(import: ImportModel, z0: f64, dz: f64) -> (World, String) {
    let world = new_world();
    let mut h = History::default();
    let name = import.name.clone();
    h.execute(&world, import);
    let loc = Location { pos: Point3::new(0.0, 0.0, z0), ..Default::default() };
    h.execute(&world, SetStock::new(&world, StockShape::Box { dx: 70.0, dy: 70.0, dz }, loc).expect("stock"));
    let tool = catalog().into_iter().find(|(n, _)| *n == "EM-1000-4FL").map(|(_, t)| t).expect("catalog");
    h.execute(&world, AddTool::new(&world, "EM-1000-4FL", tool).expect("tool"));
    (world, name)
}

/// The user's part: a D-shaped plate (top Z 20) with an R12.5 blind bore
/// to Z 0.
fn user_part() -> (World, String) {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testdata/models/Milling-test.step");
    world_with(ImportModel::new(&path).expect("the user's STEP imports"), -7.0, 27.0)
}

fn generate(world: &World, name: &str, picks: &[FeatureSel], params: &OpParams) -> Result<(Toolpath, usize), String> {
    let op = Operation {
        tool: "EM-1000-4FL".into(),
        src: SrcRef::Selection(picks.iter().map(|f| GeoSel { entity: name.to_owned(), feature: *f }).collect()),
        params: params.clone(),
        stamp: None,
        setup: None,
        area: Default::default(),
        coolant: Default::default(),
        after: Default::default(),
        suppressed: false,
    };
    operation::generate(world, &op, "").map(|(t, w)| (t, w.len())).map_err(|e| e.to_string())
}

/// (max radius about the centroid, mean z, centroid x, centroid y).
fn ring_of(pts: &[[f64; 3]]) -> (f64, f64, f64, f64) {
    let n = pts.len().max(1) as f64;
    let cx = pts.iter().map(|p| p[0]).sum::<f64>() / n;
    let cy = pts.iter().map(|p| p[1]).sum::<f64>() / n;
    let z = pts.iter().map(|p| p[2]).sum::<f64>() / n;
    let r = pts.iter().map(|p| (p[0] - cx).hypot(p[1] - cy)).fold(0.0, f64::max);
    (r, z, cx, cy)
}

/// Every way to pick one ring: the up-facing faces whose outer loop
/// `fits`, the closed chains that do, the patches with a ring that does.
fn picks_of(world: &World, name: &str, fits: impl Fn(f64, f64, f64, f64) -> bool) -> Vec<(String, FeatureSel)> {
    let e = model::find_by_name(world, name).expect("model");
    let f = model::features_of(world, &e).expect("features");
    let vs = |vi: &[u32]| vi.iter().filter_map(|&v| f.verts.get(v as usize).copied()).collect::<Vec<_>>();
    let ok = |pts: &[[f64; 3]]| {
        let (r, z, cx, cy) = ring_of(pts);
        (fits(r, z, cx, cy), z)
    };
    let mut out = Vec::new();
    for face in f.faces.iter().filter(|face| face.normal[2] > 0.99) {
        if let Some(l) = face.loops.iter().find(|l| l.kind == feature::LoopKind::Outer)
            && let (true, z) = ok(&vs(&l.verts))
        {
            out.push((format!("face z {z:.1}"), FeatureSel::Face(face.id)));
        }
    }
    for c in feature::chains(&f).into_iter().filter(|c| c.closed) {
        if let (true, z) = ok(&feature::chain_points(&f, &c)) {
            out.push((format!("chain z {z:.1}"), FeatureSel::Chain(c.id)));
        }
    }
    for p in feature::patches(&f) {
        let Some(rings) = feature::patch_rings(&f, p.id) else { continue };
        if rings.iter().filter(|r| r.closed).any(|r| ok(&vs(&r.verts)).0) {
            out.push(("wall patch".to_owned(), FeatureSel::Patch(p.id)));
        }
    }
    out
}

/// Single picks plus every pair: the same region however spelled.
fn spellings(picks: &[(String, FeatureSel)]) -> Vec<(String, Vec<FeatureSel>)> {
    let mut v: Vec<(String, Vec<FeatureSel>)> = picks.iter().map(|(l, s)| (l.clone(), vec![*s])).collect();
    for i in 0..picks.len() {
        for j in (i + 1)..picks.len() {
            v.push((format!("{} + {}", picks[i].0, picks[j].0), vec![picks[i].1, picks[j].1]));
        }
    }
    v
}

fn bore_picks(world: &World, name: &str) -> Vec<(String, FeatureSel)> {
    picks_of(world, name, |r, _, cx, cy| (r - 12.5).abs() < 0.01 && cx.hypot(cy) < 0.01)
}

/// Assert every spelling cuts the first spelling's bytes, silent.
fn all_identical(world: &World, name: &str, picks: &[(String, FeatureSel)], params: &OpParams) {
    identical_spellings(world, name, &spellings(picks), params);
}

fn identical_spellings(world: &World, name: &str, spelled: &[(String, Vec<FeatureSel>)], params: &OpParams) {
    let mut first: Option<(String, String)> = None;
    for (label, sel) in spelled {
        let (label, sel) = (label.clone(), sel.clone());
        let (t, warns) = generate(world, name, &sel, params).unwrap_or_else(|e| panic!("{label}: {e}"));
        assert_eq!(warns, 0, "{label} must verify silent (depth-aware): {:?}", t.notes);
        assert!(t.is_chained() && t.all_finite() && !t.moves.is_empty());
        let cl = common::gen_to_cltext(&t);
        match &first {
            None => first = Some((label, cl)),
            Some((l0, c0)) => assert!(*c0 == cl, "{label} cut different bytes than {l0} for {params:?}"),
        }
    }
}

#[test]
fn every_pick_of_the_bore_cuts_the_same_bytes_under_every_strategy() {
    let (world, name) = user_part();
    let picks = bore_picks(&world, &name);
    let labels: Vec<&str> = picks.iter().map(|(l, _)| l.as_str()).collect();
    // The floor face, the rim at the top and at the floor, the wall.
    assert_eq!(picks.len(), 4, "{labels:?}");
    assert!(labels.contains(&"wall patch"), "{labels:?}");
    assert_eq!(labels.iter().filter(|l| l.starts_with("chain")).count(), 2, "{labels:?}");
    for params in [
        pocket(PocketStrategy::Zigzag, 20.0, 0.0),
        pocket(PocketStrategy::Waterline, 20.0, 0.0),
        pocket(PocketStrategy::Auto, 20.0, 0.0),
        pocket(PocketStrategy::OffsetRings, 20.0, 0.0),
    ] {
        all_identical(&world, &name, &picks, &params);
    }
}

/// The same pick law under the HSM plan (M11.4 C): Adaptive and the 2D
/// Adaptive kind plan by measuring every cut against the stock - eight
/// plans of the user's bore, minutes in debug, so this half of the law
/// rides the heavy lane.
#[test]
#[ignore = "heavy lane: HSM adaptive generation"]
fn every_pick_of_the_bore_cuts_the_same_bytes_under_hsm_adaptive() {
    let (world, name) = user_part();
    let picks = bore_picks(&world, &name);
    assert_eq!(picks.len(), 4);
    for params in [pocket(PocketStrategy::Adaptive, 20.0, 0.0), adaptive2d(20.0, 0.0)] {
        all_identical(&world, &name, &picks, &params);
    }
}

fn level_feeds(t: &Toolpath, z: f64) -> Vec<Move> {
    t.moves.iter().filter(|m| m.kind == MoveKind::Feed && m.from.z == z && m.to.z == z).cloned().collect()
}

#[test]
fn the_chosen_strategy_runs_on_the_round_region_never_a_substitute() {
    let (world, name) = user_part();
    let picks = bore_picks(&world, &name);
    let floor = picks.iter().find(|(l, _)| l.starts_with("face")).map(|(_, s)| *s).expect("floor face");
    let cut = |p: &OpParams| generate(&world, &name, &[floor], p).expect("generates").0;
    // Offset rings (the pre-M11.4 C adaptive, kept by name): rings
    // stepped by the optimal load, 14.64% of D = 1.464 mm.
    let o = cut(&pocket(PocketStrategy::OffsetRings, 20.0, 0.0));
    let mut radii: Vec<f64> =
        level_feeds(&o, 15.0).iter().map(|m| (m.to.x.hypot(m.to.y) * 100.0).round() / 100.0).collect();
    radii.dedup();
    assert!(radii.len() >= 4, "{radii:?}");
    let step = camcraft_app::load::DEFAULT_OPTIMAL_LOAD * 10.0;
    for w in radii.windows(2) {
        assert!((w[1] - w[0] - step).abs() < 0.02, "ring spacing {radii:?} must be ae = {step}");
    }
    // Adaptive (M11.4 C) is the HSM plan on the very same round floor:
    // a spiral whose cut points sweep the radii continuously - never a
    // handful of concentric ring radii (the user's "just because it's a
    // round hole doesn't mean it needs to be circular paths").
    // (A helix entry: the HSM spiral opens from its bore; out of a
    // tool-size plunge it cannot hold the load and says so by falling
    // back to the rings.)
    let mut hp = pocket(PocketStrategy::Adaptive, 20.0, 0.0);
    if let OpParams::Pocket { entry, helix_radius, .. } = &mut hp {
        *entry = Entry3D::Helix;
        *helix_radius = 2.0;
    }
    // The depth-aware verifier is silent on it (M11.4 C round 2: the
    // HSM spiral never crosses itself, never leaves the floor's area).
    let (a, warns) = generate(&world, &name, &[floor], &hp).expect("generates");
    assert_eq!(warns, 0, "the user's bore under HSM verifies clean: {:?}", a.notes);
    assert!(
        !a.notes.iter().any(|n| n.contains("Offset rings cut this pocket instead")),
        "the HSM plan runs on the user's bore: {:?}",
        a.notes
    );
    let mut ar: Vec<f64> =
        level_feeds(&a, 15.0).iter().map(|m| (m.to.x.hypot(m.to.y) * 100.0).round() / 100.0).collect();
    ar.sort_by(f64::total_cmp);
    ar.dedup();
    assert!(ar.len() > 4 * radii.len(), "a spiral, not rings: {} radii vs the rings' {}", ar.len(), radii.len());
    assert_ne!(common::gen_to_cltext(&a), common::gen_to_cltext(&o));
    // Zigzag on the same round floor stays rows: straight, no ring arcs.
    let z = cut(&pocket(PocketStrategy::Zigzag, 20.0, 0.0));
    let zf = level_feeds(&z, 15.0);
    assert!(zf.iter().all(|m| m.arc.is_none()), "zigzag must not become rings on a round region");
    assert!(
        zf.iter().any(|m| (m.to.y - m.from.y).abs() < 1e-9 && (m.to.x - m.from.x).abs() > 1.0),
        "rows run along X"
    );
    assert_ne!(common::gen_to_cltext(&a), common::gen_to_cltext(&z));
}

#[test]
fn a_bottom_below_the_picked_floor_is_refused_by_name_for_every_pick() {
    let (world, name) = user_part();
    for (label, sel) in bore_picks(&world, &name) {
        let err = generate(&world, &name, &[sel], &pocket(PocketStrategy::Adaptive, 20.0, -5.0))
            .err()
            .unwrap_or_else(|| panic!("{label}: a pocket below the bore's floor must refuse"));
        assert!(err.contains("Bottom Z sits below the selected pocket's floor"), "{label}: {err}");
    }
}

/// A 50 x 50 x [0, 20] block with a 36 x 36 blind pocket to Z 5 and a
/// 6 x 6 boss standing in it to Z 12. Outward-wound.
fn block_with_boss() -> Mesh {
    const H: f64 = 18.0;
    const B: f64 = 3.0;
    let flip = |t: &Triangle| Triangle::new(t.a, t.c, t.b);
    let quad = |a: Point3, b: Point3, c: Point3, d: Point3, f: &mut Vec<Triangle>| {
        f.push(Triangle::new(a, b, c));
        f.push(Triangle::new(a, c, d));
    };
    // A horizontal square annulus between half-widths `o` and `i` at
    // `z`, facing +z.
    let ring = |o: f64, i: f64, z: f64, f: &mut Vec<Triangle>| {
        let oo = [(-o, -o), (o, -o), (o, o), (-o, o)];
        let ii = [(-i, -i), (i, -i), (i, i), (-i, i)];
        for k in 0..4 {
            let n = (k + 1) % 4;
            let p = |(x, y): (f64, f64)| Point3::new(x, y, z);
            quad(p(oo[k]), p(oo[n]), p(ii[n]), p(ii[k]), f);
        }
    };
    let mut f = Vec::new();
    let outer = cube(Point3::new(-25.0, -25.0, 0.0), Point3::new(25.0, 25.0, 20.0));
    f.extend(outer.facets[0..2].iter().cloned()); // bottom
    f.extend(outer.facets[4..12].iter().cloned()); // block walls
    ring(25.0, H, 20.0, &mut f); // top
    let hole = cube(Point3::new(-H, -H, 5.0), Point3::new(H, H, 20.0));
    f.extend(hole.facets[4..12].iter().map(flip)); // pocket walls face the axis
    ring(H, B, 5.0, &mut f); // floor around the boss
    let boss = cube(Point3::new(-B, -B, 5.0), Point3::new(B, B, 12.0));
    f.extend(boss.facets[2..12].iter().cloned()); // boss top + walls
    Mesh::from_facets(f)
}

/// The island law (M11.4 A) under each of `strategies`: the floor face,
/// the rim runs at the mouth and at the floor, and face + rim cut the
/// same bytes, and no feed below the boss top enters its tool-grown
/// footprint.
fn island_law(strategies: &[PocketStrategy]) {
    let (world, name) = world_with(ImportModel::from_mesh("part", block_with_boss()), 0.0, 20.0);
    // The pocket's floor face (half-width 18: circumradius 25.46) - its
    // loops carry the boss as a hole.
    let picks = picks_of(&world, &name, |r, _, cx, cy| (r - 648f64.sqrt()).abs() < 0.01 && cx.hypot(cy) < 0.01);
    let face = picks.iter().find(|(l, _)| l.starts_with("face")).map(|(_, s)| *s).expect("floor face");
    // A square rim is four open runs (90 degree corners end a chain);
    // picked together they stitch into the ring - at the mouth (Z 20)
    // and at the floor (Z 5). Neither spelling knows about the boss.
    let rim = |z: f64| -> Vec<FeatureSel> {
        let e = model::find_by_name(&world, &name).expect("model");
        let f = model::features_of(&world, &e).expect("features");
        feature::chains(&f)
            .into_iter()
            .filter(|c| {
                let p = feature::chain_points(&f, c);
                !p.is_empty()
                    && p.iter().all(|q| {
                        (q[2] - z).abs() < 1e-9
                            && ((q[0].abs() - 18.0).abs() < 1e-9 || (q[1].abs() - 18.0).abs() < 1e-9)
                            && q[0].abs() <= 18.0 + 1e-9
                            && q[1].abs() <= 18.0 + 1e-9
                    })
            })
            .map(|c| FeatureSel::Chain(c.id))
            .collect()
    };
    let (top, floor) = (rim(20.0), rim(5.0));
    assert!(!top.is_empty() && !floor.is_empty(), "rim runs at both planes");
    let mut with_face = top.clone();
    with_face.push(face);
    let spelled = vec![
        ("floor face".to_owned(), vec![face]),
        ("mouth rim runs".to_owned(), top),
        ("floor rim runs".to_owned(), floor),
        ("face + mouth rim".to_owned(), with_face),
    ];
    for &strategy in strategies {
        let params = pocket(strategy, 20.0, 5.0);
        identical_spellings(&world, &name, &spelled, &params);
        // No feed with its center over the boss's tool-grown footprint
        // below the boss top (r = 5 tool around a 3 mm half-width boss).
        let (t, _) = generate(&world, &name, &[face], &params).expect("generates");
        // Distance from a center to the boss square; the tool edge must
        // stand off it by the radius (the arc fitter's 0.01 band aside).
        let off = |x: f64, y: f64| (x.abs() - 3.0).max(0.0).hypot((y.abs() - 3.0).max(0.0));
        for m in t.moves.iter().filter(|m| m.kind == MoveKind::Feed && m.to.z < 12.0) {
            assert!(
                off(m.to.x, m.to.y) >= 5.0 - 0.011,
                "{strategy:?}: feed into the boss at {:?}",
                m.to
            );
        }
    }
}

/// Debug lane: the island law under Zigzag - every spelling's bytes and
/// the boss standoff, in seconds.
#[test]
fn a_face_with_an_island_and_its_rim_loop_cut_the_same_bytes_around_the_boss() {
    island_law(&[PocketStrategy::Zigzag]);
}

/// Heavy lane: the island law under the HSM Adaptive plan too (five plans
/// around the boss, each measured against the stock) - the original
/// Adaptive + Zigzag pair.
#[test]
#[ignore = "heavy lane: HSM adaptive around the boss island, five plans, ~12 min debug"]
fn a_face_with_an_island_and_its_rim_loop_cut_the_same_bytes_around_the_boss_full() {
    island_law(&[PocketStrategy::Adaptive, PocketStrategy::Zigzag]);
}

#[test]
fn a_pick_enclosing_only_part_material_is_refused_not_pocketed() {
    // The boss's top face encloses nothing but the boss: there is no
    // pocket there, and saying so beats the old answer (the boss cut
    // down, a gouge only the verifier stood between and the spindle).
    let (world, name) = world_with(ImportModel::from_mesh("part", block_with_boss()), 0.0, 20.0);
    let top = picks_of(&world, &name, |r, z, _, _| (r - 18f64.sqrt()).abs() < 0.01 && (z - 12.0).abs() < 1e-9);
    let face = top.iter().find(|(l, _)| l.starts_with("face")).map(|(_, s)| *s).expect("boss top face");
    let Err(err) = generate(&world, &name, &[face], &pocket(PocketStrategy::Adaptive, 12.0, 5.0)) else {
        panic!("a pick of solid part material must refuse");
    };
    assert!(err.contains("the part fills the whole selection"), "{err}");
}
