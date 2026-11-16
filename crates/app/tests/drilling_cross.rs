//! M5.2 stage F2 cross-drilling tests at the app level: the
//! document's side of the radial law. What "right" means here: a
//! drill op wearing typed cross holes on a mill-turn document
//! generates Ok with every XY-only verify law silent (Mode::Radial
//! stands them down - the kernel's construction law judges this
//! chain; its rapids' solid reading against the raw stock, #79, is
//! `drilling_cross_rapids`'s); the cached path carries the pose tags
//! tip-toward-shank and its tips reach the promised depths; the station change
//! traverses outside the stock's silhouette; the three refusals
//! (mixed lists, a plain mill's turret, a bottom plane on radial
//! holes) name themselves; hole recognition over the drilled bar's
//! mesh feeds the same generator through `cross_holes_of`; and the
//! list rides the project file - a pure-Z document never grows the
//! `cross` key. The kernel's own laws are pinned one crate down
//! (camcraft-motion's drilling cross suite); this file is the
//! document.
//!
//! Gate map (M4.4): covers camcraft_app::operation (the cross arm of
//! generate, the region seam for posed plunges, cross_holes_of),
//! camcraft_app::commands (Create/GenerateOperation,
//! EditOperationParams undo), camcraft_app::machine, post::machine (the turret
//! door), camcraft_app::project (the v40 `cross` grammar).
//! Lane shape (M4.4): all tests here run in seconds.

mod cross_support;

use cross_support::*;

// ---- the document's life ----

#[test]
fn a_cross_drill_op_runs_clean_in_the_document() {
    // Typed blind + through holes on a mill-turn: Ok, silent, and
    // the cached path is the kernel's signature from the world's
    // side - every pose tag states tip-toward-shank (+X for an
    // inward -X plunge), the chain is closed, no Z cycles (the
    // chain IS the radial truth), and the tips reach the promised
    // depths: blind floor x = 5, through bottom x = -16 (floor
    // -15 minus the exit allowance). The raw stock is the bar's own
    // envelope: on the 44 mm box the kernel's part-measured approach
    // stands inside the raw stock (#77), which the rapid law now
    // convicts (#79) - see drilling_cross_rapids.
    let (world, mut history) = bar_stock_world("mill-turn");
    create(&world, &mut history, "CD1", cross_params(typed(), None));
    let c = cache(&world, "CD1").expect("cached");
    assert_eq!(status(&world, "CD1"), OpStatus::Ok, "the radial rapids read clean: {:?}", c.warnings);
    assert!(c.warnings.is_empty(), "a clean radial chain reported: {:?}", c.warnings);
    let t = c.path;
    assert!(t.is_chained() && t.all_finite());
    assert!(t.cycles.is_empty(), "radial work carries no Z cycle list");
    assert!(
        t.moves.iter().all(|m| m.axis == Some([1.0, 0.0, 0.0])),
        "every pose is the -X plunge's tip->shank: +X"
    );
    let deepest_through = t
        .moves
        .iter()
        .filter(|m| m.kind != MoveKind::Rapid && m.to.z < 0.0)
        .map(|m| m.to.x)
        .fold(f64::MAX, f64::min);
    assert!((deepest_through - -16.0).abs() < 1e-9, "through tip at x = {deepest_through}");
    let blind_floor = t
        .moves
        .iter()
        .filter(|m| m.kind != MoveKind::Rapid && m.to.z > 0.0)
        .map(|m| m.to.x)
        .fold(f64::MAX, f64::min);
    assert!((blind_floor - 5.0).abs() < 1e-9, "blind floor at x = {blind_floor}");
}

