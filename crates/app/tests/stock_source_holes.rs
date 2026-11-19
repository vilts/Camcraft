//! #1631: a stock-source 2D Chamfer, Drill, Bore or Thread answers for
//! the parts standing in its stock on the operation itself. #1607 gave
//! the other 2D kinds that law (`operation::keep_law::parts_in_stock`);
//! these four stayed at `DepthLaw::None`, so a drill, bar, thread mill or
//! chamfer cone cut off the stock and into the part had 0 findings - with
//! the stock aligned the program sim held the post, with it turned (the
//! sim refuses to judge) the post went out. #2177: the device is a turn
//! off the machine axes; the sim judges a quarter-turned box.
//!
//! Now the hole kinds take their own containment law against the setup's
//! parts: a hole may enter a part only through the hole the part models
//! there, to its diameter and no deeper than its floor (the declared
//! hole's round wall excused, nothing else); the 2D Chamfer takes the
//! solid law - the edge it breaks is the stock's, not the part's. A cut
//! into the part is a blocking `Warning::Gouge` on the operation on both
//! stocks; a hole in the part's own modelled hole, a hole in stock-only
//! material and a chamfer on stock edges clear of the part post on both.
//!
//! Provenance: our own plate fixture (`common::solid_part`, round holes
//! as inscribed 32- and 96-gons) over the containment and solid laws and the
//! column sim; no vendor material consulted.
//!
//! Gate map: covers app::operation::keep_law (is_stock_kept_kind,
//! is_hole_kind, parts_in_stock), app::operation::generate (the
//! stock-source hole and chamfer laws), app::posting, app::posting_sim,
//! app::mcp (add_operation, verify).

mod common;
mod pocket_support;
mod stock_source_holes_support;

use stock_source_holes_support::*;

#[test]
fn a_hole_kind_off_the_stock_into_the_part_gouges_and_never_posts() {
    // The part models one hole at the origin (per kind: r 3 for the
    // 6 mm drill, r 2.5 - the tap drill - for the M6 tap, r 5 for the
    // bore, r 4.4 at the thread's minor). Each
    // cut below enters the part's solid: where it models no hole, wider
    // than its hole, or deeper than its floor.
    let cases = [
        ("drill-nowhere", Kind::Drill, Model::hole(3.0, 8.0), Cut::at(0.0, 9.0, 3.0, 10.0)),
        ("drill-too-deep", Kind::Drill, Model::hole(3.0, 12.0), Cut::at(0.0, 0.0, 3.0, 9.0)),
        ("tap-nowhere", Kind::Tap, Model::hole(2.5, 8.0), Cut::at(0.0, 9.0, 2.5, 10.0)),
        ("tap-too-deep", Kind::Tap, Model::hole(2.5, 12.0), Cut::at(0.0, 0.0, 2.5, 9.0)),
        ("bore-nowhere", Kind::Bore, Model::hole(5.0, 8.0), Cut::at(0.0, 9.0, 5.0, 10.0)),
        ("bore-too-wide", Kind::Bore, Model::hole(5.0, 8.0), Cut::at(0.0, 0.0, 6.0, 10.0)),
        ("thread-nowhere", Kind::Thread, Model::fine(4.4, 4.0), Cut::at(0.0, 9.5, 4.4, 10.0)),
        ("thread-too-deep", Kind::Thread, Model::fine(4.4, 14.0), Cut::at(0.0, 0.0, 4.4, 10.0)),
    ];
    for (label, kind, model, cut) in &cases {
        for rotated in [false, true] {
            let (world, mut h) = job(*model, HALF, rotated);
            if rotated && matches!(kind, Kind::Thread) {
                let e = thread_refused_on_a_turned_stock(&world, &mut h, *cut);
                assert!(e.contains("is not drilled yet"), "{label}: refused by name: {e}");
                assert_not_posted(&world, &format!("{label}-{rotated}"), "S1: no toolpath yet");
                continue;
            }
            let warns = hole_op(&world, &mut h, *kind, *cut);
            assert!(gouges(&warns) > 0, "{label} rotated {rotated}: convicted on the op: {warns:?}");
            assert_post_refused(&world, &format!("{label}-{rotated}"), rotated);
        }
    }
}

