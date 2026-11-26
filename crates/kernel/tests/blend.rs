//! Edge-blend bench (M12.1 retirement gate, fillets): constant-radius rounds
//! and chamfers through solidcore, measured against closed forms. truck has
//! no edge blend (`Unsupported`, asserted) - so this is a one-engine bench,
//! and the cross-engine leg is "not yet" for every blend, never a pass.
//! Provenance: our own fixtures; closed forms are elementary solid geometry -
//! a round of radius r on a right-angle edge of length L removes
//! r^2 (1 - pi/4) L, a chamfer of setback d removes d^2 L / 2, a blend along
//! a circular rim removes its section area times the path of the section's
//! centroid (Pappus's second theorem), a box rounded on all edges is the
//! inner box grown by a ball (Steiner's formula), and the all-edge chamfer
//! is inclusion-exclusion over the edge prisms.
//!
//! Gate map (M12.1): covers camcraft_kernel::{solidcore_impl, solidcore_blend}.

mod common;

use std::f64::consts::PI;

use camcraft_kernel::solidcore_impl::SolidcoreKernel;
use camcraft_kernel::truck_impl::TruckKernel;
use camcraft_kernel::{Blend, Kernel, KernelError, Measure, P3};
use common::{REL_FLOOR, Want, verdict};

const CUBE: (P3, P3) = ([0.0; 3], [10.0; 3]);
/// The four vertical edges of the 10-cube, picked at mid-height.
const VERTICAL: [P3; 4] = [
    [0.0, 0.0, 5.0],
    [10.0, 0.0, 5.0],
    [0.0, 10.0, 5.0],
    [10.0, 10.0, 5.0],
];
const CYL_V: f64 = PI * 9.0 * 10.0;
const CYL_BOX: (P3, P3) = ([-3.0, -3.0, 0.0], [3.0, 3.0, 10.0]);

/// Section area a round of radius `r` removes from a right-angle edge.
fn round_section(r: f64) -> f64 {
    r * r * (1.0 - PI / 4.0)
}

/// Centroid of that section, measured from the edge's corner along either
/// face: r (10 - 3 pi) / (12 - 3 pi).
fn round_centroid(r: f64) -> f64 {
    r * (10.0 - 3.0 * PI) / (12.0 - 3.0 * PI)
}

fn blended_cube(picks: &[P3], r: f64, kind: Blend) -> Result<Measure, KernelError> {
    let mut sc = SolidcoreKernel::default();
    let b = sc.make_box(CUBE.0, CUBE.1)?;
    let mut out = sc.blend_edges(b, picks, r, kind)?;
    sc.measure(&mut out)
}

/// An L block: a 20x10x10 slab with a 10-cube standing on its left half
/// (united), blended along the concave edge where they meet.
fn blended_l(r: f64, kind: Blend) -> Result<Measure, KernelError> {
    let mut sc = SolidcoreKernel::default();
    let slab = sc.make_box([0.0; 3], [20.0, 10.0, 10.0])?;
    let tower = sc.make_box([0.0, 0.0, 10.0], [10.0; 3])?;
    let l = sc.boolean_union(slab, tower)?;
    let mut out = sc.blend_edges(l, &[[10.0, 5.0, 10.0]], r, kind)?;
    sc.measure(&mut out)
}

/// A r3 x 10 cylinder at the origin with rims blended (picks clear of the
/// seam line at +X, so each names a rim and not the seam).
fn blended_cylinder(
    sc: &mut SolidcoreKernel,
    picks: &[P3],
    r: f64,
    kind: Blend,
) -> Result<Measure, KernelError> {
    let c = sc.make_cylinder([0.0; 3], 3.0, 10.0)?;
    let mut out = sc.blend_edges(c, picks, r, kind)?;
    sc.measure(&mut out)
}

fn check(w: &Want, m: Result<Measure, KernelError>) -> Option<String> {
    match m {
        Ok(m) => verdict("solidcore", &m, w, REL_FLOOR * w.volume, 1e-6).err(),
        Err(e) => Some(format!("{}: solidcore error: {e}", w.name)),
    }
}

