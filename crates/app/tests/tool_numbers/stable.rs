//! #2442: a tool's number never shifts when other tools come and go -
//! adding, deleting or importing a tool keeps every posted T/H word - and a
//! legacy project without numbers keeps the numbers it posted before
//! (#2517: in a document order that is not the name order, and mixed with
//! tools that carry their own number). Split from `tool_numbers.rs` (#2517).

use super::*;

fn post_nc(world: &World, tag: &str) -> String {
    let path = std::env::temp_dir().join(format!(
        "camcraft_tool_numbers_{tag}_{}.nc",
        std::process::id()
    ));
    let posted = posting::post_setup(world, &path);
    let _ = std::fs::remove_file(&path);
    posted.expect("program posts").program
}

#[test]
fn adding_an_earlier_sorting_tool_keeps_posted_t_and_h_words() {
    let (world, mut h) = two_tool_job();
    let before_tools = posted_tools(&world).expect("posts before");
    let before_nc = post_nc(&world, "before_add");
    assert!(
        before_nc.contains("T01")
            && before_nc.contains("H01")
            && before_nc.contains("T02")
            && before_nc.contains("H02"),
        "{before_nc}"
    );

    // Adding a tool whose name sorts earlier than both existing tools
    // (and one sorting between them) must not renumber any existing tool.
    h.execute(
        &world,
        AddTool::new(&world, "AA-FIRST", tool("EM-0600-4FL")).expect("add AA-FIRST"),
    );
    h.execute(
        &world,
        AddTool::new(&world, "EM-0500-MID", tool("EM-0600-4FL")).expect("add EM-0500-MID"),
    );

    let after_tools = posted_tools(&world).expect("posts after");
    let after_nc = post_nc(&world, "after_add");
    assert_eq!(
        after_tools, before_tools,
        "existing tools keep their posted T numbers after adding earlier-sorting tools"
    );
    assert_eq!(
        after_nc, before_nc,
        "re-posting the same operations emits the exact same T/H program"
    );
    let aa = tool_numbers::number_of(&world, "AA-FIRST").expect("AA-FIRST number");
    let mid = tool_numbers::number_of(&world, "EM-0500-MID").expect("EM-0500-MID number");
    assert_ne!(aa, before_tools[0].0);
    assert_ne!(aa, before_tools[1].0);
    assert_ne!(mid, before_tools[0].0);
    assert_ne!(mid, before_tools[1].0);
    assert_ne!(aa, mid);
}

#[test]
fn deleting_an_earlier_tool_keeps_remaining_posted_t_and_h_words() {
    let (world, mut h) = two_tool_job();
    // Add an unused earlier tool, post, then delete it: the used tools
    // keep their exact posted T/H words.
    h.execute(
        &world,
        AddTool::new(&world, "AA-UNUSED", tool("EM-0600-4FL")).expect("add unused"),
    );
    let before_tools = posted_tools(&world).expect("posts before delete");
    let before_nc = post_nc(&world, "before_delete");

    h.execute(
        &world,
        DeleteTool::new(&world, "AA-UNUSED").expect("delete AA-UNUSED"),
    );
    assert_eq!(posted_tools(&world).expect("posts after delete"), before_tools);
    assert_eq!(post_nc(&world, "after_delete"), before_nc);

    // Undo restores the deleted tool without shifting the others either.
    assert!(h.undo(&world));
    assert_eq!(posted_tools(&world).expect("posts after undo"), before_tools);
    assert_eq!(post_nc(&world, "after_undo"), before_nc);

    // Also delete a tool added before EM-0600-4FL (in a single-op program
    // on EM-0600-4FL) and verify EM-0600-4FL keeps T2.
    let (w2, mut h2) = two_tool_job();
    let em6_before = tool_numbers::number_of(&w2, "EM-0600-4FL").expect("EM-0600-4FL number");
    h2.execute(&w2, camcraft_app::commands::DeleteOperation::new(&w2, "Face1").expect("del Face1"));
    h2.execute(&w2, DeleteTool::new(&w2, "EM-0400-2FL").expect("del EM-0400-2FL"));
    h2.execute(&w2, camcraft_app::commands::GenerateOperation::new(&w2, "Face2").expect("regen Face2"));
    assert_eq!(tool_numbers::number_of(&w2, "EM-0600-4FL"), Some(em6_before));
    assert_eq!(posted_tools(&w2).expect("posts remaining"), vec![(em6_before, "EM-0600-4FL".to_owned())]);
}

