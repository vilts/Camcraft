//! Control-side cutter compensation on the contour (M13.11): the
//! default keeps every centre move byte for byte and records no span;
//! Control on straight leads records one span per ring whose programmed
//! geometry is the PART contour between the same leads; every shape
//! the control cannot carry is refused by name.
//!
//! Gate map (M4.4): covers camcraft_motion::machining2d (contour,
//! contour_comp), camcraft_motion::cl (CompSpan), camcraft_motion::links.

use camcraft_motion::cl::{CompSide, MoveKind, Toolpath};
use camcraft_motion::entry::Entry3D;
use camcraft_motion::geo2d::{Loop, Point2, Region};
use camcraft_motion::links::{Approach, Compensation, Link, Links};
use camcraft_motion::machining2d::{contour, ContourParams, Lead, Side};

fn lp(pts: &[(f64, f64)]) -> Loop {
    Loop::new(pts.iter().map(|&(x, y)| Point2::new(x, y)).collect()).unwrap()
}

fn square() -> Region {
    Region::new(vec![lp(&[(0.0, 0.0), (40.0, 0.0), (40.0, 40.0), (0.0, 40.0)])]).unwrap()
}

fn params(side: Side, lead: Lead, climb: bool, comp: Compensation) -> ContourParams {
    ContourParams {
        tool_diameter: 6.0,
        stock_to_leave: 0.0,
        clearance_z: 5.0,
        retract_z: 2.0,
        top_z: 0.0,
        bottom_z: -4.0,
        stepdown: 2.0,
        feed: 500.0,
        plunge_feed: 100.0,
        climb,
        side,
        lead,
        tolerance: 0.01,
        tab_count: 0,
        tab_width: 0.0,
        tab_height: 0.0,
        entry: Entry3D::Plunge,
        helix_radius: 1.0,
        blend_turns: false,
        links: Links { approach: Approach { compensation: comp, ..Approach::DEFAULT }, ..Links::PLANAR },
    }
}

/// Distance from `p` to the 40 x 40 square's boundary.
fn to_square(p: Point2) -> f64 {
    let dx = (p.x - 0.0).abs().min((p.x - 40.0).abs());
    let dy = (p.y - 0.0).abs().min((p.y - 40.0).abs());
    let inside = (0.0..=40.0).contains(&p.x) && (0.0..=40.0).contains(&p.y);
    if inside {
        dx.min(dy)
    } else {
        let ox = if p.x < 0.0 { -p.x } else if p.x > 40.0 { p.x - 40.0 } else { 0.0 };
        let oy = if p.y < 0.0 { -p.y } else if p.y > 40.0 { p.y - 40.0 } else { 0.0 };
        ox.hypot(oy)
    }
}

/// The distinct planes the path cuts along.
fn cut_levels(t: &Toolpath) -> Vec<f64> {
    let mut zs: Vec<f64> = Vec::new();
    for m in &t.moves {
        if m.kind == MoveKind::Feed && m.from.z == m.to.z && !zs.contains(&m.to.z) {
            zs.push(m.to.z);
        }
    }
    zs
}

fn spans_are_the_part_contour(t: &Toolpath, side: CompSide) {
    let levels = cut_levels(t);
    assert!(levels.len() >= 2, "a multi-level ladder: {levels:?}");
    assert_eq!(t.comps.len(), levels.len(), "one span per level: {:?}", t.notes);
    for c in &t.comps {
        assert_eq!(c.side, side);
        assert_eq!(c.radius, 3.0);
        assert!(c.is_sane(&t.moves));
        let n = c.programmed.len();
        assert!(n >= 6, "lead, four sides, lead: {n}");
        // The engage and cancel are the same leads: same start, same end.
        assert_eq!(c.programmed[0].from, t.moves[c.first].from);
        assert_eq!(c.programmed[n - 1].to, t.moves[c.last].to);
        assert!(c.programmed[0].arc.is_none() && c.programmed[n - 1].arc.is_none());
        // Between them the programmed walk rides the PART contour...
        for m in &c.programmed[1..n - 1] {
            assert!(to_square(Point2::new(m.to.x, m.to.y)) <= 0.011, "programmed vertex off the part: {:?}", m.to);
            assert_eq!(m.kind, MoveKind::Feed);
            assert_eq!(m.to.z, t.moves[c.first].to.z);
        }
        // ...while the centre moves it replaces stand one radius off it.
        for m in &t.moves[c.first + 1..c.last] {
            let d = to_square(Point2::new(m.to.x, m.to.y));
            assert!((d - 3.0).abs() <= 0.011, "centre vertex not one radius off the part: {:?} ({d})", m.to);
        }
        // The seam sits mid-run: straight cuts either side of it.
        assert!(t.moves[c.first + 1].arc.is_none() && t.moves[c.last - 1].arc.is_none());
        // The engage lead is straight and at least a radius long.
        let e = t.moves[c.first];
        assert!((e.to.x - e.from.x).hypot(e.to.y - e.from.y) >= 3.0 - 1e-9);
    }
}

