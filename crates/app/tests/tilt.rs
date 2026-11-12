//! M5.1 stage B acceptance: the 3+2 index is a coordinate change, not
//! new geometry math. A tilted operation's path IS the flat operation's
//! path on the rotated model - same move count, rotated bytes - and
//! every lie the stage could tell (a projection where an index was
//! commanded, a stale stamp through a tilt edit, a lean posted to a
//! machine that cannot hold it) must refuse by name.
//!
//! Gate map (M4.4): covers cl::cl, app::commands, app::geometry, app::machine, post::machine,
//! motion::machining2d, app::model, app::operation, app::posting, app::project,
//! app::schema, geometry::geometry::stock, tools::tools, sim::verify, app::world, cl::frame.

use std::path::{Path, PathBuf};

use camcraft_app::cl::{MoveKind, TiltedFrame, is_vertical};
use camcraft_app::commands::{
    AddTool, CommandError, CreateOperation, EditOperationParams, GenerateOperation, History,
    ImportModel, LoadMachine, SetStock,
};
use camcraft_app::geometry::feature::FaceKind;
use camcraft_app::geometry::{Mesh, Point3, Triangle, cube};
use camcraft_app::machining2d::{Lead, PocketStrategy, Side};
use camcraft_app::machining3d::Entry3D;
use camcraft_app::model;
use camcraft_app::operation::{self, FeatureSel, GeoSel, Height, OpParams, OpStatus, SrcRef, Tilt};
use camcraft_app::posting;
use camcraft_app::project;
use camcraft_app::schema::{Location, Operation};
use camcraft_app::stock::StockShape;
use camcraft_app::tools::{catalog, Tool};
use camcraft_app::world::new_world;
use flecs_ecs::prelude::World;

fn catalog_tool(name: &str) -> Tool {
    catalog()
        .into_iter()
        .find(|(n, _)| *n == name)
        .map(|(_, t)| t)
        .expect("catalog entry")
}

fn add_tool(world: &World, history: &mut History, name: &str) {
    let cmd = AddTool::new(world, name, catalog_tool(name));
    history.execute(world, cmd.expect("catalog tool"));
}

fn cache(world: &World, name: &str) -> Option<camcraft_app::schema::Toolpath> {
    model::find_by_name(world, name).and_then(|e| model::toolpath_of(world, &e))
}

fn status(world: &World, name: &str) -> OpStatus {
    let e = model::find_by_name(world, name).expect("entity exists");
    operation::status_of(world, &e)
}

fn op_of(world: &World, name: &str) -> Operation {
    let e = model::find_by_name(world, name).expect("op exists");
    model::operation_of(world, &e).expect("operation")
}

fn tmp(tag: &str) -> PathBuf {
    std::env::temp_dir().join(format!(
        "camcraft_tilt_{tag}_{}.camproj",
        std::process::id()
    ))
}

fn asset(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../assets/machines")
        .join(name)
}

/// The 40 x 30 x 10 block, sunk so a 90-degree-indexed pocket of it
/// stays below the table plane (the tilthead's old -500..0 band; #457).
fn block() -> Mesh {
    cube(Point3::new(0.0, 0.0, -20.0), Point3::new(40.0, 30.0, -10.0))
}

/// The same block seen through the b = 90 frame by hand: the map
/// (x, y, z) -> (y, z, x) is Ry(90) applied to the coordinates -
/// written OUTSIDE the door on purpose, so test 1 compares the
/// pipeline against an independently rotated world, not the door
/// against itself.
fn rotated_by_90(m: Mesh) -> Mesh {
    Mesh::from_facets(
        m.facets
            .iter()
            .map(|t| Triangle::new(
                Point3::new(t.a.y, t.a.z, t.a.x),
                Point3::new(t.b.y, t.b.z, t.b.x),
                Point3::new(t.c.y, t.c.z, t.c.x),
            ))
            .collect(),
    )
}