#[test]
fn a_hole_kind_in_the_parts_own_hole_or_in_stock_only_material_posts() {
    // At the modelled hole, to its size and above its floor; and at
    // (25, 0), where only the stock stands (the part ends at 15).
    let cases = [
        ("drill-in-hole", Kind::Drill, Model::hole(3.0, 8.0), Cut::at(0.0, 0.0, 3.0, 10.0)),
        ("drill-stock-only", Kind::Drill, Model::hole(3.0, 8.0), Cut::at(25.0, 0.0, 3.0, 10.0)),
        ("tap-in-hole", Kind::Tap, Model::hole(2.5, 8.0), Cut::at(0.0, 0.0, 2.5, 10.0)),
        ("bore-in-hole", Kind::Bore, Model::hole(5.0, 8.0), Cut::at(0.0, 0.0, 5.0, 10.0)),
        ("bore-stock-only", Kind::Bore, Model::hole(5.0, 8.0), Cut::at(25.0, 0.0, 5.0, 10.0)),
        ("thread-in-hole", Kind::Thread, Model::fine(4.4, 4.0), Cut::at(0.0, 0.0, 4.4, 10.0)),
        ("thread-stock-only", Kind::Thread, Model::fine(4.4, 4.0), Cut::at(25.0, 0.0, 4.4, 10.0)),
    ];
    for (label, kind, model, cut) in &cases {
        for rotated in [false, true] {
            // A Thread off a turned stock never generates (#992's way-in
            // law cannot read the program ahead there): the refusal case
            // above pins it.
            if rotated && matches!(kind, Kind::Thread) {
                continue;
            }
            let (world, mut h) = job(*model, HALF, rotated);
            let warns = hole_op(&world, &mut h, *kind, *cut);
            let blocking: Vec<_> = warns.iter().filter(|w| w.blocks_post()).collect();
            assert!(blocking.is_empty(), "{label} rotated {rotated}: {blocking:?}");
            assert_posts(&world, &format!("{label}-{rotated}"));
        }
    }
}

#[test]
fn a_chamfer_off_a_stock_flush_with_the_part_gouges_and_one_clear_of_it_posts() {
    // A 30 x 30 stock stands flush with the 30 x 30 part: its top edge
    // IS the part's, and a 1 mm break off the stock takes the part's
    // edge with no law that granted it. A 70 x 70 stock's edges stand
    // 20 mm clear of the part: the same chamfer posts.
    for rotated in [false, true] {
        let (world, mut h) = job(Model::hole(3.0, 8.0), 15.0, rotated);
        let warns = generated(&world, &mut h, CHAMFER, chamfer(1.0));
        assert!(gouges(&warns) > 0, "flush rotated {rotated}: convicted on the op: {warns:?}");
        assert_post_refused(&world, &format!("chamfer-flush-{rotated}"), rotated);

        let (world, mut h) = job(Model::hole(3.0, 8.0), HALF, rotated);
        let warns = generated(&world, &mut h, CHAMFER, chamfer(1.0));
        let blocking: Vec<_> = warns.iter().filter(|w| w.blocks_post()).collect();
        assert!(blocking.is_empty(), "clear rotated {rotated}: {blocking:?}");
        assert_posts(&world, &format!("chamfer-clear-{rotated}"));
    }
}

#[test]
fn a_tap_through_a_neighbouring_part_is_not_acquitted_at_its_core() {
    // #266 re-reads a tap's cutting moves at its thread core and drops
    // every full-diameter gouge the core run does not raise. That run
    // read only the source's own law, so the parts a tap answers to
    // beyond it - #1621's neighbours, a stock source's parts - were
    // acquitted unread: an M6 tap fed through a lid that models no hole,
    // on its way into the tap-drill hole of the part under it, carried
    // no gouge on the op. The core now answers to every part law.
    for rotated in [false, true] {
        let warns = tap_through_a_lid(rotated);
        assert!(gouges(&warns) > 0, "rotated {rotated}: the lid is convicted: {warns:?}");
    }
}

#[test]
fn a_stock_source_hole_or_chamfer_with_no_part_in_the_setup_judges_nothing() {
    for (label, kind) in [("drill", Kind::Drill), ("bore", Kind::Bore)] {
        let (world, mut h) = job_without_part(HALF);
        let warns = hole_op(&world, &mut h, kind, Cut::at(0.0, 9.0, if label == "drill" { 3.0 } else { 5.0 }, 10.0));
        assert_eq!(gouges(&warns), 0, "{label}: no part, no part law: {warns:?}");
    }
    let (world, mut h) = job_without_part(15.0);
    let warns = generated(&world, &mut h, CHAMFER, chamfer(1.0));
    assert_eq!(gouges(&warns), 0, "chamfer: no part, no part law: {warns:?}");
}