#[test]
fn computer_is_the_default_and_control_walks_the_same_ring_seamed_mid_run() {
    let lead = Lead::Linear { length: 8.0 };
    let a = contour(&square(), &params(Side::Auto, lead, true, Compensation::Computer)).unwrap();
    assert!(a.comps.is_empty(), "in computer records no span");
    assert_eq!(Approach::DEFAULT.compensation, Compensation::Computer);
    let b = contour(&square(), &params(Side::Auto, lead, true, Compensation::Control)).unwrap();
    // The same ring, the same number of cut levels, the same leads -
    // seamed mid-run, so the engage and cancel offsets coincide; the
    // centre moves stay one radius off the part (the helper's law) and
    // the computer path (every golden) is untouched by the option.
    assert_eq!(cut_levels(&a), cut_levels(&b));
    assert_eq!(a.notes, b.notes);
    // #1555: climb keeps the material right of travel - the tool left, G41.
    spans_are_the_part_contour(&b, CompSide::Left);
    // The engage lead is collinear with the first cut: the control's
    // offset at its end is square to that cut, where the centre stands.
    for c in &b.comps {
        let (e, w) = (b.moves[c.first], b.moves[c.first + 1]);
        let (ex, ey) = (e.to.x - e.from.x, e.to.y - e.from.y);
        let (wx, wy) = (w.to.x - w.from.x, w.to.y - w.from.y);
        assert!((ex * wy - ey * wx).abs() < 1e-6, "lead and first cut collinear");
    }
}

#[test]
fn the_side_follows_the_travel_and_the_contour_side() {
    let lead = Lead::Perpendicular { length: 5.0 };
    let conv = contour(&square(), &params(Side::Outside, lead, false, Compensation::Control)).unwrap();
    spans_are_the_part_contour(&conv, CompSide::Right);
    let bore = contour(&square(), &params(Side::Inside, lead, true, Compensation::Control)).unwrap();
    spans_are_the_part_contour(&bore, CompSide::Left);
    let bore_conv = contour(&square(), &params(Side::Inside, lead, false, Compensation::Control)).unwrap();
    spans_are_the_part_contour(&bore_conv, CompSide::Right);
}

#[test]
fn stock_to_leave_rides_the_programmed_contour_not_the_register() {
    let mut p = params(Side::Outside, Lead::Linear { length: 8.0 }, true, Compensation::Control);
    p.stock_to_leave = 0.5;
    let t = contour(&square(), &p).unwrap();
    assert_eq!(t.comps.len(), cut_levels(&t).len());
    for c in &t.comps {
        assert_eq!(c.radius, 3.0, "the register holds the nominal radius");
        let n = c.programmed.len();
        for m in &c.programmed[1..n - 1] {
            let d = to_square(Point2::new(m.to.x, m.to.y));
            assert!((d - 0.5).abs() <= 0.011, "the programmed contour carries the leave: {:?} ({d})", m.to);
        }
    }
}

fn refusal(p: &ContourParams) -> String {
    match contour(&square(), p) {
        Ok(t) => panic!("expected a refusal, got {} moves with {} spans", t.moves.len(), t.comps.len()),
        Err(e) => e.to_string(),
    }
}

#[test]
fn every_lead_the_control_cannot_engage_on_is_refused_by_name() {
    let arc = refusal(&params(Side::Auto, Lead::Arc { radius: 3.0 }, true, Compensation::Control));
    assert!(arc.contains("control compensation needs a straight lead-in"), "{arc}");
    let tang = refusal(&params(Side::Auto, Lead::Tangential { radius: 3.0 }, true, Compensation::Control));
    assert!(tang.contains("straight lead-in"), "{tang}");
    let none = refusal(&params(Side::Auto, Lead::None, true, Compensation::Control));
    assert!(none.contains("straight lead-in"), "{none}");
    let short = refusal(&params(Side::Auto, Lead::Linear { length: 2.0 }, true, Compensation::Control));
    assert!(short.contains("at least the tool radius (3.000 mm)") && short.contains("2.000 mm"), "{short}");
    // The lead-out is judged the same way, on its own shape.
    let mut p = params(Side::Auto, Lead::Linear { length: 8.0 }, true, Compensation::Control);
    p.links.approach.lead_out = Some(Lead::Arc { radius: 3.0 });
    let out = refusal(&p);
    assert!(out.contains("straight lead-out"), "{out}");
    // And the same parameters in computer generate.
    for lead in [Lead::Arc { radius: 3.0 }, Lead::None, Lead::Linear { length: 2.0 }] {
        contour(&square(), &params(Side::Auto, lead, true, Compensation::Computer)).unwrap();
    }
}

