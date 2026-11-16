//! #1370: the Wire Contour's stamp digests every stored word. An open
//! run's side (left / right of travel, #1109) and its run-out clearance
//! each move the stamp, so an edited open run reads Stale and its old
//! path never posts as the new cut; a closed die / punch cut never reads
//! the clearance, so its stamp does not move with it. An unrelated edit
//! and a save / load round trip leave a generated open run Ok.
//!
//! #537 (M3's review of PR #1181, items 2 and 3): the 4-axis block's
//! pairing rule, every sync point's either side and its tolerance each
//! move the stamp, and so does the machine's maximum taper; a one-profile
//! cut's stamp is pinned to the value main's arm gives.
//!
//! #539: a tabbed cut's stamp digests every tab word and the cut-off
//! condition's id, wire and gap, so an edited tab never posts a stale
//! path.
//!
//! Gate map (#1370, #537, #539): covers app::operation (params' stamp_with
//! wire arm, wire_ruled::digest, stamp, wire_op::stamp_condition),
//! app::commands (operation edit and generate), app::project
//! (wire_edm_dto save / load).
//! Provenance: our own tests over our own synthetic fixture (the
//! invented wire-iso machine of #533, #290 D10).

use flecs_ecs::prelude::World;

use camcraft_app::commands::{CreateOperation, EditOperationParams, GenerateOperation, History, ImportModel, LoadMachine, SetStock};
use camcraft_app::geometry::{feature, Mesh, Point3, Triangle};
use camcraft_app::machine::parse_mch;
use camcraft_app::model;
use camcraft_app::operation::{
    self, FeatureSel, GenStamp, GeoSel, Height, OpParams, OpStatus, SrcRef, WireContourOp, WireFourAxis, WirePairingRule,
    WireSide, WireSyncPair, WireTabs,
};
use camcraft_app::project;
use camcraft_app::schema::Location;
use camcraft_app::stock::StockShape;
use camcraft_app::world::new_world;

const FIXTURE: &str = include_str!("edm/wire-iso-fixture.mch");

/// The CCW outline `pts` extruded from z = 0 to 12.
fn prism(pts: &[(f64, f64)]) -> Mesh {
    let p = |(x, y): (f64, f64), z: f64| Point3::new(x, y, z);
    let mut f = Vec::new();
    for i in 0..pts.len() {
        let (a, b) = (pts[i], pts[(i + 1) % pts.len()]);
        f.push(Triangle::new(Point3::new(0.0, 0.0, 12.0), p(a, 12.0), p(b, 12.0)));
        f.push(Triangle::new(Point3::new(0.0, 0.0, 0.0), p(b, 0.0), p(a, 0.0)));
        f.push(Triangle::new(p(a, 0.0), p(b, 0.0), p(b, 12.0)));
        f.push(Triangle::new(p(a, 0.0), p(b, 12.0), p(a, 12.0)));
    }
    Mesh::from_facets(f)
}

fn wire_op(side: WireSide, start: Option<[f64; 2]>) -> WireContourOp {
    WireContourOp {
        top_z: Height::fixed(12.0),
        bottom_z: Height::fixed(0.0),
        condition: "C110".into(),
        side,
        start,
        tabs: None,
        capture: false,
        taper_deg: 0.0,
        four_axis: None,
        burnout: None,
        clearance: operation::DEFAULT_CLEARANCE,
        skims: Vec::new(),
    }
}

fn status(world: &World, name: &str) -> OpStatus {
    operation::status_of(world, &model::find_by_name(world, name).expect("op"))
}

fn params(world: &World, name: &str) -> WireContourOp {
    let op = model::find_by_name(world, name).and_then(|e| model::operation_of(world, &e)).expect("op");
    let OpParams::WireContour(w) = op.params else { panic!("not a wire contour") };
    w
}

fn edit(world: &World, h: &mut History, name: &str, op: WireContourOp) {
    h.execute(world, EditOperationParams::new(world, name, OpParams::WireContour(op)).expect("edit"));
}

