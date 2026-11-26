//! Issue #2340: the status strip counts the operations in the singular for
//! one and in the plural for more.
//! Provenance: our own behaviour tests over the real shell harness.

use super::*;

/// The painted "<n> Operation(s)" texts: the status strip's count label.
fn count_labels(h: &Harness<'static, ShellUi>) -> Vec<String> {
    painted(h)
        .into_iter()
        .map(|(t, _)| t)
        .filter(|t| {
            t.split_once(' ').is_some_and(|(n, word)| {
                n.chars().all(|c| c.is_ascii_digit()) && (word == "Operation" || word == "Operations")
            })
        })
        .collect()
}

#[test]
fn the_status_strip_counts_one_operation_in_the_singular_and_two_in_the_plural() {
    let _g = snapshot_guard();
    let mut h = shell_harness();
    h.run();
    assert_eq!(count_labels(&h), ["1 Operation"], "the seed holds one operation");
    {
        let st = &mut h.state_mut().state;
        let seed = model::find_by_name(&st.world, "Contour1")
            .and_then(|e| model::operation_of(&st.world, &e))
            .expect("seed op");
        st.pending.push_back(CommandKind::CreateOperation {
            name: "Contour2".into(),
            tool: seed.tool,
            src: seed.src,
            params: seed.params,
        });
        st.drain_actions();
    }
    h.run();
    assert!(model::find_by_name(&h.state().state.world, "Contour2").is_some(), "the second operation exists");
    assert_eq!(count_labels(&h), ["2 Operations"], "two operations read in the plural");
}
