//! Issue #146 at the app level: a boring bar's head and shank judged on
//! its cutting FEEDS against the material standing at each instant - the
//! revolved part, and the stock less the pilot, the bores ahead and its
//! own cut so far. A body in the wall or in uncut stock is a ShankHit
//! that blocks the post; a bar riding air another bar cleared is clean.
//!
//! Gate map (M4.4): covers camcraft_app::operation::turning_feeds (the
//! feed body law), camcraft_app::rest::ahead (a bore ahead's nose, #146
//! review B1), the bore arm of generate.
//! Lane shape (M4.4): all tests here run in seconds.
//!
//! Provenance: our own fixtures, specced from the turning kernel's
//! design notes; no vendor material consulted.

mod common;

use camcraft_app::commands::{AddTool, ImportModel};
use camcraft_app::drilling::{CycleChoice, Hole};
use camcraft_app::geo2d::Point2;
use camcraft_app::operation::{Height, OpParams, OpStatus};

use common::bore::*;

#[test]
fn a_stepped_bar_whose_shank_rubs_the_wall_during_the_ride_is_convicted() {
    // #146: head Ø8, shank Ø12, nose 0.4, boring a Ø14 hole over a Ø13
    // pilot. The ride's nose centre runs at r 6.6, so the shank (read
    // as `bar_spans` says, reaching 2 past the nose) stands out to r
    // 8.6 - through the finished wall at 7 - from 20 above the nose up
    // its whole length. The rapid law cannot see it; the feeds' body
    // law (and the solid law's band reading before it) names the ride
    // moves, and the post is refused by name.
    let (world, mut history) = bore_world();
    let wide = revolve(&[(0.0, -50.0), (BLANK, -50.0), (BLANK, 50.0), (7.0, 50.0), (7.0, FLOOR), (0.0, FLOOR)]);
    history.execute(&world, ImportModel::from_mesh("wide", wide));
    let mut bar = catalog_tool("BR-1000");
    bar.diameter = 8.0;
    bar.shank_d = 12.0;
    history.execute(&world, AddTool::new(&world, "BR-0812", bar).expect("a sane bar"));
    let mut drill = catalog_tool("DR-0400");
    drill.flute_len = 70.0;
    drill.diameter = 13.0;
    drill.shank_d = 13.0;
    history.execute(&world, AddTool::new(&world, "DR-1300-L", drill).expect("a sane drill"));
    let hole = Hole { xy: Point2::new(0.0, 0.0), radius: 6.55, entry_z: 50.0, floor_z: -16.0, through: false, stepped: false };
    let drill_params = OpParams::Drill {
        break_through: None,
        clearance_z: Height::fixed(60.0),
        retract_z: None,
        top_z: Height::fixed(50.0),
        bottom_z: None,
        cycle: CycleChoice::Auto,
        peck: 0.0,
        dwell: 0.0,
        feed: 120.0,
        plunge_feed: 80.0,
        holes: vec![hole],
        cross: vec![],
    };
    create_on(&world, &mut history, "AX0", "DR-1300-L", "wide", drill_params);
    assert_eq!(status(&world, "AX0"), OpStatus::Ok, "the pilot drills");
    create_on(&world, &mut history, "BR1", "BR-0812", "wide", bore_params());
    let c = cache(&world, "BR1").expect("cached");
    let ride: Vec<usize> = (0..c.path.moves.len())
        .filter(|&i| c.path.moves[i].kind != camcraft_app::cl::MoveKind::Rapid && c.path.moves[i].to.z > 0.0)
        .collect();
    assert!(!ride.is_empty(), "the ride climbs the wall");
    let hit = c.warnings.iter().find_map(|w| match *w {
        camcraft_app::verify::Warning::ShankHit { at, part: true } if ride.contains(&at) => Some(at),
        _ => None,
    });
    let Some(at) = hit else { panic!("the shank rode through the wall unnamed: {:?}", c.warnings) };
    let out = std::env::temp_dir().join(format!("camcraft-bore-146-{}.nc", std::process::id()));
    let _ = std::fs::remove_file(&out);
    match camcraft_app::posting::post_setup(&world, &out) {
        Ok(_) => panic!("a bar rubbing its shank on the wall posted"),
        Err(e) => assert!(e.contains("post blocked") && e.contains(&format!("move {at} ")), "refused, but not by the body law: {e}"),
    }
    assert!(!out.exists(), "a refused post writes nothing");
}

