//! #1869: sketch marks preserve analytic lines and circles through generation,
//! project storage, verification and posting.
//!
//! Gate map (M4.4): covers app::operation (job, generate, mark_law),
//! app::sketch_model, app::project, app::stock_sim, app::posting, app::mcp.
//! Provenance: original fixtures using public DXF group codes and planar geometry.

mod common;
#[path = "sketch_engrave_trace/avoid.rs"]
mod avoid;
#[path = "sketch_engrave_trace/mcp.rs"]
mod mcp;
#[path = "sketch_engrave_trace/precision.rs"]
mod precision;
#[path = "sketch_engrave_trace/safety.rs"]
mod safety;

use std::f64::consts::{FRAC_PI_2, PI, TAU};

use flecs_ecs::prelude::World;

use camcraft_app::cl::{ArcPlane, Move, MoveKind};
use camcraft_app::commands::{AddTool, CreateOperation, DxfOptions, GenerateOperation, History, ImportDxf, MoveSketch, SetStock};
use camcraft_app::geometry::Point3;
use camcraft_app::model;
use camcraft_app::operation::{self, FeatureSel, GeoSel, Height, HeightReference, OpParams, OpStatus, OpType, SrcRef};
use camcraft_app::project;
use camcraft_app::schema::{Location, Toolpath};
use camcraft_app::stock::StockShape;
use camcraft_app::tools::catalog;
use camcraft_app::world::new_world;
use camcraft_app::{posting, sketch, sketch_model, stock_sim};

const PROFILE: &str = "mark";
const OP: &str = "Mark";
const TOOL: &str = "EM-0400-2FL";
const TOP: f64 = 6.0;
const CLEARANCE: f64 = 18.0;
const EPS: f64 = 1e-6;
const KINDS: [OpType; 2] = [OpType::Engrave, OpType::Trace];

// The open stroke's quarter-circle is authored CCW from (14,5) to (10,9).
// From the line's free end (2,9), the stroke must take that arc clockwise.
// The closed stroke is a semicircle and a diameter. A separate full circle
// and a one-degree arc exercise circle splitting and short signed sweeps.
const DXF: &str = "0\nSECTION\n2\nHEADER\n9\n$INSUNITS\n70\n4\n0\nENDSEC\n0\nSECTION\n2\nENTITIES\n\
    0\nARC\n8\nopen\n10\n10\n20\n5\n40\n4\n50\n0\n51\n90\n\
    0\nLINE\n8\nopen\n10\n2\n20\n9\n11\n10\n21\n9\n\
    0\nARC\n8\nclosed\n10\n27\n20\n7\n40\n4\n50\n270\n51\n90\n\
    0\nLINE\n8\nclosed\n10\n27\n20\n3\n11\n27\n21\n11\n\
    0\nCIRCLE\n8\nclosed\n10\n41\n20\n7\n40\n3\n\
    0\nARC\n8\nshort\n10\n11\n20\n22\n40\n8\n50\n15\n51\n16\n\
    0\nLINE\n8\nexcluded\n10\n29\n20\n26\n11\n36\n21\n26\n\
    0\nENDSEC\n0\nEOF\n";

fn job(stock_height: f64) -> (World, History) {
    drawing_job(stock_height, DXF)
}

fn drawing_job(stock_height: f64, dxf: &str) -> (World, History) {
    drawing_job_with(stock_height, dxf, &DxfOptions::default())
}

fn drawing_job_with(stock_height: f64, dxf: &str, options: &DxfOptions) -> (World, History) {
    let world = new_world();
    let mut history = History::default();
    let shape = StockShape::Box {
        dx: 100.0,
        dy: 120.0,
        dz: stock_height,
    };
    history.execute(&world, SetStock::new(&world, shape, Location::default()).expect("stock"));
    let tool = catalog()
        .into_iter()
        .find(|(n, _)| *n == TOOL)
        .map(|(_, t)| t)
        .expect("4 mm end mill");
    history.execute(&world, AddTool::new(&world, TOOL, tool).expect("tool"));
    history.execute(
        &world,
        ImportDxf::from_bytes(&world, None, "mark.dxf", dxf.as_bytes(), options).expect("import"),
    );
    history.execute(&world, MoveSketch::new(&world, PROFILE, 16.0, 8.0, 90.0).expect("place"));
    (world, history)
}

fn params(kind: OpType, climb: bool, depth: f64, stepdown: f64) -> OpParams {
    let clearance_z = Height::fixed(CLEARANCE);
    let top_z = Height::of(HeightReference::TopOfSelection, 0.0);
    let bottom_z = Height::of(HeightReference::TopOfSelection, -depth);
    let retract_z = None;
    let (feed, plunge_feed, tilt) = (180.0, 45.0, None);
    match kind {
        OpType::Engrave => OpParams::Engrave {
            clearance_z,
            retract_z,
            top_z,
            bottom_z,
            stepdown,
            feed,
            plunge_feed,
            climb,
            tilt,
            picks: Default::default(),
        },
        OpType::Trace => OpParams::Trace {
            clearance_z,
            retract_z,
            top_z,
            bottom_z,
            stepdown,
            feed,
            plunge_feed,
            climb,
            tilt,
            picks: Default::default(),
        },
        _ => panic!("expected one of the mark kinds"),
    }
}

