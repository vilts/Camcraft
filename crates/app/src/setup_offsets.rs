//! Work offsets as an index (#931, slice 2): a setup posts at a 1-based
//! offset number, and the setup's own post says how many offsets it can
//! express and what the control calls each one.
//!
//! - **The index is the law.** [`crate::setup::wcs_slot`] is the stored
//!   pick (0-based, unset = 0); [`index`] is the number the stream
//!   carries. An index the post cannot express is never read as offset 1:
//!   the post-time gate ([`range_problems`]) and the command
//!   (`SetSetupWorkOffset`) refuse it by setup name.
//! - **The names are display.** [`work_offset_names`] reads the post
//!   crate's table (`camcraft_post::post::work_offset::work_offsets`),
//!   cached per resolved post and script text, so no script runs per
//!   frame. Labels, the setup sheet and refusal texts read it; it never
//!   feeds the stream.
//!
//! Provenance: our own design (issue #931) over RS-274 / ISO 6983 work
//! offset selection; no vendor material consulted.

use std::collections::HashMap;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::sync::{Mutex, OnceLock};

use flecs_ecs::prelude::World;

use crate::post::work_offset::{self, WorkOffsets};
use crate::schema::PostRef;

/// The sanity bound core keeps on a stored pick: the height digest
/// (`operation::params`) stores the slot in a `u8`. Every other limit is
/// the setup's post's.
pub const MAX_SLOT: usize = 254;

/// The 1-based offset index a setup context posts at: its stored pick
/// plus one, `1` for an unset pick. Never clamped to what a post can
/// express - that is the gate's to refuse.
#[must_use]
pub fn index(world: &World, setup: Option<&str>) -> u32 {
    u32::try_from(crate::setup::wcs_slot(world, setup)).map_or(u32::MAX, |s| s.saturating_add(1))
}

/// The work-offset table of a setup context's own post
/// ([`crate::setup_mount::post_of`]): its maximum and the control's name
/// for each offset. `Err` names a post that cannot be read or loaded.
pub fn table(world: &World, setup: Option<&str>) -> Result<WorkOffsets, String> {
    let post = crate::setup_mount::post_of(world, setup);
    table_of(&post)
}

/// [`table`] for one post reference.
pub fn table_of(post: &PostRef) -> Result<WorkOffsets, String> {
    let (label, text) = crate::posting::script_of(post)?;
    table_for(&label, &text)
}

/// The table of a script text, cached by (label, text).
pub fn table_for(label: &str, text: &str) -> Result<WorkOffsets, String> {
    let mut h = DefaultHasher::new();
    label.hash(&mut h);
    text.hash(&mut h);
    let key = h.finish();
    let cache = CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    if let Some(hit) = cache.lock().unwrap_or_else(std::sync::PoisonError::into_inner).get(&key) {
        return hit.clone();
    }
    let read = work_offset::work_offsets(text).map_err(|e| format!("post {label}: {e}"));
    let mut map = cache.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    // A bounded memo: a session edits few post files, but each edit is
    // a new key - never let the map grow without end.
    if map.len() >= CACHE_CAP {
        map.clear();
    }
    map.insert(key, read.clone());
    read
}

const CACHE_CAP: usize = 64;
static CACHE: OnceLock<Mutex<HashMap<u64, Result<WorkOffsets, String>>>> = OnceLock::new();

/// The legacy table (six offsets, `G54`..`G59`): what a setup context
/// SHOWS when its post cannot be read. Display only - the gate and the
/// command refuse an unreadable post's offsets above 6 by name.
fn legacy() -> WorkOffsets {
    WorkOffsets {
        indexed: false,
        max: work_offset::DEFAULT_MAX,
        names: work_offset::LEGACY_WORDS.iter().map(|w| (*w).to_owned()).collect(),
    }
}

/// The setup context's post's work offsets, for display: the names of
/// offsets `1..=max` (an empty name is one the post does not name).
#[must_use]
pub fn work_offset_names(world: &World, setup: Option<&str>) -> WorkOffsets {
    table(world, setup).unwrap_or_else(|_| legacy())
}

/// What the setup context's post calls offset `n` (1-based): its name,
/// else "work offset n" - never another offset's name.
#[must_use]
pub fn offset_name(world: &World, setup: Option<&str>, n: u32) -> String {
    name_in(&work_offset_names(world, setup), n)
}

