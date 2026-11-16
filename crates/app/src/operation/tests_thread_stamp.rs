//! Pins the Thread op's stamp digest of its pass control (#988): every
//! field of `ThreadPasses` moves the stamp, and the #894 default leaves
//! it as it was before #988. Specced from our own cache-stamp design
//! (operation/stamp.rs); no outside source.

use super::*;
use crate::machining2d::{PassSchedule, ThreadPasses};
use std::hash::Hasher;

fn thread(passes: ThreadPasses) -> OpParams {
    OpParams::Thread {
        clearance_z: Height::fixed(5.0),
        retract_z: None,
        top_z: Height::fixed(0.0),
        bottom_z: None,
        infeed: 0.5,
        feed: 300.0,
        plunge_feed: 100.0,
        climb: true,
        holes: vec![crate::drilling::Hole {
            xy: crate::geo2d::Point2::new(10.0, 20.0),
            radius: 4.0,
            entry_z: 0.0,
            floor_z: -12.0,
            through: false,
            stepped: false,
        }],
        pitch: Some(1.25),
        hand: crate::cl::ThreadHand::Right,
        external: false,
        passes,
    }
}

fn digest(world: &flecs_ecs::prelude::World, p: &OpParams) -> u64 {
    let mut h = std::hash::DefaultHasher::new();
    p.stamp_with(world, None, None, &mut h);
    h.finish()
}

/// The digest of `thread(ThreadPasses::default())` as the Thread arm
/// wrote it before #988 (no pass-control block). Stamps are saved in
/// the project file, so a move here turns every saved thread Stale.
const PRE_988_DIGEST: u64 = 9_379_674_177_453_187_408;

#[test]
fn thread_stamp_follows_every_pass_control() {
    let world = crate::world::new_world();
    // The default is the #894 ride: no word is written for it.
    assert_eq!(
        digest(&world, &thread(ThreadPasses::default())),
        PRE_988_DIGEST,
        "the default pass control must leave the pre-#988 digest"
    );
    // A base with every field off its default, so the 0xE8 block is
    // written for it and for each variant below: only the field that
    // differs can tell the two apart.
    let base = ThreadPasses {
        schedule: PassSchedule::ConstantArea,
        finish: 0.05,
        spring: 1,
        allowance: 0.02,
        spindle_reverse: true,
        cutter_left: true,
    };
    let b = digest(&world, &thread(base));
    assert_ne!(b, PRE_988_DIGEST, "a stated pass control moves the stamp");
    let variants = [
        ("schedule", ThreadPasses { schedule: PassSchedule::Equal, ..base }),
        ("finish", ThreadPasses { finish: 0.1, ..base }),
        ("spring", ThreadPasses { spring: 2, ..base }),
        ("allowance", ThreadPasses { allowance: 0.03, ..base }),
        ("spindle_reverse", ThreadPasses { spindle_reverse: false, ..base }),
        ("cutter_left", ThreadPasses { cutter_left: false, ..base }),
    ];
    for (field, v) in variants {
        assert_ne!(digest(&world, &thread(v)), b, "{field} must move the stamp");
    }
}