#[test]
fn legacy_unnumbered_project_keeps_posted_numbers_after_add_and_delete() {
    // A project saved before #2442 without "number" keys on its tools.
    let (seed, _h) = two_tool_job();
    let raw = project::save_text(&seed).expect("save seed");
    let mut json: serde_json::Value = serde_json::from_str(&raw).expect("parse json");
    for e in json["entities"].as_array_mut().expect("entities") {
        if let Some(t) = e.get_mut("tool").and_then(serde_json::Value::as_object_mut) {
            t.remove("number");
        }
    }
    let world = new_world();
    project::open(&world, &json.to_string()).expect("load unnumbered legacy project");

    let before_tools = posted_tools(&world).expect("posts legacy");
    let before_nc = post_nc(&world, "legacy_before");

    let mut h = History::default();
    h.execute(
        &world,
        AddTool::new(&world, "AA-FIRST", tool("EM-0600-4FL")).expect("add AA-FIRST"),
    );
    assert_eq!(
        posted_tools(&world).expect("posts after add"),
        before_tools,
        "legacy project tools keep their numbers when an earlier-sorting tool is added"
    );
    assert_eq!(post_nc(&world, "legacy_after_add"), before_nc);

    h.execute(
        &world,
        DeleteTool::new(&world, "AA-FIRST").expect("delete AA-FIRST"),
    );
    assert_eq!(posted_tools(&world).expect("posts after delete"), before_tools);
    assert_eq!(post_nc(&world, "legacy_after_delete"), before_nc);

    h.execute(&world, camcraft_app::commands::DeleteOperation::new(&world, "Face1").expect("del Face1"));
    h.execute(&world, DeleteTool::new(&world, "EM-0400-2FL").expect("del EM-0400-2FL"));
    h.execute(&world, camcraft_app::commands::GenerateOperation::new(&world, "Face2").expect("regen Face2"));
    assert_eq!(
        posted_tools(&world).expect("posts after deleting first legacy tool"),
        vec![before_tools[1].clone()]
    );
}

#[test]
fn importing_earlier_sorting_tool_keeps_existing_numbers() {
    let (world, mut h) = two_tool_job();
    let before_tools = posted_tools(&world).expect("posts before import");
    let before_nc = post_nc(&world, "before_import");

    let lib = vec![LibraryTool::bare("AA-IMPORTED", tool("EM-0800-4FL"))];
    h.execute(&world, ImportTools::new(&world, lib).expect("import"));

    assert_eq!(posted_tools(&world).expect("posts after import"), before_tools);
    assert_eq!(post_nc(&world, "after_import"), before_nc);
    let imported = tool_numbers::number_of(&world, "AA-IMPORTED").expect("imported tool number");
    assert_eq!(number(&world, "AA-IMPORTED"), Some(imported));
    assert!(!before_tools.iter().any(|(n, _)| *n == imported));
}

/// A saved project of `world` as a pre-#2442 file: the "number" key is
/// removed from every tool except the ones named in `keep`.
fn legacy_text(world: &World, keep: &[&str]) -> String {
    let raw = project::save_text(world).expect("save seed");
    let mut json: serde_json::Value = serde_json::from_str(&raw).expect("parse json");
    for e in json["entities"].as_array_mut().expect("entities") {
        let name = e["name"].as_str().unwrap_or_default().to_owned();
        if let Some(t) = e.get_mut("tool").and_then(serde_json::Value::as_object_mut)
            && !keep.contains(&name.as_str())
        {
            t.remove("number");
        }
    }
    json.to_string()
}

