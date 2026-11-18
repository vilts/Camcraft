//! #2247-#2250: the caches behind the frame-cost pieces of the #2238
//! probe. Each cache gets the rule's two pins (`docs/decisions/
//! 2238-frame-cost-rule.md` rule 5): a still frame makes no new compute
//! (a counter, never a time), and a changed input - and its undo - gives
//! the answer a fresh compute gives.
//! Provenance: our own frame-cost law over our own shell; no vendor
//! material consulted.

use camcraft_app::geometry::feature;
use camcraft_app::operation::{self, GeoSel, OpType};

use super::frame_cost_probe::{ADAPTIVE, CONTOUR, adaptive_preview, create, fig8, run, top_chains};
use super::*;

fn chips(h: &Harness<'static, ShellUi>) -> Vec<(String, bool)> {
    h.state().state.geo_chips().into_iter().map(|c| (c.label, c.missing)).collect()
}

fn align(h: &Harness<'static, ShellUi>) -> String {
    let st = &h.state().state;
    format!("{:?}", camcraft_app::pick::align_321(&st.world, &st.op_ui.geo))
}

fn set_picks(h: &mut Harness<'static, ShellUi>, items: Vec<GeoSel>) {
    run(h, |st| st.pending.push_back(CommandKind::SetOperationGeometry { name: ADAPTIVE.into(), items }));
}

/// #2247: the welded-face grouping behind the chips, the face-origin
/// targets and the 3-2-1 check is made once per feature set.
#[test]
fn pick_chips_group_the_welded_faces_once_and_follow_the_picks() {
    let mut h = fig8();
    adaptive_preview(&mut h);
    let (chips0, align0) = (chips(&h), align(&h));
    assert_eq!(chips0.len(), 1, "one welded-face chip: {chips0:?}");

    // The memo answers what a fresh grouping answers, on fig8 itself.
    let f = {
        let st = &h.state().state;
        model::find_by_name(&st.world, "fig8").and_then(|e| model::features_of(&st.world, &e)).expect("features")
    };
    let memo: Vec<_> = model::patches(&f).iter().map(|p| (p.id, p.faces.clone())).collect();
    let fresh: Vec<_> = feature::patches(&f).into_iter().map(|p| (p.id, p.faces)).collect();
    assert_eq!(memo, fresh, "a hit is the fresh grouping");

    // Still frames group nothing.
    let before = model::patch_builds();
    h.run_steps(3);
    assert_eq!(model::patch_builds() - before, 0, "a still frame groups no welded face");
    assert_eq!(chips(&h), chips0);

    // A new pick is a new chip list; the welded-face chip stays as it was.
    let mut more = h.state().state.op_ui.geo.clone();
    more.push(top_chains(&h.state().state).into_iter().next().expect("a top chain"));
    set_picks(&mut h, more);
    let chips1 = chips(&h);
    assert_eq!(chips1.len(), 2, "the chain pick adds a chip: {chips1:?}");
    assert_eq!(chips1[0], chips0[0], "the welded-face chip reads the same");

    // The undo gives the earlier answer back.
    run(&mut h, |st| st.pending.push_back(CommandKind::Undo));
    assert_eq!(chips(&h), chips0, "after the undo the chips are the first ones");
    assert_eq!(align(&h), align0, "after the undo the 3-2-1 answer is the first one");
}

/// What the page shows for the open contour, through the memo, and what
/// one fresh walk answers for the same document.
#[derive(Debug, PartialEq)]
struct Shown {
    side: Option<bool>,
    account: (Vec<String>, Option<String>),
}

fn contour_op(h: &Harness<'static, ShellUi>) -> camcraft_app::schema::Operation {
    let st = &h.state().state;
    model::find_by_name(&st.world, CONTOUR).and_then(|e| model::operation_of(&st.world, &e)).expect("the contour")
}