/// Zigzag pocket from `top_z` down to 25. `top_z` must sit AT the
/// material's local top: a pocket started below the surface leaves an
/// uncut collar the shank law (rightly) reports as a collision, and a
/// test that generates a warning per level tests nothing.
fn pocket(tilt: Option<Tilt>, top_z: f64) -> OpParams {
    OpParams::Pocket {
        links: camcraft_app::links::Links::PLANAR,
        tilt,
        stepover: 3.0,
        stock_to_leave: 0.0,
        clearance_z: Height::fixed(70.0),
        retract_z: Some(Height::fixed(60.0)),
        top_z: Height::fixed(top_z),
        bottom_z: Height::fixed(25.0),
        stepdown: 5.0,
        feed: 800.0,
        plunge_feed: 100.0,
        climb: true,
        strategy: PocketStrategy::Zigzag,
        // M5.1 H defaults: the dive, and numbers the doors accept if a
        // test flips either field.
        entry: Entry3D::Plunge,
        helix_radius: 1.0,
        optimal_load: camcraft_app::load::DEFAULT_OPTIMAL_LOAD,
        blend_turns: false,
    }
}

/// The same pocket parked on `tilt90_parked`'s anchor, with every height
/// shifted so its LOCAL ladder is the flat twin's ladder plus the door's
/// 25 mm: law L1 subtracts only the anchor's z, while the frame maps the
/// material by the full anchor dot the axis (M5.1 B's known asymmetry,
/// recorded in RECOMMENDED.md). A fixture meant to be a rigid parked copy
/// must therefore choose heights by the door, not by hand.
fn pocket_parked() -> OpParams {
    OpParams::Pocket {
        links: camcraft_app::links::Links::PLANAR,
        tilt: Some(tilt90_parked()),
        stepover: 3.0,
        stock_to_leave: 0.0,
        clearance_z: Height::fixed(80.0),
        retract_z: Some(Height::fixed(70.0)),
        top_z: Height::fixed(50.0),
        bottom_z: Height::fixed(35.0),
        stepdown: 5.0,
        feed: 800.0,
        plunge_feed: 100.0,
        climb: true,
        strategy: PocketStrategy::Zigzag,
        entry: Entry3D::Plunge,
        helix_radius: 1.0,
        optimal_load: camcraft_app::load::DEFAULT_OPTIMAL_LOAD,
        blend_turns: false,
    }
}

fn tilt90() -> Tilt {
    Tilt {
        a_deg: 0.0,
        b_deg: 90.0,
        origin: [0.0; 3],
    }
}

/// The same 90-degree index parked inside a negative Z column: the
/// cutting plane it names sits strictly below the table plane (the
/// tilthead's old -500..0 band asked it; #457 states -500..500). The L1 law reads fixed heights
/// as distance along the axis from the anchor, so dropping the anchor
/// to z = -15 and shifting x = -25 puts the 25..35 pocket levels at
/// world x = 15..25 over the block - the cut moves, the shape does not.
fn tilt90_parked() -> Tilt {
    Tilt {
        a_deg: 0.0,
        b_deg: 90.0,
        origin: [-25.0, 0.0, -15.0],
    }
}