fn want(name: &'static str, volume: f64, bbox: (P3, P3)) -> Want {
    Want { name, volume, bbox }
}

#[test]
fn truck_offers_no_edge_blend() {
    let mut truck = TruckKernel::default();
    let r = truck
        .make_box(CUBE.0, CUBE.1)
        .and_then(|b| truck.blend_edges(b, &[[5.0, 0.0, 0.0]], 2.0, Blend::Round));
    assert!(
        matches!(
            r,
            Err(KernelError::Unsupported {
                kernel: "truck",
                op: "blend_edges"
            })
        ),
        "truck blend: {:?}",
        r.map(|_| "Ok")
    );
}

#[test]
fn cube_blends_match_closed_form() {
    let one: Vec<P3> = vec![[5.0, 0.0, 0.0]];
    let mut all = VERTICAL.to_vec();
    for z in [0.0, 10.0] {
        all.extend([[5.0, 0.0, z], [5.0, 10.0, z], [0.0, 5.0, z], [10.0, 5.0, z]]);
    }
    let (a, d) = (10.0_f64, 2.0_f64);
    let core = a - 2.0 * d;
    // Steiner: the 6-cube grown by a radius-2 ball (spherical corners).
    let all_round =
        core.powi(3) + 6.0 * core * core * d + 3.0 * PI * d * d * core + 4.0 / 3.0 * PI * d.powi(3);
    // Inclusion-exclusion over the 12 edge prisms (d^2 a / 2 each; at each of
    // the 8 corners three pairs overlap d^3 / 3 and the triple d^3 / 4). The
    // three chamfer planes meet in one mitred vertex at each corner: since
    // solidcore fa67b3f5 (#3767, upstream "retire the fillet_chamfer_corner
    // flag", #21269) the chamfer-corner event is the only chamfer corner, and
    // it cuts no flat bevel triangle (the d^3 / 12 tetrahedron of the old pin).
    let all_chamfer = a.powi(3) - 6.0 * a * d * d + 6.0 * d.powi(3);
    let cases: [(&[P3], Blend, Want); 6] = [
        (
            &one,
            Blend::Round,
            want(
                "cube-round-1-edge-r2",
                1000.0 - 10.0 * round_section(d),
                CUBE,
            ),
        ),
        (
            &VERTICAL,
            Blend::Round,
            want(
                "cube-round-4-vertical-r2",
                1000.0 - 40.0 * round_section(d),
                CUBE,
            ),
        ),
        (
            &all,
            Blend::Round,
            want("cube-round-all-12-r2", all_round, CUBE),
        ),
        (
            &one,
            Blend::Chamfer,
            want("cube-chamfer-1-edge-d2", 1000.0 - 10.0 * d * d / 2.0, CUBE),
        ),
        (
            &VERTICAL,
            Blend::Chamfer,
            want(
                "cube-chamfer-4-vertical-d2",
                1000.0 - 40.0 * d * d / 2.0,
                CUBE,
            ),
        ),
        (
            &all,
            Blend::Chamfer,
            want("cube-chamfer-all-12-d2", all_chamfer, CUBE),
        ),
    ];
    let findings: Vec<String> = cases
        .iter()
        .filter_map(|(picks, kind, w)| check(w, blended_cube(picks, d, *kind)))
        .collect();
    assert!(
        findings.is_empty(),
        "blend findings:\n{}",
        findings.join("\n")
    );
}

/// Concave edges: the blend ADDS the section (the result law's envelope
/// check must not mistake that for growth).
#[test]
fn concave_l_blends_match_closed_form() {
    let bbox = ([0.0; 3], [20.0, 10.0, 20.0]);
    let findings: Vec<String> = [
        (
            Blend::Round,
            want(
                "l-concave-round-r2",
                3000.0 + 10.0 * round_section(2.0),
                bbox,
            ),
        ),
        (
            Blend::Chamfer,
            want("l-concave-chamfer-d2", 3000.0 + 10.0 * 2.0, bbox),
        ),
    ]
    .iter()
    .filter_map(|(kind, w)| check(w, blended_l(2.0, *kind)))
    .collect();
    assert!(
        findings.is_empty(),
        "blend findings:\n{}",
        findings.join("\n")
    );
}

