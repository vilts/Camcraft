//! #1489: the 2D Pocket's rest group in the operation stamp - written
//! only when on (a pocket without it writes no byte for it), and every
//! value of the group moves it, so a cached path is never served to
//! another previous tool.

use std::hash::Hasher;

use crate::geo2d::Point2;
use crate::machining2d::{
    CornerRelief, CornerReliefChoice, CornerReliefKind, CornerReliefOverride, PocketStrategy, RestParams, RestSource,
};
use crate::machining3d::Entry3D;
use crate::operation::{Height, OpParams};
use super::rest_op::REST_ADAPTIVE_CORNER_RELIEF;

fn pocket(rest: Option<RestParams>) -> OpParams {
    OpParams::Pocket {
        rest,
        blend_radius: None,
        links: crate::links::Links::PLANAR,
        tilt: None,
        stepover: 1.5,
        stock_to_leave: 0.0,
        clearance_z: Height::fixed(5.0),
        retract_z: None,
        top_z: Height::fixed(0.0),
        bottom_z: Height::fixed(-4.0),
        stepdown: 2.0,
        feed: 400.0,
        plunge_feed: 80.0,
        climb: true,
        strategy: PocketStrategy::Waterline,
        entry: Entry3D::Plunge,
        helix_radius: 1.0,
        optimal_load: crate::load::DEFAULT_OPTIMAL_LOAD,
        blend_turns: false,
        corner_relief: Default::default(),
        pass_start: Default::default(),
        picks: Default::default(),
    }
}

const PREV: RestParams =
    RestParams { source: RestSource::EnteredTool, previous_tool_diameter: 12.0, previous_corner_radius: 0.0, previous_stock_to_leave: 0.0, allowance: 0.0 };

#[test]
fn the_rest_group_moves_the_stamp() {
    let world = crate::world::new_world();
    let digest = |p: &OpParams| {
        let mut h = std::hash::DefaultHasher::new();
        p.stamp_with(&world, None, None, &mut h);
        h.finish()
    };
    let off = digest(&pocket(None));
    let on = digest(&pocket(Some(PREV)));
    assert_ne!(off, on, "rest machining on must not share the plain pocket's stamp");
    for other in [
        RestParams { previous_tool_diameter: 10.0, ..PREV },
        RestParams { previous_corner_radius: 1.0, ..PREV },
        RestParams { previous_stock_to_leave: 0.2, ..PREV },
        RestParams { allowance: 0.1, ..PREV },
        // #2033: the mode is an input - the two modes never share a path.
        RestParams { source: RestSource::PreviousOperations, ..PREV },
    ] {
        assert_ne!(on, digest(&pocket(Some(other))), "{other:?} must move the stamp");
    }
    assert_eq!(on, digest(&pocket(Some(PREV))), "the stamp is a function");
}

fn adaptive(rest: Option<RestParams>) -> OpParams {
    OpParams::Adaptive2D {
        rest,
        blend_radius: None,
        blend_turns: false,
        links: crate::links::Links::PLANAR,
        tilt: None,
        stepover: 1.5,
        stock_to_leave: 0.0,
        clearance_z: Height::fixed(5.0),
        retract_z: None,
        top_z: Height::fixed(0.0),
        bottom_z: Height::fixed(-4.0),
        stepdown: 2.0,
        feed: 400.0,
        plunge_feed: 80.0,
        climb: true,
        entry: Entry3D::Plunge,
        helix_radius: 1.0,
        optimal_load: crate::load::DEFAULT_OPTIMAL_LOAD,
        method: crate::machining2d::AdaptiveMethod::Spiral,
        troch: crate::machining2d::AdaptiveTrochParams::default(),
        corner_relief: Default::default(),
        pass_start: Default::default(),
        picks: Default::default(),
    }
}

const FROM_OPS: RestParams = RestParams { source: RestSource::PreviousOperations, previous_tool_diameter: 0.0, ..PREV };

/// #1446: the 2D Adaptive's rest group in the stamp - no byte when off,
/// and the allowance moves it.
#[test]
fn the_adaptive_rest_group_moves_the_stamp() {
    let world = crate::world::new_world();
    let digest = |p: &OpParams| {
        let mut h = std::hash::DefaultHasher::new();
        p.stamp_with(&world, None, None, &mut h);
        h.finish()
    };
    let off = digest(&adaptive(None));
    let on = digest(&adaptive(Some(FROM_OPS)));
    assert_ne!(off, on, "rest machining on must not share the plain 2D Adaptive's stamp");
    assert_ne!(on, digest(&adaptive(Some(RestParams { allowance: 0.1, ..FROM_OPS }))), "the allowance must move the stamp");
    assert_eq!(on, digest(&adaptive(Some(FROM_OPS))), "the stamp is a function");
}

