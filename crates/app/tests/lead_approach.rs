//! M11.3 B/C/D through the app: the entry, lead and feed parameters of
//! the Lead in / Lead out section on the user's own part
//! (`testdata/models/Milling-test.step`, a D-shaped plate with a blind
//! bore) - each generated through the commands and verified against the
//! part solid (depth-aware); the axis-aligned lead start turns the user's
//! 4.4-degree-off Arc entry into true G18/G19 arcs in the post; the
//! project file writes the `approach` key only when it differs and
//! round-trips it byte for byte; the stamp tells the choices apart and
//! leaves a default op's digest alone; the tool's centre-cutting flag
//! round-trips and changes the digest only when cleared; and the user's
//! keep-down helix pocket enters every level once (no red plunge).
//!
//! Gate map (M11.3): covers app::operation (params, stamp, sane, links,
//! generate), app::project (approach DTO, tool DTO), app::commands,
//! sim::verify, app::posting.

use camcraft_app::cl::{MoveKind, Toolpath};
use camcraft_app::commands::{AddTool, CreateOperation, GenerateOperation, History, ImportModel, SetStock};
use camcraft_app::geometry::Point3;
use camcraft_app::links::{Approach, Link, Links};
use camcraft_app::machining2d::{Lead, PocketStrategy, Side};
use camcraft_app::machining3d::Entry3D;
use camcraft_app::operation::{self, Height, HeightReference, OpParams, SrcRef};
use camcraft_app::schema::Location;
use camcraft_app::stock::StockShape;
use camcraft_app::tools::catalog;
use camcraft_app::world::new_world;
use camcraft_app::{model, posting, project};
use flecs_ecs::prelude::World;

fn contour_op(lead: Lead, links: Links) -> OpParams {
    OpParams::Contour {
        stock_to_leave: 0.0,
        clearance_z: Height::of(HeightReference::TopOfStock, 20.5),
        retract_z: Height::of(HeightReference::TopOfStock, 13.5),
        top_z: Height::of(HeightReference::TopOfModel, 0.0),
        bottom_z: Height::of(HeightReference::BottomOfStock, 0.0),
        stepdown: 5.0,
        feed: 1200.0,
        plunge_feed: 300.0,
        climb: true,
        side: Side::Auto,
        lead,
        tolerance: 0.01,
        tab_count: 0,
        tab_width: 5.0,
        tab_height: 1.0,
        entry: Entry3D::Arc,
        helix_radius: 1.0,
        blend_turns: true,
        tilt: None,
        links,
    }
}

/// The user's reported pocket: Source = Stock, Helix entry r 1.0, Helix
/// level descent pitch 1.0, keep down between levels, retract plane
/// between passes, top = model top.
fn pocket_op(links: Links) -> OpParams {
    OpParams::Pocket {
        stepover: 2.4,
        stock_to_leave: 0.0,
        clearance_z: Height::of(HeightReference::TopOfStock, 20.5),
        retract_z: Some(Height::of(HeightReference::TopOfStock, 2.0)),
        top_z: Height::of(HeightReference::TopOfModel, 0.0),
        bottom_z: Height::fixed(10.0),
        stepdown: 2.5,
        feed: 1800.0,
        plunge_feed: 300.0,
        climb: true,
        strategy: PocketStrategy::Waterline,
        entry: Entry3D::Helix,
        helix_radius: 1.0,
        optimal_load: camcraft_app::load::DEFAULT_OPTIMAL_LOAD,
        blend_turns: false,
        tilt: None,
        links,
    }
}