/// The wire-iso fixture mounted; a 60 x 50 x 12 part at the left end of
/// a 100 x 50 x 12 bar, cut off along its x = 60 edge by "Cutoff1"
/// (generated, kerf on the offcut's side), and a punch "Punch1" around
/// the same part, generated.
fn doc() -> (World, History) {
    let world = new_world();
    let mut h = History::default();
    let m = parse_mch(FIXTURE).expect("the fixture parses");
    h.execute(&world, LoadMachine::new(&world, "wire-iso.mch".into(), Some(m)).expect("mount"));
    h.execute(&world, ImportModel::from_mesh("part", prism(&[(0.0, 0.0), (60.0, 0.0), (60.0, 50.0), (0.0, 50.0)])));
    let loc = Location { pos: Point3::new(50.0, 25.0, 0.0), ..Location::default() };
    h.execute(&world, SetStock::new(&world, StockShape::Box { dx: 100.0, dy: 50.0, dz: 12.0 }, loc).expect("stock"));
    let e = model::find_by_name(&world, "part").expect("model");
    let f = model::features_of(&world, &e).expect("features");
    let (chain, up) = feature::chains(&f)
        .into_iter()
        .filter(|c| !c.closed)
        .find_map(|c| {
            let p = feature::chain_points(&f, &c);
            let on = p.len() >= 2 && p.iter().all(|q| (q[0] - 60.0).abs() < 1e-9 && (q[2] - 12.0).abs() < 1e-9);
            on.then(|| (c.id, p[p.len() - 1][1] > p[0][1]))
        })
        .expect("the part's x = 60 top edge is its own open chain");
    // Travelling +Y the offcut (x > 60) is on the right.
    let side = if up { WireSide::Right } else { WireSide::Left };
    let src = SrcRef::Selection(vec![GeoSel { entity: "part".into(), feature: FeatureSel::Chain(chain) }]);
    let c = CreateOperation::new(&world, "Cutoff1", "", src, OpParams::WireContour(wire_op(side, None))).expect("open op");
    h.execute(&world, c);
    h.execute(&world, GenerateOperation::new(&world, "Cutoff1").expect("the open run generates"));
    let punch = OpParams::WireContour(wire_op(WireSide::Punch, Some([-10.0, 25.0])));
    let c = CreateOperation::new(&world, "Punch1", "", SrcRef::Entity("part".into()), punch).expect("punch op");
    h.execute(&world, c);
    h.execute(&world, GenerateOperation::new(&world, "Punch1").expect("the punch generates"));
    (world, h)
}

fn stamp_of(world: &World, op: WireContourOp) -> Option<GenStamp> {
    operation::current_stamp(world, "", &SrcRef::Entity("part".into()), &OpParams::WireContour(op), "", None)
}

/// Rewrites every wire contour in `v` whose side is one of `sides`.
fn patch_wire(v: &mut serde_json::Value, sides: &[&str], f: &dyn Fn(&mut serde_json::Map<String, serde_json::Value>)) {
    match v {
        serde_json::Value::Object(m) => {
            if let Some(serde_json::Value::Object(w)) = m.get_mut("wire_contour")
                && w.get("side").and_then(serde_json::Value::as_str).is_some_and(|s| sides.contains(&s))
            {
                f(w);
            }
            m.values_mut().for_each(|c| patch_wire(c, sides, f));
        }
        serde_json::Value::Array(a) => a.iter_mut().for_each(|c| patch_wire(c, sides, f)),
        _ => {}
    }
}

/// Saves `world`, rewrites its wire contours through `patch_wire`, and
/// opens the result in a new world - the generated paths and the stamps
/// they were taken for ride along unchanged.
fn reopened(world: &World, tag: &str, sides: &[&str], f: &dyn Fn(&mut serde_json::Map<String, serde_json::Value>)) -> World {
    let file = std::env::temp_dir().join(format!("camcraft-1370-{tag}-{}.ccam", std::process::id()));
    project::save(world, &file).expect("save");
    let mut v: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&file).expect("read")).expect("json");
    patch_wire(&mut v, sides, f);
    std::fs::write(&file, serde_json::to_string_pretty(&v).expect("text")).expect("write");
    let back = new_world();
    let loaded = project::load(&back, &file);
    let _ = std::fs::remove_file(&file);
    loaded.expect("load");
    back
}

