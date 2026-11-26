//! #821: the microARC on a fixture plate. A rotary subplate (the base
//! plate a 4th axis is bolted to) is placed on a plate's dowel pair by
//! its two pin bores, lands fixed, and - with every plate under it -
//! raises the rotary's centerline: the index-swing clause judges with the
//! machine's `nose_to_axis` less the stack (or the setup's stated
//! figure, which wins). A stack whose thickness is unknown, or an
//! effective figure at or under zero, refuses by name; no subplate is
//! judged word for word as before. Save/load keeps both, and a file
//! without them loads unchanged.
//! Provenance: our own tests over our own commands; the microARC numbers
//! are the shipped machine file's (Tormach spec sheets); the subplate's
//! figures are generic, not the maker's.
//!
//! Gate map (M4.4): covers app::rotary_stack, app::accessory (subplate),
//! app::commands::{fixture_plate, accessory, setup_nose},
//! app::posting (index_swing), app::project (setup nose_to_axis key).

use camcraft_app::accessory::{Accessory, subplate};
use camcraft_app::commands::{
    AddTool, CreateOperation, CreateSetup, GenerateOperation, History, InsertAccessory, InsertPlate, LoadMachine,
    PlaceOnPlate, PlatePlacement, SelectPost, SetAccessory, SetFixtureFixed, SetSetupNoseToAxis, SetStock,
};
use camcraft_app::commands::AccessoryPlacement;
use camcraft_app::fixture;
use camcraft_app::geo2d::Point2;
use camcraft_app::geometry::{IDENTITY_Q, Point3};
use camcraft_app::machine::{Machine, parse_mch};
use camcraft_app::machining3d::Entry3D;
use camcraft_app::model;
use camcraft_app::operation::{Height, OpParams, SrcRef, Tilt};
use camcraft_app::plate::{self, MountAt};
use camcraft_app::posting::{index_swing::MARGIN_MM, verification_gate};
use camcraft_app::rotary_stack::{self, Rise};
use camcraft_app::schema::{Location, PostRef};
use camcraft_app::stock::StockShape;
use camcraft_app::tools::catalog;
use flecs_ecs::prelude::World;

const IN: f64 = 25.4;
const R: f64 = 0.375 * IN;
const STICK: f64 = 1.5 * IN;
const NOSE: f64 = 10.6 * IN;
/// The 300 x 300 mm preset plate's thickness, and the generic subplate's.
const PLATE_T: f64 = 20.0;
const SUB_T: f64 = 0.75 * IN;

fn ok<T, E: std::fmt::Debug>(r: Result<T, E>, what: &str) -> T {
    r.unwrap_or_else(|e| panic!("{what}: {e:?}"))
}

fn arc4() -> Machine {
    let text = camcraft_app::posting::builtin_machine_text("Tormach 770MX + microARC 4").expect("shipped");
    parse_mch(text).expect("parses")
}

/// The index_swing suite's document: the kit bar in the microARC, a 1/4"
/// end mill `len` mm long, one flat at A90 - and a declared setup.
fn doc(len: f64) -> (World, History) {
    let world = camcraft_app::world::new_world();
    let mut h = History::default();
    h.execute(&world, ok(LoadMachine::new(&world, "test".into(), Some(arc4())), "machine"));
    h.execute(&world, ok(SelectPost::new(&world, PostRef::Named("Tormach PathPilot".into())), "post"));
    let stock = SetStock::new(
        &world,
        StockShape::Box { dx: STICK, dy: 2.0 * R, dz: 2.0 * R },
        Location { pos: Point3::new(-STICK / 2.0, 0.0, -R), ..Location::default() },
    );
    h.execute(&world, ok(stock, "stock"));
    let mut t = catalog().into_iter().find(|(n, _)| *n == "EM-0600-4FL").map(|(_, t)| t).expect("catalog");
    t.diameter = 0.25 * IN;
    t.shank_d = t.diameter;
    t.rpm = 6000.0;
    t.feed = 15.0 * IN;
    t.holder_len = len - t.flute_len - t.shank_len;
    h.execute(&world, ok(AddTool::new(&world, "EM 1/4", t), "tool"));
    let flat = OpParams::Slot {
        a: Point2::new(-0.75 * IN, 0.0),
        b: Point2::new(-0.25 * IN, 0.0),
        width: 0.25 * IN,
        stock_to_leave: 0.0,
        clearance_z: Height::fixed(R + 0.5 * IN),
        retract_z: Some(Height::fixed(R + 0.1 * IN)),
        top_z: Height::fixed(R),
        bottom_z: Height::fixed(R - 0.015 * IN),
        stepdown: 0.015 * IN,
        stepover: 0.1 * IN,
        feed: 15.0 * IN,
        plunge_feed: 4.0 * IN,
        climb: true,
        entry: Entry3D::Plunge,
        helix_radius: 1.0, blend_turns: false, blend_radius: None,
        links: camcraft_app::links::Links::PLANAR,
        tilt: Some(Tilt { basis_version: 1, a_deg: 90.0, b_deg: 0.0, origin: [0.0; 3] }),
        extend_start: 0.0,
        extend_end: 0.0,
        picks: Default::default(),
    };
    h.execute(&world, ok(CreateOperation::new(&world, "FLAT", "EM 1/4", SrcRef::Stock, flat), "op"));
    h.execute(&world, ok(GenerateOperation::new(&world, "FLAT"), "generate"));
    h.execute(&world, ok(CreateSetup::new(&world, Some("Setup1"), None), "setup"));
    (world, h)
}

