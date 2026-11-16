//! Law (f2), the posed-contact law, end to end: a LEANING cut claims
//! its ball rests on the part, so the verifier convicts a posed
//! program whose balls stand off the skin (the pivot-law failure: a
//! pose turned about the tip carries the ball `r·(a - up)` away) or
//! inside it, and stays silent on the generator's own correct path.
//! Every fixture here is a posed 5-axis contour over a standing bar
//! and a dome - the two shapes the pivot bug was measured on.
//!
//! Gate map (M4.4): covers sim::verify (sim::verify contact law, the
//! Envelope branch of engine, the all-air verdict `cuts_no_material`),
//! motion::multiaxis (contour5d stamp).
//! Lane shape (M4.4): structural (f2) verdicts run in seconds; the
//! full envelope scans ride the same binary because these fixtures
//! are small (a 64-gon bar, a 16x8 dome).

use camcraft_app::cl::{Move, MoveKind, Toolpath};
use camcraft_app::finishing3d::{self, Finish3DParams, FinishPattern};
use camcraft_app::geo2d::{Loop, Point2, Region};
use camcraft_app::geometry::{Point3, Triangle};
use camcraft_app::multiaxis::{contour5d, AxisControl};
use camcraft_app::verify::{Mode, ToolGeom, VerifyContext, Warning, cuts_no_material, verify};

const TAU: f64 = std::f64::consts::TAU;
const R: f64 = 3.0;
const TOL: f64 = 0.05;

fn p3(x: f64, y: f64, z: f64) -> Point3 {
    Point3::new(x, y, z)
}

fn revolve(n_circ: usize, meridian: &[(f64, f64)]) -> Vec<Triangle> {
    let v = |j: usize, r: f64, z: f64| {
        let th = TAU * j as f64 / n_circ as f64;
        p3(r * th.cos(), r * th.sin(), z)
    };
    let mut t = Vec::new();
    for seg in meridian.windows(2) {
        let ((ra, za), (rb, zb)) = (seg[0], seg[1]);
        for j in 0..n_circ {
            let (a1, a2, b1, b2) = (v(j, ra, za), v(j + 1, ra, za), v(j, rb, zb), v(j + 1, rb, zb));
            if ra <= 1e-12 {
                t.push(Triangle::new(a1, b2, b1));
            } else if rb <= 1e-12 {
                t.push(Triangle::new(a1, a2, b2));
            } else {
                t.push(Triangle::new(a1, a2, b2));
                t.push(Triangle::new(a1, b2, b1));
            }
        }
    }
    t
}

/// A 10-radius bar standing 20 tall, capped.
fn bar() -> Vec<Triangle> {
    revolve(64, &[(0.0, 0.0), (10.0, 0.0), (10.0, 20.0), (0.0, 20.0)])
}

fn finish(top_z: f64, bottom_z: f64) -> Finish3DParams {
    Finish3DParams {
        pattern: FinishPattern::Waterline,
        stepover: None,
        tool_diameter: 2.0 * R,
        tolerance: TOL,
        clearance_z: 40.0,
        retract_z: None,
        top_z,
        bottom_z,
        feed: 800.0,
        plunge_feed: 200.0,
        climb: true,
        ramp: false,
        min_step: None,
        ramp_angle: 45.0,
        entry_ramp: false,
        steep_stepover: None,
        pencil_reference: None,
    }
}

fn control() -> AxisControl {
    AxisControl {
        lead_deg: 15.0,
        lag_deg: 0.0,
        max_tilt_deg: Some(60.0),
    }
}

fn ball() -> ToolGeom {
    ToolGeom {
        radius: R,
        nose_radius: 0.0,
        flute_len: 20.0,
        shank_radius: R,
        shank_len: 30.0,
        holder_radius: 15.0,
        holder_len: 40.0,
    }
}

fn square(h: f64) -> Region {
    let pts = vec![Point2::new(-h, -h), Point2::new(h, -h), Point2::new(h, h), Point2::new(-h, h)];
    Region::new(vec![Loop::new(pts).expect("loop")]).expect("region")
}

