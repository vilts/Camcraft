//! #79 at the app level: a cross drill's rapids are judged as a solid
//! against the setup's raw stock (`Mode::Radial`'s law (d),
//! `camcraft_sim::verify::rapids_radial`), and a rapid in uncut stock
//! never posts - the gate turns the cached `RapidInStock` into a
//! refusal that names the move. Two fixtures: a deterministic one (the
//! clean bar-stock document's own path with one hand-spliced station
//! hop through the bar, judged by the real verifier against the
//! document's stock), which never depends on the planner's standoff
//! (#77/#82); and the 44 mm box document, whose verdict tracks the
//! planner (vacuous once #82 hops outside the raw stock - the
//! deterministic test is the one that always speaks).
//!
//! Provenance: our own fixtures (cross_support's drilled bar), specced
//! from the Prime Directive and the shop rule "never traverse through
//! uncut stock"; no vendor material.
//!
//! Gate map (#79): covers camcraft_app::operation (the cross arm's
//! Mode::Radial verify), camcraft_app::posting (cached warnings hold
//! the post), camcraft_sim::verify (rapids_radial).
//! Lane shape: all tests here run in seconds.

mod cross_support;

use cross_support::*;

use camcraft_app::cl::{Move, Toolpath};
use camcraft_app::geo2d::{Loop, Region};
use camcraft_app::machine::AxisLimits;
use camcraft_app::verify::{verify, Mode, VerifyContext, Warning};

/// Post the document; Err is the refusal text.
fn post(world: &World, tag: &str) -> Result<(), String> {
    let out = std::env::temp_dir().join(format!("camcraft_cross_{tag}_{}.nc", std::process::id()));
    let posted = camcraft_app::posting::post_setup(world, &out);
    let _ = std::fs::remove_file(&out);
    posted.map(|_| ())
}

/// A mill-turn whose B trunnion holds the cross drill's lean, so the
/// clean program posts and a refusal speaks for the rapid alone.
fn tilting_mill_turn() -> Machine {
    let mut m = machine("mill-turn");
    m.axes.push(AxisLimits {
        name: "B".into(),
        rotary: true,
        axis: Some([0.0, 1.0, 0.0]),
        travel: Some((-120.0, 120.0)),
        ..Default::default()
    });
    m
}

/// The bar-stock document's raw stock as the verifier reads it: the
/// footprint X [-15, 15] x Y [-5, 5] under the Z-15 top, and the
/// DR-0400's body.
fn judge_on_bar_stock(t: &Toolpath) -> Vec<Warning> {
    let pts = vec![
        Point2::new(-15.0, -5.0),
        Point2::new(15.0, -5.0),
        Point2::new(15.0, 5.0),
        Point2::new(-15.0, 5.0),
    ];
    let stock = Loop::new(pts).and_then(|l| Region::new(vec![l])).expect("the bar footprint");
    let tool = catalog_tool("DR-0400").geom();
    let cx = VerifyContext {
        region: &stock,
        stock: Some(&stock),
        model: None,
        tool: &tool,
        cutter: None,
        mode: Mode::Radial,
        stock_top_z: 15.0,
        clearance_z: 20.0,
        stock_to_leave: 0.0,
        gouge_tol: 0.01,
        z_tol: 1e-3,
        finish_tol: None,
        fixtures: &[],
        spindle: None,
        ..Default::default()
    };
    verify(t, &cx)
}

