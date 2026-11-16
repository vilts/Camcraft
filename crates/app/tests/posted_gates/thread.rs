//! The thread-mill golden's gate (#894): `thread_mill_m8_multi_form`
//! regenerated through the real operation - the M8 x 1.25 thread in a
//! blind 96-flat round hole of a Ø36 disc, tap-drilled by a Drill
//! operation ahead (#992: a thread mill is not centre-cutting), the
//! catalog's Ø6 multi-form mill - and judged by every law its post runs:
//! the generator's own blocking warnings (the crest region, the root
//! solid law, #1205's round-wall test, #992's drilled-hole law) and the
//! post's verification gate over the whole program, drill included. The
//! thread is first proved byte-identical to its committed CL golden.
//!
//! #985: `thread_mill_external_m8_multi_form` the same way - the M8 x
//! 1.25 thread on a Ø8 boss of `thread_support::boss`'s stud, the stock
//! round it cleared by the roughing contour ahead, judged by the
//! external laws (the boss's crest region, the root and neighbour laws,
//! the air law) and the post's gate over the whole program.
//!
//! Provenance: our own harness over our own golden (the shared fixture
//! of `thread_mill.rs`, `thread_support`); no vendor material consulted.

use crate::common::gen_to_cltext;
use crate::thread_support::boss::{boss, external, stud, stud_world};
use crate::thread_support::{MULTI, R_M8, blocking, catalog_tool, make, thread, world_with};

use camcraft_app::cl::ThreadHand;
use camcraft_app::commands::{CreateOperation, GenerateOperation};
use camcraft_app::model;
use camcraft_app::operation::SrcRef;
use camcraft_app::posting;

use super::golden;

/// (golden, every blocking finding) for each thread golden.
pub fn thread_verdicts() -> Vec<(&'static str, Vec<String>)> {
    vec![internal_verdict(), external_verdict()]
}

fn internal_verdict() -> (&'static str, Vec<String>) {
    let (world, mut history) = world_with(R_M8, &[(MULTI, catalog_tool(MULTI))]);
    let path = make(&world, &mut history, MULTI, thread(R_M8, None, ThreadHand::Right, true));
    let name = "thread_mill_m8_multi_form.cl";
    assert_eq!(gen_to_cltext(&path), golden(name), "{name}: the regenerated program is the golden");
    let mut w = blocking(&world);
    w.extend(posting::verification_gate(&world));
    (name, w)
}

fn external_verdict() -> (&'static str, Vec<String>) {
    let (world, mut history) = stud_world(stud(4.0, Vec::new()));
    let op = CreateOperation::new(&world, "Thread1", MULTI, SrcRef::Entity("stud".into()), external(vec![boss(4.0)], None, ThreadHand::Right, true));
    history.execute(&world, op.expect("create the thread"));
    history.execute(&world, GenerateOperation::new(&world, "Thread1").expect("generate the thread"));
    let e = model::find_by_name(&world, "Thread1").expect("op");
    let path = model::toolpath_of(&world, &e).expect("cached path").path;
    let name = "thread_mill_external_m8_multi_form.cl";
    assert_eq!(gen_to_cltext(&path), golden(name), "{name}: the regenerated program is the golden");
    let mut w = blocking(&world);
    w.extend(posting::verification_gate(&world));
    (name, w)
}
