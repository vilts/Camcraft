//! The trochoidal slot (issue #220 slice 1), measured: the loops' contact
//! angle read on the dexel yardstick (`camcraft_motion::quality`, the
//! Adaptive pins' own stock) against the straight slot's full 180, the
//! verifier's laws on the loops (no crossing, no gouge), the lifted
//! returns landing on air, and the refusals by name.
//!
//! Gate map (M4.4): covers camcraft_motion::machining2d (trochoid_slot,
//! trochoid_loops), camcraft_motion::quality.

use camcraft_motion::cl::{MoveKind, Toolpath};
use camcraft_motion::geo2d::{Point2, Region};
use camcraft_motion::machining2d::{
    lifted_cut, plan_trochoidal_slot, slot, trochoidal_slot, GenError, SlotParams, TrochoidParams, TrochoidReturn,
    DEFAULT_MICROLIFT,
};
use camcraft_motion::quality::{measure, stock_for, PathQuality, QualitySpec};
use camcraft_motion::verify::{verify, Mode, ToolGeom, VerifyContext, Warning};

const D: f64 = 10.0;
const W: f64 = 16.0;
const BOTTOM: f64 = -6.0;

fn line() -> Vec<Point2> {
    vec![Point2::new(0.0, 0.0), Point2::new(40.0, 0.0)]
}

fn params() -> TrochoidParams {
    TrochoidParams {
        tool_diameter: D,
        width: W,
        loop_width: None,
        step: 1.0,
        max_engagement: 90.0,
        ret: TrochoidReturn::Microlift { height: DEFAULT_MICROLIFT },
        smoothing: 1.0,
        stock_to_leave: 0.0,
        clearance_z: 10.0,
        retract_z: Some(2.0),
        top_z: 0.0,
        bottom_z: BOTTOM,
        stepdown: 3.0,
        feed: 1200.0,
        return_feed: 3000.0,
        plunge_feed: 300.0,
        climb: true,
    }
}

fn channel(w: f64) -> Region {
    let Ok(r) = Region::sweep(&[line()], 0.5 * w) else { panic!("channel") };
    r
}

/// The path read on a fresh block of the channel standing to Z 0.
fn measured(path: &Toolpath, w: f64) -> PathQuality {
    let spec = QualitySpec::new(D);
    let Ok(mut dex) = stock_for(&channel(w), 0.0, BOTTOM, D, &spec) else { panic!("stock") };
    let Ok(q) = measure(path, &mut dex, &spec) else { panic!("measure") };
    q
}

fn verified(path: &Toolpath, w: f64) -> Vec<Warning> {
    let reg = channel(w);
    let tool = ToolGeom {
        radius: D * 0.5,
        nose_radius: 0.0,
        flute_len: 30.0,
        shank_radius: D * 0.5,
        shank_len: 40.0,
        holder_radius: 20.0,
        holder_len: 60.0,
    };
    let cx = VerifyContext {
        region: &reg,
        stock: None,
        model: None,
        tool: &tool,
        cutter: None,
        mode: Mode::Area,
        stock_top_z: 0.0,
        clearance_z: 10.0,
        stock_to_leave: 0.0,
        gouge_tol: 0.01,
        z_tol: 1e-6,
        finish_tol: None,
        fixtures: &[],
        spindle: None,
        ..Default::default()
    };
    verify(path, &cx)
}

fn plan(p: &TrochoidParams) -> camcraft_motion::machining2d::TrochoidPlan {
    match plan_trochoidal_slot(&line(), p) {
        Ok(pl) => pl,
        Err(e) => panic!("trochoidal slot refused: {e}"),
    }
}