fn ctx<'a>(region: &'a Region, facets: &'a [Triangle], tool: &'a ToolGeom) -> VerifyContext<'a> {
    VerifyContext {
        region,
        stock: None,
        model: Some(facets),
        tool,
        cutter: None,
        mode: Mode::Envelope,
        stock_top_z: 20.0,
        clearance_z: 40.0,
        stock_to_leave: 0.0,
        gouge_tol: 0.01,
        z_tol: 1e-6,
        finish_tol: Some(TOL),
        fixtures: &[],
        spindle: None,
        ..Default::default()
    }
}

/// The posed path with every LEANING move's tip pushed `d` along its
/// own pose (positive: the ball backs off the skin; negative: it
/// buries). Upright moves and rapids stand as generated.
fn shove(t: &Toolpath, d: f64) -> Toolpath {
    let mut prev_to: Option<Point3> = None;
    let moves = t
        .moves
        .iter()
        .map(|m| {
            let to = match m.axis {
                Some(a) if m.kind != MoveKind::Rapid && !camcraft_app::cl::is_vertical(&a) => {
                    p3(m.to.x + d * a[0], m.to.y + d * a[1], m.to.z + d * a[2])
                }
                _ => m.to,
            };
            let from = prev_to.unwrap_or(m.from);
            prev_to = Some(to);
            let arc = if to == m.to && from == m.from { m.arc } else { None };
            Move { from, to, arc, ..*m }
        })
        .collect();
    Toolpath::new(moves)
}

/// The pre-fix stamp's output, rebuilt: the upright plan's TIPS kept,
/// the leaning axes swapped in (the pose turned about the tip).
fn tip_pivot(t: &Toolpath) -> Toolpath {
    let mut held = [0.0, 0.0, 1.0];
    let mut prev_to: Option<Point3> = None;
    let moves = t
        .moves
        .iter()
        .map(|m| {
            if let Some(a) = m.axis {
                held = a;
            }
            // The ball the correct program cuts with is tip + r·held;
            // the tip-pivot program keeps that ball's UPRIGHT tip.
            let to = if camcraft_app::cl::is_vertical(&held) {
                m.to
            } else {
                p3(m.to.x + R * held[0], m.to.y + R * held[1], m.to.z + R * held[2] - R)
            };
            let from = prev_to.unwrap_or(m.from);
            prev_to = Some(to);
            let arc = if to == m.to && from == m.from { m.arc } else { None };
            Move { from, to, arc, ..*m }
        })
        .collect();
    Toolpath::new(moves)
}

fn envelope(w: &[Warning]) -> Option<(f64, f64)> {
    w.iter().find_map(|w| match w {
        Warning::Envelope { gouge_mm, rest_mm, .. } => Some((*gouge_mm, *rest_mm)),
        _ => None,
    })
}

#[test]
fn a_correct_posed_contour_verifies_with_zero_warnings() {
    let f = bar();
    let t = contour5d(&f, &finish(19.0, 2.0), &control(), None).expect("generates");
    assert!(!t.is_prismatic(), "the bar's rings must lean");
    let (region, tool) = (square(25.0), ball());
    let w = verify(&t, &ctx(&region, &f, &tool));
    // The ball's verdicts: no envelope, gouge or unjudged warning at
    // all. (This hand-built context stands a square of raw stock round
    // the bar, so the reach laws rightly see the leaning holder sweep
    // uncut stock; the whole-verdict zero rides the operation suite,
    // whose finishing worlds buy their stock from the model.)
    let ball_verdicts: Vec<_> = w
        .iter()
        .filter(|w| matches!(w, Warning::Envelope { .. } | Warning::Gouge { .. } | Warning::RestUnjudged { .. } | Warning::NonFinite { .. }))
        .collect();
    assert!(ball_verdicts.is_empty(), "the pivot-law program must verify silent: {ball_verdicts:?}");
}

#[test]
fn a_tip_pivoted_program_is_convicted_as_rest() {
    // The bug itself, rebuilt from the correct program: tips kept,
    // axes swapped. Every leaning ball stands most of a radius off
    // the wall - and the posed-contact law names it.
    let f = bar();
    let t = contour5d(&f, &finish(19.0, 2.0), &control(), None).expect("generates");
    let bad = tip_pivot(&t);
    let (region, tool) = (square(25.0), ball());
    let w = verify(&bad, &ctx(&region, &f, &tool));
    let (_, rest) = envelope(&w).unwrap_or_else(|| panic!("the tip-pivot bug verified silent: {w:?}"));
    assert!(rest > 1.0, "the measured air should be most of a radius, got {rest}");
}

