//! M6.2 acceptance: the ribbon's taxonomy is the registry (one
//! Section column, a derived Family, a machine-filtered section list),
//! the pins are a document setting changed through the `SetRibbonPins`
//! command (issue #143: undoable, so it counts as a document change),
//! and the project file carries them under the v42 law - absent key =
//! unpinned.
//!
//! Gate map (M4.4): covers app::operation (kinds table), app::ribbon,
//! app::commands::ribbon_pins, app::schema (RibbonPins), app::project
//! (pinned key, v42).

use camcraft_app::commands::{History, SetRibbonPins};
use camcraft_app::machine::Kind;
use camcraft_app::operation::{self, Family, KindInfo, Section, KIND_COUNT};
use camcraft_app::project;
use camcraft_app::ribbon::{pinned_of, pinned_set};
use camcraft_app::schema::RibbonPins;
use camcraft_app::world::new_world;

#[test]
fn sections_partition_the_registry() {
    // One home for every row, no row in two homes, homes are runs in
    // the table (the product order the ribbon and combo both draw on).
    let mut covered = vec![0usize; KIND_COUNT];
    for s in Section::ORDER {
        let kinds = s.kinds();
        assert!(!kinds.is_empty(), "{s:?} owns no kind");
        // Display order within a section is table order (#108 lifted
        // the contiguity law: a new kind APPENDS its id, so stored ids
        // never move, and still lists in its own section).
        for w in kinds.windows(2) {
            assert!(w[1] > w[0], "{s:?} lists out of table order");
        }
        for i in &kinds {
            covered[*i] += 1;
        }
    }
    assert!(
        covered.iter().all(|c| *c == 1),
        "sections must cover the table exactly once: {covered:?}"
    );
    for (i, k) in KindInfo::ALL.iter().enumerate() {
        assert!(k.section.kinds().contains(&i), "row {i} homeless");
    }
}

#[test]
fn family_is_derived_not_forked() {
    // The old `family` field's truth, restated as a law over `section`:
    // the turning rows (the trailing run - the M5.2 ladder, wherever
    // the middle insertions have shifted it since) are the Turning
    // section and nothing Turn-likes lives elsewhere - so
    // `KindInfo::family()` cannot drift from `Section::family()`. The
    // boundary is the table's, not a remembered number: a middle
    // insertion moves the rows, not the law (M2.4 B).
    // The family is the section's, wherever the row sits (#108: the
    // appended Probe row is a mill kind past the turning run).
    for (i, k) in KindInfo::ALL.iter().enumerate() {
        // #670: the Plasma section is the plasma family's.
        let want = match k.section {
            Section::Turning => Family::Turn,
            Section::Plasma => Family::Plasma,
            // #532: the Wire EDM section is the wire family's.
            Section::WireEdm => Family::WireEdm,
            // #541: the Sinker section is the sinker family's.
            Section::Sinker => Family::Sinker,
            _ => Family::Mill,
        };
        assert_eq!(k.family(), want, "row {i} ({}) family drifted", k.label);
    }
    for s in Section::ORDER {
        for i in s.kinds() {
            assert_eq!(
                KindInfo::ALL[i].family(),
                s.family(),
                "row {i} disagrees with its section's family"
            );
        }
    }
}

#[test]
fn shown_for_is_the_machine_filter() {
    // The hiding law in one table: a mill document shows the four
    // mill sections in ORDER, a lathe Drilling (for the axial Drill
    // the door admits since M13.5 A - re-blessed from `[Turning]`
    // when the ribbon started reading `KindInfo::admits`, M13.5 B)
    // and Turning, a mill-turn all five - and nothing else is ever
    // listed, whatever else changes.
    assert_eq!(
        operation::shown_for(Kind::Mill),
        [Section::Mill2D, Section::Mill3D, Section::Drilling, Section::MultiAxis]
    );
    assert_eq!(operation::shown_for(Kind::Lathe), [Section::Drilling, Section::Turning]);
    // A mill-turn shows every section but the plasma table's (#670):
    // it spins steel, it strikes no arc - and a plasma table shows its
    // own section alone.
    assert_eq!(
        operation::shown_for(Kind::MillTurn),
        [Section::Mill2D, Section::Mill3D, Section::Drilling, Section::MultiAxis, Section::Turning]
    );
    assert_eq!(operation::shown_for(Kind::Plasma), [Section::Plasma]);
    // The per-kind word behind the section list: a section shows
    // exactly when `kinds_for` is non-empty, and the lathe's Drilling
    // section carries the Drill kind alone.
    for machine in [Kind::Mill, Kind::Lathe, Kind::MillTurn, Kind::Plasma] {
        for s in Section::ORDER {
            let kinds = s.kinds_for(machine);
            assert_eq!(
                operation::shown_for(machine).contains(&s),
                !kinds.is_empty(),
                "{s:?} on a {machine:?}: shown iff a kind is admitted"
            );
            for i in kinds {
                assert!(KindInfo::ALL[i].admits(machine), "row {i} listed but refused");
            }
        }
    }
    let drill = KindInfo::ALL
        .iter()
        .position(|k| k.ty == operation::OpType::Drill)
        .expect("the Drill row");
    assert_eq!(Section::Drilling.kinds_for(Kind::Lathe), vec![drill]);
}