/// Done means (#1370): an open run whose side is turned left <-> right,
/// or whose run-out clearance is re-stated, with its old path and stamp
/// kept (a file edited by hand or by a script; an edit command drops the
/// stamp on its own), reads Stale - the old path is not this cut and
/// never posts as it. Before #1370 die, left and right digested alike
/// and the clearance not at all, so both read Ok.
#[test]
fn an_open_runs_side_and_clearance_each_mark_its_path_stale() {
    let (world, mut h) = doc();
    assert_eq!(status(&world, "Cutoff1"), OpStatus::Ok);
    let p0 = params(&world, "Cutoff1");
    let flip = |w: &mut serde_json::Map<String, serde_json::Value>| {
        let to = if w.get("side").and_then(serde_json::Value::as_str) == Some("left") { "right" } else { "left" };
        w.insert("side".into(), to.into());
    };
    let back = reopened(&world, "side", &["left", "right"], &flip);
    assert_ne!(params(&back, "Cutoff1").side, p0.side, "the file's side was turned");
    assert_eq!(status(&back, "Cutoff1"), OpStatus::Stale, "the kerf changed sides: the path is not this cut");
    let back = reopened(&world, "clear", &["left", "right"], &|w| {
        w.insert("clearance".into(), 2.5.into());
    });
    assert!((params(&back, "Cutoff1").clearance - 2.5).abs() < 1e-12);
    assert_eq!(status(&back, "Cutoff1"), OpStatus::Stale, "a new run-out clearance is a different program");
    // Through the commands: the edit reads Stale, undo brings the path back.
    let flipped = if p0.side == WireSide::Left { WireSide::Right } else { WireSide::Left };
    edit(&world, &mut h, "Cutoff1", WireContourOp { side: flipped, ..p0.clone() });
    assert_eq!(status(&world, "Cutoff1"), OpStatus::Stale);
    assert!(h.undo(&world));
    assert_eq!(status(&world, "Cutoff1"), OpStatus::Ok);
}

/// All four sides digest apart; a closed die / punch cut never reads the
/// clearance, so its stamp does not move with it (the closed stamps keep
/// the bytes they wrote before #1370).
#[test]
fn the_four_sides_stamp_apart_and_a_closed_cut_ignores_the_clearance() {
    let (world, _h) = doc();
    let sides = [WireSide::Die, WireSide::Punch, WireSide::Left, WireSide::Right];
    let stamps: Vec<_> = sides.iter().map(|&s| stamp_of(&world, wire_op(s, Some([-10.0, 25.0])))).collect();
    assert!(stamps.iter().all(Option::is_some), "{stamps:?}");
    for i in 0..stamps.len() {
        for j in i + 1..stamps.len() {
            assert_ne!(stamps[i], stamps[j], "{:?} and {:?} stamp alike", sides[i], sides[j]);
        }
    }
    for s in [WireSide::Die, WireSide::Punch] {
        let a = stamp_of(&world, wire_op(s, Some([-10.0, 25.0])));
        let b = stamp_of(&world, WireContourOp { clearance: 3.0, ..wire_op(s, Some([-10.0, 25.0])) });
        assert_eq!(a, b, "{s:?} reads no clearance");
    }
    for s in [WireSide::Left, WireSide::Right] {
        let a = stamp_of(&world, wire_op(s, None));
        let b = stamp_of(&world, WireContourOp { clearance: 3.0, ..wire_op(s, None) });
        assert_ne!(a, b, "{s:?} reads its clearance");
    }
}