/// [`offset_name`] within a table already read.
#[must_use]
pub fn name_in(t: &WorkOffsets, n: u32) -> String {
    let named = usize::try_from(n)
        .ok()
        .and_then(|n| n.checked_sub(1))
        .and_then(|i| t.names.get(i))
        .filter(|s| !s.is_empty());
    match named {
        Some(s) => s.clone(),
        None => format!("work offset {n}"),
    }
}

/// How the UI and MCP word offset `n` (1-based) within a table: "Work
/// offset 2 (G55)", or "Work offset 2" when the post gives it no name
/// (#931 triage wording note).
#[must_use]
pub fn entry_label(t: &WorkOffsets, n: u32) -> String {
    let named = usize::try_from(n)
        .ok()
        .and_then(|n| n.checked_sub(1))
        .and_then(|i| t.names.get(i))
        .filter(|s| !s.is_empty());
    match named {
        Some(s) => format!("Work offset {n} ({s})"),
        None => format!("Work offset {n}"),
    }
}

/// The 1-based index a caller's offset names: a whole number
/// `1..=MAX_SLOT + 1`, or one of the legacy words `G54`..`G59` (aliases
/// of 1-6). `Err` says what is accepted. Whether the setup's post can
/// express it is [`check`]'s (the command's) to say.
pub fn parse_index(v: &serde_json::Value) -> Result<u32, String> {
    let top = u32::try_from(MAX_SLOT).map_or(u32::MAX, |m| m.saturating_add(1));
    let bad = || {
        format!("work offset must be a whole number 1 to {top}, or one of the words G54-G59 (offsets 1-6), got {v}")
    };
    match v {
        serde_json::Value::Number(n) => n
            .as_u64()
            .and_then(|n| u32::try_from(n).ok())
            .filter(|n| (1..=top).contains(n))
            .ok_or_else(bad),
        serde_json::Value::String(s) => legacy_slot(s)
            .and_then(|i| u32::try_from(i).ok())
            .map(|i| i.saturating_add(1))
            .ok_or_else(bad),
        _ => Err(bad()),
    }
}

/// The name of the offset a setup context posts at ([`index`]).
#[must_use]
pub fn wcs_name(world: &World, setup: Option<&str>) -> String {
    offset_name(world, setup, index(world, setup))
}

/// The pick a legacy word names (`G54` = 0 .. `G59` = 5), for callers
/// that still speak words (MCP, slice 3 widens it).
#[must_use]
pub fn legacy_slot(word: &str) -> Option<usize> {
    work_offset::LEGACY_WORDS.iter().position(|w| w.eq_ignore_ascii_case(word.trim()))
}

/// Can the setup's own post express offset `n`? `Err` is the refusal,
/// naming the setup and the post: an offset above the post's maximum,
/// or above 6 on a post that cannot be read.
pub fn check(world: &World, setup: &str, n: u32) -> Result<(), String> {
    let post = crate::setup_mount::post_of(world, Some(setup));
    let max = match table_of(&post) {
        Ok(t) => t.max,
        Err(why) if n > work_offset::DEFAULT_MAX => {
            return Err(format!(
                "setup {setup} uses work offset {n}, but its post cannot be read to confirm it has that many ({why})"
            ));
        }
        Err(_) => return Ok(()),
    };
    if n == 0 || n > max {
        let label = crate::posting::post_ref_label(&post);
        return Err(format!(
            "setup {setup} uses work offset {n}, but its post {label} expresses work offsets 1-{max} only - \
             pick one of those on the Setups page or use a post with more offsets"
        ));
    }
    Ok(())
}

/// The post-time gate (#931): every setup with an operation in the
/// program, judged against its own post. An index the post cannot
/// express blocks the post by setup name - never posted at offset 1.
#[must_use]
pub fn range_problems(world: &World) -> Vec<String> {
    let mut seen: Vec<String> = Vec::new();
    let mut out = Vec::new();
    for (_, e) in crate::setup::operation_order(world) {
        let Some(op) = crate::model::operation_of(world, &e) else {
            continue;
        };
        if op.suppressed {
            continue;
        }
        let Some(name) = crate::setup::effective(world, op.setup.as_deref()) else {
            continue; // no setup: the unset pick, offset 1
        };
        if seen.contains(&name) {
            continue;
        }
        seen.push(name.clone());
        if let Err(why) = check(world, &name, index(world, Some(&name))) {
            out.push(why);
        }
    }
    out
}