/// The law stage B stands on: run the planar generator in the frame's
/// local coordinates and cross to the world through the one door, and
/// the result is the ordinary flat program of the rotated world - move
/// for move. Both paths generate over the SAME generator inputs (the
/// tilted op's local facets ARE the twin model's facets by
/// construction), so equality is structural: same count, same kinds,
/// coordinates equal to the (z, x, y) relabeling within float noise.
#[test]
fn tilted_pocket_is_the_flat_pocket_rotated() {
    let world = new_world();
    let mut history = History::default();
    add_tool(&world, &mut history, "EM-0600-4FL");
    history.execute(&world, ImportModel::from_mesh("part", block()));
    history.execute(&world, ImportModel::from_mesh("twin", rotated_by_90(block())));
    history.execute(
        &world,
        CreateOperation::new(&world, "Flat", "EM-0600-4FL", SrcRef::Entity("twin".into()), pocket(None, 40.0))
            .expect("create flat"),
    );
    history.execute(
        &world,
        CreateOperation::new(
            &world,
            "Tilt",
            "EM-0600-4FL",
            SrcRef::Entity("part".into()),
            pocket(Some(tilt90()), 40.0),
        )
        .expect("create tilted"),
    );
    history.execute(&world, GenerateOperation::new(&world, "Flat").expect("generate flat"));
    history.execute(&world, GenerateOperation::new(&world, "Tilt").expect("generate tilted"));
    assert_eq!(status(&world, "Flat"), OpStatus::Ok);
    assert_eq!(status(&world, "Tilt"), OpStatus::Ok, "the tilt verifies clean in local");

    let flat = cache(&world, "Flat").expect("flat cache").path;
    let tilted = cache(&world, "Tilt").expect("tilted cache").path;
    assert!(!flat.is_empty());
    assert_eq!(
        tilted.moves.len(),
        flat.moves.len(),
        "exact structure: the door adds and drops no move"
    );
    for (i, (f, t)) in flat.moves.iter().zip(tilted.moves.iter()).enumerate() {
        assert_eq!(f.kind, t.kind, "move {i} kind");
        // to_world of the b = 90 frame is (x, y, z) -> (z, x, y).
        let want = Point3::new(f.to.z, f.to.x, f.to.y);
        let got = t.to;
        assert!(
            (want.x - got.x).abs() < 1e-9
                && (want.y - got.y).abs() < 1e-9
                && (want.z - got.z).abs() < 1e-9,
            "move {i} to: {want:?} vs {got:?}"
        );
        let want = Point3::new(f.from.z, f.from.x, f.from.y);
        assert!(
            (want.x - t.from.x).abs() < 1e-9
                && (want.y - t.from.y).abs() < 1e-9
                && (want.z - t.from.z).abs() < 1e-9,
            "move {i} from: {want:?} vs {:?}",
            t.from
        );
        // The pose: every move of the tilted path commands the frame's
        // axis; the prismatic twin commands none that leans.
        let leaning = t.axis.is_some_and(|a| !is_vertical(&a));
        assert!(leaning, "move {i} must carry the tilt stamp");
        assert!(f.axis.is_none() || !f.axis.is_some_and(|a| !is_vertical(&a)));
    }
    // The axis really is x-hat: the frame's own normalized letter.
    let f = TiltedFrame::from_angles(Point3::ZERO, 0.0, 90.0).expect("frame");
    let a = tilted.moves[0].axis.expect("stamped");
    for k in 0..3 {
        assert!((a[k] - f.axis[k]).abs() < 1e-12, "{a:?} vs {:?}", f.axis);
    }
    // The height law L1, read as a plane: every feed tip sits at axial
    // distance 25..=35 from the anchor - the fixed heights interpreted
    // along the tool axis, not through a world-Z projection.
    for (i, m) in tilted.moves.iter().enumerate() {
        if m.kind == MoveKind::Rapid {
            continue;
        }
        let d = [m.to.x, m.to.y, m.to.z];
        let axial = d[0] * f.axis[0] + d[1] * f.axis[1] + d[2] * f.axis[2];
        assert!(
            (25.0 - 1e-9..=40.0 + 1e-9).contains(&axial),
            "feed move {i} at axial {axial}"
        );
    }
}

