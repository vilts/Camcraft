//! #529 (follow-up to #303): a cached edge-break record is credited only
//! within its own operation's stated break width. A project file whose
//! record was hand-inflated no longer forgives a cut below the op's real
//! break - the record is dropped and the columns are judged sharp; a
//! record on a kind that declares no break is not credited at all.
//!
//! Gate map (#529): covers app::stock_sim, sim::stock_sim (breaks::bounded,
//! breaks::declared_width), app::project (the breaks key, read back).
//! Provenance: our own fixtures (#303's brick and chamfer).

use camcraft_app::cl::{BreakFrame, BreakProfile, EdgeBreak, EdgeBreakVolume, Move, MoveKind};
use camcraft_app::commands::{AddTool, CreateOperation, GenerateOperation, History, ImportModel, SetStock};
use camcraft_app::geometry::{cube, Point3};
use camcraft_app::operation::{self, Height, HeightReference, OpParams, OpType, SrcRef};
use camcraft_app::stock::StockShape;
use camcraft_app::stock_sim::breaks;
use camcraft_app::tools::catalog;
use camcraft_app::world::new_world;
use camcraft_app::{model, posting, project};
use flecs_ecs::prelude::World;

const CONE: &str = "CM-0600-90";

fn seeded(ty: OpType) -> OpParams {
    let base = OpParams::Face {
        holes: Default::default(),
        tilt: None,
        stepover: 3.0,
        clearance_z: Height::of(HeightReference::TopOfStock, 25.0),
        cut_z: Height::of(HeightReference::TopOfStock, 0.0),
        top_z: None,
        stepdown: None,
        feed: 1200.0,
        plunge_feed: 300.0,
        climb: true,
        blend_turns: false,
    };
    operation::retype_params(&base, ty.ordinal())
}

/// #303's finished brick: the stock is the 40 x 20 x 10 part, a default
/// 2D chamfer on its top loop.
fn chamfered_brick() -> World {
    let world = new_world();
    let mut h = History::default();
    h.execute(&world, ImportModel::from_mesh("brick", cube(Point3::new(-20.0, -10.0, 0.0), Point3::new(20.0, 10.0, 10.0))));
    let stock = SetStock::new(&world, StockShape::FromModel { source: "brick".into() }, Default::default());
    h.execute(&world, stock.expect("stock"));
    let tool = catalog().into_iter().find(|(n, _)| *n == CONE).map(|(_, t)| t).expect("cone");
    h.execute(&world, AddTool::new(&world, CONE, tool).expect("tool"));
    let op = CreateOperation::new(&world, "OP", CONE, SrcRef::Entity("brick".into()), seeded(OpType::Chamfer2D));
    h.execute(&world, op.expect("create"));
    h.execute(&world, GenerateOperation::new(&world, "OP").expect("generate"));
    world
}

/// Lower every cached move by `dz` (a ride deeper than asked).
fn sink(world: &World, dz: f64) {
    let e = model::find_by_name(world, "OP").expect("op");
    let mut t = model::toolpath_of(world, &e).expect("cached");
    for m in &mut t.path.moves {
        m.from.z -= dz;
        m.to.z -= dz;
    }
    e.entity_view(world).set(camcraft_app::schema::Toolpath::new(t.path, t.warnings));
}

fn gouge(world: &World) -> Option<String> {
    posting::verification_gate(world).into_iter().find(|l| l.starts_with("gouge:"))
}

/// Save, scale every cached break's depth by `k` in the file text (a
/// hand edit), and load it back.
fn edited(world: &World, k: f64) -> World {
    let path = edited_path(k);
    project::save(world, &path).expect("save");
    let mut v: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&path).expect("read")).expect("json");
    fn scale(v: &mut serde_json::Value, k: f64, hits: &mut usize) {
        match v {
            serde_json::Value::Object(m) => {
                if let Some(serde_json::Value::Array(bs)) = m.get_mut("breaks") {
                    for b in bs {
                        if let Some(d) = b.get("depth").and_then(serde_json::Value::as_f64) {
                            b["depth"] = (d * k).into();
                            *hits += 1;
                        }
                    }
                }
                for x in m.values_mut() {
                    scale(x, k, hits);
                }
            }
            serde_json::Value::Array(a) => a.iter_mut().for_each(|x| scale(x, k, hits)),
            _ => {}
        }
    }
    let mut hits = 0;
    scale(&mut v, k, &mut hits);
    assert!(hits > 0, "the file carries the chamfer's break");
    std::fs::write(&path, v.to_string()).expect("write");
    let loaded = new_world();
    project::load(&loaded, &path).expect("load");
    let _ = std::fs::remove_file(&path);
    loaded
}

