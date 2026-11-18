//! PR #2136: frame-carry pins for current-main edge-break volumes.
//! Gate map: covers app::operation::wcs_yaw.

use super::{Rot2, WcsYaw};
use crate::cl::{BreakFrame, BreakProfile, EdgeBreak, EdgeBreakVolume, Toolpath};
use crate::geometry::Point3;

fn close(a: f64, b: f64) {
    assert!((a - b).abs() < 1e-9, "{a} != {b}");
}

#[test]
fn wcs_yaw_carries_edge_break_volume_frames_and_profile() {
    let yaw = WcsYaw { origin: [11.25, -7.5], yaw_rad: 30_f64.to_radians() };
    let r = Rot2::new(yaw.origin, yaw.yaw_rad);
    let (s, c) = yaw.yaw_rad.sin_cos();
    let h = std::f64::consts::FRAC_1_SQRT_2;
    for profile in [BreakProfile::Flat, BreakProfile::ConcaveBall { radius: 2.0 }, BreakProfile::ConvexRound { radius: 2.0 }] {
        let edge_break = EdgeBreak {
            edge: vec![Point3::new(2.0, 1.0, 3.0), Point3::new(6.0, 1.0, 3.0)],
            depth: 0.4,
            slope: 0.0,
            volume: Some(EdgeBreakVolume {
                width: 1.0,
                frames: vec![BreakFrame { outward: [0.0, h, h], across: [0.0, h, -h] }],
                profile,
            }),
        };
        assert!(edge_break.is_sane());
        let path = Toolpath { breaks: vec![edge_break.clone()], ..Default::default() };
        let carried = yaw.exit_path(&path, 0.01);
        let b = &carried.breaks[0];
        assert!(b.is_sane(), "carried frames must remain orthogonal to the rotated edge");
        assert_eq!(b.depth, edge_break.depth);
        assert_eq!(b.slope, edge_break.slope);
        assert_eq!(b.edge, edge_break.edge.iter().map(|p| r.p3(*p)).collect::<Vec<_>>());
        let v = b.volume.as_ref().expect("framed cap is preserved");
        assert_eq!(v.width, 1.0);
        assert_eq!(v.profile, profile);
        assert_eq!(v.frames.len(), 1);
        for (got, want) in v.frames[0].outward.into_iter().zip([-h * s, h * c, h]) { close(got, want); }
        for (got, want) in v.frames[0].across.into_iter().zip([-h * s, h * c, -h]) { close(got, want); }
        let p = Point3::new(4.0, 1.0, 3.0);
        let q = r.p3(p);
        let before = edge_break.floor_below(p.x, p.y, p.z).expect("fixture grants edge credit");
        let after = b.floor_below(q.x, q.y, q.z).expect("rotation preserves edge credit");
        close(before, after);
        // The caller's record remains owned and unchanged.
        assert_eq!(path.breaks[0], edge_break);
    }
}

#[test]
fn wcs_yaw_preserves_legacy_edge_break_without_volume() {
    let yaw = WcsYaw { origin: [11.25, -7.5], yaw_rad: -37_f64.to_radians() };
    let r = Rot2::new(yaw.origin, yaw.yaw_rad);
    let b = EdgeBreak {
        edge: vec![Point3::new(2.0, 1.0, 3.0), Point3::new(6.0, 1.0, 3.0)],
        depth: 0.4, slope: 1.0, volume: None,
    };
    let path = Toolpath { breaks: vec![b.clone()], ..Default::default() };
    let carried = yaw.exit_path(&path, 0.01);
    let got = &carried.breaks[0];
    assert!(got.is_sane());
    assert_eq!(got.volume, None);
    assert_eq!((got.depth, got.slope), (b.depth, b.slope));
    let p = Point3::new(4.0, 1.2, 3.0);
    let q = r.p3(p);
    close(b.floor_below(p.x, p.y, p.z).unwrap(), got.floor_below(q.x, q.y, q.z).unwrap());
}