/// An edit of another operation, a closed cut's clearance re-stated in
/// its file, and a plain save / load round trip leave the cuts Ok.
#[test]
fn an_unrelated_edit_and_a_round_trip_leave_the_cuts_ok() {
    let (world, mut h) = doc();
    let punch = params(&world, "Punch1");
    edit(&world, &mut h, "Punch1", WireContourOp { start: Some([-12.0, 25.0]), ..punch });
    assert_eq!(status(&world, "Punch1"), OpStatus::Stale, "the punch's own edit is its own");
    assert_eq!(status(&world, "Cutoff1"), OpStatus::Ok, "another op's edit never touches the open run");
    assert!(h.undo(&world));
    assert_eq!(status(&world, "Punch1"), OpStatus::Ok);
    let back = reopened(&world, "punch", &["punch"], &|w| {
        w.insert("clearance".into(), 4.0.into());
    });
    assert!((params(&back, "Punch1").clearance - 4.0).abs() < 1e-12);
    assert_eq!(status(&back, "Punch1"), OpStatus::Ok, "a closed cut reads no clearance");
    let back = reopened(&world, "plain", &[], &|_| {});
    assert_eq!(params(&back, "Cutoff1"), params(&world, "Cutoff1"));
    assert_eq!(status(&back, "Cutoff1"), OpStatus::Ok, "the loaded open run is the same cut");
    assert_eq!(status(&back, "Punch1"), OpStatus::Ok);
}

type Pt = Option<[f64; 2]>;

fn four(pairing: WirePairingRule, sync: &[(Pt, Pt)], tolerance: f64) -> WireContourOp {
    let sync = sync.iter().map(|&(upper, lower)| WireSyncPair { upper, lower }).collect();
    WireContourOp { four_axis: Some(WireFourAxis { pairing, sync, tolerance }), ..wire_op(WireSide::Punch, Some([-10.0, 25.0])) }
}

/// The punch's stamp with no 4-axis block, as main's wire arm (before
/// #537) computes it for this document. Every byte the 4-axis cut adds
/// is written only when the block is stated, so this value does not
/// move; if a toolchain changes `DefaultHasher`, re-pin it only after
/// checking the arm still writes nothing for `four_axis: None`.
const ONE_PROFILE_STAMP: u64 = 16_116_910_563_891_173_640;

/// Done means (M3, PR #1181 item 2): the pairing rule, each sync point
/// on each side of each pair, and the tolerance each move the stamp, so
/// an edited 4-axis cut never posts its old path; a one-profile cut's
/// stamp is byte for byte main's.
#[test]
fn the_4_axis_block_moves_the_stamp_and_a_one_profile_cut_keeps_its_own() {
    let (world, _h) = doc();
    let (a, b, c, d) = (Some([1.0, 2.0]), Some([3.0, 4.0]), Some([5.0, 6.0]), Some([7.0, 8.0]));
    let base = stamp_of(&world, four(WirePairingRule::SyncPoints, &[(a, b), (c, d)], 0.05));
    let moved = Some([9.0, 9.0]);
    let edits = [
        ("pairing rule", four(WirePairingRule::ArcLength, &[(a, b), (c, d)], 0.05)),
        ("pair 1 upper", four(WirePairingRule::SyncPoints, &[(moved, b), (c, d)], 0.05)),
        ("pair 1 lower", four(WirePairingRule::SyncPoints, &[(a, moved), (c, d)], 0.05)),
        ("pair 2 upper", four(WirePairingRule::SyncPoints, &[(a, b), (moved, d)], 0.05)),
        ("pair 2 lower", four(WirePairingRule::SyncPoints, &[(a, b), (c, moved)], 0.05)),
        ("pair 1 upper unstated", four(WirePairingRule::SyncPoints, &[(None, b), (c, d)], 0.05)),
        ("pair 2 lower unstated", four(WirePairingRule::SyncPoints, &[(a, b), (c, None)], 0.05)),
        ("a pair dropped", four(WirePairingRule::SyncPoints, &[(a, b)], 0.05)),
        ("tolerance", four(WirePairingRule::SyncPoints, &[(a, b), (c, d)], 0.08)),
    ];
    assert!(base.is_some());
    for (what, op) in edits {
        assert_ne!(stamp_of(&world, op), base, "the {what} moves the stamp");
    }
    let plain = stamp_of(&world, wire_op(WireSide::Punch, Some([-10.0, 25.0])));
    assert_ne!(plain, base, "the 4-axis block is digested at all");
    assert_eq!(plain, Some(GenStamp(ONE_PROFILE_STAMP)), "a one-profile cut's stamp is main's: {plain:?}");
}