/// The preview and the cut share one region law, so the viewport
/// outline of a tilted op is the flat outline of its local model,
/// mapped out through the frame - never a stale or projected shadow.
#[test]
fn tilted_preview_region_is_the_flat_region_rotated() {
    let tilt = Tilt {
        a_deg: 0.0,
        b_deg: 35.0,
        origin: [0.0; 3],
    };
    let f = TiltedFrame::from_angles(Point3::ZERO, tilt.a_deg, tilt.b_deg).expect("frame");
    // Tall enough that the frame's local z spans the pocket's 25..35
    // levels: with b = 35 the axial coordinate is 0.57 x + 0.82 z, so
    // this block reads local z from -16 to 43 - the sections exist.
    let tower = cube(Point3::new(0.0, 0.0, -10.0), Point3::new(40.0, 30.0, 25.0));
    let world = new_world();
    let mut history = History::default();
    add_tool(&world, &mut history, "EM-0600-4FL");
    history.execute(&world, ImportModel::from_mesh("part", tower));
    // The twin model IS the frame's local view of `part`, built through
    // to_local so a 35-degree rotation is not re-derived by hand.
    let local: Vec<Triangle> = f.local_facets(
        &model::find_by_name(&world, "part")
            .and_then(|e| model::facets_of(&world, &e))
            .expect("facets")[..],
    );
    history.execute(&world, ImportModel::from_mesh("twin", Mesh::from_facets(local)));
    history.execute(
        &world,
        CreateOperation::new(&world, "Flat", "EM-0600-4FL", SrcRef::Entity("twin".into()), pocket(None, 35.0))
            .expect("create"),
    );
    history.execute(
        &world,
        CreateOperation::new(
            &world,
            "Tilt",
            "EM-0600-4FL",
            SrcRef::Entity("part".into()),
            pocket(Some(tilt), 35.0),
        )
        .expect("create"),
    );

    let flat = operation::region_of(&world, &op_of(&world, "Flat")).expect("flat region");
    let tilted = operation::region_of(&world, &op_of(&world, "Tilt")).expect("tilted region");
    assert_eq!(flat.loops().len(), tilted.loops().len());
    // The region lives in the cutting plane: map the flat loop's
    // points up to the section's local height (35 - origin.z) and out.
    // Compare as point SETS: the two loops may start at different
    // vertices (slice walks facets in storage order, and the twin's
    // stored order is the decomposition's, not the frame mapping's),
    // but the lattice itself must be the same set of world points.
    let mut want: Vec<[f64; 2]> = Vec::new();
    for l in flat.loops() {
        for pf in l.points() {
            let w = f.to_world(Point3::new(pf.x, pf.y, 35.0));
            want.push([w.x, w.y]);
        }
    }
    let mut got: Vec<[f64; 2]> = tilted.loops().iter().flat_map(|l| l.points()).map(|pt| [pt.x, pt.y]).collect();
    assert_eq!(want.len(), got.len(), "same lattice");
    want.sort_by(|a, b| a[0].total_cmp(&b[0]).then(a[1].total_cmp(&b[1])));
    got.sort_by(|a, b| a[0].total_cmp(&b[0]).then(a[1].total_cmp(&b[1])));
    for (i, (w, g)) in want.iter().zip(got.iter()).enumerate() {
        assert!(
            // The twin's facets ride the mesh import's vertex weld
            // (6-decimal quantization); the tilted region's come from
            // in-memory local_facets. The door itself is exact - the
            // gap is the weld, so the assert forgives 1e-5, not less.
            (w[0] - g[0]).abs() < 1e-5 && (w[1] - g[1]).abs() < 1e-5,
            "lattice point {i}: {w:?} vs {g:?}"
        );
    }
}

