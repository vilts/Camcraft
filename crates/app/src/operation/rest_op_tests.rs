//! #1489: the 2D Pocket's rest group in the operation stamp - written
//! only when on (a pocket without it writes no byte for it), and every
//! value of the group moves it, so a cached path is never served to
//! another previous tool.

use std::hash::Hasher;

use crate::machining2d::{PocketStrategy, RestParams};
use crate::machining3d::Entry3D;
use crate::operation::{Height, OpParams};

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
    }
}

const PREV: RestParams =
    RestParams { previous_tool_diameter: 12.0, previous_corner_radius: 0.0, previous_stock_to_leave: 0.0, allowance: 0.0 };

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
    ] {
        assert_ne!(on, digest(&pocket(Some(other))), "{other:?} must move the stamp");
    }
    assert_eq!(on, digest(&pocket(Some(PREV))), "the stamp is a function");
}