/// Rounds along circular rims (Pappus), including a hole's rim in a boolean
/// result, where the section sits outside the hole.
#[test]
fn rim_rounds_match_pappus() {
    let r = 1.0;
    let rim = round_section(r) * 2.0 * PI * (3.0 - round_centroid(r));
    let mut findings = Vec::new();
    let mut sc = SolidcoreKernel::default();
    let w = want("cylinder-top-rim-round-r1", CYL_V - rim, CYL_BOX);
    findings.extend(check(
        &w,
        blended_cylinder(&mut sc, &[[0.0, 3.0, 10.0]], r, Blend::Round),
    ));
    let w = want("cylinder-both-rims-round-r1", CYL_V - 2.0 * rim, CYL_BOX);
    let both = [[0.0, 3.0, 10.0], [0.0, -3.0, 0.0]];
    findings.extend(check(&w, blended_cylinder(&mut sc, &both, r, Blend::Round)));
    let hole_r = 2.0;
    let removed = round_section(r) * 2.0 * PI * (hole_r + round_centroid(r));
    let w = want("hole-rim-round-r1", 1000.0 - 40.0 * PI - removed, CUBE);
    let m = (|| {
        let s = sc.make_box(CUBE.0, CUBE.1)?;
        let c = sc.make_cylinder([5.0, 5.0, -1.0], hole_r, 12.0)?;
        let holed = sc.boolean_subtract(s, c)?;
        let mut out = sc.blend_edges(holed, &[[5.0, 5.0 + hole_r, 10.0]], r, Blend::Round)?;
        sc.measure(&mut out)
    })();
    findings.extend(check(&w, m));
    assert!(
        findings.is_empty(),
        "blend findings:\n{}",
        findings.join("\n")
    );
}

/// The chamfer of a circular rim is a cone; removed: (d^2 / 2) 2 pi (R - d/3).
fn rim_chamfer_want() -> Want {
    want(
        "cylinder-top-rim-chamfer-d1",
        CYL_V - 0.5 * 2.0 * PI * (3.0 - 1.0 / 3.0),
        CYL_BOX,
    )
}

/// Promoted from "not yet" (#917): up to solidcore 55d0e76 the port's default
/// path traced a chamfer along an arc as an offset intersection instead of a
/// cone (upstream WR-421, solidcore#420) and left the closed form by ~1.9e-6
/// relative. From solidcore 2016386 the default path agrees. The result must
/// heal as a closed solid (`heal` hands out nothing else) and measure at the
/// cone's closed form, in volume and box.
#[test]
fn rim_chamfer_default_path_matches_the_cone() {
    let w = rim_chamfer_want();
    let mut sc = SolidcoreKernel::default();
    let m = (|| {
        let c = sc.make_cylinder([0.0; 3], 3.0, 10.0)?;
        let out = sc.blend_edges(c, &[[0.0, 3.0, 10.0]], 1.0, Blend::Chamfer)?;
        let mut solid = sc.heal(out)?;
        sc.measure(&mut solid)
    })();
    let f = check(&w, m);
    assert!(f.is_none(), "default path, closed solid: {f:?}");
}

/// The same chamfer with the port's opt-in fix `fillet_chamfer_cone_on_arc`
/// agrees - what turning that fix on at the seam would buy (decision 3 in
/// docs/decisions/27-solidcore-bench.md).
#[test]
fn rim_chamfer_agrees_with_cone_on_arc_fix() {
    let mut sc = SolidcoreKernel::default();
    sc.k.fixes.fillet_chamfer_cone_on_arc = true;
    let w = rim_chamfer_want();
    let f = check(
        &w,
        blended_cylinder(&mut sc, &[[0.0, 3.0, 10.0]], 1.0, Blend::Chamfer),
    );
    assert!(f.is_none(), "{f:?}");
}
