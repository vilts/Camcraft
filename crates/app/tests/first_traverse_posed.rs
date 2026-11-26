//! #3084: the first traverse over work that the machine's own
//! work-turning rotaries can leave at any pose. The post sends Z, then
//! X/Y, then the move with the rotary words, so the X/Y traverse runs
//! with each work-turning rotary where the previous program left it.
//!
//! - Two rotaries turn the work (A and B on the table): a rotary kind
//!   reads its stock on the sphere about the WCS origin, where the lines
//!   meet. The laid square bar's stock corner stands 18.815 from it, so
//!   a clearance of 12 (over the 9.899 of the one fitted axis) is held
//!   by name; 20 passes.
//! - The table tilts the work (A on the table, B in the head): a 5-axis
//!   kind reads its stock swept a full turn about A, not in the part
//!   frame. The dome's 40 x 40 x 12 stock turns up to 23.324, so a
//!   clearance of 20 (over the block's top 12) is held; 25 passes.
//! - A work-turning rotary that states no line refuses by name.
//! - #3127: a table A whose stated pivot moves its line off the WCS
//!   origin refuses by name; a zero pivot is read as before.
//! - #3248: the same refusal with two work-turning rotaries; a vertical
//!   line's stated pivot is accepted, the law drops out.
//!
//! Provenance: our own machine files and fixtures (the rotary suite's
//! turned bar, the first-traverse suite's dome); the shop rule "never
//! traverse through uncut stock or steel" (machine-tool handbooks). No
//! vendor material.
//!
//! Gate map (#3084): covers app::operation (first_traverse_posed,
//! generate, stamp), sim::verify (first_traverse_posed).

mod common;

use camcraft_app::commands::{AddTool, CreateOperation, GenerateOperation, History, ImportModel, LoadMachine, SetStock, SetWcsOrigin};
use camcraft_app::geometry::{Mesh, Point3, Triangle, IDENTITY_Q};
use camcraft_app::machine::parse_mch;
use camcraft_app::operation::{Height, OpParams, OpStatus, SrcRef};
use camcraft_app::schema::Location;
use camcraft_app::stock::StockShape;
use camcraft_app::tools::catalog;
use camcraft_app::verify::Warning;
use camcraft_app::{model, posting};
use flecs_ecs::prelude::World;

const BALL: &str = "BN-0600";

/// Which rotaries turn the work.
#[derive(Clone, Copy)]
enum Rig {
    /// A and B on the table: both turn the work.
    TwoWork,
    /// As `TwoWork`, with A's stated `pivot` (#3248).
    TwoWorkAPivot([f64; 3]),
    /// A on the table turns the work; B rides the head.
    TableA,
    /// A rotary Q on the table turns the work about no stated line; B
    /// rides the head.
    Unlined,
    /// As `TableA`, with A's stated `pivot` (#3127).
    TableAPivot([f64; 3]),
    /// A rotary C on the table turns the work about no stated line (its
    /// RS-274 line is vertical); B rides the head. With C's stated
    /// `pivot` (#3248).
    VerticalCPivot([f64; 3]),
}

