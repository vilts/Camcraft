//! Wear cutter compensation on the contour (#38): the centre path is
//! walked exactly as under Control (same ring, same mid-run seam, same
//! leads) and each ring records a span flagged `wear` whose programmed
//! moves ARE those centre moves - the register then holds only the
//! wear delta. The lead law and the refusals are Control's, named for
//! wear.
//!
//! Gate map (M4.4): covers camcraft_motion::machining2d (contour,
//! contour_comp), camcraft_motion::cl (CompSpan), camcraft_motion::links.

use camcraft_motion::cl::{CompSide, MoveKind, Toolpath};
use camcraft_motion::entry::Entry3D;
use camcraft_motion::geo2d::{Loop, Point2, Region};
use camcraft_motion::links::{Approach, Compensation, Link, Links};
use camcraft_motion::machining2d::{contour, ContourParams, Lead, Side};

fn square() -> Region {
    let pts = [(0.0, 0.0), (40.0, 0.0), (40.0, 40.0), (0.0, 40.0)];
    Region::new(vec![Loop::new(pts.iter().map(|&(x, y)| Point2::new(x, y)).collect()).unwrap()]).unwrap()
}

fn params(lead: Lead, climb: bool, comp: Compensation) -> ContourParams {
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
        side: Side::Auto,
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

fn levels(t: &Toolpath) -> usize {
    let mut zs: Vec<f64> = Vec::new();
    for m in &t.moves {
        if m.kind == MoveKind::Feed && m.from.z == m.to.z && !zs.contains(&m.to.z) {
            zs.push(m.to.z);
        }
    }
    zs.len()
}

fn refusal(p: &ContourParams) -> String {
    match contour(&square(), p) {
        Ok(t) => panic!("expected a refusal, got {} moves with {} spans", t.moves.len(), t.comps.len()),
        Err(e) => e.to_string(),
    }
}

#[test]
fn wear_walks_controls_centre_path_and_programs_it_verbatim() {
    let lead = Lead::Linear { length: 8.0 };
    let wear = contour(&square(), &params(lead, true, Compensation::Wear)).unwrap();
    let control = contour(&square(), &params(lead, true, Compensation::Control)).unwrap();
    // The machine runs the same centre moves in both modes.
    assert_eq!(wear.moves, control.moves);
    assert!(levels(&wear) >= 2);
    assert_eq!(wear.comps.len(), levels(&wear), "one span per level: {:?}", wear.notes);
    for (w, c) in wear.comps.iter().zip(&control.comps) {
        assert!(w.wear && !c.wear);
        assert_eq!((w.side, w.first, w.last, w.radius), (c.side, c.first, c.last, c.radius));
        assert_eq!(w.side, CompSide::Left, "climb keeps the centre left of the part (G41, #1555)");
        assert!(w.is_sane(&wear.moves));
        // The programmed text is the centre walk itself.
        assert_eq!(w.programmed.as_slice(), &wear.moves[w.first..=w.last]);
    }
    // Conventional travel engages G41.
    let conv = contour(&square(), &params(lead, false, Compensation::Wear)).unwrap();
    assert!(!conv.comps.is_empty() && conv.comps.iter().all(|c| c.side == CompSide::Right && c.wear));
}

#[test]
fn wear_keeps_the_control_lead_law_and_refuses_by_its_own_name() {
    let arc = refusal(&params(Lead::Arc { radius: 3.0 }, true, Compensation::Wear));
    assert!(arc.contains("wear compensation needs a straight lead-in"), "{arc}");
    let none = refusal(&params(Lead::None, true, Compensation::Wear));
    assert!(none.contains("wear compensation") && none.contains("straight lead-in"), "{none}");
    let short = refusal(&params(Lead::Linear { length: 2.0 }, true, Compensation::Wear));
    assert!(short.contains("at least the tool radius (3.000 mm)"), "{short}");
    let mut p = params(Lead::Linear { length: 8.0 }, true, Compensation::Wear);
    p.links.approach.lead_out = Some(Lead::Arc { radius: 3.0 });
    assert!(refusal(&p).contains("wear compensation needs a straight lead-out"));
    let mut p = params(Lead::Linear { length: 8.0 }, true, Compensation::Wear);
    p.tab_count = 2;
    p.tab_width = 4.0;
    p.tab_height = 1.0;
    assert!(refusal(&p).contains("wear compensation does not ride tabs"));
    let mut p = params(Lead::Linear { length: 8.0 }, true, Compensation::Wear);
    p.links.level = Link::KeepDown;
    assert!(refusal(&p).contains("wear compensation needs Clearance or Retract links"));
}