/// The top of the machine's table (its Z-travel minimum), where a plate
/// or a lone subplate stands (#821 review: the rise is read from there).
fn table_top(world: &World) -> f64 {
    fixture::table_box(world, None).expect("the microARC machine states a table").max.z
}

/// A fixed 300 x 300 mm dowel plate on the table, well under the bar
/// (its frame's origin on its top face); its name.
fn plate(world: &World, h: &mut History) -> String {
    let p = plate::preset("300 x 300 mm").expect("preset").params();
    let at = Location { pos: Point3::new(-150.0, -150.0, table_top(world) + PLATE_T), rot: IDENTITY_Q };
    let cmd = ok(InsertPlate::new(world, &p, None, PlatePlacement::At(at)), "plate");
    let name = cmd.name().to_owned();
    h.execute(world, cmd);
    h.execute(world, ok(SetFixtureFixed::new(world, &name, true), "fixed"));
    name
}

/// The generic subplate with its bores 100 mm apart (the plate's C2-C4).
fn subplate_params(thickness: f64) -> Accessory {
    ok(subplate::generic().with_dim("bore_spacing", 100.0).and_then(|a| a.with_dim("thickness", thickness)), "dims")
}

/// The subplate standing on the bare table (the T-slots).
fn add_subplate(world: &World, h: &mut History, thickness: f64) -> String {
    let at = Location { pos: Point3::new(0.0, 0.0, table_top(world)), rot: IDENTITY_Q };
    let cmd = ok(InsertAccessory::new(world, &subplate_params(thickness), None, AccessoryPlacement::At(at)), "subplate");
    let name = cmd.name().to_owned();
    h.execute(world, cmd);
    name
}

fn on_dowels(a: &str, b: &str) -> MountAt {
    MountAt::Dowels([a.into(), b.into()])
}

fn swing_lines(world: &World) -> Vec<String> {
    verification_gate(world)
        .into_iter()
        .filter(|p| p.contains("index turns the work") || p.contains("nose_to_axis") || p.contains("index swing"))
        .collect()
}

fn bar_swept() -> f64 {
    R * std::f64::consts::SQRT_2
}

#[test]
fn a_subplate_lands_fixed_and_seats_on_a_dowel_pair_by_its_pins() {
    let (world, mut h) = doc(3.0 * IN);
    let p = plate(&world, &mut h);
    let s = add_subplate(&world, &mut h, SUB_T);
    let def = |n: &str| model::find_by_name(&world, n).and_then(|e| fixture::def_of(&world, &e)).expect(n);
    assert!(def(&s).fixed, "a rotary subplate never turns with the work");
    let place = ok(PlaceOnPlate::new(&world, &s, &p, on_dowels("C2", "C4"), 0.0), "pins fit");
    // C2 is (75, 125) on the plate's top face; the underside stands on it.
    let at = place.location().pos;
    let top = table_top(&world) + PLATE_T;
    assert!((at.x - -75.0).abs() < 1e-9 && (at.y - -25.0).abs() < 1e-9 && (at.z - top).abs() < 1e-9, "{at:?}");
    h.execute(&world, place);
    let hole = PlaceOnPlate::new(&world, &s, &p, MountAt::Hole("C3".into()), 0.0).err().expect("a hole is no pin pair");
    assert!(hole.to_string().contains("located by its two pin bores - place it on a dowel pair"), "{hole}");
    let short = PlaceOnPlate::new(&world, &s, &p, on_dowels("C2", "C3"), 0.0).err().expect("50 mm is not 100 mm");
    assert!(short.to_string().contains("pin bores 100.000 mm apart, but dowels C2-C3"), "{short}");
    assert!(short.to_string().contains("are 50.000 mm apart"), "{short}");
    // Resizing the placed subplate keeps the pin law; its thickness may change.
    let wide = SetAccessory::new(&world, &s, subplate_params(SUB_T).with_dim("bore_spacing", 120.0).expect("dim"));
    assert!(wide.err().expect("pins no longer fit").to_string().contains("120.000 mm apart"));
    h.execute(&world, ok(SetAccessory::new(&world, &s, subplate_params(25.0)), "thicker"));
    match rotary_stack::rise(&world, None) {
        Rise::Stack(st) => assert!((st.thickness - (25.0 + PLATE_T)).abs() < 1e-9, "{st:?}"),
        other => panic!("{other:?}"),
    }
    // Bores off the plate are refused at creation.
    let off = subplate::generic().with_dim("bore_x", 1.0).expect("dim").check();
    assert!(off.expect_err("over the edge").contains("first bore from the -X end"));
}