/// Done means (M3, PR #1181 item 3): the machine re-stating its maximum
/// taper re-stamps a 4-axis cut - it reads the maximum as a taper does -
/// and leaves a vertical cut's stamp alone.
#[test]
fn the_machines_maximum_taper_moves_a_4_axis_cuts_stamp() {
    let (world, mut h) = doc();
    let cut = four(WirePairingRule::ArcLength, &[], 0.05);
    let plain = wire_op(WireSide::Punch, Some([-10.0, 25.0]));
    let (before, plain_before) = (stamp_of(&world, cut.clone()), stamp_of(&world, plain.clone()));
    let m = parse_mch(&FIXTURE.replace("\"max_taper_deg\": 15.0,", "\"max_taper_deg\": 5.0,")).expect("parses");
    h.execute(&world, LoadMachine::new(&world, "wire-iso.mch".into(), Some(m)).expect("re-mount"));
    assert!(before.is_some());
    assert_ne!(stamp_of(&world, cut), before, "a 4-axis cut reads the machine's maximum taper");
    assert_eq!(stamp_of(&world, plain), plain_before, "a vertical cut never reads it");
    assert_eq!(status(&world, "Punch1"), OpStatus::Ok, "the generated vertical punch stays Ok");
}

/// The fixture plus the tab cut-off's SYNTHETIC conditions: C120 of
/// `wire` and `gap`, and C130 always of the main cut's 0.25 / 0.04.
fn tabbed_world(wire: f64, gap: f64) -> World {
    let mut v: serde_json::Value = serde_json::from_str(FIXTURE).expect("fixture json");
    let conds = v["edm"]["conditions"].as_array_mut().expect("conditions");
    conds.push(serde_json::json!({ "id": "C120", "name": "synthetic cut-off", "gap": gap, "wire_diameter": wire, "role": "rough", "notes": "SYNTHETIC" }));
    conds.push(serde_json::json!({ "id": "C130", "name": "synthetic cut-off b", "gap": 0.04, "wire_diameter": 0.25, "role": "rough", "notes": "SYNTHETIC" }));
    let world = new_world();
    let mut h = History::default();
    let m = parse_mch(&v.to_string()).expect("the fixture parses");
    h.execute(&world, LoadMachine::new(&world, "wire-iso.mch".into(), Some(m)).expect("mount"));
    h.execute(&world, ImportModel::from_mesh("part", prism(&[(0.0, 0.0), (60.0, 0.0), (60.0, 50.0), (0.0, 50.0)])));
    world
}

fn tabbed(t: WireTabs) -> WireContourOp {
    WireContourOp { tabs: Some(t), ..wire_op(WireSide::Punch, Some([-10.0, 25.0])) }
}

fn two_tabs() -> WireTabs {
    WireTabs { count: Some(2), width: Some(1.0), at: vec![[0.0, 25.0], [60.0, 25.0]], cutoff_condition: "C120".into() }
}

