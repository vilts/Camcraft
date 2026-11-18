//! The links choices off the generators (M11.2): the project file writes
//! the `links` key only when the choice differs from the kind's earlier
//! shape and round-trips it byte for byte; the stamp tells the choices
//! apart; nonsense links are refused at the door.

use super::*;

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
        rest: None,
        blend_radius: None,
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
    let back = roundtrip(pocket2d(Links::PLANAR), None);
    assert!(matches!(back, OpParams::Pocket { rest: None, .. }), "default pocket stays non-rest after loading");
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
    assert!(matches!(back, OpParams::Pocket { rest: None, .. }), "picked links preserve the non-rest pocket");
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