#[test]
fn the_station_change_traverses_outside_the_silhouette() {
    // Two holes on different stations (X and Y): every rapid either
    // rides a hole's own axis (the kerf's air) or stays outside the
    // silhouette radius - the chord law the kernel proved, read
    // back off the document's cached path.
    let (world, mut history) = cross_world("mill-turn");
    let holes = vec![
        cross(Point3::new(15.0, 0.0, 10.0), nx(), 2.0, -8.0, false),
        cross(Point3::new(0.0, 15.0, 10.0), ny(), 2.0, -8.0, false),
    ];
    create(&world, &mut history, "CD1", cross_params(holes, None));
    let c = cache(&world, "CD1").expect("cached");
    assert_eq!(status(&world, "CD1"), OpStatus::Ok, "{:?}", c.warnings);
    let r = stock_r();
    let t = c.path;
    for (i, m) in t.moves.iter().enumerate() {
        if m.kind != MoveKind::Rapid {
            continue;
        }
        let axial = (m.from.y.abs() < 1e-9 && m.to.y.abs() < 1e-9)
            || (m.from.x.abs() < 1e-9 && m.to.x.abs() < 1e-9);
        if axial {
            continue; // the hole's own kerf
        }
        for f in [0.0, 0.5, 1.0] {
            let p = Point3::new(
                m.from.x + (m.to.x - m.from.x) * f,
                m.from.y + (m.to.y - m.from.y) * f,
                m.from.z + (m.to.z - m.from.z) * f,
            );
            let rho = (p.x * p.x + p.y * p.y).sqrt();
            assert!(rho >= r - 1e-9, "rapid {i} clips the blank at ({}, {})", p.x, p.y);
        }
    }
}

// ---- the refusals, each by name ----

fn op(params: OpParams) -> Operation {
    Operation {
        tool: "DR-0400".into(),
        src: SrcRef::Entity("bar".into()),
        params,
        stamp: None,
        setup: None,
        area: Default::default(),
        coolant: Default::default(),
        after: Default::default(),
        suppressed: false,
    }
}

#[test]
fn a_mixed_list_refuses_by_name() {
    let (world, _h) = cross_world("mill-turn");
    let mut p = cross_params(typed(), None);
    if let OpParams::Drill { holes, .. } = &mut p {
        holes.push(camcraft_app::drilling::Hole {
            xy: Point2::new(0.0, 0.0),
            radius: 2.0,
            entry_z: 15.0,
            floor_z: 0.0,
            through: false,
            stepped: false,
        });
    }
    let e = operation::generate(&world, &op(p), "").expect_err("a mixed list cannot run");
    assert!(format!("{e}").contains("not both"), "{e}");
}

#[test]
fn a_plain_mill_has_no_turret() {
    let (world, _h) = cross_world("mill");
    let e = operation::generate(&world, &op(cross_params(typed(), None)), "")
        .expect_err("cross holes ask for live tooling");
    assert!(format!("{e}").contains("live-tooling"), "{e}");
}

#[test]
fn a_bottom_plane_cannot_drive_radial_holes() {
    let (world, _h) = cross_world("mill-turn");
    let e = operation::generate(&world, &op(cross_params(typed(), Some(Height::fixed(0.0)))), "")
        .expect_err("the depth rides each hole, not a plane");
    assert!(format!("{e}").contains("cannot drive radial holes"), "{e}");
}

// ---- recognition, round trip, staleness ----

#[test]
fn recognition_finds_the_bar_two_holes_and_they_generate() {
    // The document's own detector, over the imported mesh: exactly
    // the two modeled bores - a blind floor at x = 5 and a through
    // that spans the bar - and the chain they drive comes back Ok.
    let (world, mut history) = cross_world("mill-turn");
    let found =
        operation::cross_holes_of(&world, None, &SrcRef::Entity("bar".into())).expect("recognized");
    assert_eq!(found.len(), 2, "{found:?}");
    let blind = found.iter().find(|h| h.origin.z > 0.0).expect("the blind one");
    assert!((blind.floor_d - -10.0).abs() < 0.5 && !blind.through, "{blind:?}");
    let thru = found.iter().find(|h| h.origin.z < 0.0).expect("the through one");
    assert!(thru.through, "{thru:?}");
    create(&world, &mut history, "CD1", cross_params(found, None));
    let c = cache(&world, "CD1").expect("cached");
    assert_eq!(status(&world, "CD1"), OpStatus::Ok, "{:?}", c.warnings);
}

