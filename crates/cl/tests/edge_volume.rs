//! Provenance: analytic rigid-frame prism and circle fixtures for #1891.
//! Gate map: covers camcraft_cl::cl (edge_break, edge_volume, frame).

use camcraft_cl::cl::{BreakFrame, BreakProfile, EdgeBreak, EdgeBreakVolume, TiltedFrame, Toolpath};
use camcraft_cl::geometry::Point3;

fn p(x: f64, y: f64, z: f64) -> Point3 { Point3::new(x, y, z) }

fn flat() -> EdgeBreak {
    EdgeBreak {
        edge: vec![p(-5.0, 0.0, 0.0), p(5.0, 0.0, 0.0)],
        depth: 0.5,
        slope: 0.0,
        volume: Some(EdgeBreakVolume {
            width: 1.0,
            frames: vec![BreakFrame { outward: [0.0, 0.0, 1.0], across: [0.0, 1.0, 0.0] }],
            profile: BreakProfile::Flat,
        }),
    }
}

fn near(actual: Option<f64>, expected: f64) {
    let actual = actual.expect("the column intersects a connected declared break");
    assert!((actual - expected).abs() < 1e-9, "{actual} != {expected}");
}

#[test]
fn flat_volume_has_finite_width_depth_and_segment_ends() {
    let b = flat();
    assert!(b.is_sane());
    assert_eq!(b.width(), 1.0);
    near(b.floor_below(0.0, 0.0, 0.0), -0.5);
    near(b.floor_below(5.0, 0.5, 0.0), -0.5);
    for (x, y, z) in [(5.01, 0.0, 0.0), (0.0, 0.51, 0.0), (0.0, 0.0, 0.01), (0.0, 0.0, -0.51)] {
        assert!(b.floor_below(x, y, z).is_none(), "outside column ({x},{y},{z}) was credited");
    }
    assert!(b.floor_below(f64::NAN, 0.0, 0.0).is_none());
    assert!(b.floor_below(0.0, f64::INFINITY, 0.0).is_none());
    assert!(b.floor_at(0.0, 0.0).is_none(), "a framed cap must not masquerade as a vertical cone");
}

#[test]
fn concave_ball_and_convex_round_keep_their_circle_profiles() {
    let radius = 0.75;
    let sag = |s: f64| radius - (radius * radius - s * s).sqrt();
    for s in [0.0, 0.1, 0.3, 0.49] {
        let mut b = flat();
        b.volume.as_mut().expect("volume").profile = BreakProfile::ConcaveBall { radius };
        near(b.floor_below(0.0, s, 0.0), -(b.depth - sag(s)));
        b.volume.as_mut().expect("volume").profile = BreakProfile::ConvexRound { radius };
        near(b.floor_below(0.0, s, 0.0), -(b.depth - sag(0.5) + sag(s)));
    }
    let mut b = flat();
    b.volume.as_mut().expect("volume").profile = BreakProfile::ConcaveBall { radius: 0.5 };
    assert!(b.is_sane(), "R = w/2 is legal");
    near(b.floor_below(0.0, 0.4, 0.0), -0.3);
    for radius in [1e20, 1e100, 1e300] {
        b.volume.as_mut().expect("volume").profile = BreakProfile::ConcaveBall { radius };
        near(b.floor_below(0.0, 0.3, 0.0), -0.5);
        b.volume.as_mut().expect("volume").profile = BreakProfile::ConvexRound { radius };
        near(b.floor_below(0.0, 0.3, 0.0), -0.5);
    }
}

#[test]
fn vertical_oblique_and_steep_edges_use_the_local_frame() {
    let mut b = flat();
    b.edge = vec![p(0.0, 0.0, -5.0), p(0.0, 0.0, 5.0)];
    b.volume.as_mut().expect("volume").frames[0].outward = [1.0, 0.0, 0.0];
    near(b.floor_below(-0.25, 0.0, 0.0), -5.0);
    for eps in [0.5, 1e-5, 1e-12] {
        let t = (1.0_f64 - eps * eps).sqrt();
        b.edge = vec![p(-5.0 * eps, 0.0, -5.0 * t), p(5.0 * eps, 0.0, 5.0 * t)];
        b.volume.as_mut().expect("volume").frames[0].outward = [-t, 0.0, eps];
        let expected = -(0.5 / eps).min(5.0 / t);
        near(b.floor_below(0.0, 0.0, 0.0), expected);
    }
}