/// The memo's answer, asserted equal to a fresh walk's.
fn shown(h: &Harness<'static, ShellUi>, why: &str) -> Shown {
    let (st, op) = (&h.state().state, contour_op(h));
    let SrcRef::Selection(items) = &op.src else { panic!("{why}: the contour has picks") };
    let setup = op.setup.as_deref();
    let memo = Shown {
        side: operation::contour_auto_side(&st.world, setup, &op.src, &op.params),
        account: operation::selection_account(&st.world, setup, items, &op.params),
    };
    let fresh = Shown {
        side: operation::contour_auto_side_fresh(&st.world, setup, &op.src, &op.params),
        account: operation::selection_account_fresh(&st.world, setup, items, &op.params),
    };
    assert_eq!(memo, fresh, "{why}: the cached side and account are the fresh walk's");
    memo
}

/// Run `change`, then assert the page walked again and shows the fresh
/// answer.
fn changed(h: &mut Harness<'static, ShellUi>, why: &str, change: impl FnOnce(&mut State)) -> Shown {
    let before = operation::selection_walks();
    run(h, change);
    assert!(operation::selection_walks() > before, "{why}: a changed input walks again");
    shown(h, why)
}

fn set_contour(h: &mut Harness<'static, ShellUi>, why: &str, items: Vec<GeoSel>) -> Shown {
    changed(h, why, |st| st.pending.push_back(CommandKind::SetOperationGeometry { name: CONTOUR.into(), items }))
}

/// An undo may land on an input the memo still holds: then it is a hit,
/// and the hit must still be the fresh walk's answer.
fn undo(h: &mut Harness<'static, ShellUi>, why: &str) -> Shown {
    run(h, |st| st.pending.push_back(CommandKind::Undo));
    shown(h, why)
}

/// #2248, #2249: the open contour's selection account and Auto side are
/// one walk per input. A still frame walks nothing; each input change -
/// the picks, the side, the part's pose - and each undo gives the answer
/// a fresh walk gives, so the side the page names is the side the walk
/// that generation runs would take.
#[test]
fn contour_side_and_account_walk_once_per_input() {
    let mut h = fig8();
    let chains = top_chains(&h.state().state);
    assert!(chains.len() > 1, "fig8 has several top chains: {chains:?}");
    create(&mut h, CONTOUR, OpType::Contour, chains.clone());
    run(&mut h, |st| crate::actions::target(st, UiTarget::Operation(CONTOUR.into())));
    let all = shown(&h, "all top chains");

    // Still frames walk nothing, and the page shows the same answer.
    h.run_steps(2);
    let before = operation::selection_walks();
    h.run_steps(3);
    assert_eq!(operation::selection_walks() - before, 0, "a still frame walks no selection");
    assert_eq!(shown(&h, "still"), all);

    // Each top chain alone: a new pick is a new key.
    let mut sides = Vec::new();
    for (i, c) in chains.iter().enumerate() {
        sides.push(set_contour(&mut h, &format!("top chain {i} alone"), vec![c.clone()]).side);
    }
    println!("auto side per top chain: {sides:?}; all chains: {:?}", all.side);
    let back = undo(&mut h, "undo the last pick");
    assert_eq!(back.side, sides[sides.len() - 2], "the undo shows the earlier pick's side");
    // Back to the first picks: a hit or a walk, the answer is the first.
    run(&mut h, |st| st.pending.push_back(CommandKind::SetOperationGeometry { name: CONTOUR.into(), items: chains.clone() }));
    assert_eq!(shown(&h, "all chains again"), all);

    // The side parameter: a named side is no Auto verdict.
    let op = contour_op(&h);
    let OpParams::Contour { .. } = &op.params else { panic!("a contour") };
    let mut inside = op.params.clone();
    if let OpParams::Contour { side, .. } = &mut inside {
        *side = camcraft_app::machining2d::Side::Inside;
    }
    changed(&mut h, "side Inside", |st| st.pending.push_back(CommandKind::EditOperationParams { name: CONTOUR.into(), params: inside }));
    assert_eq!(undo(&mut h, "undo side Inside"), all);

    // The part's pose: a move reads new world points.
    changed(&mut h, "fig8 moved", |st| {
        st.pending.push_back(CommandKind::MoveModel { name: "fig8".into(), pos: [7.0, -3.0, 2.0], roll_deg: 0.0, pitch_deg: 0.0, yaw_deg: 30.0 });
    });
    assert_eq!(undo(&mut h, "undo the move"), all);
}

