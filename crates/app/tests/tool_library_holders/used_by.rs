//! #3897 (U6 of #3663): the holder list's used-by count. The key is the
//! library reference's entry name (`HolderRef::entry`); case and spacing
//! do not split a holder in two. A drawn, meshed or bare tool uses no
//! library holder. Provenance: our own tests over our own query.

use super::{BARE, DRAWN, MESH, cat, source};
use camcraft_app::commands::{AddTool, SetToolBody};
use camcraft_app::tool_body::{holder_users, tools_using_holder};

#[test]
fn the_used_by_count_counts_tools_by_entry() {
    let (w, mut h) = source();
    h.execute(&w, AddTool::new(&w, "Twin", cat("EM-0600-4FL")).expect("add"));
    h.execute(&w, SetToolBody::library(&w, "Twin", "bt30  er20", None, None).expect("holder"));
    h.execute(&w, AddTool::new(&w, "Other", cat("EM-0600-4FL")).expect("add"));
    h.execute(&w, SetToolBody::library(&w, "Other", "BT40 ER32", None, None).expect("holder"));

    assert_eq!(tools_using_holder(&w, "BT30 ER20"), vec![DRAWN.to_owned(), "Twin".to_owned()]);
    assert_eq!(tools_using_holder(&w, "bt30 ER20 "), tools_using_holder(&w, "BT30 ER20"), "the key ignores case and spacing");
    assert_eq!(tools_using_holder(&w, "BT40 ER32"), vec!["Other".to_owned()]);
    assert!(tools_using_holder(&w, "BT40 ER16").is_empty(), "a holder no tool uses counts none");
    let users: Vec<String> = holder_users(&w).into_iter().map(|(n, _)| n).collect();
    for n in [MESH, BARE] {
        assert!(!users.contains(&n.to_owned()), "{n} has no library holder");
    }
    assert_eq!(users.len(), 3, "{users:?}");

    // One undo takes the holder off again, and the count follows.
    assert!(h.undo(&w), "one undo step");
    assert!(tools_using_holder(&w, "BT40 ER32").is_empty(), "undo drops the user");
}