fn user_job(tool_name: &str, src_entity: bool, params: OpParams) -> (World, Toolpath, Vec<String>) {
    let world = new_world();
    let mut h = History::default();
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testdata/models/Milling-test.step");
    let imp = ImportModel::new(&path).expect("the user's STEP imports");
    let name = imp.name.clone();
    h.execute(&world, imp);
    let loc = Location { pos: Point3::new(0.0, 0.0, -7.0), ..Default::default() };
    h.execute(&world, SetStock::new(&world, StockShape::Box { dx: 70.0, dy: 70.0, dz: 27.0 }, loc).expect("stock"));
    let tool = catalog().into_iter().find(|(n, _)| *n == tool_name).map(|(_, t)| t).expect("catalog");
    h.execute(&world, AddTool::new(&world, tool_name, tool).expect("tool"));
    let src = if src_entity { SrcRef::Entity(name) } else { SrcRef::Stock };
    h.execute(&world, CreateOperation::new(&world, "Op1", tool_name, src, params).expect("create"));
    h.execute(&world, GenerateOperation::new(&world, "Op1").expect("generate"));
    let e = model::find_by_name(&world, "Op1").expect("op");
    let tp = model::toolpath_of(&world, &e).expect("cached path");
    let warns = tp.warnings.iter().map(|w| format!("{w:?}")).collect();
    let path = tp.path.clone();
    (world, path, warns)
}

fn with(a: Approach) -> Links {
    Links { approach: a, ..Links::PLANAR }
}

fn vertical_arcs(t: &Toolpath) -> usize {
    t.moves.iter().filter(|m| m.arc.is_some_and(|a| !a.plane.is_xy())).count()
}

#[test]
fn an_axis_aligned_lead_start_posts_the_users_arc_entry_as_g18_or_g19() {
    let lead = Lead::Tangential { radius: 5.0 };
    // Today: the outer ring enters 4.4 degrees off X - chords, said once.
    let (_w, base, warns) = user_job("EM-1000-4FL", true, contour_op(lead, Links::PLANAR));
    assert!(warns.is_empty(), "{warns:?}");
    assert_eq!(vertical_arcs(&base), 0);
    let a = Approach { axis_start: true, ..Approach::DEFAULT };
    let (world, t, warns) = user_job("EM-1000-4FL", true, contour_op(lead, with(a)));
    assert!(warns.is_empty(), "verify silent against the part: {warns:?} / {:?}", t.notes);
    assert!(vertical_arcs(&t) > 0, "true vertical arcs now: {:?}", t.notes);
    let nc = std::env::temp_dir().join(format!("camcraft-lead-approach-g18-{}.nc", std::process::id()));
    posting::post_setup(&world, &nc).expect("posts");
    let text = std::fs::read_to_string(&nc).expect("written");
    let _ = std::fs::remove_file(&nc);
    assert!(text.contains("G18") || text.contains("G19"), "a plane word in the post");
}

#[test]
fn every_lead_option_on_the_users_part_verifies_silent() {
    let lead = Lead::Tangential { radius: 5.0 };
    for a in [
        Approach { lead_out: Some(Lead::Perpendicular { length: 4.0 }), ..Approach::DEFAULT },
        Approach { ext_in: 3.0, ext_out: 3.0, ..Approach::DEFAULT },
        Approach { overlap: 2.0, ..Approach::DEFAULT },
        Approach { lead_feed_pct: 50.0, ..Approach::DEFAULT },
        Approach { arc_radius: 8.0, arc_sweep: 45.0, ..Approach::DEFAULT },
    ] {
        let (_w, t, warns) = user_job("EM-1000-4FL", true, contour_op(lead, with(a)));
        assert!(warns.is_empty(), "{a:?}: {warns:?} / {:?}", t.notes);
        assert!(t.is_chained() && t.all_finite());
    }
    // Perpendicular lead-in with a vertical lead-in height: plunge entry
    // grazes the lead start a height over the plane and descends along it.
    let a = Approach { lead_height: 2.0, ..Approach::DEFAULT };
    let mut op = contour_op(Lead::Perpendicular { length: 4.0 }, with(a));
    if let OpParams::Contour { entry, .. } = &mut op {
        *entry = Entry3D::Plunge;
    }
    let (_w, t, warns) = user_job("EM-1000-4FL", true, op);
    assert!(warns.is_empty(), "{warns:?} / {:?}", t.notes);
    let ramped = t.moves.iter().filter(|m| m.kind == MoveKind::Feed && m.to.z < m.from.z).count();
    assert!(ramped > 0, "the lead-in descends: {:?}", t.notes);
}

