//! The arc entry on the 3-D roughing ladder (M11.1): the circular ramp
//! certified by the ramp's chord law at the destination level, verify-
//! clean against the model, and the shell's named substitution.
//!
//! Gate map (M11.1): covers camcraft_motion::machining3d (ladder arc arm),
//! camcraft_motion::entry_arc.

use camcraft_motion::cl::{MoveKind, Toolpath};
use camcraft_motion::geo2d::{Loop, Point2, Region};
use camcraft_motion::geometry::{Point3, Triangle};
use camcraft_motion::machining2d::PocketStrategy;
use camcraft_motion::machining3d::{Entry3D, Pocket3DParams, pocket3d, pocket3d_offset};
use camcraft_motion::verify::{Mode, ToolGeom, VerifyContext, Warning, verify};

fn rect(x0: f64, y0: f64, x1: f64, y1: f64) -> Region {
    Region::new(vec![Loop::new(vec![
        Point2::new(x0, y0),
        Point2::new(x1, y0),
        Point2::new(x1, y1),
        Point2::new(x0, y1),
    ])
    .unwrap()])
    .unwrap()
}

/// A solid axis-aligned box as a facet soup, outward CCW.
fn box_facets(min: [f64; 3], max: [f64; 3]) -> Vec<Triangle> {
    let v = |i: usize| {
        Point3::new(
            [min[0], max[0]][i & 1],
            [min[1], max[1]][(i >> 1) & 1],
            [min[2], max[2]][(i >> 2) & 1],
        )
    };
    let quad = |a: usize, b: usize, c: usize, d: usize| -> [Triangle; 2] {
        [Triangle::new(v(a), v(b), v(c)), Triangle::new(v(a), v(c), v(d))]
    };
    let mut t = Vec::with_capacity(12);
    t.extend(quad(4, 5, 7, 6));
    t.extend(quad(0, 2, 3, 1));
    t.extend(quad(0, 1, 5, 4));
    t.extend(quad(2, 6, 7, 3));
    t.extend(quad(1, 3, 7, 5));
    t.extend(quad(2, 0, 4, 6));
    t
}

/// The slab under the pocket's finished floor, plus a 10x10 boss to
/// z = 0 at the center: an island the arc must never touch.
fn slab_with_boss() -> Vec<Triangle> {
    let mut f = box_facets([-20.0, -20.0, -20.0], [20.0, 20.0, -10.0]);
    f.extend(box_facets([-5.0, -5.0, -10.0], [5.0, 5.0, 0.0]));
    f
}

fn params(strategy: PocketStrategy) -> Pocket3DParams {
    Pocket3DParams {
        links: camcraft_motion::links::Links::ROUGH3D,
        tool_diameter: 8.0,
        stepover: 3.0,
        stock_to_leave: 0.0,
        clearance_z: 5.0,
        top_z: 0.0,
        bottom_z: -10.0,
        stepdown: 5.0,
        feed: 500.0,
        plunge_feed: 100.0,
        climb: true,
        strategy,
        retract_z: None,
        entry: Entry3D::Arc,
        helix_radius: 1.0,
        optimal_load: camcraft_motion::load::DEFAULT_OPTIMAL_LOAD,
    }
}

fn verify_against(facets: &[Triangle], region: &Region, path: &Toolpath) -> Vec<Warning> {
    let tool = ToolGeom {
        radius: 4.0,
        nose_radius: 0.0,
        flute_len: 30.0,
        shank_radius: 9.0,
        shank_len: 40.0,
        holder_radius: 20.0,
        holder_len: 60.0,
    };
    let cx = VerifyContext {
        region,
        stock: None,
        model: Some(facets),
        tool: &tool,
        cutter: None,
        mode: Mode::Area,
        stock_top_z: 0.0,
        clearance_z: 5.0,
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

/// Descending straight feed runs of three or more vertices: the fans.
fn fans(t: &Toolpath) -> Vec<Vec<Point3>> {
    let mut out = Vec::new();
    let mut run: Vec<Point3> = Vec::new();
    for m in &t.moves {
        if m.kind == MoveKind::Feed && m.arc.is_none() && m.to.z < m.from.z {
            if run.is_empty() {
                run.push(m.from);
            }
            run.push(m.to);
        } else if !run.is_empty() {
            out.push(std::mem::take(&mut run));
        }
    }
    if !run.is_empty() {
        out.push(run);
    }
    out.into_iter().filter(|f| f.len() >= 3).collect()
}

#[test]
fn the_3d_ladder_arc_is_verify_clean_or_named_on_every_level() {
    let f = slab_with_boss();
    let region = rect(-20.0, -20.0, 20.0, 20.0);
    for strategy in [PocketStrategy::Zigzag, PocketStrategy::Waterline] {
        let t = pocket3d(&f, &region, &params(strategy)).expect("arc ladder valid");
        assert!(t.is_chained() && t.all_finite(), "{strategy:?}");
        assert_eq!(verify_against(&f, &region, &t), Vec::new(), "{strategy:?} gouged");
        let said = t.notes.iter().filter(|n| n.starts_with("entry: the arc")).count();
        let fans = fans(&t);
        assert!(fans.len() + said >= 2, "{strategy:?}: every level rides or says: {:?}", t.notes);
        for fan in &fans {
            // Vertical-plane containment of each certified fan.
            let (a, e) = (fan[0], fan[fan.len() - 1]);
            let (dx, dy) = (e.x - a.x, e.y - a.y);
            let len = dx.hypot(dy);
            for q in fan {
                let off = ((q.x - e.x) * dy - (q.y - e.y) * dx).abs() / len;
                assert!(off <= 2e-6, "fan vertex off its vertical plane: {q:?}");
            }
        }
    }
}

#[test]
fn the_shell_substitutes_the_arc_by_name() {
    let f = slab_with_boss();
    let region = rect(-20.0, -20.0, 20.0, 20.0);
    let t = pocket3d_offset(&f, &region, &params(PocketStrategy::Waterline)).expect("valid");
    assert!(t.is_chained() && t.all_finite());
    assert!(fans(&t).is_empty(), "no unproved fan rides a lifted shell");
    assert!(
        t.notes.iter().any(|n| n.contains("arc entry is not proved onto a lifted shell")),
        "the shell's substitution is said: {:?}",
        t.notes
    );
}

#[test]
fn the_rest_ladder_never_rides_the_arc() {
    // The coarse-only family law: a rest pass starts inside planes the
    // coarse pass owns, so its entries are the vertical dive whatever
    // the family - the arc changes nothing there (pinned by the plain
    // ladder's Plunge-equal rest suffix).
    let f = slab_with_boss();
    let region = rect(-20.0, -20.0, 20.0, 20.0);
    let arc = camcraft_motion::machining3d::pocket3d_with_rest(&f, &region, &params(PocketStrategy::Zigzag));
    let (t, _) = arc.expect("arc rest valid");
    assert!(t.is_chained() && t.all_finite());
    assert_eq!(verify_against(&f, &region, &t), Vec::new());
}
