//! #1186: `Tool::why_not_sane` names the first rule broken, and its
//! verdict is the law's verdict before #1186, byte for byte: the old
//! `is_sane` body is kept here as an oracle and swept against it.
//! Provenance: our own tests over our own law.

use super::*;
use crate::tools::catalog;
use crate::turning_data::TurningData;

/// `TurningData::is_sane` before #1186, verbatim.
fn old_turning_sane(t: &TurningData) -> bool {
    t.vc.is_finite()
        && t.vc > 0.0
        && t.vc <= MAX_VC
        && t.feed_per_rev.is_finite()
        && t.feed_per_rev > 0.0
        && t.feed_per_rev <= MAX_FEED_PER_REV
        && t.max_rpm.is_none_or(|m| m.is_finite() && m > 0.0 && m <= MAX_RPM_CLAMP)
}

/// `Tool::is_sane` before #1186, verbatim (comments dropped).
#[allow(clippy::float_cmp)]
fn old_is_sane(t: &Tool) -> bool {
    let fin = [
        t.diameter,
        t.corner_r,
        t.tip_angle,
        t.pitch,
        t.nose_radius,
        t.width,
        t.flute_len,
        t.shank_d,
        t.shank_len,
        t.holder_d,
        t.holder_len,
        t.rpm,
        t.feed,
    ]
    .into_iter()
    .all(|v| v.is_finite());
    fin && t.diameter > 0.0
        && t.diameter <= 500.0
        && t.corner_r >= 0.0
        && t.corner_r <= t.diameter * 0.5
        && t.tip_angle < 180.0
        && t.pitch >= 0.0
        && t.nose_radius >= 0.0
        && t.width >= 0.0
        && t.flute_len > 0.0
        && t.shank_d >= 0.0
        && t.shank_len >= 0.0
        && t.holder_d >= 0.0
        && t.holder_len >= 0.0
        && (if t.kind == ToolKind::Torch { t.rpm == 0.0 } else { t.rpm > 0.0 })
        && t.feed > 0.0
        && t.flutes.is_none_or(|z| (1..=32).contains(&z))
        && t.orientation.is_none_or(crate::orientation::is_valid)
        && t.turning.is_none_or(|d| old_turning_sane(&d) && d.max_rpm.is_none_or(|m| t.rpm <= m))
        && match t.kind {
            ToolKind::Drill => t.tip_angle > 60.0,
            ToolKind::Tap => t.pitch > 0.0,
            ToolKind::Turning => {
                t.nose_radius > 0.0 && t.nose_radius <= t.diameter * 0.5
            }
            ToolKind::Groove => t.width > 0.0 && t.width < t.diameter,
            ToolKind::Thread => {
                t.width > 0.0
                    && t.width < t.diameter
                    && t.tip_angle > 0.0
                    && t.tip_angle < 180.0
            }
            ToolKind::Form => {
                t.width > 0.0
                    && t.width < t.diameter
                    && t.nose_radius > 0.0
                    && t.width <= 2.0 * t.nose_radius
            }
            ToolKind::Taper => t.tip_angle > 0.0,
            ToolKind::Probe => {
                (t.corner_r - t.diameter * 0.5).abs() < 1e-9
                    && t.flute_len >= t.diameter
            }
            ToolKind::Torch => t.corner_r == 0.0,
            _ => true,
        }
}

const KINDS: [ToolKind; 13] = [
    ToolKind::EndMill,
    ToolKind::BallNose,
    ToolKind::BullNose,
    ToolKind::Drill,
    ToolKind::Tap,
    ToolKind::Bore,
    ToolKind::Turning,
    ToolKind::Groove,
    ToolKind::Thread,
    ToolKind::Form,
    ToolKind::Taper,
    ToolKind::Probe,
    ToolKind::Torch,
];

fn agree(t: &Tool, what: &str) {
    let why = t.why_not_sane();
    assert_eq!(why.is_none(), old_is_sane(t), "{what}: {why:?} on {t:?}");
    assert_eq!(t.is_sane(), old_is_sane(t), "{what}");
}

#[test]
fn the_named_law_gives_the_old_verdict_everywhere() {
    let values = [0.0, -1.0, 0.25, 1.0, 3.0, 6.0, 59.0, 61.0, 179.0, 180.0, 501.0, f64::NAN, f64::INFINITY];
    let mut n = 0usize;
    for (name, base) in catalog() {
        for kind in KINDS {
            let tool = Tool { kind, ..base.clone() };
            agree(&tool, name);
            for &v in &values {
                let fields: [fn(&mut Tool, f64); 13] = [
                    |t, v| t.diameter = v,
                    |t, v| t.corner_r = v,
                    |t, v| t.tip_angle = v,
                    |t, v| t.pitch = v,
                    |t, v| t.nose_radius = v,
                    |t, v| t.width = v,
                    |t, v| t.flute_len = v,
                    |t, v| t.shank_d = v,
                    |t, v| t.shank_len = v,
                    |t, v| t.holder_d = v,
                    |t, v| t.holder_len = v,
                    |t, v| t.rpm = v,
                    |t, v| t.feed = v,
                ];
                for (k, set) in fields.iter().enumerate() {
                    let mut t = tool.clone();
                    set(&mut t, v);
                    agree(&t, &format!("{name} as {kind:?}, field {k} = {v}"));
                    n += 1;
                }
            }
            for flutes in [None, Some(0), Some(1), Some(32), Some(33)] {
                agree(&Tool { flutes, ..tool.clone() }, &format!("{name} flutes {flutes:?}"));
            }
            for orientation in [None, Some(0), Some(1), Some(9), Some(10)] {
                agree(&Tool { orientation, ..tool.clone() }, &format!("{name} orientation {orientation:?}"));
            }
            for (vc, f, m) in [(150.0, 0.2, None), (0.0, 0.2, None), (2001.0, 0.2, None), (150.0, 0.0, None), (150.0, 3.5, None), (150.0, 0.2, Some(0.0)), (150.0, 0.2, Some(1.0)), (150.0, 0.2, Some(1e6)), (f64::NAN, 0.2, None)] {
                let turning = Some(TurningData { vc, feed_per_rev: f, max_rpm: m });
                agree(&Tool { turning, ..tool.clone() }, &format!("{name} turning {vc} {f} {m:?}"));
            }
        }
    }
    assert!(n > 1000, "the sweep ran: {n}");
}

#[test]
fn a_turning_field_is_named() {
    let (_, base) = catalog().into_iter().next().expect("catalog");
    let t = Tool { turning: Some(TurningData { vc: 0.0, feed_per_rev: 0.2, max_rpm: None }), ..base };
    assert_eq!(
        t.why_not_sane().as_deref(),
        Some("turning surface speed 0 m/min is outside 0..2000 m/min (it must be over 0)")
    );
}

#[test]
fn a_milling_field_is_named() {
    let (_, base) = catalog().into_iter().find(|(_, t)| t.kind == ToolKind::EndMill).expect("an end mill");
    let t = Tool { corner_r: base.diameter, ..base.clone() };
    assert_eq!(
        t.why_not_sane(),
        Some(format!("corner radius {} mm is more than half the {} mm diameter", base.diameter, base.diameter))
    );
    let t = Tool { feed: -5.0, ..base };
    assert_eq!(t.why_not_sane().as_deref(), Some("feed -5 mm/min must be over 0"));
}