#[test]
fn the_reported_keep_down_helix_pocket_enters_every_level_once() {
    let links = Links { transition: Some(Entry3D::Helix), helix_pitch: 1.0, level: Link::KeepDown, ..Links::PLANAR };
    let (_w, t, warns) = user_job("EM-0600-4FL", false, pocket_op(links));
    assert!(warns.is_empty(), "{warns:?}");
    let helix_ends: Vec<usize> = (1..t.moves.len())
        .filter(|&i| {
            let (a, b) = (&t.moves[i - 1], &t.moves[i]);
            a.arc.is_some() && a.to.z < a.from.z && !(b.arc.is_some() && b.to.z < b.from.z)
        })
        .collect();
    assert!(!helix_ends.is_empty(), "{:?}", t.notes);
    for i in helix_ends {
        let z = t.moves[i - 1].to.z;
        // After a helix: no plunge until the level is done.
        let next_level = t.moves[i..].iter().position(|m| m.kind == MoveKind::Feed && m.from.z == m.to.z && m.to.z < z - 1e-9);
        let end = next_level.map_or(t.moves.len(), |k| i + k);
        let first_cut = t.moves[i..end].iter().position(|m| m.kind == MoveKind::Feed && m.from.z == m.to.z);
        let first_cut = first_cut.map_or(end, |k| i + k);
        assert!(
            !t.moves[i..first_cut].iter().any(|m| m.kind == MoveKind::Plunge || m.to.z > m.from.z),
            "no lift or plunge between the helix and the first cut at Z {z}: {:?}",
            &t.moves[i..first_cut]
        );
    }
    assert!(!t.notes.iter().any(|n| n.contains("second dive")), "{:?}", t.notes);
}

fn roundtrip(params: OpParams, key: Option<&str>) -> OpParams {
    let world = new_world();
    let mut h = History::default();
    h.execute(&world, SetStock::new(&world, StockShape::Box { dx: 60.0, dy: 60.0, dz: 20.0 }, Default::default()).expect("stock"));
    let mut tool = catalog().into_iter().find(|(n, _)| *n == "EM-1000-4FL").map(|(_, t)| t).expect("catalog");
    tool.center_cutting = key.is_none();
    h.execute(&world, AddTool::new(&world, "T", tool).expect("tool"));
    h.execute(&world, CreateOperation::new(&world, "Op1", "T", SrcRef::Stock, params).expect("create"));
    let path = std::env::temp_dir().join(format!(
        "camcraft-lead-approach-{}-{}.camproj",
        key.map_or(String::from("none"), |k| k.chars().filter(char::is_ascii_alphanumeric).collect::<String>()),
        std::process::id()
    ));
    project::save(&world, &path).expect("save");
    let first = std::fs::read_to_string(&path).expect("read");
    match key {
        None => {
            assert!(!first.contains("\"approach\""), "a default approach writes no key");
            assert!(!first.contains("center_cutting"), "a centre-cutting tool writes no key");
        }
        Some(k) => {
            assert!(first.contains(k), "{k} missing: {first}");
            assert!(first.contains("\"center_cutting\": false"), "the cleared flag is written");
        }
    }
    let world2 = new_world();
    project::load(&world2, &path).expect("load");
    project::save(&world2, &path).expect("resave");
    let second = std::fs::read_to_string(&path).expect("reread");
    let _ = std::fs::remove_file(&path);
    assert_eq!(first, second, "round-trips byte for byte");
    let e = model::find_by_name(&world2, "Op1").expect("op");
    model::operation_of(&world2, &e).expect("operation").params
}

