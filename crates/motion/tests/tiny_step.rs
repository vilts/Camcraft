//! A step too small to ladder is refused by name, never allocated (#691).
//! Gate map: covers motion::machining2d::face (band, pass_budget) and
//! motion::machining2d::levels (cut_levels), through Face and 2D Contour.
//! Provenance: our own regression - a 1e-9 mm stepover aborted the
//! process on a ~96 GB allocation.

use camcraft_motion::entry::Entry3D;
use camcraft_motion::geo2d::{Loop, Point2, Region};
use camcraft_motion::machining2d::{contour, face, ContourParams, FaceParams, GenError, Lead, Side, MAX_PASSES};

fn square(s: f64) -> Region {
    let l = Loop::new(vec![Point2::new(0.0, 0.0), Point2::new(s, 0.0), Point2::new(s, s), Point2::new(0.0, s)]);
    Region::new(vec![l.expect("loop")]).expect("region")
}

fn fp(stepover: f64, stepdown: Option<f64>) -> FaceParams {
    FaceParams {
        holes: Default::default(),
        tool_diameter: 6.0,
        stepover,
        clearance_z: 15.0,
        cut_z: 9.5,
        top_z: stepdown.map(|_| 10.0),
        stepdown,
        feed: 1000.0,
        plunge_feed: 300.0,
        climb: true,
        blend_turns: false, blend_radius: None,
        stock_top: None,
    }
}

fn cp(stepdown: f64) -> ContourParams {
    ContourParams {
        tool_diameter: 6.0,
        stock_to_leave: 0.0,
        clearance_z: 5.0,
        retract_z: 2.0,
        top_z: 0.0,
        bottom_z: -1.0,
        stepdown,
        feed: 500.0,
        plunge_feed: 100.0,
        climb: true,
        side: Side::Outside,
        lead: Lead::None,
        tolerance: 0.01,
        tab_count: 0,
        tab_width: 0.0,
        tab_height: 0.0,
        entry: Entry3D::Plunge,
        helix_radius: 1.0,
        blend_turns: false, blend_radius: None,
        links: camcraft_motion::links::Links::PLANAR,
        picks: Default::default(),
    }
}

fn by_name(what: &str, r: Result<impl Sized, GenError>, word: &str) {
    match r {
        Err(GenError::Invalid(m)) => {
            assert!(m.contains(word) && m.contains("passes"), "{what}: refused, but not by name: {m}");
        }
        Err(e) => panic!("{what}: wrong refusal {e:?}"),
        Ok(_) => panic!("{what}: a {word} that needs past {MAX_PASSES} passes ran"),
    }
}

#[test]
fn a_face_stepover_too_small_to_ladder_is_refused_by_name() {
    by_name("face 1e-9 stepover", face(&square(10.0), &fp(1e-9, None)), "stepover");
    // Below the coordinate's float resolution: the ladder could never advance.
    by_name("face 1e-300 stepover", face(&square(10.0), &fp(1e-300, None)), "stepover");
}

#[test]
fn a_stepdown_too_small_to_ladder_is_refused_by_name_on_face_and_contour() {
    by_name("staged face 1e-9 stepdown", face(&square(10.0), &fp(2.0, Some(1e-9))), "stepdown");
    by_name("contour 1e-9 stepdown", contour(&square(10.0), &cp(1e-9)), "stepdown");
}

#[test]
fn a_small_but_real_step_still_generates() {
    let f = face(&square(10.0), &fp(0.05, Some(0.05)));
    assert!(f.is_ok(), "0.05 mm face: {:?}", f.err());
    let c = contour(&square(10.0), &cp(0.05));
    assert!(c.is_ok(), "0.05 mm contour stepdown: {:?}", c.err());
}