#[test]
fn a_ball_backed_off_its_skin_is_convicted_and_a_buried_one_gouges() {
    let f = bar();
    let t = contour5d(&f, &finish(19.0, 2.0), &control(), None).expect("generates");
    let (region, tool) = (square(25.0), ball());
    // 0.5 mm of air at every claimed contact: rest, by name.
    let w = verify(&shove(&t, 0.5), &ctx(&region, &f, &tool));
    let (_, rest) = envelope(&w).unwrap_or_else(|| panic!("an air finish verified silent: {w:?}"));
    assert!((rest - 0.5).abs() < 0.05, "rest {rest} should read the 0.5 mm stand-off");
    // 0.3 mm into the wall: the facet gouge law convicts.
    let w = verify(&shove(&t, -0.3), &ctx(&region, &f, &tool));
    let (gouge, _) = envelope(&w).unwrap_or_else(|| panic!("a buried ball verified silent: {w:?}"));
    assert!(gouge > 0.2, "gouge {gouge} should read the 0.3 mm bite");
}

#[test]
fn the_upright_finish_is_not_judged_by_the_contact_law() {
    // An upright finish claims nothing: its own waterline verifies
    // silent, and lifting it into the air is the ridge law's and
    // rest machining's question, never (f2)'s - the law is scoped
    // to the leans that state contact.
    let f = bar();
    let t = finishing3d::waterline(&f, &finish(19.0, 2.0)).expect("generates");
    let (region, tool) = (square(25.0), ball());
    assert_eq!(verify(&t, &ctx(&region, &f, &tool)), Vec::new());
}

/// `t` with every move lifted `dz`.
fn lift(t: &Toolpath, dz: f64) -> Toolpath {
    let moves = t
        .moves
        .iter()
        .map(|m| Move { from: p3(m.from.x, m.from.y, m.from.z + dz), to: p3(m.to.x, m.to.y, m.to.z + dz), ..*m })
        .collect();
    Toolpath::new(moves)
}

#[test]
fn an_upright_all_air_finish_is_judged_clean_and_cutting_nothing() {
    // The M13.8 owed item: an upright finish lifted wholly above the
    // material verifies silent - no ball meets the part, so neither
    // the facet gouge law nor the ridge rest law speaks, and (f2) is
    // scoped to leans. That silence was unjudged. The all-air verdict
    // now says it positively: clean, cuts nothing.
    let f = bar();
    let t = finishing3d::waterline(&f, &finish(19.0, 2.0)).expect("generates");
    let (region, tool) = (square(25.0), ball());
    let c = ctx(&region, &f, &tool);
    // The bar's top and the stock top are both Z 20; the lowest ring
    // is at Z 2, so a 30 mm lift puts every cut at Z >= 32.
    let air = lift(&t, 30.0);
    assert_eq!(verify(&air, &c), Vec::new(), "an all-air pass gouges and leaves nothing it was near");
    assert!(cuts_no_material(&air, c.model, c.stock_top_z, &tool, c.gouge_tol), "the all-air pass is proven clean");

    // The finish on the part is NOT all-air: it cuts.
    assert!(!cuts_no_material(&t, c.model, c.stock_top_z, &tool, c.gouge_tol));
    // A lift that leaves the low rings inside the stock's height is
    // not proven either (it may cut stock above the part).
    assert!(!cuts_no_material(&lift(&t, 10.0), c.model, c.stock_top_z, &tool, c.gouge_tol));
    // One cut back down at the part ends the proof.
    let mut one = air.clone();
    if let Some(k) = one.moves.iter().position(|m| m.kind != MoveKind::Rapid) {
        one.moves[k].to.z = 19.0;
    }
    assert!(!cuts_no_material(&one, c.model, c.stock_top_z, &tool, c.gouge_tol));
    // A posed all-air pass is not this law's: a leaning stack can
    // reach below its tip's plane, and (f2) judges leans.
    let mut posed = air.clone();
    for m in &mut posed.moves {
        m.axis = Some([0.5_f64.sqrt(), 0.0, 0.5_f64.sqrt()]);
    }
    assert!(!cuts_no_material(&posed, c.model, c.stock_top_z, &tool, c.gouge_tol));
    // No cut at all states nothing.
    assert!(!cuts_no_material(&Toolpath::new(Vec::new()), c.model, c.stock_top_z, &tool, c.gouge_tol));
}
