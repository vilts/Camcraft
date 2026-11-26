//! #2826: the Rotary Flow band end to end. The caller PICKS the guide
//! chains (the edge rings bounding a band on a turned part), the axis
//! comes from the mounted machine's stated rotary line
//! (tormach-770mx-microarc4.mch's A about X, millturn.mch's spindle C
//! about Z), and the kind blends flow passes between each adjacent
//! pair about it. What "right" means here:
//!
//! - the band generates through the operation layer with a ball, and
//!   refuses a flat tool by name;
//! - every cut move states a pose radial about the band's own axis -
//!   no world-Z clipping of the band, the heights read as BALL-CENTER
//!   RADII;
//! - the microARC's stated A and the mill-turn's spindle C both carry
//!   the band, and a clearance radius inside the work is refused by
//!   name.
//!
//! Provenance: our own fixtures over our own machine files and
//! commands; nothing of vendor binaries.
//!
//! Gate map (M4.4): covers app::operation (the RotaryFlow arm),
//! app::rotary_stated, camcraft-motion (rotary_flow, axis_stated),
//! app::fixture, app::posting.

mod common;

use flecs_ecs::prelude::World;

use camcraft_app::cl::{Move, MoveKind};
use camcraft_app::commands::{
    AddTool, CommandError, CreateOperation, GenerateOperation, History, ImportModel, LoadMachine, SetStock,
};
use camcraft_app::geometry::{feature, IDENTITY_Q, Mesh, Point3, Triangle};
use camcraft_app::machine::{AxisLimits, Coolant, LiveToolCodes, Machine};
use camcraft_app::model;
use camcraft_app::operation::{FeatureSel, GeoSel, Height, OpParams, OpStatus, SrcRef};
use camcraft_app::schema::Location;
use camcraft_app::stock::StockShape;
use camcraft_app::tools::catalog;
use camcraft_app::{fixture, operation};

const TAU: f64 = std::f64::consts::TAU;
/// The tormach-770mx-microarc4.mch registry entry: an A about X,
/// stated `[1, 0, 0]`, work-turning.
const MICROARC: &str = "Tormach 770MX + microARC 4";
/// The millturn.mch registry entry: a spindle C about Z, stated.
const MILLTURN: &str = "Mill-turn";

/// The turned bar's meridian (rho, s): wall r6 with an interior
/// cylindrical band (the rings at -8 and 2), shoulder, neck r5.
const MERIDIAN: [(f64, f64); 7] = [(0.0, -16.0), (6.0, -16.0), (6.0, -8.0), (6.0, 2.0), (5.0, 8.0), (5.0, 16.0), (0.0, 16.0)];

/// The bar revolved about `axis` (0 = world X, laid; 2 = world Z,
/// standing), 72 facets around, centred on the origin.
fn bar(axis: usize) -> Vec<Triangle> {
    let pt = |rho: f64, s: f64, j: usize| {
        let th = TAU * j as f64 / 72.0;
        match axis {
            0 => Point3::new(s, rho * th.cos(), rho * th.sin()),
            _ => Point3::new(rho * th.cos(), rho * th.sin(), s),
        }
    };
    let mut f = Vec::new();
    for w in MERIDIAN.windows(2) {
        let ((ra, za), (rb, zb)) = (w[0], w[1]);
        for j in 0..72 {
            let k = (j + 1) % 72;
            let (a1, a2, b1, b2) = (pt(ra, za, j), pt(ra, za, k), pt(rb, zb, j), pt(rb, zb, k));
            if ra <= 1e-12 {
                f.push(Triangle::new(a1, b2, b1));
            } else if rb <= 1e-12 {
                f.push(Triangle::new(a1, a2, b2));
            } else {
                f.push(Triangle::new(a1, a2, b2));
                f.push(Triangle::new(a1, b2, b1));
            }
        }
    }
    f
}

fn flow(stated_axis: Option<[f64; 3]>, clearance: f64) -> OpParams {
    OpParams::RotaryFlow {
        tolerance: 0.1,
        stepover: Some(2.0),
        clearance_z: Height::fixed(clearance),
        retract_z: None,
        feed: 500.0,
        plunge_feed: 100.0,
        climb: true,
        lead_deg: 0.0,
        lag_deg: 0.0,
        max_tilt_deg: 60.0,
        stated_axis,
    }
}