#[test]
fn a_station_hop_spliced_through_the_bar_is_named_and_the_post_refuses() {
    // Deterministic (#79 review): the clean bar-stock cross drill,
    // then one rapid appended from where the program ends across the
    // bar to the mirror station - a hop no planner made,
    // so the verdict never rides #77's standoff. The real verifier on
    // the document's stock names exactly that rapid; the cache wears
    // the verdict (a test fixture, not a UI poke - the stamp is the
    // inputs', so the op stays fresh), and the gate refuses by name.
    let (world, mut history) = bar_stock_world("mill-turn");
    let cmd = LoadMachine::new(&world, "tilt.mch".into(), Some(tilting_mill_turn()));
    history.execute(&world, cmd.expect("machine"));
    create(&world, &mut history, "CD1", cross_params(typed(), None));
    assert_eq!(status(&world, "CD1"), OpStatus::Ok);
    let e = model::find_by_name(&world, "CD1").expect("the op");
    let mut c = model::toolpath_of(&world, &e).expect("cached");
    assert!(c.warnings.is_empty(), "the clean document reported: {:?}", c.warnings);
    assert!(judge_on_bar_stock(&c.path).is_empty(), "the hand context disagrees with generation");
    post(&world, "bar_clean").expect("the clean cross drill posts");

    let last = c.path.moves.last().expect("a path").to;
    assert!(last.x > 15.0, "the program ends outside the bar's +X face: {last:?}");
    // The program ends on the through hole's axis (z -10): the hop
    // climbs to the mirror station at z 0, across the solid bar
    // between the holes - a straight run down that hole would ride
    // its kerf, and rightly read clean.
    let through = Point3::new(-last.x, last.y, 0.0);
    let mut hop = Move::new(MoveKind::Rapid, last, through, None);
    hop.axis = c.path.moves.last().and_then(|m| m.axis);
    c.path.moves.push(hop);
    let at = c.path.moves.len() - 1;
    let w = judge_on_bar_stock(&c.path);
    assert_eq!(w, vec![Warning::RapidInStock { at }], "{w:?}");
    // Rebuilt, never edited in place: the cache's path signature is
    // taken at construction (#109 review B1).
    e.entity_view(&world).set(camcraft_app::schema::Toolpath::new(c.path, w));
    assert_eq!(status(&world, "CD1"), OpStatus::Ok, "the splice must not stale the op");
    match post(&world, "bar_spliced") {
        Ok(()) => panic!("a rapid through the bar posted"),
        Err(err) => assert!(
            err.contains("post blocked") && err.contains(&format!("rapid move {at}")),
            "refused, but not by name: {err}"
        ),
    }
}

/// Whether `p` stands inside the 44 mm raw box (X/Y [-22, 22],
/// Z [0, 44]) and on no plunge line (tip or trailing body) cut
/// before move `upto` - the test's own measure of "a rapid in uncut
/// stock", independent of the verifier's.
fn in_uncut_box(t: &Toolpath, upto: usize, p: Point3) -> bool {
    let inside = p.x.abs() < 22.0 - 1e-6 && p.y.abs() < 22.0 - 1e-6 && p.z > 1e-6 && p.z < 44.0 - 1e-6;
    let kerf = t.moves[..upto].iter().filter(|m| m.kind != MoveKind::Rapid).any(|m| {
        let on_line = (p.y - m.to.y).abs() < 1e-6 && (p.z - m.to.z).abs() < 1e-6;
        // Every plunge here runs -X with its body trailing out +X.
        on_line && p.x >= m.to.x.min(m.from.x) - 1e-6
    });
    inside && !kerf
}

#[test]
fn a_rapid_inside_the_raw_box_stock_never_posts_silently() {
    // The planner-tracking twin: the 44 mm box is the setup's raw
    // stock. Wherever the cross drill's rapids stand (the test's own
    // measure: every rapid endpoint inside the box and off any earlier
    // plunge line), each is named RapidInStock and the post refuses.
    // A planner that hops outside the raw stock (#82) owes nothing
    // here - no rapid inside, no verdict asked; the spliced test above
    // keeps the gate's refusal pinned either way.
    let (world, mut history) = cross_world("mill-turn");
    let cmd = LoadMachine::new(&world, "tilt.mch".into(), Some(tilting_mill_turn()));
    history.execute(&world, cmd.expect("machine"));
    create(&world, &mut history, "CD1", cross_params(typed(), None));
    let c = cache(&world, "CD1").expect("cached");
    let t = &c.path;
    let through_stock: Vec<usize> = t
        .moves
        .iter()
        .enumerate()
        .filter(|(i, m)| m.kind == MoveKind::Rapid && (in_uncut_box(t, *i, m.from) || in_uncut_box(t, *i, m.to)))
        .map(|(i, _)| i)
        .collect();
    if through_stock.is_empty() {
        return;
    }
    for i in &through_stock {
        assert!(
            c.warnings.contains(&Warning::RapidInStock { at: *i }),
            "rapid {i} in raw stock went unnamed: {:?}",
            c.warnings
        );
    }
    match post(&world, "box") {
        Ok(()) => panic!("a radial program with rapids {through_stock:?} in raw stock posted"),
        Err(e) => assert!(
            e.contains("post blocked") && through_stock.iter().all(|i| e.contains(&format!("rapid move {i} "))),
            "refused, but not by the rapid law: {e}"
        ),
    }
}