#[test]
fn oblique_circle_rays_hit_the_arc_before_its_box() {
    let mut b = flat();
    let cos = 0.75_f64.sqrt();
    let volume = b.volume.as_mut().expect("volume");
    volume.frames[0] = BreakFrame { outward: [0.0, 0.5, cos], across: [0.0, cos, -0.5] };
    volume.profile = BreakProfile::ConcaveBall { radius: 0.75 };
    let center = 0.75 - b.depth;
    let h = -cos * center + (cos * cos * center * center + 2.0 * 0.75 * b.depth - b.depth * b.depth).sqrt();
    near(b.floor_below(0.0, 0.0, 0.0), -h);
    b.volume.as_mut().expect("volume").profile = BreakProfile::ConvexRound { radius: 0.75 };
    let center = b.depth + (0.75_f64 * 0.75 - 0.25).sqrt();
    let h = cos * center - (cos * cos * center * center - center * center + 0.75 * 0.75).sqrt();
    near(b.floor_below(0.0, 0.0, 0.0), -h);
}

#[test]
fn tiny_circle_direction_rates_do_not_underflow_into_extra_credit() {
    let eps = 1e-200;
    let length = 1e201;
    let mut b = flat();
    // The column starts at the segment's own start, so its offset from
    // the edge is exact: a cap this steep is refused anywhere roundoff
    // could move where the column enters it.
    b.edge = vec![p(0.0, 0.0, 0.0), p(-length * eps, 0.0, -length)];
    let v = b.volume.as_mut().expect("volume");
    v.frames[0].outward = [-1.0, 0.0, eps];
    v.profile = BreakProfile::ConcaveBall { radius: 0.5 };
    assert!(b.is_sane());
    let actual = b.floor_below(0.0, 0.25, 0.0).expect("intersecting cap");
    let expected = -(0.25_f64 - 0.0625).sqrt() / eps;
    assert!((actual / expected - 1.0).abs() < 1e-12, "extra credit: {actual:e} versus {expected:e}");
}

#[test]
fn disconnected_lower_cap_does_not_extend_credit() {
    let mut b = flat();
    b.depth = 0.2;
    let v = b.volume.as_mut().expect("volume");
    v.width = 0.4;
    b.edge = vec![p(0.0, 0.0, 0.0), p(1.0, 0.0, 0.0), p(1.0, 0.0, -1.0), p(0.0, 0.0, -1.0)];
    v.frames = vec![
        BreakFrame { outward: [0.0, 0.0, 1.0], across: [0.0, 1.0, 0.0] },
        BreakFrame { outward: [1.0, 0.0, 0.0], across: [0.0, 1.0, 0.0] },
        BreakFrame { outward: [0.0, 0.0, 1.0], across: [0.0, 1.0, 0.0] },
    ];
    assert!(b.is_sane());
    near(b.floor_below(0.5, 0.0, 0.0), -0.2);
    assert!(b.floor_below(0.5, 0.0, 1.0).is_none(), "a detached record cannot forgive a boss");
}