/// A picked edge is world geometry; projecting it onto a tilted plane
/// is not implemented, and cutting a shadow of a pick would be the
/// silent wrong answer. Both the cut and the preview say so - the
/// first by name, the second by drawing nothing.
#[test]
fn tilt_on_a_picked_selection_refuses_by_name() {
    let world = new_world();
    let mut history = History::default();
    add_tool(&world, &mut history, "EM-0600-4FL");
    history.execute(
        &world,
        ImportModel::from_mesh("part", cube(Point3::ZERO, Point3::new(10.0, 10.0, 5.0))),
    );
    let e = model::find_by_name(&world, "part").expect("entity");
    let feats = model::features_of(&world, &e).expect("features");
    let top = FeatureSel::Face(
        feats
            .faces
            .iter()
            .filter(|fa| matches!(fa.kind, FaceKind::Planar) && fa.normal[2].abs() > 0.9)
            .max_by(|a, b| a.offset.total_cmp(&b.offset))
            .map(|fa| fa.id)
            .expect("a horizontal planar face"),
    );
    let contour = |tilt: Option<Tilt>| OpParams::Contour {
        tilt,
        stock_to_leave: 0.0,
        clearance_z: Height::fixed(10.0),
        retract_z: Height::fixed(8.0),
        top_z: Height::fixed(5.0),
        bottom_z: Height::fixed(0.0),
        stepdown: 5.0,
        feed: 800.0,
        plunge_feed: 200.0,
        climb: true,
        side: Side::Inside,
        lead: Lead::None,
        tolerance: 0.01,
        tab_count: 0,
        tab_width: 0.0,
        tab_height: 0.0,
        entry: Entry3D::Plunge,
        helix_radius: 1.0,
        blend_turns: false,
        links: camcraft_app::links::Links::PLANAR,
    };
    // The prismatic control: the same pick on the same part is legal.
    history.execute(
        &world,
        CreateOperation::new(
            &world,
            "Sel",
            "EM-0600-4FL",
            SrcRef::Selection(vec![GeoSel {
                entity: "part".into(),
                feature: top,
            }]),
            contour(None),
        )
        .expect("create"),
    );
    history.execute(&world, GenerateOperation::new(&world, "Sel").expect("flat selection cuts"));
    assert!(
        operation::region_of(&world, &op_of(&world, "Sel")).is_some(),
        "and draws"
    );

    history.execute(
        &world,
        EditOperationParams::new(&world, "Sel", contour(Some(tilt90()))).expect("edit"),
    );
    let err = GenerateOperation::new(&world, "Sel")
        .err()
        .expect("the tilt must refuse");
    assert!(
        err.to_string()
            .contains("a tilted plane cannot yet index a picked selection"),
        "{err}"
    );
    assert!(
        operation::region_of(&world, &op_of(&world, "Sel")).is_none(),
        "and the preview draws nothing rather than a shadow"
    );
}

/// The index answers at the command door: angles outside the canonical
/// ranges, or angles that are not numbers, never reach a generator -
/// an op that "tilts" by a non-finite letter must not exist.
#[test]
fn an_insane_tilt_refuses_at_the_command_door() {
    let world = new_world();
    let mut history = History::default();
    history.execute(
        &world,
        SetStock::new(
            &world,
            StockShape::Box {
                dx: 60.0,
                dy: 60.0,
                dz: 20.0,
            },
            Location::default(),
        )
        .expect("box stock"),
    );
    add_tool(&world, &mut history, "EM-0400-2FL");
    let face = |tilt: Option<Tilt>| OpParams::Face { holes: Default::default(), blend_turns: false,
        tilt,
        stepover: 3.0,
        clearance_z: Height::fixed(25.0),
        cut_z: Height::fixed(19.5),
        top_z: None,
        stepdown: None,
        feed: 1200.0,
        plunge_feed: 300.0,
        climb: true,
    };
    for bad in [
        Tilt {
            a_deg: 120.0,
            b_deg: 0.0,
            origin: [0.0; 3],
        },
        Tilt {
            a_deg: 0.0,
            b_deg: 200.0,
            origin: [0.0; 3],
        },
        Tilt {
            a_deg: f64::NAN,
            b_deg: 0.0,
            origin: [0.0; 3],
        },
        Tilt {
            a_deg: 0.0,
            b_deg: 0.0,
            origin: [0.0, f64::INFINITY, 0.0],
        },
    ] {
        assert!(
            matches!(
                CreateOperation::new(&world, "F", "EM-0400-2FL", SrcRef::Stock, face(Some(bad))),
                Err(CommandError::Invalid(_))
            ),
            "tilt {bad:?} must refuse at the door"
        );
        assert!(
            model::find_by_name(&world, "F").is_none(),
            "a refused creation writes nothing"
        );
    }
}