#[test]
fn the_same_program_clears_on_the_table_and_is_refused_on_the_stack() {
    // 20 mm to spare on the bare table: passes.
    let len = NOSE - bar_swept() - MARGIN_MM - 20.0;
    let (world, mut h) = doc(len);
    let p = plate(&world, &mut h);
    assert!(swing_lines(&world).is_empty(), "a plate alone raises nothing: {:?}", verification_gate(&world));
    let s = add_subplate(&world, &mut h, SUB_T);
    h.execute(&world, ok(PlaceOnPlate::new(&world, &s, &p, on_dowels("C2", "C4"), 0.0), "place"));
    // The 39.05 mm stack eats the 20 mm: refused, naming the stack.
    let lines = swing_lines(&world);
    assert_eq!(lines.len(), 1, "{lines:?}");
    let l = &lines[0];
    let clearance = 20.0 + MARGIN_MM - (SUB_T + PLATE_T);
    assert!(l.starts_with("FLAT: the A90.000 index turns the work"), "{l}");
    assert!(l.contains(&format!("a clearance of {clearance:.3} mm")), "{l}");
    assert!(l.contains(&format!("less the {:.3} mm stack under the rotary: '{s}' 19.050 mm on '{p}' 20.000 mm", SUB_T + PLATE_T)), "{l}");
    let out = std::env::temp_dir().join(format!("camcraft_821_stack_{}.nc", std::process::id()));
    let err = camcraft_app::posting::post_setup(&world, &out).expect_err("never posted on the raised line");
    assert!(err.contains("stack under the rotary"), "{err}");
    assert!(!out.exists());
    // Undo the placement: the subplate stands alone on the T-slots - its
    // own 19.05 mm leaves 5.95 mm over the bar, inside the margin's 5.
    assert!(h.undo(&world));
    assert!(swing_lines(&world).is_empty(), "{:?}", verification_gate(&world));
    assert_eq!(rotary_stack::rise(&world, None), Rise::Stack(rotary_stack::Stack { thickness: SUB_T, layers: vec![(s, SUB_T)] }));
}

#[test]
fn a_stack_of_unknown_thickness_is_refused_by_name() {
    let (world, mut h) = doc(3.0 * IN);
    let p = plate(&world, &mut h);
    let s = add_subplate(&world, &mut h, SUB_T);
    h.execute(&world, ok(PlaceOnPlate::new(&world, &s, &p, on_dowels("C2", "C4"), 0.0), "place"));
    assert!(swing_lines(&world).is_empty(), "the short tool clears the stack: {:?}", verification_gate(&world));
    // The plate as an imported plate reads it: its grid, no figures.
    let e = model::find_by_name(&world, &p).expect("plate");
    let mut d = fixture::def_of(&world, &e).expect("def");
    if let Some(g) = d.plate.plate.as_mut() {
        g.params = None;
    }
    e.entity_view(&world).set(d);
    let lines = swing_lines(&world);
    assert_eq!(lines.len(), 1, "{lines:?}");
    let l = &lines[0];
    assert!(l.starts_with(&format!("FLAT: rotary subplate '{s}' stands on '{p}', a plate whose thickness is not stated")), "{l}");
    assert!(l.contains("the A90.000 index swing is not judged"), "{l}");
    // A measured figure for the setup wins: judged again.
    h.execute(&world, ok(SetSetupNoseToAxis::new(&world, "Setup1", Some(NOSE - 40.0)), "stated"));
    assert!(swing_lines(&world).is_empty(), "{:?}", verification_gate(&world));
}

#[test]
fn an_effective_nose_at_or_under_zero_is_refused_by_name() {
    let (world, mut h) = doc(3.0 * IN);
    let _ = add_subplate(&world, &mut h, NOSE + 1.0);
    let lines = swing_lines(&world);
    assert_eq!(lines.len(), 1, "{lines:?}");
    assert!(lines[0].starts_with("FLAT: the effective nose_to_axis is -1.000 mm"), "{}", lines[0]);
    assert!(lines[0].contains("index swing is not judged"), "{}", lines[0]);
}

