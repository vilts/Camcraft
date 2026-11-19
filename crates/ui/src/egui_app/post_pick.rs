//! The post picker's list and its selected entry (#2380): the shipped
//! posts whose file is in the post folder, and the document's post found
//! in that list BY NAME. A post whose file is gone stays the selection and
//! says so; the picker never shows a different post as chosen, and it
//! never changes the document's post by itself.
//!
//! Provenance: our own rule, from the review of PR #2414 (the default post
//! resolves through `posting::post_name`, as the post itself does).

use camcraft_app::posting::{self, FolderScan};
use camcraft_app::schema::PostRef;
use std::path::PathBuf;

/// The text after a post's name when its file is not in the post folder.
pub(super) const MISSING: &str = "(file missing)";

/// What the post combo shows for one document post and one folder scan.
pub(super) struct PostChoice<'a> {
    /// The shipped posts whose file is in the folder, in table order.
    pub names: Vec<&'static str>,
    /// The document's post in `names`, found by name.
    pub sel: Option<usize>,
    /// The document's post among the folder's other files.
    pub file_sel: Option<&'a (String, PathBuf)>,
    /// The combo's selected text: always the post that the program uses.
    pub text: String,
}

/// The list and selection for `current` against `scan`. Never indexes
/// past the list: a post that is not in it shows its own name and
/// [`MISSING`], a file post that is not in it shows "custom".
pub(super) fn choose<'a>(current: &PostRef, scan: &'a FolderScan) -> PostChoice<'a> {
    let names: Vec<&'static str> = posting::BUILTINS.iter().map(|(n, _)| *n).filter(|n| scan.present.contains(n)).collect();
    let wanted = posting::post_name(current);
    let sel = wanted.and_then(|w| names.iter().position(|m| *m == w));
    let file_sel = match current {
        PostRef::File(f) => scan.extra.iter().find(|(_, p)| p.display().to_string() == *f),
        _ => None,
    };
    let text = match (sel.and_then(|i| names.get(i)), file_sel, wanted) {
        (Some(n), _, _) => (*n).to_owned(),
        (None, Some((n, _)), _) => n.clone(),
        (None, None, Some(w)) => format!("{w} {MISSING}"),
        (None, None, None) => "custom".to_owned(),
    };
    PostChoice { names, sel, file_sel, text }
}

/// The line under the picker when the post folder did not read.
pub(super) fn folder_problem(why: &str) -> String {
    format!("The post folder could not be read: {why}")
}

#[cfg(test)]
thread_local! {
    /// A test's own post folder scan, for this thread only (the shared
    /// scan reads the real folder, and other tests run beside this one).
    pub(super) static SCAN_FOR_TEST: std::cell::RefCell<Option<FolderScan>> = const { std::cell::RefCell::new(None) };
}

/// The post folder as last scanned (a test can set its own on its thread).
pub(super) fn scan() -> FolderScan {
    #[cfg(test)]
    if let Some(s) = SCAN_FOR_TEST.with(|s| s.borrow().clone()) {
        return s;
    }
    posting::post_scan()
}
