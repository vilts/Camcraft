//! The lathe-voice law over EVERY operation kind (#1190): for each row
//! of the kind table, on a path that stays on one meridian (what a lathe
//! post cannot tell from turning) and on a path that stays on the
//! spindle axis, the outcome is one of three - work spindle, live tool,
//! refused by name - and never "a milling cutter on the work spindle".

use super::*;
use crate::cl::{Cycle, CycleKind, Move};
use crate::geo2d::Point2;
use crate::geometry::Point3;
use crate::operation::{Carried, Height, KindInfo, OpType};
use crate::schema::{Name, Operation, Toolpath as Cached};

const ORIGIN: [f64; 3] = [0.0, 0.0, 0.0];

fn carried() -> Carried {
    Carried {
        feed: 300.0,
        plunge: 100.0,
        climb: true,
        clearance: Height::fixed(30.0),
        stepover: 2.0,
        stepdown: 1.0,
        top: Height::fixed(20.0),
        bottom: Height::fixed(10.0),
        retract: Height::fixed(25.0),
        tilt: None,
    }
}

fn params(ty: OpType) -> OpParams {
    (ty.info().seed)(&carried())
}

fn p(x: f64, y: f64, z: f64) -> Point3 {
    Point3::new(x, y, z)
}

/// A pass along Y = 0 across the axis: a face pass with an end mill,
/// and - move for move - a lathe's facing pass. Reproduction (b).
fn meridian() -> Toolpath {
    Toolpath::new(vec![
        Move::new(MoveKind::Rapid, p(-14.0, 0.0, 30.0), p(-14.0, 0.0, 4.0), None),
        Move::new(MoveKind::Feed, p(-14.0, 0.0, 4.0), p(14.0, 0.0, 4.0), Some(300.0)),
        Move::new(MoveKind::Rapid, p(14.0, 0.0, 4.0), p(14.0, 0.0, 30.0), None),
    ])
}

/// One hole down the line through (`x`, `y`): rapid to R, the cycle's
/// plunge, the exit. Reproduction (a) at (0, 0).
fn hole(x: f64, y: f64) -> Toolpath {
    let mut t = Toolpath::new(vec![
        Move::new(MoveKind::Rapid, p(x, y, 30.0), p(x, y, 20.0), None),
        Move::new(MoveKind::Plunge, p(x, y, 20.0), p(x, y, 10.0), Some(100.0)),
        Move::new(MoveKind::Rapid, p(x, y, 10.0), p(x, y, 30.0), None),
    ]);
    t.cycles = vec![Cycle {
        kind: CycleKind::Deep,
        xy: Point2::new(x, y),
        r_z: 20.0,
        bottom_z: 10.0,
        entry_z: 20.0,
        peck: 3.0,
        dwell: 0.0,
        first_move: 0,
    }];
    t
}

fn stating_c(mut t: Toolpath) -> Toolpath {
    for m in &mut t.moves {
        m.rotary = Some(0.0);
    }
    t
}

/// What the law must answer for a kind whose path states no C. An
/// exhaustive match: a new kind does not compile until it is decided.
#[derive(Debug, PartialEq)]
enum Want {
    /// The turning family: the work spindle, on any path.
    Turns,
    /// The Drill: the work spindle on centre, refused off it.
    OnCentreHole,
    /// Refused wherever its path lies; its cutter must itself turn (or,
    /// the probe, has no spindle).
    Refused,
}

fn want(ty: OpType) -> Want {
    match ty {
        OpType::Facing
        | OpType::ProfileRough
        | OpType::ProfileFinish
        | OpType::GrooveRough
        | OpType::GrooveFinish
        | OpType::Part
        | OpType::Threading
        | OpType::Chamfering
        | OpType::TaperTurning
        | OpType::MultiPassTurning
        | OpType::FormTurning
        | OpType::BoringTurning => Want::Turns,
        OpType::Drill => Want::OnCentreHole,
        OpType::Face
        | OpType::Contour
        | OpType::Pocket
        | OpType::Slot
        | OpType::Trace
        | OpType::Adaptive2D
        | OpType::Offset2D
        | OpType::Engrave
        | OpType::Chamfer2D
        | OpType::Circular
        | OpType::Thread
        | OpType::Bore
        | OpType::Pocket3D
        | OpType::Finish3D
        | OpType::Deburr
        | OpType::Contour5D
        | OpType::Parallel5D
        | OpType::Swarf
        | OpType::AdvancedSwarf
        | OpType::RotaryContour
        | OpType::RotaryParallel
        | OpType::RotaryOffset
        | OpType::MultiAxisRough
        | OpType::Flow5D
        | OpType::MultiAxisContour
        | OpType::Probe
        | OpType::PlasmaProfile
        | OpType::TrochoidalSlot => Want::Refused,
    }
}