fn layer(name: &str) -> GeoSel {
    GeoSel {
        entity: PROFILE.into(),
        feature: FeatureSel::SketchLayer(name.into()),
    }
}

fn open_pick(world: &World) -> GeoSel {
    let (_, sketch) = sketch::find(world, PROFILE).expect("profile");
    let chains = sketch.data.chains().by_layer("open");
    assert_eq!(chains.len(), 1);
    assert_eq!(chains[0].segments().len(), 2);
    GeoSel {
        entity: PROFILE.into(),
        feature: FeatureSel::SketchChain(chains[0].id()),
    }
}

fn add_op(world: &World, history: &mut History, src: SrcRef, params: OpParams) {
    history.execute(
        world,
        CreateOperation::new(world, OP, TOOL, src, params).expect("create mark operation"),
    );
}

fn generated(world: &World, history: &mut History) -> Toolpath {
    history.execute(world, GenerateOperation::new(world, OP).expect("generate mark"));
    let entity = model::find_by_name(world, OP).expect("mark operation");
    model::toolpath_of(world, &entity).expect("mark toolpath")
}

fn near(actual: f64, expected: f64) {
    assert!((actual - expected).abs() < EPS, "{actual:.12} should be {expected:.12}");
}

fn near_xy(p: Point3, [x, y]: [f64; 2]) -> bool {
    (p.x - x).abs() < EPS && (p.y - y).abs() < EPS
}

fn same_xy(a: Point3, b: Point3) -> bool {
    near_xy(a, [b.x, b.y])
}

fn ends(m: &Move, from: [f64; 2], to: [f64; 2]) -> bool {
    near_xy(m.from, from) && near_xy(m.to, to)
}

fn oriented_ends(m: &Move, a: [f64; 2], b: [f64; 2]) -> bool {
    ends(m, a, b) || ends(m, b, a)
}

fn assert_levels(moves: &[Move]) {
    let mut plunges = [0; 3];
    for (i, m) in moves.iter().enumerate() {
        // Every inter-stroke transfer (including a move to a new level)
        // is a rapid at clearance. With no separate retract height set,
        // returning to clearance splits all four independent strokes.
        match m.kind {
            MoveKind::Rapid if !same_xy(m.from, m.to) => {
                near(m.from.z, CLEARANCE);
                near(m.to.z, CLEARANCE);
            }
            MoveKind::Plunge => {
                assert!(same_xy(m.from, m.to) && m.arc.is_none(), "{m:?}");
                assert_eq!(m.feed, Some(45.0));
                let level = [5.8, 5.6, 5.5]
                    .iter()
                    .position(|z| (m.to.z - z).abs() < EPS)
                    .expect("plunge at a cut level");
                plunges[level] += 1;
                let done = moves[i + 1..]
                    .iter()
                    .find(|next| next.kind == MoveKind::Rapid)
                    .expect("retract after stroke");
                assert!(same_xy(done.from, done.to), "{done:?}");
                near(done.to.z, CLEARANCE);
            }
            MoveKind::Feed => {
                near(m.from.z, m.to.z);
                assert_eq!(m.feed, Some(180.0));
                assert!([5.8, 5.6, 5.5].iter().any(|z| (m.to.z - z).abs() < EPS), "{m:?}");
            }
            MoveKind::Rapid => {}
        }
        if let Some(prev) = i.checked_sub(1).map(|j| moves[j]) {
            assert_eq!(prev.to, m.from, "gap before move {i}");
        }
    }
    assert_eq!(plunges, [4; 3], "one plunge per stroke at each depth");
    near(moves.last().expect("last move").to.z, CLEARANCE);
}