/// The pick overlay lines through the memo, asserted equal to a fresh
/// build of the same state.
fn pick_lines(h: &Harness<'static, ShellUi>, why: &str) -> Vec<[f32; 6]> {
    let st = &h.state().state;
    let memo = crate::interact::selection_feature_lines(st).to_vec();
    assert_eq!(memo, crate::interact::selection_feature_lines_fresh(st), "{why}: the cached lines are a fresh build's");
    memo
}

/// Run `change`, then assert the lines were built again and are a fresh
/// build's.
fn rebuilt(h: &mut Harness<'static, ShellUi>, why: &str, change: impl FnOnce(&mut State)) -> Vec<[f32; 6]> {
    let before = crate::pick_line_memo::pick_line_builds();
    run(h, change);
    let lines = pick_lines(h, why);
    assert!(crate::pick_line_memo::pick_line_builds() > before, "{why}: a changed input builds again");
    lines
}

/// Still frames as the shell runs them: a frame, then a scene collect.
fn still_frames(h: &mut Harness<'static, ShellUi>, n: usize) {
    for _ in 0..n {
        h.step();
        std::hint::black_box(world::collect(&h.state().state));
    }
}

/// #2250: the violet pick lines of the open operation are built once per
/// input. A still frame builds nothing; a new pick, a hidden model and a
/// moved part each give the lines a fresh build gives, and each undo
/// gives the first lines back.
#[test]
fn pick_lines_build_once_per_input() {
    let mut h = fig8();
    adaptive_preview(&mut h);
    let first = pick_lines(&h, "one welded face");
    assert!(!first.is_empty(), "the welded face draws its outline");

    // Still frames build nothing.
    still_frames(&mut h, 2);
    let before = crate::pick_line_memo::pick_line_builds();
    still_frames(&mut h, 3);
    assert_eq!(crate::pick_line_memo::pick_line_builds() - before, 0, "a still frame builds no pick line");
    assert_eq!(pick_lines(&h, "still"), first);

    // A new pick: more lines, built again.
    let mut more = h.state().state.op_ui.geo.clone();
    more.push(top_chains(&h.state().state).into_iter().next().expect("a top chain"));
    let two = rebuilt(&mut h, "a chain added", |st| {
        st.pending.push_back(CommandKind::SetOperationGeometry { name: ADAPTIVE.into(), items: more });
    });
    assert!(two.len() > first.len(), "the chain adds lines");
    run(&mut h, |st| st.pending.push_back(CommandKind::Undo));
    assert_eq!(pick_lines(&h, "undo the chain"), first);

    // A display-off model draws no pick.
    let hidden = rebuilt(&mut h, "fig8 hidden", |st| st.hidden_models.push("fig8".into()));
    assert!(hidden.is_empty(), "a hidden model draws no pick lines");
    run(&mut h, |st| st.hidden_models.retain(|n| n != "fig8"));
    assert_eq!(pick_lines(&h, "fig8 shown again"), first);

    // The part's pose: a move draws the outline where the part is now.
    let moved = rebuilt(&mut h, "fig8 moved", |st| {
        st.pending.push_back(CommandKind::MoveModel { name: "fig8".into(), pos: [7.0, -3.0, 2.0], roll_deg: 0.0, pitch_deg: 0.0, yaw_deg: 30.0 });
    });
    assert_ne!(moved, first, "the moved outline is in a new place");
    run(&mut h, |st| st.pending.push_back(CommandKind::Undo));
    assert_eq!(pick_lines(&h, "undo the move"), first);
}