#[test]
fn framed_sanity_refuses_corruption_by_every_geometric_bound() {
    let mut cases = Vec::new();
    let b = flat();
    for depth in [0.0, -1.0, f64::NAN, f64::INFINITY, 1.01] {
        cases.push(EdgeBreak { depth, ..b.clone() });
    }
    for slope in [1.0, f64::NAN, f64::INFINITY] {
        cases.push(EdgeBreak { slope, ..b.clone() });
    }
    for width in [0.0, -1.0, f64::NAN, f64::INFINITY, 0.4] {
        let mut bad = b.clone();
        bad.volume.as_mut().expect("volume").width = width;
        cases.push(bad);
    }
    for radius in [0.49, 0.0, -1.0, f64::NAN, f64::INFINITY] {
        for profile in [BreakProfile::ConcaveBall { radius }, BreakProfile::ConvexRound { radius }] {
            let mut bad = b.clone();
            bad.volume.as_mut().expect("volume").profile = profile;
            cases.push(bad);
        }
    }
    for frame in [
        BreakFrame { outward: [0.0, 0.0, 2.0], across: [0.0, 1.0, 0.0] },
        BreakFrame { outward: [1.0, 0.0, 0.0], across: [0.0, 1.0, 0.0] },
        BreakFrame { outward: [0.0, 0.0, 1.0], across: [0.0, 0.0, 1.0] },
        BreakFrame { outward: [0.0, 0.0, 1.0], across: [0.0, f64::NAN, 0.0] },
        BreakFrame { outward: [0.0, 0.0, 1.0], across: [1.0, 0.0, 0.0] },
        BreakFrame { outward: [0.0, 0.0, 1.0], across: [0.0, 2.0, 0.0] },
        BreakFrame { outward: [f64::NAN, 0.0, 1.0], across: [0.0, 1.0, 0.0] },
    ] {
        let mut bad = b.clone();
        bad.volume.as_mut().expect("volume").frames[0] = frame;
        cases.push(bad);
    }
    let mut missing = b.clone();
    missing.volume.as_mut().expect("volume").frames.clear();
    cases.push(missing);
    let mut extra = b.clone();
    extra.volume.as_mut().expect("volume").frames.push(BreakFrame { outward: [0.0, 0.0, 1.0], across: [0.0, 1.0, 0.0] });
    cases.push(extra);
    for edge in [vec![], vec![p(0.0, 0.0, 0.0)], vec![p(-f64::MAX, 0.0, 0.0), p(f64::MAX, 0.0, 0.0)]] {
        cases.push(EdgeBreak { edge, ..b.clone() });
    }
    let mut shallow = b.clone();
    shallow.depth = 0.1;
    shallow.volume.as_mut().expect("volume").profile = BreakProfile::ConcaveBall { radius: 0.5 };
    cases.push(shallow);
    let mut huge = b.clone();
    huge.depth = f64::MAX;
    huge.volume.as_mut().expect("volume").width = f64::MAX;
    cases.push(huge);
    let mut collapsed = b.clone();
    collapsed.edge[1] = collapsed.edge[0];
    cases.push(collapsed);
    let mut corrupt = b;
    corrupt.edge[1].z = f64::INFINITY;
    cases.push(corrupt);
    for (i, bad) in cases.iter().enumerate() {
        assert!(!bad.is_sane(), "case {i} survived sanity: {bad:?}");
        assert!(bad.floor_below(0.0, 0.0, 0.0).is_none(), "case {i} credited material");
    }
}

#[test]
fn tilted_world_path_rotates_frames_and_keeps_legacy_refusal() {
    let frame = TiltedFrame::from_angles(p(2.0, 3.0, 4.0), 25.0, 35.0).expect("frame");
    let b = flat();
    let path = Toolpath { breaks: vec![b.clone(), EdgeBreak { volume: None, slope: 1.0, ..b.clone() }], ..Toolpath::default() };
    let world = frame.world_path(&path, 0.01);
    assert_eq!(world.breaks.len(), 1, "framed volumes survive tilt; vertical cones still drop");
    let mapped = &world.breaks[0];
    assert!(mapped.is_sane(), "rotated record remains orthonormal");
    assert_eq!(mapped.edge, b.edge.iter().map(|q| frame.to_world(*q)).collect::<Vec<_>>());
    let top = frame.to_world(p(0.0, 0.0, 0.0));
    assert!(mapped.floor_below(top.x, top.y, top.z).is_some());
    let out = mapped.volume.as_ref().expect("volume").frames[0].outward;
    for (a, e) in out.into_iter().zip(frame.axis) { assert!((a - e).abs() < 1e-12); }
}

#[test]
fn legacy_cones_keep_their_law_and_reject_nonfinite_width() {
    let legacy = EdgeBreak { volume: None, slope: 1.0, ..flat() };
    assert!(legacy.is_sane());
    assert_eq!(legacy.width(), 0.5);
    assert_eq!(legacy.floor_at(0.0, 0.25), Some((-0.25, 0.0)));
    assert!(!EdgeBreak { depth: f64::MAX, slope: f64::MIN_POSITIVE, ..legacy }.is_sane());
}