fn refusal(v: &LatheVoice) -> &str {
    match v {
        LatheVoice::Refused(why) => why,
        other => panic!("expected a refusal, got {other:?}"),
    }
}

#[test]
fn every_kind_is_work_spindle_live_tool_or_refused_by_name() {
    let (mut turns, mut holes, mut refused) = (0, 0, 0);
    for info in KindInfo::ALL {
        let params = params(info.ty);
        let (on_meridian, on_axis) = (voice(&params, &meridian(), ORIGIN), voice(&params, &hole(0.0, 0.0), ORIGIN));
        match want(info.ty) {
            Want::Turns => {
                turns += 1;
                assert_eq!(info.family(), Family::Turn, "{}", info.label);
                assert_eq!(on_meridian, LatheVoice::WorkSpindle, "{}", info.label);
                assert_eq!(on_axis, LatheVoice::WorkSpindle, "{}", info.label);
            }
            Want::OnCentreHole => {
                holes += 1;
                assert_eq!(on_axis, LatheVoice::WorkSpindle, "{} on centre is how a lathe drills", info.label);
                assert!(refusal(&on_meridian).contains("off the spindle axis"), "{on_meridian:?}");
            }
            Want::Refused => {
                refused += 1;
                assert_ne!(info.family(), Family::Turn, "{}", info.label);
                // On the meridian AND on centre: a Bore, a thread mill,
                // an engrave or a probe on the axis is still not a hole
                // cycle a lathe runs with the work turning.
                for v in [&on_meridian, &on_axis] {
                    let why = refusal(v);
                    assert!(why.contains(&format!("a {} operation", info.label)), "{}: names its kind: {why}", info.label);
                    assert!(why.contains("work spindle"), "{}: says what would turn: {why}", info.label);
                    assert!(why.contains("suppress the operation"), "{}: says what to do: {why}", info.label);
                }
            }
        }
        // The post's own order: a path stating C is live-tool work.
        assert_eq!(voice(&params, &stating_c(meridian()), ORIGIN), LatheVoice::LiveTool, "{}", info.label);
    }
    assert_eq!((turns, holes, refused), (12, 1, 28), "the table covers every kind");
    assert_eq!(turns + holes + refused, KindInfo::ALL.len());
}

#[test]
fn a_drill_is_admitted_only_with_every_cut_on_the_spindle_axis_along_z() {
    let drill = params(OpType::Drill);
    // The axis is world Z through the work offset, wherever that is.
    assert_eq!(voice(&drill, &hole(5.0, -3.0), [5.0, -3.0, 7.0]), LatheVoice::WorkSpindle);
    // Off the axis but on the meridian (X 10, Y 0): the post's own
    // meridian check cannot see it; this law names the hole.
    let off = voice(&drill, &hole(10.0, 0.0), ORIGIN);
    assert!(refusal(&off).contains("X10.000 Y0.000 from the work offset"), "{off:?}");
    // The posts' lattice is the tolerance: half a micron, no more.
    assert_eq!(voice(&drill, &hole(0.0005, -0.0005), ORIGIN), LatheVoice::WorkSpindle);
    assert!(matches!(voice(&drill, &hole(0.0006, 0.0), ORIGIN), LatheVoice::Refused(_)));
    assert!(matches!(voice(&drill, &hole(0.0, f64::NAN), ORIGIN), LatheVoice::Refused(_)));
    // A cycle record off the axis refuses even if no feed move shows it.
    let mut stray = hole(0.0, 0.0);
    stray.cycles[0].xy = Point2::new(0.0, 3.0);
    assert!(refusal(&voice(&drill, &stray, ORIGIN)).contains("Y3.000"));
    // Rapids are free: the tool may come in from the side, in air.
    let mut side = hole(0.0, 0.0);
    side.moves.insert(0, Move::new(MoveKind::Rapid, p(40.0, 0.0, 30.0), p(0.0, 0.0, 30.0), None));
    side.cycles[0].first_move = 1;
    assert_eq!(voice(&drill, &side, ORIGIN), LatheVoice::WorkSpindle);
    // A feed move off the axis is a cut off the axis.
    let mut cut = hole(0.0, 0.0);
    cut.moves.push(Move::new(MoveKind::Feed, p(0.0, 0.0, 30.0), p(2.0, 0.0, 30.0), Some(100.0)));
    assert!(matches!(voice(&drill, &cut, ORIGIN), LatheVoice::Refused(_)));
    // A cross hole: the drill is tilted off the spindle axis.
    let mut cross = hole(0.0, 0.0);
    for m in &mut cross.moves {
        m.axis = Some([1.0, 0.0, 0.0]);
    }
    assert!(refusal(&voice(&drill, &cross, ORIGIN)).contains("tilted drill"));
    // A pose along +Z states nothing new.
    let mut upright = hole(0.0, 0.0);
    for m in &mut upright.moves {
        m.axis = Some([0.0, 0.0, 1.0]);
    }
    assert_eq!(voice(&drill, &upright, ORIGIN), LatheVoice::WorkSpindle);
}

