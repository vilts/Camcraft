//! #1543 / #1550 / #1593 / #1595: the depth law the kinds that cut geometry
//! typed by hand (Circular, Slot, Trochoidal slot) take per source, unit by unit - a finished-part
//! source, whole model or pick, is judged (`Section`, the one
//! `authored_law` both kinds call), and a source that is not the part
//! (the Stock, `part_solid` false) is never judged - and the mark law
//! Engrave and Trace take instead (#1595). Our own fixtures.

use super::generate::{authored_law, depth_law_of, DepthLaw};
use super::{seed_params, FeatureSel, GeoSel, OpType, SrcRef};

fn circular() -> super::OpParams {
    seed_params(OpType::Circular.ordinal())
}

fn slot() -> super::OpParams {
    seed_params(OpType::Slot.ordinal())
}

fn trochoid() -> super::OpParams {
    seed_params(OpType::TrochoidalSlot.ordinal())
}

fn picked() -> SrcRef {
    SrcRef::Selection(vec![GeoSel { entity: "part".into(), feature: FeatureSel::Chain(7) }])
}

#[test]
fn a_whole_model_circular_is_judged_against_the_part() {
    assert!(matches!(circular(), super::OpParams::Circular { .. }), "the seed is a Circular");
    assert_eq!(depth_law_of(&circular(), &SrcRef::Entity("part".into()), true), DepthLaw::Section);
}

#[test]
fn a_picked_circular_is_judged_against_the_part() {
    // #1550: a ring typed below the picked floor gouges the part it
    // was picked off.
    assert_eq!(depth_law_of(&circular(), &picked(), true), DepthLaw::Section);
}

#[test]
fn a_source_that_is_not_the_part_is_never_judged() {
    for p in [circular(), slot(), trochoid()] {
        assert_eq!(depth_law_of(&p, &SrcRef::Stock, false), DepthLaw::None);
        assert_eq!(depth_law_of(&p, &SrcRef::Entity("stock".into()), false), DepthLaw::None);
        assert_eq!(depth_law_of(&p, &picked(), false), DepthLaw::None);
    }
}

#[test]
fn the_slot_answers_to_the_same_authored_law() {
    // #1504 / #1537: picked or whole model, the Slot keeps the part.
    assert!(matches!(slot(), super::OpParams::Slot { .. }), "the seed is a Slot");
    for src in [SrcRef::Entity("part".into()), picked()] {
        assert_eq!(depth_law_of(&slot(), &src, true), authored_law(&src));
        assert_eq!(depth_law_of(&circular(), &src, true), authored_law(&src));
    }
}

#[test]
fn the_shared_authored_law_reads_the_source_alone() {
    // The one law Circular and Slot share: the finished part is judged,
    // picked or whole; the Stock never is.
    assert_eq!(authored_law(&SrcRef::Entity("part".into())), DepthLaw::Section);
    assert_eq!(authored_law(&picked()), DepthLaw::Section);
    assert_eq!(authored_law(&SrcRef::Stock), DepthLaw::None);
}

#[test]
fn the_trochoidal_slot_answers_to_the_same_authored_law() {
    // #1593: its endpoints and width are typed like the Slot's, so a
    // channel through the wall or under the floor gouges the part,
    // picked or whole model.
    assert!(matches!(trochoid(), super::OpParams::TrochoidalSlot { .. }), "the seed is a Trochoidal slot");
    for src in [SrcRef::Entity("part".into()), picked()] {
        assert_eq!(depth_law_of(&trochoid(), &src, true), DepthLaw::Section);
    }
}

#[test]
fn the_2d_chamfer_alone_stays_unjudged_on_the_part() {
    // #1593 audit, as #1595 and #1603 left it: the one 2D kind still at
    // `None` on a finished-part source is the 2D Chamfer - it enters
    // the part by design at a depth derived from its width and angle,
    // and the program sim judges it against its declared break (#303).
    // Moving it is a decision, not a drift.
    let p = seed_params(OpType::Chamfer2D.ordinal());
    assert_eq!(depth_law_of(&p, &SrcRef::Entity("part".into()), true), DepthLaw::None, "Chamfer2D whole");
    assert_eq!(depth_law_of(&p, &picked(), true), DepthLaw::None, "Chamfer2D picked");
}

#[test]
fn the_pocket_family_keeps_a_whole_model_and_sections_a_pick() {
    // #1603: a whole model is cleared around and kept (`Keep`: an area
    // offset or extension into it gouges); a pick is `Section` as before.
    let whole = SrcRef::Entity("part".into());
    for t in [OpType::Pocket, OpType::Adaptive2D, OpType::Offset2D] {
        let p = seed_params(t.ordinal());
        assert!(super::keep_law::is_keep_kind(&p), "{t:?}");
        assert_eq!(depth_law_of(&p, &whole, true), DepthLaw::Keep, "{t:?} whole");
        assert_eq!(depth_law_of(&p, &picked(), true), DepthLaw::Section, "{t:?} picked");
        // Not a `part_solid` job: the stock-source reading is the
        // job's `keep_parts`, never this table.
        assert_eq!(depth_law_of(&p, &SrcRef::Stock, false), DepthLaw::None, "{t:?} stock");
    }
    for t in [OpType::Contour, OpType::Face, OpType::Slot, OpType::Engrave, OpType::Trace, OpType::Chamfer2D, OpType::Drill] {
        assert!(!super::keep_law::is_keep_kind(&seed_params(t.ordinal())), "{t:?}");
    }
}

#[test]
fn engrave_and_trace_on_the_part_answer_to_the_mark_law() {
    // #1595: they enter the part by design, so `Section` would convict
    // every legal engraving - but only as a mark, whole model or pick.
    // A source that is not the part is never judged.
    let whole = SrcRef::Entity("part".into());
    for t in [OpType::Trace, OpType::Engrave] {
        let p = seed_params(t.ordinal());
        assert_eq!(depth_law_of(&p, &whole, true), DepthLaw::Mark, "{t:?} whole");
        assert_eq!(depth_law_of(&p, &picked(), true), DepthLaw::Mark, "{t:?} picked");
        assert_eq!(depth_law_of(&p, &SrcRef::Stock, false), DepthLaw::None, "{t:?} stock");
        assert_eq!(depth_law_of(&p, &picked(), false), DepthLaw::None, "{t:?} picked off the stock");
    }
}