/// `text` with the entity `name` moved to the front of the entity list: a
/// file whose order is not the name order a save writes (an older build, or
/// a hand edit).
fn put_first(text: &str, name: &str) -> String {
    let mut json: serde_json::Value = serde_json::from_str(text).expect("parse json");
    let list = json["entities"].as_array_mut().expect("entities");
    let at = list.iter().position(|e| e["name"] == name).expect("the entity is in the file");
    let e = list.remove(at);
    list.insert(0, e);
    json.to_string()
}

/// The tool names of a saved project, in file (document) order.
fn file_tool_order(text: &str) -> Vec<String> {
    let json: serde_json::Value = serde_json::from_str(text).expect("parse json");
    json["entities"]
        .as_array()
        .expect("entities")
        .iter()
        .filter(|e| e.get("tool").is_some_and(|t| !t.is_null()))
        .filter_map(|e| e["name"].as_str().map(str::to_owned))
        .collect()
}

#[test]
fn a_legacy_project_numbers_by_name_order_not_by_document_order() {
    // #2517: EM-0600 was added first AND stands first in the file, so the
    // document order and the name order differ (a save writes name order, so
    // the file is reordered by hand, as an older build or an edit leaves it).
    // The legacy law freezes each tool to its 1-based place in the
    // NAME-sorted list - what such a file posted before #2442.
    let (seed, _h) = tool_job(["EM-0600-4FL", "EM-0400-2FL"]);
    let text = put_first(&legacy_text(&seed, &[]), "EM-0600-4FL");
    assert_eq!(
        file_tool_order(&text),
        ["EM-0600-4FL", "EM-0400-2FL"],
        "the fixture's document order is not its name order"
    );
    let world = new_world();
    project::open(&world, &text).expect("load unnumbered legacy project");
    assert_eq!(number(&world, "EM-0400-2FL"), Some(1), "first by name");
    assert_eq!(number(&world, "EM-0600-4FL"), Some(2), "second by name, though added first");
    assert_eq!(
        posted_tools(&world).expect("posts legacy"),
        vec![(1, "EM-0400-2FL".to_owned()), (2, "EM-0600-4FL".to_owned())]
    );
    let nc = post_nc(&world, "legacy_name_order");
    assert!(nc.contains("T01") && nc.contains("T02"), "{nc}");
}

#[test]
fn a_legacy_project_mixing_numbered_and_unnumbered_tools_keeps_both() {
    // #2517: EM-0400 carries its own T7; AA-SPARE (unused) and EM-0600
    // have no number. A carried number stays; each other tool freezes to
    // its place in the name-sorted list of ALL tools: AA-SPARE 1, EM-0400
    // (place 2, but it keeps 7), EM-0600 3.
    let (seed, mut h) = two_tool_job();
    h.execute(&seed, AddTool::new(&seed, "AA-SPARE", tool("EM-0600-4FL")).expect("add AA-SPARE"));
    set(&seed, &mut h, "EM-0400-2FL", Some(7));
    let text = legacy_text(&seed, &["EM-0400-2FL"]);
    let world = new_world();
    project::open(&world, &text).expect("load mixed legacy project");
    assert_eq!(number(&world, "AA-SPARE"), Some(1));
    assert_eq!(number(&world, "EM-0400-2FL"), Some(7), "a carried number is kept");
    assert_eq!(number(&world, "EM-0600-4FL"), Some(3), "its place among all three tools by name");
    let before = posted_tools(&world).expect("posts mixed legacy");
    assert_eq!(before, vec![(7, "EM-0400-2FL".to_owned()), (3, "EM-0600-4FL".to_owned())]);
    // The frozen numbers hold when a tool that sorts first comes and goes.
    let mut h = History::default();
    h.execute(&world, AddTool::new(&world, "AA-FIRST", tool("EM-0600-4FL")).expect("add AA-FIRST"));
    assert_eq!(posted_tools(&world).expect("posts after add"), before);
    h.execute(&world, DeleteTool::new(&world, "AA-SPARE").expect("delete AA-SPARE"));
    assert_eq!(posted_tools(&world).expect("posts after delete"), before);
}