/// Done means (#539 review): each tab word alone - the count, the width,
/// a position (x or y), one more position, the cut-off condition's id
/// (C130 states the same wire and gap as C120, so only the id tells
/// them apart) - and the cut-off condition's wire or gap as the machine
/// states it, each moves the stamp; so an edited tab marks its path
/// Stale and the old one never posts as the new cut.
#[test]
fn every_tab_word_and_the_cut_off_wire_and_gap_move_the_stamp() {
    let world = tabbed_world(0.25, 0.04);
    let base = stamp_of(&world, tabbed(two_tabs()));
    assert!(base.is_some());
    assert_eq!(base, stamp_of(&world, tabbed(two_tabs())), "the stamp is a function of the cut");
    assert_ne!(base, stamp_of(&world, wire_op(WireSide::Punch, Some([-10.0, 25.0]))), "tabs off");
    let t = two_tabs;
    for (what, tabs) in [
        ("count", WireTabs { count: Some(3), ..t() }),
        ("count unset", WireTabs { count: None, ..t() }),
        ("width", WireTabs { width: Some(1.5), ..t() }),
        ("width unset", WireTabs { width: None, ..t() }),
        ("a position's x", WireTabs { at: vec![[0.5, 25.0], [60.0, 25.0]], ..t() }),
        ("a position's y", WireTabs { at: vec![[0.0, 25.0], [60.0, 24.0]], ..t() }),
        ("one more position", WireTabs { at: vec![[0.0, 25.0], [60.0, 25.0], [30.0, 0.0]], ..t() }),
        ("the cut-off condition id", WireTabs { cutoff_condition: "C130".into(), ..t() }),
    ] {
        assert_ne!(base, stamp_of(&world, tabbed(tabs)), "{what} alone must move the stamp");
    }
    let wire = tabbed_world(0.3, 0.04);
    assert_ne!(base, stamp_of(&wire, tabbed(two_tabs())), "the cut-off condition's wire alone must move the stamp");
    let gap = tabbed_world(0.25, 0.05);
    assert_ne!(base, stamp_of(&gap, tabbed(two_tabs())), "the cut-off condition's gap alone must move the stamp");
    // The main condition is untouched across the three machines.
    let plain = wire_op(WireSide::Punch, Some([-10.0, 25.0]));
    assert_eq!(stamp_of(&world, plain.clone()), stamp_of(&wire, plain.clone()));
    assert_eq!(stamp_of(&world, plain.clone()), stamp_of(&gap, plain));
}

/// The fixture with its machine's maximum taper stated as `max`, and the
/// part imported (no stock, so the two machines differ in that one word).
fn world_with_max_taper(max: f64) -> World {
    let mut v: serde_json::Value = serde_json::from_str(FIXTURE).expect("fixture json");
    v["edm"]["max_taper_deg"] = max.into();
    let world = new_world();
    let mut h = History::default();
    let m = parse_mch(&v.to_string()).expect("the fixture parses");
    h.execute(&world, LoadMachine::new(&world, "wire-iso.mch".into(), Some(m)).expect("mount"));
    h.execute(&world, ImportModel::from_mesh("part", prism(&[(0.0, 0.0), (60.0, 0.0), (60.0, 50.0), (0.0, 50.0)])));
    world
}

/// #1380: the taper alone moves the stamp. A stated taper is digested
/// under its own tag (params.rs), so a path cut at one taper reads Stale
/// at another - for a closed punch and for an open run - and the machine's
/// maximum taper moves a tapered stamp too (stamp_condition). Done means:
/// changing only `taper_deg` changes the stamp.
#[test]
fn a_taper_alone_moves_the_stamp_and_so_does_the_machine_maximum() {
    let (world, _h) = doc();
    for side in [WireSide::Punch, WireSide::Left] {
        let start = if side == WireSide::Punch { Some([-10.0, 25.0]) } else { None };
        let at = |deg: f64| stamp_of(&world, WireContourOp { taper_deg: deg, ..wire_op(side, start) });
        let (flat, two, three) = (at(0.0), at(2.0), at(3.0));
        assert!(flat.is_some() && two.is_some() && three.is_some(), "{side:?}: {flat:?} {two:?} {three:?}");
        assert_ne!(flat, two, "{side:?}: a 2 degree taper alone must move the stamp");
        assert_ne!(two, three, "{side:?}: 2 and 3 degrees must stamp apart");
    }
    let tapered = WireContourOp { taper_deg: 2.0, ..wire_op(WireSide::Punch, Some([-10.0, 25.0])) };
    let at15 = stamp_of(&world_with_max_taper(15.0), tapered.clone());
    let at9 = stamp_of(&world_with_max_taper(9.0), tapered);
    assert!(at15.is_some() && at9.is_some());
    assert_ne!(at15, at9, "the machine's maximum taper alone must move a tapered stamp");
}