#[test]
fn parallel_project_edits_use_distinct_paths() {
    let paths: Vec<_> = std::thread::scope(|scope| {
        let workers: Vec<_> = (0..8).map(|_| scope.spawn(|| {
            (0..16).map(|_| edited_path(1.0)).collect::<Vec<_>>()
        })).collect();
        workers.into_iter().flat_map(|worker| worker.join().expect("path worker")).collect()
    });
    let unique: std::collections::HashSet<_> = paths.iter().collect();
    assert_eq!(unique.len(), paths.len(), "parallel helper calls must never share a project file");
}

fn edited_path(k: f64) -> std::path::PathBuf {
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let id = NEXT.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!("camcraft-529-{}-{k}-{id}.ccp", std::process::id()))
}

#[test]
fn an_inflated_record_no_longer_forgives_a_deeper_cut() {
    let world = chamfered_brick();
    assert!(gouge(&world).is_none(), "the honest chamfer posts");
    sink(&world, 0.3);
    assert!(gouge(&world).is_some(), "0.3 mm under its stated break is a gouge");
    // The hand edit: a break five times as deep would cover the 0.3 mm.
    let loaded = edited(&world, 5.0);
    let line = gouge(&loaded).expect("the inflated record forgave a real gouge");
    assert!(line.starts_with("gouge:"), "{line}");
}

#[test]
fn an_unedited_file_still_credits_its_own_break() {
    let world = chamfered_brick();
    let loaded = edited(&world, 1.0);
    assert!(gouge(&loaded).is_none(), "the op's own break still posts after a round trip");
}

#[test]
fn the_bound_reads_the_ops_own_width() {
    let chamfer = seeded(OpType::Chamfer2D);
    let w = breaks::declared_width(&chamfer).expect("a chamfer declares a width");
    let edge = vec![Point3::new(0.0, 0.0, 10.0), Point3::new(10.0, 0.0, 10.0)];
    let fits = EdgeBreak { edge: edge.clone(), depth: w, slope: 1.0, volume: None };
    let wide = EdgeBreak { edge: edge.clone(), depth: 3.0 * w, slope: 1.0, volume: None };
    let kept = breaks::bounded("OP", &chamfer, vec![fits.clone(), wide]);
    assert_eq!(kept, vec![fits.clone()], "only the record within the stated width");
    let deburr = seeded(OpType::Deburr);
    assert!(breaks::declared_width(&deburr).is_some(), "a deburr declares a width");
    assert!(breaks::bounded("F", &seeded(OpType::Face), vec![fits]).is_empty(), "a face declares no break");
}

#[test]
fn a_framed_cap_verifies_after_loading_but_a_deeper_or_forged_cut_convicts() {
    let world = chamfered_brick();
    let e = model::find_by_name(&world, "OP").expect("op");
    let mut cached = model::toolpath_of(&world, &e).expect("cache");
    let width = breaks::declared_width(&seeded(OpType::Chamfer2D)).expect("width");
    let depth = width / 4.0;
    cached.path.moves = vec![Move {
        rotary: None, kind: MoveKind::Feed,
        from: Point3::new(-2.0, -10.0, 10.0 - depth),
        to: Point3::new(2.0, -10.0, 10.0 - depth),
        feed: Some(500.0), arc: None, axis: None,
    }];
    cached.path.breaks = vec![EdgeBreak {
        edge: vec![Point3::new(-3.0, -10.0, 10.0), Point3::new(3.0, -10.0, 10.0)],
        depth, slope: 0.0,
        volume: Some(EdgeBreakVolume {
            width, profile: BreakProfile::Flat,
            frames: vec![BreakFrame { outward: [0.0, 0.0, 1.0], across: [0.0, 1.0, 0.0] }],
        }),
    }];
    e.entity_view(&world).set(camcraft_app::schema::Toolpath::new(cached.path, cached.warnings));
    assert!(gouge(&world).is_none(), "an honest cap verifies");
    let loaded = edited(&world, 1.0);
    assert!(gouge(&loaded).is_none(), "the cap still verifies after loading");
    sink(&world, depth);
    assert!(gouge(&world).is_some(), "a cut below the cap is a real gouge");
    let loaded = edited(&world, 10.0);
    assert!(gouge(&loaded).is_some(), "an inflated cached depth cannot forgive the gouge");
    let mut cached = model::toolpath_of(&world, &e).expect("cache");
    cached.path.breaks[0].depth = width * 2.0;
    cached.path.breaks[0].volume.as_mut().expect("volume").width = width * 4.0;
    assert!(cached.path.breaks[0].is_sane(), "the forged record is geometrically sane but exceeds its operation");
    e.entity_view(&world).set(camcraft_app::schema::Toolpath::new(cached.path, cached.warnings));
    let loaded = edited(&world, 1.0);
    assert!(gouge(&loaded).is_some(), "inflating width and depth together cannot evade the operation bound");
}