/// The band's guide chains: the model's feature chains whose points
/// all stand at one of the given stations along the axis (the
/// shoulder ring at -2 and the neck ring at 2 bound the neck band).
fn band_picks(world: &World, axis: usize, stations: [f64; 2]) -> SrcRef {
    let e = model::find_by_name(world, "bar").expect("bar");
    let f = model::features_of(world, &e).expect("features");
    let mut items: Vec<([f64; 3], GeoSel)> = feature::chains(&f)
        .into_iter()
        .filter_map(|c| {
            let pts = feature::chain_points(&f, &c);
            let on_band = pts.iter().all(|p| (p[axis] - stations[0]).abs() < 1e-4 || (p[axis] - stations[1]).abs() < 1e-4);
            let n = pts.len() as f64;
            let key = pts.iter().fold([0.0f64; 3], |a, p| [a[0] + p[0] / n, a[1] + p[1] / n, a[2] + p[2] / n]);
            on_band.then(|| (key, GeoSel { entity: "bar".into(), feature: FeatureSel::Chain(c.id) }))
        })
        .collect();
    items.sort_by(|a, b| a.0[0].total_cmp(&b.0[0]).then(a.0[1].total_cmp(&b.0[1])).then(a.0[2].total_cmp(&b.0[2])));
    assert!(items.len() >= 2, "the band's ring chains must exist, got {}", items.len());
    SrcRef::Selection(items.into_iter().map(|(_, g)| g).collect())
}

/// A world with the ball, a snug round stock, the model "bar" (the
/// revolve about `axis`) and the flow op over the band's picks, on the
/// given machine (its stated rotary line judged against the bar's
/// faces). The WCS origin stays at the default, so the stated line
/// through it is the bar's own axis. No generation runs here.
fn build(axis: usize, params: OpParams, machine: Option<&str>) -> World {
    build_with(axis, params, |world, h| {
        if let Some(m) = machine {
            h.execute(world, LoadMachine::builtin(world, m).expect("machine"));
        }
    })
}

/// [`build`] on a hand-built bench machine instead of a registry name:
/// the standing bar on the bench's spindle C line.
fn build_on(params: OpParams, machine: Machine) -> World {
    build_with(2, params, |world, h| {
        h.execute(world, LoadMachine::new(world, "millturn.mch".into(), Some(machine)).expect("bench"));
    })
}

fn build_with(axis: usize, params: OpParams, load: impl FnOnce(&World, &mut History)) -> World {
    let world = camcraft_app::world::new_world();
    let mut h = History::default();
    let tool = catalog().into_iter().find(|(n, _)| *n == "BN-0600").map(|(_, t)| t).expect("catalog ball");
    h.execute(&world, AddTool::new(&world, "BN-0600", tool).expect("tool"));
    load(&world, &mut h);
    let loc = if axis == 0 {
        Location { pos: Point3::new(-16.0, 0.0, 0.0), rot: camcraft_app::geometry::quat_from_axis_angle(1, std::f64::consts::FRAC_PI_2) }
    } else {
        Location { pos: Point3::new(0.0, 0.0, -16.0), rot: IDENTITY_Q }
    };
    h.execute(
        &world,
        SetStock::new(&world, StockShape::Cylinder { diameter: 12.0, height: 32.0 }, loc).expect("stock"),
    );
    h.execute(&world, ImportModel::from_mesh("bar", Mesh::from_facets(bar(axis))));
    h.execute(
        &world,
        CreateOperation::new(&world, "OP", "BN-0600", band_picks(&world, axis, [2.0, 8.0]), params).expect("create"),
    );
    world
}

/// [`build`], with the band generated.
fn world_for(axis: usize, params: OpParams, machine: Option<&str>) -> World {
    let world = build(axis, params, machine);
    h_generate(&world);
    world
}

/// The op's generation, as the command runs it.
fn h_generate(world: &World) {
    let cmd = GenerateOperation::new(world, "OP").expect("generate");
    History::default().execute(world, cmd);
}

fn moves(world: &World) -> Vec<camcraft_app::cl::Move> {
    let e = model::find_by_name(world, "OP").expect("op");
    model::toolpath_of(world, &e).expect("a cached path").path.moves
}

/// The path's stated C range (deg), for the travel assertions.
fn c_range(ms: &[camcraft_app::cl::Move]) -> (f64, f64) {
    ms.iter()
        .filter_map(|m| m.rotary)
        .fold((f64::INFINITY, f64::NEG_INFINITY), |(l, h), c| (l.min(c), h.max(c)))
}