/// The full-width slot: the loops hold the limit on the yardstick, where
/// the straight slot along the same line runs at 180 degrees.
#[test]
fn trochoid_full_width_slot_holds_the_limit_where_a_straight_slot_reads_180() {
    let pl = plan(&params());
    let q = measured(&pl.path, W);
    eprintln!(
        "trochoid: {} loops, step {:.3}, rho {:.3}, max {:.1} deg at {:?} (planner {:.1}), lift {:.1} mm",
        pl.loops, pl.step, pl.loop_radius, q.max_deg, q.max_at, pl.max_engagement, q.lift_len
    );
    assert!(q.max_deg <= 90.0 + 1e-9, "max {:.1} deg at {:?}", q.max_deg, q.max_at);
    assert!(q.slot_len == 0.0, "{:.2} mm of the loops slotted", q.slot_len);
    assert!(pl.loops >= 30, "{} loops", pl.loops);
    let sp = SlotParams {
        tool_diameter: D,
        width: W,
        stock_to_leave: 0.0,
        clearance_z: 10.0,
        retract_z: Some(2.0),
        top_z: 0.0,
        bottom_z: BOTTOM,
        stepdown: 3.0,
        stepover: 3.0,
        feed: 1200.0,
        plunge_feed: 300.0,
        climb: true,
        entry: camcraft_motion::entry::Entry3D::Plunge,
        helix_radius: 0.0,
        links: camcraft_motion::links::Links::PLANAR,
    };
    let Ok(straight) = slot(Point2::new(0.0, 0.0), Point2::new(40.0, 0.0), &sp) else { panic!("slot") };
    let s = measured(&straight, W);
    eprintln!("straight slot: max {:.1} deg, {:.1} mm at 170+", s.max_deg, s.slot_len);
    assert!(s.max_deg >= 175.0, "the straight slot read {:.1} deg", s.max_deg);
}

/// The measured trace stays under a tighter limit too: the planner cuts
/// the step until it does, and says so (a short channel keeps the
/// ladder's walk inside the debug lane).
#[test]
fn trochoid_step_is_cut_until_a_tight_limit_holds() {
    let mut p = params();
    p.step = 2.0;
    p.max_engagement = 60.0;
    p.bottom_z = -3.0;
    let short = vec![Point2::new(0.0, 0.0), Point2::new(15.0, 0.0)];
    let pl = match plan_trochoidal_slot(&short, &p) {
        Ok(pl) => pl,
        Err(e) => panic!("trochoidal slot refused: {e}"),
    };
    let Ok(ch) = Region::sweep(&[short], 0.5 * W) else { panic!("channel") };
    let spec = QualitySpec::new(D);
    let Ok(mut dex) = stock_for(&ch, 0.0, BOTTOM, D, &spec) else { panic!("stock") };
    let Ok(q) = measure(&pl.path, &mut dex, &spec) else { panic!("measure") };
    assert!(pl.step < 2.0, "step {}", pl.step);
    assert!(q.max_deg <= 60.0 + 1e-9, "max {:.1} deg at {:?}", q.max_deg, q.max_at);
    assert!(pl.path.notes.iter().any(|n| n.contains("forward step was cut")), "{:?}", pl.path.notes);
}

/// The verifier's laws hold on the loops: no Feed crossing at a shared
/// depth, no gouge past the channel walls - climb and conventional,
/// microlift and retract returns.
#[test]
fn trochoid_loops_pass_the_verifier() {
    for climb in [true, false] {
        for ret in [TrochoidReturn::Microlift { height: DEFAULT_MICROLIFT }, TrochoidReturn::Retract] {
            let mut p = params();
            p.climb = climb;
            p.ret = ret;
            let pl = plan(&p);
            let w = verified(&pl.path, W);
            let bad: Vec<&Warning> = w
                .iter()
                .filter(|x| matches!(x, Warning::Crossing { .. } | Warning::Gouge { .. } | Warning::RapidInStock { .. }))
                .collect();
            assert!(bad.is_empty(), "climb {climb}, {ret:?}: {bad:?}");
        }
    }
}

/// Every lifted return lands on air: read inside the cutter's disc on
/// the dexel stock the instant before it runs (the rim reading grazes
/// the face the loop just cut and is no collision).
#[test]
fn trochoid_microlift_returns_land_on_air() {
    let pl = plan(&params());
    let spec = QualitySpec::new(D);
    let Ok(mut dex) = stock_for(&channel(W), 0.0, BOTTOM, D, &spec) else { panic!("stock") };
    assert!(matches!(lifted_cut(&pl.path, &mut dex, 0.5 * D), Ok(None)));
    let lifted = pl
        .path
        .moves
        .iter()
        .filter(|m| m.kind == MoveKind::Feed && (m.from.z - m.to.z).abs() < 1e-9)
        .filter(|m| [0.0, -3.0, -6.0].iter().any(|z| (m.to.z - (z + DEFAULT_MICROLIFT)).abs() < 1e-9))
        .count();
    assert!(lifted >= 2 * pl.loops, "{lifted} lifted moves for {} loops", pl.loops);
    // The law bites: the same returns dropped onto the floor's plane
    // before the loop they retrace is cut stand in stock.
    let mut early = pl.path.clone();
    early.moves.retain(|m| !(m.kind == MoveKind::Feed && m.arc.is_some() && (m.to.z - (-3.0)).abs() < 1e-9));
    let Ok(mut dex) = stock_for(&channel(W), 0.0, BOTTOM, D, &spec) else { panic!("stock") };
    assert!(matches!(lifted_cut(&early, &mut dex, 0.5 * D), Ok(Some(_))), "a return over uncut stock must be caught");
}

