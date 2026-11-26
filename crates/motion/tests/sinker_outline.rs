//! Provenance: synthetic wall-offset examples from general die-sink EDM practice (#1810).
//! Gate map (#1810): covers motion::sinker_undersize::judge_outline and cavity_outline_refusal.

use camcraft_motion::sinker_orbit::{Orbit, OrbitKind};
use camcraft_motion::sinker_undersize::{
    cavity_outline_refusal, judge_outline, SinkerPass, Undersize,
};

const CAVITY: [[f64; 2]; 4] = [[10.0, 0.0], [0.0, 6.0], [-10.0, 0.0], [0.0, -6.0]];

fn diamond(inset: f64) -> [[f64; 2]; 4] {
    let scale = 1.0 - inset * (1.0_f64 / 100.0 + 1.0 / 36.0).sqrt();
    CAVITY.map(|p| [p[0] * scale, p[1] * scale])
}

fn burn(electrode: &[[f64; 2]; 4]) -> Undersize {
    Undersize {
        pass: SinkerPass::Finish,
        cavity: (20.0, 12.0),
        electrode: (2.0 * electrode[0][0], 2.0 * electrode[1][1]),
        gap: 0.02,
        undersize: 0.02,
        orbit: None,
        outline: true,
    }
}

#[test]
fn a_diamond_finish_is_judged_on_wall_offsets() {
    let electrode = diamond(0.02);
    assert_eq!(
        judge_outline(&burn(&electrode), &CAVITY, &electrode, "finish").expect("the finish fits"),
        0.02
    );
}

#[test]
fn matching_extents_do_not_hide_an_oversized_electrode_outline() {
    let electrode = diamond(0.02);
    let x = electrode[0][0];
    let y = electrode[1][1];
    let boxed = [[x, y], [-x, y], [-x, -y], [x, -y]];
    let why = judge_outline(&burn(&electrode), &CAVITY, &boxed, "finish")
        .expect_err("the rectangle crosses the walls")
        .to_string();
    assert!(
        why.contains("electrode is too large") && why.contains("undersize offset"),
        "{why}"
    );
}

#[test]
fn a_finish_that_leaves_a_wall_undersize_is_refused() {
    let electrode = diamond(0.1);
    let why = judge_outline(&burn(&electrode), &CAVITY, &electrode, "finish")
        .expect_err("the electrode is too small")
        .to_string();
    assert!(
        why.contains("sinker finish leaves the cavity under size on a wall"),
        "{why}"
    );
}

#[test]
fn malformed_cavity_outlines_are_refused_by_name() {
    for cavity in [
        vec![[0.0, 0.0], [1.0, 0.0]],
        vec![[0.0, 0.0], [1.0, 1.0], [0.0, 1.0], [1.0, 0.0]],
        vec![[0.0, 0.0], [f64::NAN, 1.0], [0.0, 1.0]],
    ] {
        let why = cavity_outline_refusal(&cavity).expect("a malformed cavity is refused");
        assert!(why.contains("sinker cavity outline"), "{why}");
    }
    assert!(cavity_outline_refusal(&CAVITY).is_none());
}

#[test]
fn an_excessive_cavity_outline_is_refused_before_geometry() {
    let count = camcraft_tools::electrode_outline::MAX_POINTS + 1;
    let cavity: Vec<[f64; 2]> = (0..count)
        .map(|i| {
            let angle = std::f64::consts::TAU * i as f64 / count as f64;
            [angle.cos(), angle.sin()]
        })
        .collect();
    let why = cavity_outline_refusal(&cavity).expect("an excessive cavity outline is refused");
    assert!(
        why.contains("cavity")
            && why.contains(&camcraft_tools::electrode_outline::MAX_POINTS.to_string()),
        "{why}"
    );
}

#[test]
fn a_cavity_outline_that_collapses_on_the_geometry_lattice_is_refused() {
    let thin = [[0.0, 0.0], [1.0, 0.0], [1.0, 2e-7], [0.0, 2e-7]];
    assert!(
        camcraft_tools::electrode_outline::shape_refusal(&thin).is_none(),
        "the input polygon passes the structural limits"
    );
    let why = cavity_outline_refusal(&thin).expect("the cavity collapses on the geometry lattice");
    assert!(why.contains("one nonempty section"), "{why}");
}

#[test]
fn cavity_outline_extents_must_match_the_stated_size() {
    let electrode = diamond(0.02);
    let u = Undersize {
        cavity: (18.0, 12.0),
        ..burn(&electrode)
    };
    let why = judge_outline(&u, &CAVITY, &electrode, "finish")
        .expect_err("the dimensions disagree")
        .to_string();
    assert!(
        why.contains("the cavity spans") && why.contains("stated size"),
        "{why}"
    );
}

#[test]
fn an_undersize_that_consumes_the_cavity_is_refused() {
    let cavity = [[0.5, 0.5], [-0.5, 0.5], [-0.5, -0.5], [0.5, -0.5]];
    let electrode = cavity.map(|p| [p[0] * 0.1, p[1] * 0.1]);
    let u = Undersize {
        pass: SinkerPass::Rough,
        cavity: (1.0, 1.0),
        electrode: (0.1, 0.1),
        undersize: 0.6,
        gap: 0.1,
        ..burn(&diamond(0.02))
    };
    let why = judge_outline(&u, &cavity, &electrode, "rough")
        .expect_err("the inset consumes the cavity")
        .to_string();
    assert!(
        why.contains("wall offset consumes or splits the cavity"),
        "{why}"
    );
}

#[test]
fn a_small_electrode_cannot_hide_a_gap_that_burns_past_the_floor() {
    let electrode = diamond(0.1);
    let u = Undersize {
        pass: SinkerPass::Rough,
        undersize: 0.01,
        ..burn(&electrode)
    };
    let why = judge_outline(&u, &CAVITY, &electrode, "rough")
        .expect_err("the gap exceeds the floor stand-off")
        .to_string();
    assert!(why.contains("past size"), "{why}");
}

#[test]
fn a_vector_orbit_does_not_certify_angled_walls() {
    let electrode = diamond(0.03);
    let u = Undersize {
        undersize: 0.03,
        orbit: Some(Orbit {
            kind: OrbitKind::Vector,
            radius: 0.01,
            vectors: 4,
            turns: 1,
        }),
        ..burn(&electrode)
    };
    let why = judge_outline(&u, &CAVITY, &electrode, "finish")
        .expect_err("four axis directions do not reach angled walls")
        .to_string();
    assert!(
        why.contains("a vector orbit does not certify") && why.contains("angled walls"),
        "{why}"
    );
}