#[test]
fn the_horizontal_arc_entry_on_the_users_pocket_verifies_silent() {
    // M11.3 (owed): the user's waterline pocket with the G17 arc entry -
    // every level opens with a planar arc onto its first ring, verified
    // against the part solid; the stated fallbacks are named, never
    // silent.
    for entry in [Entry3D::Helix, Entry3D::Plunge, Entry3D::Ramp] {
        let mut op = pocket_op(with(Approach { arc_xy: true, ..Approach::DEFAULT }));
        if let OpParams::Pocket { entry: e, .. } = &mut op {
            *e = entry;
        }
        let (_w, t, warns) = user_job("EM-0600-4FL", false, op);
        assert!(warns.is_empty(), "{entry:?}: {warns:?} / {:?}", t.notes);
        assert!(t.is_chained() && t.all_finite());
        let first = t.moves.iter().find(|m| m.kind == MoveKind::Feed && m.from.z == m.to.z).expect("a cut");
        assert!(first.arc.is_some_and(|a| a.plane.is_xy()), "{entry:?}: the first cut is the G17 arc: {first:?} / {:?}", t.notes);
        assert!(!t.notes.iter().any(|n| n.contains("horizontal arc entry does not fit")), "{entry:?}: {:?}", t.notes);
    }
}

#[test]
fn the_approach_round_trips_and_the_defaults_write_no_key() {
    let lead = Lead::Tangential { radius: 5.0 };
    let _ = roundtrip(contour_op(lead, Links::PLANAR), None);
    let a = Approach {
        ramp_angle: 4.0,
        ramp_max_len: 6.0,
        ramp_to_depth: true,
        helix_min_radius: 0.5,
        helix_pitch: 0.8,
        helix_cw: true,
        arc_radius: 7.0,
        arc_sweep: 60.0,
        plunge_height: 1.0,
        ramp_feed: 400.0,
        plunge_feed: 150.0,
        lead_feed_pct: 70.0,
        max_engagement: 0.5,
        lead_out: Some(Lead::Perpendicular { length: 3.0 }),
        ext_in: 2.0,
        ext_out: 1.5,
        lead_height: 1.0,
        overlap: 2.5,
        axis_start: true,
        axis_tol: 2.0,
        arc_xy: true,
        ..Approach::DEFAULT
    };
    let back = roundtrip(contour_op(Lead::Perpendicular { length: 4.0 }, with(a)), Some("\"approach\""));
    assert_eq!(back.links(), Some(&with(a)));
}

#[test]
fn the_stamp_tells_the_approach_apart_and_a_default_op_keeps_its_digest() {
    let world = new_world();
    let mut h = History::default();
    h.execute(&world, SetStock::new(&world, StockShape::Box { dx: 60.0, dy: 60.0, dz: 20.0 }, Default::default()).expect("stock"));
    let tool = catalog().into_iter().find(|(n, _)| *n == "EM-1000-4FL").map(|(_, t)| t).expect("catalog");
    h.execute(&world, AddTool::new(&world, "T", tool.clone()).expect("tool"));
    let lead = Lead::Tangential { radius: 5.0 };
    let stamp = |l: Links| operation::current_stamp(&world, "T", &SrcRef::Stock, &contour_op(lead, l), "X", None);
    let base = stamp(Links::PLANAR);
    assert!(base.is_some());
    // A links choice with a default approach keeps the M11.2 digest:
    // an approach change adds bytes on top of it.
    let kd = Links { level: Link::Retract, ..Links::PLANAR };
    assert_ne!(stamp(kd), base);
    for a in [
        Approach { ramp_angle: 3.0, ..Approach::DEFAULT },
        Approach { helix_cw: true, ..Approach::DEFAULT },
        Approach { plunge_height: 0.0, ..Approach::DEFAULT },
        Approach { lead_out: Some(Lead::None), ..Approach::DEFAULT },
        Approach { axis_start: true, ..Approach::DEFAULT },
        Approach { arc_xy: true, ..Approach::DEFAULT },
    ] {
        assert_ne!(stamp(with(a)), base, "{a:?}");
        assert_ne!(stamp(Links { approach: a, ..kd }), stamp(kd), "{a:?}");
    }
    // The runtime centre-cutting flag never reaches the stored digest.
    assert_eq!(stamp(with(Approach { center_cutting: false, ..Approach::DEFAULT })), base);
    // Nonsense refused at the door.
    let bad = with(Approach { arc_sweep: 200.0, ..Approach::DEFAULT });
    assert!(CreateOperation::new(&world, "Bad", "T", SrcRef::Stock, contour_op(lead, bad)).is_err());
}