/// A channel no wider than the tool is refused by name, as is a loop
/// band wider than the channel leaves and a limit no step can hold.
#[test]
fn trochoid_refuses_a_channel_narrower_than_the_tool_by_name() {
    let mut p = params();
    p.width = 9.5;
    match trochoidal_slot(&line(), &p) {
        Err(GenError::Invalid(m)) => assert!(m.contains("too narrow") && m.contains("Slot"), "{m}"),
        other => panic!("expected the narrow-channel refusal, got {other:?}"),
    }
    let mut p = params();
    p.loop_width = Some(7.0);
    match trochoidal_slot(&line(), &p) {
        Err(GenError::Invalid(m)) => assert!(m.contains("loop width"), "{m}"),
        other => panic!("expected the loop-width refusal, got {other:?}"),
    }
    let mut p = params();
    p.max_engagement = 5.0;
    match trochoidal_slot(&line(), &p) {
        Err(GenError::Invalid(m)) => assert!(m.contains("engagement limit"), "{m}"),
        other => panic!("expected the engagement refusal, got {other:?}"),
    }
}

/// A narrower loop band leaves the walls standing and says how much.
#[test]
fn trochoid_narrow_band_names_the_wall_stock() {
    let mut p = params();
    p.loop_width = Some(4.0);
    let pl = plan(&p);
    assert!(pl.path.notes.iter().any(|n| n.contains("short of each wall")), "{:?}", pl.path.notes);
}

/// The loops are arcs: every front arc, fillet and retrace posts as one
/// G2/G3 move, not a chord run.
#[test]
fn trochoid_loops_post_as_arcs() {
    let pl = plan(&params());
    assert!(pl.path.arc_count() > 4 * pl.loops, "{} arcs for {} loops", pl.path.arc_count(), pl.loops);
}


/// #774: a forward step so short that every rung of the ladder needs
/// more loops than the cap is refused by that cause - the cap and the
/// step named - not as an engagement reading that was never taken.
#[test]
fn trochoid_step_past_the_loop_cap_is_refused_by_that_cause() {
    let mut p = params();
    p.step = 0.0001;
    match trochoidal_slot(&line(), &p) {
        Err(GenError::Invalid(m)) => assert!(
            m.contains("forward step is too small") && m.contains("20000 loop cap") && m.contains("0.0001 mm"),
            "{m}"
        ),
        other => panic!("expected the loop-cap refusal, got {other:?}"),
    }
}

/// #774: a microlift is a feed move, so it never lifts the return above
/// the clearance plane - a lift past the clearance above the top is
/// refused by name; one right at it still cuts.
#[test]
fn trochoid_microlift_above_the_clearance_plane_is_refused() {
    let mut p = params();
    p.ret = TrochoidReturn::Microlift { height: 100.0 };
    match trochoidal_slot(&line(), &p) {
        Err(GenError::Invalid(m)) => assert!(m.contains("microlift") && m.contains("clearance plane"), "{m}"),
        other => panic!("expected the microlift refusal, got {other:?}"),
    }
    let mut p = params();
    p.ret = TrochoidReturn::Microlift { height: p.clearance_z - p.top_z };
    assert!(trochoidal_slot(&line(), &p).is_ok(), "a lift up to the clearance plane is allowed");
}

/// #774: a bore descent needing more helix turns than the cap is refused
/// by name, never cut as the one turn the old fallback substituted.
#[test]
fn trochoid_bore_past_the_helix_turn_cap_is_refused() {
    let mut p = params();
    // The full 6 mm band (3 mm helix radius: 6 mm of drop a turn) over
    // one 6200 mm level wants ~1034 turns, past the 1024 cap.
    p.bottom_z = -6200.0;
    p.stepdown = 6200.0;
    match trochoidal_slot(&line(), &p) {
        // #871: the levels built before the refusal ride it as a ghost.
        Err(GenError::Invalid(m) | GenError::Ghosted { msg: m, .. }) => {
            assert!(m.contains("helix") && m.contains("1024 turns"), "{m}")
        }
        other => panic!("expected the helix-turn refusal, got {other:?}"),
    }
}
