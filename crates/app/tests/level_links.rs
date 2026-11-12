//! Level transitions and links through the app (M11.2 C/D): the user's
//! own part (`testdata/models/Milling-test.step`, a D-shaped plate with a
//! blind bore) contoured and pocketed under every level link and
//! transition, generated through the commands and verified against the
//! part solid (the depth-aware law) - every path silent; the project
//! file writes the `links` key only when the choice differs from the
//! kind's earlier shape and round-trips it byte for byte; the stamp
//! tells the choices apart and leaves a default op's digest alone; and
//! the helix entry posts as continuous helical G2/G3 words.
//!
//! Gate map (M11.2): covers app::operation (params, stamp, sane, links),
//! app::project (links DTO, contour migration), app::commands, sim::verify,
//! app::posting.

use camcraft_app::cl::{Move, MoveKind, Toolpath};
use camcraft_app::commands::{AddTool, CreateOperation, GenerateOperation, History, ImportModel, SetStock};
use camcraft_app::geometry::Point3;
use camcraft_app::links::{Link, Links};
use camcraft_app::machining2d::{Lead, PocketStrategy, Side};
use camcraft_app::machining3d::Entry3D;
use camcraft_app::operation::{self, Height, HeightReference, OpParams, SrcRef};
use camcraft_app::schema::Location;
use camcraft_app::stock::StockShape;
use camcraft_app::tools::catalog;
use camcraft_app::world::new_world;
use camcraft_app::{model, posting, project};
use flecs_ecs::prelude::World;

const TOOL: &str = "EM-1000-4FL";