#[test]
fn connection_allowance_does_not_grow_with_world_position() {
    // Far from the origin a float step is large, so every gap here is a
    // real distance between representable heights, never lost roundoff.
    for z in [0.0, 1e3, 1e6, 1e9, 1e12, 1e14, -1e14] {
        let mut b = flat();
        for q in &mut b.edge { q.z = z; }
        assert!(b.is_sane());
        let floor = b.floor_below(0.0, 0.0, z).expect("a cap that starts at the surface is connected");
        assert!((floor - (z - 0.5)).abs() <= 1e-9, "z={z}: {floor}");
        assert!(floor >= z - 0.5, "z={z}: credit below the declared depth");
        let micron = 1.0 / 1024.0;
        if z.abs() <= 1e6 {
            assert_eq!(b.floor_below(0.0, 0.0, z + micron), None, "z={z}: a micron gap was joined");
        }
        for gap in [0.09375, 0.25, 0.375] {
            let top = z + gap;
            assert_eq!(top - z, gap, "the gap is exact at z={z}");
            assert_eq!(b.floor_below(0.0, 0.0, top), None, "z={z}: a {gap} mm gap was joined");
        }
    }
    // The same law for a tilted cap moved far along every axis.
    for shift in [0.0, 1e3, 1e14] {
        let frame = TiltedFrame::from_angles(p(shift, -shift, shift), 25.0, 35.0).expect("frame");
        let path = Toolpath { breaks: vec![flat()], ..Toolpath::default() };
        let world = frame.world_path(&path, 0.01);
        let mapped = &world.breaks[0];
        let top = frame.to_world(p(0.0, 0.0, 0.0));
        if shift <= 1e3 {
            assert!(mapped.floor_below(top.x, top.y, top.z).is_some(), "shift={shift}: connected cap lost");
        }
        assert_eq!(mapped.floor_below(top.x, top.y, top.z + 0.25), None, "shift={shift}: a 0.25 mm gap was joined");
    }
}

#[test]
fn an_uncertain_interval_end_is_never_read_in_favour_of_the_cut() {
    // The side planes are one nanoradian off vertical, so where a column
    // meets one, roundoff moves that height by microns.
    let side = |tilt: f64| {
        let mut b = flat();
        b.volume.as_mut().expect("volume").frames[0] =
            BreakFrame { outward: [0.0, tilt.sin(), tilt.cos()], across: [0.0, tilt.cos(), -tilt.sin()] };
        assert!(b.is_sane());
        b
    };
    let b = side(1e-9);
    near(b.floor_below(0.0, 0.0, 0.0), -0.5);
    // The cap ends near 0.2 mm here: credit stops short of the doubt.
    let floor = b.floor_below(0.0, 0.5 - 2e-10, 0.0).expect("the certain part is credited");
    assert!(floor > -0.2 && floor < -0.19, "{floor}");
    // The column may enter this cap as late as a micron down: no credit.
    assert_eq!(side(-1e-9).floor_below(0.0, 0.5 - 1e-15, 0.0), None);
    // An unrelated huge width leaves a well-conditioned column alone.
    let mut wide = side(1e-9);
    wide.volume.as_mut().expect("volume").width = 1e14;
    near(wide.floor_below(0.0, 0.0, 0.0), -0.5);
}

#[test]
fn a_column_at_a_vertex_of_barely_sloped_segments_keeps_its_credit() {
    // Mesh noise leaves both end planes a few picoradians off vertical.
    // The column stands exactly on the shared vertex, so it meets them
    // with no roundoff and the cap below it stays connected.
    let mut b = flat();
    b.edge = vec![p(-5.0, 0.0, -1e-11), p(0.0, 0.0, 0.0), p(5.0, 0.0, -1e-11)];
    let frame = b.volume.as_ref().expect("volume").frames[0];
    b.volume.as_mut().expect("volume").frames = vec![frame, frame];
    assert!(b.is_sane());
    near(b.floor_below(0.0, 0.0, 0.0), -0.5);
    near(b.floor_below(-2.5, 0.0, -5e-12), -0.5);
    assert_eq!(b.floor_below(0.0, 0.0, 1.0 / 1024.0), None, "a micron above the vertex is a gap");
}
