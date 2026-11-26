//! The session's own machine (`.mch`) and post (`.cps`) files (#2950): the
//! texts the browser build read from the user's disk as bytes, kept by file
//! name for this run of the app only. A page has no paths, so the machine
//! mount and the post choice name such a file by its bare name; the readers
//! here answer that name before a disk read is tried. A text is only kept
//! when it passes the same checks a file from disk passes (machine:
//! caller validation and parse; post: compile and run on an empty stream), so a
//! bad file never enters a list. A saved project that names a file which
//! is not loaded again reads from disk, fails, and is refused by name.
//!
//! The store belongs to the thread that filled it: the browser runs the
//! whole app on one thread, and each native test thread gets its own
//! files. A reader on another thread does not find a session name, so the
//! disk read fails and the caller refuses that name - never a swap.
//! Provenance: our own store over the machine and post laws; no
//! HSMWorks material.

use std::cell::RefCell;
use std::collections::BTreeMap;

use crate::machine::Machine;
use crate::post::engine;

/// Maximum combined bytes of file names and texts retained by this session.
pub const SESSION_TEXT_LIMIT: usize = 32 << 20;

struct Store {
    limit: usize,
    retained: usize,
    machines: BTreeMap<String, String>,
    posts: BTreeMap<String, String>,
}

impl Default for Store {
    fn default() -> Self {
        Self {
            limit: SESSION_TEXT_LIMIT,
            retained: 0,
            machines: BTreeMap::new(),
            posts: BTreeMap::new(),
        }
    }
}

impl Store {
    fn insert(&mut self, name: &str, text: &str, post: bool) -> Result<(), String> {
        let entries = if post {
            &mut self.posts
        } else {
            &mut self.machines
        };
        let old = entries.get(name);
        if post && old.is_some_and(|old| old != text) {
            return Err(format!(
                "{name}: different post text already exists under this name; use a new file name"
            ));
        }
        let previous = old.map_or(0, |text| name.len().saturating_add(text.len()));
        let retained = self
            .retained
            .saturating_sub(previous)
            .checked_add(name.len())
            .and_then(|n| n.checked_add(text.len()))
            .filter(|n| *n <= self.limit)
            .ok_or_else(|| {
                format!("{name}: the session text limit is exceeded; start a new session")
            })?;
        entries.insert(name.to_owned(), text.to_owned());
        self.retained = retained;
        Ok(())
    }
}

thread_local! {
    static STORE: RefCell<Store> = RefCell::new(Store::default());
}

/// Run `f` on this thread's store; `None` only on a re-entrant call.
fn with<R>(f: impl FnOnce(&mut Store) -> R) -> Option<R> {
    STORE.with(|s| s.try_borrow_mut().ok().map(|mut s| f(&mut s)))
}

fn keep(name: &str, text: &str, post: bool) -> Result<(), String> {
    with(|s| s.insert(name, text, post))
        .ok_or_else(|| format!("{name}: the session file list is busy - load it again"))?
}

/// Validate a machine with the caller's checks, then retain its text.
/// The app supplies its holder-aware validator and parser.
pub fn add_machine(
    name: &str,
    text: &str,
    check: impl FnOnce(&str) -> Result<Machine, String>,
) -> Result<Machine, String> {
    check_size(name, text)?;
    let machine = check(text).map_err(|why| format!("{name}: {why}"))?;
    keep(name, text, false)?;
    Ok(machine)
}

/// Compile and run a post on an empty stream, then retain it. Different
/// text under an existing name is refused, so a selected post stays stable.
pub fn add_post(name: &str, text: &str) -> Result<(), String> {
    check_size(name, text)?;
    engine::run(text, &serde_json::Value::Null, "")
        .map_err(|e| format!("post script {name} does not run: {e}"))?;
    keep(name, text, true)
}

fn check_size(name: &str, text: &str) -> Result<(), String> {
    if name
        .len()
        .checked_add(text.len())
        .is_none_or(|n| n > SESSION_TEXT_LIMIT)
    {
        return Err(format!(
            "{name}: the session text limit is exceeded; start a new session"
        ));
    }
    Ok(())
}

/// The kept text of the machine file `name`, if this session loaded it.
#[must_use]
pub fn machine_text(name: &str) -> Option<String> {
    with(|s| s.machines.get(name).cloned()).flatten()
}

/// The kept text of the post script `name`, if this session loaded it.
#[must_use]
pub fn post_text(name: &str) -> Option<String> {
    with(|s| s.posts.get(name).cloned()).flatten()
}

/// The session's machine file names, sorted.
#[must_use]
pub fn machine_names() -> Vec<String> {
    with(|s| s.machines.keys().cloned().collect()).unwrap_or_default()
}

/// The session's post script names, sorted.
#[must_use]
pub fn post_names() -> Vec<String> {
    with(|s| s.posts.keys().cloned().collect()).unwrap_or_default()
}

/// A machine file's text: the session's copy of `path` when it has one,
/// else the file on disk.
pub fn read_machine(path: &std::path::Path) -> std::io::Result<String> {
    match path.to_str().and_then(machine_text) {
        Some(text) => Ok(text),
        None => std::fs::read_to_string(path),
    }
}

/// A post script's text: the session's copy of `path` when it has one,
/// else the file on disk.
pub fn read_post(path: &str) -> std::io::Result<String> {
    match post_text(path) {
        Some(text) => Ok(text),
        None => std::fs::read_to_string(path),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retained_text_budget_covers_both_lists_and_refuses_by_name() {
        let mut store = Store {
            limit: 20,
            ..Store::default()
        };
        store
            .insert("a.mch", "12345", false)
            .expect("first ten bytes");
        store
            .insert("b.cps", "12345", true)
            .expect("second ten bytes");
        assert!(
            store
                .insert("c.mch", "1", false)
                .expect_err("full")
                .contains("c.mch")
        );
        assert_eq!(store.retained, 20);
        assert!(!store.machines.contains_key("c.mch"));
        store
            .insert("a.mch", "1", false)
            .expect("replacement releases four bytes");
        assert_eq!(store.retained, 16);
        store
            .insert("b.cps", "12345", true)
            .expect("same text uses no extra bytes");
        assert_eq!(store.retained, 16);
    }

    #[test]
    fn a_bad_file_is_refused_by_name_and_never_kept() {
        assert!(
            add_machine("unit-2950-bad.mch", "[]", |_| Err("not a machine".into()))
                .expect_err("not a machine")
                .contains("unit-2950-bad.mch")
        );
        assert!(
            add_post("unit-2950-bad.cps", "throw 1;")
                .expect_err("does not run")
                .contains("unit-2950-bad.cps")
        );
        assert!(!machine_names().contains(&"unit-2950-bad.mch".to_owned()));
        assert!(!post_names().contains(&"unit-2950-bad.cps".to_owned()));
        assert!(read_post("unit-2950-bad.cps").is_err());
    }
}