/// #1446: the door needs no tool to refuse a bad rest value on a 2D Adaptive.
#[test]
fn the_adaptive_rest_group_refuses_bad_values_without_a_tool() {
    assert_eq!(adaptive(Some(FROM_OPS)).named_refusal(), None);
    for bad in [-0.1, f64::NAN] {
        let p = adaptive(Some(RestParams { allowance: bad, ..FROM_OPS }));
        let why = p.named_refusal().unwrap_or_default();
        assert!(why.contains("rest allowance") || why.contains("must be finite"), "{bad}: {why}");
        assert!(!p.is_sane(), "{bad}");
    }
}

/// #1446 / #2400: corner relief beside rest machining on a 2D Adaptive
/// is refused by name at the door, and remains legal when rest is off.
#[test]
fn the_adaptive_rest_group_refuses_corner_relief() {
    let dogbone = CornerRelief { kind: CornerReliefKind::Dogbone, ..Default::default() };
    let ov_only = CornerRelief {
        overrides: vec![CornerReliefOverride { at: Point2::new(1.0, 2.0), choice: CornerReliefChoice::Dogbone }],
        ..Default::default()
    };
    for relief in [dogbone.clone(), ov_only] {
        let mut p = adaptive(Some(FROM_OPS));
        if let OpParams::Adaptive2D { corner_relief, .. } = &mut p {
            *corner_relief = relief;
        }
        let why = p.named_refusal().unwrap_or_default();
        assert!(why.contains(REST_ADAPTIVE_CORNER_RELIEF), "expected '{REST_ADAPTIVE_CORNER_RELIEF}', got: {why}");
        assert!(!p.is_sane());
    }
    let mut off = adaptive(None);
    if let OpParams::Adaptive2D { corner_relief, .. } = &mut off {
        *corner_relief = dogbone;
    }
    assert_eq!(off.named_refusal(), None);
    assert!(off.is_sane());
}

/// #3354: Inside and Outside move the pocket's stamp; Auto is the stamp
/// every earlier pocket had (nothing written).
#[test]
fn pass_start_moves_the_pocket_stamp_only_off_auto() {
    use crate::machining2d::PassStart;
    let world = crate::world::new_world();
    let digest = |s: PassStart| {
        let mut p = pocket(None);
        if let OpParams::Pocket { pass_start, .. } = &mut p {
            *pass_start = s;
        }
        let mut h = std::hash::DefaultHasher::new();
        p.stamp_with(&world, None, None, &mut h);
        h.finish()
    };
    let auto = digest(PassStart::Auto);
    let (inside, outside) = (digest(PassStart::Inside), digest(PassStart::Outside));
    assert_ne!(auto, inside, "Inside must not share Auto's stamp");
    assert_ne!(auto, outside, "Outside must not share Auto's stamp");
    assert_ne!(inside, outside, "the two sides must not share a stamp");
}

/// #3361: the same law on 3D Pocket. Inside and Outside move the stamp;
/// Auto writes nothing, so every earlier 3D pocket keeps its stamp.
#[test]
fn pass_start_moves_the_pocket3d_stamp_only_off_auto() {
    use crate::machining2d::PassStart;
    let world = crate::world::new_world();
    let base = crate::operation::retype_params(&pocket(None), crate::operation::OpType::Pocket3D.ordinal());
    let digest = |s: PassStart| {
        let mut p = base.clone();
        let OpParams::Pocket3D { pass_start, .. } = &mut p else { panic!("not a 3D Pocket: {p:?}") };
        *pass_start = s;
        let mut h = std::hash::DefaultHasher::new();
        p.stamp_with(&world, None, None, &mut h);
        h.finish()
    };
    let auto = digest(PassStart::Auto);
    let (inside, outside) = (digest(PassStart::Inside), digest(PassStart::Outside));
    assert_ne!(auto, inside, "Inside must not share Auto's stamp");
    assert_ne!(auto, outside, "Outside must not share Auto's stamp");
    assert_ne!(inside, outside, "the two sides must not share a stamp");
}
