//! Pins the Thread op's stamp digest of its pass control (#988): every
//! field of `ThreadPasses` moves the stamp, and the #894 default leaves
//! it as it was before #988. Specced from our own cache-stamp design
//! (operation/stamp.rs); no outside source.

use super::*;
use crate::machining2d::{PassSchedule, ThreadPasses};
use std::hash::Hasher;

fn thread(passes: ThreadPasses) -> OpParams {
    OpParams::Thread {
        break_through: None,
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
        picks: Default::default(),
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

/// The #1849 module split must keep #419's chip-break stamp bytes and
/// the pre-existing hand/plan bytes: either edit invalidates cached NC.
#[test]
fn threading_stamp_keeps_dips_and_existing_controls_after_split() {
    use crate::turning::{ThreadDips, ThreadInfeed};
    let world = crate::world::new_world();
    let base = ThreadInfeed {
        dips: ThreadDips { interval: 4.0, height: 0.3, length: 0.0 },
        ..ThreadInfeed::default()
    };
    let params = |plan, hand| OpParams::Threading {
        tolerance: 0.05,
        clearance_z: Height::fixed(34.0),
        top_z: Height::fixed(24.3),
        bottom_z: Height::fixed(7.7),
        pitch: 2.0,
        depth: 1.2,
        infeed: 0.4,
        feed: 120.0,
        plunge_feed: 40.0,
        infeed_plan: plan,
        hand,
        picks: Default::default(),
    };
    let right = crate::cl::ThreadHand::Right;
    let b = digest(&world, &params(base, right));
    for (field, dips) in [
        ("interval", ThreadDips { interval: 6.0, ..base.dips }),
        ("height", ThreadDips { height: 0.2, ..base.dips }),
        ("length", ThreadDips { length: 1.0, ..base.dips }),
    ] {
        assert_ne!(digest(&world, &params(ThreadInfeed { dips, ..base }, right)), b, "{field} must move the stamp");
    }
    assert_ne!(digest(&world, &params(ThreadInfeed::default(), right)), b, "disabling dips must move the stamp");
    assert_ne!(digest(&world, &params(base, crate::cl::ThreadHand::Left)), b, "main's hand bytes must remain");
    assert_ne!(digest(&world, &params(ThreadInfeed { repeat_each_depth: true, ..base }, right)), b, "main's repeat bytes must remain");
}