/// Control comp with a Ø10 tool on `side`.
fn d10(side: Side) -> ContourParams {
    ContourParams { tool_diameter: 10.0, ..params(side, Lead::Linear { length: 8.0 }, true, Compensation::Control) }
}

fn refused_on(region: &Region, p: &ContourParams) -> String {
    match contour(region, p) {
        Ok(t) => panic!("expected a refusal, got {} moves with {} spans", t.moves.len(), t.comps.len()),
        Err(e) => e.to_string(),
    }
}

#[test]
fn sharp_inside_corners_between_long_edges_still_ride_the_control() {
    // An L boss: its inner corner turns toward the tool, its edges are
    // far longer than the r * tan(45 deg) the offset trims off them.
    let ell = Region::new(vec![lp(&[(0.0, 0.0), (60.0, 0.0), (60.0, 20.0), (20.0, 20.0), (20.0, 60.0), (0.0, 60.0)])]).unwrap();
    let t = contour(&ell, &d10(Side::Outside)).unwrap();
    assert_eq!(t.comps.len(), cut_levels(&t).len(), "{:?}", t.notes);
    // A square pocket: four sharp inside corners, the control rounds them.
    let t = contour(&square(), &d10(Side::Inside)).unwrap();
    assert_eq!(t.comps.len(), cut_levels(&t).len(), "{:?}", t.notes);
}

/// A 40 x 40 pocket with corner fillets of radius `rf`, 8 facets each.
fn filleted(rf: f64) -> Region {
    let s = 40.0f64;
    let mut pts = Vec::new();
    for (cx, cy, a0) in [(s - rf, rf, -90.0f64), (s - rf, s - rf, 0.0), (rf, s - rf, 90.0), (rf, rf, 180.0)] {
        for k in 0..=8 {
            let a = (a0 + 90.0 * k as f64 / 8.0).to_radians();
            pts.push((cx + rf * a.cos(), cy + rf * a.sin()));
        }
    }
    Region::new(vec![lp(&pts)]).unwrap()
}

#[test]
fn an_inside_fillet_tighter_than_the_tool_is_refused_by_name() {
    // A fillet at least the tool radius offsets cleanly: the control cuts it.
    let t = contour(&filleted(6.0), &d10(Side::Inside)).unwrap();
    assert_eq!(t.comps.len(), cut_levels(&t).len(), "{:?}", t.notes);
    // r 2 fillets under a Ø10 tool cannot be offset by the register.
    let pocket = filleted(2.0);
    let why = refused_on(&pocket, &d10(Side::Inside));
    assert!(why.contains("control compensation cannot cut the inside corner at ("), "{why}");
    assert!(why.contains("its radius 2.0") || why.contains("its radius 1.9"), "{why}");
    assert!(why.contains("under the tool's 5.000 mm") && why.contains("Compensation: In computer"), "{why}");
    // The same pocket in computer generates (the centre path rounds it).
    let mut p = d10(Side::Inside);
    p.links.approach.compensation = Compensation::Computer;
    contour(&pocket, &p).unwrap();
}

#[test]
fn a_neck_narrower_than_the_tool_is_refused_by_name() {
    // Two 30 x 30 lobes joined by a 6 mm neck, Ø10 tool: the centre
    // offset splits in two, the control's offset of the one programmed
    // ring would cross itself in the neck.
    let bell = Region::new(vec![lp(&[
        (0.0, 0.0), (30.0, 0.0), (30.0, 12.0), (40.0, 12.0), (40.0, 0.0), (70.0, 0.0),
        (70.0, 30.0), (40.0, 30.0), (40.0, 18.0), (30.0, 18.0), (30.0, 30.0), (0.0, 30.0),
    ])])
    .unwrap();
    let why = refused_on(&bell, &d10(Side::Inside));
    assert!(why.contains("control compensation cannot cut the gap at (35.000, 15.000)"), "{why}");
    assert!(why.contains("narrower than the tool's 10.000 mm diameter") && why.contains("In computer"), "{why}");
}

#[test]
fn tabs_blends_overlap_and_keep_down_links_are_refused_by_name() {
    let base = params(Side::Auto, Lead::Linear { length: 8.0 }, true, Compensation::Control);
    let tabs = refusal(&ContourParams { tab_count: 2, tab_width: 4.0, tab_height: 1.0, ..base });
    assert!(tabs.contains("does not ride tabs"), "{tabs}");
    let blend = refusal(&ContourParams { blend_turns: true, ..base });
    assert!(blend.contains("corner blends"), "{blend}");
    let mut ov = base;
    ov.links.approach.overlap = 2.0;
    let ov = refusal(&ov);
    assert!(ov.contains("overlap"), "{ov}");
    let mut kd = base;
    kd.links.level = Link::KeepDown;
    kd.links.transition = Some(Entry3D::Ramp);
    let kd = refusal(&kd);
    assert!(kd.contains("keep-down"), "{kd}");
}