#[test]
fn pins_clean_on_write_and_strangers_on_read() {
    let world = new_world();
    // Write law: order kept, duplicates and out-of-range dropped.
    pinned_set(&world, &[0, 0, KIND_COUNT, 5, 9999, 29]);
    assert_eq!(pinned_of(&world), vec![0, 5, 29], "write must clean");
    // Read law (the theme's stranger rule in the pin's clothes): a
    // list that reaches past this build's table still reads back the
    // kinds this build can name.
    world.set(RibbonPins(vec![29, KIND_COUNT, 3]));
    assert_eq!(pinned_of(&world), vec![29, 3], "read must filter");
    // No singleton at all (a virgin world) reads as unpinned.
    let fresh = new_world();
    assert!(pinned_of(&fresh).is_empty());
}

#[test]
fn pins_ride_the_project_and_the_default_writes_no_key() {
    // v42: an unpinned document pays nothing for the field...
    let path = std::env::temp_dir().join(format!(
        "camcraft_ribbon_rt_{}.camproj",
        std::process::id()
    ));
    let world = new_world();
    project::save(&world, &path).expect("save");
    let text = std::fs::read_to_string(&path).expect("read");
    assert!(
        !text.contains("\"pinned\""),
        "the default must omit the key (the mm/units law)"
    );
    // ...a pinned one names the indices in pin order...
    pinned_set(&world, &[28, 2]);
    project::save(&world, &path).expect("resave");
    let pinned_text = std::fs::read_to_string(&path).expect("reread");
    let file: serde_json::Value = serde_json::from_str(&pinned_text).expect("json");
    assert_eq!(file["pinned"], serde_json::json!([28, 2]));
    assert_eq!(file["version"], serde_json::json!(project::PROJECT_VERSION));
    // ...and loading lands them exactly, through the apply seam (the
    // leak law: a stale singleton cannot survive an open).
    let world2 = new_world();
    pinned_set(&world2, &[7]);
    project::load(&world2, &path).expect("load");
    assert_eq!(pinned_of(&world2), vec![28, 2], "load must land, not merge");
    // Round-trip byte-shape: the reopened document saves as it came.
    project::save(&world2, &path).expect("resave2");
    let second = std::fs::read_to_string(&path).expect("read2");
    let _ = std::fs::remove_file(&path);
    assert_eq!(pinned_text, second, "save/load/save must be byte-identical");

    // The v41 shape: the key simply was not there, and the file opens.
    let world3 = new_world();
    project::open(&world3, r#"{"version": 41, "entities": []}"#).expect("v41 opens");
    assert!(pinned_of(&world3).is_empty(), "absent means unpinned");
}

#[test]
fn v42_pins_shift_for_the_twins_row_insert() {
    // M2.4 A put two rows in the MIDDLE of the 2D mill's run, so a
    // v42 file's pin indices name rows that moved: 0..=4 still name
    // the same kind, and from the Drill row up each gains the two
    // new rows' worth. The rungs CHAIN (M2.4 B proved it from the
    // other side): v42 -> v43 by the twins' rung, v43 -> v44 by the
    // trio's, v44 -> v45 by the M2.4 C pair's two-boundary step, and
    // v47 -> v48 by the Deburr seat (a tail index is above the 15
    // boundary, so it rides that +1 too) - the old Drill row lands
    // three + two + one past its old home, the last turning row one
    // rung more. The PostDto law seen from the other side - names
    // survive growth, indices need a rung.
    let world = new_world();
    project::open(
        &world,
        r#"{"version": 42, "pinned": [4, 5, 29], "entities": []}"#,
    )
    .expect("a v42 pinned file steps up");
    let pins = pinned_of(&world);
    assert_eq!(
        pins,
        vec![4, 11, 37],
        "Trace stays; the Drill row takes three rungs, the turning tail a fourth"
    );
    assert_eq!(KindInfo::ALL[pins[1]].ty, operation::OpType::Drill);
    assert_eq!(KindInfo::ALL[pins[2]].ty, operation::OpType::BoringTurning);
}

#[test]
fn v43_pins_shift_for_the_m24b_row_insert() {
    // The same law one rung closer: M2.4 B put three rows after
    // Trace (tags 7, 8, 9), so a v43 file's indices 0..=6 still name
    // their kinds and 7 up gains three. The Offset row (6) is the
    // run's last old name; the Drill row (7) is the run's first
    // shifted one. The v45 rung chains after: the landed 10 takes
    // the Thread's +1, the landed 34 the pair's +2 - and the v48
    // Deburr seat adds its +1 to everything from 15 up.
    let world = new_world();
    project::open(
        &world,
        r#"{"version": 43, "pinned": [6, 7, 31], "entities": []}"#,
    )
    .expect("a v43 pinned file steps up");
    let pins = pinned_of(&world);
    assert_eq!(pins, vec![6, 11, 37], "below the insert stays, from Drill up shifts");
    assert_eq!(KindInfo::ALL[pins[0]].ty, operation::OpType::Offset2D);
    assert_eq!(KindInfo::ALL[pins[1]].ty, operation::OpType::Drill);
    assert_eq!(KindInfo::ALL[pins[2]].ty, operation::OpType::BoringTurning);
}

#[test]
fn v44_pins_take_the_two_boundary_step() {
    // M2.4 C seats its two rows IN their sections' runs (the
    // contiguity law will not abide a tail-append: Mill2D must stay
    // a run and Turning the tail). Thread closes the Mill2D run at
    // 10, so the old Drill row (10) shifts +1; Bore joins Drilling
    // at 12, so everything from the old Pocket3D row (11) up shifts
    // +2. The rung is two boundaries, not one - and this is the
    // proof: the old Drill lands at 11 (its +1), the old Threading
    // (29) and the old last-turning (34) both land +2. The v48
    // Deburr rung chains after (+1 from 15 up): Threading lands at
    // 32, the turning tail at 37, the Drill below the seat at 11.
    let world = new_world();
    project::open(
        &world,
        r#"{"version": 44, "pinned": [4, 10, 29, 34], "entities": []}"#,
    )
    .expect("a v44 pinned file steps up");
    let pins = pinned_of(&world);
    assert_eq!(
        pins,
        vec![4, 11, 32, 37],
        "below Thread stays, Drill +1, from Pocket3D up +2, from the seat up +1 more"
    );
    assert_eq!(KindInfo::ALL[pins[0]].ty, operation::OpType::Trace);
    assert_eq!(KindInfo::ALL[pins[1]].ty, operation::OpType::Drill, "the drill took the +1");
    assert_eq!(KindInfo::ALL[pins[2]].ty, operation::OpType::Threading);
    assert_eq!(KindInfo::ALL[pins[3]].ty, operation::OpType::BoringTurning);
    // And the two new rows sit where the rungs' arithmetic says.
    assert_eq!(KindInfo::ALL[10].ty, operation::OpType::Thread, "Thread closes Mill2D");
    assert_eq!(KindInfo::ALL[12].ty, operation::OpType::Bore, "Bore joins Drilling");
}

#[test]
fn v47_pins_take_the_deburr_seat_shift() {
    // M2.5 E B seats Deburr at 15 IN the Mill3D run (the partition
    // law's contiguity, not a tail-append): below the seat the names
    // stand - Finish3D was 14 and stays 14 - and from the old 5-Axis
    // Contour row up every index slides +1. One cut, one rung: the
    // M2.4 choreography seen from the middle of the table.
    let world = new_world();
    project::open(
        &world,
        r#"{"version": 47, "pinned": [4, 14, 15, 36], "entities": []}"#,
    )
    .expect("a v47 pinned file steps up");
    let pins = pinned_of(&world);
    assert_eq!(pins, vec![4, 14, 16, 37], "below the seat stays, from Contour5D up slides");
    assert_eq!(KindInfo::ALL[pins[0]].ty, operation::OpType::Trace);
    assert_eq!(
        KindInfo::ALL[pins[1]].ty,
        operation::OpType::Finish3D,
        "the row under the seat keeps its number"
    );
    assert_eq!(
        KindInfo::ALL[pins[2]].ty,
        operation::OpType::Contour5D,
        "the old 15 lands at 16"
    );
    assert_eq!(
        KindInfo::ALL[pins[3]].ty,
        operation::OpType::BoringTurning,
        "the table's tail slides too"
    );
    // And the new row sits where the rung's arithmetic says.
    assert_eq!(
        KindInfo::ALL[15].ty,
        operation::OpType::Deburr,
        "Deburr seats in the Mill3D run"
    );
}

#[test]
fn the_probe_appends_its_id_and_lists_in_drilling() {
    // #108: stored kind ids never move - the probe takes the next id,
    // every older row keeps its own - and the menu still shows it in
    // the Drilling section, behind the hole cycles.
    assert_eq!(KindInfo::ALL.len(), KIND_COUNT);
    assert_eq!(KindInfo::ALL[38].ty, operation::OpType::Probe);
    // #670 appended the plasma profile behind it (39), #220 the trochoid
    // (40), #541 the sinker burn (42), #545 the sinker side burn (43).
    assert_eq!(KindInfo::ALL[39].ty, operation::OpType::PlasmaProfile);
    assert_eq!(KindInfo::ALL[40].ty, operation::OpType::TrochoidalSlot);
    // #532 appended the wire contour behind them (41).
    // #541 appended the sinker burn behind it (42).
    assert_eq!(KindInfo::ALL[42].ty, operation::OpType::SinkerBurn);
    assert_eq!(KindInfo::ALL[KIND_COUNT - 1].ty, operation::OpType::SinkerSideBurn);
    assert_eq!(KindInfo::ALL[13].ty, operation::OpType::Pocket3D, "Pocket3D keeps id 13");
    assert_eq!(KindInfo::ALL[37].ty, operation::OpType::BoringTurning, "the turning tail keeps 37");
    let drilling: Vec<operation::OpType> = Section::Drilling.kinds().into_iter().map(|i| KindInfo::ALL[i].ty).collect();
    assert_eq!(drilling, [operation::OpType::Drill, operation::OpType::Bore, operation::OpType::Probe]);
    assert!(Section::Drilling.kinds_for(Kind::Mill).contains(&38));
    assert!(!Section::Drilling.kinds_for(Kind::Lathe).contains(&38), "a lathe has no probe row");
}

#[test]
fn the_trochoidal_slot_appends_its_id_and_lists_in_mill2d() {
    // #220: appended the probe's way - id 40 (behind the plasma profile's 39), every older row keeps its
    // own - and the menu shows it in the 2D mill section.
    assert_eq!(KindInfo::ALL[40].ty, operation::OpType::TrochoidalSlot);
    assert_eq!(operation::OpType::TrochoidalSlot.ordinal(), 40);
    assert!(Section::Mill2D.kinds_for(Kind::Mill).contains(&40));
    assert!(!Section::Mill2D.kinds_for(Kind::Lathe).contains(&40), "a lathe has no trochoid row");
}

/// Issue #143: a pin goes through the history. Undo puts the previous
/// pins back, redo re-applies, the list is cleaned like `pinned_set`, and
/// a list equal to the current one is refused so no no-op lands on the
/// undo stack.
#[test]
fn a_pin_is_a_command_undo_restores_the_previous_pins() {
    let world = new_world();
    let mut history = History::default();
    let pin = SetRibbonPins::new(&world, &[3, 3, KIND_COUNT, 7]).expect("a new pin list");
    history.execute(&world, pin);
    assert_eq!(pinned_of(&world), vec![3, 7], "the command writes the cleaned list");
    assert!(history.can_undo(), "the pin is on the undo stack");

    let unpin = SetRibbonPins::new(&world, &[7]).expect("an unpin");
    history.execute(&world, unpin);
    assert_eq!(pinned_of(&world), vec![7]);

    assert!(history.undo(&world));
    assert_eq!(pinned_of(&world), vec![3, 7], "undo restores the unpinned kind");
    assert!(history.undo(&world));
    assert!(pinned_of(&world).is_empty(), "undo restores the unpinned ribbon");
    assert!(history.redo(&world));
    assert_eq!(pinned_of(&world), vec![3, 7], "redo re-applies the pin");

    // The same list again (after cleaning) is refused, not pushed.
    assert!(SetRibbonPins::new(&world, &[3, 7, 3]).is_err(), "a no-op pin is refused");
}