fn contour_op(links: Links) -> OpParams {
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
        lead: Lead::Tangential { radius: 5.0 },
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

/// The 3-D pocket ladder on the part: the footprint minus the part
/// standing above each level - the blind bore, cleared to its floor.
fn pocket_op(entry: Entry3D, links: Links) -> OpParams {
    OpParams::Pocket3D {
        links,
        stepover: 4.0,
        stock_to_leave: 0.0,
        clearance_z: Height::of(HeightReference::TopOfStock, 20.5),
        retract_z: Some(Height::of(HeightReference::TopOfStock, 2.0)),
        top_z: Height::of(HeightReference::TopOfModel, 0.0),
        // The blind bore's floor (Z 0): the ladder clears the bore, not the
        // part's underside (see `probe_helix_gouge` for the stock-bottom case).
        bottom_z: Height::fixed(0.0),
        stepdown: 2.5,
        feed: 1200.0,
        plunge_feed: 300.0,
        climb: true,
        strategy: PocketStrategy::Waterline,
        entry,
        helix_radius: 2.0,
        rest_rough: false,
        optimal_load: camcraft_app::load::DEFAULT_OPTIMAL_LOAD,
        feed_opt: false,
        shell: false,
    }
}

/// The user's part in a 70 x 70 block from Z -7 to its top (Z 20), the
/// 10 mm tool, one op - generated through the commands.
fn user_job(params: OpParams) -> (World, History, Toolpath, Vec<String>) {
    let world = new_world();
    let mut h = History::default();
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testdata/models/Milling-test.step");
    let imp = ImportModel::new(&path).expect("the user's STEP imports");
    let name = imp.name.clone();
    h.execute(&world, imp);
    let loc = Location { pos: Point3::new(0.0, 0.0, -7.0), ..Default::default() };
    h.execute(&world, SetStock::new(&world, StockShape::Box { dx: 70.0, dy: 70.0, dz: 27.0 }, loc).expect("stock"));
    let tool = catalog().into_iter().find(|(n, _)| *n == TOOL).map(|(_, t)| t).expect("catalog");
    h.execute(&world, AddTool::new(&world, TOOL, tool).expect("tool"));
    h.execute(&world, CreateOperation::new(&world, "Op1", TOOL, SrcRef::Entity(name), params).expect("create"));
    h.execute(&world, GenerateOperation::new(&world, "Op1").expect("generate"));
    let e = model::find_by_name(&world, "Op1").expect("op");
    let tp = model::toolpath_of(&world, &e).expect("cached path");
    let warns = tp.warnings.iter().map(|w| format!("{w:?}")).collect();
    let path = tp.path.clone();
    (world, h, path, warns)
}

fn planar(m: &Move) -> bool {
    m.kind == MoveKind::Feed && m.from.z == m.to.z
}

/// `(above, below, moves between)` for every level change.
fn transitions(t: &Toolpath) -> Vec<(f64, f64, Vec<Move>)> {
    let mut out = Vec::new();
    let mut level: Option<f64> = None;
    let mut between = Vec::new();
    for m in &t.moves {
        if planar(m) {
            if let Some(z) = level
                && m.to.z < z - 1e-9
            {
                out.push((z, m.to.z, std::mem::take(&mut between)));
            }
            between.clear();
            level = Some(m.to.z);
        } else {
            between.push(*m);
        }
    }
    out
}

fn top(ms: &[Move]) -> f64 {
    ms.iter().map(|m| m.to.z.max(m.from.z)).fold(f64::NEG_INFINITY, f64::max)
}

#[test]
fn the_users_part_contours_clean_under_every_level_link() {
    // Stock top 20: clearance 40.5, retract 33.5.
    let cases = [
        Links::PLANAR,
        Links { level: Link::Retract, ..Links::PLANAR },
        Links { level: Link::KeepDown, transition: Some(Entry3D::Ramp), ..Links::PLANAR },
        Links { level: Link::KeepDown, transition: Some(Entry3D::Plunge), ..Links::PLANAR },
        Links { level: Link::KeepDown, transition: Some(Entry3D::Helix), helix_pitch: 2.5, ..Links::PLANAR },
        Links { level: Link::KeepDown, transition: Some(Entry3D::Arc), ..Links::PLANAR },
        Links { pass: Link::KeepDown, ..Links::PLANAR },
        Links { pass: Link::Clearance, level: Link::Retract, ..Links::PLANAR },
    ];
    for links in cases {
        let (_w, _h, t, warns) = user_job(contour_op(links));
        assert!(warns.is_empty(), "{links:?}: {warns:?} / {:?}", t.notes);
        assert!(t.is_chained() && t.all_finite());
        let tr = transitions(&t);
        assert!(!tr.is_empty(), "{links:?}");
        for (above, below, ms) in &tr {
            if links.level == Link::Retract {
                assert!(top(ms) < 40.5, "{links:?}: {above} -> {below} stays under clearance");
            }
        }
        if links.level == Link::KeepDown {
            // Ring by ring: each ring's own ladder stays down (the only
            // rise is the move to the next ring's top, or a level the
            // notes name).
            let stayed = tr.iter().filter(|(above, _, ms)| top(ms) <= above + 1e-9).count();
            let rose = tr.len() - stayed;
            let named = t.notes.iter().filter(|n| n.starts_with("level link")).count();
            assert!(stayed >= 4, "{links:?}: the rings stay down: {stayed} of {}", tr.len());
            assert!(rose <= 1 + named, "{links:?}: {rose} rises, {named} named: {:?}", t.notes);
        }
    }
}

#[test]
fn the_users_part_pockets_its_bore_clean_under_every_choice() {
    let cases = [
        (Entry3D::Plunge, Links::ROUGH3D),
        (Entry3D::Helix, Links::ROUGH3D),
        (Entry3D::Plunge, Links { transition: Some(Entry3D::Helix), helix_pitch: 1.0, ..Links::ROUGH3D }),
        (Entry3D::Plunge, Links { transition: Some(Entry3D::Ramp), ramp_angle: 5.0, ..Links::ROUGH3D }),
        (Entry3D::Helix, Links { level: Link::Retract, ..Links::ROUGH3D }),
        (Entry3D::Helix, Links { level: Link::KeepDown, ..Links::ROUGH3D }),
        (Entry3D::Plunge, Links { level: Link::KeepDown, transition: Some(Entry3D::Arc), ..Links::ROUGH3D }),
        (Entry3D::Plunge, Links { pass: Link::Retract, ..Links::ROUGH3D }),
        (Entry3D::Plunge, Links { pass: Link::KeepDown, max_stay_down: 5.0, ..Links::ROUGH3D }),
    ];
    for (entry, links) in cases {
        let (_w, _h, t, warns) = user_job(pocket_op(entry, links));
        assert!(warns.is_empty(), "{entry:?} {links:?}: {warns:?} / {:?}", t.notes);
        assert!(t.is_chained() && t.all_finite() && !t.moves.is_empty());
        if links.level == Link::Retract {
            for (above, below, ms) in transitions(&t) {
                assert!(top(&ms) <= 22.0 + 1e-9, "{above} -> {below}: retract plane 22, got {}", top(&ms));
            }
        }
        if links.level == Link::KeepDown {
            let said = t.notes.iter().any(|n| n.starts_with("link:"));
            for (above, below, ms) in transitions(&t) {
                assert!(top(&ms) <= above + 1e-9 || said, "{above} -> {below}: {:?}", t.notes);
            }
        }
    }
}

#[test]
fn a_helix_entry_posts_continuous_helical_arcs() {
    let (world, _h, t, warns) = user_job(pocket_op(Entry3D::Helix, Links::ROUGH3D));
    assert!(warns.is_empty(), "{warns:?}");
    let helix: Vec<&Move> = t.moves.iter().filter(|m| m.arc.is_some() && m.to.z < m.from.z).collect();
    assert!(!helix.is_empty(), "{:?}", t.notes);
    let nc = std::env::temp_dir().join(format!("camcraft-level-links-helix-{}.nc", std::process::id()));
    posting::post_setup(&world, &nc).expect("posts");
    let text = std::fs::read_to_string(&nc).expect("written");
    let _ = std::fs::remove_file(&nc);
    // Every helical arc word carries its Z: G2/G3 with X, Y and Z.
    let helical = text
        .lines()
        .filter(|l| {
            let w: Vec<&str> = l.split_whitespace().collect();
            w.iter().any(|t| matches!(*t, "G2" | "G3" | "G02" | "G03"))
                && w.iter().any(|t| t.starts_with('Z'))
        })
        .count();
    assert!(helical >= helix.len(), "{helical} helical words for {} helix moves", helix.len());
}

fn roundtrip(params: OpParams, key: Option<&str>) -> OpParams {
    let world = new_world();
    let mut h = History::default();
    h.execute(&world, SetStock::new(&world, StockShape::Box { dx: 60.0, dy: 60.0, dz: 20.0 }, Default::default()).expect("stock"));
    let tool = catalog().into_iter().find(|(n, _)| *n == TOOL).map(|(_, t)| t).expect("catalog");
    h.execute(&world, AddTool::new(&world, TOOL, tool).expect("tool"));
    h.execute(&world, CreateOperation::new(&world, "Op1", TOOL, SrcRef::Stock, params).expect("create"));
    let path = std::env::temp_dir().join(format!("camcraft-level-links-{}-{}.camproj", key.map_or(String::from("none"), |k| k.chars().filter(char::is_ascii_alphanumeric).collect::<String>()), std::process::id()));
    project::save(&world, &path).expect("save");
    let first = std::fs::read_to_string(&path).expect("read");
    match key {
        None => assert!(!first.contains("\"links\""), "a default links choice writes no key: {first}"),
        Some(k) => assert!(first.contains(k), "{k} missing: {first}"),
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
fn links_round_trip_and_the_defaults_write_no_key() {
    let pocket2d = |links: Links| OpParams::Pocket {
        stepover: 3.0,
        stock_to_leave: 0.0,
        clearance_z: Height::fixed(25.0),
        retract_z: Some(Height::fixed(22.0)),
        top_z: Height::fixed(20.0),
        bottom_z: Height::fixed(14.0),
        stepdown: 2.0,
        feed: 800.0,
        plunge_feed: 200.0,
        climb: true,
        strategy: PocketStrategy::Waterline,
        entry: Entry3D::Plunge,
        helix_radius: 1.0,
        optimal_load: camcraft_app::load::DEFAULT_OPTIMAL_LOAD,
        blend_turns: false,
        tilt: None,
        links,
    };
    let _ = roundtrip(pocket2d(Links::PLANAR), None);
    let picked = Links {
        transition: Some(Entry3D::Helix),
        helix_pitch: 0.75,
        level: Link::KeepDown,
        pass: Link::KeepDown,
        max_stay_down: 12.0,
        ..Links::PLANAR
    };
    let back = roundtrip(pocket2d(picked), Some("\"level\": \"keep_down\""));
    assert_eq!(back.links(), Some(&picked));
    // The 3-D ladder's default is its own (pass links keep down): no key.
    let _ = roundtrip(pocket_op(Entry3D::Plunge, Links::ROUGH3D), None);
    let p3 = Links { pass: Link::Retract, ..Links::ROUGH3D };
    let back = roundtrip(pocket_op(Entry3D::Plunge, p3), Some("\"pass\": \"retract\""));
    assert_eq!(back.links(), Some(&p3));
    // The contour's new choices ride the `links` object; the old stay-down
    // ramp still writes the old keys (the previous build reads it right).
    let c = Links { level: Link::Retract, transition: Some(Entry3D::Plunge), ..Links::PLANAR };
    let back = roundtrip(contour_op(c), Some("\"links\""));
    assert_eq!(back.links(), Some(&c));
    let old = Links { level: Link::KeepDown, transition: Some(Entry3D::Ramp), ramp_angle: 4.0, ..Links::PLANAR };
    let back = roundtrip(contour_op(old), Some("\"level_link\": \"stay_down_ramp\""));
    assert_eq!(back.links(), Some(&old));
}

#[test]
fn the_stamp_tells_the_choices_apart() {
    let world = new_world();
    let mut h = History::default();
    h.execute(&world, SetStock::new(&world, StockShape::Box { dx: 60.0, dy: 60.0, dz: 20.0 }, Default::default()).expect("stock"));
    let tool = catalog().into_iter().find(|(n, _)| *n == TOOL).map(|(_, t)| t).expect("catalog");
    h.execute(&world, AddTool::new(&world, TOOL, tool).expect("tool"));
    let stamp = |l: Links| operation::current_stamp(&world, TOOL, &SrcRef::Stock, &pocket_op(Entry3D::Plunge, l), "X", None);
    let base = stamp(Links::ROUGH3D);
    assert!(base.is_some());
    for other in [
        Links { level: Link::Retract, ..Links::ROUGH3D },
        Links { pass: Link::Retract, ..Links::ROUGH3D },
        Links { transition: Some(Entry3D::Ramp), ..Links::ROUGH3D },
        Links { max_stay_down: 3.0, ..Links::ROUGH3D },
    ] {
        assert_ne!(stamp(other), base, "{other:?}");
    }
}

/// Found while building M11.2 (pre-existing, not a links defect): the
/// 3-D ladder run to the STOCK bottom on this part found a sliver level
/// at Z -7 under the part's underside (Z -5) - the plane sections no
/// part there, so the old islands law read it as open - and a helix
/// entry there orbited from Z 0 down through the bore's solid floor (the
/// depth-aware verifier convicted it, so it never posted silently). The
/// islands law now subtracts all the material standing above a level
/// (`machining3d::material_above`): under the closed floor no level
/// exists, so no move of any entry arm goes below the bore floor inside
/// the bore - nor anywhere under the part - and the path verifies clean.
#[test]
fn a_helix_under_the_bore_floor_is_never_silent() {
    for entry in [Entry3D::Helix, Entry3D::Plunge, Entry3D::Ramp, Entry3D::Arc] {
        let mut op = pocket_op(entry, Links::ROUGH3D);
        if let OpParams::Pocket3D { bottom_z, .. } = &mut op {
            *bottom_z = Height::of(HeightReference::BottomOfStock, 0.0);
        }
        let (_w, _h, t, warns) = user_job(op);
        assert!(!t.moves.is_empty(), "{entry:?}: the bore is still cut");
        let in_bore = |p: &Point3| p.x.hypot(p.y) < 12.5;
        let under = t.moves.iter().find(|m| [m.from, m.to].iter().any(|q| in_bore(q) && q.z < -1e-6));
        assert!(under.is_none(), "{entry:?}: a move under the bore floor: {under:?}");
        let low = t.moves.iter().map(|m| m.to.z.min(m.from.z)).fold(f64::INFINITY, f64::min);
        assert!(low >= -1e-6, "{entry:?}: the part shadows everything below Z 0, got {low}");
        assert!(warns.is_empty(), "{entry:?}: {warns:?} / {:?}", t.notes);
    }
}

#[test]
fn nonsense_links_are_refused_at_the_door() {
    for bad in [
        Links { transition: Some(Entry3D::Ramp), ramp_angle: 0.0, ..Links::PLANAR },
        Links { helix_pitch: f64::NAN, ..Links::PLANAR },
        Links { max_stay_down: -1.0, ..Links::PLANAR },
    ] {
        assert!(!contour_op(bad).is_sane(), "{bad:?}");
        assert!(!pocket_op(Entry3D::Plunge, bad).is_sane(), "{bad:?}");
    }
}