#[test]
fn the_setup_s_stated_nose_wins_and_is_an_undoable_command() {
    let len = NOSE - bar_swept() - MARGIN_MM - 20.0;
    let (world, mut h) = doc(len);
    for bad in [0.0, -3.0, f64::NAN, f64::INFINITY, 6000.0] {
        assert!(SetSetupNoseToAxis::new(&world, "Setup1", Some(bad)).is_err(), "{bad} refused");
    }
    assert!(SetSetupNoseToAxis::new(&world, "Setup1", None).is_err(), "no change refused");
    assert!(SetSetupNoseToAxis::new(&world, "Nope", Some(200.0)).is_err(), "unknown setup refused");
    // 30 mm less than the table figure, measured: tighter, refused.
    h.execute(&world, ok(SetSetupNoseToAxis::new(&world, "Setup1", Some(NOSE - 30.0)), "stated"));
    let lines = swing_lines(&world);
    assert_eq!(lines.len(), 1, "{lines:?}");
    assert!(lines[0].contains(&format!("(the setup's stated nose_to_axis {:.3} mm)", NOSE - 30.0)), "{}", lines[0]);
    assert!(h.undo(&world));
    assert_eq!(rotary_stack::stated(&world, None), None);
    assert!(swing_lines(&world).is_empty());
    assert!(h.redo(&world));
    assert_eq!(rotary_stack::stated(&world, None), Some(NOSE - 30.0));
}

#[test]
fn no_stack_is_judged_word_for_word_as_before() {
    let len = NOSE - bar_swept() - MARGIN_MM + 2.0;
    let (world, mut h) = doc(len);
    let _plate = plate(&world, &mut h);
    let tip = NOSE - len;
    let want = format!(
        "FLAT: the A90.000 index turns the work under the tool - at the safe Z the tip of EM 1/4 ({len:.3} mm long) \
         is {tip:.3} mm above the A centerline, the stock, part and turning fixtures sweep {:.3} mm, a clearance of \
         3.000 mm against the 5.0 mm needed",
        bar_swept()
    );
    assert_eq!(swing_lines(&world), vec![want]);
    assert_eq!(rotary_stack::rise(&world, None), Rise::Table);
}

#[test]
fn the_subplate_and_the_stated_nose_survive_a_save_and_old_files_load_unchanged() {
    let (world, mut h) = doc(3.0 * IN);
    let p = plate(&world, &mut h);
    let s = add_subplate(&world, &mut h, SUB_T);
    h.execute(&world, ok(PlaceOnPlate::new(&world, &s, &p, on_dowels("C2", "C4"), 0.0), "place"));
    h.execute(&world, ok(SetSetupNoseToAxis::new(&world, "Setup1", Some(250.5)), "stated"));
    let text = ok(serde_json::to_string_pretty(&camcraft_app::project::collect(&world)), "save");
    assert!(text.contains("\"rotary_subplate\"") && text.contains("\"nose_to_axis\": 250.5"), "{text}");
    let back = camcraft_app::world::new_world();
    ok(camcraft_app::project::open_noted(&back, &text), "opens");
    let def = |w: &World| model::find_by_name(w, &s).and_then(|e| fixture::def_of(w, &e)).expect("subplate");
    assert_eq!(def(&back), def(&world), "the subplate, its place and its fixed flag reload");
    assert_eq!(rotary_stack::stated(&back, None), Some(250.5));
    assert_eq!(rotary_stack::rise(&back, None), rotary_stack::rise(&world, None));
    // A file from before #821: no setup key, the subplate gone - loads as no stack.
    let mut j: serde_json::Value = ok(serde_json::from_str(&text), "json");
    strip(&mut j);
    let old = camcraft_app::world::new_world();
    ok(camcraft_app::project::open_noted(&old, &j.to_string()), "opens");
    assert_eq!(rotary_stack::stated(&old, None), None);
    assert_eq!(rotary_stack::rise(&old, None), Rise::Table);
}

/// Drop every `nose_to_axis` key of a setup and every rotary subplate row.
fn strip(v: &mut serde_json::Value) {
    if let Some(rows) = v.get_mut("entities").and_then(|e| e.as_array_mut()) {
        rows.retain(|r| !r.to_string().contains("rotary_subplate"));
        for r in rows {
            if let Some(s) = r.get_mut("setup").and_then(|s| s.as_object_mut()) {
                s.remove("nose_to_axis");
            }
        }
    }
}