/// The tilt is operation state like every other parameter: it rides
/// the project file, the stamp, and the history.
#[test]
fn tilt_round_trips_through_the_project_file() {
    let path = tmp("rt");
    let world = new_world();
    let mut history = History::default();
    add_tool(&world, &mut history, "EM-0600-4FL");
    history.execute(&world, ImportModel::from_mesh("part", block()));
    let tilted = pocket(
        Some(Tilt {
            a_deg: 12.5,
            b_deg: -37.5,
            origin: [1.5, 2.5, -3.25],
        }),
        40.0,
    );
    history.execute(
        &world,
        CreateOperation::new(&world, "P", "EM-0600-4FL", SrcRef::Entity("part".into()), tilted.clone())
            .expect("create"),
    );
    project::save(&world, &path).expect("save");
    let world2 = new_world();
    project::load(&world2, &path).expect("load");
    let back = op_of(&world2, "P").params;
    assert_eq!(back, tilted, "the index survives the file byte-faithfully");
    let _ = std::fs::remove_file(&path);
}

/// Editing the tilt is editing a parameter: the stamp clears, the
/// cached path is declared stale (never silently kept), and undo
/// restores the prismatic index exactly.
#[test]
fn a_tilt_edit_marks_the_operation_stale_and_undo_restores() {
    let world = new_world();
    let mut history = History::default();
    history.execute(
        &world,
        SetStock::new(
            &world,
            StockShape::Box {
                dx: 60.0,
                dy: 60.0,
                dz: 20.0,
            },
            Location::default(),
        )
        .expect("box stock"),
    );
    add_tool(&world, &mut history, "EM-0400-2FL");
    history.execute(
        &world,
        CreateOperation::new(&world, "F", "EM-0400-2FL", SrcRef::Stock, OpParams::Face { holes: Default::default(), blend_turns: false,
            tilt: None,
            stepover: 3.0,
            clearance_z: Height::fixed(25.0),
            cut_z: Height::fixed(19.5),
            top_z: None,
            stepdown: None,
            feed: 1200.0,
            plunge_feed: 300.0,
            climb: true,
        })
        .expect("create"),
    );
    history.execute(&world, GenerateOperation::new(&world, "F").expect("generate"));
    assert_eq!(status(&world, "F"), OpStatus::Ok);
    history.execute(
        &world,
        EditOperationParams::new(
            &world,
            "F",
            OpParams::Face { holes: Default::default(), blend_turns: false,
                tilt: Some(Tilt {
                    a_deg: 0.0,
                    b_deg: 20.0,
                    origin: [0.0; 3],
                }),
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
        .expect("edit"),
    );
    assert_eq!(
        status(&world, "F"),
        OpStatus::Stale,
        "the cached path predates the index"
    );
    assert!(history.undo(&world));
    assert_eq!(
        status(&world, "F"),
        OpStatus::Ok,
        "undo restores the prismatic stamp"
    );
    assert_eq!(op_of(&world, "F").params.tilt(), None);
}

/// End to end, the way the shop asks it: a tilted pocket on a machine
/// that owns the letters posts the tilt; the same program against a 3
/// axis machine comes back named, because steel that cannot lean
/// cannot promise a lean.
#[test]
fn a_tilted_program_posts_under_a_tilthead_and_refuses_under_a_vmc() {
    let build = |w: World| {
        let world = w;
        let mut history = History::default();
        add_tool(&world, &mut history, "EM-0600-4FL");
        history.execute(&world, ImportModel::from_mesh("part", block()));
        history.execute(
            &world,
            CreateOperation::new(
                &world,
                "P",
                "EM-0600-4FL",
                SrcRef::Entity("part".into()),
                pocket_parked(),
            )
            .expect("create"),
        );
        history.execute(&world, GenerateOperation::new(&world, "P").expect("generate"));
        (world, history)
    };

    // The tilthead: A and B with 120 degrees of travel - 90 is inside.
    let (world, mut history) = build(new_world());
    history.execute(
        &world,
        LoadMachine::from_file(&world, &asset("tilthead.mch")).expect("mount"),
    );
    let out = tmp("tilthead.nc");
    let posted = posting::post_setup(&world, &out).expect("the letters hold the lean");
    assert!(
        posted.program.contains("G43.4"),
        "a tilting machine's program arms the tilt: {}",
        posted.program
    );
    assert!(out.exists());
    let _ = std::fs::remove_file(&out);

    // The plain VMC: no rotary anywhere - the SAME cached path must
    // refuse the post, by name, writing nothing.
    let (world, mut history) = build(new_world());
    history.execute(
        &world,
        LoadMachine::from_file(&world, &asset("3axis-vmc.mch")).expect("mount"),
    );
    let out = tmp("vmc.nc");
    let err = posting::post_setup(&world, &out).expect_err("a 3-axis machine tilts nothing");
    assert!(err.contains("tilt"), "{err}");
    assert!(!out.exists(), "a refused post writes nothing");
}

/// M5.1 stage H on the index: the entry family was lifted to the plane
/// the tilted pocket runs in, so a helix dives in LOCAL coordinates and
/// the one door (cl's world_path) answers with chord polylines - never
/// a tilted arc, never a lie. The 35-degree tower is the fixture on
/// purpose: its local section is wide enough for the widened disc, so
/// the dive is real, and the Plunge twin shows what "real" changed.
#[test]
fn a_tilted_pocket_sinks_on_a_helix_in_chords() {
    let tilt = Tilt {
        a_deg: 0.0,
        b_deg: 35.0,
        origin: [0.0; 3],
    };
    let tower = cube(Point3::new(0.0, 0.0, -10.0), Point3::new(40.0, 30.0, 25.0));
    let world = new_world();
    let mut history = History::default();
    add_tool(&world, &mut history, "EM-0600-4FL");
    history.execute(&world, ImportModel::from_mesh("part", tower));
    let mut helix = pocket(Some(tilt), 35.0);
    let OpParams::Pocket {
        entry, helix_radius, ..
    } = &mut helix
    else {
        panic!("pocket params expected");
    };
    *entry = Entry3D::Helix;
    *helix_radius = 2.0; // the 6 mm tool leaves room to spare in the band
    for (name, p) in [("Helix", helix), ("Plain", pocket(Some(tilt), 35.0))] {
        history.execute(
            &world,
            CreateOperation::new(
                &world,
                name,
                "EM-0600-4FL",
                SrcRef::Entity("part".into()),
                p,
            )
            .expect("create"),
        );
        history.execute(&world, GenerateOperation::new(&world, name).expect("generate"));
        assert_eq!(status(&world, name), OpStatus::Ok, "{name} verifies clean in local");
    }
    let f = TiltedFrame::from_angles(Point3::ZERO, tilt.a_deg, tilt.b_deg).expect("frame");
    // The cached path is already world-expressed (the door ran at
    // generate), so the axial depth of a move end is a plain dot with
    // the axis letter - mapping it through the frame again would tilt
    // the answer twice and invent a slope of stepover x sin(b).
    let axial = |p: Point3| p.x * f.axis[0] + p.y * f.axis[1] + p.z * f.axis[2];
    // A feed that changes its axial depth is the diagonal the plain
    // ladder never posts: arcs crossed the plane, chords followed.
    let diagonals = |t: &camcraft_app::cl::Toolpath| -> usize {
        t.moves
            .iter()
            .filter(|m| {
                m.kind == MoveKind::Feed && (axial(m.from) - axial(m.to)).abs() > 1e-6
            })
            .count()
    };
    let helix = cache(&world, "Helix").expect("helix cache").path;
    let plain = cache(&world, "Plain").expect("plain cache").path;
    assert!(helix.is_chained() && helix.all_finite(), "a real program");
    assert!(
        helix.moves.iter().all(|m| m.arc.is_none()),
        "the door answers with chords, never a tilted arc"
    );
    assert!(diagonals(&helix) > 0, "the helix sank, it did not fall back");
    assert_eq!(diagonals(&plain), 0, "the Plunge twin is the plain ladder");
}