/// A bench shaped like millturn.mch's spindle: a C about Z with
/// `travel` stated, its rate stated only when `c_rate` is Some (#906).
fn bench(travel: (f64, f64), c_rate: Option<f64>) -> Machine {
    Machine {
        model: "Bench mill-turn".into(),
        kind: Some("mill-turn".into()),
        spindle_max: Some(6000.0),
        coolants: vec![Coolant { name: "Flood".into(), m_code: "M8".into() }],
        axes: vec![
            AxisLimits { name: "X".into(), feed: Some(4000.0), ..Default::default() },
            AxisLimits { name: "Y".into(), feed: Some(4000.0), ..Default::default() },
            AxisLimits { name: "Z".into(), feed: Some(4000.0), ..Default::default() },
            AxisLimits {
                name: "C".into(),
                rotary: true,
                axis: Some([0.0, 0.0, 1.0]),
                travel: Some(travel),
                feed: c_rate,
                rapid: c_rate.map(|v| 2.0 * v),
                ..Default::default()
            },
        ],
        live_tool: Some(LiveToolCodes {
            c_axis_on: "M1001".into(),
            c_axis_off: "M1002".into(),
            spindle_on: "M1003".into(),
            spindle_off: "M1004".into(),
        }),
        ..Default::default()
    }
}

fn is_ok(world: &World) -> bool {
    let e = model::find_by_name(world, "OP").expect("op");
    OpStatus::Ok == operation::status_of(world, &e)
}

/// The generation refusal's own words, when the command refuses.
fn generate_err_text(world: &World, name: &str) -> String {
    match GenerateOperation::new(world, name) {
        Err(e) => e.to_string(),
        Ok(cmd) => {
            History::default().execute(world, cmd);
            String::from("generated")
        }
    }
}

#[test]
fn the_band_generates_on_the_stated_a_and_cuts_radially() {
    // The microARC 4's stated A is [1, 0, 0] through the work offset
    // origin: the bar lies ON it, the judge holds the line, and every
    // cut pose is radial - the ball-centre stands on the bar's own
    // meridian radius, never on a world-Z ladder.
    let world = world_for(0, flow(None, 25.0), Some(MICROARC));
    assert!(is_ok(&world), "the band generates on the stated A");
    let ms = moves(&world);
    let cuts = ms.iter().filter(|m| m.kind != MoveKind::Rapid).count();
    assert!(cuts > 50, "a whole band of contact moves, got {cuts}");
    for (i, m) in ms.iter().enumerate() {
        let Some(a) = m.axis else {
            panic!("move {i} states no pose: {m:?}");
        };
        // Laid bar: axis world X, so the radial of the move's own
        // azimuth is [0, cos, sin] in the Y/Z plane.
        let rho = (m.to.y * m.to.y + m.to.z * m.to.z).sqrt();
        assert!(rho > 1.0, "cut {i} rides off the bar: {m:?}");
        let lean = a[1] * (m.to.y / rho) + a[2] * (m.to.z / rho);
        assert!(
            a.iter().all(|v| v.is_finite()) && (lean - 1.0).abs() < 1e-6 && a[0].abs() < 1e-6,
            "cut {i} is not radial about the bar's axis: pose {a:?} at {:?}",
            m.to
        );
    }
}

/// #2828: the shipped millturn's spindle C (-360..360 travel, stated
/// rates) carries the full-ring band. Each flow pass winds a whole
/// turn of C; between passes the path retracts to its link cylinder,
/// and the unwind rewinds whole turns there - orbit rapids on the
/// cylinder - so every C stays inside the travel. The refusal the
/// unproven law answered before is gone. (The section is not posted
/// here: the radial field law's own verdict on the band's convex
/// crease is a separate, already-reported finding.)
#[test]
fn the_millturns_spindle_c_unwinds_a_full_ring_band_at_its_radial_links() {
    let world = world_for(2, flow(None, 25.0), Some(MILLTURN));
    assert!(is_ok(&world), "the band generates and unwinds on the spindle C");
    let ms = moves(&world);
    let (lo, hi) = c_range(&ms);
    assert!(lo >= -360.0 - 1e-9 && hi <= 360.0 + 1e-9, "every C inside the travel: {lo}..{hi}");
    for w in ms.windows(2) {
        assert_eq!(w[0].to, w[1].from, "the chain holds through the rewinds");
    }
    let worst = ms.windows(2).filter_map(|w| Some((w[1].rotary? - w[0].rotary?).abs())).fold(0.0, f64::max);
    assert!(worst < 90.0, "C stays continuous through the rewinds, worst {worst}");
    // The passes still wind whole turns (a multi-pass band), and the
    // rewinds ride as orbit rapids on the retract cylinder - tip
    // radius 25 less the 3 mm ball - spanning whole turns of C.
    let fed: f64 = ms.iter().filter(|m| m.kind == MoveKind::Feed).filter_map(|m| m.rotary).fold(0.0, |a, c| a.max(c.abs()));
    assert!(fed >= 300.0, "the band's passes wind C, widest {fed}");
    let on_cyl = |m: &Move| {
        m.kind == MoveKind::Rapid
            && m.from != m.to
            && m.from.z == m.to.z
            && (m.from.x.hypot(m.from.y) - 22.0).abs() < 0.5
            && (m.to.x.hypot(m.to.y) - 22.0).abs() < 0.5
    };
    let mut best_run = 0.0f64;
    let mut run: Vec<&Move> = Vec::new();
    let flush = |run: &mut Vec<&Move>, best: &mut f64| {
        if let (Some(first), Some(last)) = (run.first().and_then(|m| m.rotary), run.last().and_then(|m| m.rotary)) {
            *best = best.max((last - first).abs());
        }
        run.clear();
    };
    for m in &ms {
        if on_cyl(m) {
            run.push(m);
        } else {
            flush(&mut run, &mut best_run);
        }
    }
    flush(&mut run, &mut best_run);
    assert!(best_run >= 300.0, "a rewind orbits a whole turn on the link cylinder, widest run {best_run}");
    // The verify's own read still answers the machine's stated line
    // for the sheet and the axis pick.
    let e = model::find_by_name(&world, "OP").expect("op");
    let op = model::operation_of(&world, &e).expect("operation");
    let _ = fixture::rotary_axis_of(&world, &op);
}