fn tmp_path(tag: &str) -> PathBuf {
    std::env::temp_dir().join(format!("camcraft_cross_{tag}_{}.camproj", std::process::id()))
}

#[test]
fn the_cross_list_rides_the_project_file() {
    // Save/load round trip: the list comes back exact (directions
    // NOT latticed - a unit vector survives to the bit) and the op
    // regenerates Ok in the loaded document. A pure-Z drill file
    // never grows the `cross` key at all - the v40 default.
    let (world, mut history) = cross_world("mill-turn");
    create(&world, &mut history, "CD1", cross_params(typed(), None));
    let path = tmp_path("cross");
    project::save(&world, &path).expect("save");
    let text = std::fs::read_to_string(&path).expect("readable");
    assert!(text.contains("\"cross\""), "the radial list is in the file");
    let world2 = new_world();
    project::load(&world2, &path).expect("load");
    let e = model::find_by_name(&world2, "CD1").expect("op entity");
    let op = model::operation_of(&world2, &e).expect("the op is in the file");
    let OpParams::Drill { cross, .. } = &op.params else { panic!("drill params") };
    assert_eq!(cross, &typed(), "the list is exact through the file");
    history.execute(&world2, GenerateOperation::new(&world2, "CD1").expect("regenerate"));
    assert_eq!(status(&world2, "CD1"), OpStatus::Ok, "and it runs again");
    std::fs::remove_file(&path).ok();
}

#[test]
fn a_z_only_drill_file_never_writes_the_cross_key() {
    // The v39 -> v40 step-up promises: absent means empty means
    // exactly the old document.
    let (world, mut history) = cross_world("mill");
    // R at the raw box's top, Z 44 (issue #91: an R plane under the
    // stock top is a rapid through it, refused at generation).
    let p = OpParams::Drill {
        clearance_z: Height::fixed(50.0),
        retract_z: None,
        top_z: Height::fixed(44.0),
        bottom_z: None,
        cycle: CycleChoice::Auto,
        peck: 0.0,
        dwell: 0.0,
        feed: 120.0,
        plunge_feed: 80.0,
        holes: vec![camcraft_app::drilling::Hole {
            xy: Point2::new(0.0, 0.0),
            radius: 2.0,
            entry_z: 15.0,
            floor_z: 5.0,
            through: false,
            stepped: false,
        }],
        cross: vec![],
    };
    create(&world, &mut history, "D1", p);
    // The fixture is an honest program: it generates (R under the box
    // top would be refused, and the file would carry an Error op).
    assert_eq!(status(&world, "D1"), OpStatus::Ok, "the Z-only drill generates with R on the stock top");
    let path = tmp_path("zonly");
    project::save(&world, &path).expect("save");
    let text = std::fs::read_to_string(&path).expect("readable");
    assert!(!text.contains("\"cross\""), "a pure-Z file stays a pure-Z file");
    std::fs::remove_file(&path).ok();
}

#[test]
fn editing_the_cross_list_ages_the_cache_and_undo_restores() {
    // Everything-is-a-command: deepening the blind floor marks the
    // cached chain Stale (the digest moved), and undoing the edit
    // lands the old list exactly - Ok once regenerated.
    let (world, mut history) = cross_world("mill-turn");
    create(&world, &mut history, "CD1", cross_params(typed(), None));
    assert_eq!(status(&world, "CD1"), OpStatus::Ok);
    let mut deeper = typed();
    deeper[0].floor_d = -12.0;
    history.execute(
        &world,
        EditOperationParams::new(&world, "CD1", cross_params(deeper, None)).expect("edit"),
    );
    assert_eq!(status(&world, "CD1"), OpStatus::Stale, "the digest moved with the floor");
    assert!(history.undo(&world), "undo the edit");
    assert_eq!(status(&world, "CD1"), OpStatus::Ok, "the cached chain is the old list's again");
}