fn assert_geometry(moves: &[Move], climb: bool) {
    let open_ends = ([7.0, 10.0], [7.0, 18.0], [11.0, 22.0]);
    let short_end = |angle: f64| [-6.0 - 8.0 * angle.to_radians().sin(), 19.0 + 8.0 * angle.to_radians().cos()];

    for level in [5.8, 5.6, 5.5] {
        let mut lines = [0; 2];
        let mut sweeps = [0.0; 4];
        let mut arcs_at = [0; 4];
        let mut circle_direction = None;
        let mut semicircle_ends = [false; 2];
        for m in moves.iter().filter(|m| m.kind == MoveKind::Feed && (m.to.z - level).abs() < EPS) {
            if let Some(a) = m.arc {
                assert_eq!(a.plane, ArcPlane::Xy);
                let (radius, sweep) = a.radius_sweep(m.from, m.to);
                let centers = [(11.0, 18.0), (9.0, 35.0), (9.0, 49.0), (-6.0, 19.0)];
                let index = centers
                    .iter()
                    .position(|(x, y)| (a.center.x - x).abs() < EPS && (a.center.y - y).abs() < EPS)
                    .unwrap_or_else(|| panic!("unexpected arc {m:?}"));
                near(radius, [4.0, 4.0, 3.0, 8.0][index]);
                near((m.to.x - a.center.x).hypot(m.to.y - a.center.y), radius);
                match index {
                    0 => {
                        assert_eq!(a.ccw, !climb, "{m:?}");
                        let (_, middle, end) = open_ends;
                        assert!(if climb { ends(m, middle, end) } else { ends(m, end, middle) }, "{m:?}");
                    }
                    1 => {
                        for (reached, p) in semicircle_ends.iter_mut().zip([[13.0, 35.0], [5.0, 35.0]]) {
                            *reached |= near_xy(m.from, p) || near_xy(m.to, p);
                        }
                        let middle = a.point_at(m.from, m.to, 0.5).expect("semicircle midpoint");
                        assert!(m.from.y >= 35.0 - EPS && m.to.y >= 35.0 - EPS && middle.y > 35.0, "{m:?}");
                    }
                    2 => assert_eq!(*circle_direction.get_or_insert(a.ccw), a.ccw, "{m:?}"),
                    3 => assert!(oriented_ends(m, short_end(15.0), short_end(16.0)), "{m:?}"),
                    _ => unreachable!(),
                }
                sweeps[index] += sweep;
                arcs_at[index] += 1;
            } else {
                let (start, middle, _) = open_ends;
                if (climb && ends(m, start, middle)) || (!climb && ends(m, middle, start)) {
                    lines[0] += 1;
                } else if oriented_ends(m, [13.0, 35.0], [5.0, 35.0]) {
                    lines[1] += 1;
                } else {
                    panic!("unexpected feed bridge, excluded geometry or flattened arc: {m:?}");
                }
            }
        }
        assert_eq!(lines, [1; 2], "each source line is cut once at Z={level}");
        assert_eq!([arcs_at[0], arcs_at[3]], [1; 2], "each short source arc is cut once at Z={level}");
        assert_eq!(semicircle_ends, [true; 2], "the closed stroke reaches both ends of the semicircle");
        assert!(arcs_at[2] >= 2, "a full circle needs distinct CL endpoints");
        for (actual, expected) in sweeps.into_iter().zip([FRAC_PI_2, PI, TAU, 1.0_f64.to_radians()]) {
            near(actual, expected);
        }
    }
}

#[test]
fn placed_sketch_marks_keep_open_and_closed_arcs_and_retract_between_strokes() {
    for kind in KINDS {
        for climb in [true, false] {
            let (world, mut history) = job(TOP);
            let src = SrcRef::Selection(vec![layer("closed"), layer("open"), open_pick(&world), layer("short")]);
            add_op(&world, &mut history, src, params(kind, climb, 0.5, 0.2));
            let mark = generated(&world, &mut history);
            assert!(mark.warnings.iter().all(|w| !w.blocks_post()), "{kind:?}: {:?}", mark.warnings);
            assert_levels(&mark.path.moves);
            assert_geometry(&mark.path.moves, climb);
        }
    }
}

#[test]
fn sketch_pick_placement_and_path_reopen_and_post_without_a_part() {
    for kind in KINDS {
        let (world, mut history) = job(TOP);
        let src = SrcRef::Selection(vec![open_pick(&world), layer("closed")]);
        add_op(&world, &mut history, src, params(kind, true, 0.5, 0.5));
        let before = generated(&world, &mut history);

        let fresh = new_world();
        project::open(&fresh, &project::save_text(&world).expect("save project")).expect("reopen project");
        assert!(!sketch_model::has_part(&fresh));
        let entity = model::find_by_name(&fresh, OP).expect("reopened operation");
        let op = model::operation_of(&fresh, &entity).expect("operation");
        let SrcRef::Selection(picks) = op.src else {
            panic!("sketch picks were lost")
        };
        assert_eq!(picks, [open_pick(&fresh), layer("closed")]);
        assert_eq!(operation::status_of(&fresh, &entity), OpStatus::Ok);
        let after = generated(&fresh, &mut History::default());
        assert_eq!(before.path.moves, after.path.moves);

        let report = posting::verification_report(&fresh);
        assert!(report.problems.is_empty(), "{kind:?}: {:?}", report.problems);
        assert!(report.not_judged.is_none(), "{kind:?}: {:?}", report.not_judged);
        let sim = stock_sim::simulate_document(&fresh).expect("stock sim with no part");
        assert_eq!(sim.dev.gouge_cells, 0);
        assert!(
            sim.surface.verts.iter().any(|v| (v.z - 5.5).abs() < 0.1),
            "the cutter removes material"
        );

        let post = posting::post_setup_unwritten(&fresh).expect("post sketch mark");
        assert!(
            post.program
                .lines()
                .any(|l| l.split_whitespace().any(|word| matches!(word, "G02" | "G2"))),
            "the reversed quarter-circle posts as a clockwise arc: {}",
            post.program
        );
        common::write_golden("sketch_engrave_trace.cl", &common::gen_to_cltext(&before.path));
    }
}