#[test]
fn a_bar_whose_head_rides_the_uncut_band_above_its_nose_is_convicted() {
    // #146: the catalog Ø10 bar (nose 0.4) bores the Ø12 hole over a
    // Ø11 pilot. Its rapids fit (centred, the body spans |r| <= 5.2),
    // but on the ascending ride the nose centre runs at r 5.6 and the
    // head's edge with it - above the nose, where the band between the
    // pilot's 5.5 and the wall is stock the ride has not reached yet.
    // The part is never touched: only the stock reading convicts.
    let (world, mut history) = bore_world();
    let mut drill = catalog_tool("DR-0400");
    drill.flute_len = 70.0;
    drill.diameter = 10.6;
    drill.shank_d = 10.6;
    history.execute(&world, AddTool::new(&world, "DR-1060", drill).expect("a sane drill"));
    pilot_of(&world, &mut history, "DR-1060", 5.3);
    create(&world, &mut history, "BR1", bore_params());
    let c = cache(&world, "BR1").expect("cached");
    let hit = c.warnings.iter().find_map(|w| match *w {
        camcraft_app::verify::Warning::ShankHit { at, part: false } if c.path.moves[at].kind != camcraft_app::cl::MoveKind::Rapid => Some(at),
        _ => None,
    });
    let Some(at) = hit else { panic!("the head rode the uncut band unnamed: {:?}", c.warnings) };
    assert!(!c.warnings.contains(&camcraft_app::verify::Warning::RapidInStock { at: 0 }), "the approach fits the pilot: {:?}", c.warnings);
    let out = std::env::temp_dir().join(format!("camcraft-bore-146-band-{}.nc", std::process::id()));
    let _ = std::fs::remove_file(&out);
    match camcraft_app::posting::post_setup(&world, &out) {
        Ok(_) => panic!("a bar riding its head through uncut stock posted"),
        Err(e) => assert!(e.contains("post blocked") && e.contains(&format!("shank/holder at move {at} ")), "refused, but not by the body law: {e}"),
    }
    assert!(!out.exists(), "a refused post writes nothing");
}

#[test]
fn a_finishing_bar_riding_where_a_rougher_bar_cut_is_clean() {
    // #146 review B1: a Ø12 hole over a Ø10.9 pilot, bored by BR1 (the
    // Ø10 bar with a 0.8 nose: centre r ~5.2) and then BR2 (the catalog
    // 0.4 nose: centre r ~5.5). BR2's head edge rides above its nose
    // past the pilot's 5.45 - but BR1's nose swept out to the wall, so
    // that band is air: the bore ahead is credited with its whole nose,
    // not its centre line. Both windows run to the stock's top, so no
    // band stands above either ride.
    let (world, mut history) = bore_world();
    let mut drill = catalog_tool("DR-0400");
    drill.flute_len = 70.0;
    drill.diameter = 10.9;
    drill.shank_d = 10.9;
    history.execute(&world, AddTool::new(&world, "DR-1090", drill).expect("a sane drill"));
    pilot_of(&world, &mut history, "DR-1090", 5.45);
    let mut rough = catalog_tool("BR-1000");
    rough.nose_radius = 0.8;
    history.execute(&world, AddTool::new(&world, "BR-1008", rough).expect("a sane bar"));
    let full = || match bore_params() {
        OpParams::BoringTurning { tolerance, clearance_z, bottom_z, feed, plunge_feed, .. } => {
            OpParams::BoringTurning { tolerance, clearance_z, top_z: Height::fixed(50.0), bottom_z, feed, plunge_feed }
        }
        p => p,
    };
    create_on(&world, &mut history, "BR1", "BR-1008", "bar", full());
    let c1 = cache(&world, "BR1").expect("cached");
    assert!(c1.warnings.is_empty(), "the rougher bar rode clean: {:?}", c1.warnings);
    create_on(&world, &mut history, "BR2", "BR-1000", "bar", full());
    let c2 = cache(&world, "BR2").expect("cached");
    assert!(c2.warnings.is_empty(), "the finishing bar rides the rougher's air: {:?}", c2.warnings);
    assert_eq!(status(&world, "BR2"), OpStatus::Ok);
}