/// #906: a rewind needs a stated C rate. The bench states the travel
/// but no rate, so the multi-pass band's rewind is refused by name -
/// never posted at an uncapped feed - and no path is cached.
#[test]
fn a_flow_rewind_without_a_stated_c_rate_is_refused_by_name() {
    let world = build_on(flow(None, 25.0), bench((-360.0, 360.0), None));
    let err = generate_err_text(&world, "OP");
    assert!(err.contains("Rotary Flow") && err.contains("states a C travel but no C rate"), "{err}");
    let e = model::find_by_name(&world, "OP").expect("op");
    assert!(model::toolpath_of(&world, &e).is_none(), "a refused band leaves no path");
}

/// The single-cut stretch the radial rewind cannot answer: one flow
/// pass spans a whole turn of C between the band's radial links, wider
/// than a travel that narrow - no clear link stands inside it, and
/// the band is refused by name.
#[test]
fn a_flow_pass_wider_than_the_stated_travel_refuses_by_name() {
    let world = build_on(flow(None, 25.0), bench((-90.0, 90.0), Some(6000.0)));
    let err = generate_err_text(&world, "OP");
    assert!(err.contains("Rotary Flow") && err.contains("no clear link stands inside it"), "{err}");
    let e = model::find_by_name(&world, "OP").expect("op");
    assert!(model::toolpath_of(&world, &e).is_none(), "a refused band leaves no path");
}

#[test]
fn the_band_generates_and_a_buried_radius_is_refused_by_name() {
    // The kernel's radial guard (#2774's alignment): a clearance
    // radius inside the work's own radius plus the ball is refused at
    // the generator, by name - never a silent shallow program.
    // (Posting the band is held out of this test: the radial field
    // law's 0.25 mm cell quantization reads the ball resting on the
    // band's convex crease ring as a gouge past its 0.01 allowance,
    // and that warning blocks the post - the finding is reported on
    // the issue.)
    let world = world_for(0, flow(None, 25.0), Some(MICROARC));
    assert!(is_ok(&world), "the band generates on the stated A");

    let buried = build(0, flow(None, 8.0), Some(MICROARC));
    let err = generate_err_text(&buried, "OP");
    assert!(
        err.contains("does not clear the work") && err.contains("ball-center radius"),
        "{err}"
    );
    let e = model::find_by_name(&buried, "OP").expect("op");
    assert!(model::toolpath_of(&buried, &e).is_none(), "a refused band leaves no path");
}

#[test]
fn a_flat_tool_is_refused_by_name_on_the_band() {
    // The EM has no ball to rest a radial pose on: refused at the
    // create door, by name - the status Never.
    let world = camcraft_app::world::new_world();
    let mut h = History::default();
    let em = catalog().into_iter().find(|(n, _)| *n == "EM-0600-4FL").map(|(_, t)| t).expect("flat");
    h.execute(&world, AddTool::new(&world, "EM-0600-4FL", em).expect("tool"));
    h.execute(&world, ImportModel::from_mesh("bar", Mesh::from_facets(bar(2))));
    let ask = CreateOperation::new(&world, "OP", "EM-0600-4FL", band_picks(&world, 2, [2.0, 8.0]), flow(None, 25.0));
    let Err(CommandError::Invalid(msg)) = ask else {
        panic!("expected an invalid refusal");
    };
    assert!(msg.contains("ball-nose"), "{msg}");
}