#[test]
fn over_mcp_a_stock_source_hole_or_chamfer_answers_for_the_part_the_same() {
    // MCP's `add_operation` defaults the source to the stock.
    let drill = |x: f64, y: f64| json!({ "holes": [{ "xy": [x, y], "radius": 3.0, "entry_z": 20.0, "floor_z": 10.0, "through": false }] });
    let bad = mcp_verify("drill-nowhere", 35.0, "Drill", DRILL, drill(0.0, 9.0));
    assert!(op_gouges(&bad) > 0, "a drill into the part is the op's own gouge: {bad}");
    assert_eq!(bad["ok"], json!(false), "{bad}");
    for (tag, x) in [("drill-in-hole", 0.0), ("drill-stock-only", 25.0)] {
        let good = mcp_verify(tag, 35.0, "Drill", DRILL, drill(x, 0.0));
        assert_eq!(op_gouges(&good), 0, "{tag}: {good}");
        assert_eq!(good["ok"], json!(true), "{tag}: {good}");
    }
    let chamfer = json!({ "width": 1.0, "angle_deg": 45.0, "top_z": 20.0 });
    let flush = mcp_verify("chamfer-flush", 15.0, "2D Chamfer", CHAMFER, chamfer.clone());
    assert!(op_gouges(&flush) > 0, "a chamfer off a flush stock breaks the part: {flush}");
    assert_eq!(flush["ok"], json!(false), "{flush}");
    let clear = mcp_verify("chamfer-clear", 35.0, "2D Chamfer", CHAMFER, chamfer);
    assert_eq!(op_gouges(&clear), 0, "{clear}");
    assert_eq!(clear["ok"], json!(true), "{clear}");
}

#[test]
fn a_part_added_under_a_stock_source_drill_is_judged_at_the_post_by_the_hole_law() {
    // #1691: on a turned stock the program sim does not judge, so the post
    // judges the kept parts again (#1628) - by generation's own law. A
    // Drill off the stock at (25, 0), where only the stock stands, posts.
    // A second part then added there: with its own modelled hole at the
    // drill's size the hole law excuses its faceted wall and the program
    // posts; solid there, the drill cuts it and the post refuses by name.
    for (label, hole) in [("holed", true), ("solid", false)] {
        let (world, mut h) = job(Model::hole(3.0, 8.0), HALF, true);
        let warns = hole_op(&world, &mut h, Kind::Drill, Cut::at(25.0, 0.0, 3.0, 10.0));
        assert!(warns.iter().all(|w| !w.blocks_post()), "{label}: a clean generation: {warns:?}");
        assert!(pocket_support::oblique_refused(&world), "{label}: the device holds: the program sim does not judge this stock");
        assert_posts(&world, &format!("added-{label}-before"));
        let mut f = Vec::new();
        let ring = common::solid_part::circle((25.0, 0.0), 3.0, 32);
        let rim = hole.then_some((ring.as_slice(), 8.0));
        common::solid_part::plate(&common::solid_part::rect(20.0, -5.0, 30.0, 5.0), rim, 0.0, TOP, &mut f);
        h.execute(&world, ImportModel::from_mesh("other", Mesh::from_facets(f)));
        if hole {
            assert_posts(&world, "added-holed");
        } else {
            assert_not_posted(&world, "added-solid", "S1 now cuts part 'other'");
        }
    }
}