fn op_entity(world: &World, name: &str, ty: OpType, path: Toolpath) -> Entity {
    let op = Operation {
        tool: "T".into(),
        src: crate::operation::SrcRef::Stock,
        params: params(ty),
        stamp: None,
        setup: None,
        area: Default::default(),
        coolant: Default::default(),
        after: Default::default(),
        suppressed: false,
    };
    world.entity().set(Name(name.into())).set(op).set(Cached::new(path, Vec::new())).id()
}

/// The door as the three post bodies call it, over every kind, on the
/// shipped scripts: a turning dialect refuses by name, a milling
/// dialect is left alone.
#[test]
fn the_door_refuses_on_every_turning_dialect_and_passes_every_milling_one() {
    let world = crate::world::new_world();
    for info in KindInfo::ALL {
        op_entity(&world, &format!("m-{}", info.tag), info.ty, meridian());
        op_entity(&world, &format!("c-{}", info.tag), info.ty, stating_c(meridian()));
    }
    for (post, cps) in crate::posting::BUILTINS {
        let lathe = crate::posting::LATHE_POSTS.contains(post);
        assert_eq!(crate::post::dialect::speaks_turning(cps).ok(), Some(lathe), "{post}: manifest and registry agree");
        for info in KindInfo::ALL {
            let (m, c) = (format!("m-{}", info.tag), format!("c-{}", info.tag));
            assert_eq!(at_post(&world, [c.as_str()], cps), Ok(()), "{post}: {} stating C is live-tool work", info.label);
            let got = at_post(&world, [m.as_str()], cps);
            if !lathe || want(info.ty) == Want::Turns {
                assert_eq!(got, Ok(()), "{post}: {}", info.label);
                continue;
            }
            let why = got.expect_err("a lathe post may not open it");
            assert!(why.starts_with(&format!("{m}: a {} operation", info.label)), "{post}: {why}");
        }
    }
    // The shared predicate: only what the work spindle turns.
    let drill = op_entity(&world, "on-centre", OpType::Drill, hole(0.0, 0.0));
    let face = op_entity(&world, "face", OpType::Face, meridian());
    let live = op_entity(&world, "live", OpType::RotaryContour, stating_c(meridian()));
    let turn = op_entity(&world, "turn", OpType::Facing, meridian());
    assert!(on_work_spindle(&world, &drill) && on_work_spindle(&world, &turn));
    assert!(!on_work_spindle(&world, &face) && !on_work_spindle(&world, &live));
    // The first refusal in program order is the one named.
    let both = at_post(&world, ["on-centre", "turn", "face", "live"], "capabilities = 2;");
    assert!(both.is_err_and(|w| w.starts_with("face: a Face operation would be posted on the work spindle")));
    // A script that does not load is a refusal, never a pass.
    assert!(at_post(&world, ["face"], "capabilities = ;").is_err_and(|w| w.starts_with("post engine:")));
}