fn machine_text(rig: Rig) -> String {
    let rot = |id: &str, name: &str, axis: &str, inner: &str| {
        format!(r#"{{ "id": "{id}", "name": "{name}", "type": "rotary"{axis}, "travel": {{ "min": -120.0, "max": 120.0 }}{inner} }}"#)
    };
    let head = r#"{ "id": "hed", "name": "head", "type": "head", "spindle": { "min_speed": 50.0, "max_speed": 15000.0 } }"#;
    let x = r#", "axis": [1.0, 0.0, 0.0]"#;
    let y = r#", "axis": [0.0, 1.0, 0.0]"#;
    let head_b = rot("rb", "B", y, &format!(r#", "parts": [ {head} ]"#));
    let (z_parts, table) = match rig {
        Rig::TwoWork | Rig::TwoWorkAPivot(_) => (head.to_string(), rot("ra", "A", x, &format!(r#", "parts": [ {} ]"#, rot("rb", "B", y, "")))),
        Rig::TableA | Rig::TableAPivot(_) => (head_b, rot("ra", "A", x, "")),
        Rig::Unlined => (head_b, rot("rq", "Q", "", "")),
        Rig::VerticalCPivot(_) => (head_b, rot("rc", "C", "", "")),
    };
    let first = match rig {
        Rig::Unlined => "Q",
        Rig::VerticalCPivot(_) => "C",
        _ => "A",
    };
    format!(
        r#"{{
  "general": {{ "model": "TEST POSED", "vendor": "Camcraft", "kind": "mill" }},
  "kinematics": {{ "default": {{
    "units": {{ "length": "mm", "angle": "degrees" }},
    "parts": [
      {{ "id": "base", "name": "table", "type": "table", "parts": [
        {{ "id": "crl", "name": "X", "type": "linear", "travel": {{ "min": -200.0, "max": 200.0 }}, "parts": [
          {{ "id": "sld", "name": "Y", "type": "linear", "travel": {{ "min": -200.0, "max": 200.0 }} }} ] }},
        {{ "id": "col", "name": "Z", "type": "linear", "travel": {{ "min": -100.0, "max": 300.0 }}, "safe_retract": 0.0, "parts": [ {z_parts} ] }},
        {table}
      ] }}
    ] }} }},
  "controller": {{ "synced_configuration": {{ "parts": {{
    "X": {{ "coordinate": 0 }}, "Y": {{ "coordinate": 1 }}, "Z": {{ "coordinate": 2 }},
    "{first}": {{ "coordinate": 3 }}, "B": {{ "coordinate": 4 }}
  }} }} }},
  "tooling": {{ "number_of_tools": 24 }}
}}"#
    )
}

fn world_on(rig: Rig) -> (World, History) {
    let world = camcraft_app::world::new_world();
    let mut h = History::default();
    let mut m = parse_mch(&machine_text(rig)).unwrap_or_else(|e| panic!("test machine: {e}"));
    if let Rig::TableAPivot(at) | Rig::TwoWorkAPivot(at) | Rig::VerticalCPivot(at) = rig {
        let name = if matches!(rig, Rig::VerticalCPivot(_)) { "C" } else { "A" };
        m.axes.iter_mut().filter(|a| a.name == name).for_each(|a| a.pivot = Some(at));
    }
    h.execute(&world, LoadMachine::new(&world, "test-posed.mch".into(), Some(m)).expect("machine"));
    h.execute(&world, SetWcsOrigin::new(&world, Location { pos: Point3::ZERO, rot: IDENTITY_Q }).expect("G54 on the lines"));
    let t = catalog().into_iter().find(|(n, _)| *n == BALL).map(|(_, t)| t).expect("catalog");
    h.execute(&world, AddTool::new(&world, BALL, t).expect("tool"));
    (world, h)
}

fn first_low(warns: &[Warning]) -> Option<(f64, f64, bool)> {
    warns.iter().find_map(|w| match *w {
        Warning::FirstTraverseLow { height_mm, top_mm, fixture, .. } => Some((height_mm, top_mm, fixture)),
        _ => None,
    })
}


/// The rotary suite's turned bar laid along X in a 32 x 14 x 14 square
/// bar of stock (x +-16, y +-7, z +-7), on `rig`'s two-rotary table.
/// `Err` carries the refusal, as [`contour5d_on`] does.
fn laid_bar_on(rig: Rig, clearance: f64) -> Result<(World, OpStatus, Vec<Warning>), String> {
    let (world, mut h) = world_on(rig);
    let at = Location { pos: Point3::new(0.0, 0.0, -7.0), rot: IDENTITY_Q };
    h.execute(&world, SetStock::new(&world, StockShape::Box { dx: 32.0, dy: 14.0, dz: 14.0 }, at).expect("stock"));
    h.execute(&world, ImportModel::from_mesh("bar", Mesh::from_facets(common::rotary_bar::laid())));
    let p = OpParams::RotaryParallel {
        tolerance: 0.1,
        stepover: Some(2.0),
        clearance_z: Height::fixed(clearance),
        retract_z: None,
        top_z: Height::fixed(6.0),
        bottom_z: Height::fixed(1.0),
        feed: 500.0,
        plunge_feed: 100.0,
        climb: true,
        lead_deg: 0.0,
        lag_deg: 0.0,
        max_tilt_deg: 60.0,
        stated_axis: None,
        picks: Default::default(),
    };
    h.execute(&world, CreateOperation::new(&world, "OP", BALL, SrcRef::Entity("bar".into()), p).expect("create"));
    let cmd = GenerateOperation::new(&world, "OP").map_err(|e| format!("{e:?}"))?;
    h.execute(&world, cmd);
    let e = model::find_by_name(&world, "OP").expect("op");
    let cached = model::toolpath_of(&world, &e).ok_or_else(|| format!("no path: {:?}", camcraft_app::operation::status_of(&world, &e)))?;
    let (status, warns) = (camcraft_app::operation::status_of(&world, &e), cached.warnings.clone());
    Ok((world, status, warns))
}

/// [`laid_bar_on`] the plain two-rotary rig, where a refusal is a failure.
fn laid_bar_on_two_rotaries(clearance: f64) -> (World, OpStatus, Vec<Warning>) {
    laid_bar_on(Rig::TwoWork, clearance).unwrap_or_else(|e| panic!("{e}"))
}

#[test]
fn two_work_turning_rotaries_read_the_rotary_kinds_stock_on_its_sphere() {
    // The stock corner (16, 7, 7) from the WCS origin: sqrt(354).
    let sphere = 354f64.sqrt();
    let (world, status, warns) = laid_bar_on_two_rotaries(12.0);
    assert_eq!(status, OpStatus::Ok);
    let (height, top, fixture) = first_low(&warns).unwrap_or_else(|| panic!("clearance 12: no first-traverse verdict in {warns:?}"));
    assert!((top - sphere).abs() < 1e-6 && !fixture, "the stock corner on the sphere: {top}, want {sphere}");
    assert!((height - 12.0).abs() < 1e-6, "the post traverses at the clearance: {height}");
    let n = warns.iter().filter(|w| matches!(w, Warning::FirstTraverseLow { .. })).count();
    assert_eq!(n, 1, "one verdict names the highest top: {warns:?}");
    assert!(warns.iter().any(Warning::blocks_post));
    assert!(posting::post_setup_unwritten(&world).is_err(), "the program must not post");

    let (_, status, warns) = laid_bar_on_two_rotaries(20.0);
    assert_eq!(status, OpStatus::Ok);
    assert_eq!(first_low(&warns), None, "clearance 20 stands over the sphere: {warns:?}");
}

/// A spherical cap (radius 12, to 60 degrees off the pole) on z = 0.
fn dome() -> Mesh {
    const TAU: f64 = std::f64::consts::TAU;
    let (radius, polar_max, n_lat, n_lon) = (12.0, std::f64::consts::FRAC_PI_3, 6usize, 24usize);
    let ring = |k: usize| -> Vec<Point3> {
        let ph = polar_max * k as f64 / n_lat as f64;
        let (z, rr) = (radius * ph.cos(), radius * ph.sin());
        (0..n_lon).map(|j| Point3::new(rr * (TAU * j as f64 / n_lon as f64).cos(), rr * (TAU * j as f64 / n_lon as f64).sin(), z)).collect()
    };
    let apex = Point3::new(0.0, 0.0, radius);
    let first = ring(1);
    let mut f: Vec<Triangle> = (0..n_lon).map(|j| Triangle::new(apex, first[j], first[(j + 1) % n_lon])).collect();
    for k in 1..n_lat {
        let (a, b) = (ring(k), ring(k + 1));
        for j in 0..n_lon {
            let n = (j + 1) % n_lon;
            f.push(Triangle::new(a[j], b[j], b[n]));
            f.push(Triangle::new(a[j], b[n], a[n]));
        }
    }
    Mesh::from_facets(f)
}

/// A 5-Axis Contour over the dome in a 40 x 40 x 12 block (x, y +-20,
/// z 0..12) on `rig`.
fn contour5d_on(rig: Rig, clearance: f64) -> Result<(World, OpStatus, Vec<Warning>), String> {
    let (world, mut h) = world_on(rig);
    let at = Location { pos: Point3::ZERO, rot: IDENTITY_Q };
    h.execute(&world, SetStock::new(&world, StockShape::Box { dx: 40.0, dy: 40.0, dz: 12.0 }, at).expect("stock"));
    h.execute(&world, ImportModel::from_mesh("dome", dome()));
    let p = OpParams::Contour5D {
        tolerance: 0.1,
        clearance_z: Height::fixed(clearance),
        retract_z: None,
        top_z: Height::fixed(11.0),
        bottom_z: Height::fixed(7.0),
        feed: 500.0,
        plunge_feed: 100.0,
        climb: true,
        lead_deg: 15.0,
        lag_deg: 0.0,
        max_tilt_deg: 60.0,
        shank_avoid: false,
        picks: Default::default(),
    };
    h.execute(&world, CreateOperation::new(&world, "OP", BALL, SrcRef::Entity("dome".into()), p).expect("create"));
    let cmd = GenerateOperation::new(&world, "OP").map_err(|e| format!("{e:?}"))?;
    h.execute(&world, cmd);
    let e = model::find_by_name(&world, "OP").expect("op");
    let cached = model::toolpath_of(&world, &e).ok_or_else(|| format!("no path: {:?}", camcraft_app::operation::status_of(&world, &e)))?;
    let (status, warns) = (camcraft_app::operation::status_of(&world, &e), cached.warnings.clone());
    Ok((world, status, warns))
}

#[test]
fn a_tilting_table_reads_a_5_axis_kinds_stock_swept_about_its_line() {
    // The stock's top edge (x, 20, 12) turns about A on radius sqrt(544);
    // the part frame reads only the block's top, Z12.
    let swept = 544f64.sqrt();
    let (world, status, warns) = contour5d_on(Rig::TableA, 20.0).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(status, OpStatus::Ok);
    let (height, top, fixture) = first_low(&warns).unwrap_or_else(|| panic!("clearance 20: no first-traverse verdict in {warns:?}"));
    assert!((top - swept).abs() < 1e-6 && !fixture, "the stock edge turned up: {top}, want {swept}");
    assert!((height - 20.0).abs() < 1e-6, "{height}");
    assert!(posting::post_setup_unwritten(&world).is_err(), "the program must not post");

    let (_, status, warns) = contour5d_on(Rig::TableA, 25.0).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(status, OpStatus::Ok);
    assert_eq!(first_low(&warns), None, "clearance 25 stands over the swept stock: {warns:?}");
}

#[test]
fn a_work_turning_rotary_with_no_line_refuses_by_name() {
    let refused = match contour5d_on(Rig::Unlined, 25.0) {
        Err(e) => e,
        Ok((_, status, warns)) => panic!("no refusal: {status:?} {warns:?}"),
    };
    for text in ["work-turning rotary Q", "first traverse", "cannot be judged"] {
        assert!(refused.contains(text), "{text}: {refused}");
    }
}

#[test]
fn a_table_line_off_the_wcs_origin_refuses_by_name() {
    // #3127: A's line 40 mm over the WCS origin sweeps the stock higher
    // than the line through the origin reads; the law refuses.
    let refused = match contour5d_on(Rig::TableAPivot([0.0, 0.0, 40.0]), 25.0) {
        Err(e) => e,
        Ok((_, status, warns)) => panic!("no refusal: {status:?} {warns:?}"),
    };
    for text in ["work-turning rotary A", "pivot or offset", "off the WCS origin", "cannot be judged"] {
        assert!(refused.contains(text), "{text}: {refused}");
    }
}

#[test]
fn a_zero_pivot_reads_the_table_line_through_the_wcs_origin() {
    let (_, status, warns) = contour5d_on(Rig::TableAPivot([0.0; 3]), 25.0).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(status, OpStatus::Ok);
    assert_eq!(first_low(&warns), None, "clearance 25 stands over the swept stock: {warns:?}");
}

#[test]
fn one_of_two_work_turning_rotaries_off_the_wcs_origin_refuses_by_name() {
    // #3248: with A and B both turning the work, A's pivot 40 mm over the
    // WCS origin moves the sphere's centre; the law refuses by name.
    let refused = match laid_bar_on(Rig::TwoWorkAPivot([0.0, 0.0, 40.0]), 20.0) {
        Err(e) => e,
        Ok((_, status, warns)) => panic!("no refusal: {status:?} {warns:?}"),
    };
    for text in ["work-turning rotary A", "pivot or offset", "off the WCS origin", "cannot be judged"] {
        assert!(refused.contains(text), "{text}: {refused}");
    }
}

#[test]
fn a_zero_pivot_on_two_work_turning_rotaries_reads_the_sphere_as_before() {
    // #3248: the control. A zero pivot is the plain two-rotary rig.
    let (_, status, warns) = laid_bar_on(Rig::TwoWorkAPivot([0.0; 3]), 20.0).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(status, OpStatus::Ok);
    assert_eq!(first_low(&warns), None, "clearance 20 stands over the sphere: {warns:?}");
}

#[test]
fn a_vertical_work_turning_rotary_with_a_stated_pivot_is_not_refused() {
    // #3248: the single-rotary branch refuses an off-origin pivot only on
    // a line that is not vertical. C's RS-274 line is vertical, so its
    // stated pivot is ignored by design and the law drops out.
    for pivot in [[0.0, 0.0, 40.0], [0.0; 3]] {
        let (_, status, warns) = contour5d_on(Rig::VerticalCPivot(pivot), 25.0).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(status, OpStatus::Ok);
        assert_eq!(first_low(&warns), None, "pivot {pivot:?}: the law drops out: {warns:?}");
    }
}