#[test]
fn stock_source_chamfer_refusal_names_cause_and_part_as_source_workaround() {
    // #1637: a stock-source 2D Chamfer flush with the part breaks into
    // the part; its refusal on generate, post, and MCP verify must name
    // the cause and tell the user to chamfer with the part as source.
    for rotated in [false, true] {
        let (world, mut h) = job(Model::hole(3.0, 8.0), 15.0, rotated);
        let warns = generated(&world, &mut h, CHAMFER, chamfer(1.0));
        let hints: Vec<String> = warns.iter().filter(|w| w.blocks_post()).map(ToString::to_string).collect();
        assert!(
            hints.iter().any(|m| m.contains("stock-source chamfer breaks into a part") && m.contains("chamfer with the part as source")),
            "generate rotated {rotated}: expected cause + workaround in warnings: {hints:?}"
        );
        if rotated {
            assert!(pocket_support::oblique_refused(&world), "the device holds: the program sim does not judge this stock");
            assert_not_posted(&world, "chamfer-hint-rotated", "chamfer with the part as source");
        }

        // A part added under a previously clear rotated-stock chamfer is
        // also caught at the post by the same hint.
        if rotated {
            let (re_world, mut rh) = job(Model::hole(3.0, 8.0), HALF, true);
            let clear_warns = generated(&re_world, &mut rh, CHAMFER, chamfer(1.0));
            assert!(clear_warns.iter().all(|w| !w.blocks_post()), "{clear_warns:?}");
            let mut f = Vec::new();
            common::solid_part::plate(&common::solid_part::rect(30.0, -5.0, 35.0, 5.0), None, 0.0, TOP, &mut f);
            rh.execute(&re_world, ImportModel::from_mesh("other", Mesh::from_facets(f)));
            assert_not_posted(&re_world, "chamfer-rejudge-hint", "chamfer with the part as source");
        }

        // Choosing the part as source produces a clean path that posts.
        let (part_world, mut ph) = job(Model::hole(3.0, 8.0), 15.0, rotated);
        ph.execute(
            &part_world,
            CreateOperation::new(&part_world, "S1", CHAMFER, SrcRef::Entity("part".into()), chamfer(1.0)).expect("create part-source chamfer"),
        );
        ph.execute(&part_world, GenerateOperation::new(&part_world, "S1").expect("generate part-source chamfer"));
        let e = model::find_by_name(&part_world, "S1").expect("op");
        let tp = model::toolpath_of(&part_world, &e).expect("cached path");
        assert!(tp.warnings.iter().all(|w| !w.blocks_post()), "part-source rotated {rotated}: {:?}", tp.warnings);
        assert_posts(&part_world, &format!("chamfer-part-source-{rotated}"));
    }

    let chamfer_json = json!({ "width": 1.0, "angle_deg": 45.0, "top_z": 20.0 });
    let flush = mcp_verify("chamfer-hint-mcp", 15.0, "2D Chamfer", CHAMFER, chamfer_json);
    assert_eq!(flush["ok"], json!(false), "{flush}");
    let findings = flush["findings"].as_array().expect("findings array");
    assert!(
        findings.iter().any(|f| {
            f["op"] == json!("S1")
                && f["kind"] == json!("gouge")
                && f["blocks_post"] == json!(true)
                && f["message"]
                    .as_str()
                    .is_some_and(|m| m.contains("stock-source chamfer breaks into a part") && m.contains("chamfer with the part as source"))
        }),
        "MCP verify findings missing cause + workaround: {flush}"
    );
}

#[test]
fn framed_stock_chamfer_live_refusal_keeps_hint_without_local_move_number() {
    use camcraft_app::operation::Tilt;
    // #2483: the ride plane stands at mid-wall. A 10-degree plane at the
    // stock's top (Z20) crosses the top face along a line that is no edge.
    // The #2435 wall law refuses that line by name, and the refusal is
    // right. At Z10 the plane and the corner plane Z9 cut only the walls.
    const MID: f64 = 10.0;
    let (world, mut h) = job_without_part(HALF);
    let mut params = chamfer(1.0);
    if let OpParams::Chamfer2D { tilt, top_z, .. } = &mut params {
        *tilt = Some(Tilt { basis_version: 1, a_deg: 0.0, b_deg: 10.0, origin: [0.0; 3] });
        *top_z = Height::fixed(MID);
    }
    let warns = generated(&world, &mut h, CHAMFER, params);
    assert!(warns.iter().all(|w| !w.blocks_post()), "previously clear chamfer: {warns:?}");
    let e = model::find_by_name(&world, "S1").expect("op");
    let op = model::operation_of(&world, &e).expect("operation");
    let path = model::toolpath_of(&world, &e).expect("cached path").path;
    assert!(op.params.tilt().is_some(), "exercise the operation frame");
    assert!(camcraft_app::operation::kept_rejudge::moved_part_problem(&world, "S1", &op, &path).is_none());
    let mut f = Vec::new();
    // Model the added part in the cutting plane: its top meets the
    // chamfer, while its body stays below the cutter's shank.
    common::solid_part::plate(&common::solid_part::rect(-50.0, -50.0, 50.0, 50.0), None, 0.0, MID, &mut f);
    let frame = op.params.tilt().expect("tilt").frame().expect("frame");
    let f = f.iter().map(|t| camcraft_app::geometry::Triangle::new(frame.to_world(t.a), frame.to_world(t.b), frame.to_world(t.c))).collect();
    h.execute(&world, ImportModel::from_mesh("other", Mesh::from_facets(f)));
    let refusal = camcraft_app::operation::kept_rejudge::moved_part_problem(&world, "S1", &op, &path).expect("live refusal");
    assert!(refusal.contains("S1 now cuts part 'other'"), "{refusal}");
    assert!(refusal.contains("stock-source chamfer breaks into a part"), "{refusal}");
    assert!(refusal.contains("chamfer with the part as source"), "{refusal}");
    assert!(!refusal.contains("move "), "local move numbers are not posted move numbers: {refusal}");
    assert_not_posted(&world, "framed-chamfer-live", "stock-source chamfer breaks into a part");
}
